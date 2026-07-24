//! readingbuddy TUI — the ratatui frontend.
//!
//! All engine work goes through `readingbuddy::Engine`; this crate owns every
//! byte that reaches the terminal.

mod app;
mod config;
mod event;
mod render3d;
mod theme;
mod ui;

use std::io::{Stdout, Write, stdout};
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use clap::Parser;
use crossterm::event::{
    KeyboardEnhancementFlags, PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
    supports_keyboard_enhancement,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use readingbuddy::{Book, BookSort, Engine, EngineConfig};

use render3d::{RenderParams, Scene};

#[derive(Parser)]
#[command(
    name = "readingbuddy-tui",
    version,
    about = "Reading companion, on screen: browse the library and view a book in 3D"
)]
struct Cli {
    /// Data directory root (default: READINGBUDDY_DATA_DIR env or current dir)
    #[arg(long)]
    data_dir: Option<PathBuf>,

    /// Open straight into a book (id, ISBN, or title fragment)
    #[arg(long)]
    book: Option<String>,

    /// Render one frame of the object to stdout as WxH terminal cells and exit
    #[arg(long, value_name = "WxH")]
    dump_frame: Option<String>,

    /// Yaw,pitch in radians for --dump-frame (default: the standard pose)
    #[arg(long, value_name = "YAW,PITCH", requires = "dump_frame")]
    pose: Option<String>,

    /// Also write a PNG of the quantized cells (what the terminal would show)
    #[arg(long, value_name = "PATH", requires = "dump_frame")]
    dump_png: Option<PathBuf>,

    /// Block-glyph family: octant (default, 2x4, needs a Unicode-16 font) or
    /// quadrant (2x2, works on any truecolor terminal)
    #[arg(long, value_enum, default_value_t = GlyphArg::Octant)]
    glyphs: GlyphArg,
}

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
enum GlyphArg {
    Octant,
    Quadrant,
}

impl From<GlyphArg> for render3d::GlyphSet {
    fn from(g: GlyphArg) -> Self {
        match g {
            GlyphArg::Octant => render3d::GlyphSet::Octant,
            GlyphArg::Quadrant => render3d::GlyphSet::Quadrant,
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    let data_root = cli
        .data_dir
        .clone()
        .or_else(|| std::env::var_os("READINGBUDDY_DATA_DIR").map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("."));
    let engine = Engine::open(EngineConfig::rooted_at(data_root)).await?;

    if let Some(spec) = &cli.dump_frame {
        return dump_frame(&engine, spec, &cli).await;
    }

    // Apply the persisted accent before the first draw. A bad/missing file is a
    // warning, never fatal — it must not brick the TUI.
    match config::load() {
        Ok(cfg) => {
            if let Some(rgb) = cfg.accent.as_deref().and_then(theme::parse_hex) {
                theme::set_accent(rgb);
            }
        }
        Err(e) => eprintln!("warning: {e:#}"),
    }

    let mut app = app::App::new(engine).await?;
    app.params.glyphs = cli.glyphs.into();
    if let Some(selector) = &cli.book {
        let book = resolve_book(&app.engine, selector).await?;
        app.open_book(book).await?;
    }

    let mut terminal = setup_terminal()?;
    let result = app::run(&mut terminal, &mut app).await;
    restore_terminal();
    result
}

/// First match for a selector, or a clear error listing the ambiguity.
async fn resolve_book(engine: &Engine, selector: &str) -> Result<Book> {
    let mut found = engine.resolve_books(selector).await?;
    match found.len() {
        0 => bail!("no book matched '{selector}'"),
        _ => Ok(found.remove(0)),
    }
}

/// `--dump-frame`: render the object once and write it to stdout with ANSI
/// truecolor. No alternate screen, no raw mode — it works in a pipe.
async fn dump_frame(engine: &Engine, spec: &str, cli: &Cli) -> Result<()> {
    let (w, h) = parse_size(spec)?;
    let book = match cli.book.as_deref() {
        Some(s) => resolve_book(engine, s).await?,
        None => engine
            .storage
            .list_books(1, BookSort::LastModified)
            .await?
            .into_iter()
            .next()
            .unwrap_or_else(|| Book {
                title: Some("Untitled".into()),
                ..Book::default()
            }),
    };

    let mut params = RenderParams {
        glyphs: cli.glyphs.into(),
        ..RenderParams::default()
    };
    if let Some(spec) = cli.pose.as_deref() {
        let (yaw, pitch) = spec
            .split_once(',')
            .context("--pose expects YAW,PITCH in radians")?;
        params.pose.yaw = yaw.trim().parse().context("bad yaw")?;
        params.pose.pitch = pitch.trim().parse().context("bad pitch")?;
    }

    let mut scene = Scene::new(engine.config.images_dir.clone());
    let fb = scene.frame(&book, w, h, params);
    if let Some(path) = &cli.dump_png {
        render3d::blit::to_png(fb, params.glyphs, path)?;
        eprintln!("wrote {}", path.display());
    }
    let mut out = stdout().lock();
    writeln!(out, "{}", book.display_title())?;
    write!(out, "{}", render3d::blit::to_ansi(fb, params.glyphs))?;
    Ok(())
}

fn parse_size(spec: &str) -> Result<(u16, u16)> {
    let (w, h) = spec
        .split_once(['x', 'X'])
        .context("expected WxH, e.g. 100x30")?;
    Ok((w.trim().parse()?, h.trim().parse()?))
}

fn setup_terminal() -> Result<Terminal<CrosstermBackend<Stdout>>> {
    enable_raw_mode()?;
    execute!(stdout(), EnterAlternateScreen)?;
    // Ask for the Kitty keyboard protocol so modified keys like Shift+Enter
    // arrive as distinct events (plain terminals report them as bare Enter).
    // Purely a nicety — terminals that don't support it are left untouched.
    if supports_keyboard_enhancement().unwrap_or(false) {
        let _ = execute!(
            stdout(),
            PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES)
        );
    }
    // A panic must never leave the pane in raw mode with no cursor.
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        restore_terminal();
        previous(info);
    }));
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;
    terminal.hide_cursor()?;
    terminal.clear()?;
    Ok(terminal)
}

fn restore_terminal() {
    // Pop the enhancement flags we may have pushed; terminals that never got a
    // push ignore the pop, so it's safe to send unconditionally.
    let _ = execute!(stdout(), PopKeyboardEnhancementFlags);
    let _ = disable_raw_mode();
    let _ = execute!(stdout(), LeaveAlternateScreen, crossterm::cursor::Show);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_frame_sizes() {
        assert_eq!(parse_size("100x30").unwrap(), (100, 30));
        assert_eq!(parse_size(" 40 X 20 ").unwrap(), (40, 20));
        assert!(parse_size("100").is_err());
        assert!(parse_size("axb").is_err());
    }
}
