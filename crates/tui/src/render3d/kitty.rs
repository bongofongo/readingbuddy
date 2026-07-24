//! The kitty graphics protocol: getting an RGBA image onto the screen, and off
//! it again.
//!
//! Two halves, deliberately separated:
//!
//! - **Transmission** is escape sequences ([`transmit`], [`delete`]) — zlib'd,
//!   base64'd, chunked, and wrapped in tmux DCS passthrough when needed.
//! - **Placement** is ordinary text ([`placeholder_symbol`], [`id_color`]).
//!
//! That second half is the good part. Rather than telling the terminal *where*
//! to draw (which tmux would immediately invalidate on any redraw), we create a
//! **virtual placement** and then emit `U+10EEEE` cells carrying the row and
//! column in combining diacritics and the image id in the foreground colour.
//! tmux — and ratatui's diff engine, and the layout code — see nothing but
//! text, so scrolling, resizing, pane rotation and cell-level overdraw all work
//! with no special handling at all. The image follows its cells around.
//!
//! Everything the terminal sends back is suppressed with `q=2`, because the app
//! is about to hand the tty to crossterm's `EventStream`: an unsolicited reply
//! would surface as a phantom keypress.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use ratatui::style::Color;

use super::caps::maybe_wrap;
use super::raster::RgbaBuf;

/// The placeholder character. A cell containing this (plus diacritics, plus an
/// id in its foreground colour) shows one cell of the image.
pub const PLACEHOLDER: char = '\u{10EEEE}';

/// Kitty's documented payload ceiling for one escape chunk, in base64 bytes.
const CHUNK: usize = 4096;

