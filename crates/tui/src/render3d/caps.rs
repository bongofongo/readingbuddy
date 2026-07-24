//! What the terminal can actually do — probed once, at startup.
//!
//! The book has more than one presentation, and which one is best is a property
//! of the *situation*, not of a single boolean. `$TMUX` alone was the old test
//! and it is the wrong one: kitty graphics survive tmux perfectly well through
//! DCS passthrough, so the interesting question is "does this terminal answer
//! the kitty graphics query, and can bytes reach it?"
//!
//! The probe writes three escapes and reads the replies off the tty:
//!
//! | query | reply | tells us |
//! |---|---|---|
//! | `_Ga=q` (tmux-wrapped) | `_Gi=<id>;OK` | kitty graphics works, passthrough included |
//! | `CSI 16 t` (tmux-wrapped) | `CSI 6;h;w t` | the outer terminal's cell size in pixels |
//! | `CSI c` (never wrapped) | `CSI ? … c` | **sentinel**: proof the terminal is answering at all |
//!
//! The sentinel is the load-bearing part. Every terminal answers DA1, and DA1
//! goes unwrapped so tmux itself answers it. So a DA1 reply with no `_G` reply
//! is a *definitive* "no kitty graphics here" rather than an indistinguishable
//! timeout — which is what lets the probe finish fast and be sure. Total
//! silence means we are not talking to a real terminal, and only then do the
//! environment heuristics get a vote.
//!
//! Two things measured in kitty + tmux 3.5a, both of which shape the code:
//!
//! - The replies really do come back through passthrough, but **out of order**:
//!   tmux answers DA1 locally and immediately, while kitty's `_G` and cell-size
//!   replies make the longer trip and land *after* it. Stopping at the sentinel
//!   would therefore miss both, which is why [`GRACE`] exists.
//! - tmux routes terminal input to the **active pane only**. Probing from a
//!   background pane sees DA1 (tmux's own) but never kitty's replies, and so
//!   concludes "no graphics". That is a false negative, but a safe one: it
//!   degrades to glyphs rather than spraying escapes, it only happens if the
//!   pane is unfocused during the first 200 ms of startup, and `--render rich`
//!   or the runtime toggle overrides it.

use std::io::{IsTerminal, Write, stdin, stdout};
use std::sync::atomic::{AtomicU8, Ordering};
use std::time::{Duration, Instant};

/// How long to wait for any reply at all before giving up on the terminal.
const DEADLINE: Duration = Duration::from_millis(200);
/// Once the DA1 sentinel lands, how much longer to wait for stragglers. The
/// kitty reply travels further than tmux's own DA1 answer, so it can arrive
/// second even though it was asked first.
const GRACE: Duration = Duration::from_millis(60);
/// Arbitrary id for the query image; distinct from the ids the renderer uses so
/// a stray reply can never be mistaken for a real one.
const PROBE_ID: u32 = 31;
/// Assumed cell size when nothing reports one. Conservative: guessing small
/// under-renders (slightly soft) where guessing large would blow the pixel
/// budget for no visible gain.
const ASSUMED_CELL_PX: (u16, u16) = (8, 16);

/// State of tmux's `allow-passthrough` for our pane.
///
/// It is a *pane* option, which is the whole reason this is tractable: we can
/// turn it on for our own pane and put it back on the way out, without ever
/// touching the user's `tmux.conf` or affecting any other pane.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Passthrough {
    /// Not running under tmux — nothing to arrange.
    NotTmux,
    /// Already enabled (inherited or set by the user).
    Already,
    /// We turned it on and owe a restore.
    EnabledByUs,
    /// tmux is present but we could not enable it; pixels cannot get out.
    Unavailable,
}

impl Passthrough {
    /// Can escape sequences reach the outer terminal?
    pub fn is_open(self) -> bool {
        !matches!(self, Passthrough::Unavailable)
    }
}

/// The probed situation. `Copy` on purpose: `ui::book::present_book` relies on
/// disjoint *direct field* borrows of `App`, so anything the presenter needs
/// must be copyable out of `App` before the `&mut app.scene` borrow starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Caps {
    /// The terminal answered the kitty graphics query.
    pub kitty_graphics: bool,
    /// Cell size in device pixels, `(width, height)`.
    pub cell_px: (u16, u16),
    /// False when `cell_px` is the assumed default rather than a real report.
    pub cell_px_measured: bool,
    pub in_tmux: bool,
    pub passthrough: Passthrough,
}

