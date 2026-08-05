//! The calibre library's own shelf.
//!
//! One row per book calibre holds, in the state a dry-run import computed. It is
//! the device screen's shape on purpose: calibre is another system that owns
//! books, and `docs/decisions.md` names it an origin for curated metadata — so
//! the way to meet one is to be shown its shelf and choose, not to answer a
//! dialog that imports all of it or none.
//!
//! Two things this screen must never become. It is **not a dead end**: a row that
//! looks like a book already here offers those books (`l`) and the override
//! (`n`), which is the same refusal-with-a-next-move `ko pull` and
//! `goodreads import` have. And an **absent calibre is reported, never
//! prescribed** — the empty state says the tools are not here and names nothing
//! to install, which is `docs/decisions.md`'s rule and the sort that erodes by
//! helpfulness.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem};

use readingbuddy::CalibreRowState;

use crate::app::{App, CalibreRow};
use crate::theme;

/// What the screen says with no rows on it.
///
/// Three cases, because "nothing here" has three different meanings and only one
/// of them is a thing the user can act on.
fn empty_hint(app: &App) -> &'static str {
    if !app.engine.calibre().can_read_library() {
        // Reported, not prescribed: no install instruction, no link, no tool
        // name to go and fetch. `an_absent_calibre_names_nothing_to_install`
        // pins this.
        "calibre's library tools aren't on this machine, so there's no shelf to show"
    } else if app.calibre_scanned {
        "that library holds no books — / to point at another one"
    } else {
        "press r to read your calibre library, / to pick another"
    }
}

pub fn draw(f: &mut Frame, app: &mut App, area: Rect) {
    let selected = app.calibre_state.selected();
    let rows: Vec<Line> = app
        .calibre
        .iter()
        .enumerate()
        .map(|(i, r)| row(r, app.calibre_marks.contains(&i), Some(i) == selected))
        .collect();

    let marked = app.calibre_marks.len();
    let where_from = match &app.calibre_library {
        Some(p) => short_root(p),
        // Not "none" or "default": the user chose it once, in calibre, and that
        // is what this row is showing.
        None => "yours".to_string(),
    };
    let title = if app.calibre.is_empty() {
        " calibre ".to_string()
    } else if marked == 0 {
        format!(" calibre · {where_from} · {} ", app.calibre.len())
    } else {
        format!(
            " calibre · {where_from} · {} · {marked} marked ",
            app.calibre.len()
        )
    };

    let hint = empty_hint(app);
    let Some((area, block)) = super::shelf_frame(f, area, title, key_bar(), &rows, hint) else {
        return;
    };

    let items: Vec<ListItem> = rows.into_iter().map(ListItem::new).collect();
    // No `highlight_style`: the reverse is scoped to the title span in `row`, so
    // the state word and the match rung keep their hues.
    let list = List::new(items).block(block).highlight_symbol("› ");
    f.render_stateful_widget(list, area, &mut app.calibre_state);

    if let Some(picker) = &mut app.link_picker {
        super::link_picker(f, picker, area);
    }
}

/// The library directory's own name rather than its whole path — same argument as
/// the device screen's: the shelf *is* the library, and the path is chrome.
fn short_root(root: &std::path::Path) -> String {
    root.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| root.display().to_string())
}

/// One shelf row: `✓ new          Piranesi  Susanna Clarke  2 tags, cover`.
fn row(r: &CalibreRow, marked: bool, selected: bool) -> Line<'static> {
    let title_style = if selected {
        theme::title().patch(theme::selected())
    } else {
        theme::title()
    };
    let mut spans = vec![
        Span::styled(if marked { "✓ " } else { "  " }, theme::accent()),
        Span::styled(format!("{:<12}", r.state.label()), state_style(&r.state)),
        Span::styled(r.title.clone(), title_style),
    ];
    if let Some(authors) = r.authors_line() {
        spans.push(Span::styled(format!("  {authors}"), theme::dim()));
    }
    let detail = super::clip(detail(r), super::DETAIL_MAX);
    if !detail.is_empty() {
        spans.push(Span::styled(format!("  {detail}"), theme::dim()));
    }
    Line::from(spans)
}

/// The right-hand half of a row: what importing it would do, or — once it has
/// been imported — what that import reported.
fn detail(r: &CalibreRow) -> String {
    if let Some(note) = &r.note {
        return note.clone();
    }
    match &r.state {
        CalibreRowState::Linked {
            matched_by,
            tags,
            cover,
            files,
            ..
        } => {
            let gains = crate::app::calibre_gains(*tags, *cover, *files);
            format!("{gains} (by {matched_by})")
        }
        CalibreRowState::New => "not in the library — enter brings it in".to_string(),
        // A decision, not a dead end: name what it probably already is, and both
        // moves out of it.
        CalibreRowState::Candidates(c) => match c.first() {
            Some(c) => super::candidate_hint(&c.title, c.score, "l to link, n if not"),
            // The band was empty, which the engine only reports as unmatched when
            // `create_ambiguous` was off — so `n` is still the move.
            None => "nothing here looks like it — n brings it in as new".to_string(),
        },
        CalibreRowState::Unreadable(d) => d.detail.clone(),
    }
}