/// The row/column diacritics, index = the number they encode.
///
/// Generated from the `rowcolumn-diacritics.txt` that ships with kitty
/// (`Contents/Resources/doc/kitty/html/_downloads/*/`), itself derived from
/// UnicodeData 6.0.0. **Do not hand-edit**: the same discipline as `blit.rs`'s
/// octant table, and for the same reason — a single transposed entry puts one
/// row or column of the image in the wrong place, which is maddening to spot by
/// eye. `diacritic_table_is_intact` guards it.
#[rustfmt::skip]
const DIACRITICS: [char; 297] = [
    '\u{0305}', '\u{030d}', '\u{030e}', '\u{0310}', '\u{0312}', '\u{033d}', '\u{033e}',
    '\u{033f}', '\u{0346}', '\u{034a}', '\u{034b}', '\u{034c}', '\u{0350}', '\u{0351}',
    '\u{0352}', '\u{0357}', '\u{035b}', '\u{0363}', '\u{0364}', '\u{0365}', '\u{0366}',
    '\u{0367}', '\u{0368}', '\u{0369}', '\u{036a}', '\u{036b}', '\u{036c}', '\u{036d}',
    '\u{036e}', '\u{036f}', '\u{0483}', '\u{0484}', '\u{0485}', '\u{0486}', '\u{0487}',
    '\u{0592}', '\u{0593}', '\u{0594}', '\u{0595}', '\u{0597}', '\u{0598}', '\u{0599}',
    '\u{059c}', '\u{059d}', '\u{059e}', '\u{059f}', '\u{05a0}', '\u{05a1}', '\u{05a8}',
    '\u{05a9}', '\u{05ab}', '\u{05ac}', '\u{05af}', '\u{05c4}', '\u{0610}', '\u{0611}',
    '\u{0612}', '\u{0613}', '\u{0614}', '\u{0615}', '\u{0616}', '\u{0617}', '\u{0657}',
    '\u{0658}', '\u{0659}', '\u{065a}', '\u{065b}', '\u{065d}', '\u{065e}', '\u{06d6}',
    '\u{06d7}', '\u{06d8}', '\u{06d9}', '\u{06da}', '\u{06db}', '\u{06dc}', '\u{06df}',
    '\u{06e0}', '\u{06e1}', '\u{06e2}', '\u{06e4}', '\u{06e7}', '\u{06e8}', '\u{06eb}',
    '\u{06ec}', '\u{0730}', '\u{0732}', '\u{0733}', '\u{0735}', '\u{0736}', '\u{073a}',
    '\u{073d}', '\u{073f}', '\u{0740}', '\u{0741}', '\u{0743}', '\u{0745}', '\u{0747}',
    '\u{0749}', '\u{074a}', '\u{07eb}', '\u{07ec}', '\u{07ed}', '\u{07ee}', '\u{07ef}',
    '\u{07f0}', '\u{07f1}', '\u{07f3}', '\u{0816}', '\u{0817}', '\u{0818}', '\u{0819}',
    '\u{081b}', '\u{081c}', '\u{081d}', '\u{081e}', '\u{081f}', '\u{0820}', '\u{0821}',
    '\u{0822}', '\u{0823}', '\u{0825}', '\u{0826}', '\u{0827}', '\u{0829}', '\u{082a}',
    '\u{082b}', '\u{082c}', '\u{082d}', '\u{0951}', '\u{0953}', '\u{0954}', '\u{0f82}',
    '\u{0f83}', '\u{0f86}', '\u{0f87}', '\u{135d}', '\u{135e}', '\u{135f}', '\u{17dd}',
    '\u{193a}', '\u{1a17}', '\u{1a75}', '\u{1a76}', '\u{1a77}', '\u{1a78}', '\u{1a79}',
    '\u{1a7a}', '\u{1a7b}', '\u{1a7c}', '\u{1b6b}', '\u{1b6d}', '\u{1b6e}', '\u{1b6f}',
    '\u{1b70}', '\u{1b71}', '\u{1b72}', '\u{1b73}', '\u{1cd0}', '\u{1cd1}', '\u{1cd2}',
    '\u{1cda}', '\u{1cdb}', '\u{1ce0}', '\u{1dc0}', '\u{1dc1}', '\u{1dc3}', '\u{1dc4}',
    '\u{1dc5}', '\u{1dc6}', '\u{1dc7}', '\u{1dc8}', '\u{1dc9}', '\u{1dcb}', '\u{1dcc}',
    '\u{1dd1}', '\u{1dd2}', '\u{1dd3}', '\u{1dd4}', '\u{1dd5}', '\u{1dd6}', '\u{1dd7}',
    '\u{1dd8}', '\u{1dd9}', '\u{1dda}', '\u{1ddb}', '\u{1ddc}', '\u{1ddd}', '\u{1dde}',
    '\u{1ddf}', '\u{1de0}', '\u{1de1}', '\u{1de2}', '\u{1de3}', '\u{1de4}', '\u{1de5}',
    '\u{1de6}', '\u{1dfe}', '\u{20d0}', '\u{20d1}', '\u{20d4}', '\u{20d5}', '\u{20d6}',
    '\u{20d7}', '\u{20db}', '\u{20dc}', '\u{20e1}', '\u{20e7}', '\u{20e9}', '\u{20f0}',
    '\u{2cef}', '\u{2cf0}', '\u{2cf1}', '\u{2de0}', '\u{2de1}', '\u{2de2}', '\u{2de3}',
    '\u{2de4}', '\u{2de5}', '\u{2de6}', '\u{2de7}', '\u{2de8}', '\u{2de9}', '\u{2dea}',
    '\u{2deb}', '\u{2dec}', '\u{2ded}', '\u{2dee}', '\u{2def}', '\u{2df0}', '\u{2df1}',
    '\u{2df2}', '\u{2df3}', '\u{2df4}', '\u{2df5}', '\u{2df6}', '\u{2df7}', '\u{2df8}',
    '\u{2df9}', '\u{2dfa}', '\u{2dfb}', '\u{2dfc}', '\u{2dfd}', '\u{2dfe}', '\u{2dff}',
    '\u{a66f}', '\u{a67c}', '\u{a67d}', '\u{a6f0}', '\u{a6f1}', '\u{a8e0}', '\u{a8e1}',
    '\u{a8e2}', '\u{a8e3}', '\u{a8e4}', '\u{a8e5}', '\u{a8e6}', '\u{a8e7}', '\u{a8e8}',
    '\u{a8e9}', '\u{a8ea}', '\u{a8eb}', '\u{a8ec}', '\u{a8ed}', '\u{a8ee}', '\u{a8ef}',
    '\u{a8f0}', '\u{a8f1}', '\u{aab0}', '\u{aab2}', '\u{aab3}', '\u{aab7}', '\u{aab8}',
    '\u{aabe}', '\u{aabf}', '\u{aac1}', '\u{fe20}', '\u{fe21}', '\u{fe22}', '\u{fe23}',
    '\u{fe24}', '\u{fe25}', '\u{fe26}', '\u{10a0f}', '\u{10a38}', '\u{1d185}', '\u{1d186}',
    '\u{1d187}', '\u{1d188}', '\u{1d189}', '\u{1d1aa}', '\u{1d1ab}', '\u{1d1ac}', '\u{1d1ad}',
    '\u{1d242}', '\u{1d243}', '\u{1d244}',
];

