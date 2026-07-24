//! Application state and the async event loop.

use std::time::Duration;

use anyhow::Result;
use crossterm::event::{Event, EventStream, KeyCode, KeyEvent, KeyEventKind};
use futures::StreamExt;
use ratatui::Terminal;
use ratatui::backend::Backend;
use ratatui::layout::Position;
use ratatui::widgets::ListState;
use readingbuddy::{
    Book, BookSort, Engine, FlashcardRow, Highlight, NewNoteInput, NoteKind, NoteRecord,
    RankedResult, SearchRequest,
};

use crossterm::event::KeyModifiers;

use crate::config::{self, TuiConfig};
use crate::event::Action;
use crate::render3d::{GlyphSet, Pose, RenderParams, Scene};
use crate::theme;
use crate::ui;
use crate::ui::input::InputState;
use crate::ui::textedit::TextEditor;

/// Idle spin: a full turn about the bottom of the spine, so the book sweeps
/// round like a slow top. Radians per tick at 20fps — about 35s for 360°.
const SPIN_SPEED: f32 = 0.009;
/// The pitch nods gently while the book turns, on its own slower cycle.
const NOD: f32 = 0.06;
const NOD_SPEED: f32 = 0.011;
const TICK: Duration = Duration::from_millis(50);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Menu,
    Library,
    Book,
    Search,
    Settings,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuItem {
    Library,
    Continue,
    Search,
    AddIsbn,
    ImportKo,
    Cards,
    Settings,
    Quit,
}

pub const MENU: [(MenuItem, &str, &str); 8] = [
    (MenuItem::Library, "Library", "browse everything you've saved"),
    (MenuItem::Continue, "Continue reading", "jump to the most recent book"),
    (MenuItem::Search, "Search books", "find and add from OpenLibrary + Google Books"),
    (MenuItem::AddIsbn, "Add by ISBN", "look up a single edition and save it"),
    (MenuItem::ImportKo, "Import KOReader", "pull highlights from a .sdr / library path"),
    (MenuItem::Cards, "Flashcards", "count the cards waiting for export"),
    (MenuItem::Settings, "Settings", "data locations, API key, glyph set"),
    (MenuItem::Quit, "Quit", ""),
];

/// The tabs of the single-book view. `Info` keeps the metadata + controls; the
/// rest are per-book lists (notes, highlights, flashcards).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BookTab {
    Info,
    Notes,
    Highlights,
    Cards,
}

pub const BOOK_TABS: [(BookTab, &str); 4] = [
    (BookTab::Info, "Info"),
    (BookTab::Notes, "Notes"),
    (BookTab::Highlights, "Highlights"),
    (BookTab::Cards, "Cards"),
];

/// A book plus everything the view shows alongside it.
pub struct BookView {
    pub book: Book,
    pub notes: Vec<NoteRecord>,
    pub highlights: Vec<Highlight>,
    pub cards: Vec<FlashcardRow>,
}

impl BookView {
    pub fn tab_len(&self, tab: BookTab) -> usize {
        match tab {
            BookTab::Info => 0,
            BookTab::Notes => self.notes.len(),
            BookTab::Highlights => self.highlights.len(),
            BookTab::Cards => self.cards.len(),
        }
    }
}

/// What an open text input is collecting, so `commit` knows what to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputContext {
    SearchQuery,
    IsbnAdd,
    ProgressPage,
    KoPath,
    /// Optional page anchor asked for after composing a new note.
    NotePage,
    /// A `#RRGGBB` accent color typed on the settings screen.
    AccentHex,
}

/// A composed-but-unsaved note, held while its optional page anchor is asked.
pub struct PendingNote {
    pub book_id: Option<i64>,
    pub location: Option<String>,
    pub body: String,
}

pub struct Input {
    pub context: InputContext,
    pub state: InputState,
    pub prompt: &'static str,
}

/// A pending yes/no confirmation.
#[derive(Debug, Clone)]
pub enum Confirm {
    RemoveBook { id: i64, title: String },
}

/// What an open in-house editor will do on save.
pub enum NoteTarget {
    New {
        book_id: Option<i64>,
        page: Option<i64>,
        location: Option<String>,
        highlight_id: Option<i64>,
    },
    Edit(NoteRecord),
}

/// An in-progress note composition: the target plus the live editor buffer.
pub struct NoteDraft {
    pub target: NoteTarget,
    pub editor: TextEditor,
}

impl NoteDraft {
    pub fn title(&self) -> &'static str {
        match self.target {
            NoteTarget::New { .. } => "new note",
            NoteTarget::Edit(_) => "edit note",
        }
    }
}

pub struct App {
    pub engine: Engine,
    pub screen: Screen,
    pub menu_index: usize,
    pub library: Vec<Book>,
    pub library_state: ListState,
    pub view: Option<BookView>,
    /// The book-view section the right pane highlights (in the menu) or shows
    /// (when entered).
    pub book_tab: BookTab,
    /// false: the right pane is the section menu; true: a section is open.
    pub in_section: bool,
    /// Selection within the active book-view list (notes/highlights/cards).
    pub tab_state: ListState,
    pub scene: Scene,
    pub params: RenderParams,
    /// Idle spin on/off.
    pub spinning: bool,
    pub show_options: bool,
    pub status: Option<String>,
    pub input: Option<Input>,
    pub note_editor: Option<NoteDraft>,
    pub pending_note: Option<PendingNote>,
    pub confirm: Option<Confirm>,
    pub search_results: Vec<RankedResult>,
    pub search_state: ListState,
    pub dirty: bool,
    pub quit: bool,
    /// Pitch the nod oscillates around; the yaw just keeps turning.
    base_pitch: f32,
    phase: f32,
    /// Index into `theme::PRESETS` the settings ←/→ cycle last landed on.
    pub accent_idx: usize,
}

