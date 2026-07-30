//! The `?` page: what this screen is, and what it responds to.
//!
//! One page per [`Screen`], and the split between them is the whole design.
//! **A screen's page lists only the keys that screen gives a meaning of its
//! own.** The keys that mean the same thing everywhere — `q`, `m`, `esc`, the
//! `j`/`k` movement pair, `?` itself — are on the menu's page and nowhere else,
//! because repeating them nine times would bury the two or three lines that are
//! actually news about the screen you are standing on. That is a rule, not a
//! habit: [`tests::no_screen_page_repeats_a_global_key`] asserts it.
//!
//! The menu's page therefore does double duty — it is the app's introduction as
//! well as the global key table, which is right twice over: the menu is the one
//! screen that is about the app rather than about a book, and it is both the
//! screen the app opens on and the screen `esc` walks back to.
//!
//! Nothing here counts anything and nothing here is a dead end: the page is a
//! place you look, and any key closes it.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Padding, Paragraph, Wrap};

use crate::app::Screen;
use crate::theme;

/// A run of key rows under an optional heading. Only the menu's page has more
/// than one — the global table and the menu's own keys are two different kinds
/// of fact and reading as one list would flatten them.
pub struct Section {
    pub heading: Option<&'static str>,
    pub keys: &'static [(&'static str, &'static str)],
}

/// One screen's page: what it is, then what it responds to.
pub struct Help {
    pub title: &'static str,
    /// Prose, pre-split into lines. Short on purpose — the page describes the
    /// screen in a breath and then gets out of the way.
    pub about: &'static [&'static str],
    pub sections: &'static [Section],
}

/// The keys every screen answers to. Listed once, on the menu's page.
const GLOBAL: Section = Section {
    heading: Some("everywhere"),
    keys: &[
        ("↑ ↓ / k j", "move the selection"),
        ("ctrl-d / ctrl-u", "jump a page, stopping at the ends"),
        ("enter", "open what you are standing on"),
        ("esc / b", "back one step, stopping at the menu"),
        ("m", "the menu, from anywhere"),
        ("?", "this page, for whatever screen you are on"),
        ("q", "quit — the only way out"),
    ],
};