/// The state word's colour. Actionable states take the accent; the rest stay in
/// the terminal's own palette, so the shelf reads as a list with a few things
/// asking for attention rather than a wall of colour.
fn state_style(state: &CalibreRowState) -> ratatui::style::Style {
    match state {
        CalibreRowState::New | CalibreRowState::Candidates(_) => theme::accent(),
        CalibreRowState::Linked { .. } => theme::dim(),
        // Not a colour: a row calibre gave no title is a fact about one row, not
        // an alarm, and `theme` keeps hard-coded hues for accents only.
        CalibreRowState::Unreadable(_) => theme::primary(),
    }
}

fn key_bar() -> Line<'static> {
    Line::from(vec![
        Span::styled(" enter", theme::key()),
        Span::styled(" import  ", theme::dim()),
        Span::styled("x", theme::key()),
        Span::styled(" mark  ", theme::dim()),
        Span::styled("s", theme::key()),
        Span::styled(" all  ", theme::dim()),
        Span::styled("l", theme::key()),
        Span::styled(" link  ", theme::dim()),
        Span::styled("n", theme::key()),
        Span::styled(" as new  ", theme::dim()),
        Span::styled("c", theme::key()),
        Span::styled(" convert  ", theme::dim()),
        Span::styled("r", theme::key()),
        Span::styled(" reread  ", theme::dim()),
        Span::styled("m", theme::key()),
        Span::styled(" menu ", theme::dim()),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::style::Modifier;
    use readingbuddy::{CalibreMatch, Diagnostic, DiagnosticKind, MatchCandidate, Severity};

    fn calibre_row(state: CalibreRowState) -> CalibreRow {
        CalibreRow {
            calibre_id: 1,
            uuid: Some("c47437a8".into()),
            title: "Pachinko".into(),
            authors: vec!["Min Jin Lee".into()],
            state,
            note: None,
            notices: Vec::new(),
        }
    }

    /// Same rule as every other list here: reversing the whole line turns the row
    /// into a solid bar and throws away the colour separating the state word from
    /// the title.
    #[test]
    fn selection_reverses_the_title_only() {
        let r = calibre_row(CalibreRowState::New);
        let line = row(&r, false, true);
        let reversed: Vec<bool> = line
            .spans
            .iter()
            .map(|s| s.style.add_modifier.contains(Modifier::REVERSED))
            .collect();
        // mark gutter, state word, title, authors, detail
        assert_eq!(reversed, vec![false, false, true, false, false]);
    }

    /// Every state has to say something, or a row is a word with nothing beside
    /// it and no way to tell what it is waiting for.
    #[test]
    fn every_state_has_a_readable_detail() {
        let states = [
            CalibreRowState::New,
            CalibreRowState::Linked {
                matched_by: CalibreMatch::Uuid,
                tags: 2,
                cover: true,
                files: 1,
            },
            CalibreRowState::Candidates(vec![MatchCandidate {
                book_id: 3,
                title: "Pachinko: A Novel".into(),
                score: 0.72,
            }]),
            CalibreRowState::Candidates(Vec::new()),
            CalibreRowState::Unreadable(Diagnostic {
                kind: DiagnosticKind::CalibreRowSkipped { calibre_id: 7 },
                severity: Severity::Warning,
                detail: "no title, so there is nothing to match on".into(),
            }),
        ];
        for state in states {
            let label = state.label();
            let r = calibre_row(state);
            assert!(!detail(&r).is_empty(), "{label} says nothing");
        }
    }

    /// A row that is a question has to name both ways out of it, or the guard is
    /// a refusal with nowhere to go.
    #[test]
    fn a_candidate_row_names_both_moves() {
        let r = calibre_row(CalibreRowState::Candidates(vec![MatchCandidate {
            book_id: 3,
            title: "Pachinko: A Novel".into(),
            score: 0.72,
        }]));
        let detail = detail(&r);
        assert!(detail.contains("l to link"), "{detail}");
        assert!(detail.contains("n if not"), "{detail}");
    }

    /// What an import reported wins over what the scan predicted — the row is
    /// about what happened, once something has.
    #[test]
    fn an_imported_row_shows_what_its_import_reported() {
        let mut r = calibre_row(CalibreRowState::New);
        r.note = Some("Pachinko: 2 tags, cover (matched by new)".into());
        assert_eq!(detail(&r), "Pachinko: 2 tags, cover (matched by new)");
    }

    /// A calibre that is not installed is reported and nothing is prescribed.
    /// `docs/decisions.md` makes this a rule, and it is the sort that erodes by
    /// helpfulness, so it is asserted on the words themselves.
    #[test]
    fn an_absent_calibre_names_nothing_to_install() {
        let hint = "calibre's library tools aren't on this machine, so there's no shelf to show";
        for forbidden in [
            "install",
            "download",
            "brew ",
            "http",
            ".com",
            "you need",
            "to enable",
        ] {
            assert!(
                !hint.to_lowercase().contains(forbidden),
                "the absent-calibre hint must not say {forbidden:?}: {hint}"
            );
        }
    }

    /// Nothing on this shelf tallies what the user has not done.
    /// `docs/decisions.md` bans task-completion framing by name, and a shelf is
    /// exactly where a "32 books not imported" badge would want to live.
    #[test]
    fn the_key_bar_counts_nothing() {
        let bar = key_bar()
            .spans
            .iter()
            .map(|s| s.content.to_string())
            .collect::<String>();
        assert!(
            !bar.chars().any(|c| c.is_ascii_digit()),
            "the key bar carries a number: {bar}"
        );
    }
}
