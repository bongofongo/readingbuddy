//! Log subscriber for the TUI.
//!
//! Unlike the CLI this cannot write to the terminal at all: stdout is the
//! ratatui backend and stderr scribbles across the user's pane. So the log is a
//! file, always.

use std::path::{Path, PathBuf};

use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::EnvFilter;

/// Keeps the log writer alive.
///
/// **Must outlive `restore_terminal()`** — dropping it flushes, and dropping it
/// early silently truncates the tail of the log, which is precisely the part
/// you want after something went wrong.
pub struct LogHandle {
    _guard: WorkerGuard,
    pub path: PathBuf,
}

/// Install the file subscriber under `log_dir`.
///
/// Returns `None` if the directory cannot be created — a missing log is never
/// worth refusing to start the app over.
pub fn init(log_dir: &Path, level: Option<&str>) -> Option<LogHandle> {
    if let Err(e) = std::fs::create_dir_all(log_dir) {
        eprintln!(
            "warning: could not create log dir {}: {e}",
            log_dir.display()
        );
        return None;
    }
    let path = log_dir.join("tui.log");

    // Nobody is watching a file scroll past, so `info` is the useful default.
    // READINGBUDDY_LOG then RUST_LOG override it.
    let directive = level.unwrap_or("readingbuddy=info,readingbuddy_tui=info");
    let filter = std::env::var("READINGBUDDY_LOG")
        .ok()
        .map(EnvFilter::new)
        .or_else(|| EnvFilter::try_from_default_env().ok())
        .unwrap_or_else(|| EnvFilter::new(directive));

    let appender = tracing_appender::rolling::never(log_dir, "tui.log");
    // Non-blocking matters at a 20fps redraw, but note the trade: when the
    // queue fills, events are **dropped**. The log is lossy under load, so an
    // absence in it is not evidence that nothing happened.
    let (writer, guard) = tracing_appender::non_blocking(appender);

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(writer)
        .with_ansi(false)
        .init();

    Some(LogHandle {
        _guard: guard,
        path,
    })
}
