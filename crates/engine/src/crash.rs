//! Crash reports: a panic hook that leaves something readable behind.
//!
//! This lives in the engine rather than being duplicated across both frontends.
//! The project rule is "zero **terminal** I/O" — the engine already writes vault
//! markdown and cover images, so writing a file it was explicitly handed a path
//! for breaks nothing. It still installs nothing on its own: a frontend calls
//! [`install_hook`], the engine never does.

use std::backtrace::Backtrace;
use std::io::Write;
use std::panic::Location;
use std::path::{Path, PathBuf};

/// Rotate once the log passes this. Small — a crash log nobody can open is no
/// better than none.
const MAX_LOG_BYTES: u64 = 1024 * 1024;

/// Static facts about the running program, supplied by the frontend.
#[derive(Debug, Clone)]
pub struct CrashContext {
    pub app: &'static str,
    pub version: &'static str,
    /// Directory for `crash.log`. Usually `EngineConfig::log_dir`.
    pub log_dir: PathBuf,
    /// The tracing log file, if the frontend keeps one.
    ///
    /// The *path*, deliberately — not a tail of recent events. A ring buffer of
    /// breadcrumbs would need a lock, and a panic hook must never take a lock
    /// the panicking thread might already hold.
    pub log_file: Option<PathBuf>,
}

/// Render a crash report. Pure and testable — no I/O, no clock beyond the
/// timestamp it is given.
///
/// Takes the panic's parts rather than a `&PanicHookInfo` on purpose: that type
/// cannot be constructed outside `std`, so a function taking it directly could
/// never be unit-tested.
pub fn format_report(
    timestamp: &str,
    msg: &str,
    location: Option<&Location<'_>>,
    backtrace: &str,
    thread: &str,
    ctx: &CrashContext,
) -> String {
    let loc = location
        .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
        .unwrap_or_else(|| "<unknown>".to_string());

    let mut out = String::new();
    out.push_str(&format!("time:      {timestamp}\n"));
    out.push_str(&format!("app:       {} {}\n", ctx.app, ctx.version));
    out.push_str(&format!(
        "platform:  {} {}\n",
        std::env::consts::OS,
        std::env::consts::ARCH
    ));
    out.push_str(&format!("thread:    {thread}\n"));
    out.push_str(&format!("location:  {loc}\n"));
    if let Some(f) = &ctx.log_file {
        out.push_str(&format!("log:       {}\n", f.display()));
    }
    out.push_str(&format!("panic:     {msg}\n"));
    out.push_str("backtrace:\n");
    for line in backtrace.lines() {
        out.push_str("  ");
        out.push_str(line);
        out.push('\n');
    }

    // A panic message can carry a URL, and a URL can carry an API key. The
    // whole report goes through the same scrubber as every other output path.
    crate::providers::googlebooks::scrub_key(&out)
}

/// Append a rendered report to `<log_dir>/crash.log`, rotating first if the
/// file has grown past [`MAX_LOG_BYTES`]. Returns the path written.
///
/// Every step is best-effort: a panic *inside* a panic hook aborts the process,
/// which would destroy the very report being written.
fn append_report(log_dir: &Path, report: &str) -> Option<PathBuf> {
    std::fs::create_dir_all(log_dir).ok()?;
    let path = log_dir.join("crash.log");

    if std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0) > MAX_LOG_BYTES {
        let _ = std::fs::rename(&path, log_dir.join("crash.log.1"));
    }

    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .ok()?;
    // Separator first, so consecutive crashes stay distinguishable.
    let _ = f.write_all(b"\n--- crash ---\n");
    let _ = f.write_all(report.as_bytes());
    let _ = f.flush();
    Some(path)
}

/// Extract the panic payload as a string. `panic!("literal")` yields a `&str`;
/// `panic!("{x}")` and `.expect(..)` yield a `String`.
pub fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "<non-string panic payload>".to_string()
    }
}

/// Install a panic hook that writes a crash report, then chains to whatever hook
/// was already installed.
///
/// **Install this before the terminal is put into raw mode.** `set_hook` chains
/// through `take_hook`, so the *last* hook installed runs *first*. The TUI
/// installs its terminal-restoring hook during setup; if this one goes in
/// beforehand, the order on panic becomes:
///
/// ```text
/// TUI hook -> restore_terminal() -> this hook -> write file -> std default -> stderr
/// ```
///
/// so the terminal is already back to normal by the time anything prints, and
/// the TUI's existing hook needs no changes at all.
pub fn install_hook(ctx: CrashContext) {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let msg = panic_message(info.payload());
        let thread = std::thread::current()
            .name()
            .unwrap_or("<unnamed>")
            .to_string();
        // `force_capture`, not `capture`: this must work without the user
        // having set RUST_BACKTRACE. Needs the release profile to keep debug
        // info, or it resolves to bare addresses.
        let bt = Backtrace::force_capture().to_string();
        let ts = timestamp();

        let report = format_report(&ts, &msg, info.location(), &bt, &thread, &ctx);
        if let Some(path) = append_report(&ctx.log_dir, &report) {
            // The entire user-facing payoff. Without this line the file is
            // written and never found. Runs after the TUI has already restored
            // the terminal, so it lands legibly in the scrollback.
            eprintln!(
                "\nreadingbuddy crashed. Report written to {}",
                path.display()
            );
        }
        previous(info);
    }));
}

