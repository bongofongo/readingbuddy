//! Application state and the async event loop.

use std::time::{Duration, Instant};

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
use crate::render3d::{Caps, GlyphSet, Pose, RenderMode, RenderParams, Scene};
use crate::theme;
use crate::ui;
use crate::ui::input::InputState;
use crate::ui::textedit::TextEditor;

/// Idle spin: a full turn about the bottom of the spine, so the book sweeps
/// round like a slow top. Radians per tick at 20fps — about 27s for 360°.
pub const SPIN_SPEED: f32 = 0.01164;
/// The pitch nods gently while the book turns, on its own slower cycle.
const NOD: f32 = 0.06;
const NOD_SPEED: f32 = 0.011;
pub const TICK: Duration = Duration::from_millis(50);

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
    (
        MenuItem::Library,
        "Library",
        "browse everything you've saved",
    ),
    (
        MenuItem::Continue,
        "Continue reading",
        "jump to the most recent book",
    ),
    (
        MenuItem::Search,
        "Search books",
        "find and add from OpenLibrary + Google Books",
    ),
    (
        MenuItem::AddIsbn,
        "Add by ISBN",
        "look up a single edition and save it",
    ),
    (
        MenuItem::ImportKo,
        "Import KOReader",
        "pull highlights from a .sdr / library path",
    ),
    (
        MenuItem::Cards,
        "Flashcards",
        "count the cards waiting for export",
    ),
    (
        MenuItem::Settings,
        "Settings",
        "data locations, API key, glyph set",
    ),
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

/// The stage of the Google Books API-key modal.
pub enum ApiKeyStage {
    /// Typing/pasting the key (held raw; rendered as asterisks).
    Entry { value: String },
    /// The key is being live-checked against Google.
    Verifying,
    /// The outcome page: `ok` picks the success/failure copy.
    Result { ok: bool, message: String },
}

/// The API-key modal, opened from the settings screen.
pub struct ApiKeyModal {
    pub stage: ApiKeyStage,
}

/// A pending yes/no confirmation.
#[derive(Debug, Clone)]
pub enum Confirm {
    RemoveBook {
        id: i64,
        title: String,
    },
    /// Delete the selected note (held whole so we still have its file path).
    DeleteNote(NoteRecord),
    /// Discard the in-progress editor draft stashed in `pending_discard`.
    DiscardDraft,
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
    /// Which presentation path the book view uses, resolved at startup from
    /// [`Caps`] and `--render`.
    pub render_mode: RenderMode,
    /// What the terminal told us it can do. `Copy`, because `present_book`
    /// reads it out of `App` before taking the `&mut app.scene` borrow.
    pub caps: Caps,
    /// Pixel-path state. A direct field, not behind a method: `present_book`
    /// borrows it disjointly alongside `scene` and `view`.
    pub rich: crate::render3d::present::RichState,
    /// The terminal's size at the last draw, so `redraw` can notice a resize.
    /// `None` until the first frame, which counts as a resize — nothing has been
    /// transmitted yet, so invalidating costs nothing.
    pub term_size: Option<(u16, u16)>,
    /// Frame cost accounting. Inert unless `--perf-log` gave it somewhere to
    /// write, so the normal session pays a relaxed atomic load per frame.
    pub perf: crate::perf::Recorder,
    /// A terminal round-trip measured out of band, attached to the next frame
    /// record. **Only the bench sets this** — sampling it needs exclusive stdin,
    /// which the live loop's `EventStream` owns.
    pub rtt_sample: Option<crate::render3d::caps::Rtt>,
    /// The user's book-view layout tweaks (pane rotation + divider slide).
    pub layout: crate::ui::LayoutPrefs,
    /// The ambient background layer for the non-book screens. Off by default;
    /// the persisted setting is applied at startup.
    pub ambient: crate::ambient::Ambient,
    /// Idle spin on/off.
    pub spinning: bool,
    pub show_options: bool,
    pub status: Option<String>,
    pub input: Option<Input>,
    pub note_editor: Option<NoteDraft>,
    pub pending_note: Option<PendingNote>,
    /// A non-blank draft set aside while its discard is confirmed.
    pub pending_discard: Option<NoteDraft>,
    /// The Google Books API-key modal, when open.
    pub api_key: Option<ApiKeyModal>,
    /// A submitted key awaiting its (async) live-check; the event loop drains
    /// this after drawing the "verifying" frame.
    pub pending_verify: Option<String>,
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
            render_mode: RenderMode::default(),
            caps: Caps::default(),
            rich: crate::render3d::present::RichState::default(),
            term_size: None,
            perf: crate::perf::Recorder::disabled(crate::perf::Meter::new()),
            rtt_sample: None,
            layout: crate::ui::LayoutPrefs::default(),
            ambient: crate::ambient::Ambient::default(),
            spinning: true,
            show_options: false,
            status: None,
            input: None,
            note_editor: None,
            pending_note: None,
            pending_discard: None,
            api_key: None,
            pending_verify: None,
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

    /// Choose the book-view presentation path (glyph vs rich). Set once at
    /// startup from the probed [`Caps`] and `--render`.
    pub fn set_render_mode(&mut self, mode: RenderMode) {
        self.render_mode = mode;
        self.dirty = true;
    }

