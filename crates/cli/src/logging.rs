//! Log subscriber for the CLI.
//!
//! The engine only *emits* tracing events; deciding where they go is a frontend
//! job, which is what keeps the engine free of terminal I/O.

use std::io::IsTerminal;

use tracing_subscriber::EnvFilter;

/// Translate `-v` count and `-q` into a filter directive.
///
/// The default is `error`, not `warn`, and that is deliberate: at `warn` the
/// engine's own event at each degradation site would print *and* the command
/// would print its `warning: {diagnostic}` line for the same failure. Defaulting
/// to `error` keeps output byte-identical to before logging existed, and `-v`
/// turns the stream on.
fn directive(verbose: u8, quiet: bool) -> String {
    if quiet {
        return "off".to_string();
    }
    match verbose {
        0 => "readingbuddy=error".to_string(),
        1 => "readingbuddy=info".to_string(),
        2 => "readingbuddy=debug".to_string(),
        _ => "readingbuddy=trace,readingbuddy_cli=trace".to_string(),
    }
}

/// Install the subscriber. Writes to **stderr**, so stdout stays pipeable.
pub fn init(verbose: u8, quiet: bool) {
    // RUST_LOG always wins over the flags.
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(directive(verbose, quiet)));

    let builder = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .with_ansi(std::io::stderr().is_terminal());

    // At -v the log is a human reading along, so keep it terse; past that it is
    // someone debugging, who wants the target and the timestamp.
    if verbose <= 1 {
        builder.without_time().with_target(false).init();
    } else {
        builder.init();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quiet_beats_verbosity() {
        assert_eq!(directive(3, true), "off");
    }

    /// The default must stay `error`: anything louder double-reports every
    /// degradation, once from the engine's event and once from the command's
    /// own `warning:` line.
    #[test]
    fn default_is_quiet_enough_not_to_double_report() {
        assert_eq!(directive(0, false), "readingbuddy=error");
    }

    #[test]
    fn verbosity_climbs_and_then_saturates() {
        assert_eq!(directive(1, false), "readingbuddy=info");
        assert_eq!(directive(2, false), "readingbuddy=debug");
        assert_eq!(directive(3, false), directive(9, false));
    }
}
