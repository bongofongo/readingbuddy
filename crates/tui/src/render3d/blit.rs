//! Quadrant-block presentation: **four** subpixels per terminal cell (2 wide,
//! 2 tall), drawn with one of the sixteen block glyphs in U+2580..U+259F.
//!
//! A cell can only hold two colors, so each cell is a two-color quantization
//! of its four subpixels: try all sixteen ways of splitting them, keep the
//! split with the lowest squared error, and paint the winning glyph with the
//! two group means. Where a cell straddles the book's silhouette the split is
//! forced by coverage instead, which is what gives the edges quarter-cell
//! resolution rather than the whole-column stair-steps half-blocks produce.
//!
//! Works everywhere truecolor does — including inside tmux, where the
//! kitty/sixel image protocols do not.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;

use super::math::Vec3;

/// Glyphs indexed by their coverage mask: bit 0 top-left, 1 top-right,
/// 2 bottom-left, 3 bottom-right. Set bits are painted in the foreground.
const GLYPHS: [&str; 16] = [
    " ", "▘", "▝", "▀", "▖", "▌", "▞", "▛", "▗", "▚", "▐", "▜", "▄", "▙", "▟", "█",
];

/// A framebuffer of optional RGB subpixels. `None` means "the book isn't
/// here" and leaves the terminal's own background showing through.
///
/// Dimensions are twice the cell grid on **both** axes.
#[derive(Debug, Clone)]
pub struct RgbBuf {
    pub width: u16,
    pub height: u16,
    px: Vec<Option<Vec3>>,
}

impl RgbBuf {
    pub fn new(width: u16, height: u16) -> RgbBuf {
        RgbBuf {
            width,
            height,
            px: vec![None; width as usize * height as usize],
        }
    }

    pub fn set(&mut self, x: u16, y: u16, color: Option<Vec3>) {
        if x < self.width && y < self.height {
            self.px[y as usize * self.width as usize + x as usize] = color;
        }
    }

    pub fn get(&self, x: u16, y: u16) -> Option<Vec3> {
        if x >= self.width || y >= self.height {
            return None;
        }
        self.px[y as usize * self.width as usize + x as usize]
    }

    /// The four subpixels of cell `(col, row)`, in mask-bit order.
    fn cell(&self, col: u16, row: u16) -> [Option<Vec3>; 4] {
        let (x, y) = (col * 2, row * 2);
        [
            self.get(x, y),
            self.get(x + 1, y),
            self.get(x, y + 1),
            self.get(x + 1, y + 1),
        ]
    }
}

/// What one cell should look like. `None` means the terminal default.
struct CellPlan {
    symbol: &'static str,
    fg: Option<Vec3>,
    bg: Option<Vec3>,
}

fn mean(samples: &[Option<Vec3>; 4], mask: u8) -> Option<Vec3> {
    let mut sum = Vec3::ZERO;
    let mut n = 0.0;
    for (i, s) in samples.iter().enumerate() {
        if mask & (1 << i) != 0
            && let Some(c) = s
        {
            sum = sum + *c;
            n += 1.0;
        }
    }
    (n > 0.0).then(|| sum / n)
}

/// Squared error of representing the masked samples by `center`.
fn error(samples: &[Option<Vec3>; 4], mask: u8, center: Option<Vec3>) -> f32 {
    let Some(center) = center else { return 0.0 };
    let mut err = 0.0;
    for (i, s) in samples.iter().enumerate() {
        if mask & (1 << i) != 0
            && let Some(c) = s
        {
            let d = *c - center;
            err += d.dot(d);
        }
    }
    err
}

