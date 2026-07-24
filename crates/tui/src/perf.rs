//! Cost accounting for the render path.
//!
//! This module exists because of one expensive lesson: **our own numbers did
//! not show the problem.** The first rich renderer streamed 7.1 MB/s of texture
//! at the terminal and made it unusable, while every instrument we had reported
//! health — trace and encode were a few milliseconds, `write`/`flush` never
//! blocked, and a bench sustained 270 fps at 27 MB/s without complaint. The cost
//! landed *inside* the terminal (zlib decompress, whole-texture re-upload,
//! recomposite, with tmux re-parsing on the way), which is exactly the region no
//! timer on our side of the pty can see.
//!
//! So this module measures two things nothing else did:
//!
//! 1. **Bytes, split by class.** [`CountingWriter`] wraps both the ratatui
//!    backend and the graphics writer, so text bytes and image bytes are counted
//!    separately. Keeping them apart is the whole point — a byte of image costs a
//!    terminal far more than a byte of text, so a single total is a misleading
//!    number rather than a useful one.
//! 2. **Stage timings**, via a thread-local scratchpad rather than a parameter
//!    threaded through the presenter. The single-book view depends on *disjoint
//!    direct field* borrows of `App` (see `ui::book::present_book`), so adding
//!    another `&mut` to that seam would be a borrow-checker fight for no gain.
//!    Drawing is single-threaded, so a `Cell` is sufficient.
//!
//! The honest measurement of the terminal's own backlog lives elsewhere, in
//! `render3d::caps::rtt_probe` — it needs exclusive stdin, so it can only run
//! under the bench.
//!
//! Everything here is inert unless a log path is set (`--perf-log`, or
//! `READINGBUDDY_PERF_LOG`): the recording calls check one relaxed atomic and
//! return.

use std::cell::Cell;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use serde::Serialize;

/// Set once when a log path is accepted. Checked before every recording call so
/// the default configuration costs a relaxed load and a branch.
static ENABLED: AtomicBool = AtomicBool::new(false);

pub fn enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

/// Which side of the "a byte is not a byte" line a write falls on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ByteClass {
    /// Cells written by ratatui's diff — what the glyph path costs.
    Text,
    /// Graphics-protocol payload: the escape, the compressed texture, the
    /// delete. What the pixel path costs.
    Image,
}

#[derive(Debug, Default)]
struct Counters {
    text: AtomicU64,
    image: AtomicU64,
    writes: AtomicU64,
}

/// Shared byte counters.
///
/// Atomics rather than a mutex, for the same reason `kitty`'s live-image id is:
/// these are touched from the writer the panic hook may also be unwinding
/// through, and a lock there could deadlock on a lock the panicking thread
/// already holds.
#[derive(Clone, Default)]
pub struct Meter(Arc<Counters>);

impl Meter {
    pub fn new() -> Meter {
        Meter::default()
    }

    fn add(&self, class: ByteClass, n: u64) {
        match class {
            ByteClass::Text => self.0.text.fetch_add(n, Ordering::Relaxed),
            ByteClass::Image => self.0.image.fetch_add(n, Ordering::Relaxed),
        };
        self.0.writes.fetch_add(1, Ordering::Relaxed);
    }

    pub fn snapshot(&self) -> Bytes {
        Bytes {
            text: self.0.text.load(Ordering::Relaxed),
            image: self.0.image.load(Ordering::Relaxed),
            writes: self.0.writes.load(Ordering::Relaxed),
        }
    }
}

/// A reading of the counters. Deltas between two of these are one frame's cost.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Bytes {
    pub text: u64,
    pub image: u64,
    pub writes: u64,
}

impl Bytes {
    pub fn since(self, earlier: Bytes) -> Bytes {
        Bytes {
            text: self.text.saturating_sub(earlier.text),
            image: self.image.saturating_sub(earlier.image),
            writes: self.writes.saturating_sub(earlier.writes),
        }
    }
}

/// Counts everything on its way to the terminal, then forwards it untouched.
///
/// Wraps the `Stdout` given to `CrosstermBackend` (as [`ByteClass::Text`]) and
/// the graphics writer inside `RichState` (as [`ByteClass::Image`]). Counting at
/// the writer rather than at the call sites is what makes the glyph path
/// measurable at all: its bytes are produced by ratatui's diff, which we never
/// see.
pub struct CountingWriter<W> {
    inner: W,
    meter: Meter,
    class: ByteClass,
}

impl<W> CountingWriter<W> {
    pub fn new(inner: W, meter: Meter, class: ByteClass) -> Self {
        CountingWriter {
            inner,
            meter,
            class,
        }
    }
}

impl<W: Write> Write for CountingWriter<W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let n = self.inner.write(buf)?;
        self.meter.add(self.class, n as u64);
        Ok(n)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