impl App {
    pub async fn new(engine: Engine) -> Result<App> {
        let scene = Scene::new(engine.config.images_dir.clone());
        let mut app = App {
            engine,
            screen: Screen::Menu,
            menu_index: 0,
            library: Vec::new(),
            library_state: ListState::default(),
            view: None,
            book_tab: BookTab::Info,
            in_section: false,
            tab_state: ListState::default(),
            scene,
            params: RenderParams::default(),
            spinning: true,
            show_options: false,
            status: None,
            input: None,
            note_editor: None,
            pending_note: None,
            confirm: None,
            search_results: Vec::new(),
            search_state: ListState::default(),
            dirty: true,
            quit: false,
            base_pitch: Pose::default().pitch,
            phase: 0.0,
            // Land the cycle on whichever preset matches the loaded accent.
            accent_idx: theme::PRESETS
                .iter()
                .position(|(_, rgb)| *rgb == theme::accent_rgb())
                .unwrap_or(0),
        };
        app.refresh_library().await?;
        Ok(app)
    }

    pub async fn refresh_library(&mut self) -> Result<()> {
        self.library = self
            .engine
            .storage
            .list_books(200, BookSort::LastModified)
            .await?;
        if !self.library.is_empty() && self.library_state.selected().is_none() {
            self.library_state.select(Some(0));
        }
        if self.library_state.selected().is_some_and(|i| i >= self.library.len()) {
            self.library_state
                .select((!self.library.is_empty()).then(|| self.library.len() - 1));
        }
        self.dirty = true;
        Ok(())
    }

    /// Load a book into the viewing mode, fetching its notes/highlights/cards.
    pub async fn open_book(&mut self, book: Book) -> Result<()> {
        let view = self.load_view(book).await?;
        self.view = Some(view);
        self.book_tab = BookTab::Info;
        self.in_section = false;
        self.tab_state.select(None);
        self.screen = Screen::Book;
        self.status = None;
        self.dirty = true;
        Ok(())
    }

    async fn load_view(&self, book: Book) -> Result<BookView> {
        let (notes, highlights, cards) = match book.id {
            Some(id) => (
                self.engine.list_notes(Some(id)).await?,
                self.engine.storage.list_highlights(id).await?,
                self.engine.list_flashcards_for_book(id).await?,
            ),
            None => (Vec::new(), Vec::new(), Vec::new()),
        };
        Ok(BookView {
            book,
            notes,
            highlights,
            cards,
        })
    }

    /// Re-fetch the open book and its lists after a mutation.
    async fn reload_view(&mut self) -> Result<()> {
        let Some(id) = self.view.as_ref().and_then(|v| v.book.id) else {
            return Ok(());
        };
        if let Some(book) = self.engine.storage.get_book(id).await? {
            self.view = Some(self.load_view(book).await?);
            self.clamp_tab_selection();
        }
        Ok(())
    }

    fn reset_pose(&mut self) {
        self.params.pose = Pose::default();
        self.base_pitch = self.params.pose.pitch;
        self.phase = 0.0;
    }

    /// Advance the idle animation. Returns true when something moved.
    fn tick(&mut self) -> bool {
        if !(self.spinning
            && self.screen == Screen::Book
            && self.input.is_none()
            && self.note_editor.is_none())
        {
            return false;
        }
        self.phase += NOD_SPEED;
        self.params.pose.yaw = wrap_angle(self.params.pose.yaw + SPIN_SPEED);
        self.params.pose.pitch = self.base_pitch + self.phase.sin() * NOD;
        true
    }

    // ---- action dispatch ---------------------------------------------------

    pub async fn handle(&mut self, action: Action) -> Result<()> {
        self.dirty = true;
        match (self.screen, action) {
            (_, Action::Quit) => self.quit = true,
            (_, Action::Menu) => {
                self.screen = Screen::Menu;
                self.status = None;
            }
            (_, Action::Refresh) => self.refresh_library().await?,

            (Screen::Menu, Action::Up) => {
                self.menu_index = (self.menu_index + MENU.len() - 1) % MENU.len();
            }
            (Screen::Menu, Action::Down) => {
                self.menu_index = (self.menu_index + 1) % MENU.len();
            }
            (Screen::Menu, Action::Select) => self.activate_menu().await?,
            (Screen::Menu, Action::Back) => self.quit = true,

            (Screen::Library, Action::Up) => self.step_library(-1),
            (Screen::Library, Action::Down) => self.step_library(1),
            (Screen::Library, Action::Back) => self.screen = Screen::Menu,
            (Screen::Library, Action::Delete) => self.ask_remove_selected(),
            (Screen::Library, Action::Select) => {
                if let Some(book) = self
                    .library_state
                    .selected()
                    .and_then(|i| self.library.get(i))
                    .cloned()
                {
                    self.open_book(book).await?;
                }
            }

            (Screen::Search, Action::Up) => self.step_search(-1),
            (Screen::Search, Action::Down) => self.step_search(1),
            (Screen::Search, Action::Query) => {
                self.start_input(InputContext::SearchQuery, "search", "")
            }
            (Screen::Search, Action::Select) => self.add_search_result().await?,
            (Screen::Search, Action::Back) => self.screen = Screen::Menu,

            (Screen::Settings, Action::Select | Action::ToggleSpin) => self.toggle_glyphs(),
            (Screen::Settings, Action::Left) => self.cycle_accent(-1),
            (Screen::Settings, Action::Right) => self.cycle_accent(1),
            (Screen::Settings, Action::Query) => {
                self.start_input(InputContext::AccentHex, "accent #RRGGBB", &theme::to_hex(theme::accent_rgb()))
            }
            (Screen::Settings, Action::Back) => self.screen = Screen::Menu,

            (Screen::Book, action) => self.handle_book(action).await?,

            _ => self.dirty = false,
        }
        Ok(())
    }

