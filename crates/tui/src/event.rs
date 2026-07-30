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
    /// A jump of several rows, stopping at the end of the list rather than
    /// wrapping. `ctrl-u` / `ctrl-d`, and the page keys, which mean the same
    /// thing on a keyboard that has them.
    PageUp,
    PageDown,
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
    /// Cycle the library list's order (library screen).
    CycleSort,
    /// Mark / unmark the selected device row.
    Mark,
    /// Bring the marked device rows across (or every syncable one).
    Sync,
    /// Link the selected device row to a book already in the library.
    Link,
    /// Walk the device again.
    Rescan,
    /// Bring a row in as a **new** book even though it looks like one already
    /// here — the `--new` escape hatch `ko pull`, `goodreads import` and
    /// `calibre import` all carry, as a key.
    ///
    /// Its own action rather than a modifier on [`Action::Select`]: the refusal
    /// is the default and the override has to be a separate, deliberate
    /// keystroke, or the guard is decoration.
    CreateAnyway,
    /// Convert a book between formats through calibre.
    Convert,
    /// Show the current screen's help page.
    ///
    /// Global, and deliberately so: the page is per-screen but the *key* is not,
    /// or the one key a lost user tries would itself have to be looked up.
    Help,
}

/// The key map, with the screen's own bindings applied first.
///
/// [`map_key`] is deliberately screen-agnostic — the screens interpret the
/// directions themselves — but the three import shelves' documented keys (`x`
/// mark, `l` link, `r` rescan, `n` create-anyway, `c` convert) are ones the global
/// map already spends on the book view (export, right, reset, new note,
/// nothing). None of them mean anything on a list of somebody else's books, so
/// those screens claim them rather than the global actions being renamed into
/// something that reads wrong on every screen.
///
/// **Each screen claims its own set, not one shared table.** They overlap by four
/// keys and differ by two, and the differences are the point: `n` is the global
/// `NewNote` and only the two screens with an ambiguous-row escape hatch have
/// anything better to do with it, while `x` stays `Export` on the Goodreads screen
/// because that is the one place a Goodreads CSV is written.
pub fn map_key_on(screen: crate::app::Screen, key: KeyEvent) -> Option<Action> {
    use crate::app::Screen;
    let claimed = match screen {
        Screen::Device => map_device_key(key),
        Screen::Calibre => map_calibre_key(key),
        Screen::Goodreads => map_goodreads_key(key),
        _ => None,
    };
    claimed.or_else(|| map_key(key))
}

/// Is this key a plain letter, i.e. one a screen may claim at all?
///
/// Release events and anything with Control held are the global map's business —
/// `ctrl-c` in particular must never be shadowed by a screen.
fn claimable(key: KeyEvent) -> bool {
    key.kind != KeyEventKind::Release && !key.modifiers.contains(KeyModifiers::CONTROL)
}

