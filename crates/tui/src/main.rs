//! readingbuddy TUI — the ratatui frontend.
//!
//! All engine work goes through `readingbuddy::Engine`; this crate owns every
//! byte that reaches the terminal.

mod ambient;
mod app;
mod clipboard;
mod config;
mod event;
mod logging;
mod perf;
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

    /// Book-view renderer. `rich` is the hybrid — glyphs while the book turns,
    /// pixels when it parks — which is also what `auto` picks wherever the
    /// terminal can take pixels.
    #[arg(long, value_enum, default_value_t = RenderArg::Auto)]
    render: RenderArg,

    /// Probe the terminal, print what it can do and which renderer that picks,
    /// then exit. Needs a real tty — pipe it and it reports the safe floor.
    #[arg(long)]
    probe: bool,

    /// Panic on purpose, to verify the crash hook restores the terminal, writes
    /// a report, and prints where it went. Hidden: a test affordance, not UI.
    #[arg(long, hide = true)]
    panic_now: bool,

    /// Push N rich frames at the terminal as fast as it will take them and
    /// report the achieved rate. Separates our cost from the terminal's: run it
    /// in a real pane, since it measures the pty and the compositor too.
    #[arg(long, value_name = "N")]
    bench_rich: Option<u32>,

    /// Run a scripted spin/park/resize workload through the real presenter and
    /// report what it cost — including how long the terminal itself took to
    /// answer while under load. Run it in a real, *active* pane.
    #[arg(long, value_enum, value_name = "MODE")]
    bench_render: Option<BenchArg>,

    /// Repeat the --bench-render script N times (default 1). The rect the
    /// object gets is whatever the real pane gives it, and is reported — it
    /// cannot be forced without lying about what is being measured.
    #[arg(long, value_name = "N", requires = "bench_render")]
    bench_reps: Option<u32>,

    /// Draw as fast as the machine allows instead of pacing to the app's 20fps
    /// tick. Answers "how much throughput is there", *not* "what does the app
    /// cost" — the retransmit throttle keys off pose, so free-running inflates
    /// the send rate far above anything a real session produces.
    #[arg(long, requires = "bench_render")]
    bench_free_run: bool,

    /// Name the environment this run was measured in, recorded in the history
    /// TSV (default `live`). `scripts/bench-sandbox.sh` passes `box`.
    ///
    /// Rows from different environments are *not* comparable — a disposable
    /// kitty window with a pinned font and a private tmux server has a
    /// different cell size, window size and neighbour set from the pane you
    /// work in. This column is what stops the trend line from silently mixing
    /// them, the same failure `cover_px` was added to prevent.
    #[arg(long, value_name = "NAME", requires = "bench_render")]
    bench_env: Option<String>,

    /// Append a JSON record per frame to this file. Off by default; the
    /// recording calls cost one relaxed atomic load when it is absent.
    #[arg(long, value_name = "PATH", env = "READINGBUDDY_PERF_LOG")]
    perf_log: Option<PathBuf>,

    /// Append one summary row per benched mode to this TSV, so runs can be
    /// compared over time. The per-frame log is the detail for one run; this is
    /// the trend line across many.
    #[arg(long, value_name = "PATH", env = "READINGBUDDY_PERF_HISTORY")]
    perf_history: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
enum BenchArg {
    Glyph,
    Rich,
    /// The known-bad configuration — pixels even while moving. Kept so the
    /// latency numbers have something pathological to be calibrated against.
    RichAlways,
    /// All three, one after another, on identical work.
    All,
}

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
enum RenderArg {
    /// Pixels wherever the terminal can take them, block glyphs otherwise.
    Auto,
    /// Block glyphs always. Works anywhere truecolor does.
    Glyph,
    /// The hybrid: block glyphs while the book turns, true pixels the moment it
    /// parks. Falls back to glyphs on a terminal with no pixel protocol.
    Rich,
    /// Pixels even while the book is moving. **Known bad** — kept as a
    /// diagnostic so the terminal-latency measurement has something
    /// pathological to be calibrated against, not as a usable setting.
    RichAlways,
}