    /// Book-view keys. The book spins on the left; the right pane is either the
    /// section menu (Up/Down move it, Enter/→ enters a section) or an open
    /// section (Up/Down move its list, Esc/← returns to the menu). View-wide
    /// actions (note, progress, finish, export, spin) work in either mode.
    async fn handle_book(&mut self, action: Action) -> Result<()> {
        match action {
            Action::ToggleOptions => self.show_options = !self.show_options,
            Action::Reset => {
                self.reset_pose();
                self.spinning = true;
            }
            Action::ToggleSpin => self.spinning = !self.spinning,
            Action::NewNote => self.new_note(false),
            Action::EditProgress => self.start_input(InputContext::ProgressPage, "page", ""),
            Action::ToggleFinished => self.toggle_finished().await?,
            Action::Export => self.export_cards().await?,

            // Back / left: leave the section, or leave the book.
            Action::Back | Action::Left => {
                if self.in_section {
                    self.in_section = false;
                } else {
                    self.screen = Screen::Library;
                }
            }
            Action::Up => {
                if self.in_section {
                    self.step_tab(-1);
                } else {
                    self.cycle_tab(-1);
                }
            }
            Action::Down | Action::NextTab => {
                if self.in_section {
                    self.step_tab(1);
                } else {
                    self.cycle_tab(1);
                }
            }
            Action::PrevTab => {
                if !self.in_section {
                    self.cycle_tab(-1);
                }
            }
            // Enter / right: open the highlighted section, or act on a row.
            Action::Select | Action::Right => {
                if self.in_section {
                    self.activate_tab_row().await?;
                } else {
                    self.enter_section();
                }
            }
            _ => self.dirty = false,
        }
        Ok(())
    }

    /// Open the highlighted section into the right pane.
    fn enter_section(&mut self) {
        self.in_section = true;
        self.clamp_tab_selection();
    }

    async fn activate_menu(&mut self) -> Result<()> {
        match MENU[self.menu_index].0 {
            MenuItem::Library => {
                self.refresh_library().await?;
                if self.library.is_empty() {
                    self.status =
                        Some("library is empty — add a book with Search or Add by ISBN".into());
                } else {
                    self.screen = Screen::Library;
                }
            }
            MenuItem::Continue => {
                self.refresh_library().await?;
                match self.library.first().cloned() {
                    Some(book) => self.open_book(book).await?,
                    None => self.status = Some("nothing to continue — the library is empty".into()),
                }
            }
            MenuItem::Search => {
                self.screen = Screen::Search;
                self.search_results.clear();
                self.search_state.select(None);
                self.start_input(InputContext::SearchQuery, "search", "");
            }
            MenuItem::AddIsbn => self.start_input(InputContext::IsbnAdd, "isbn", ""),
            MenuItem::ImportKo => self.start_input(InputContext::KoPath, "koreader path", ""),
            MenuItem::Cards => {
                let n = self.engine.list_flashcards(false).await?.len();
                self.status = Some(format!("{n} flashcard candidate(s) pending — open a book's Cards tab to export"));
            }
            MenuItem::Settings => self.screen = Screen::Settings,
            MenuItem::Quit => self.quit = true,
        }
        Ok(())
    }

    // ---- book-view sub-actions --------------------------------------------

    fn cycle_tab(&mut self, delta: isize) {
        let cur = BOOK_TABS.iter().position(|(t, _)| *t == self.book_tab).unwrap_or(0);
        let next = (cur as isize + delta).rem_euclid(BOOK_TABS.len() as isize) as usize;
        self.book_tab = BOOK_TABS[next].0;
        self.tab_state.select(None);
        self.clamp_tab_selection();
    }

    fn clamp_tab_selection(&mut self) {
        let len = self.view.as_ref().map_or(0, |v| v.tab_len(self.book_tab));
        match self.tab_state.selected() {
            _ if len == 0 => self.tab_state.select(None),
            None => self.tab_state.select(Some(0)),
            Some(i) if i >= len => self.tab_state.select(Some(len - 1)),
            _ => {}
        }
    }

    fn step_tab(&mut self, delta: isize) {
        let len = self.view.as_ref().map_or(0, |v| v.tab_len(self.book_tab));
        if len == 0 {
            return;
        }
        let cur = self.tab_state.selected().unwrap_or(0) as isize;
        self.tab_state.select(Some((cur + delta).rem_euclid(len as isize) as usize));
    }

    /// Enter on a list row: edit a note, or anchor a new note to a highlight.
    async fn activate_tab_row(&mut self) -> Result<()> {
        match self.book_tab {
            BookTab::Notes => self.edit_selected_note().await?,
            BookTab::Highlights => self.new_note(true),
            _ => self.dirty = false,
        }
        Ok(())
    }

    /// Open the in-house editor on a fresh note. When `from_highlight`, anchor
    /// it to the selected highlight (inheriting its page/chapter); otherwise
    /// anchor to the book's current reading page.
    fn new_note(&mut self, from_highlight: bool) {
        let Some(view) = &self.view else { return };
        let target = if from_highlight {
            let Some(h) = self.tab_state.selected().and_then(|i| view.highlights.get(i)) else {
                self.status = Some("no highlight selected".into());
                return;
            };
            NoteTarget::New {
                book_id: view.book.id,
                page: h.page,
                location: h.chapter.clone(),
                highlight_id: Some(h.id),
            }
        } else {
            NoteTarget::New {
                book_id: view.book.id,
                page: view.book.current_page,
                location: None,
                highlight_id: None,
            }
        };
        self.note_editor = Some(NoteDraft {
            target,
            editor: TextEditor::new(""),
        });
        self.dirty = true;
    }

    /// Open the editor on the selected note's body (frontmatter preserved).
    async fn edit_selected_note(&mut self) -> Result<()> {
        let note = self
            .tab_state
            .selected()
            .and_then(|i| self.view.as_ref().and_then(|v| v.notes.get(i)))
            .cloned();
        let Some(note) = note else {
            return Ok(());
        };
        let body = self.engine.note_body(&note).unwrap_or_default();
        self.note_editor = Some(NoteDraft {
            target: NoteTarget::Edit(note),
            editor: TextEditor::new(&body),
        });
        self.dirty = true;
        Ok(())
    }