impl Default for Caps {
    /// The safe floor: block glyphs, nothing assumed. Tests and `--dump-frame`
    /// get this, so no test can accidentally emit graphics escapes.
    fn default() -> Self {
        Caps {
            kitty_graphics: false,
            cell_px: ASSUMED_CELL_PX,
            cell_px_measured: false,
            in_tmux: false,
            passthrough: Passthrough::NotTmux,
        }
    }
}

impl Caps {
    /// Is the pixel path actually usable? Both halves matter: a terminal that
    /// speaks the protocol is no use if tmux is eating the bytes.
    pub fn supports_pixels(self) -> bool {
        self.kitty_graphics && self.passthrough.is_open()
    }
}

/// What came back off the tty. Separated from the I/O so the parsing is a pure
/// function over bytes and can be unit-tested against captured replies.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct ProbeReplies {
    pub kitty_ok: bool,
    /// `(width, height)` in pixels, from `CSI 6 ; height ; width t`.
    pub cell_px: Option<(u16, u16)>,
    /// The DA1 sentinel landed, so the terminal is definitely answering.
    pub da1: bool,
}

/// Wrap a payload for tmux's DCS passthrough: `ESC P tmux; … ESC \`, with every
/// inner ESC doubled so tmux forwards it instead of interpreting it.
pub fn wrap_tmux(payload: &str) -> String {
    let mut out = String::with_capacity(payload.len() + 16);
    out.push_str("\x1bPtmux;");
    for ch in payload.chars() {
        if ch == '\x1b' {
            out.push('\x1b');
        }
        out.push(ch);
    }
    out.push_str("\x1b\\");
    out
}

/// Pass `payload` through tmux when needed, or hand it back untouched.
pub fn maybe_wrap(payload: &str, in_tmux: bool) -> String {
    if in_tmux {
        wrap_tmux(payload)
    } else {
        payload.to_string()
    }
}

/// Scan raw tty bytes for the three replies. Tolerant by design: replies can
/// interleave, arrive in any order, and be surrounded by unrelated input.
pub fn parse_replies(bytes: &[u8], probe_id: u32) -> ProbeReplies {
    let mut out = ProbeReplies::default();
    // `_Gi=<id>;OK` — the id must match so an unrelated graphics response
    // (or an echo of our own query) can't be read as success.
    let expected = format!("\x1b_Gi={probe_id};OK");
    out.kitty_ok = find(bytes, expected.as_bytes()).is_some();

    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != 0x1b {
            i += 1;
            continue;
        }
        // CSI 6 ; height ; width t — the cell-size report.
        if let Some(rest) = bytes.get(i + 1..)
            && rest.starts_with(b"[6;")
            && let Some(end) = rest.iter().position(|&b| b == b't')
            && let Ok(text) = std::str::from_utf8(&rest[3..end])
        {
            let mut parts = text.split(';');
            if let (Some(h), Some(w)) = (parts.next(), parts.next())
                && let (Ok(h), Ok(w)) = (h.trim().parse::<u16>(), w.trim().parse::<u16>())
                && let Some(px) = sane_cell_px(w, h)
            {
                out.cell_px = Some(px);
            }
        }
        // CSI ? … c — DA1.
        if let Some(rest) = bytes.get(i + 1..)
            && rest.starts_with(b"[?")
            && rest.iter().take_while(|&&b| b != 0x1b).any(|&b| b == b'c')
        {
            out.da1 = true;
        }
        i += 1;
    }
    out
}

/// Reject cell sizes no real font produces. A garbage report is worse than no
/// report: it silently scales every rich frame to the wrong resolution, whereas
/// falling back to the assumed size is merely slightly soft.
///
/// Text cells are taller than they are wide, by somewhere between 1.2x and 3x
/// on anything anyone reads code in.
fn sane_cell_px(w: u16, h: u16) -> Option<(u16, u16)> {
    if !(4..=64).contains(&w) || !(8..=128).contains(&h) {
        return None;
    }
    let ratio = h as f32 / w as f32;
    (1.2..=3.0).contains(&ratio).then_some((w, h))
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    (0..=haystack.len() - needle.len()).find(|&i| &haystack[i..i + needle.len()] == needle)
}

/// Probe the terminal. Must run with raw mode already on (so replies are not
/// line-buffered or echoed) and before ratatui's event reader starts, so the
/// reply bytes cannot leak into the later `EventStream`.
pub fn probe() -> Caps {
    probe_verbose().0
}