/// Largest image span the placeholder scheme can address, in cells per axis.
pub const MAX_SPAN: u16 = DIACRITICS.len() as u16;

/// Allocate the process's image id.
///
/// Deliberately below 2^24 and non-zero. Under 2^24 the whole id fits the
/// 24-bit foreground colour, so the third ("most significant byte") diacritic
/// is never needed and every placeholder cell stays two diacritics long. The
/// pid low bits keep two concurrent instances in the same terminal apart.
pub fn image_id() -> u32 {
    0x00B0_0000 | (std::process::id() & 0xFFFF)
}

/// The image id, encoded as a foreground colour for a placeholder cell.
pub fn id_color(id: u32) -> Color {
    Color::Rgb((id >> 16) as u8, (id >> 8) as u8, id as u8)
}

/// The placeholder cell text for `(row, col)` of an image, or `None` when the
/// image spans more cells than the diacritic table can address.
///
/// **Both diacritics are always emitted**, even though kitty lets consecutive
/// cells inherit them from the cell on their left. That shorthand is unusable
/// here: ratatui's `Buffer::diff` emits only the cells that changed, in
/// possibly non-contiguous runs, so "the cell to the left" may never have been
/// written this frame. Spelling out every cell costs four bytes and removes the
/// entire class of bug.
pub fn placeholder_symbol(row: u16, col: u16) -> Option<String> {
    let r = *DIACRITICS.get(row as usize)?;
    let c = *DIACRITICS.get(col as usize)?;
    let mut s = String::with_capacity(12);
    s.push(PLACEHOLDER);
    s.push(r);
    s.push(c);
    Some(s)
}

/// Transmit `img` as image `id` and create a virtual placement spanning
/// `cols` x `rows` cells.
///
/// `a=T` does both in one escape. `U=1` marks the placement virtual (it draws
/// nothing by itself — the placeholder cells do that), `f=32` is straight RGBA,
/// `o=z` zlib, `q=2` silences all responses.
pub fn transmit(img: &RgbaBuf, id: u32, cols: u16, rows: u16, in_tmux: bool) -> String {
    use base64::Engine as _;
    use flate2::{Compression, write::ZlibEncoder};
    use std::io::Write as _;

    // Level 1: a smooth product render compresses several-fold, and the cheap
    // level runs fast enough that the win on the wire dwarfs the CPU cost.
    let mut enc = ZlibEncoder::new(Vec::new(), Compression::new(1));
    let compressed = match enc.write_all(&img.px).and_then(|()| enc.finish()) {
        Ok(v) => v,
        // Compression cannot really fail against a Vec, but a renderer must
        // never take the app down: drop the frame instead.
        Err(_) => return String::new(),
    };
    let payload = base64::engine::general_purpose::STANDARD.encode(&compressed);

    let mut out = String::with_capacity(payload.len() + 256);
    let chunks: Vec<&str> = payload
        .as_bytes()
        .chunks(CHUNK)
        .map(|c| std::str::from_utf8(c).expect("base64 is ASCII"))
        .collect();
    let last = chunks.len().saturating_sub(1);

    for (i, chunk) in chunks.iter().enumerate() {
        let more = u8::from(i < last);
        let escape = if i == 0 {
            format!(
                "\x1b_Ga=T,U=1,i={id},f=32,s={},v={},c={cols},r={rows},o=z,t=d,q=2,m={more};{chunk}\x1b\\",
                img.width, img.height
            )
        } else {
            format!("\x1b_Gm={more};{chunk}\x1b\\")
        };
        // Each chunk is a complete escape wrapped in its own passthrough — an
        // escape must never be split across two DCS wrappers.
        out.push_str(&maybe_wrap(&escape, in_tmux));
    }
    out
}

/// Delete image `id` and every placement of it.
pub fn delete(id: u32, in_tmux: bool) -> String {
    maybe_wrap(&format!("\x1b_Ga=d,d=I,i={id},q=2\x1b\\"), in_tmux)
}

// ---------------------------------------------------------------------------
// Teardown
// ---------------------------------------------------------------------------

/// The image currently on screen, and whether it needs tmux wrapping.
///
/// Process-global and atomic. Global because `restore_terminal` is a free
/// function reached from a `'static` panic hook that cannot borrow `App`;
/// atomic rather than a `Mutex` because that hook must never deadlock on a lock
/// the panicking thread already holds, nor care about poisoning.
static LIVE_IMAGE: AtomicU32 = AtomicU32::new(0);
static LIVE_IN_TMUX: AtomicBool = AtomicBool::new(false);