/// The page for a screen. Exhaustive on [`Screen`], so a new screen cannot be
/// added without deciding what its page says.
pub fn page(screen: Screen) -> Help {
    match screen {
        Screen::Menu => Help {
            title: " readingbuddy ",
            about: &[
                "A place for the books you read: what you are part-way through,",
                "what you have highlighted, and what you thought about it.",
                "",
                "This is the front door, and every row on it is a place to go:",
                "what you have open right now, the whole library, a provider",
                "search, and the other systems that own books — a mounted",
                "KOReader, a calibre library, a Goodreads export.",
                "",
                "Nothing here is a queue and nothing counts what you have not",
                "done. Every screen keeps its keys in its own border, and ? on",
                "any of them says what that screen is for. esc retraces the way",
                "you came and stops here, so nothing you open is a dead end.",
            ],
            sections: &[
                GLOBAL,
                Section {
                    heading: Some("here"),
                    keys: &[("enter", "open the highlighted row")],
                },
            ],
        },

        Screen::Home => Help {
            title: " reading ",
            about: &[
                "What you have open — one row per reading you have not finished,",
                "most recently touched first. Every row is a place to go rather",
                "than a task, so there is no count of anything on it.",
                "",
                "A reflection is private and accretes as you read. A review is",
                "written for other people and is the one note that carries a",
                "rating. There is one of each per reading, so the key opens the",
                "same file the second time you press it.",
            ],
            sections: &[Section {
                heading: None,
                keys: &[
                    ("enter", "open the book"),
                    ("e", "this reading's reflection"),
                    ("w", "this reading's review"),
                    ("/", "find a book in the library"),
                ],
            }],
        },

        Screen::Library => Help {
            title: " library ",
            about: &[
                "Every book readingbuddy holds.",
                "",
                "`/` narrows the list to what you type, and the border says so",
                "while it is narrowed — a short list must never look like a",
                "library that has lost books. esc widens it back before it",
                "leaves the screen.",
                "",
                "`s` cycles the order: most recently touched, by title, by the",
                "author's last name, then by year with the newest first. The",
                "border always names the one in force, the book you were",
                "standing on stays under the cursor, and the order is",
                "remembered for next time.",
            ],
            sections: &[Section {
                heading: None,
                keys: &[
                    ("enter", "open the book"),
                    ("/", "narrow the list"),
                    ("s", "cycle the order"),
                    ("d", "remove the book from the library"),
                ],
            }],
        },

        Screen::Book => Help {
            title: " a book ",
            about: &[
                "The book itself, turning, beside its sections: Info, Notes,",
                "Highlights and Cards. ↑ ↓ move the section menu, enter or →",
                "opens one, esc or ← backs out — the book keeps turning either",
                "way.",
                "",
                "Notes carry a marker: ◆ is the reading's reflection, ◇ its",
                "review. Standing on a note, L shows what links to it and what",
                "it links out to, including targets nobody has written yet.",
            ],
            sections: &[
                Section {
                    heading: Some("this book"),
                    keys: &[
                        ("n", "write a note against this book"),
                        ("e", "the reflection"),
                        ("w", "the review, and its rating"),
                        ("L", "what links to the selected note"),
                        ("d", "delete the selected note"),
                        ("p", "set the page you are on"),
                        ("f", "mark it finished, or unfinish it"),
                        ("x", "export this book's flashcards"),
                    ],
                },
                Section {
                    heading: Some("the view"),
                    keys: &[
                        ("space", "stop or start the turn"),
                        ("r", "reset the pose"),
                        ("t", "rotate the panes"),
                        ("tab", "show or hide the section pane"),
                        ("[ ]", "slide the divider"),
                        ("v", "swap pixels for block glyphs"),
                        ("o", "expand or collapse the key bar"),
                    ],
                },
            ],
        },

        Screen::Search => Help {
            title: " search ",
            about: &[
                "A federated search across OpenLibrary and Google Books. A title,",
                "an author or an ISBN all work; a provider that fails or times",
                "out becomes a warning rather than an empty screen.",
                "",
                "enter adds the highlighted result to the library, cover and all.",
            ],
            sections: &[Section {
                heading: None,
                keys: &[
                    ("/", "ask something else"),
                    ("enter", "add this result to the library"),
                ],
            }],
        },

        Screen::Settings => Help {
            title: " settings ",
            about: &[
                "Where your data lives, and how the app looks. The paths are",
                "shown rather than editable — they come from --data-dir or",
                "READINGBUDDY_DATA_DIR.",
                "",
                "The Google Books key is optional: without one the provider",
                "still answers, on a lower quota. Only a masked form of it is",
                "ever shown, here or anywhere else.",
            ],
            sections: &[Section {
                heading: None,
                keys: &[
                    ("← →", "cycle the accent colour"),
                    ("/", "type an accent hex directly"),
                    ("enter", "swap the glyph set (octant / quadrant)"),
                    ("a", "cycle the ambient background"),
                    ("g", "paste a Google Books API key"),
                ],
            }],
        },

        Screen::Device => Help {
            title: " device ",
            about: &[
                "A mounted KOReader, as its own shelf. Each row is a book on the",
                "device in the state the last scan found it: new here, unchanged,",
                "updated since you last looked, or a sidecar that would not parse.",
                "",
                "The device owns its highlights and its reading state, so a pull",
                "copies them here and conflicts resolve toward the device. A row",
                "that looks like a book you already have is offered rather than",
                "duplicated — l picks which one it is.",
            ],
            sections: &[Section {
                heading: None,
                keys: &[
                    ("enter", "bring this book across"),
                    ("x", "mark the row"),
                    ("s", "sync the marked rows, or every syncable one"),
                    ("l", "link the row to a book already here"),
                    ("r", "walk the device again"),
                    ("/", "scan a different path"),
                ],
            }],
        },

        Screen::Calibre => Help {
            title: " calibre ",
            about: &[
                "A calibre library, as its own shelf — the device screen's shape,",
                "because calibre is another system that owns books. Each row is",
                "linked already, new here, or ambiguous enough to ask about.",
                "",
                "s never sweeps an ambiguous row: that row is a question, and",
                "answering it by creating a duplicate is what l and n are for.",
                "calibre is found if it is installed and reported if it is not.",
            ],
            sections: &[Section {
                heading: None,
                keys: &[
                    ("enter", "import this row"),
                    ("x", "mark the row"),
                    ("s", "import the marked rows, or every unambiguous one"),
                    ("l", "link the row to a book already here"),
                    ("n", "import it as a new book anyway"),
                    ("c", "convert a file between formats"),
                    ("r", "read the library again"),
                    ("/", "read a different library"),
                ],
            }],
        },

        Screen::Goodreads => Help {
            title: " goodreads ",
            about: &[
                "A Goodreads export, previewed before any of it is applied — and",
                "the screen your own library is written out from. Not a shelf: a",
                "CSV is matched against the library as a whole, so the rows it",
                "could not place are listed last, where they are the work.",
                "",
                "Shelves become readings: read, currently-reading, or nothing at",
                "all for to-read. Prose you have already written here is kept,",
                "never overwritten.",
            ],
            sections: &[Section {
                heading: None,
                keys: &[
                    ("s", "apply the whole preview"),
                    ("l", "link the selected row to a book already here"),
                    ("n", "apply it, unmatched rows in as new books"),
                    ("x", "export this library as a Goodreads CSV"),
                    ("/", "read a different file"),
                    ("r", "read the file again"),
                ],
            }],
        },
    }
}