/// [`probe`], plus the raw reply bytes. `--probe` prints them: when detection
/// surprises you, the bytes the terminal actually sent are the only evidence
/// that settles it.
pub fn probe_verbose() -> (Caps, Vec<u8>) {
    let in_tmux = std::env::var_os("TMUX").is_some();
    let mut caps = Caps {
        in_tmux,
        ..Caps::default()
    };

    // Not a terminal (piped, redirected, a test harness): never write escapes.
    if !stdin().is_terminal() || !stdout().is_terminal() {
        return (caps, Vec::new());
    }

    if in_tmux {
        caps.passthrough = enable_passthrough();
        if caps.passthrough == Passthrough::Unavailable {
            // Nothing can reach the outer terminal; don't bother asking.
            return (caps, Vec::new());
        }
    }

    let kitty_query = format!("\x1b_Gi={PROBE_ID},s=1,v=1,a=q,t=d,f=24;AAAA\x1b\\");
    let mut out = String::new();
    out.push_str(&maybe_wrap(&kitty_query, in_tmux));
    out.push_str(&maybe_wrap("\x1b[16t", in_tmux));
    // DA1 goes unwrapped on purpose: inside tmux, tmux answers it itself, so it
    // arrives even when passthrough is broken. That makes it a true sentinel.
    out.push_str("\x1b[c");

    let mut stdout = stdout();
    if stdout.write_all(out.as_bytes()).is_err() || stdout.flush().is_err() {
        return (caps, Vec::new());
    }

    let raw = read_replies();
    let replies = parse_replies(&raw, PROBE_ID);
    if replies.kitty_ok {
        caps.kitty_graphics = true;
    } else if !replies.da1 {
        // Total silence — the terminal never answered anything, so we learned
        // nothing. Only here do environment guesses earn a vote.
        caps.kitty_graphics = env_suggests_kitty();
    }
    // A DA1 with no `_G` reply is definitive: this terminal has no kitty
    // graphics, and no env var gets to overrule what the terminal just said.

    if let Some(px) = replies.cell_px {
        caps.cell_px = px;
        caps.cell_px_measured = true;
    } else if let Some(px) = cell_px_from_window_size() {
        caps.cell_px = px;
        caps.cell_px_measured = true;
    }

    (caps, raw)
}

/// Read from the tty until we have what we need, the sentinel's grace period
/// expires, or the hard deadline passes. Never blocks indefinitely.
#[cfg(unix)]
fn read_replies() -> Vec<u8> {
    use std::os::fd::AsRawFd;

    let fd = stdin().as_raw_fd();
    let start = Instant::now();
    let mut buf = Vec::with_capacity(256);
    let mut chunk = [0u8; 256];
    let mut sentinel_at: Option<Instant> = None;

    loop {
        let elapsed = start.elapsed();
        if elapsed >= DEADLINE {
            break;
        }
        let mut budget = DEADLINE - elapsed;
        if let Some(seen) = sentinel_at {
            let grace_left = GRACE.saturating_sub(seen.elapsed());
            if grace_left.is_zero() {
                break;
            }
            budget = budget.min(grace_left);
        }

        let mut pfd = libc::pollfd {
            fd,
            events: libc::POLLIN,
            revents: 0,
        };
        // SAFETY: a single valid pollfd for a fd we own, count matches.
        let ready = unsafe { libc::poll(&mut pfd, 1, budget.as_millis() as libc::c_int) };
        if ready <= 0 {
            break;
        }
        // SAFETY: reading into a stack buffer we own, length matches.
        let n = unsafe { libc::read(fd, chunk.as_mut_ptr() as *mut libc::c_void, chunk.len()) };
        if n <= 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..n as usize]);

        let replies = parse_replies(&buf, PROBE_ID);
        // Everything we asked for is in: stop early rather than burn the grace.
        if replies.kitty_ok && replies.cell_px.is_some() && replies.da1 {
            break;
        }
        if replies.da1 && sentinel_at.is_none() {
            sentinel_at = Some(Instant::now());
        }
    }

    buf
}

#[cfg(not(unix))]
fn read_replies() -> Vec<u8> {
    Vec::new()
}