/// The device screen's own bindings. Everything it does not claim — `m`, `q`,
/// the arrows, Enter, Esc — falls through to [`map_key`], so the screen is
/// never a dead end.
fn map_device_key(key: KeyEvent) -> Option<Action> {
    if !claimable(key) {
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

/// The calibre shelf's bindings: the device screen's four, plus `n` for the
/// escape hatch and `c` for a conversion.
///
/// Deliberately **not** shared with `map_device_key` as one table, even though
/// four of the six agree: `n` is the global `NewNote`, and a device row is not a
/// thing you write a note on but a shelf where `n` would then mean two things
/// depending on which shelf. The two screens claiming different sets is the
/// honest description of that.
fn map_calibre_key(key: KeyEvent) -> Option<Action> {
    if !claimable(key) {
        return None;
    }
    match key.code {
        KeyCode::Char('x') => Some(Action::Mark),
        KeyCode::Char('s') => Some(Action::Sync),
        KeyCode::Char('l') => Some(Action::Link),
        KeyCode::Char('r') => Some(Action::Rescan),
        KeyCode::Char('n') => Some(Action::CreateAnyway),
        KeyCode::Char('c') => Some(Action::Convert),
        _ => None,
    }
}

/// The Goodreads screen's bindings.
///
/// **`x` is absent on purpose**: it is the global `Export`, and export is exactly
/// what it means here — the one screen from which a Goodreads CSV is written.
/// Claiming it for `Mark` would spend the key on a marking scheme the screen has
/// no use for, since a CSV lands whole or not at all.
fn map_goodreads_key(key: KeyEvent) -> Option<Action> {
    if !claimable(key) {
        return None;
    }
    match key.code {
        KeyCode::Char('s') => Some(Action::Sync),
        KeyCode::Char('l') => Some(Action::Link),
        KeyCode::Char('r') => Some(Action::Rescan),
        KeyCode::Char('n') => Some(Action::CreateAnyway),
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
            // vim's half-page pair. They live in the control map rather than
            // taking two more letters because that is where every user who
            // reaches for them will look, and `d` and `u` are both letters this
            // app would otherwise have to give up (`d` deletes).
            KeyCode::Char('d') => Some(Action::PageDown),
            KeyCode::Char('u') => Some(Action::PageUp),
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
        // The same jump, for a keyboard that has the keys. Not an alias anybody
        // has to learn — it is what these keys already mean everywhere else.
        KeyCode::PageDown => Some(Action::PageDown),
        KeyCode::PageUp => Some(Action::PageUp),
        KeyCode::Char(' ') => Some(Action::ToggleSpin),
        KeyCode::Char('o') => Some(Action::ToggleOptions),
        // `?` used to be a second spelling of `o`, which meant the one key
        // everybody tries when lost expanded a key bar on the single screen
        // that has one and did nothing at all on the other eight.
        KeyCode::Char('?') => Some(Action::Help),
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
        // Global in the map, meaningful on the library screen — the same shape
        // as `a` above, which only the settings screen answers. It is safe to
        // take globally *because* the three import shelves claim `s` for `Sync`
        // in their own maps, which run first: the one meaning of `s` that was
        // already spent is the one this cannot reach.
        KeyCode::Char('s') => Some(Action::CycleSort),
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

    /// `s` cycles the library's order — global in the map, answered only by the
    /// library screen, exactly like `a` below. It is safe to take globally
    /// *because* the three import shelves claim `s` for `Sync` in their own
    /// maps, which run first: the meaning that was already spent is the one
    /// this cannot reach.
    #[test]
    fn s_sorts_the_library_and_still_syncs_the_shelves() {
        use crate::app::Screen;
        assert_eq!(map_key(press(KeyCode::Char('s'))), Some(Action::CycleSort));
        assert_eq!(
            map_key_on(Screen::Library, press(KeyCode::Char('s'))),
            Some(Action::CycleSort)
        );
        for screen in [Screen::Device, Screen::Calibre, Screen::Goodreads] {
            assert_eq!(
                map_key_on(screen, press(KeyCode::Char('s'))),
                Some(Action::Sync),
                "sort took the sync key on {screen:?}"
            );
        }
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
            assert_eq!(
                map_key_on(Screen::Calibre, press(code)),
                map_key(press(code))
            );
            assert_eq!(
                map_key_on(Screen::Goodreads, press(code)),
                map_key(press(code))
            );
        }
    }

    /// The two import shelves claim their own letters, and only on themselves.
    ///
    /// `n` is the interesting one: it is the global `NewNote`, and the calibre and
    /// Goodreads screens spend it on the `--new` escape hatch. The device screen
    /// deliberately does *not*, so this asserts the rebinding is scoped to the two
    /// screens that documented it rather than leaking across every list.
    #[test]
    fn the_import_screens_claim_their_own_letters_only() {
        use crate::app::Screen;
        // calibre: the device's four, plus the escape hatch and a conversion.
        for (code, want) in [
            (KeyCode::Char('x'), Action::Mark),
            (KeyCode::Char('s'), Action::Sync),
            (KeyCode::Char('l'), Action::Link),
            (KeyCode::Char('r'), Action::Rescan),
            (KeyCode::Char('n'), Action::CreateAnyway),
            (KeyCode::Char('c'), Action::Convert),
        ] {
            assert_eq!(map_key_on(Screen::Calibre, press(code)), Some(want));
            assert_eq!(
                map_key_on(Screen::Book, press(code)),
                map_key(press(code)),
                "{code:?} was rebound off the calibre screen"
            );
        }

        // Goodreads: the same minus `x` and `c`.
        for (code, want) in [
            (KeyCode::Char('s'), Action::Sync),
            (KeyCode::Char('l'), Action::Link),
            (KeyCode::Char('r'), Action::Rescan),
            (KeyCode::Char('n'), Action::CreateAnyway),
        ] {
            assert_eq!(map_key_on(Screen::Goodreads, press(code)), Some(want));
        }
    }

    /// **`x` keeps its global meaning on the Goodreads screen**, which is the one
    /// place a Goodreads CSV is written. Claiming it for `Mark` — as the two other
    /// shelves do — would spend the key on a marking scheme a whole-file import has
    /// no use for, and would leave export with no key at all.
    #[test]
    fn x_still_exports_on_the_goodreads_screen() {
        use crate::app::Screen;
        assert_eq!(
            map_key_on(Screen::Goodreads, press(KeyCode::Char('x'))),
            Some(Action::Export)
        );
        // And `n` did not shadow the note key anywhere it means something.
        assert_eq!(
            map_key_on(Screen::Device, press(KeyCode::Char('n'))),
            Some(Action::NewNote)
        );
    }

    /// A screen may not shadow `ctrl-c`. The claim maps run before the global one,
    /// so this is the guard that they never see a modified key at all.
    #[test]
    fn no_screen_can_shadow_ctrl_c() {
        use crate::app::Screen;
        for screen in [Screen::Device, Screen::Calibre, Screen::Goodreads] {
            let ctrl_c = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
            assert_eq!(map_key_on(screen, ctrl_c), Some(Action::Quit));
            // And `s`/`n` with control held reach nothing rather than the shelf.
            for code in [KeyCode::Char('s'), KeyCode::Char('n')] {
                assert_eq!(
                    map_key_on(screen, KeyEvent::new(code, KeyModifiers::CONTROL)),
                    None
                );
            }
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

    /// `?` is the help page on every screen, including the three that claim
    /// their own letters — being lost on an import shelf is exactly when it is
    /// wanted. `o` keeps the key bar it always had.
    #[test]
    fn question_mark_is_help_everywhere_and_o_is_still_the_key_bar() {
        use crate::app::Screen;
        assert_eq!(map_key(press(KeyCode::Char('?'))), Some(Action::Help));
        assert_eq!(
            map_key(press(KeyCode::Char('o'))),
            Some(Action::ToggleOptions)
        );
        for screen in [
            Screen::Home,
            Screen::Menu,
            Screen::Book,
            Screen::Device,
            Screen::Calibre,
            Screen::Goodreads,
        ] {
            assert_eq!(
                map_key_on(screen, press(KeyCode::Char('?'))),
                Some(Action::Help),
                "help was shadowed on {screen:?}"
            );
        }
    }

    #[test]
    fn ctrl_c_quits_and_other_ctrl_keys_are_ignored() {
        let ctrl = |c| KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL);
        assert_eq!(map_key(ctrl('c')), Some(Action::Quit));
        assert_eq!(map_key(ctrl('l')), None);
    }

    /// The half-page pair, and the plain letters it must not have disturbed —
    /// `d` removes a book and `u` means nothing, and both stay that way.
    #[test]
    fn the_page_keys_are_control_only() {
        let ctrl = |c| KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL);
        assert_eq!(map_key(ctrl('d')), Some(Action::PageDown));
        assert_eq!(map_key(ctrl('u')), Some(Action::PageUp));
        assert_eq!(map_key(press(KeyCode::Char('d'))), Some(Action::Delete));
        assert_eq!(map_key(press(KeyCode::Char('u'))), None);
        // The keys a keyboard with them already spells this way.
        assert_eq!(map_key(press(KeyCode::PageDown)), Some(Action::PageDown));
        assert_eq!(map_key(press(KeyCode::PageUp)), Some(Action::PageUp));
    }

    /// A screen may claim letters, never a control key — so the three import
    /// shelves cannot shadow the jump the way they shadow `x` and `r`.
    #[test]
    fn no_screen_shadows_the_page_keys() {
        use crate::app::Screen;
        let ctrl = |c| KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL);
        for screen in [Screen::Device, Screen::Calibre, Screen::Goodreads] {
            assert_eq!(map_key_on(screen, ctrl('d')), Some(Action::PageDown));
            assert_eq!(map_key_on(screen, ctrl('u')), Some(Action::PageUp));
        }
    }

    #[test]
    fn key_release_is_ignored() {
        let mut ev = press(KeyCode::Char('q'));
        ev.kind = KeyEventKind::Release;
        assert_eq!(map_key(ev), None);
    }
}