fn plan(samples: [Option<Vec3>; 4]) -> CellPlan {
    let covered: u8 = samples
        .iter()
        .enumerate()
        .filter(|(_, s)| s.is_some())
        .map(|(i, _)| 1u8 << i)
        .sum();

    if covered == 0 {
        return CellPlan {
            symbol: " ",
            fg: None,
            bg: None,
        };
    }
    if covered != 0b1111 {
        // Straddling the silhouette: the split is dictated by coverage, and
        // the uncovered quarters show the terminal background.
        return CellPlan {
            symbol: GLYPHS[covered as usize],
            fg: mean(&samples, covered),
            bg: None,
        };
    }

    // Fully covered: pick the two-color split that loses the least detail.
    // Seeded with the solid block so a flat cell — and any tie — stays solid
    // rather than picking an arbitrary partition of identical colors.
    let mut best = (error(&samples, 0b1111, mean(&samples, 0b1111)), 15u8);
    for mask in 1..15u8 {
        let inverse = !mask & 0b1111;
        let err = error(&samples, mask, mean(&samples, mask))
            + error(&samples, inverse, mean(&samples, inverse));
        if err < best.0 {
            best = (err, mask);
        }
    }
    let mask = best.1;
    let inverse = !mask & 0b1111;
    CellPlan {
        symbol: GLYPHS[mask as usize],
        fg: mean(&samples, mask),
        bg: mean(&samples, inverse),
    }
}

fn to_color(c: Option<Vec3>) -> Color {
    match c {
        Some(c) => {
            let ch = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
            Color::Rgb(ch(c.x), ch(c.y), ch(c.z))
        }
        None => Color::Reset,
    }
}

/// Draw `fb` into `area`. The framebuffer is expected to be
/// `area.width * 2` x `area.height * 2`; anything outside is ignored.
pub fn blit(fb: &RgbBuf, area: Rect, buf: &mut Buffer) {
    for row in 0..area.height {
        for col in 0..area.width {
            let p = plan(fb.cell(col, row));
            buf[(area.x + col, area.y + row)]
                .set_symbol(p.symbol)
                .set_fg(to_color(p.fg))
                .set_bg(to_color(p.bg));
        }
    }
}

/// Render the framebuffer to plain text with ANSI truecolor escapes — the
/// `--dump-frame` path, which needs no terminal setup at all.
pub fn to_ansi(fb: &RgbBuf) -> String {
    let mut out = String::new();
    for row in 0..fb.height / 2 {
        for col in 0..fb.width / 2 {
            let p = plan(fb.cell(col, row));
            match p.fg {
                Some(c) => out.push_str(&format!(
                    "\x1b[38;2;{};{};{}m",
                    byte(c.x),
                    byte(c.y),
                    byte(c.z)
                )),
                None => out.push_str("\x1b[39m"),
            }
            match p.bg {
                Some(c) => out.push_str(&format!(
                    "\x1b[48;2;{};{};{}m",
                    byte(c.x),
                    byte(c.y),
                    byte(c.z)
                )),
                None => out.push_str("\x1b[49m"),
            }
            out.push_str(p.symbol);
        }
        out.push_str("\x1b[0m\n");
    }
    out
}

