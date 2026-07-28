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
    /// Open this reading's reflection: private, accretes, the hub of the graph.
    Reflect,
    /// Open this reading's review: public prose, and the one note kind that
    /// carries a rating.
    Review,
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
        // The reflection and the review, and **not** on `r` / `v`.
        //
        // Those are the mnemonic letters and both are already spent — `r`
        // resets the pose, `v` swaps the renderer — on the one screen where the
        // book actually turns, so renaming either would read wrong exactly
        // where it currently reads right. `map_key_on` could scope a rebinding
        // to a screen (it is what the device shelf does with `x`/`l`/`r`), but
        // both the home screen *and* the book view need this pair, and an
        // override map two screens install identically is the global map with
        // extra steps. So the pair takes two unspent letters: `w` for the
        // review one writes for other people, `e` for the reflection beside it.
        KeyCode::Char('e') => Some(Action::Reflect),
        KeyCode::Char('w') => Some(Action::Review),
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

    /// The reflection/review pair is global, and the keys it would rather have
    /// still mean what they meant.
    #[test]
    fn the_reflection_pair_takes_unspent_keys() {
        assert_eq!(map_key(press(KeyCode::Char('e'))), Some(Action::Reflect));
        assert_eq!(map_key(press(KeyCode::Char('w'))), Some(Action::Review));
        assert_eq!(map_key(press(KeyCode::Char('r'))), Some(Action::Reset));
        assert_eq!(
            map_key(press(KeyCode::Char('v'))),
            Some(Action::ToggleRenderer)
        );

        // Global, so the home screen and the book view get the same pair — and
        // the device screen's override map does not shadow it either.
        use crate::app::Screen;
        for screen in [Screen::Home, Screen::Book, Screen::Device] {
            assert_eq!(
                map_key_on(screen, press(KeyCode::Char('e'))),
                Some(Action::Reflect),
                "reflect was rebound on {screen:?}"
            );
        }
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