    /// Route a key to the open note editor. Enter saves, Esc cancels, the
    /// newline chords (Shift/Alt+Enter, Ctrl+J) add a line, the rest edits.
    async fn on_editor_key(&mut self, key: KeyEvent) -> Result<()> {
        self.dirty = true;
        let Some(draft) = self.note_editor.as_mut() else {
            return Ok(());
        };
        let newline_chord = key.modifiers.intersects(KeyModifiers::SHIFT | KeyModifiers::ALT);
        match key.code {
            KeyCode::Esc => {
                self.note_editor = None;
                self.status = Some("note discarded".into());
            }
            KeyCode::Enter if newline_chord => draft.editor.newline(),
            KeyCode::Char('j') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                draft.editor.newline()
            }
            KeyCode::Enter => self.save_note_editor().await?,
            KeyCode::Backspace => draft.editor.backspace(),
            KeyCode::Left => draft.editor.left(),
            KeyCode::Right => draft.editor.right(),
            KeyCode::Up => draft.editor.up(),
            KeyCode::Down => draft.editor.down(),
            KeyCode::Char(c) => draft.editor.insert(c),
            _ => {}
        }
        Ok(())
    }

    async fn save_note_editor(&mut self) -> Result<()> {
        let Some(draft) = self.note_editor.take() else {
            return Ok(());
        };
        if draft.editor.is_blank() {
            self.status = Some("empty note, nothing saved".into());
            return Ok(());
        }
        let body = draft.editor.text();
        match draft.target {
            // A note anchored to a highlight already has its page; save straight.
            NoteTarget::New {
                book_id,
                page,
                location,
                highlight_id: Some(highlight_id),
            } => {
                self.create_note_from(book_id, page, location, Some(highlight_id), body)
                    .await?;
            }
            // A plain new note: ask for an optional page before saving.
            NoteTarget::New {
                book_id, location, ..
            } => {
                self.pending_note = Some(PendingNote {
                    book_id,
                    location,
                    body,
                });
                self.start_input(InputContext::NotePage, "page (optional, enter to skip)", "");
            }
            NoteTarget::Edit(note) => {
                self.engine.update_note_body(&note, &body).await?;
                self.status = Some(format!("updated “{}”", note.title));
                self.reload_view().await?;
                self.clamp_tab_selection();
            }
        }
        Ok(())
    }

    /// Persist a note and refresh the view, jumping to the Notes section.
    async fn create_note_from(
        &mut self,
        book_id: Option<i64>,
        page: Option<i64>,
        location: Option<String>,
        highlight_id: Option<i64>,
        body: String,
    ) -> Result<()> {
        let created = self
            .engine
            .create_note(NewNoteInput {
                book_id,
                highlight_id,
                page,
                location,
                kind: NoteKind::Note,
                title: None,
                body,
            })
            .await?;
        self.status = Some(format!("saved note “{}”", created.title));
        self.book_tab = BookTab::Notes;
        self.in_section = true;
        self.reload_view().await?;
        self.clamp_tab_selection();
        Ok(())
    }

    /// Finish the pending note with an optional page (empty/invalid = none).
    async fn commit_pending_note(&mut self, page_text: String) -> Result<()> {
        let Some(pending) = self.pending_note.take() else {
            return Ok(());
        };
        let page = page_text.trim().parse::<i64>().ok().filter(|p| *p > 0);
        self.create_note_from(pending.book_id, page, pending.location, None, pending.body)
            .await?;
        Ok(())
    }

    async fn toggle_finished(&mut self) -> Result<()> {
        let Some(id) = self.view.as_ref().and_then(|v| v.book.id) else {
            return Ok(());
        };
        let finished = self.view.as_ref().is_some_and(|v| v.book.finished);
        self.engine
            .storage
            .update_progress(id, None, Some(!finished))
            .await?;
        self.reload_view().await?;
        self.status = Some(if finished {
            "marked unfinished".into()
        } else {
            "marked finished 🎉".into()
        });
        Ok(())
    }

    async fn export_cards(&mut self) -> Result<()> {
        let (tsv, count) = self.engine.export_flashcards(false).await?;
        if count == 0 {
            self.status = Some("no unexported cards".into());
            return Ok(());
        }
        let dir = self
            .engine
            .config
            .vault_dir
            .parent()
            .unwrap_or(&self.engine.config.vault_dir)
            .to_path_buf();
        let out = dir.join("flashcards.tsv");
        std::fs::write(&out, tsv)?;
        self.status = Some(format!("exported {count} cards -> {}", out.display()));
        self.reload_view().await?;
        Ok(())
    }

    fn toggle_glyphs(&mut self) {
        self.params.glyphs = match self.params.glyphs {
            GlyphSet::Octant => GlyphSet::Quadrant,
            GlyphSet::Quadrant => GlyphSet::Octant,
        };
        self.status = Some(format!("glyph set: {:?}", self.params.glyphs));
    }

    /// Step through the preset palette (`dir` = +1 / -1), apply live, persist.
    fn cycle_accent(&mut self, dir: i64) {
        let n = theme::PRESETS.len() as i64;
        self.accent_idx = (((self.accent_idx as i64 + dir) % n + n) % n) as usize;
        let (name, rgb) = theme::PRESETS[self.accent_idx];
        theme::set_accent(rgb);
        self.persist_accent();
        self.status = Some(format!("accent: {name} {}", theme::to_hex(rgb)));
    }

    /// Apply an arbitrary accent (from the hex box) and persist it.
    fn set_accent_rgb(&mut self, rgb: u32) {
        theme::set_accent(rgb);
        self.accent_idx = theme::PRESETS
            .iter()
            .position(|(_, p)| *p == rgb)
            .unwrap_or(self.accent_idx);
        self.persist_accent();
        self.status = Some(format!("accent: {}", theme::to_hex(rgb)));
    }

    /// Write the current accent to the TUI config file. A failure is a status
    /// warning, never fatal.
    fn persist_accent(&mut self) {
        let cfg = TuiConfig {
            accent: Some(theme::to_hex(theme::accent_rgb())),
        };
        if let Err(e) = config::save(&cfg) {
            self.status = Some(format!("could not save accent: {e:#}"));
        }
    }

    // ---- library remove ----------------------------------------------------

    fn ask_remove_selected(&mut self) {
        if let Some(book) = self
            .library_state
            .selected()
            .and_then(|i| self.library.get(i))
            && let Some(id) = book.id
        {
            self.confirm = Some(Confirm::RemoveBook {
                id,
                title: book.display_title().to_string(),
            });
        }
    }

    async fn resolve_confirm(&mut self, yes: bool) -> Result<()> {
        let confirm = self.confirm.take();
        self.dirty = true;
        if !yes {
            self.status = Some("kept.".into());
            return Ok(());
        }
        match confirm {
            Some(Confirm::RemoveBook { id, title }) => {
                self.engine.delete_book(id).await?;
                self.refresh_library().await?;
                self.status = Some(format!("removed {title}"));
            }
            None => {}
        }
        Ok(())
    }

    // ---- text input --------------------------------------------------------

    fn start_input(&mut self, context: InputContext, prompt: &'static str, initial: &str) {
        self.input = Some(Input {
            context,
            state: InputState::new(initial),
            prompt,
        });
        self.dirty = true;
    }

    async fn on_input_key(&mut self, key: KeyEvent) -> Result<()> {
        self.dirty = true;
        let Some(input) = self.input.as_mut() else {
            return Ok(());
        };
        match key.code {
            KeyCode::Esc => {
                let context = input.context;
                self.input = None;
                if context == InputContext::NotePage {
                    // Esc skips the page but keeps the note already written.
                    self.commit_pending_note(String::new()).await?;
                } else if self.screen == Screen::Search && self.search_results.is_empty() {
                    // A cancelled search query with no results falls back to menu.
                    self.screen = Screen::Menu;
                }
            }
            KeyCode::Enter => {
                let text = input.state.take().trim().to_string();
                let context = input.context;
                self.input = None;
                self.commit_input(context, text).await?;
            }
            KeyCode::Backspace => input.state.backspace(),
            KeyCode::Left => input.state.left(),
            KeyCode::Right => input.state.right(),
            KeyCode::Char(c) => input.state.insert(c),
            _ => {}
        }
        Ok(())
    }

    async fn commit_input(&mut self, context: InputContext, text: String) -> Result<()> {
        // The page prompt commits even when empty (empty = no page).
        if context == InputContext::NotePage {
            return self.commit_pending_note(text).await;
        }
        if text.is_empty() {
            if context == InputContext::SearchQuery && self.search_results.is_empty() {
                self.screen = Screen::Menu;
            }
            return Ok(());
        }
        match context {
            InputContext::SearchQuery => self.run_search(text).await?,
            InputContext::IsbnAdd => self.add_isbn(text).await?,
            InputContext::KoPath => self.import_ko(text).await?,
            InputContext::ProgressPage => self.commit_progress(text).await?,
            InputContext::AccentHex => match theme::parse_hex(&text) {
                Some(rgb) => self.set_accent_rgb(rgb),
                None => self.status = Some(format!("not a #RRGGBB color: {text}")),
            },
            InputContext::NotePage => unreachable!("handled above"),
        }
        Ok(())
    }

    async fn commit_progress(&mut self, text: String) -> Result<()> {
        let Some(id) = self.view.as_ref().and_then(|v| v.book.id) else {
            return Ok(());
        };
        match text.parse::<i64>() {
            Ok(page) => {
                self.engine.storage.update_progress(id, Some(page), None).await?;
                self.reload_view().await?;
                self.status = Some(format!("progress → page {page}"));
            }
            Err(_) => self.status = Some(format!("'{text}' isn't a page number")),
        }
        Ok(())
    }

    // ---- global actions ----------------------------------------------------

    async fn run_search(&mut self, query: String) -> Result<()> {
        self.status = Some(format!("searching “{query}”…"));
        let req = SearchRequest {
            query: Some(query),
            limit: 15,
            ..Default::default()
        };
        match self.engine.search(&req).await {
            Ok(outcome) => {
                self.search_results = outcome.results;
                self.search_state
                    .select((!self.search_results.is_empty()).then_some(0));
                self.status = Some(if self.search_results.is_empty() {
                    "nothing found".into()
                } else {
                    format!("{} results — enter to add, / to search again", self.search_results.len())
                });
            }
            Err(e) => self.status = Some(format!("search failed: {e}")),
        }
        Ok(())
    }

    async fn add_search_result(&mut self) -> Result<()> {
        let Some(mut book) = self
            .search_state
            .selected()
            .and_then(|i| self.search_results.get(i))
            .map(|r| r.book.clone())
        else {
            return Ok(());
        };
        if book.cover_url.is_some() {
            self.engine.download_cover(&mut book).await.ok();
        }
        let saved = self.engine.save_book(&book).await?;
        self.status = Some(format!("saved {}", saved.display_title()));
        self.refresh_library().await?;
        Ok(())
    }

    async fn add_isbn(&mut self, isbn: String) -> Result<()> {
        match self.engine.lookup_isbn(&isbn).await {
            Ok(Some(mut book)) => {
                if book.cover_url.is_some() {
                    self.engine.download_cover(&mut book).await.ok();
                }
                let saved = self.engine.save_book(&book).await?;
                self.status = Some(format!("added {}", saved.display_title()));
                self.refresh_library().await?;
            }
            Ok(None) => self.status = Some(format!("no edition found for ISBN {isbn}")),
            Err(e) => self.status = Some(format!("lookup failed: {e}")),
        }
        Ok(())
    }

    async fn import_ko(&mut self, path: String) -> Result<()> {
        let p = std::path::Path::new(&path);
        match self.engine.import_koreader(p, false).await {
            Ok(report) => {
                let inserted: usize = report.imported.iter().map(|s| s.inserted).sum();
                let books = report.imported.len();
                let unmatched = report.unmatched.len();
                self.status = Some(format!(
                    "imported {inserted} highlights across {books} book(s); {unmatched} unmatched"
                ));
                self.refresh_library().await?;
                self.reload_view().await?;
            }
            Err(e) => self.status = Some(format!("import failed: {e}")),
        }
        Ok(())
    }

    // ---- list / object helpers --------------------------------------------

    fn step_library(&mut self, delta: isize) {
        if self.library.is_empty() {
            return;
        }
        let len = self.library.len() as isize;
        let cur = self.library_state.selected().unwrap_or(0) as isize;
        self.library_state
            .select(Some(((cur + delta).rem_euclid(len)) as usize));
    }

    fn step_search(&mut self, delta: isize) {
        if self.search_results.is_empty() {
            return;
        }
        let len = self.search_results.len() as isize;
        let cur = self.search_state.selected().unwrap_or(0) as isize;
        self.search_state
            .select(Some(((cur + delta).rem_euclid(len)) as usize));
    }

}

