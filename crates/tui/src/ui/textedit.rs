//! A minimal multi-line text editor for composing a short note in-pane — no
//! undo, no selection, no scrolling cleverness. It exists so writing a note
//! never has to hand the terminal to `$EDITOR`. Notes are 2–3 sentences; this
//! only needs the most basic editing.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};

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

    // Keep the cursor's logical line visible; long lines wrap within the box.
    let rows = text_area.height as usize;
    let first = ed.row.saturating_sub(rows.saturating_sub(1));
    let mut lines = Vec::new();
    for (i, line) in ed.lines.iter().enumerate().skip(first) {
        lines.push(cursor_line(line, i == ed.row, ed.col));
    }
    f.render_widget(
        Paragraph::new(lines).wrap(Wrap { trim: false }),
        text_area,
    );

    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(" enter ", theme::key()),
            Span::styled("save  ", theme::dim()),
            Span::styled(" esc ", theme::key()),
            Span::styled("cancel  ", theme::dim()),
            Span::styled(" ⌥/⇧↵ ", theme::key()),
            Span::styled("newline", theme::dim()),
        ])),
        hint,
    );
}

/// One rendered line, with a reversed cell at the cursor when it's the active
/// row (a trailing block when the cursor sits past the last char).
fn cursor_line(line: &str, active: bool, col: usize) -> Line<'static> {
    if !active {
        return Line::from(line.to_string());
    }
    let chars: Vec<char> = line.chars().collect();
    let mut spans = Vec::new();
    for (i, c) in chars.iter().enumerate() {
        if i == col {
            spans.push(Span::styled(c.to_string(), theme::selected()));
        } else {
            spans.push(Span::raw(c.to_string()));
        }
    }
    if col >= chars.len() {
        spans.push(Span::styled(" ", theme::selected()));
    }
    Line::from(spans)
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