/// A phase of one frame's work.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    /// Raytracing, either raster.
    Trace,
    /// Compress + base64 for the graphics protocol.
    Encode,
    /// Quantizing subpixels into block glyphs.
    Blit,
    /// Handing the escape to the writer, including its flush.
    Transmit,
}

const STAGE_COUNT: usize = 4;

impl Stage {
    fn idx(self) -> usize {
        match self {
            Stage::Trace => 0,
            Stage::Encode => 1,
            Stage::Blit => 2,
            Stage::Transmit => 3,
        }
    }
}

/// How the frame was actually produced. Filled in by the presenter, read by the
/// event loop, for the same borrow reason the stage scratchpad exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Quality {
    /// Not a pixel frame at all.
    #[default]
    None,
    Motion,
    Settle,
}

/// What the presenter did this frame.
#[derive(Debug, Clone, Copy, Default)]
pub struct FrameNote {
    pub quality: Quality,
    /// The object's rect in cells. Not the terminal's size — this is what the
    /// cost actually scales with, and what two bench runs have to match on to be
    /// comparable.
    pub cols: u16,
    pub rows: u16,
    pub px_w: u32,
    pub px_h: u32,
    /// An image actually went on the wire (as opposed to the cached one being
    /// reused, or the frame being glyphs).
    pub transmitted: bool,
    /// Rich mode was asked for but glyphs were drawn — either the deliberate
    /// motion fallback or a refusal (incapable terminal, oversized rect).
    pub fell_back: bool,
}

thread_local! {
    static STAGES: Cell<[u64; STAGE_COUNT]> = const { Cell::new([0; STAGE_COUNT]) };
    static NOTE: Cell<FrameNote> = const {
        Cell::new(FrameNote {
            quality: Quality::None,
            cols: 0,
            rows: 0,
            px_w: 0,
            px_h: 0,
            transmitted: false,
            fell_back: false,
        })
    };
}

/// Add to a stage's running total for the current frame.
pub fn record(stage: Stage, elapsed: Duration) {
    if !enabled() {
        return;
    }
    STAGES.with(|s| {
        let mut v = s.get();
        v[stage.idx()] += elapsed.as_micros() as u64;
        s.set(v);
    });
}

/// Time a block into a stage. The guard records on drop, so an early return
/// still measures.
pub fn scope(stage: Stage) -> Scope {
    Scope {
        stage,
        start: Instant::now(),
    }
}

pub struct Scope {
    stage: Stage,
    start: Instant,
}

impl Drop for Scope {
    fn drop(&mut self) {
        record(self.stage, self.start.elapsed());
    }
}

/// Tell the recorder how this frame was produced.
pub fn note(note: FrameNote) {
    if !enabled() {
        return;
    }
    NOTE.set(note);
}

/// Record the object's rect without disturbing the rest of the note.
///
/// The glyph presenter calls this, including when it is the hybrid's motion
/// fallback — so the rect is logged either way, while the rich presenter's
/// verdict (quality, `fell_back`) survives the hand-off.
pub fn note_rect(cols: u16, rows: u16) {
    if !enabled() {
        return;
    }
    NOTE.set(FrameNote {
        cols,
        rows,
        ..NOTE.get()
    });
}

/// Drain the scratchpad. Called once per frame by the event loop.
fn take_stages() -> [u64; STAGE_COUNT] {
    STAGES.replace([0; STAGE_COUNT])
}

fn take_note() -> FrameNote {
    NOTE.replace(FrameNote::default())
}

/// One line of the JSONL log.
#[derive(Debug, Serialize)]
pub struct FrameRecord {
    pub seq: u64,
    /// Milliseconds since the recorder opened.
    pub t_ms: u64,
    pub mode: &'static str,
    pub quality: Quality,
    pub moving: bool,
    /// The object's rect in cells.
    pub cols: u16,
    pub rows: u16,
    /// The whole terminal, for context when a run is compared against another
    /// at a different window size.
    pub term_cols: u16,
    pub term_rows: u16,
    pub px_w: u32,
    pub px_h: u32,
    pub transmitted: bool,
    pub fell_back: bool,
    pub trace_us: u64,
    pub encode_us: u64,
    pub blit_us: u64,
    pub transmit_us: u64,
    /// The whole `Terminal::draw`, which is the only number that includes
    /// ratatui's diff.
    pub draw_us: u64,
    pub bytes_text: u64,
    pub bytes_image: u64,
    pub writes: u64,
    /// Round-trip to the terminal itself, when the bench sampled it this frame.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rtt_kitty_us: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rtt_tmux_us: Option<u64>,
}