fn wrap_angle(a: f32) -> f32 {
    let tau = std::f32::consts::TAU;
    let mut a = a % tau;
    if a > std::f32::consts::PI {
        a -= tau;
    } else if a < -std::f32::consts::PI {
        a += tau;
    }
    a
}

/// The event loop: crossterm events and a 20fps animation tick, redrawing only
/// when something actually changed.
pub async fn run<B: Backend>(terminal: &mut Terminal<B>, app: &mut App) -> Result<()> {
    let mut events = EventStream::new();
    let mut ticker = tokio::time::interval(TICK);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    terminal.draw(|f| ui::draw(f, app))?;
    park_cursor(terminal)?;
    app.dirty = false;

    loop {
        tokio::select! {
            maybe_event = events.next() => {
                match maybe_event {
                    Some(Ok(Event::Key(key))) => dispatch_key(app, key).await?,
                    Some(Ok(Event::Resize(_, _))) => app.dirty = true,
                    Some(Err(e)) => return Err(e.into()),
                    None => break,
                    _ => {}
                }
            }
            _ = ticker.tick() => {
                if app.tick() {
                    app.dirty = true;
                }
            }
        }

        if app.quit {
            break;
        }
        if app.dirty {
            terminal.draw(|f| ui::draw(f, app))?;
            park_cursor(terminal)?;
            app.dirty = false;
        }
    }
    Ok(())
}