impl RenderArg {
    /// Resolve to a concrete mode against what the terminal actually reported.
    ///
    /// `Auto` takes pixels wherever the terminal can hold them. That is a
    /// reversal: it used to pick glyphs unconditionally, because an animated
    /// pixel book cost the terminal far more than block glyphs and undercut the
    /// point of a lightweight tmux-native TUI. What changed is that `Rich` no
    /// longer animates in pixels — it is the hybrid, glyphs in motion and pixels
    /// at rest — so the spin now costs exactly what it always did, and the one
    /// thing standing against `auto` is gone.
    ///
    /// A mode forced on a terminal that can't take pixels still degrades to
    /// glyph rather than spraying escapes at it — the flag is a preference, not
    /// a promise the terminal has to keep.
    fn resolve(self, caps: render3d::Caps) -> render3d::RenderMode {
        use render3d::RenderMode;
        match self {
            RenderArg::Glyph => RenderMode::Glyph,
            RenderArg::RichAlways if caps.supports_pixels() => RenderMode::RichAlways,
            _ if caps.supports_pixels() => RenderMode::Rich,
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
    // Held for the whole run: dropping the guard flushes, and dropping it early
    // truncates the tail of the log — the part that matters after a crash.
    // Installed BEFORE setup_terminal, and that ordering is the whole trick:
    // `set_hook` chains via `take_hook`, so the LAST hook installed runs FIRST.
    // setup_terminal installs the terminal-restoring hook afterwards, giving
    // TUI hook -> restore_terminal() -> this -> write file -> std default.
    // The pane is therefore already sane before anything prints.
    readingbuddy::crash::install_hook(readingbuddy::CrashContext {
        app: "readingbuddy-tui",
        version: env!("CARGO_PKG_VERSION"),
        log_dir: engine_config.log_dir.clone(),
        log_file: Some(engine_config.log_dir.join("tui.log")),
    });

    let log = logging::init(&engine_config.log_dir, None);
    if let Some(h) = &log {
        tracing::info!(version = env!("CARGO_PKG_VERSION"), log = %h.path.display(), "readingbuddy-tui starting");
    }
    let engine = Engine::open(engine_config).await?;

    // Deliberate crash, for verifying the hook end-to-end (terminal restored,
    // report written, path echoed). Hidden: it is a test affordance, not UI.
    if cli.panic_now {
        panic!("--panic-now: deliberate crash to exercise the crash hook");
    }

    if let Some(spec) = &cli.dump_frame {
        return dump_frame(&engine, spec, &cli).await;
    }

    if let Some(frames) = cli.bench_rich {
        return bench_rich(&engine, frames, &cli).await;
    }

    if let Some(which) = cli.bench_render {
        return bench_render(engine, which, &cli).await;
    }

    // Apply the persisted settings before the first draw. A bad/missing file is
    // a warning, never fatal — it must not brick the TUI.
    let mut motif = ambient::Motif::default();
    match config::load() {
        Ok(cfg) => {
            if let Some(rgb) = cfg.accent.as_deref().and_then(theme::parse_hex) {
                theme::set_accent(rgb);
            }
            // An unrecognised motif is simply "off", not an error: the layer is
            // decoration, and a hand-edited typo should not stop the app.
            if let Some(m) = cfg.ambient.as_deref().and_then(ambient::Motif::from_label) {
                motif = m;
            }
        }
        Err(e) => eprintln!("warning: {e:#}"),
    }

    let mut app = app::App::new(engine).await?;
    app.ambient = ambient::Ambient::new(motif);
    app.params.glyphs = cli.glyphs.into();
    if let Some(selector) = &cli.book {
        let book = resolve_book(&app.engine, selector).await?;
        app.open_book(book).await?;
    }

    // One meter for both classes of byte: it is handed to the ratatui backend
    // (text) and to the graphics writer (image), so the two are counted apart.
    let meter = perf::Meter::new();
    if let Some(path) = &cli.perf_log {
        match perf::Recorder::open(path, meter.clone()) {
            Ok(rec) => app.perf = rec,
            // A diagnostic that refuses to start is not a reason to refuse to
            // run: warn before the alternate screen swallows it, and carry on.
            Err(e) => eprintln!("warning: perf log {}: {e}", path.display()),
        }
    }

    // The probe has to run with raw mode on and before ratatui reads any input,
    // so the mode can only be settled once the terminal is up.
    let (mut terminal, caps) = setup_terminal(meter.clone())?;
    app.set_caps(caps);
    app.set_render_mode(cli.render.resolve(caps));
    app.rich.count_bytes(meter);
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

/// One phase of the bench script.
#[derive(Debug, Clone, Copy)]
enum Phase {
    /// The book turns, one draw per simulated tick.
    Spin(u32),
    /// The book is parked and redrawn anyway, which is what a settle costs.
    Park(u32),
    /// Slide the divider, changing the object's rect exactly as `[`/`]` do.
    Slide(f32),
}

/// The workload every mode is measured on. Fixed, so two runs are comparable
/// and so no mode gets an easier ride than another.
const SCRIPT: &[Phase] = &[
    Phase::Spin(100),
    Phase::Park(20),
    Phase::Spin(60),
    Phase::Slide(0.3),
    Phase::Spin(40),
    Phase::Park(20),
];

/// Sample the terminal's own latency this often (in frames).
///
/// 8 gives ~30 samples over the script, which is the floor for a p90 that means
/// anything — at 12 samples the "p90" is just the second-worst value. Each probe
/// returns as soon as both replies land (a few ms), so it fits inside a tick
/// without becoming the load itself.
const RTT_EVERY: u32 = 8;

/// Frames drawn and thrown away before each mode's measured run.
const WARMUP: u32 = 25;

/// How long to wait for the terminal to answer a latency probe.
///
/// Deliberately **shorter than a tick**. A probe that outlives the frame budget
/// has stopped measuring the workload and started changing it: at a 250ms
/// deadline, a run where the terminal never answered stalled 30 times per mode
/// and dragged a paced 20fps bench down to 13fps — the numbers then described a
/// workload that does not exist.
const RTT_DEADLINE: std::time::Duration = std::time::Duration::from_millis(40);

/// Consecutive silent probes before sampling gives up for the rest of the run.
const RTT_GIVE_UP: u32 = 2;

/// What one mode's run cost.
struct BenchSummary {
    mode: &'static str,
    frames: u32,
    wall: std::time::Duration,
    bytes_text: u64,
    bytes_image: u64,
    transmits: u32,
    rect: (u16, u16),
    trace_us: u64,
    draw_us: u64,
    /// Image bytes emitted on frames where the book was moving. **The number
    /// this whole exercise is about**: for the hybrid it must be zero.
    moving_image_bytes: u64,
    rtt_kitty: Vec<u64>,
    rtt_tmux: Vec<u64>,
}

/// `--bench-render`: what a mode actually costs, terminal included.
///
/// The difference from [`bench_rich`] is that this drives the **real presenter
/// through a real `Terminal::draw`**, so the retransmit throttle, the
/// placeholder cells and ratatui's diff are all in the measurement — and so the
/// glyph path can be measured at all, which nothing could do before.
///
/// It also samples `caps::rtt_probe` under load. That is the honest number: our
/// own timings all looked healthy while the first rich renderer was making
/// terminals unusable, because the cost is paid inside the terminal. A terminal
/// that is drowning answers slowly, so ask it.
///
/// Run it in a real, **active** pane: tmux routes input to the focused pane
/// only, so a background pane sees no replies and the RTT column comes back
/// empty.
async fn bench_render(engine: Engine, which: BenchArg, cli: &Cli) -> Result<()> {
    // Fail with a reason rather than the raw `Device not configured` that
    // enabling raw mode on a pipe produces. Piping this measures nothing anyway:
    // the point is the terminal's own behaviour.
    if !std::io::IsTerminal::is_terminal(&stdout()) {
        bail!("--bench-render needs a real terminal — it measures the terminal, not us");
    }
    let reps = cli.bench_reps.unwrap_or(1).max(1);
    let modes: &[render3d::RenderMode] = match which {
        BenchArg::Glyph => &[render3d::RenderMode::Glyph],
        BenchArg::Rich => &[render3d::RenderMode::Rich],
        BenchArg::RichAlways => &[render3d::RenderMode::RichAlways],
        BenchArg::All => &[
            render3d::RenderMode::Glyph,
            render3d::RenderMode::Rich,
            render3d::RenderMode::RichAlways,
        ],
    };

    // A procedural plate compresses several times better than a photograph, so
    // benching without a real cover produces numbers that are wrong in the
    // flattering direction. Pick a book that has one rather than whichever was
    // touched last — an explicit `--book` still wins, and the warning still
    // fires if nothing in the library qualifies.
    // Ask the renderer's own resolver, not a second guess at the same rule —
    // and rank by the decoded width, because a 180px OpenLibrary thumbnail
    // upscaled into a 358px render is smooth, compresses far better than a real
    // photograph, and flatters the image numbers almost as much as the
    // procedural plate does.
    let cover_width = |b: &Book| -> u32 {
        b.cover_path
            .as_deref()
            .and_then(|p| render3d::resolve_cover(&engine.config.images_dir, p))
            .and_then(|p| image::image_dimensions(p).ok())
            .map(|(w, _)| w)
            .unwrap_or(0)
    };
    let book = match &cli.book {
        Some(sel) => resolve_book(&engine, sel).await?,
        None => {
            let library = engine
                .storage
                .list_books(200, BookSort::LastModified)
                .await?;
            library
                .iter()
                .max_by_key(|b| cover_width(b))
                .cloned()
                .context("no books in the library to bench with")?
        }
    };
    let cover_px = cover_width(&book);

    let meter = perf::Meter::new();
    let mut app = app::App::new(engine).await?;
    app.params.glyphs = cli.glyphs.into();
    app.open_book(book.clone()).await?;
    if let Some(path) = &cli.perf_log {
        match perf::Recorder::open(path, meter.clone()) {
            Ok(rec) => app.perf = rec,
            Err(e) => eprintln!("warning: perf log {}: {e}", path.display()),
        }
    }
    app.perf.enable_in_memory();

    let (mut terminal, caps) = setup_terminal(meter.clone())?;
    app.set_caps(caps);
    app.rich.count_bytes(meter);

    // Reps outer, modes inner: whichever mode runs first absorbs whatever the
    // machine was still doing (a release build's IO tail, a cold cache), and a
    // transient that lands entirely inside one mode's block reads as that mode
    // being slow. Interleaving spreads it, so `--bench-reps 3` is the answer to
    // a result that looks surprising.
    let mut summaries = Vec::new();
    let mut rtt_silent = 0u32;
    for _ in 0..reps {
        for &mode in modes {
            app.set_render_mode(mode);
            app.rich.drop_image();
            app.layout.divider_bias = 0.0;
            app.params.pose = render3d::Pose::default();
            let _ = app.perf.drain_frames();

            // Pace to the app's own tick unless asked not to. Free-running
            // measures throughput, which is the wrong question: the retransmit
            // throttle keys off *pose*, so at 1000fps consecutive frames land in
            // different coarse buckets far more often and the send rate inflates
            // by ~50x over anything a real session produces. Same interval and
            // the same missed-tick behaviour as `app::run`, so a bench frame is
            // a real frame.
            let mut ticker = tokio::time::interval(app::TICK);
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

            // Warm-up, discarded. The first frames of a mode pay for a cold
            // cover decode, a cold frame cache and whatever the OS was still
            // finishing. Measured: the first 20 frames ran 40% slower than the
            // rest of the same run, which is enough to swamp the difference the
            // bench exists to show.
            // Spinning, so the warm-up exercises the motion path — and
            // deterministic, so every mode starts the script from the same pose.
            app.spinning = true;
            for _ in 0..WARMUP {
                if !cli.bench_free_run {
                    ticker.tick().await;
                }
                app.tick();
                app::redraw(&mut terminal, &mut app)?;
            }
            let _ = app.perf.drain_frames();

            let wall = std::time::Instant::now();
            let mut frame = 0u32;
            for phase in SCRIPT {
                match *phase {
                    Phase::Slide(delta) => {
                        app.layout.divider_bias =
                            (app.layout.divider_bias + delta).clamp(-1.0, 1.0);
                    }
                    Phase::Spin(n) | Phase::Park(n) => {
                        app.spinning = matches!(phase, Phase::Spin(_));
                        for _ in 0..n {
                            if !cli.bench_free_run {
                                ticker.tick().await;
                            }
                            app.tick();
                            app::redraw(&mut terminal, &mut app)?;
                            // Sample **immediately after handing the terminal a
                            // frame**, and before waiting out the rest of the
                            // tick. Probing before the draw instead put every
                            // sample in the idle gap, where the terminal has
                            // already caught up: paced runs then showed all
                            // three modes at an identical ~3.6 ms, which is the
                            // quiescent round-trip through tmux passthrough and
                            // measures nothing about load. The sample lands on
                            // the next frame's record — immaterial at this
                            // sampling rate, and honest about when it was taken.
                            if frame.is_multiple_of(RTT_EVERY) && rtt_silent < RTT_GIVE_UP {
                                let rtt = render3d::caps::rtt_probe(caps.in_tmux, RTT_DEADLINE);
                                // A terminal that is not answering will not
                                // start; keep waiting on it and the stalls
                                // become the workload.
                                rtt_silent = if rtt.kitty.is_none() {
                                    rtt_silent + 1
                                } else {
                                    0
                                };
                                app.rtt_sample = Some(rtt);
                            }
                            frame += 1;
                        }
                    }
                }
            }
            summaries.push(summarize(
                mode.label(),
                frame,
                wall.elapsed(),
                &mut app.perf,
            ));
        }
    }

    app.rich.drop_image();
    app.perf.flush();
    restore_terminal();
    report_bench(&summaries, cover_px, caps, cli.bench_free_run);
    if let Some(path) = &cli.perf_history {
        let env = cli.bench_env.as_deref().unwrap_or("live");
        match append_history(path, &summaries, cover_px, caps, env) {
            Ok(()) => eprintln!("history     : {}", path.display()),
            // The measurement already happened; failing to file it is not a
            // reason to fail the command.
            Err(e) => eprintln!("warning: perf history {}: {e}", path.display()),
        }
    }
    Ok(())
}

/// Append one row per mode to a TSV, creating it with a header.
///
/// Deliberately not JSON: this file is meant to be `sort`ed, `cut`, pasted into
/// a spreadsheet, or plotted, and it accumulates across runs — whereas the
/// per-frame JSONL is written fresh per run and describes a single session.
///
/// The timestamp is epoch seconds. No date formatting crate is worth adding for
/// one column, and epoch sorts correctly without one.
///
/// A file whose header does not match the current columns is **rotated aside**
/// rather than appended to. Appending under a stale header is silent corruption
/// of the one artefact whose entire purpose is being compared over time — it
/// already happened once, when `real_cover` (a bool) became `cover_px` (a width)
/// and three runs' rows landed under the old name.
const HISTORY_HEADER: &str = "epoch_s\tmode\tframes\tfps\ttext_MBps\timage_MBps\tmoving_KB\tsends\t\
     trace_us\tdraw_us\trtt_kitty_p50_us\trtt_kitty_p90_us\trtt_tmux_p50_us\t\
     rtt_tmux_p90_us\tcols\trows\tin_tmux\tcover_px\tenv\tload1";

/// The 1-minute load average, or 0 where the OS will not say.
///
/// Recorded per row because the latency columns are the ones that move under
/// contention, and after the fact there is no other way to tell a real
/// regression from "a release build was still linking". A row measured at load
/// 4 on an 8-core machine is evidence about the machine, not the renderer.
fn load1() -> f64 {
    #[cfg(unix)]
    {
        let mut avg = [0.0f64; 3];
        // SAFETY: getloadavg writes at most `nelem` doubles into the buffer.
        let n = unsafe { libc::getloadavg(avg.as_mut_ptr(), 1) };
        if n >= 1 { avg[0] } else { 0.0 }
    }
    #[cfg(not(unix))]
    {
        0.0
    }
}

fn append_history(
    path: &std::path::Path,
    summaries: &[BenchSummary],
    cover_px: u32,
    caps: render3d::Caps,
    env: &str,
) -> std::io::Result<()> {
    use std::io::Write as _;

    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir)?;
    }
    if path.exists() {
        let stale = std::fs::read_to_string(path)
            .map(|t| t.lines().next().map(str::to_owned))
            .ok()
            .flatten()
            .is_none_or(|h| h != HISTORY_HEADER);
        if stale {
            let aside = path.with_extension("tsv.old");
            std::fs::rename(path, &aside)?;
            eprintln!(
                "note: history columns changed; previous file kept at {}",
                aside.display()
            );
        }
    }
    let fresh = !path.exists();
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    if fresh {
        writeln!(f, "{HISTORY_HEADER}")?;
    }
    let epoch = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // Sampled once, after the run: the load the machine carried *through* the
    // measurement is what the reader wants, and the 1-minute average at the end
    // covers roughly that window for a run of this length.
    let load = load1();
    for s in summaries {
        let secs = s.wall.as_secs_f64().max(1e-9);
        writeln!(
            f,
            "{epoch}\t{}\t{}\t{:.1}\t{:.3}\t{:.3}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{:.2}",
            s.mode,
            s.frames,
            s.frames as f64 / secs,
            s.bytes_text as f64 / secs / 1.0e6,
            s.bytes_image as f64 / secs / 1.0e6,
            s.moving_image_bytes / 1024,
            s.transmits,
            s.trace_us,
            s.draw_us,
            perf::percentile(&s.rtt_kitty, 50.0),
            perf::percentile(&s.rtt_kitty, 90.0),
            perf::percentile(&s.rtt_tmux, 50.0),
            perf::percentile(&s.rtt_tmux, 90.0),
            s.rect.0,
            s.rect.1,
            caps.in_tmux,
            cover_px,
            env,
            load,
        )?;
    }
    Ok(())
}

fn summarize(
    mode: &'static str,
    frames: u32,
    wall: std::time::Duration,
    rec: &mut perf::Recorder,
) -> BenchSummary {
    let recs = rec.drain_frames();
    let n = recs.len().max(1) as u64;
    BenchSummary {
        mode,
        frames,
        wall,
        bytes_text: recs.iter().map(|r| r.bytes_text).sum(),
        bytes_image: recs.iter().map(|r| r.bytes_image).sum(),
        transmits: recs.iter().filter(|r| r.transmitted).count() as u32,
        rect: recs
            .iter()
            .map(|r| (r.cols, r.rows))
            .max()
            .unwrap_or((0, 0)),
        trace_us: recs.iter().map(|r| r.trace_us).sum::<u64>() / n,
        draw_us: recs.iter().map(|r| r.draw_us).sum::<u64>() / n,
        moving_image_bytes: recs
            .iter()
            .filter(|r| r.moving)
            .map(|r| r.bytes_image)
            .sum(),
        rtt_kitty: sorted(recs.iter().filter_map(|r| r.rtt_kitty_us)),
        rtt_tmux: sorted(recs.iter().filter_map(|r| r.rtt_tmux_us)),
    }
}

fn sorted(it: impl Iterator<Item = u64>) -> Vec<u64> {
    let mut v: Vec<u64> = it.collect();
    v.sort_unstable();
    v
}

fn report_bench(summaries: &[BenchSummary], cover_px: u32, caps: render3d::Caps, free_run: bool) {
    // Smoothness is what compresses, so both "no cover" and "a thumbnail
    // upscaled into the render" understate the image bytes — the second is the
    // sneakier one, because the book visibly has a cover.
    if cover_px == 0 {
        eprintln!(
            "WARNING: this book has no cover image on disk, so the render used the\n\
             procedural plate. It compresses several times better than a photograph —\n\
             the image byte rates below are optimistic. Bench a book with a real cover."
        );
    } else if cover_px < 400 {
        eprintln!(
            "WARNING: the best cover in the library is only {cover_px}px wide (an\n\
             OpenLibrary '-M' thumbnail is 180px). Upscaled into the render it is\n\
             smooth, so it compresses far better than a real cover and the image\n\
             byte rates below are optimistic."
        );
    }
    if !caps.supports_pixels() {
        eprintln!(
            "WARNING: this terminal reports no usable pixel protocol, so every mode\n\
             fell back to glyphs and the comparison is meaningless."
        );
    }
    if free_run {
        eprintln!(
            "WARNING: --bench-free-run. These rates are throughput, NOT what the app\n\
             costs: the retransmit throttle keys off pose, so drawing faster than the\n\
             20fps tick inflates the send rate well above any real session."
        );
    }
    eprintln!(
        "in tmux     : {}   cell px: {:?}   paced: {}",
        caps.in_tmux,
        caps.cell_px,
        if free_run { "no (free-run)" } else { "20fps" }
    );
    eprintln!();
    eprintln!(
        "{:<12} {:>6} {:>6} {:>9} {:>10} {:>9} {:>7} {:>8} {:>8}",
        "mode", "frames", "fps", "text MB/s", "image MB/s", "moving-KB", "sends", "trace", "draw"
    );
    for s in summaries {
        let secs = s.wall.as_secs_f64().max(1e-9);
        eprintln!(
            "{:<12} {:>6} {:>6.1} {:>9.2} {:>10.2} {:>9} {:>7} {:>7}µs {:>7}µs",
            s.mode,
            s.frames,
            s.frames as f64 / secs,
            s.bytes_text as f64 / secs / 1.0e6,
            s.bytes_image as f64 / secs / 1.0e6,
            s.moving_image_bytes / 1024,
            s.transmits,
            s.trace_us,
            s.draw_us,
        );
    }
    eprintln!();
    eprintln!("object rect : {:?}", summaries.first().map(|s| s.rect));
    eprintln!(
        "'moving-KB' is image bytes sent while the book was animating. For the\n\
         hybrid this must be 0 — that is the invariant, not a budget."
    );
    eprintln!();
    // The honest number: how long the terminal took to answer while we loaded
    // it. Every timing on our side of the pty looked healthy during the version
    // that made terminals unusable.
    eprintln!(
        "{:<12} {:>10} {:>10} {:>10} {:>10} {:>10} {:>10}",
        "mode", "kitty p50", "kitty p90", "kitty max", "tmux p50", "tmux p90", "tmux max"
    );
    for s in summaries {
        let ms = |v: &[u64], p: f64| perf::percentile(v, p) as f64 / 1000.0;
        eprintln!(
            "{:<12} {:>9.1}m {:>9.1}m {:>9.1}m {:>9.1}m {:>9.1}m {:>9.1}m",
            s.mode,
            ms(&s.rtt_kitty, 50.0),
            ms(&s.rtt_kitty, 90.0),
            ms(&s.rtt_kitty, 100.0),
            ms(&s.rtt_tmux, 50.0),
            ms(&s.rtt_tmux, 90.0),
            ms(&s.rtt_tmux, 100.0),
        );
    }
    if summaries.iter().all(|s| s.rtt_kitty.is_empty()) {
        eprintln!(
            "\nNo graphics round-trips came back, so the latency columns are empty and\n\
             every mode almost certainly fell back to glyphs. Inside tmux this usually\n\
             means the pane was not the active one — tmux routes input to the focused\n\
             pane only. Check with `--probe` from the same pane."
        );
    }
    // A paced run that missed its pacing measured a different workload than the
    // one it reports, so say so rather than let the columns look ordinary.
    if !free_run {
        let target = 1.0 / app::TICK.as_secs_f64();
        if let Some(worst) = summaries
            .iter()
            .map(|s| s.frames as f64 / s.wall.as_secs_f64().max(1e-9))
            .min_by(|a, b| a.partial_cmp(b).expect("finite"))
            && worst < target * 0.9
        {
            eprintln!(
                "\nWARNING: a run managed only {worst:.1} fps against the {target:.0} fps tick.\n\
                 The frame budget was missed, so these columns describe a slower\n\
                 workload than the app's. Re-run on an idle machine."
            );
        }
    }
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

/// The terminal, with every byte on its way out counted.
///
/// Wrapping the backend's writer is the only way to measure the *glyph* path at
/// all: its bytes are produced by ratatui's diff, which the app never sees. Once
/// that number exists, "glyphs are cheap" stops being a claim and becomes a
/// measurement.
type CountedTerminal = Terminal<CrosstermBackend<perf::CountingWriter<Stdout>>>;

fn setup_terminal(meter: perf::Meter) -> Result<(CountedTerminal, render3d::Caps)> {
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
    let counted = perf::CountingWriter::new(stdout(), meter, perf::ByteClass::Text);
    let mut terminal = Terminal::new(CrosstermBackend::new(counted))?;
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

    fn summary(mode: &'static str, image_bytes: u64) -> BenchSummary {
        BenchSummary {
            mode,
            frames: 240,
            wall: std::time::Duration::from_secs(12),
            bytes_text: 1_000_000,
            bytes_image: image_bytes,
            transmits: 3,
            rect: (50, 26),
            trace_us: 2_100,
            draw_us: 3_400,
            moving_image_bytes: image_bytes,
            rtt_kitty: vec![900, 1_200, 4_000],
            rtt_tmux: vec![300, 400],
        }
    }

    #[test]
    fn the_history_file_gets_a_header_once_and_a_row_per_run() {
        // The point of this file is comparison across runs, so appending (not
        // truncating) and writing the header exactly once are the behaviours
        // that matter — a second header line in the middle breaks every tool
        // that would read it.
        let dir = std::env::temp_dir().join(format!("readingbuddy-history-{}", std::process::id()));
        let path = dir.join("history.tsv");
        let _ = std::fs::remove_file(&path);

        let caps = render3d::Caps::default();
        append_history(
            &path,
            &[summary("glyph", 0), summary("rich", 0)],
            525,
            caps,
            "live",
        )
        .unwrap();
        append_history(&path, &[summary("glyph", 4_096)], 0, caps, "box").unwrap();

        let text = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 4, "expected header + 3 rows:\n{text}");
        assert!(lines[0].starts_with("epoch_s\tmode\t"), "{}", lines[0]);
        assert_eq!(
            lines.iter().filter(|l| l.starts_with("epoch_s")).count(),
            1,
            "the header was written more than once"
        );
        // Every row must have as many fields as the header, or `cut`/`column`
        // silently misalign the columns instead of failing.
        let cols = lines[0].split('\t').count();
        for row in &lines[1..] {
            assert_eq!(row.split('\t').count(), cols, "ragged row: {row}");
        }
        // The column the whole exercise is about.
        assert!(
            lines[1].contains("\t0\t"),
            "moving_KB missing: {}",
            lines[1]
        );
        assert!(lines[3].contains("\t4\t"), "moving_KB wrong: {}", lines[3]);
        // Rows must carry the environment they were measured in, or a sandbox
        // run and a live-pane run trend against each other.
        assert!(lines[1].contains("\tlive\t"), "env missing: {}", lines[1]);
        assert!(lines[3].contains("\tbox\t"), "env missing: {}", lines[3]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_history_file_with_stale_columns_is_rotated_not_appended_to() {
        // This is the bug that already happened: `real_cover` (a bool) became
        // `cover_px` (a width), and three runs' rows went on landing under the
        // old column name. Rows that mean different things must not share a
        // header, and the old ones must not be destroyed either.
        let dir = std::env::temp_dir().join(format!("readingbuddy-rotate-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("history.tsv");
        std::fs::write(&path, "epoch_s\tmode\told_column\n1\tglyph\ttrue\n").unwrap();

        append_history(
            &path,
            &[summary("glyph", 0)],
            525,
            render3d::Caps::default(),
            "live",
        )
        .unwrap();

        let text = std::fs::read_to_string(&path).unwrap();
        assert!(
            text.starts_with(HISTORY_HEADER),
            "the new file did not get the current header"
        );
        assert_eq!(text.lines().count(), 2, "old rows leaked into the new file");
        let kept = std::fs::read_to_string(dir.join("history.tsv.old")).unwrap();
        assert!(
            kept.contains("old_column"),
            "the old run data was destroyed"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn parses_frame_sizes() {
        assert_eq!(parse_size("100x30").unwrap(), (100, 30));
        assert_eq!(parse_size(" 40 X 20 ").unwrap(), (40, 20));
        assert!(parse_size("100").is_err());
        assert!(parse_size("axb").is_err());
    }
}