/// The page as lines, ready to measure and draw. Public for the tests and for
/// `print_help`, which is how a change to the wording is looked at without a
/// terminal.
pub fn lines(help: &Help) -> Vec<Line<'static>> {
    let mut out: Vec<Line> = Vec::new();
    for text in help.about {
        out.push(Line::from(Span::styled(*text, theme::dim())));
    }
    // The key column is padded to the widest key across the *whole* page, so
    // the descriptions line up down the page rather than per section.
    let width = help
        .sections
        .iter()
        .flat_map(|s| s.keys.iter())
        .map(|(k, _)| k.chars().count())
        .max()
        .unwrap_or(0);
    for section in help.sections {
        out.push(Line::from(""));
        if let Some(heading) = section.heading {
            out.push(Line::from(Span::styled(heading, theme::accent())));
        }
        for (key, what) in section.keys {
            out.push(Line::from(vec![
                Span::styled(format!("{key:>width$}  "), theme::key()),
                Span::styled(*what, theme::primary()),
            ]));
        }
    }
    out
}

/// Draw the page over whatever screen is underneath.
pub fn render(f: &mut Frame, area: Rect, screen: Screen) {
    let help = page(screen);
    let rows = lines(&help);
    let width = rows
        .iter()
        .map(|l| l.width() as u16)
        .max()
        .unwrap_or(20)
        .max(help.title.chars().count() as u16)
        .saturating_add(4);
    let box_area = super::centered(area, width, rows.len() as u16 + 2);
    // `Clear` first: this floats over a screen that is still drawn underneath,
    // and a `Block` styles the cells it does not draw without blanking them.
    f.render_widget(ratatui::widgets::Clear, box_area);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme::dim())
        .padding(Padding::horizontal(1))
        .title(Span::styled(help.title, theme::accent()))
        // The way out, in the border — the same place every other screen keeps
        // its keys, and the reason this page cannot be the dead end it exists
        // to prevent.
        .title_bottom(
            Line::from(vec![
                Span::styled(" any key ", theme::key()),
                Span::styled("back ", theme::dim()),
            ])
            .centered(),
        );
    let inner = block.inner(box_area);
    f.render_widget(block, box_area);
    // A `Block` on a 1×1 pane has no inside at all, and a `Paragraph` into a
    // zero rect is what panics.
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    f.render_widget(Paragraph::new(rows).wrap(Wrap { trim: false }), inner);
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCREENS: [Screen; 9] = [
        Screen::Home,
        Screen::Menu,
        Screen::Library,
        Screen::Book,
        Screen::Search,
        Screen::Settings,
        Screen::Device,
        Screen::Calibre,
        Screen::Goodreads,
    ];

    /// Every screen has a page, and every page says something and lists
    /// something. A page with an empty key table would be a screen claiming it
    /// responds to nothing.
    #[test]
    fn every_screen_has_a_page_with_prose_and_keys() {
        for screen in SCREENS {
            let help = page(screen);
            assert!(!help.about.is_empty(), "{screen:?} has no description");
            assert!(
                help.sections.iter().any(|s| !s.keys.is_empty()),
                "{screen:?} lists no keys"
            );
            assert!(
                help.title.starts_with(' '),
                "{screen:?} title needs padding"
            );
        }
    }

    /// **The rule the split exists for.** A key that means the same thing on
    /// every screen belongs on the menu's page and nowhere else — repeated nine
    /// times it buries the two or three lines that are actually news.
    ///
    /// `enter` is deliberately absent from this list: it opens, pulls, imports
    /// and adds depending on where you are standing, so its meaning *is*
    /// screen-specific and each page owes it a line.
    #[test]
    fn no_screen_page_repeats_a_global_key() {
        const GLOBALS: [&str; 9] = [
            "q",
            "m",
            "?",
            "j",
            "k",
            "esc",
            "b",
            "↑ ↓ / k j",
            "ctrl-d / ctrl-u",
        ];
        for screen in SCREENS {
            if screen == Screen::Menu {
                continue;
            }
            for section in page(screen).sections {
                for (key, _) in section.keys {
                    assert!(
                        !GLOBALS.contains(key),
                        "{screen:?} repeats the global key `{key}`"
                    );
                }
            }
        }
    }

    /// The menu's page is the one that carries the globals, and it is the app's
    /// introduction as well — so it says the app's name and it lists `q`.
    #[test]
    fn the_menu_page_is_the_introduction_and_the_global_table() {
        let help = page(Screen::Menu);
        assert!(help.title.contains("readingbuddy"));
        let keys: Vec<&str> = help
            .sections
            .iter()
            .flat_map(|s| s.keys.iter())
            .map(|(k, _)| *k)
            .collect();
        for global in ["q", "m", "?"] {
            assert!(
                keys.contains(&global),
                "the menu page never mentions {global}"
            );
        }
    }

    /// No page tallies anything — `docs/decisions.md` rules out task-completion
    /// framing by name, and a help page is exactly where a "3 of 7" would look
    /// harmless.
    #[test]
    fn no_page_counts_anything() {
        for screen in SCREENS {
            let help = page(screen);
            for line in help.about {
                assert!(
                    !line.chars().any(|c| c.is_ascii_digit()),
                    "{screen:?} has a number in its description: {line}"
                );
            }
        }
    }

    /// The key column is padded to the widest key on the page, so the
    /// descriptions line up across sections rather than within them.
    #[test]
    fn the_key_column_is_one_width_down_the_whole_page() {
        let help = page(Screen::Book);
        let rendered = lines(&help);
        let widths: Vec<usize> = rendered
            .iter()
            .filter(|l| l.spans.len() == 2)
            .map(|l| l.spans[0].content.chars().count())
            .collect();
        assert!(widths.len() > 8, "expected the book page's two sections");
        assert!(
            widths.iter().all(|w| *w == widths[0]),
            "the key column changes width down the page"
        );
    }
}