    /// Record what the terminal reported. Tests and `--dump-frame` never call
    /// this, so they keep [`Caps::default`] and can never emit graphics.
    pub fn set_caps(&mut self, caps: Caps) {
        self.caps = caps;
        self.rich.set_caps(caps);
        self.dirty = true;
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
        if self
            .library_state
            .selected()
            .is_some_and(|i| i >= self.library.len())
        {
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

    /// Swap between the pixel and glyph renderers.
    ///
    /// Leaving rich mode must delete the image explicitly: the placeholder
    /// cells stop being written, but the image the terminal is holding would
    /// otherwise stay composited underneath the block glyphs.
    fn toggle_renderer(&mut self) {
        if self.render_mode.is_rich() {
            self.rich.drop_image();
            self.render_mode = RenderMode::Glyph;
            self.status = Some("renderer: glyphs".into());
        } else if self.caps.supports_pixels() {
            // Always the hybrid: `RichAlways` is a diagnostic reached by flag,
            // never something a keypress can land the user in.
            self.render_mode = RenderMode::Rich;
            self.status = Some("renderer: pixels when still".into());
        } else {
            self.status = Some("this terminal has no pixel protocol".into());
        }
        self.dirty = true;
    }

    fn reset_pose(&mut self) {
        self.params.pose = Pose::default();
        self.base_pitch = self.params.pose.pitch;
        self.phase = 0.0;
    }

    /// Rotate the book-view panes one quarter-turn clockwise. With two panes
    /// this walks the single divider around; a fourth turn returns to the
    /// aspect default.
    fn rotate_layout(&mut self) {
        self.layout.rotation = (self.layout.rotation + 1) % 4;
        self.status = Some("panes rotated ↻".into());
    }

    /// Show or hide the section pane. It comes back in wherever the layout
    /// currently puts it — `t` decides the side, tab decides whether it is there.
    fn toggle_panel(&mut self) {
        self.layout.panel = !self.layout.panel;
        self.status = Some(if self.layout.panel {
            "sections shown".into()
        } else {
            "sections hidden — t to bring them back".into()
        });
    }

    /// Bring the section pane back for a key that has nothing to act on without
    /// it: asking for the menu is asking for the pane.
    fn ensure_panel(&mut self) {
        if !self.layout.panel {
            self.layout.panel = true;
            self.status = None;
        }
    }

    /// Slide the pane divider, growing (positive) or shrinking the object's
    /// share. The bias is stored as an offset from the default; rendering
    /// clamps it so neither pane can vanish.
    fn slide_divider(&mut self, delta: f32) {
        self.layout.divider_bias = (self.layout.divider_bias + delta).clamp(-1.0, 1.0);
        self.status = Some("panes resized".into());
    }

    /// Whether the book is animating right now.
    ///
    /// The single source of truth for "moving": [`App::tick`] uses it to decide
    /// whether to advance the pose, and the draw path hands it to the renderer,
    /// which is what lets the hybrid switch to pixels the instant the book
    /// parks. The pixel path can also *infer* motion by comparing consecutive
    /// poses, but that inference is blind to exactly this case — a parked book
    /// stops redrawing at all, so there is no next draw to compare against, and
    /// the crisp frame would never be transmitted.
    pub fn animating(&self) -> bool {
        self.spinning
            && self.screen == Screen::Book
            && self.input.is_none()
            && self.note_editor.is_none()
            && self.api_key.is_none()
    }

    /// Whether the ambient layer is on screen right now.
    ///
    /// Deliberately **not** folded into [`App::animating`]. That one means "the
    /// book is moving" and the renderer believes it: widen it and a parked book
    /// reports `moving = true` forever, so the hybrid never transmits its crisp
    /// frame. The two are mutually exclusive by construction — ambient never
    /// runs on `Screen::Book` — but they are kept apart because conflating them
    /// is precisely the regression that would not show up in a test of either.
    pub fn ambient_visible(&self) -> bool {
        self.ambient.motif != crate::ambient::Motif::Off && self.screen != Screen::Book
    }

    /// Whether the ambient layer is *moving*. Separate from
    /// [`App::ambient_visible`] on purpose: under a modal the field keeps being
    /// drawn but stops drifting, so opening a search box calms the background
    /// instead of making it vanish — and a frozen field asks for no redraws.
    fn ambient_animating(&self) -> bool {
        self.ambient_visible()
            && self.input.is_none()
            && self.note_editor.is_none()
            && self.api_key.is_none()
    }

    /// Advance the idle animations. Returns true when something moved.
    pub fn tick(&mut self) -> bool {
        // Both arms are evaluated before the `||`. Today the two are mutually
        // exclusive — ambient never runs on `Screen::Book`, the book only
        // animates there — so short-circuiting would happen to be harmless;
        // written this way so it stays harmless if that ever stops being true.
        let book = self.tick_book();
        let ambient = self.tick_ambient();
        book || ambient
    }

    /// Advance the book's spin + nod.
    fn tick_book(&mut self) -> bool {
        if !self.animating() {
            return false;
        }
        self.phase += NOD_SPEED;
        self.params.pose.yaw = wrap_angle(self.params.pose.yaw + SPIN_SPEED);
        self.params.pose.pitch = self.base_pitch + self.phase.sin() * NOD;
        true
    }

    /// Advance the ambient clock. Returns true only on the ticks where the
    /// layer's own (much slower) frame rate actually produces a new frame.
    fn tick_ambient(&mut self) -> bool {
        if !self.ambient_animating() {
            return false;
        }
        self.ambient.advance(TICK.as_secs_f32())
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
            (Screen::Settings, Action::Query) => self.start_input(
                InputContext::AccentHex,
                "accent #RRGGBB",
                &theme::to_hex(theme::accent_rgb()),
            ),
            (Screen::Settings, Action::EditApiKey) => self.open_api_key(),
            (Screen::Settings, Action::CycleAmbient) => self.cycle_ambient(),
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
            Action::RotateLayout => self.rotate_layout(),
            Action::TogglePanel => self.toggle_panel(),
            Action::ToggleRenderer => self.toggle_renderer(),
            Action::GrowBook => self.slide_divider(crate::ui::DIVIDER_STEP),
            Action::ShrinkBook => self.slide_divider(-crate::ui::DIVIDER_STEP),
            Action::NewNote => self.new_note(false),
            Action::Delete => self.ask_delete_selected_note(),
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
                self.ensure_panel();
                if self.in_section {
                    self.step_tab(-1);
                } else {
                    self.cycle_tab(-1);
                }
            }
            Action::Down => {
                self.ensure_panel();
                if self.in_section {
                    self.step_tab(1);
                } else {
                    self.cycle_tab(1);
                }
            }
            Action::PrevTab => {
                self.ensure_panel();
                if !self.in_section {
                    self.cycle_tab(-1);
                }
            }
            // Enter / right: open the highlighted section, or act on a row.
            Action::Select | Action::Right => {
                self.ensure_panel();
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
                self.status = Some(format!(
                    "{n} flashcard candidate(s) pending — open a book's Cards tab to export"
                ));
            }
            MenuItem::Settings => self.screen = Screen::Settings,
            MenuItem::Quit => self.quit = true,
        }
        Ok(())
    }

    // ---- book-view sub-actions --------------------------------------------

