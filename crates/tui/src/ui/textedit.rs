//! A minimal multi-line text editor for composing a short note in-pane — no
//! undo, no selection, no scrolling cleverness. It exists so writing a note
//! never has to hand the terminal to `$EDITOR`. Notes are 2–3 sentences; this
//! only needs the most basic editing.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use crate::theme;

/// Lines of text plus a char-indexed cursor at `(row, col)`.
#[derive(Debug, Clone)]
pub struct TextEditor {
    lines: Vec<String>,
    row: usize,
    col: usize,
}

impl TextEditor {
    pub fn new(initial: &str) -> Self {
        let mut lines: Vec<String> = initial.split('\n').map(str::to_string).collect();
        if lines.is_empty() {
            lines.push(String::new());
        }
        let row = lines.len() - 1;
        let col = lines[row].chars().count();
        TextEditor { lines, row, col }
    }

    pub fn text(&self) -> String {
        self.lines.join("\n")
    }

    pub fn is_blank(&self) -> bool {
        self.lines.iter().all(|l| l.trim().is_empty())
    }

    fn line_len(&self, row: usize) -> usize {
        self.lines[row].chars().count()
    }

    /// Byte offset of char `col` within `row` (or end-of-line).
    fn byte_at(&self, row: usize, col: usize) -> usize {
        self.lines[row]
            .char_indices()
            .nth(col)
            .map(|(b, _)| b)
            .unwrap_or(self.lines[row].len())
    }

    pub fn insert(&mut self, c: char) {
        let b = self.byte_at(self.row, self.col);
        self.lines[self.row].insert(b, c);
        self.col += 1;
    }

    pub fn newline(&mut self) {
        let b = self.byte_at(self.row, self.col);
        let tail = self.lines[self.row].split_off(b);
        self.lines.insert(self.row + 1, tail);
        self.row += 1;
        self.col = 0;
    }

    pub fn backspace(&mut self) {
        if self.col > 0 {
            let b = self.byte_at(self.row, self.col - 1);
            self.lines[self.row].remove(b);
            self.col -= 1;
        } else if self.row > 0 {
            // Join with the previous line, cursor landing at the seam.
            let cur = self.lines.remove(self.row);
            self.row -= 1;
            self.col = self.line_len(self.row);
            self.lines[self.row].push_str(&cur);
        }
    }

    pub fn left(&mut self) {
        if self.col > 0 {
            self.col -= 1;
        } else if self.row > 0 {
            self.row -= 1;
            self.col = self.line_len(self.row);
        }
    }

    pub fn right(&mut self) {
        if self.col < self.line_len(self.row) {
            self.col += 1;
        } else if self.row + 1 < self.lines.len() {
            self.row += 1;
            self.col = 0;
        }
    }

    pub fn up(&mut self) {
        if self.row > 0 {
            self.row -= 1;
            self.col = self.col.min(self.line_len(self.row));
        }
    }

    pub fn down(&mut self) {
        if self.row + 1 < self.lines.len() {
            self.row += 1;
            self.col = self.col.min(self.line_len(self.row));
        }
    }
}

/// Draw the editor as a bordered box with a cursor and a hint line.
pub fn render(f: &mut Frame, area: Rect, title: &str, ed: &TextEditor) {
    f.render_widget(Clear, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme::accent())
        .title(Span::styled(format!(" {title} "), theme::title()));
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }

    let [text_area, hint] =
        Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(inner);

    // Wrap manually rather than via ratatui's `Wrap`: a whitespace-only line
    // (an empty line carries just the reversed cursor cell) makes the word
    // wrapper emit a phantom blank row, floating the cursor one line below the
    // text. Doing it ourselves also pins the cursor's visual row exactly.
    let lines = wrapped_lines(ed, text_area.width as usize, text_area.height as usize);
    f.render_widget(Paragraph::new(lines), text_area);

    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(" enter ", theme::key()),
            Span::styled("save  ", theme::dim()),
            Span::styled(" esc ", theme::key()),
            Span::styled("cancel  ", theme::dim()),
            Span::styled(" ⌥/⇧↵ ", theme::key()),
            Span::styled("newline  ", theme::dim()),
            Span::styled(" ⇥ ", theme::key()),
            Span::styled("tab", theme::dim()),
        ])),
        hint,
    );
}

/// How many spaces a `\t` occupies on screen. The buffer keeps the real tab;
/// only the rendering expands it. A literal `\t` written into the cell buffer
/// would make the terminal jump to its own tab stop, desyncing ratatui's diff
/// and smearing text across the pane.
const TAB_WIDTH: usize = 4;

/// A logical line as display cells: tabs expanded to spaces, so one cell = one
/// column on screen.
fn display_cells(line: &str) -> Vec<char> {
    let mut cells = Vec::new();
    for c in line.chars() {
        if c == '\t' {
            cells.extend(std::iter::repeat_n(' ', TAB_WIDTH));
        } else {
            cells.push(c);
        }
    }
    cells
}

/// The display column of char `col` within `line` (tabs counting as `TAB_WIDTH`);
/// clamps to the line's full display width when `col` is past the end.
fn display_col(line: &str, col: usize) -> usize {
    line.chars()
        .take(col)
        .map(|c| if c == '\t' { TAB_WIDTH } else { 1 })
        .sum()
}

