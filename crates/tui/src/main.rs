//! readingbuddy TUI — the ratatui frontend.
//!
//! All engine work goes through `readingbuddy::Engine`; this crate owns every
//! byte that reaches the terminal.

mod app;
mod clipboard;
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

    /// Book-view renderer: auto (true pixels wherever the terminal supports
    /// them, block glyphs otherwise), or force one.
    #[arg(long, value_enum, default_value_t = RenderArg::Auto)]
    render: RenderArg,

    /// Probe the terminal, print what it can do and which renderer that picks,
    /// then exit. Needs a real tty — pipe it and it reports the safe floor.
    #[arg(long)]
    probe: bool,

    /// Push N rich frames at the terminal as fast as it will take them and
    /// report the achieved rate. Separates our cost from the terminal's: run it
    /// in a real pane, since it measures the pty and the compositor too.
    #[arg(long, value_name = "N")]
    bench_rich: Option<u32>,
}

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
enum RenderArg {
    Auto,
    Glyph,
    Rich,
}

impl RenderArg {
    /// Resolve to a concrete mode against what the terminal actually reported.
    ///
    /// **`Auto` picks glyph even where pixels are available**, deliberately. The
    /// pixel path is correct and looks good, but an animated one costs the
    /// terminal far more than the block-glyph path and undercuts the whole
    /// point of a lightweight tmux-native TUI (see `docs/rich-renderer.md` —
    /// "Why auto is glyph"). Until the spin stops sending images, rich is
    /// opt-in: `--render rich`, or `v` in the book view.
    ///
    /// `Rich` forced on a terminal that can't take pixels still degrades to
    /// glyph rather than spraying escapes at it — the flag is a preference, not
    /// a promise the terminal has to keep.
    fn resolve(self, caps: render3d::Caps) -> render3d::RenderMode {
        use render3d::RenderMode;
        match self {
            RenderArg::Rich if caps.supports_pixels() => RenderMode::Rich,
            _ => RenderMode::Glyph,
        }
    }
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

    // Before anything opens the engine: the probe is pure terminal I/O and must
    // not create a `database/` directory as a side effect of a diagnostic.
    if cli.probe {
        return report_caps(&cli);
    }

    let data_root = cli
        .data_dir
        .clone()
        .or_else(|| std::env::var_os("READINGBUDDY_DATA_DIR").map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("."));
    let mut engine_config = EngineConfig::rooted_at(data_root);
    // Env wins (matching the CLI's precedence); otherwise fall back to the
    // shared config file, so a key set in either frontend is picked up here.
    if engine_config.google_api_key.is_none() {
        engine_config.google_api_key = config::load_google_key();
    }
    let engine = Engine::open(engine_config).await?;

    if let Some(spec) = &cli.dump_frame {
        return dump_frame(&engine, spec, &cli).await;
    }