    fn cycle_tab(&mut self, delta: isize) {
        let cur = BOOK_TABS
            .iter()
            .position(|(t, _)| *t == self.book_tab)
            .unwrap_or(0);
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
        self.tab_state
            .select(Some((cur + delta).rem_euclid(len as isize) as usize));
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
            let Some(h) = self
                .tab_state
                .selected()
                .and_then(|i| view.highlights.get(i))
            else {
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

    /// Ask to delete the highlighted note — only in the open Notes section.
    fn ask_delete_selected_note(&mut self) {
        if self.book_tab != BookTab::Notes || !self.in_section {
            self.dirty = false;
            return;
        }
        let note = self
            .tab_state
            .selected()
            .and_then(|i| self.view.as_ref().and_then(|v| v.notes.get(i)))
            .cloned();
        match note {
            Some(note) => self.confirm = Some(Confirm::DeleteNote(note)),
            None => self.dirty = false,
        }
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
        let newline_chord = key
            .modifiers
            .intersects(KeyModifiers::SHIFT | KeyModifiers::ALT);
        match key.code {
            KeyCode::Esc => {
                let editing_saved = matches!(draft.target, NoteTarget::Edit(_));
                if editing_saved {
                    // Esc on an existing note just closes the editor — the saved
                    // note is left as-is. Deletion is only ever via `d`.
                    self.note_editor = None;
                    self.status = Some("edit cancelled".into());
                } else if draft.editor.is_blank() {
                    // A brand-new note with nothing written — no need to ask.
                    self.note_editor = None;
                    self.status = Some("note discarded".into());
                } else {
                    // A new note with real text: ask before throwing it away.
                    self.pending_discard = self.note_editor.take();
                    self.confirm = Some(Confirm::DiscardDraft);
                    self.status = Some("discard note?  y / n".into());
                }
            }
            KeyCode::Enter if newline_chord => draft.editor.newline(),
            KeyCode::Char('j') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                draft.editor.newline()
            }
            KeyCode::Enter => self.save_note_editor().await?,
            KeyCode::Tab => draft.editor.insert('\t'),
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
        self.persist_config();
        self.status = Some(format!("accent: {name} {}", theme::to_hex(rgb)));
    }

    /// Apply an arbitrary accent (from the hex box) and persist it.
    fn set_accent_rgb(&mut self, rgb: u32) {
        theme::set_accent(rgb);
        self.accent_idx = theme::PRESETS
            .iter()
            .position(|(_, p)| *p == rgb)
            .unwrap_or(self.accent_idx);
        self.persist_config();
        self.status = Some(format!("accent: {}", theme::to_hex(rgb)));
    }

    /// Cycle the ambient motif, apply it live, persist.
    fn cycle_ambient(&mut self) {
        let motif = self.ambient.motif.next();
        self.ambient = crate::ambient::Ambient::new(motif);
        self.persist_config();
        self.status = Some(format!("ambient: {}", motif.label()));
    }

    /// Write the TUI config file from **live state**, every field of it.
    ///
    /// Not a struct literal of the one field being changed: `config::save`
    /// serializes the whole struct over the file, so a literal silently blanks
    /// every other setting — change the accent and the ambient motif is gone.
    /// That is the same overwrite hazard `config.rs` documents between the two
    /// config *files*, and it is just as easy to reproduce inside one of them.
    /// A failure is a status warning, never fatal.
    fn persist_config(&mut self) {
        let cfg = self.config_snapshot();
        if let Err(e) = config::save(&cfg) {
            self.status = Some(format!("could not save settings: {e:#}"));
        }
    }

    /// Every persisted TUI setting, read from live state. Split out from
    /// [`App::persist_config`] so the "no field gets blanked" invariant is
    /// testable without writing to the real config file.
    fn config_snapshot(&self) -> TuiConfig {
        TuiConfig {
            accent: Some(theme::to_hex(theme::accent_rgb())),
            ambient: Some(self.ambient.motif.label().to_string()),
        }
    }

    // ---- google api key ----------------------------------------------------

    /// Open the API-key modal onto a blank entry field.
    fn open_api_key(&mut self) {
        self.api_key = Some(ApiKeyModal {
            stage: ApiKeyStage::Entry {
                value: String::new(),
            },
        });
        self.status = None;
        self.dirty = true;
    }

    /// Route a key to the open API-key modal. Verification itself is deferred:
    /// on submit we stash the key in `pending_verify` and let the event loop run
    /// the network check after it has drawn the "verifying" frame.
    fn on_api_key_key(&mut self, key: KeyEvent) {
        self.dirty = true;
        let Some(mut modal) = self.api_key.take() else {
            return;
        };
        let mut closed = false;
        let mut next_stage = None;
        match &mut modal.stage {
            ApiKeyStage::Entry { value } => {
                let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
                match key.code {
                    KeyCode::Esc => closed = true,
                    KeyCode::Enter => {
                        let submitted = value.trim().to_string();
                        if submitted.is_empty() {
                            self.status = Some("paste a key first — Ctrl+V".into());
                        } else {
                            self.pending_verify = Some(submitted);
                            next_stage = Some(ApiKeyStage::Verifying);
                        }
                    }
                    KeyCode::Char('v') if ctrl => match crate::clipboard::read() {
                        Ok(text) => value.push_str(&text),
                        Err(e) => self.status = Some(format!("clipboard unavailable: {e:#}")),
                    },
                    KeyCode::Backspace => {
                        value.pop();
                    }
                    // Typing is allowed too; only bare characters (no Ctrl).
                    KeyCode::Char(c) if !ctrl => value.push(c),
                    _ => {}
                }
            }
            // Keys are inert while the network check is in flight.
            ApiKeyStage::Verifying => {}
            ApiKeyStage::Result { ok, .. } => {
                if *ok {
                    // Success: any key dismisses.
                    closed = true;
                } else {
                    match key.code {
                        KeyCode::Esc => closed = true,
                        KeyCode::Enter | KeyCode::Char('r') => {
                            next_stage = Some(ApiKeyStage::Entry {
                                value: String::new(),
                            });
                        }
                        _ => {}
                    }
                }
            }
        }
        if let Some(stage) = next_stage {
            modal.stage = stage;
        }
        if !closed {
            self.api_key = Some(modal);
        }
    }

    /// Run the deferred live-check, persist + apply a good key, and move the
    /// modal to its result page. Called by the event loop, not a key handler.
    async fn finish_verify(&mut self, key: String) {
        self.dirty = true;
        let stage = match readingbuddy::verify_google_key(&key).await {
            Ok(()) => {
                self.engine.set_google_api_key(Some(key.clone()));
                match config::save_google_key(&key) {
                    Ok(_) => ApiKeyStage::Result {
                        ok: true,
                        message: "verified and saved.".into(),
                    },
                    // Live but not persisted: honest about it, key still active.
                    Err(e) => ApiKeyStage::Result {
                        ok: true,
                        message: format!("verified, but couldn't save it: {e:#}"),
                    },
                }
            }
            Err(reason) => ApiKeyStage::Result {
                ok: false,
                message: reason,
            },
        };
        if let Some(modal) = self.api_key.as_mut() {
            modal.stage = stage;
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
        match confirm {
            Some(Confirm::RemoveBook { id, title }) if yes => {
                self.engine.delete_book(id).await?;
                self.refresh_library().await?;
                self.status = Some(format!("removed {title}"));
            }
            Some(Confirm::DeleteNote(note)) if yes => {
                self.engine.delete_note(&note).await?;
                self.status = Some(format!("deleted “{}”", note.title));
                self.reload_view().await?;
                self.clamp_tab_selection();
            }
            Some(Confirm::DiscardDraft) if yes => {
                self.pending_discard = None;
                self.status = Some("note discarded".into());
            }
            // Declining a discard puts the draft back in the editor.
            Some(Confirm::DiscardDraft) => {
                self.note_editor = self.pending_discard.take();
                self.status = None;
            }
            // Any other decline just keeps things as they were.
            Some(_) => self.status = Some("kept.".into()),
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
                self.engine
                    .storage
                    .update_progress(id, Some(page), None)
                    .await?;
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
                // A dead provider means fewer results, and this used to say
                // nothing at all about it — the user saw a short list with no
                // hint that half the sources never answered. `failed_providers`
                // is exactly what the typed diagnostics bought.
                let degraded = outcome.failed_providers();
                self.search_results = outcome.results;
                self.search_state
                    .select((!self.search_results.is_empty()).then_some(0));

                let names: Vec<String> = degraded.iter().map(|p| p.to_string()).collect();
                let suffix = if names.is_empty() {
                    String::new()
                } else {
                    format!(" (no answer from {})", names.join(", "))
                };

                self.status = Some(if self.search_results.is_empty() {
                    format!("nothing found{suffix}")
                } else {
                    format!(
                        "{} results — enter to add, / to search again{suffix}",
                        self.search_results.len()
                    )
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

    redraw(terminal, app)?;

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
            redraw(terminal, app)?;
        }
        // A submitted key: the frame above showed "verifying", so run the
        // (blocking-ish) network check now and redraw its result immediately.
        if let Some(key) = app.pending_verify.take() {
            app.finish_verify(key).await;
            redraw(terminal, app)?;
        }
    }
    Ok(())
}

/// Draw one frame, and account for what it cost.
///
/// Every draw goes through here, which is what makes `params.moving`
/// authoritative: the renderer is told whether the book is animating rather than
/// having to guess from pose history, and the guess is wrong in exactly the case
/// the hybrid depends on (a parked book stops redrawing, so there is no next
/// frame to infer from).
pub fn redraw<B: Backend>(terminal: &mut Terminal<B>, app: &mut App) -> Result<()> {
    app.params.moving = Some(app.animating());
    let rtt = app.rtt_sample.take();
    let term = terminal.size()?;
    // A resize invalidates whatever image the terminal is holding — see
    // `RichState::resized`. This is the one place that sees every draw, and the
    // only place that sees the terminal's size, so it is where the two meet.
    if app.term_size != Some((term.width, term.height)) {
        app.term_size = Some((term.width, term.height));
        app.rich.resized();
    }
    app.perf.frame_begin();
    let started = Instant::now();
    terminal.draw(|f| ui::draw(f, app))?;
    let draw = started.elapsed();
    park_cursor(terminal)?;
    app.perf.frame_end(crate::perf::FrameCtx {
        mode: app.render_mode.label(),
        moving: app.animating(),
        term_cols: term.width,
        term_rows: term.height,
        draw,
        // The terminal round-trip needs exclusive stdin, which `EventStream`
        // owns in a live session — so this is always `None` there, and only the
        // bench (which owns stdin) ever leaves a sample here.
        rtt_kitty: rtt.and_then(|r| r.kitty),
        rtt_tmux: rtt.and_then(|r| r.tmux),
    });
    app.dirty = false;
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
    } else if app.api_key.is_some() {
        app.on_api_key_key(key);
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
        let tmp =
            std::env::temp_dir().join(format!("readingbuddy-tui-tests-{}-{n}", std::process::id()));
        std::fs::remove_dir_all(&tmp).ok();
        let config = EngineConfig {
            db_url: "sqlite::memory:".into(),
            images_dir: tmp.join("images"),
            vault_dir: tmp.join("vault"),
            log_dir: tmp.join("logs"),
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
    /// Bytes ratatui actually put on the wire for one glyph spin.
    ///
    /// The claim "block glyphs are cheap" had no harness behind it until now —
    /// every measurement we had was of the *pixel* path, because the glyph
    /// path's bytes are produced by ratatui's diff, which the app never sees.
    /// Counting at the backend's writer is what makes it measurable, and this is
    /// the baseline the hybrid's motion frames have to match.
    async fn glyph_spin_bytes(w: u16, h: u16, ticks: u32) -> u64 {
        use crate::perf::{ByteClass, CountingWriter, Meter};
        use ratatui::backend::CrosstermBackend;
        use ratatui::{TerminalOptions, Viewport};

        let mut app = test_app().await;
        let book = app.library.first().cloned().expect("seeded book");
        app.open_book(book).await.expect("open");
        app.render_mode = RenderMode::Glyph;
        app.spinning = true;

        let meter = Meter::new();
        let backend = CrosstermBackend::new(CountingWriter::new(
            Vec::new(),
            meter.clone(),
            ByteClass::Text,
        ));
        // A fixed viewport, so the terminal never asks the OS for a window size
        // it does not have in a test process.
        let mut terminal = ratatui::Terminal::with_options(
            backend,
            TerminalOptions {
                viewport: Viewport::Fixed(ratatui::layout::Rect::new(0, 0, w, h)),
            },
        )
        .expect("terminal");

        redraw(&mut terminal, &mut app).expect("first draw");
        let start = meter.snapshot();
        for _ in 0..ticks {
            app.tick();
            redraw(&mut terminal, &mut app).expect("draw");
        }
        meter.snapshot().since(start).text
    }

    /// Bytes ratatui put on the wire for `ticks` ticks of the ambient layer,
    /// parked on the menu.
    ///
    /// The same harness as [`glyph_spin_bytes`], and for the same reason: this
    /// repo's stated lesson is that its own numbers did not show the 7.1 MB/s
    /// disaster, because the cost lived somewhere nothing was counting. A
    /// background that repaints scattered cells across the *whole* terminal is
    /// exactly the shape of thing that could repeat it, so it gets measured
    /// rather than reasoned about.
    ///
    /// `Motif::Off` is a meaningful argument here: it never redraws at all, so
    /// it reads 0 and makes the layer's cost a subtraction.
    async fn ambient_bytes(motif: crate::ambient::Motif, w: u16, h: u16, ticks: u32) -> u64 {
        use crate::perf::{ByteClass, CountingWriter, Meter};
        use ratatui::backend::CrosstermBackend;
        use ratatui::{TerminalOptions, Viewport};

        let mut app = test_app().await;
        app.screen = Screen::Menu;
        app.ambient = crate::ambient::Ambient::new(motif);

        let meter = Meter::new();
        let backend = CrosstermBackend::new(CountingWriter::new(
            Vec::new(),
            meter.clone(),
            ByteClass::Text,
        ));
        let mut terminal = ratatui::Terminal::with_options(
            backend,
            TerminalOptions {
                viewport: Viewport::Fixed(ratatui::layout::Rect::new(0, 0, w, h)),
            },
        )
        .expect("terminal");

        redraw(&mut terminal, &mut app).expect("first draw");
        let start = meter.snapshot();
        for _ in 0..ticks {
            // Exactly the live loop's rule: only a tick that reports a change
            // costs a redraw. Measuring an unconditional redraw would measure a
            // program we do not ship.
            if app.tick() {
                redraw(&mut terminal, &mut app).expect("draw");
            }
        }
        meter.snapshot().since(start).text
    }

    /// What the ambient layer costs on the wire, by motif and terminal size.
    ///
    /// `cargo test --release -p readingbuddy-tui -- --ignored --nocapture ambient_wire_rate`
    #[tokio::test]
    #[ignore = "timing, not correctness; run on a release build"]
    async fn ambient_wire_rate() {
        use crate::ambient::Motif;
        let ticks = 60;
        println!(
            "{:>10} {:>10} {:>10} {:>10}",
            "motif", "rect", "B/tick", "MB/s"
        );
        for motif in Motif::ALL {
            for (w, h) in [(80u16, 24u16), (120, 40), (200, 50)] {
                let bytes = ambient_bytes(motif, w, h, ticks).await;
                println!(
                    "{:>10} {:>10} {:>10} {:>10.4}",
                    motif.label(),
                    format!("{w}x{h}"),
                    bytes / ticks as u64,
                    bytes as f64 / (ticks as f64 * TICK.as_secs_f64()) / 1.0e6
                );
            }
        }
        println!("(off never redraws, so it is the baseline the others subtract from)");
    }

    #[tokio::test]
    async fn the_ambient_layer_stays_inside_its_byte_budget() {
        use crate::ambient::Motif;
        let ticks = 60; // three seconds at the 20fps tick

        // Off is the baseline: a menu with no motif is completely idle, and
        // that is the property that makes the layer opt-in rather than a tax.
        assert_eq!(
            ambient_bytes(Motif::Off, 120, 40, ticks).await,
            0,
            "an idle menu drew something"
        );

        // Measured at 120x40: motes 0.002 MB/s, contours 0.018 — against 0.131
        // for a full glyph book spin on the same pane. The ceiling keeps ~3x
        // headroom over contours on purpose: it exists to catch a change that
        // makes the layer *categorically* expensive, not to freeze a number
        // that will drift with tuning.
        //
        // It has already earned its keep once. Drawing the contour bands with a
        // brightness ramp instead of one flat ink cost 0.174 MB/s here — every
        // lit cell's averaged colour moved a fraction per frame, so ratatui's
        // diff repainted the whole terminal. Nothing looked wrong on screen.
        for motif in [Motif::Motes, Motif::Contours] {
            let bytes = ambient_bytes(motif, 120, 40, ticks).await;
            let per_second = bytes as f64 / (ticks as f64 * TICK.as_secs_f64());
            assert!(
                per_second < 0.05e6,
                "{} now costs {:.3} MB/s ({bytes} bytes over {ticks} ticks)",
                motif.label(),
                per_second / 1.0e6
            );
            assert!(bytes > 0, "{} drew nothing at all", motif.label());
        }
    }

    /// What a glyph spin actually puts on the wire, across rect sizes.
    ///
    /// The reporting counterpart to the pass/fail gate below, and the glyph
    /// analogue of `raster::wire_rate`. Reported rather than asserted because
    /// the interesting part is the *shape*: the rate plateaus once the object
    /// hits `BOOK_MAX_COLS`/`BOOK_MAX_ROWS`, since a bigger pane then grows the
    /// panel rather than the book, and only changed cells are sent.
    ///
    /// `cargo test --release -p readingbuddy-tui -- --ignored --nocapture glyph_wire_rate`
    #[tokio::test]
    #[ignore = "timing, not correctness; run on a release build"]
    async fn glyph_wire_rate() {
        let ticks = 60;
        println!("{:>10} {:>10} {:>10}", "rect", "B/frame", "MB/s");
        for (w, h) in [(30u16, 16u16), (50, 26), (80, 30), (120, 40)] {
            let bytes = glyph_spin_bytes(w, h, ticks).await;
            println!(
                "{:>10} {:>10} {:>10.3}",
                format!("{w}x{h}"),
                bytes / ticks as u64,
                bytes as f64 / (ticks as f64 * TICK.as_secs_f64()) / 1.0e6
            );
        }
        println!("(the whole terminal, not the object rect — the object is capped inside it)");
    }

    #[tokio::test]
    async fn a_glyph_spin_stays_inside_its_byte_budget() {
        // 60 ticks is three seconds at the 20fps tick. Measured: 0.088 MB/s on
        // this 50x26 rect, 0.131 MB/s on a full 120x40 pane — well under the
        // design note's 0.45 MB/s, which was an upper bound assuming a full
        // repaint, where ratatui only sends the cells that changed.
        //
        // The ceiling keeps ~5x headroom on purpose: this is here to catch a
        // change that makes the spin *categorically* more expensive, not to
        // freeze a number that will drift with font, layout and pose.
        let ticks = 60;
        let bytes = glyph_spin_bytes(50, 26, ticks).await;
        let per_second = bytes as f64 / (ticks as f64 * TICK.as_secs_f64());
        assert!(
            per_second < 0.5e6,
            "the glyph spin now costs {:.2} MB/s ({bytes} bytes over {ticks} ticks)",
            per_second / 1.0e6
        );
        assert!(bytes > 0, "the spin drew nothing at all");
    }

    #[tokio::test]
    async fn the_hybrid_costs_no_more_than_glyphs_while_moving() {
        // The whole point of the hybrid, stated as a comparison rather than a
        // constant: a moving book in rich mode must be indistinguishable from a
        // moving book in glyph mode. `caps` is left at its safe floor here, so
        // this also proves the fallback never leaks escapes.
        let ticks = 60;
        let glyph = glyph_spin_bytes(50, 26, ticks).await;

        use crate::perf::{ByteClass, CountingWriter, Meter};
        use ratatui::backend::CrosstermBackend;
        use ratatui::{TerminalOptions, Viewport};
        let mut app = test_app().await;
        let book = app.library.first().cloned().expect("seeded book");
        app.open_book(book).await.expect("open");
        app.render_mode = RenderMode::Rich;
        app.spinning = true;

        let meter = Meter::new();
        let backend = CrosstermBackend::new(CountingWriter::new(
            Vec::new(),
            meter.clone(),
            ByteClass::Text,
        ));
        let mut terminal = ratatui::Terminal::with_options(
            backend,
            TerminalOptions {
                viewport: Viewport::Fixed(ratatui::layout::Rect::new(0, 0, 50, 26)),
            },
        )
        .expect("terminal");
        redraw(&mut terminal, &mut app).expect("first draw");
        let start = meter.snapshot();
        for _ in 0..ticks {
            app.tick();
            redraw(&mut terminal, &mut app).expect("draw");
        }
        let hybrid = meter.snapshot().since(start);

        assert_eq!(hybrid.image, 0, "the hybrid sent image bytes while moving");
        assert_eq!(
            hybrid.text, glyph,
            "a moving book in rich mode drew something other than the glyph frame"
        );
    }

    /// The invariant that keeps the kitty hybrid alive.
    ///
    /// The ambient layer has to make the event loop redraw, and the obvious way
    /// to do that — folding it into `animating()` — would be silently fatal: a
    /// parked book would report `moving = true` forever and never transmit its
    /// crisp frame. Nothing about the ambient layer would look broken, so this
    /// is the test that has to notice.
    #[tokio::test]
    async fn ambient_dirties_without_claiming_the_book_is_moving() {
        let mut app = test_app().await;
        app.screen = Screen::Menu;
        app.ambient = crate::ambient::Ambient::new(crate::ambient::Motif::Motes);

        // Somewhere in a second of ticks the ambient clock must ask for a
        // redraw — otherwise the layer is frozen and the feature does nothing.
        let asked = (0..20).filter(|_| app.tick()).count();
        assert!(asked > 0, "the ambient layer never asked for a redraw");
        assert!(
            !app.animating(),
            "ambient motion must never register as the book moving"
        );

        let mut terminal = ratatui::Terminal::new(TestBackend::new(80, 24)).expect("terminal");
        redraw(&mut terminal, &mut app).expect("draw");
        assert_eq!(
            app.params.moving,
            Some(false),
            "the renderer was told the book is moving while it sat parked"
        );
    }

    #[tokio::test]
    async fn the_book_screen_has_no_ambient() {
        // The object owns that pane; a second moving thing beside it is noise.
        let mut app = test_app().await;
        let book = app.library.first().cloned().expect("seeded book");
        app.open_book(book).await.expect("open");
        app.screen = Screen::Book;
        app.spinning = false;
        app.ambient = crate::ambient::Ambient::new(crate::ambient::Motif::Contours);

        assert!(!app.ambient_visible());
        // A parked book on the book screen is fully idle whatever the motif is.
        assert!(
            (0..40).all(|_| !app.tick()),
            "the book screen redrew for the ambient layer"
        );

        // And the drawn frame is identical to the one with the layer off.
        let draw_with = |app: &mut App| {
            let mut t = ratatui::Terminal::new(TestBackend::new(120, 40)).expect("terminal");
            t.draw(|f| ui::draw(f, app)).expect("draw");
            t.backend().buffer().clone()
        };
        let with = draw_with(&mut app);
        app.ambient = crate::ambient::Ambient::new(crate::ambient::Motif::Off);
        let without = draw_with(&mut app);
        assert_eq!(with, without, "ambient leaked onto the book screen");
    }

    #[tokio::test]
    async fn an_open_modal_freezes_the_ambient() {
        // Still drawn — vanishing on keystroke would be worse than drifting —
        // but no longer moving, and so no longer asking for redraws.
        let mut app = test_app().await;
        app.screen = Screen::Search;
        app.ambient = crate::ambient::Ambient::new(crate::ambient::Motif::Motes);
        app.start_input(InputContext::SearchQuery, "search", "");

        assert!(app.ambient_visible(), "the layer should still be drawn");
        assert!(
            (0..40).all(|_| !app.tick()),
            "the ambient layer kept drifting under an open input box"
        );
    }

    #[tokio::test]
    async fn a_motif_setting_does_not_disturb_the_book_spin() {
        // The book's own path must be untouched by the ambient plumbing.
        let mut app = test_app().await;
        let book = app.library.first().cloned().expect("seeded book");
        app.open_book(book).await.expect("open");
        app.spinning = true;
        app.screen = Screen::Book;
        app.ambient = crate::ambient::Ambient::new(crate::ambient::Motif::Motes);

        let before = app.params.pose.yaw;
        assert!(app.tick(), "a spinning book must still request its redraw");
        assert_ne!(app.params.pose.yaw, before, "the book stopped turning");
    }

    /// `config::save` serializes the whole struct over the file, so persisting
    /// one setting from a struct literal silently blanks every other one —
    /// change the accent and the ambient motif is gone. Asserted on
    /// `config_snapshot`, which is what every persist path writes.
    ///
    /// Deliberately does *not* call `persist_config`: that writes to the user's
    /// real `~/.config/readingbuddy/tui.toml`, and a test has no business
    /// touching it.
    #[tokio::test]
    async fn persisting_carries_every_setting_not_just_the_changed_one() {
        let mut app = test_app().await;
        app.ambient = crate::ambient::Ambient::new(crate::ambient::Motif::Contours);
        theme::set_accent(0x12_34_56);

        let cfg = app.config_snapshot();
        assert_eq!(cfg.ambient.as_deref(), Some("contours"));
        assert_eq!(cfg.accent.as_deref(), Some("#123456"));
    }

    #[tokio::test]
    async fn every_screen_draws_at_every_size() {
        let mut app = test_app().await;
        let book = app.library.first().cloned().expect("seeded book");
        app.open_book(book).await.expect("open");

        // Every motif, because the ambient layer's sizing arithmetic runs on
        // every non-book screen and 1x1 is where it is likeliest to go wrong.
        for motif in crate::ambient::Motif::ALL {
            app.ambient = crate::ambient::Ambient::new(motif);
            for (w, h) in [
                (120, 40),
                (80, 24),
                (40, 30),
                (30, 30),
                (20, 8),
                (4, 2),
                (1, 1),
            ] {
                draw_every_screen_once(&mut app, w, h).await;
            }
        }
    }

    /// The body of [`every_screen_draws_at_every_size`], for one terminal size.
    async fn draw_every_screen_once(app: &mut App, w: u16, h: u16) {
        {
            let app = &mut *app;
            let mut terminal = ratatui::Terminal::new(TestBackend::new(w, h)).expect("terminal");
            for screen in [
                Screen::Menu,
                Screen::Library,
                Screen::Book,
                Screen::Search,
                Screen::Settings,
            ] {
                app.screen = screen;
                for tab in [
                    BookTab::Info,
                    BookTab::Notes,
                    BookTab::Highlights,
                    BookTab::Cards,
                ] {
                    app.book_tab = tab;
                    // Draw both the section menu and the entered section.
                    for in_section in [false, true] {
                        app.in_section = in_section;
                        app.clamp_tab_selection();
                        for options in [false, true] {
                            app.show_options = options;
                            // The section pane's toggle rides along with the key
                            // bar's, so both states are drawn at every size
                            // without doubling an already-large sweep.
                            app.layout.panel = options;
                            terminal.draw(|f| ui::draw(f, app)).expect("draw");
                        }
                    }
                }
            }
            // With a text input and a confirmation overlaid.
            app.screen = Screen::Search;
            app.start_input(InputContext::SearchQuery, "search", "pachinko");
            terminal.draw(|f| ui::draw(f, app)).expect("draw input");
            app.input = None;
            app.screen = Screen::Library;
            app.confirm = Some(Confirm::RemoveBook {
                id: 1,
                title: "X".into(),
            });
            terminal.draw(|f| ui::draw(f, app)).expect("draw confirm");
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
            terminal.draw(|f| ui::draw(f, app)).expect("draw editor");
            app.note_editor = None;
            // With the API-key modal open over the settings screen, at each stage.
            app.screen = Screen::Settings;
            for stage in [
                ApiKeyStage::Entry {
                    value: "AIzaSyDummyKey".into(),
                },
                ApiKeyStage::Verifying,
                ApiKeyStage::Result {
                    ok: true,
                    message: "verified and saved.".into(),
                },
                ApiKeyStage::Result {
                    ok: false,
                    message: "API key not valid".into(),
                },
            ] {
                app.api_key = Some(ApiKeyModal { stage });
                terminal.draw(|f| ui::draw(f, app)).expect("draw api key");
            }
            app.api_key = None;
        }
    }

    /// A resize must put a fresh image on screen, even when the object rect
    /// comes out of it exactly the same size.
    ///
    /// The stacked layout caps *both* of that rect's axes (`STACK_MAX_COLS`,
    /// `BOOK_MAX_ROWS`), so resizing the window routinely leaves it identical —
    /// and the transmit cache is keyed on the rect. Without the resize hook the
    /// key still matches, nothing is re-sent, and the terminal (which drops or
    /// misplaces images across a resize, tmux especially) is left showing half
    /// an image or none. The side-by-side layouts hide this: their object takes
    /// the whole pane height, so a vertical resize always changes the key.
    #[tokio::test]
    async fn a_resize_retransmits_the_parked_image() {
        use crate::render3d::caps::{Caps, Passthrough};
        use crate::render3d::present::RichState;
        use std::sync::{Arc, Mutex};

        #[derive(Clone, Default)]
        struct Sink(Arc<Mutex<Vec<u8>>>);
        impl std::io::Write for Sink {
            fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
                self.0.lock().unwrap().extend_from_slice(buf);
                Ok(buf.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        impl Sink {
            fn take(&self) -> String {
                let mut g = self.0.lock().unwrap();
                let s = String::from_utf8_lossy(&g).to_string();
                g.clear();
                s
            }
        }

        let caps = Caps {
            kitty_graphics: true,
            cell_px: (9, 19),
            cell_px_measured: true,
            in_tmux: true,
            passthrough: Passthrough::Already,
        };
        let sink = Sink::default();

        let mut app = test_app().await;
        let book = app.library.first().cloned().expect("seeded book");
        app.open_book(book).await.expect("open");
        app.set_render_mode(RenderMode::Rich);
        app.caps = caps;
        app.rich = RichState::with_writer(caps, Box::new(sink.clone()));
        app.screen = Screen::Book;
        app.spinning = false;

        // Portrait, so the layout stacks: the object rect is 84 x 26 — both caps
        // binding — at 120 columns and at 130.
        let mut terminal = ratatui::Terminal::new(TestBackend::new(120, 70)).expect("terminal");
        redraw(&mut terminal, &mut app).expect("first draw");
        assert!(
            sink.take().contains("a=T"),
            "the parked book transmitted no image to begin with"
        );

        // Same rect, new window. The image has to be sent again.
        terminal.backend_mut().resize(130, 70);
        terminal.autoresize().expect("autoresize");
        redraw(&mut terminal, &mut app).expect("redraw after resize");
        assert!(
            sink.take().contains("a=T"),
            "a resize left the old image on screen and sent nothing"
        );
    }

    #[tokio::test]
    async fn rich_mode_draws_the_book_view_without_panicking() {
        // Rich falls back to the glyph presenter today; drawing it at every
        // layout (Stacked/Split/Compact) must never panic.
        let mut app = test_app().await;
        let book = app.library.first().cloned().expect("seeded book");
        app.open_book(book).await.expect("open");
        app.set_render_mode(RenderMode::Rich);
        app.screen = Screen::Book;
        // Portrait (Stacked), landscape (Split), tiny (Compact).
        for (w, h) in [(40, 60), (120, 40), (20, 8)] {
            let mut terminal = ratatui::Terminal::new(TestBackend::new(w, h)).expect("terminal");
            for in_section in [false, true] {
                app.in_section = in_section;
                terminal.draw(|f| ui::draw(f, &mut app)).expect("draw rich");
            }
        }
    }

    #[tokio::test]
    async fn rotating_toggling_and_resizing_adjust_layout_and_redraw() {
        let mut app = test_app().await;
        let book = app.library.first().cloned().expect("seeded book");
        app.open_book(book).await.expect("open");
        assert!(
            app.layout.panel,
            "the view opens with its sections in reach"
        );
        assert_eq!(app.layout.rotation, 0);
        assert_eq!(app.layout.divider_bias, 0.0);

        // `t` still rotates: clockwise, wrapping after four turns.
        for expected in [1, 2, 3, 0] {
            app.handle(Action::RotateLayout).await.expect("rotate");
            assert_eq!(app.layout.rotation, expected);
        }

        // Tab takes the section pane away and brings it back.
        app.handle(Action::TogglePanel).await.expect("toggle");
        assert!(!app.layout.panel);
        app.handle(Action::TogglePanel).await.expect("toggle");
        assert!(app.layout.panel);

        // Asking for the menu is asking for the pane: the keys that drive it
        // bring it back rather than doing nothing.
        for action in [Action::Down, Action::Up, Action::Select, Action::Right] {
            app.layout.panel = false;
            app.in_section = false;
            app.handle(action).await.expect("menu key");
            assert!(app.layout.panel, "{action:?} left the pane hidden");
        }
        app.in_section = false;

        // Grow / shrink move the divider bias and clamp.
        app.handle(Action::GrowBook).await.expect("grow");
        assert!(app.layout.divider_bias > 0.0);
        app.handle(Action::ShrinkBook).await.expect("shrink");
        app.handle(Action::ShrinkBook).await.expect("shrink");
        assert!(app.layout.divider_bias < 0.0);
        for _ in 0..100 {
            app.handle(Action::GrowBook).await.expect("grow");
        }
        assert!(app.layout.divider_bias <= 1.0, "bias stays clamped");

        // Every orientation, panel shown or hidden, at an extreme bias, must
        // still draw at any size.
        app.screen = Screen::Book;
        for rotation in 0..4 {
            app.layout.rotation = rotation;
            app.layout.panel = rotation % 2 == 0;
            for bias in [-1.0, 0.0, 1.0] {
                app.layout.divider_bias = bias;
                for (w, h) in [(120, 40), (40, 60), (26, 8)] {
                    let mut t = ratatui::Terminal::new(TestBackend::new(w, h)).expect("term");
                    t.draw(|f| ui::draw(f, &mut app)).expect("draw");
                }
            }
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
            app.on_editor_key(KeyEvent::from(KeyCode::Char(c)))
                .await
                .expect("type");
        }
        app.on_editor_key(KeyEvent::from(KeyCode::Enter))
            .await
            .expect("finish body");
        assert!(app.note_editor.is_none());
        assert!(app.input.is_some(), "asks for a page");

        for c in "58".chars() {
            app.on_input_key(KeyEvent::from(KeyCode::Char(c)))
                .await
                .expect("type page");
        }
        app.on_input_key(KeyEvent::from(KeyCode::Enter))
            .await
            .expect("commit page");

        let notes = &app.view.as_ref().unwrap().notes;
        assert_eq!(notes.len(), before + 1);
        assert!(
            notes.iter().any(|n| n.page == Some(58)),
            "the new note carries the entered page"
        );
        assert!(app.in_section && app.book_tab == BookTab::Notes);
    }

    #[tokio::test]
    async fn editor_inserts_tabs_and_shift_enter_newlines() {
        let mut app = test_app().await;
        let book = app.library.first().cloned().expect("seeded book");
        app.open_book(book).await.expect("open");

        app.handle(Action::NewNote).await.expect("open editor");
        app.on_editor_key(KeyEvent::from(KeyCode::Char('a')))
            .await
            .expect("type");
        app.on_editor_key(KeyEvent::from(KeyCode::Tab))
            .await
            .expect("tab");
        app.on_editor_key(KeyEvent::from(KeyCode::Char('b')))
            .await
            .expect("type");
        // Shift+Enter adds a line rather than saving.
        app.on_editor_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::SHIFT))
            .await
            .expect("shift-enter");
        app.on_editor_key(KeyEvent::from(KeyCode::Char('c')))
            .await
            .expect("type");

        let draft = app.note_editor.as_ref().expect("editor still open");
        assert_eq!(draft.editor.text(), "a\tb\nc");
    }

    #[tokio::test]
    async fn empty_page_prompt_saves_without_an_anchor() {
        let mut app = test_app().await;
        let book = app.library.first().cloned().expect("seeded book");
        app.open_book(book).await.expect("open");

        app.handle(Action::NewNote).await.expect("open editor");
        app.on_editor_key(KeyEvent::from(KeyCode::Char('x')))
            .await
            .expect("type");
        app.on_editor_key(KeyEvent::from(KeyCode::Enter))
            .await
            .expect("finish body");
        // Empty page + Enter → saved with no page.
        app.on_input_key(KeyEvent::from(KeyCode::Enter))
            .await
            .expect("skip page");

        assert!(app.input.is_none());
        assert!(
            app.view
                .as_ref()
                .unwrap()
                .notes
                .iter()
                .any(|n| n.page.is_none()),
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
            app.on_editor_key(KeyEvent::from(KeyCode::Char(c)))
                .await
                .expect("type");
        }
        app.on_editor_key(KeyEvent::from(KeyCode::Enter))
            .await
            .expect("save");

        let updated = std::fs::read_to_string(&path).unwrap();
        assert!(updated.contains("page: 120"), "frontmatter kept");
        assert!(updated.contains("rewritten body"));
        assert!(!updated.contains("Symphony"), "old body gone");
    }

    #[tokio::test]
    async fn esc_on_an_existing_note_cancels_without_prompting() {
        // Opening a saved note to edit, then Esc, must just close the editor —
        // no discard confirmation (that's only for brand-new notes), no
        // deletion (that's only ever `d`).
        let mut app = test_app().await;
        let book = app.library.first().cloned().expect("seeded book");
        app.open_book(book).await.expect("open");
        app.book_tab = BookTab::Notes;
        app.in_section = true;
        app.clamp_tab_selection();

        let note = app.view.as_ref().unwrap().notes[0].clone();
        let path = app.engine.config.vault_dir.join(&note.file_path);
        let original = std::fs::read_to_string(&path).unwrap();

        // Open the editor (loads the saved body), then Esc immediately.
        app.handle(Action::Select).await.expect("open editor");
        assert!(app.note_editor.is_some());
        app.on_editor_key(KeyEvent::from(KeyCode::Esc))
            .await
            .expect("esc");

        assert!(app.note_editor.is_none(), "editor closed");
        assert!(app.confirm.is_none(), "no discard prompt for a saved note");
        assert!(app.pending_discard.is_none());
        assert_eq!(app.view.as_ref().unwrap().notes.len(), 1, "note kept");
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            original,
            "saved note file untouched"
        );
    }

    #[tokio::test]
    async fn deleting_a_note_needs_confirmation() {
        let mut app = test_app().await;
        let book = app.library.first().cloned().expect("seeded book");
        app.open_book(book).await.expect("open");
        // Enter the Notes section with the seeded note selected.
        app.book_tab = BookTab::Notes;
        app.in_section = true;
        app.clamp_tab_selection();
        let before = app.view.as_ref().unwrap().notes.len();
        assert_eq!(before, 1);
        let path = app
            .engine
            .config
            .vault_dir
            .join(&app.view.as_ref().unwrap().notes[0].file_path);
        assert!(path.exists());

        // `d` asks; declining keeps the note and its file.
        app.handle(Action::Delete).await.expect("ask");
        assert!(matches!(app.confirm, Some(Confirm::DeleteNote(_))));
        app.resolve_confirm(false).await.expect("keep");
        assert_eq!(app.view.as_ref().unwrap().notes.len(), before);
        assert!(path.exists());

        // Asking again and confirming removes the row and the file.
        app.handle(Action::Delete).await.expect("ask again");
        app.resolve_confirm(true).await.expect("delete");
        assert!(app.view.as_ref().unwrap().notes.is_empty());
        assert!(!path.exists(), "the note file was left behind");
    }

    #[tokio::test]
    async fn deleting_a_note_only_works_in_the_open_notes_section() {
        let mut app = test_app().await;
        let book = app.library.first().cloned().expect("seeded book");
        app.open_book(book).await.expect("open");
        // On the section menu (not entered), and on other tabs, `d` is inert.
        app.book_tab = BookTab::Notes;
        app.in_section = false;
        app.handle(Action::Delete).await.expect("menu d");
        assert!(app.confirm.is_none());

        app.book_tab = BookTab::Highlights;
        app.in_section = true;
        app.clamp_tab_selection();
        app.handle(Action::Delete).await.expect("highlights d");
        assert!(app.confirm.is_none());
    }

    #[tokio::test]
    async fn discarding_a_written_draft_asks_first() {
        let mut app = test_app().await;
        let book = app.library.first().cloned().expect("seeded book");
        app.open_book(book).await.expect("open");
        let before = app.view.as_ref().unwrap().notes.len();

        // Type something, then Esc: the draft is stashed behind a confirm.
        app.handle(Action::NewNote).await.expect("open editor");
        for c in "half a thought".chars() {
            app.on_editor_key(KeyEvent::from(KeyCode::Char(c)))
                .await
                .expect("type");
        }
        app.on_editor_key(KeyEvent::from(KeyCode::Esc))
            .await
            .expect("esc");
        assert!(app.note_editor.is_none());
        assert!(matches!(app.confirm, Some(Confirm::DiscardDraft)));

        // Declining restores the editor with the text intact.
        app.resolve_confirm(false).await.expect("keep editing");
        assert!(app.confirm.is_none());
        let draft = app.note_editor.as_ref().expect("editor restored");
        assert_eq!(draft.editor.text(), "half a thought");

        // Esc + confirm actually throws it away; nothing is saved.
        app.on_editor_key(KeyEvent::from(KeyCode::Esc))
            .await
            .expect("esc again");
        app.resolve_confirm(true).await.expect("discard");
        assert!(app.note_editor.is_none());
        assert!(app.pending_discard.is_none());
        assert_eq!(app.view.as_ref().unwrap().notes.len(), before);
    }

    #[tokio::test]
    async fn discarding_a_blank_draft_needs_no_confirmation() {
        let mut app = test_app().await;
        let book = app.library.first().cloned().expect("seeded book");
        app.open_book(book).await.expect("open");

        app.handle(Action::NewNote).await.expect("open editor");
        // Esc on an untouched editor closes it immediately, no prompt.
        app.on_editor_key(KeyEvent::from(KeyCode::Esc))
            .await
            .expect("esc");
        assert!(app.note_editor.is_none());
        assert!(app.confirm.is_none());
        assert!(app.pending_discard.is_none());
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
        assert!(
            (app.params.pose.yaw - held).abs() < 0.05,
            "resumes smoothly"
        );
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

    /// Print the list screens: the shrink-wrapped box, and beside each row a
    /// `#` under every cell carrying `REVERSED`.
    ///
    /// The mask is the point. Selection styling is invisible in a symbol dump,
    /// which is how a highlight that reversed the *whole* line rather than just
    /// the title went unnoticed — the text is identical either way.
    ///
    /// `cargo test -p readingbuddy-tui -- --ignored --nocapture print_lists`
    #[test]
    #[ignore = "development aid: prints the list screens"]
    fn print_lists() {
        use ratatui::style::Modifier;

        let rt = tokio::runtime::Runtime::new().unwrap();
        let mut app = rt.block_on(test_app());

        for extra in ["Piranesi", "The Overstory", "Klara and the Sun"] {
            app.library.push(Book {
                title: Some(extra.into()),
                authors: vec!["Someone".into()],
                publish_year: Some(2020),
                ..Book::default()
            });
        }
        app.library_state.select(Some(0));
        app.search_results = app
            .library
            .iter()
            .map(|b| readingbuddy::RankedResult {
                book: b.clone(),
                sources: Vec::new(),
                score: 1.0,
            })
            .collect();
        app.search_state.select(Some(0));

        let (w, h) = (96u16, 16u16);
        for screen in [Screen::Library, Screen::Search] {
            app.screen = screen;
            let mut t = ratatui::Terminal::new(TestBackend::new(w, h)).unwrap();
            t.draw(|f| ui::draw(f, &mut app)).unwrap();
            println!("=== {screen:?} {w}x{h} ===");
            let buf = t.backend().buffer();
            for y in 0..h {
                let text: String = (0..w)
                    .map(|x| buf[(x, y)].symbol().chars().next().unwrap_or(' '))
                    .collect();
                let mask: String = (0..w)
                    .map(|x| {
                        if buf[(x, y)].modifier.contains(Modifier::REVERSED) {
                            '#'
                        } else {
                            ' '
                        }
                    })
                    .collect();
                if mask.trim().is_empty() {
                    println!("|{text}|");
                } else {
                    println!("|{text}|  reversed: |{}|", mask.trim_end());
                }
            }
        }
    }

    /// Print the menu under each motif, a few frames apart.
    ///
    /// The only honest review of a look is looking at it, and the byte budget
    /// says nothing about whether the field reads as calm or as noise.
    ///
    /// `cargo test -p readingbuddy-tui -- --ignored --nocapture print_ambient`
    #[test]
    #[ignore = "development aid: prints the ambient layer"]
    fn print_ambient() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let mut app = rt.block_on(test_app());
        app.screen = Screen::Menu;
        let (w, h) = (110u16, 32u16);

        for motif in crate::ambient::Motif::ALL {
            if motif == crate::ambient::Motif::Off {
                continue; // its clock never advances — nothing to show, and
                // waiting for a frame that never arrives hangs.
            }
            app.ambient = crate::ambient::Ambient::new(motif);
            for frame in [0u32, 12, 40] {
                // Drive the real clock, so this shows the frames the app shows.
                while app.ambient.frame_index() < frame {
                    app.tick();
                }
                let mut t = ratatui::Terminal::new(TestBackend::new(w, h)).unwrap();
                t.draw(|f| ui::draw(f, &mut app)).unwrap();
                println!("=== {} · frame {frame} ===", motif.label());
                let buf = t.backend().buffer();
                for y in 0..h {
                    let row: String = (0..w)
                        .map(|x| buf[(x, y)].symbol().chars().next().unwrap_or(' '))
                        .collect();
                    println!("|{row}|");
                }
            }
        }
    }

    #[test]
    #[ignore = "development aid: prints the composed layout"]
    fn print_layout() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let mut app = rt.block_on(test_app());
        let book = app.library.first().cloned().unwrap();
        rt.block_on(app.open_book(book)).unwrap();
        app.show_options = true;
        // Wide cases too: the object is centred on the window (with the panel
        // beside it) once the width affords the panel its floor, and that is the
        // only place to see it. The last case is tab — the pane dismissed, which
        // is the one layout where the object has the whole window.
        for (w, h, panel) in [
            (110, 32, true),
            (44, 26, true),
            (86, 30, true),
            (180, 44, true),
            (180, 44, false),
        ] {
            app.layout.panel = panel;
            let mut t = ratatui::Terminal::new(TestBackend::new(w, h)).unwrap();
            t.draw(|f| ui::draw(f, &mut app)).unwrap();
            println!("=== {w}x{h} ===");
            let buf = t.backend().buffer();
            for y in 0..h {
                let row: String = (0..w)
                    .map(|x| {
                        let s = buf[(x, y)].symbol();
                        match s {
                            "▀" | "▄" => '#',
                            " " => ' ',
                            _ => s.chars().next().unwrap_or(' '),
                        }
                    })
                    .collect();
                println!("|{row}|");
            }
        }
    }

    #[tokio::test]
    async fn api_key_box_collects_a_key_and_defers_verification() {
        let mut app = test_app().await;
        app.screen = Screen::Settings;

        // `g` opens the paste box onto a blank field.
        app.handle(Action::EditApiKey).await.expect("open");
        assert!(matches!(
            app.api_key.as_ref().map(|m| &m.stage),
            Some(ApiKeyStage::Entry { .. })
        ));

        // Typed characters accumulate (paste follows the same path).
        for c in "AIzaKEY".chars() {
            app.on_api_key_key(KeyEvent::from(KeyCode::Char(c)));
        }
        app.on_api_key_key(KeyEvent::from(KeyCode::Backspace));
        match app.api_key.as_ref().map(|m| &m.stage) {
            Some(ApiKeyStage::Entry { value }) => assert_eq!(value, "AIzaKE"),
            _ => panic!("still in entry"),
        }

        // Enter defers the live-check: stage → Verifying, key queued, box kept.
        app.on_api_key_key(KeyEvent::from(KeyCode::Enter));
        assert_eq!(app.pending_verify.as_deref(), Some("AIzaKE"));
        assert!(matches!(
            app.api_key.as_ref().map(|m| &m.stage),
            Some(ApiKeyStage::Verifying)
        ));
    }

    #[tokio::test]
    async fn api_key_box_cancels_and_empty_submit_is_inert() {
        let mut app = test_app().await;
        app.open_api_key();

        // Enter on an empty field submits nothing.
        app.on_api_key_key(KeyEvent::from(KeyCode::Enter));
        assert!(app.pending_verify.is_none());
        assert!(app.api_key.is_some(), "box stays open");

        // Esc closes the box without queuing a verification.
        app.on_api_key_key(KeyEvent::from(KeyCode::Esc));
        assert!(app.api_key.is_none());
        assert!(app.pending_verify.is_none());
    }

    #[tokio::test]
    async fn api_key_failure_page_retries_or_cancels() {
        let mut app = test_app().await;

        // A failure page: Enter goes back to a fresh entry field.
        app.api_key = Some(ApiKeyModal {
            stage: ApiKeyStage::Result {
                ok: false,
                message: "bad key".into(),
            },
        });
        app.on_api_key_key(KeyEvent::from(KeyCode::Enter));
        match app.api_key.as_ref().map(|m| &m.stage) {
            Some(ApiKeyStage::Entry { value }) => assert!(value.is_empty()),
            _ => panic!("retry reopens the entry field"),
        }

        // From a failure page, Esc closes.
        app.api_key = Some(ApiKeyModal {
            stage: ApiKeyStage::Result {
                ok: false,
                message: "bad key".into(),
            },
        });
        app.on_api_key_key(KeyEvent::from(KeyCode::Esc));
        assert!(app.api_key.is_none());

        // A success page dismisses on any key.
        app.api_key = Some(ApiKeyModal {
            stage: ApiKeyStage::Result {
                ok: true,
                message: "ok".into(),
            },
        });
        app.on_api_key_key(KeyEvent::from(KeyCode::Char(' ')));
        assert!(app.api_key.is_none());
    }

    #[test]
    fn angles_wrap_into_a_single_turn() {
        let a = wrap_angle(std::f32::consts::TAU + 0.5);
        assert!((a - 0.5).abs() < 1e-4, "{a}");
        let b = wrap_angle(-std::f32::consts::TAU - 0.5);
        assert!((b + 0.5).abs() < 1e-4, "{b}");
    }
}