/// Lay the editor out as visual rows, character-wrapping each logical line to
/// `width` and scrolling vertically so the cursor's row stays inside `height`.
/// The cursor is a reversed cell placed at its exact display column (a trailing
/// block when it sits past the last character), wrapping to the next row when
/// it reaches the edge.
fn wrapped_lines(ed: &TextEditor, width: usize, height: usize) -> Vec<Line<'static>> {
    let width = width.max(1);
    let mut visual: Vec<Line<'static>> = Vec::new();
    let mut cursor_row = 0usize;

    for (r, line) in ed.lines.iter().enumerate() {
        let cells = display_cells(line);
        let active = r == ed.row;
        let cur = if active { display_col(line, ed.col) } else { 0 };
        // Cover the text and, when the cursor sits past it, its trailing cell.
        let extent = if active {
            cells.len().max(cur + 1)
        } else {
            cells.len()
        };
        let sub_rows = extent.div_ceil(width).max(1);
        for s in 0..sub_rows {
            let start = s * width;
            let end = (start + width).min(cells.len());
            let mut spans = Vec::new();
            for (i, &ch) in cells.iter().enumerate().take(end).skip(start) {
                if active && i == cur {
                    spans.push(Span::styled(ch.to_string(), theme::selected()));
                } else {
                    spans.push(Span::raw(ch.to_string()));
                }
            }
            // Cursor beyond the last character: pad to it, then the block.
            if active && cur >= cells.len() && (start..start + width).contains(&cur) {
                for _ in end..cur {
                    spans.push(Span::raw(" ".to_string()));
                }
                spans.push(Span::styled(" ".to_string(), theme::selected()));
            }
            if active && (start..start + width).contains(&cur) {
                cursor_row = visual.len();
            }
            visual.push(Line::from(spans));
        }
    }

    let first = cursor_row.saturating_sub(height.saturating_sub(1));
    visual.into_iter().skip(first).take(height.max(1)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn types_and_deletes_across_lines() {
        let mut ed = TextEditor::new("");
        for c in "hello".chars() {
            ed.insert(c);
        }
        ed.newline();
        for c in "world".chars() {
            ed.insert(c);
        }
        assert_eq!(ed.text(), "hello\nworld");
        assert!(!ed.is_blank());

        // Backspace at column 0 joins the two lines.
        ed.left();
        ed.left();
        ed.left();
        ed.left();
        ed.left(); // now at start of "world"
        assert_eq!((ed.row, ed.col), (1, 0));
        ed.backspace();
        assert_eq!(ed.text(), "helloworld");
        assert_eq!((ed.row, ed.col), (0, 5));
    }

    /// Flatten a rendered line back to its text, marking the reversed cursor
    /// cell with `#`.
    fn row_text(line: &Line) -> String {
        line.spans
            .iter()
            .map(|s| {
                if s.style.add_modifier.contains(ratatui::style::Modifier::REVERSED) {
                    "#".repeat(s.content.chars().count())
                } else {
                    s.content.to_string()
                }
            })
            .collect()
    }

    #[test]
    fn a_tab_renders_as_spaces_never_a_raw_tab() {
        // The buffer keeps the real tab...
        let mut ed = TextEditor::new("");
        ed.insert('a');
        ed.insert('\t');
        ed.insert('b');
        assert_eq!(ed.text(), "a\tb");

        // ...but the rendered line must not carry one, or the terminal desyncs.
        let rows = wrapped_lines(&ed, 40, 6);
        let rendered = row_text(&rows[0]);
        assert!(!rendered.contains('\t'), "raw tab leaked into a rendered line");
        assert!(rendered.starts_with("a    b"), "tab did not expand: {rendered:?}");
    }

    #[test]
    fn the_cursor_sits_on_the_text_row_not_below_it() {
        // An empty trailing line: the cursor block must be the row right after
        // the last text, with no phantom blank between them.
        let ed = TextEditor::new("one\ntwo\n");
        let rows = wrapped_lines(&ed, 30, 6);
        assert_eq!(row_text(&rows[0]), "one");
        assert_eq!(row_text(&rows[1]), "two");
        assert_eq!(row_text(&rows[2]), "#", "cursor floated off its row");

        // A fresh, wholly empty note: the cursor is on the very first row.
        let empty = TextEditor::new("");
        let rows = wrapped_lines(&empty, 30, 6);
        assert_eq!(row_text(&rows[0]), "#");

        // Typing onto that line keeps the cursor on the same row.
        let typed = TextEditor::new("one\ntwo\ns");
        let rows = wrapped_lines(&typed, 30, 6);
        assert_eq!(row_text(&rows[2]), "s#");
    }

    #[test]
    fn a_long_line_wraps_and_carries_the_cursor() {
        let ed = TextEditor::new("abcdefghij"); // 10 chars, width 4
        let rows = wrapped_lines(&ed, 4, 6);
        assert_eq!(row_text(&rows[0]), "abcd");
        assert_eq!(row_text(&rows[1]), "efgh");
        assert_eq!(row_text(&rows[2]), "ij#"); // cursor past the end, wrapped
    }

    #[test]
    fn cursor_moves_and_clamps() {
        let mut ed = TextEditor::new("ab\ncdef");
        // starts at end of last line
        assert_eq!((ed.row, ed.col), (1, 4));
        ed.up();
        // col clamps to the shorter line
        assert_eq!((ed.row, ed.col), (0, 2));
        ed.down();
        assert_eq!((ed.row, ed.col), (1, 2));

        let mut blank = TextEditor::new("   \n\t");
        assert!(blank.is_blank());
        blank.insert('x');
        assert!(!blank.is_blank());
    }
}
