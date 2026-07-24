//! Key → [`Action`] mapping. Screens interpret the directions themselves, so
//! the same keys navigate a list and rotate the book.

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Quit,
    /// Back to the menu from anywhere.
    Menu,
    /// One step back in the screen stack.
    Back,
    Up,
    Down,
    Left,
    Right,
    Select,
    ToggleSpin,
    ToggleOptions,
    Reset,
    Refresh,
    /// Cycle book-view tabs.
    NextTab,
    PrevTab,
    /// Compose a new note.
    NewNote,
    /// Update the reading page.
    EditProgress,
    /// Toggle the finished flag.
    ToggleFinished,
    /// Export flashcards.
    Export,
    /// Remove the selected library book.
    Delete,
    /// Open / reopen a search query.
    Query,
}

pub fn map_key(key: KeyEvent) -> Option<Action> {
    // Windows delivers press *and* release; only act on press.
    if key.kind == KeyEventKind::Release {
        return None;
    }
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        return match key.code {
            KeyCode::Char('c') => Some(Action::Quit),
            _ => None,
        };
    }
    match key.code {
        KeyCode::Char('q') => Some(Action::Quit),
        KeyCode::Char('m') => Some(Action::Menu),
        KeyCode::Esc => Some(Action::Back),
        KeyCode::Up | KeyCode::Char('k') => Some(Action::Up),
        KeyCode::Down | KeyCode::Char('j') => Some(Action::Down),
        KeyCode::Left | KeyCode::Char('h') => Some(Action::Left),
        KeyCode::Right | KeyCode::Char('l') => Some(Action::Right),
        KeyCode::Enter => Some(Action::Select),
        KeyCode::Char(' ') => Some(Action::ToggleSpin),
        KeyCode::Char('o') | KeyCode::Char('?') => Some(Action::ToggleOptions),
        KeyCode::Char('r') => Some(Action::Reset),
        KeyCode::F(5) => Some(Action::Refresh),
        KeyCode::Tab => Some(Action::NextTab),
        KeyCode::BackTab => Some(Action::PrevTab),
        KeyCode::Char('n') => Some(Action::NewNote),
        KeyCode::Char('p') => Some(Action::EditProgress),
        KeyCode::Char('f') => Some(Action::ToggleFinished),
        KeyCode::Char('x') => Some(Action::Export),
        KeyCode::Char('d') => Some(Action::Delete),
        KeyCode::Char('/') => Some(Action::Query),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn vim_keys_and_arrows_agree() {
        assert_eq!(map_key(press(KeyCode::Char('j'))), map_key(press(KeyCode::Down)));
        assert_eq!(map_key(press(KeyCode::Char('l'))), map_key(press(KeyCode::Right)));
    }

    #[test]
    fn ctrl_c_quits_and_other_ctrl_keys_are_ignored() {
        let ctrl = |c| KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL);
        assert_eq!(map_key(ctrl('c')), Some(Action::Quit));
        assert_eq!(map_key(ctrl('l')), None);
    }

    #[test]
    fn key_release_is_ignored() {
        let mut ev = press(KeyCode::Char('q'));
        ev.kind = KeyEventKind::Release;
        assert_eq!(map_key(ev), None);
    }
}