fn timestamp() -> String {
    use time::format_description::well_known::Rfc3339;
    time::OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "<unknown time>".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> CrashContext {
        CrashContext {
            app: "readingbuddy-tui",
            version: "0.1.0",
            log_dir: PathBuf::from("/tmp/rb/logs"),
            log_file: Some(PathBuf::from("/tmp/rb/logs/tui.log")),
        }
    }

    #[test]
    fn a_report_carries_what_a_bug_report_needs() {
        let r = format_report(
            "2026-07-27T10:00:00Z",
            "index out of bounds",
            None,
            "0: foo\n1: bar",
            "main",
            &ctx(),
        );
        assert!(r.contains("readingbuddy-tui 0.1.0"));
        assert!(r.contains("index out of bounds"));
        assert!(r.contains("thread:    main"));
        assert!(r.contains(std::env::consts::OS));
        // The path of the tracing log, so the two can be read together.
        assert!(r.contains("/tmp/rb/logs/tui.log"));
        // Backtrace lines are indented, so they can't be confused for headers.
        assert!(r.contains("  0: foo"));
    }

    /// A panic message can quote a URL, and a URL can carry an API key. A crash
    /// report is exactly the file someone pastes into a bug report.
    #[test]
    fn a_report_never_leaks_an_api_key() {
        let r = format_report(
            "2026-07-27T10:00:00Z",
            "failed on https://googleapis.com/books?q=x&key=SECRETVALUE123",
            None,
            "0: somewhere?key=ALSOSECRET",
            "main",
            &ctx(),
        );
        assert!(!r.contains("SECRETVALUE123"), "{r}");
        assert!(!r.contains("ALSOSECRET"), "{r}");
        assert!(r.contains("key=REDACTED"));
    }

    #[test]
    fn missing_location_is_reported_not_panicked_on() {
        let r = format_report("t", "boom", None, "", "main", &ctx());
        assert!(r.contains("location:  <unknown>"));
    }

    #[test]
    fn panic_payloads_of_both_shapes_are_recovered() {
        let literal: Box<dyn std::any::Any + Send> = Box::new("literal panic");
        assert_eq!(panic_message(&*literal), "literal panic");

        let formatted: Box<dyn std::any::Any + Send> = Box::new(String::from("formatted 42"));
        assert_eq!(panic_message(&*formatted), "formatted 42");

        let weird: Box<dyn std::any::Any + Send> = Box::new(7u32);
        assert_eq!(panic_message(&*weird), "<non-string panic payload>");
    }

    #[test]
    fn reports_append_rather_than_overwrite_and_rotate_when_large() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();

        append_report(dir, "first").unwrap();
        append_report(dir, "second").unwrap();
        let body = std::fs::read_to_string(dir.join("crash.log")).unwrap();
        assert!(
            body.contains("first") && body.contains("second"),
            "a second crash overwrote the first: {body}"
        );
        assert_eq!(
            body.matches("--- crash ---").count(),
            2,
            "crashes must stay separable"
        );

        // Past the size cap the current log is rotated aside, not deleted.
        std::fs::write(
            dir.join("crash.log"),
            vec![b'x'; (MAX_LOG_BYTES + 1) as usize],
        )
        .unwrap();
        append_report(dir, "after rotation").unwrap();
        assert!(dir.join("crash.log.1").exists(), "old log was not kept");
        let fresh = std::fs::read_to_string(dir.join("crash.log")).unwrap();
        assert!(fresh.contains("after rotation"));
        assert!(!fresh.contains("xxxx"), "rotation did not start a new file");
    }

    /// A hook that panics aborts the process and destroys the report it was
    /// writing, so an unwritable directory must be survivable.
    #[test]
    fn an_unwritable_log_dir_is_survivable() {
        let tmp = tempfile::tempdir().unwrap();
        // A *file* where the directory should be: create_dir_all must fail.
        let blocked = tmp.path().join("not-a-dir");
        std::fs::write(&blocked, b"x").unwrap();
        assert_eq!(append_report(&blocked, "report"), None);
    }
}