    if let Some(frames) = cli.bench_rich {
        return bench_rich(&engine, frames, &cli).await;
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

    // The probe has to run with raw mode on and before ratatui reads any input,
    // so the mode can only be settled once the terminal is up.
    let (mut terminal, caps) = setup_terminal()?;
    app.set_caps(caps);
    app.set_render_mode(cli.render.resolve(caps));
    let result = app::run(&mut terminal, &mut app).await;
    restore_terminal();
    result
}

/// `--probe`: run the capability probe and report it in plain text.
///
/// Raw mode is required for the replies to arrive unechoed and unbuffered, so
/// this puts the terminal in raw mode for the length of the probe and takes it
/// straight back out. No alternate screen — the output is meant to be read.
fn report_caps(cli: &Cli) -> Result<()> {
    let raw_mode = enable_raw_mode().is_ok();
    let (caps, raw) = render3d::caps::probe_verbose();
    if raw_mode {
        let _ = disable_raw_mode();
    }
    // Give tmux its pane option back before we print anything.
    render3d::caps::restore_passthrough();

    let mode = cli.render.resolve(caps);
    let (w, h) = caps.cell_px;
    println!("kitty graphics : {}", caps.kitty_graphics);
    println!(
        "cell size      : {w}x{h} px ({})",
        if caps.cell_px_measured {
            "reported"
        } else {
            "assumed"
        }
    );
    println!("tmux           : {}", caps.in_tmux);
    println!("passthrough    : {:?}", caps.passthrough);
    println!("pixels usable  : {}", caps.supports_pixels());
    println!("renderer       : {mode:?} (--render {:?})", cli.render);
    // The bytes the terminal actually sent. When detection surprises you this
    // is the only evidence that settles it, so print it rather than describe it.
    println!(
        "raw reply      : {}",
        String::from_utf8_lossy(&raw).escape_debug()
    );
    Ok(())
}

/// `--bench-rich N`: the decisive measurement for "why is the spin slow".
///
/// Traces, encodes and *actually writes* N frames at the terminal, timing each
/// stage separately. Our own cost (trace + encode) is known to be small; if the
/// write time dwarfs it, the pty and the compositor are the bottleneck and the
/// fix has to be fewer or smaller frames, not faster code.
async fn bench_rich(engine: &Engine, frames: u32, cli: &Cli) -> Result<()> {
    use std::time::Instant;

    let book = match &cli.book {
        Some(sel) => resolve_book(engine, sel).await?,
        None => engine
            .storage
            .list_books(1, BookSort::LastModified)
            .await?
            .into_iter()
            .next()
            .context("no books in the library to bench with")?,
    };

    let caps = render3d::caps::probe();
    render3d::caps::restore_passthrough();
    let (cols, rows) = cli
        .dump_frame
        .as_deref()
        .map(parse_size)
        .transpose()?
        .unwrap_or((50, 26));

    let mut scene = Scene::new(engine.config.images_dir.clone());
    let mut params = RenderParams::default();
    let mut out = stdout();
    let id = render3d::kitty::image_id();

    let (mut trace_ns, mut encode_ns, mut write_ns, mut bytes) = (0u128, 0u128, 0u128, 0usize);
    let wall = Instant::now();
    for i in 0..frames {
        // Advance like the real spin does, so every frame is a cache miss.
        params.pose.yaw += 0.01164;
        let target = render3d::raster::target_for(
            cols,
            rows,
            caps.cell_px,
            render3d::raster::Quality::Motion,
        );

        let t0 = Instant::now();
        let img = {
            let cover = scene.cover(&book, target.width.clamp(24, 2048));
            let model = render3d::Model::new(&book, cover);
            render3d::raster::render_rgba(target, &model, cover, params)
        };
        trace_ns += t0.elapsed().as_nanos();

        let t1 = Instant::now();
        let esc = render3d::kitty::transmit(&img, id, cols, rows, caps.in_tmux);
        encode_ns += t1.elapsed().as_nanos();

        let t2 = Instant::now();
        out.write_all(esc.as_bytes())?;
        out.flush()?;
        write_ns += t2.elapsed().as_nanos();
        bytes += esc.len();

        if i == 0 {
            eprintln!(
                "frame is {}x{} px, {} KB",
                target.width,
                target.height,
                esc.len() / 1024
            );
        }
    }
    let total = wall.elapsed();
    // Leave nothing on screen.
    let _ = out.write_all(render3d::kitty::delete(id, caps.in_tmux).as_bytes());
    let _ = out.flush();

    let ms = |ns: u128| ns as f64 / 1.0e6 / frames as f64;
    eprintln!("book        : {}", book.display_title());
    eprintln!("in tmux     : {}", caps.in_tmux);
    eprintln!("cell px     : {:?}", caps.cell_px);
    eprintln!("trace       : {:.2} ms/frame", ms(trace_ns));
    eprintln!("encode      : {:.2} ms/frame", ms(encode_ns));
    eprintln!("write+flush : {:.2} ms/frame", ms(write_ns));
    eprintln!(
        "achieved    : {:.1} fps ({:.1} MB/s)",
        frames as f64 / total.as_secs_f64(),
        bytes as f64 / total.as_secs_f64() / 1.0e6
    );
    Ok(())
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

fn setup_terminal() -> Result<(Terminal<CrosstermBackend<Stdout>>, render3d::Caps)> {
    enable_raw_mode()?;
    execute!(stdout(), EnterAlternateScreen)?;
    // Probe here and nowhere else: raw mode is on (so replies aren't echoed or
    // line-buffered), we're on the alternate screen (so nothing lands in the
    // user's scrollback), and ratatui's event reader has not started — which is
    // what guarantees the reply bytes can't leak into the later `EventStream`.
    let caps = render3d::caps::probe();
    // Ask for the Kitty keyboard protocol so modified keys like Shift+Enter
    // arrive as distinct events (plain terminals report them as bare Enter).
    // Purely a nicety — terminals that don't support it are left untouched.
    if supports_keyboard_enhancement().unwrap_or(false) {
        let _ = execute!(
            stdout(),
            PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES)
        );
    }
    // A panic must never leave the pane in raw mode with no cursor — nor with
    // an orphaned image on screen or tmux left in a state we imposed.
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        restore_terminal();
        previous(info);
    }));
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;
    terminal.hide_cursor()?;
    terminal.clear()?;
    Ok((terminal, caps))
}

fn restore_terminal() {
    // First, while the alternate screen is still current: the image placement
    // lives on *that* screen, so it has to be deleted before we leave, or it
    // ghosts over whatever the user had underneath.
    render3d::kitty::teardown();
    // Pop the enhancement flags we may have pushed; terminals that never got a
    // push ignore the pop, so it's safe to send unconditionally.
    let _ = execute!(stdout(), PopKeyboardEnhancementFlags);
    let _ = disable_raw_mode();
    let _ = execute!(stdout(), LeaveAlternateScreen, crossterm::cursor::Show);
    // Hand tmux's pane option back last, once nothing else needs to get out.
    render3d::caps::restore_passthrough();
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
