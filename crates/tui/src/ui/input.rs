//! A minimal single-line text input, used for the search query, an ISBN, a
//! progress page, a note's location, and a KOReader path. The multi-line note
//! *body* is composed in `$EDITOR` (see `crate::editor`), not here.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::theme;

/// Editable line + a char-indexed cursor.
#[derive(Debug, Default, Clone)]
pub struct InputState {
    pub value: String,
    /// Cursor position as a char index in `[0, char_count]`.
    cursor: usize,
}

impl InputState {
    pub fn new(initial: &str) -> Self {
        InputState {
            value: initial.to_string(),
            cursor: initial.chars().count(),
        }
    }

    fn len(&self) -> usize {
        self.value.chars().count()
    }

    /// Byte offset of char `idx` (or end-of-string).
    fn byte_at(&self, idx: usize) -> usize {
        self.value
            .char_indices()
            .nth(idx)
            .map(|(b, _)| b)
            .unwrap_or(self.value.len())
    }

    pub fn insert(&mut self, c: char) {
        let b = self.byte_at(self.cursor);
        self.value.insert(b, c);
        self.cursor += 1;
    }

    pub fn backspace(&mut self) {
        if self.cursor > 0 {
            let b = self.byte_at(self.cursor - 1);
            self.value.remove(b);
            self.cursor -= 1;
        }
    }

    pub fn left(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    pub fn right(&mut self) {
        if self.cursor < self.len() {
            self.cursor += 1;
        }
    }

    /// Take the value out, leaving the state empty.
    pub fn take(&mut self) -> String {
        self.cursor = 0;
        std::mem::take(&mut self.value)
    }
}

/// Draw a bordered input box titled `prompt`, with a block cursor.
pub fn render(f: &mut Frame, area: Rect, prompt: &str, state: &InputState) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme::accent())
        .title(Span::styled(format!(" {prompt} "), theme::title()));
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }

    // Value with a reversed cell where the cursor sits (or a trailing block at
    // the end). Kept on one line; the box is short-lived so no scrolling.
    let chars: Vec<char> = state.value.chars().collect();
    let mut spans = Vec::new();
    for (i, c) in chars.iter().enumerate() {
        if i == state.cursor {
            spans.push(Span::styled(c.to_string(), theme::selected()));
        } else {
            spans.push(Span::raw(c.to_string()));
        }
    }
    if state.cursor >= chars.len() {
        spans.push(Span::styled(" ", theme::selected()));
    }
    f.render_widget(Paragraph::new(Line::from(spans)), inner);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edits_track_the_cursor() {
        let mut s = InputState::new("");
        for c in "abc".chars() {
            s.insert(c);
        }
        assert_eq!(s.value, "abc");
        s.left();
        s.insert('X');
        assert_eq!(s.value, "abXc");
        s.backspace();
        assert_eq!(s.value, "abc");
        assert_eq!(s.take(), "abc");
        assert_eq!(s.value, "");
    }

    #[test]
    fn cursor_never_escapes_bounds() {
        let mut s = InputState::new("hi");
        s.right();
        s.right();
        s.right();
        s.insert('!');
        assert_eq!(s.value, "hi!");
        s.left();
        s.left();
        s.left();
        s.left();
        s.backspace(); // at start, no-op
        assert_eq!(s.value, "hi!");
    }
}