/// Derive cell pixels from the kernel's window size, which reports the window
/// in both cells and pixels. Usually zero inside tmux (tmux is not a real
/// window), hence the escape query first.
fn cell_px_from_window_size() -> Option<(u16, u16)> {
    let ws = crossterm::terminal::window_size().ok()?;
    if ws.width == 0 || ws.height == 0 || ws.columns == 0 || ws.rows == 0 {
        return None;
    }
    Some((ws.width / ws.columns, ws.height / ws.rows))
}

/// Last-resort guess, used *only* when the terminal answered nothing at all.
/// `TERM_PROGRAM` is useless inside tmux (it reads `tmux`), but the outer
/// terminal's own markers survive in the environment.
fn env_suggests_kitty() -> bool {
    let has = |k: &str| std::env::var_os(k).is_some_and(|v| !v.is_empty());
    if has("KITTY_WINDOW_ID") || has("GHOSTTY_RESOURCES_DIR") || has("WEZTERM_EXECUTABLE") {
        return true;
    }
    let term = std::env::var("TERM").unwrap_or_default();
    if term.contains("kitty") || term.contains("ghostty") {
        return true;
    }
    matches!(
        std::env::var("TERM_PROGRAM").unwrap_or_default().as_str(),
        "ghostty" | "WezTerm"
    )
}

// ---------------------------------------------------------------------------
// tmux passthrough
// ---------------------------------------------------------------------------

/// What we owe tmux on the way out.
///
/// An atomic rather than a `Mutex`, deliberately: the panic hook runs this, and
/// a mutex could be poisoned or — worse — still held by the very thread that is
/// panicking, which would deadlock the hook. There are only three states, so an
/// atomic says everything a lock would.
///
/// We only ever set the option when its effective value was *not* already on,
/// so the pane-scoped value we displaced can only have been "unset" or "off".
const OWE_NOTHING: u8 = 0;
const OWE_UNSET: u8 = 1;
const OWE_OFF: u8 = 2;
static PRIOR_PASSTHROUGH: AtomicU8 = AtomicU8::new(OWE_NOTHING);