/// What the caller knows that the scratchpad doesn't.
#[derive(Debug, Clone, Copy)]
pub struct FrameCtx {
    pub mode: &'static str,
    pub moving: bool,
    pub term_cols: u16,
    pub term_rows: u16,
    pub draw: Duration,
    pub rtt_kitty: Option<Duration>,
    pub rtt_tmux: Option<Duration>,
}

/// Owns the log file and the per-frame bookkeeping.
///
/// A direct field of `App`, like `scene` and `rich`, so the draw path can reach
/// it without borrowing all of `App`.
pub struct Recorder {
    meter: Meter,
    sink: Option<BufWriter<File>>,
    start: Instant,
    seq: u64,
    last: Bytes,
    /// Kept for the bench's summary table, which wants aggregates rather than
    /// the per-frame stream.
    frames: Vec<FrameRecord>,
    keep_frames: bool,
}

impl Recorder {
    /// A recorder that writes nothing. Still owns a [`Meter`], so byte counting
    /// works for tests and for the bench even without a log file.
    pub fn disabled(meter: Meter) -> Recorder {
        Recorder {
            meter,
            sink: None,
            start: Instant::now(),
            seq: 0,
            last: Bytes::default(),
            frames: Vec::new(),
            keep_frames: false,
        }
    }

    /// Open a JSONL log. A failure here is reported to the caller but must never
    /// be fatal: a diagnostic that bricks the TUI is worse than no diagnostic.
    pub fn open(path: &Path, meter: Meter) -> std::io::Result<Recorder> {
        // Create the directory rather than failing on it: `--perf-log
        // perf/run.jsonl` is the obvious thing to type, and a diagnostic that
        // quietly writes nothing because a directory was missing is worse than
        // one that never existed.
        if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
            std::fs::create_dir_all(dir)?;
        }
        let file = File::create(path)?;
        ENABLED.store(true, Ordering::Relaxed);
        Ok(Recorder {
            meter,
            sink: Some(BufWriter::new(file)),
            start: Instant::now(),
            seq: 0,
            last: Bytes::default(),
            frames: Vec::new(),
            keep_frames: false,
        })
    }

    /// Turn on recording without a file. The bench needs the counters and the
    /// stage timings for its summary even when the user asked for no log.
    pub fn enable_in_memory(&mut self) {
        ENABLED.store(true, Ordering::Relaxed);
        self.keep_frames = true;
    }

    /// Baseline the byte counters. Called just before the frame's work so the
    /// delta covers this frame only.
    pub fn frame_begin(&mut self) {
        self.last = self.meter.snapshot();
    }

    /// Close a frame: drain the scratchpad, diff the counters, write the line.
    pub fn frame_end(&mut self, ctx: FrameCtx) {
        if !enabled() {
            return;
        }
        let stages = take_stages();
        let note = take_note();
        let delta = self.meter.snapshot().since(self.last);
        let rec = FrameRecord {
            seq: self.seq,
            t_ms: self.start.elapsed().as_millis() as u64,
            mode: ctx.mode,
            quality: note.quality,
            moving: ctx.moving,
            cols: note.cols,
            rows: note.rows,
            term_cols: ctx.term_cols,
            term_rows: ctx.term_rows,
            px_w: note.px_w,
            px_h: note.px_h,
            transmitted: note.transmitted,
            fell_back: note.fell_back,
            trace_us: stages[Stage::Trace.idx()],
            encode_us: stages[Stage::Encode.idx()],
            blit_us: stages[Stage::Blit.idx()],
            transmit_us: stages[Stage::Transmit.idx()],
            draw_us: ctx.draw.as_micros() as u64,
            bytes_text: delta.text,
            bytes_image: delta.image,
            writes: delta.writes,
            rtt_kitty_us: ctx.rtt_kitty.map(|d| d.as_micros() as u64),
            rtt_tmux_us: ctx.rtt_tmux.map(|d| d.as_micros() as u64),
        };
        self.seq += 1;
        if let Some(sink) = self.sink.as_mut()
            && let Ok(line) = serde_json::to_string(&rec)
        {
            // A write failure disables the log rather than propagating: the
            // frame has already been drawn and the user is mid-session.
            if writeln!(sink, "{line}").is_err() {
                self.sink = None;
            }
        }
        if self.keep_frames {
            self.frames.push(rec);
        }
    }

    /// Take the frames recorded so far. The bench calls this between modes, so
    /// each summary covers only its own run.
    pub fn drain_frames(&mut self) -> Vec<FrameRecord> {
        std::mem::take(&mut self.frames)
    }

    pub fn flush(&mut self) {
        if let Some(sink) = self.sink.as_mut() {
            let _ = sink.flush();
        }
    }
}

impl Drop for Recorder {
    fn drop(&mut self) {
        self.flush();
    }
}