fn byte(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// Write a PNG of what the terminal will actually show: every cell is planned
/// exactly as [`blit`] would, then painted as its 2x2 quarters. Subpixels are
/// half as wide as they are tall, so each is drawn 1x2 to keep the proportions
/// honest — a cell is 2x4 image pixels.
///
/// This is the development view. Rendering the raw framebuffer instead would
/// flatter the result, hiding exactly the glyph quantization worth judging.
pub fn to_png(fb: &RgbBuf, path: &std::path::Path) -> std::io::Result<()> {
    let (cols, rows) = (fb.width / 2, fb.height / 2);
    let img = image::RgbImage::from_fn(cols as u32 * 2, rows as u32 * 4, |x, y| {
        let (col, row) = ((x / 2) as u16, (y / 4) as u16);
        let quarter = (x % 2) + (y / 2) % 2 * 2;
        let p = plan(fb.cell(col, row));
        let lit = (1u8 << quarter) & mask_of(p.symbol) != 0;
        match if lit { p.fg } else { p.bg } {
            Some(c) => image::Rgb([byte(c.x), byte(c.y), byte(c.z)]),
            // Checkerboard for "no book here", so the silhouette is obvious.
            None => {
                let v = if (x / 8 + y / 16) % 2 == 0 { 60 } else { 75 };
                image::Rgb([v, v, v])
            }
        }
    });
    img.save(path)
        .map_err(|e| std::io::Error::other(e.to_string()))
}

fn mask_of(symbol: &str) -> u8 {
    GLYPHS.iter().position(|g| *g == symbol).unwrap_or(0) as u8
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render3d::math::vec3;

    const RED: Vec3 = vec3(1.0, 0.0, 0.0);
    const BLUE: Vec3 = vec3(0.0, 0.0, 1.0);

    fn planned(samples: [Option<Vec3>; 4]) -> (&'static str, Color, Color) {
        let p = plan(samples);
        (p.symbol, to_color(p.fg), to_color(p.bg))
    }

    #[test]
    fn an_empty_cell_leaves_the_terminal_alone() {
        assert_eq!(planned([None; 4]), (" ", Color::Reset, Color::Reset));
    }

    #[test]
    fn coverage_dictates_the_glyph_at_the_silhouette_edge() {
        // Only the top-left quarter is on the book.
        let (sym, fg, bg) = planned([Some(RED), None, None, None]);
        assert_eq!(sym, "▘");
        assert_eq!(fg, Color::Rgb(255, 0, 0));
        assert_eq!(bg, Color::Reset);

        // The left half.
        let (sym, ..) = planned([Some(RED), None, Some(RED), None]);
        assert_eq!(sym, "▌");
        // Three quarters, the bottom-right missing.
        let (sym, ..) = planned([Some(RED), Some(RED), Some(RED), None]);
        assert_eq!(sym, "▛");
    }

    #[test]
    fn a_full_cell_splits_where_the_colors_actually_differ() {
        // Red on top, blue underneath: the upper half block is exact.
        let (sym, fg, bg) = planned([Some(RED), Some(RED), Some(BLUE), Some(BLUE)]);
        assert_eq!(sym, "▀");
        assert_eq!(fg, Color::Rgb(255, 0, 0));
        assert_eq!(bg, Color::Rgb(0, 0, 255));

        // Red left, blue right — a split half-blocks alone cannot express.
        let (sym, fg, bg) = planned([Some(RED), Some(BLUE), Some(RED), Some(BLUE)]);
        assert_eq!(sym, "▌");
        assert_eq!(fg, Color::Rgb(255, 0, 0));
        assert_eq!(bg, Color::Rgb(0, 0, 255));

        // A single odd quarter gets its own corner glyph.
        let (sym, ..) = planned([Some(RED), Some(BLUE), Some(BLUE), Some(BLUE)]);
        assert_eq!(sym, "▘");
    }

    #[test]
    fn a_flat_cell_becomes_a_solid_block() {
        let (sym, fg, _) = planned([Some(RED); 4]);
        assert_eq!(sym, "█");
        assert_eq!(fg, Color::Rgb(255, 0, 0));
    }

    #[test]
    fn blit_maps_four_subpixels_onto_one_cell() {
        let mut fb = RgbBuf::new(4, 2);
        fb.set(0, 0, Some(RED));
        fb.set(1, 0, Some(RED));
        fb.set(0, 1, Some(BLUE));
        fb.set(1, 1, Some(BLUE));
        let area = Rect::new(0, 0, 2, 1);
        let mut buf = Buffer::empty(area);
        blit(&fb, area, &mut buf);

        assert_eq!(buf[(0, 0)].symbol(), "▀");
        assert_eq!(buf[(0, 0)].fg, Color::Rgb(255, 0, 0));
        assert_eq!(buf[(0, 0)].bg, Color::Rgb(0, 0, 255));
        assert_eq!(buf[(1, 0)].symbol(), " ");
    }

    #[test]
    fn ansi_dump_emits_one_line_per_cell_row() {
        let mut fb = RgbBuf::new(6, 4);
        fb.set(1, 1, Some(vec3(0.5, 0.5, 0.5)));
        let text = to_ansi(&fb);
        assert_eq!(text.lines().count(), 2);
        assert!(text.contains("128"));
    }
}