fn tmux(args: &[&str]) -> Option<String> {
    let out = std::process::Command::new("tmux")
        .args(args)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Turn `allow-passthrough` on for *our pane only*, remembering what to put
/// back. Scoping to the pane is deliberate: it needs no config file edit, no
/// user action, and it dies with the pane even if we are killed outright.
fn enable_passthrough() -> Passthrough {
    let pane = std::env::var("TMUX_PANE").unwrap_or_default();
    if pane.is_empty() {
        return Passthrough::Unavailable;
    }

    // The effective value, inherited options included.
    match tmux(&["show", "-pAv", "-t", &pane, "allow-passthrough"]).as_deref() {
        Some("on") | Some("all") => return Passthrough::Already,
        None => return Passthrough::Unavailable,
        _ => {}
    }

    // The pane-scoped value specifically, so the restore is exact rather than
    // approximate: an empty answer means the pane had no value of its own and
    // was inheriting, which we must restore by unsetting, not by writing "off".
    let owed = match tmux(&["show", "-pv", "-t", &pane, "allow-passthrough"]).as_deref() {
        Some("") | None => OWE_UNSET,
        Some(_) => OWE_OFF,
    };
    if tmux(&["set", "-p", "-t", &pane, "allow-passthrough", "on"]).is_none() {
        return Passthrough::Unavailable;
    }
    PRIOR_PASSTHROUGH.store(owed, Ordering::SeqCst);
    Passthrough::EnabledByUs
}

/// Put `allow-passthrough` back where we found it.
///
/// Idempotent (the swap clears the debt), never panics, takes no locks — it has
/// to be callable from the panic hook.
pub fn restore_passthrough() {
    let owed = PRIOR_PASSTHROUGH.swap(OWE_NOTHING, Ordering::SeqCst);
    if owed == OWE_NOTHING {
        return;
    }
    let pane = std::env::var("TMUX_PANE").unwrap_or_default();
    if pane.is_empty() {
        return;
    }
    if owed == OWE_UNSET {
        tmux(&["set", "-p", "-t", &pane, "-u", "allow-passthrough"]);
    } else {
        tmux(&["set", "-p", "-t", &pane, "allow-passthrough", "off"]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wraps_for_tmux_and_doubles_escapes() {
        assert_eq!(wrap_tmux("\x1b[16t"), "\x1bPtmux;\x1b\x1b[16t\x1b\\");
        assert_eq!(wrap_tmux("plain"), "\x1bPtmux;plain\x1b\\");
        // Every ESC in the payload must double, not just the first.
        assert_eq!(
            wrap_tmux("\x1ba\x1bb"),
            "\x1bPtmux;\x1b\x1ba\x1b\x1bb\x1b\\"
        );
    }

    #[test]
    fn maybe_wrap_is_identity_outside_tmux() {
        assert_eq!(maybe_wrap("\x1b[c", false), "\x1b[c");
        assert_eq!(maybe_wrap("\x1b[c", true), wrap_tmux("\x1b[c"));
    }

    #[test]
    fn parses_a_full_kitty_reply() {
        let raw = b"\x1b_Gi=31;OK\x1b\\\x1b[6;39;19t\x1b[?62;c";
        let r = parse_replies(raw, 31);
        assert!(r.kitty_ok);
        // The report is height-then-width; we store (width, height).
        assert_eq!(r.cell_px, Some((19, 39)));
        assert!(r.da1);
    }

    #[test]
    fn da1_without_graphics_is_a_definitive_no() {
        let r = parse_replies(b"\x1b[?62;22c", 31);
        assert!(!r.kitty_ok);
        assert!(r.da1);
        assert_eq!(r.cell_px, None);
    }

    #[test]
    fn silence_reports_nothing() {
        assert_eq!(parse_replies(b"", 31), ProbeReplies::default());
    }

    #[test]
    fn ignores_a_reply_for_a_different_image_id() {
        let r = parse_replies(b"\x1b_Gi=7;OK\x1b\\\x1b[?62;c", 31);
        assert!(!r.kitty_ok, "another image's OK must not count as ours");
        assert!(r.da1);
    }

    #[test]
    fn tolerates_interleaved_and_out_of_order_replies() {
        // DA1 first (tmux answers locally), kitty's reply trailing behind.
        let raw = b"\x1b[?62;c\x1b[6;32;16t\x1b_Gi=31;OK\x1b\\";
        let r = parse_replies(raw, 31);
        assert!(r.kitty_ok && r.da1);
        assert_eq!(r.cell_px, Some((16, 32)));
    }

    #[test]
    fn tolerates_unrelated_input_around_the_replies() {
        let raw = b"qqq\x1b[?62;c\x1bOA\x1b_Gi=31;OK\x1b\\zz";
        let r = parse_replies(raw, 31);
        assert!(r.kitty_ok && r.da1);
    }

    #[test]
    fn a_truncated_cell_report_is_no_report() {
        // No terminating 't' yet — must not half-parse into a bogus size.
        assert_eq!(parse_replies(b"\x1b[6;39;19", 31).cell_px, None);
        // Zero is not a usable cell size.
        assert_eq!(parse_replies(b"\x1b[6;0;0t", 31).cell_px, None);
    }

    #[test]
    fn rejects_cell_sizes_no_font_produces() {
        // A garbage report would silently scale every rich frame wrongly, so
        // implausible geometry has to be discarded, not clamped into range.
        assert_eq!(
            parse_replies(b"\x1b[6;3;40t", 31).cell_px,
            None,
            "wider than tall"
        );
        assert_eq!(parse_replies(b"\x1b[6;400;2t", 31).cell_px, None, "absurd");
        assert_eq!(
            parse_replies(b"\x1b[6;16;16t", 31).cell_px,
            None,
            "square cell"
        );
        // Plausible geometry survives: a retina kitty cell and a small one.
        assert_eq!(parse_replies(b"\x1b[6;39;19t", 31).cell_px, Some((19, 39)));
        assert_eq!(parse_replies(b"\x1b[6;16;8t", 31).cell_px, Some((8, 16)));
    }

    #[test]
    fn default_caps_are_the_safe_floor() {
        let caps = Caps::default();
        assert!(!caps.supports_pixels());
        assert!(!caps.cell_px_measured);
        assert_eq!(caps.passthrough, Passthrough::NotTmux);
    }

    #[test]
    fn pixels_need_both_protocol_and_an_open_path() {
        let base = Caps {
            kitty_graphics: true,
            in_tmux: true,
            passthrough: Passthrough::EnabledByUs,
            ..Caps::default()
        };
        assert!(base.supports_pixels());
        assert!(
            !Caps {
                passthrough: Passthrough::Unavailable,
                ..base
            }
            .supports_pixels(),
            "a blocked tmux must veto pixels even on a capable terminal"
        );
        assert!(
            !Caps {
                kitty_graphics: false,
                ..base
            }
            .supports_pixels()
        );
    }
}