/// Nearest-rank percentile over a sorted slice. Used for the RTT distribution,
/// where the tail is the interesting part — a p50 hides exactly the stalls we
/// are hunting.
pub fn percentile(sorted: &[u64], p: f64) -> u64 {
    if sorted.is_empty() {
        return 0;
    }
    let rank = ((p / 100.0) * sorted.len() as f64).ceil().max(1.0) as usize;
    sorted[rank.min(sorted.len()) - 1]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Serialize the module's global `ENABLED` flag across tests, which cargo
    /// runs in parallel threads by default.
    fn guard() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        LOCK.lock().unwrap_or_else(|e| e.into_inner())
    }

    #[test]
    fn a_counting_writer_separates_text_from_image_bytes() {
        let meter = Meter::new();
        let mut text = CountingWriter::new(Vec::new(), meter.clone(), ByteClass::Text);
        let mut image = CountingWriter::new(Vec::new(), meter.clone(), ByteClass::Image);
        text.write_all(b"hello").unwrap();
        image.write_all(b"\x1b_Gpayload").unwrap();
        image.write_all(b"more").unwrap();

        let b = meter.snapshot();
        assert_eq!(b.text, 5);
        assert_eq!(b.image, 14);
        // Keeping the classes apart is the point: a byte of image costs the
        // terminal far more than a byte of text, so a combined total would be a
        // misleading number rather than a useful one.
        assert_eq!(b.writes, 3);
    }

    #[test]
    fn byte_deltas_are_per_frame() {
        let meter = Meter::new();
        let mut w = CountingWriter::new(Vec::new(), meter.clone(), ByteClass::Text);
        w.write_all(b"1234").unwrap();
        let after_first = meter.snapshot();
        w.write_all(b"567").unwrap();
        assert_eq!(meter.snapshot().since(after_first).text, 3);
    }

    #[test]
    fn recording_is_inert_until_enabled() {
        let _g = guard();
        ENABLED.store(false, Ordering::Relaxed);
        take_stages();
        record(Stage::Trace, Duration::from_millis(5));
        assert_eq!(take_stages(), [0; STAGE_COUNT], "recorded while disabled");

        ENABLED.store(true, Ordering::Relaxed);
        record(Stage::Trace, Duration::from_millis(5));
        assert_eq!(take_stages()[Stage::Trace.idx()], 5_000);
        ENABLED.store(false, Ordering::Relaxed);
    }

    #[test]
    fn a_scope_records_on_drop() {
        let _g = guard();
        ENABLED.store(true, Ordering::Relaxed);
        take_stages();
        {
            let _t = scope(Stage::Blit);
            // Long enough to register at microsecond granularity, which is what
            // the log stores.
            std::thread::sleep(Duration::from_millis(2));
        }
        assert!(take_stages()[Stage::Blit.idx()] > 0);
        ENABLED.store(false, Ordering::Relaxed);
    }

    #[test]
    fn a_frame_line_carries_the_byte_split() {
        let _g = guard();
        let dir = std::env::temp_dir().join("readingbuddy-perf-test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("frames.jsonl");
        let meter = Meter::new();
        let mut rec = Recorder::open(&path, meter.clone()).unwrap();

        rec.frame_begin();
        let mut w = CountingWriter::new(Vec::new(), meter.clone(), ByteClass::Image);
        w.write_all(&[0u8; 128]).unwrap();
        note(FrameNote {
            quality: Quality::Settle,
            cols: 50,
            rows: 26,
            px_w: 348,
            px_h: 344,
            transmitted: true,
            fell_back: false,
        });
        rec.frame_end(FrameCtx {
            mode: "rich",
            moving: false,
            term_cols: 120,
            term_rows: 40,
            draw: Duration::from_micros(900),
            rtt_kitty: Some(Duration::from_micros(1200)),
            rtt_tmux: None,
        });
        rec.flush();
        ENABLED.store(false, Ordering::Relaxed);

        let line = std::fs::read_to_string(&path).unwrap();
        assert!(line.contains("\"bytes_image\":128"), "{line}");
        assert!(line.contains("\"bytes_text\":0"), "{line}");
        assert!(line.contains("\"quality\":\"settle\""), "{line}");
        assert!(line.contains("\"rtt_kitty_us\":1200"), "{line}");
        // An absent sample is omitted rather than logged as a zero, which would
        // read as "the terminal answered instantly".
        assert!(!line.contains("rtt_tmux_us"), "{line}");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn percentiles_take_the_tail() {
        let v: Vec<u64> = (1..=100).collect();
        assert_eq!(percentile(&v, 50.0), 50);
        assert_eq!(percentile(&v, 90.0), 90);
        assert_eq!(percentile(&v, 100.0), 100);
        assert_eq!(percentile(&[], 90.0), 0);
    }
}
