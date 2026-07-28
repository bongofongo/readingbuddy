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
    /// Rotate the book-view panes clockwise.
    RotateLayout,
    /// Show or hide the book view's section pane.
    TogglePanel,
    /// Swap the book renderer between true pixels and block glyphs.
    ToggleRenderer,
    /// Slide the pane divider to grow / shrink the object's share.
    GrowBook,
    ShrinkBook,
    /// Step back through the book view's sections (shift-tab; ↑ does it too).
    PrevTab,
    /// Compose a new note.
    NewNote,
    /// Show what links to (and out of) the selected note.
    Links,
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
    /// Enter the Google Books API key (settings screen).
    EditApiKey,
    /// Cycle the ambient background motif (settings screen).
    CycleAmbient,
    /// Mark / unmark the selected device row.
    Mark,
    /// Bring the marked device rows across (or every syncable one).
    Sync,
    /// Link the selected device row to a book already in the library.
    Link,
    /// Walk the device again.
    Rescan,
}

/// The key map, with the screen's own bindings applied first.
///
/// [`map_key`] is deliberately screen-agnostic — the screens interpret the
/// directions themselves — but the device screen's documented keys (`x` mark,
/// `l` link, `r` rescan) are three the global map already spends on the book
/// view (export, right, reset). None of those three mean anything on a device
/// list, so the screen claims them rather than the actions being renamed into
/// something that reads wrong on both screens.
pub fn map_key_on(screen: crate::app::Screen, key: KeyEvent) -> Option<Action> {
    if screen == crate::app::Screen::Device
        && let Some(action) = map_device_key(key)
    {
        return Some(action);
    }
    map_key(key)
}

/// The device screen's own bindings. Everything it does not claim — `m`, `q`,
/// the arrows, Enter, Esc — falls through to [`map_key`], so the screen is
/// never a dead end.
fn map_device_key(key: KeyEvent) -> Option<Action> {
    if key.kind == KeyEventKind::Release || key.modifiers.contains(KeyModifiers::CONTROL) {
        return None;
    }
    match key.code {
        KeyCode::Char('x') => Some(Action::Mark),
        KeyCode::Char('s') => Some(Action::Sync),
        KeyCode::Char('l') => Some(Action::Link),
        KeyCode::Char('r') => Some(Action::Rescan),
        _ => None,
    }
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
    // Shift-L, before the unshifted map: lowercase `l` is the vim right and has
    // a real meaning on every list here, so the links pane takes its capital
    // rather than a letter that stands for nothing. Terminals disagree about
    // whether a shifted letter arrives already upper-cased, so both spellings
    // are matched — the one that is not sent costs a comparison.
    if matches!(key.code, KeyCode::Char('L'))
        || (matches!(key.code, KeyCode::Char('l')) && key.modifiers.contains(KeyModifiers::SHIFT))
    {
        return Some(Action::Links);
    }
    match key.code {
        KeyCode::Char('q') => Some(Action::Quit),
        KeyCode::Char('m') => Some(Action::Menu),
        KeyCode::Esc | KeyCode::Char('b') => Some(Action::Back),
        KeyCode::Up | KeyCode::Char('k') => Some(Action::Up),
        KeyCode::Down | KeyCode::Char('j') => Some(Action::Down),
        KeyCode::Left | KeyCode::Char('h') => Some(Action::Left),
        KeyCode::Right | KeyCode::Char('l') => Some(Action::Right),
        KeyCode::Enter => Some(Action::Select),
        KeyCode::Char(' ') => Some(Action::ToggleSpin),
        KeyCode::Char('o') | KeyCode::Char('?') => Some(Action::ToggleOptions),
        KeyCode::Char('r') => Some(Action::Reset),
        KeyCode::Char('t') => Some(Action::RotateLayout),
        KeyCode::Char('v') => Some(Action::ToggleRenderer),
        KeyCode::Char(']') => Some(Action::GrowBook),
        KeyCode::Char('[') => Some(Action::ShrinkBook),
        KeyCode::F(5) => Some(Action::Refresh),
        // Tab is the section pane's own key: it brings the menu up and takes it
        // away. Stepping through the sections is what ↑ / ↓ are for.
        KeyCode::Tab => Some(Action::TogglePanel),
        KeyCode::BackTab => Some(Action::PrevTab),
        KeyCode::Char('n') => Some(Action::NewNote),
        KeyCode::Char('p') => Some(Action::EditProgress),
        KeyCode::Char('f') => Some(Action::ToggleFinished),
        KeyCode::Char('x') => Some(Action::Export),
        KeyCode::Char('d') => Some(Action::Delete),
        KeyCode::Char('/') => Some(Action::Query),
        KeyCode::Char('g') => Some(Action::EditApiKey),
        KeyCode::Char('a') => Some(Action::CycleAmbient),
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
        assert_eq!(
            map_key(press(KeyCode::Char('j'))),
            map_key(press(KeyCode::Down))
        );
        assert_eq!(
            map_key(press(KeyCode::Char('l'))),
            map_key(press(KeyCode::Right))
        );
    }

    #[test]
    fn b_is_an_alias_for_back() {
        assert_eq!(map_key(press(KeyCode::Char('b'))), Some(Action::Back));
        assert_eq!(
            map_key(press(KeyCode::Char('b'))),
            map_key(press(KeyCode::Esc))
        );
    }

    #[test]
    fn a_cycles_the_ambient_motif() {
        assert_eq!(
            map_key(press(KeyCode::Char('a'))),
            Some(Action::CycleAmbient)
        );
    }

    /// The three keys the device screen takes over, and the guarantee that it
    /// only takes them there.
    #[test]
    fn the_device_screen_rebinds_three_keys_and_nothing_else() {
        use crate::app::Screen;
        for (code, want) in [
            (KeyCode::Char('x'), Action::Mark),
            (KeyCode::Char('s'), Action::Sync),
            (KeyCode::Char('l'), Action::Link),
            (KeyCode::Char('r'), Action::Rescan),
        ] {
            assert_eq!(map_key_on(Screen::Device, press(code)), Some(want));
            assert_eq!(
                map_key_on(Screen::Book, press(code)),
                map_key(press(code)),
                "{code:?} was rebound off the device screen"
            );
        }

        // Everything else still falls through, so `m` and the arrows work and
        // the screen cannot become a dead end.
        for code in [
            KeyCode::Char('m'),
            KeyCode::Char('q'),
            KeyCode::Down,
            KeyCode::Enter,
            KeyCode::Esc,
        ] {
            assert_eq!(
                map_key_on(Screen::Device, press(code)),
                map_key(press(code))
            );
        }
    }

    /// `L` is the links pane, and taking the capital must not have disturbed
    /// the lowercase `l` every list uses to step right.
    #[test]
    fn shift_l_opens_the_links_pane_and_plain_l_still_moves_right() {
        assert_eq!(map_key(press(KeyCode::Char('L'))), Some(Action::Links));
        assert_eq!(
            map_key(KeyEvent::new(KeyCode::Char('l'), KeyModifiers::SHIFT)),
            Some(Action::Links),
            "a terminal that sends shift+l unshifted still means links"
        );
        assert_eq!(map_key(press(KeyCode::Char('l'))), Some(Action::Right));
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