/// Park the (hidden) cursor at a fixed cell after every draw. ratatui leaves it
/// on the last-written diff cell, which moves every animation frame; some
/// terminals (kitty's `cursor_trail`) then streak a trail chasing the book.
/// Pinning it to the bottom-right corner gives the trail nowhere to wander.
fn park_cursor<B: Backend>(terminal: &mut Terminal<B>) -> Result<()> {
    let area = terminal.get_frame().area();
    if area.width == 0 || area.height == 0 {
        return Ok(());
    }
    let backend = terminal.backend_mut();
    backend.set_cursor_position(Position::new(area.width - 1, area.height - 1))?;
    backend.flush()?;
    Ok(())
}

/// Route a key: to the open text input, a pending confirmation, or the normal
/// action map — in that priority.
async fn dispatch_key(app: &mut App, key: KeyEvent) -> Result<()> {
    if key.kind == KeyEventKind::Release {
        return Ok(());
    }
    if app.note_editor.is_some() {
        app.on_editor_key(key).await?;
    } else if app.input.is_some() {
        app.on_input_key(key).await?;
    } else if app.confirm.is_some() {
        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => app.resolve_confirm(true).await?,
            _ => app.resolve_confirm(false).await?,
        }
    } else if let Some(action) = crate::event::map_key(key) {
        app.handle(action).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use readingbuddy::{EngineConfig, NewNoteInput};

    /// An engine on an in-memory database — the same trick the engine's own
    /// tests use, so nothing here touches the user's library.
    async fn test_app() -> App {
        // A unique dir per invocation: tests run in parallel and share a
        // process, so a per-pid vault would let them wipe each other's files.
        use std::sync::atomic::{AtomicUsize, Ordering};
        static SEQ: AtomicUsize = AtomicUsize::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let tmp = std::env::temp_dir()
            .join(format!("readingbuddy-tui-tests-{}-{n}", std::process::id()));
        std::fs::remove_dir_all(&tmp).ok();
        let config = EngineConfig {
            db_url: "sqlite::memory:".into(),
            images_dir: tmp.join("images"),
            vault_dir: tmp.join("vault"),
            google_api_key: None,
        };
        let engine = Engine::open(config).await.expect("engine");
        let book = engine
            .save_book(&Book {
                title: Some("Station Eleven".into()),
                authors: vec!["Emily St. John Mandel".into()],
                page_count: Some(333),
                current_page: Some(120),
                isbn_13: Some("9781447268963".into()),
                ..Book::default()
            })
            .await
            .expect("save");
        let id = book.id.expect("id");
        // Seed one of each so the tab lists render with content.
        engine
            .storage
            .insert_highlight(
                id,
                &readingbuddy::storage::NewHighlight {
                    text: "survival is insufficient".into(),
                    chapter: Some("1".into()),
                    page: Some(58),
                    pos0: Some("/body/p[1]".into()),
                    pos1: None,
                    ko_datetime: Some("2026-01-01 10:00:00".into()),
                    color: None,
                    note: None,
                    source: "koreader".into(),
                },
            )
            .await
            .expect("highlight");
        engine
            .storage
            .insert_flashcard(id, None, "insufficient", Some("ch1"))
            .await
            .expect("card");
        engine
            .create_note(NewNoteInput {
                book_id: Some(id),
                page: Some(120),
                body: "A note about the [[Symphony]].".into(),
                ..NewNoteInput::default()
            })
            .await
            .expect("note");
        App::new(engine).await.expect("app")
    }

    /// Draw every screen and every book tab at sizes from a full terminal down
    /// to a pane too small to hold anything. Layout arithmetic is the likeliest
    /// place to panic, and a panic here would wreck the user's tmux pane.
    #[tokio::test]
    async fn every_screen_draws_at_every_size() {
        let mut app = test_app().await;
        let book = app.library.first().cloned().expect("seeded book");
        app.open_book(book).await.expect("open");

        for (w, h) in [(120, 40), (80, 24), (40, 30), (30, 30), (20, 8), (4, 2), (1, 1)] {
            let mut terminal =
                ratatui::Terminal::new(TestBackend::new(w, h)).expect("terminal");
            for screen in [
                Screen::Menu,
                Screen::Library,
                Screen::Book,
                Screen::Search,
                Screen::Settings,
            ] {
                app.screen = screen;
                for tab in [BookTab::Info, BookTab::Notes, BookTab::Highlights, BookTab::Cards] {
                    app.book_tab = tab;
                    // Draw both the section menu and the entered section.
                    for in_section in [false, true] {
                        app.in_section = in_section;
                        app.clamp_tab_selection();
                        for options in [false, true] {
                            app.show_options = options;
                            terminal.draw(|f| ui::draw(f, &mut app)).expect("draw");
                        }
                    }
                }
            }
            // With a text input and a confirmation overlaid.
            app.screen = Screen::Search;
            app.start_input(InputContext::SearchQuery, "search", "pachinko");
            terminal.draw(|f| ui::draw(f, &mut app)).expect("draw input");
            app.input = None;
            app.screen = Screen::Library;
            app.confirm = Some(Confirm::RemoveBook { id: 1, title: "X".into() });
            terminal.draw(|f| ui::draw(f, &mut app)).expect("draw confirm");
            app.confirm = None;
            // With the note editor open over the book view.
            app.screen = Screen::Book;
            app.note_editor = Some(NoteDraft {
                target: NoteTarget::New {
                    book_id: Some(1),
                    page: Some(1),
                    location: None,
                    highlight_id: None,
                },
                editor: TextEditor::new("a line\nanother line"),
            });
            terminal.draw(|f| ui::draw(f, &mut app)).expect("draw editor");
            app.note_editor = None;
        }
    }

    #[tokio::test]
    async fn menu_selects_a_section_then_the_section_navigates() {
        let mut app = test_app().await;
        let book = app.library.first().cloned().expect("seeded book");
        app.open_book(book).await.expect("open");
        assert_eq!(app.book_tab, BookTab::Info);
        assert!(!app.in_section, "opens onto the section menu");

        // Down moves the menu highlight; the list isn't entered yet.
        app.handle(Action::Down).await.expect("menu down");
        assert_eq!(app.book_tab, BookTab::Notes);
        assert!(!app.in_section);

        // Enter opens the Notes section; the seeded note gets a selection.
        app.handle(Action::Select).await.expect("enter section");
        assert!(app.in_section);
        assert_eq!(app.tab_state.selected(), Some(0));

        // Now Down moves the list (one note, wraps to itself).
        app.handle(Action::Down).await.expect("list down");
        assert_eq!(app.tab_state.selected(), Some(0));

        // Esc leaves the section back to the menu, not the library.
        app.handle(Action::Back).await.expect("back to menu");
        assert!(!app.in_section);
        assert_eq!(app.screen, Screen::Book);

        // Esc again leaves the book.
        app.handle(Action::Back).await.expect("back to library");
        assert_eq!(app.screen, Screen::Library);
    }

    #[tokio::test]
    async fn progress_input_updates_the_book() {
        let mut app = test_app().await;
        let book = app.library.first().cloned().expect("seeded book");
        app.open_book(book).await.expect("open");

        app.handle(Action::EditProgress).await.expect("open input");
        assert!(app.input.is_some());
        for c in "200".chars() {
            app.on_input_key(KeyEvent::from(KeyCode::Char(c)))
                .await
                .expect("type");
        }
        app.on_input_key(KeyEvent::from(KeyCode::Enter))
            .await
            .expect("commit");
        assert!(app.input.is_none());
        assert_eq!(app.view.as_ref().unwrap().book.current_page, Some(200));
    }

    #[tokio::test]
    async fn new_note_asks_for_a_page_then_saves() {
        let mut app = test_app().await;
        let book = app.library.first().cloned().expect("seeded book");
        app.open_book(book).await.expect("open");
        let before = app.view.as_ref().unwrap().notes.len();

        // `n` opens the editor; save transitions to the page prompt.
        app.handle(Action::NewNote).await.expect("open editor");
        assert!(app.note_editor.is_some());
        for c in "fresh thought".chars() {
            app.on_editor_key(KeyEvent::from(KeyCode::Char(c))).await.expect("type");
        }
        app.on_editor_key(KeyEvent::from(KeyCode::Enter)).await.expect("finish body");
        assert!(app.note_editor.is_none());
        assert!(app.input.is_some(), "asks for a page");

        for c in "58".chars() {
            app.on_input_key(KeyEvent::from(KeyCode::Char(c))).await.expect("type page");
        }
        app.on_input_key(KeyEvent::from(KeyCode::Enter)).await.expect("commit page");

        let notes = &app.view.as_ref().unwrap().notes;
        assert_eq!(notes.len(), before + 1);
        assert!(
            notes.iter().any(|n| n.page == Some(58)),
            "the new note carries the entered page"
        );
        assert!(app.in_section && app.book_tab == BookTab::Notes);
    }

    #[tokio::test]
    async fn empty_page_prompt_saves_without_an_anchor() {
        let mut app = test_app().await;
        let book = app.library.first().cloned().expect("seeded book");
        app.open_book(book).await.expect("open");

        app.handle(Action::NewNote).await.expect("open editor");
        app.on_editor_key(KeyEvent::from(KeyCode::Char('x'))).await.expect("type");
        app.on_editor_key(KeyEvent::from(KeyCode::Enter)).await.expect("finish body");
        // Empty page + Enter → saved with no page.
        app.on_input_key(KeyEvent::from(KeyCode::Enter)).await.expect("skip page");

        assert!(app.input.is_none());
        assert!(
            app.view.as_ref().unwrap().notes.iter().any(|n| n.page.is_none()),
            "the new note saved without a page"
        );
    }

    #[tokio::test]
    async fn editor_updates_a_body_preserving_frontmatter() {
        let mut app = test_app().await;
        let book = app.library.first().cloned().expect("seeded book");
        app.open_book(book).await.expect("open");
        app.book_tab = BookTab::Notes;
        app.in_section = true;
        app.clamp_tab_selection();

        let note = app.view.as_ref().unwrap().notes[0].clone();
        let path = app.engine.config.vault_dir.join(&note.file_path);
        let original = std::fs::read_to_string(&path).unwrap();
        assert!(original.contains("page: 120"));

        // Enter inside the Notes section opens the editor on the body; retype it.
        app.handle(Action::Select).await.expect("open editor");
        assert!(app.note_editor.is_some());
        // Clear the loaded body, then type a new one.
        if let Some(draft) = app.note_editor.as_mut() {
            for _ in 0..200 {
                draft.editor.backspace();
            }
        }
        for c in "rewritten body".chars() {
            app.on_editor_key(KeyEvent::from(KeyCode::Char(c))).await.expect("type");
        }
        app.on_editor_key(KeyEvent::from(KeyCode::Enter)).await.expect("save");

        let updated = std::fs::read_to_string(&path).unwrap();
        assert!(updated.contains("page: 120"), "frontmatter kept");
        assert!(updated.contains("rewritten body"));
        assert!(!updated.contains("Symphony"), "old body gone");
    }

    #[tokio::test]
    async fn removing_a_book_needs_confirmation() {
        let mut app = test_app().await;
        app.screen = Screen::Library;
        app.library_state.select(Some(0));
        assert_eq!(app.library.len(), 1);

        app.handle(Action::Delete).await.expect("ask");
        assert!(app.confirm.is_some());
        app.resolve_confirm(false).await.expect("keep");
        assert_eq!(app.library.len(), 1);

        app.handle(Action::Delete).await.expect("ask again");
        app.resolve_confirm(true).await.expect("remove");
        assert!(app.library.is_empty());
    }

    #[tokio::test]
    async fn spin_can_be_paused_and_resumed() {
        let mut app = test_app().await;
        let book = app.library.first().cloned().expect("seeded book");
        app.open_book(book).await.expect("open");
        assert!(app.spinning);

        app.handle(Action::ToggleSpin).await.expect("pause");
        assert!(!app.spinning);
        let held = app.params.pose.yaw;
        assert!(!app.tick(), "a paused book doesn't move");
        assert_eq!(app.params.pose.yaw, held);

        app.handle(Action::ToggleSpin).await.expect("resume");
        assert!(app.spinning);
        assert!(app.tick(), "a resumed book moves");
        assert!((app.params.pose.yaw - held).abs() < 0.05, "resumes smoothly");
    }

    #[tokio::test]
    async fn the_spin_goes_all_the_way_round() {
        let mut app = test_app().await;
        let book = app.library.first().cloned().expect("seeded book");
        app.open_book(book).await.expect("open");

        let mut seen_back = false;
        let mut seen_front = false;
        for _ in 0..2000 {
            app.tick();
            let yaw = app.params.pose.yaw.abs();
            seen_back |= yaw > 2.4;
            seen_front |= yaw < 0.2;
            assert!(yaw <= std::f32::consts::PI + 1e-3, "yaw escaped: {yaw}");
        }
        assert!(seen_back && seen_front, "never completed a turn");
    }

    #[tokio::test]
    async fn quitting_and_navigation_move_between_screens() {
        let mut app = test_app().await;
        assert_eq!(app.screen, Screen::Menu);
        app.menu_index = 0; // Library
        app.handle(Action::Select).await.expect("open library");
        assert_eq!(app.screen, Screen::Library);
        app.handle(Action::Select).await.expect("open book");
        assert_eq!(app.screen, Screen::Book);
        app.handle(Action::Back).await.expect("back");
        assert_eq!(app.screen, Screen::Library);
        app.handle(Action::Menu).await.expect("menu");
        assert_eq!(app.screen, Screen::Menu);
        app.handle(Action::Quit).await.expect("quit");
        assert!(app.quit);
    }

    #[test]
    #[ignore = "development aid: prints the composed layout"]
    fn print_layout() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let mut app = rt.block_on(test_app());
        let book = app.library.first().cloned().unwrap();
        rt.block_on(app.open_book(book)).unwrap();
        app.show_options = true;
        for (w, h) in [(110, 32), (44, 26)] {
            let mut t = ratatui::Terminal::new(TestBackend::new(w, h)).unwrap();
            t.draw(|f| ui::draw(f, &mut app)).unwrap();
            println!("=== {w}x{h} ===");
            let buf = t.backend().buffer();
            for y in 0..h {
                let row: String = (0..w).map(|x| {
                    let s = buf[(x, y)].symbol();
                    match s { "▀" | "▄" => '#', " " => ' ', _ => s.chars().next().unwrap_or(' ') }
                }).collect();
                println!("|{row}|");
            }
        }
    }

    #[test]
    fn angles_wrap_into_a_single_turn() {
        let a = wrap_angle(std::f32::consts::TAU + 0.5);
        assert!((a - 0.5).abs() < 1e-4, "{a}");
        let b = wrap_angle(-std::f32::consts::TAU - 0.5);
        assert!((b + 0.5).abs() < 1e-4, "{b}");
    }
}