/// Note that `id` is now on screen and will need deleting.
pub fn register_live(id: u32, in_tmux: bool) {
    LIVE_IN_TMUX.store(in_tmux, Ordering::SeqCst);
    LIVE_IMAGE.store(id, Ordering::SeqCst);
}

/// Forget the live image without emitting anything (the caller already did).
pub fn forget_live() {
    LIVE_IMAGE.store(0, Ordering::SeqCst);
}

/// Delete whatever we left on screen. Idempotent — the swap clears the debt, so
/// calling it from both the panic hook and normal shutdown is safe.
///
/// Must run *before* leaving the alternate screen: the placement lives on that
/// screen and has to be deleted while it is still the current one.
pub fn teardown() {
    let id = LIVE_IMAGE.swap(0, Ordering::SeqCst);
    if id == 0 {
        return;
    }
    let in_tmux = LIVE_IN_TMUX.load(Ordering::SeqCst);
    use std::io::Write as _;
    let mut out = std::io::stdout();
    let _ = out.write_all(delete(id, in_tmux).as_bytes());
    let _ = out.flush();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn img(w: u32, h: u32) -> RgbaBuf {
        let mut b = RgbaBuf::new(w, h);
        // Non-uniform, so a round-trip actually proves something.
        for (i, px) in b.px.iter_mut().enumerate() {
            *px = (i % 251) as u8;
        }
        b
    }

    #[test]
    fn diacritic_table_is_intact() {
        assert_eq!(DIACRITICS.len(), 297);
        // The two the protocol docs spell out by hand.
        assert_eq!(DIACRITICS[0], '\u{0305}');
        assert_eq!(DIACRITICS[1], '\u{030D}');
        assert_eq!(DIACRITICS[2], '\u{030E}');
        // Generated from UnicodeData, so it must be strictly ascending and
        // free of duplicates — a repeat would alias two rows onto each other.
        for pair in DIACRITICS.windows(2) {
            assert!(pair[0] < pair[1], "not ascending at {:?}", pair);
        }
    }

    #[test]
    fn a_placeholder_cell_is_the_marker_plus_two_diacritics() {
        let s = placeholder_symbol(0, 1).expect("in range");
        let chars: Vec<char> = s.chars().collect();
        assert_eq!(chars, vec![PLACEHOLDER, '\u{0305}', '\u{030D}']);
        // The doc's worked example: image 42 at (0,0) and (0,1).
        assert_eq!(placeholder_symbol(0, 0).unwrap().chars().count(), 3);
    }

    #[test]
    fn placeholder_cells_stay_inline_in_a_ratatui_cell() {
        // `Cell` stores its symbol in a CompactString with 24 bytes of inline
        // capacity; spilling to the heap for every cell of every frame would
        // be a real cost. Worst case here is 4 + 4 + 4 bytes.
        let worst = placeholder_symbol(MAX_SPAN - 1, MAX_SPAN - 1).unwrap();
        assert!(worst.len() <= 24, "{} bytes would spill", worst.len());
    }

    #[test]
    fn a_placeholder_cell_measures_exactly_one_column() {
        // Load-bearing, and not obvious: ratatui's `Buffer::diff` skips
        // `symbol().width() - 1` cells after each one it emits. If U+10EEEE
        // ever measured two columns, every second cell of the image would go
        // unwritten and the object would visibly tear. UAX#11 classes Plane-16
        // private use as Ambiguous (width 1 outside CJK context) and combining
        // marks as zero, so this holds — but it holds by assumption unless
        // something checks it.
        use unicode_width::UnicodeWidthStr;
        for (row, col) in [(0u16, 0u16), (3, 7), (MAX_SPAN - 1, MAX_SPAN - 1)] {
            let s = placeholder_symbol(row, col).unwrap();
            assert_eq!(s.width(), 1, "({row},{col}) measured {}", s.width());
        }
    }

    #[test]
    fn spans_past_the_table_are_refused_not_wrapped() {
        // Silently wrapping would put a row of the image in the wrong place;
        // the caller falls back to glyphs instead.
        assert!(placeholder_symbol(MAX_SPAN, 0).is_none());
        assert!(placeholder_symbol(0, MAX_SPAN).is_none());
        assert!(placeholder_symbol(MAX_SPAN - 1, MAX_SPAN - 1).is_some());
    }

    #[test]
    fn the_image_id_fits_a_24_bit_foreground_colour() {
        let id = image_id();
        assert!(
            id != 0 && id < (1 << 24),
            "id {id:#x} needs a third diacritic"
        );
        assert_eq!(id_color(0x00B0_1234), Color::Rgb(0xB0, 0x12, 0x34));
    }

    #[test]
    fn transmit_round_trips_through_zlib_and_base64() {
        use base64::Engine as _;
        use std::io::Read as _;

        let src = img(40, 30);
        let esc = transmit(&src, 7, 10, 5, false);
        // Pull the payload back out of every chunk and reassemble it.
        let mut b64 = String::new();
        for part in esc.split("\x1b_G").skip(1) {
            let body = part.trim_end_matches("\x1b\\");
            let (_, payload) = body.split_once(';').expect("chunk has a payload");
            b64.push_str(payload);
        }
        let deflated = base64::engine::general_purpose::STANDARD
            .decode(b64.as_bytes())
            .expect("valid base64");
        let mut raw = Vec::new();
        flate2::read::ZlibDecoder::new(&deflated[..])
            .read_to_end(&mut raw)
            .expect("valid zlib");
        assert_eq!(raw, src.px, "the pixels did not survive the encoding");
    }

    #[test]
    fn the_first_chunk_carries_the_control_keys_and_the_rest_do_not() {
        let esc = transmit(&img(64, 64), 9, 4, 2, false);
        let first = esc.split("\x1b\\").next().unwrap();
        assert!(first.contains("a=T"), "no transmit-and-place");
        assert!(first.contains("U=1"), "placement must be virtual");
        assert!(first.contains("f=32"), "payload is RGBA");
        assert!(first.contains("o=z"), "payload is zlib");
        assert!(first.contains("q=2"), "responses must be suppressed");
        assert!(first.contains("s=64,v=64"), "pixel dimensions missing");
        assert!(first.contains("c=4,r=2"), "cell span missing");
        // Continuations carry only the continuation flag.
        for part in esc.split("\x1b_G").skip(2) {
            assert!(part.starts_with("m="), "continuation restated control keys");
        }
    }

    #[test]
    fn chunking_marks_every_chunk_but_the_last_as_continued() {
        // Big enough to need several chunks even after compressing.
        let esc = transmit(&img(400, 400), 3, 10, 10, false);
        let chunks: Vec<&str> = esc.split("\x1b_G").skip(1).collect();
        assert!(chunks.len() > 1, "test needs a multi-chunk payload");
        for chunk in &chunks[..chunks.len() - 1] {
            assert!(
                chunk.contains("m=1"),
                "non-final chunk not marked continued"
            );
        }
        assert!(
            chunks.last().unwrap().contains("m=0"),
            "final chunk not marked complete"
        );
        // No chunk may exceed kitty's documented payload limit.
        for chunk in &chunks {
            let body = chunk.trim_end_matches("\x1b\\");
            let payload = body.split_once(';').expect("has payload").1;
            assert!(payload.len() <= CHUNK, "chunk of {} bytes", payload.len());
        }
    }

    #[test]
    fn a_single_chunk_payload_is_marked_complete_immediately() {
        let esc = transmit(&img(2, 2), 5, 1, 1, false);
        assert_eq!(esc.matches("\x1b_G").count(), 1);
        assert!(esc.contains("m=0"));
    }

    #[test]
    fn tmux_wrapping_covers_every_chunk_separately() {
        let esc = transmit(&img(400, 400), 3, 10, 10, true);
        let opens = esc.matches("\x1bPtmux;").count();
        let graphics = esc.matches("\x1b\x1b_G").count();
        assert!(opens > 1, "test needs a multi-chunk payload");
        assert_eq!(
            opens, graphics,
            "each graphics escape needs its own passthrough wrapper"
        );
    }

    #[test]
    fn deletes_target_the_image_and_stay_quiet() {
        assert_eq!(delete(42, false), "\x1b_Ga=d,d=I,i=42,q=2\x1b\\");
        // Teardown runs through tmux too, or the image outlives the app.
        assert!(delete(42, true).starts_with("\x1bPtmux;"));
    }

    #[test]
    fn teardown_is_idempotent() {
        // It runs from both the panic hook and normal shutdown, so a second
        // call must be a no-op rather than a second delete.
        register_live(image_id(), false);
        assert_ne!(LIVE_IMAGE.load(Ordering::SeqCst), 0);
        teardown();
        assert_eq!(LIVE_IMAGE.load(Ordering::SeqCst), 0);
        teardown();
        assert_eq!(LIVE_IMAGE.load(Ordering::SeqCst), 0);
    }
}
