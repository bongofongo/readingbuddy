//! Application state and the async event loop.

use std::collections::{HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::Result;
use crossterm::event::{Event, EventStream, KeyCode, KeyEvent, KeyEventKind};
use futures::StreamExt;
use ratatui::Terminal;
use ratatui::backend::Backend;
use ratatui::layout::Position;
use ratatui::widgets::ListState;
use readingbuddy::{
    Book, BookSort, DeviceBook, DeviceState, Diagnostic, Engine, EngineError, FlashcardRow,
    Highlight, MatchCandidate, MountEvent, MountWatcher, NewNoteInput, NoteKind, NoteRecord,
    RankedResult, Reading, SearchRequest,
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

/// How many open readings the home screen asks for.
///
/// A ceiling rather than a page size: the list scrolls, and a person with more
/// than this many books genuinely on the go has a different problem. It is not
/// shown anywhere — the screen carries no count of anything.
const HOME_LIMIT: i64 = 50;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    /// What you are currently reading — one row per open reading, and the
    /// screen the app opens to. `m` still reaches the menu from anywhere, so
    /// making this the front door takes nothing away.
    Home,
    Menu,
    Library,
    Book,
    Search,
    Settings,
    /// The mounted reader's own shelf: one row per book on the device.
    Device,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuItem {
    /// Back to the currently-reading shelf. Esc from the menu goes there too;
    /// this row is what makes it findable rather than a thing you have to know.
    Home,
    Library,
    Continue,
    Search,
    AddIsbn,
    ImportKo,
    Device,
    Cards,
    Settings,
    Quit,
}

pub const MENU: [(MenuItem, &str, &str); 10] = [
    (
        MenuItem::Home,
        "Currently reading",
        "the books you have open",
    ),
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
        MenuItem::Device,
        "Device",
        "the shelf on your mounted reader",
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

/// One row of the device screen: what the scan said, plus what this session has
/// since done to it.
///
/// The scan's own `DeviceBook` is kept whole rather than flattened into strings
/// — `ui::device` needs the state's numbers, and `l` needs `New`'s candidate
/// band. `notices` is the reason this wrapper exists at all: the TUI has one
/// `status` line and every other engine call flattens into it, which is fine for
/// "search failed" and useless for forty books where the third one had a bad
/// sidecar. [`Diagnostic`] is `Clone` precisely so a frontend can buffer it.
pub struct DeviceRow {
    pub book: DeviceBook,
    /// What the last pull of *this* row reported, if it has been pulled.
    pub note: Option<String>,
    /// Degradations from this row's own pull, kept beside it rather than
    /// overwriting the shared status line.
    pub notices: Vec<Diagnostic>,
}

impl DeviceRow {
    pub(crate) fn new(book: DeviceBook) -> DeviceRow {
        DeviceRow {
            book,
            note: None,
            notices: Vec::new(),
        }
    }
}

/// The candidate chooser opened by `l` on an unmatched row.
///
/// It offers exactly the band the engine already computed
/// (`CANDIDATE_MIN..AUTO_MATCH`) — the point of `DeviceState::New` carrying its
/// candidates is that unmatched is a decision rather than a dead end, and
/// re-deriving a different list here would quietly disagree with the CLI.
pub struct LinkPicker {
    /// The sidecar being linked. Held by path so a rescan underneath cannot
    /// leave the picker pointing at a different row.
    pub path: PathBuf,
    pub title: String,
    pub candidates: Vec<MatchCandidate>,
    pub state: ListState,
}

/// What an open text input is collecting, so `commit` knows what to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputContext {
    SearchQuery,
    IsbnAdd,
    ProgressPage,
    KoPath,
    /// A device (or library) path to scan, when no reader was found by itself.
    DevicePath,
    /// Optional page anchor asked for after composing a new note.
    NotePage,
    /// The rating asked for after saving a review — the one note kind that
    /// carries one. Empty (or Esc) leaves whatever rating it already had:
    /// skipping is not clearing.
    ReviewRating,
    /// A `#RRGGBB` accent color typed on the settings screen.
    AccentHex,
}

/// One edge of the note graph, as one row of the links pane.
///
/// Both variants are the *same* edge set read from opposite ends — which is the
/// reason `Storage::backlinks` refuses a dangling-by-title union, and the reason
/// this enum has two variants rather than one with a direction flag: the two
/// come from two engine calls and carry different things (an outbound edge may
/// have no note at all).
#[derive(Debug, Clone)]
pub enum LinkRow {
    /// This note links out. `to` is `None` for a **dangling** target: a
    /// `[[wikilink]]` naming a note nobody has written. Kept as the text it is —
    /// a forward reference resolves itself the moment that note exists, and
    /// dropping it would lose the one thing it is for.
    Out {
        /// What to show: the target note's own title when it resolves, else the
        /// wikilink text exactly as the body wrote it.
        title: String,
        to: Option<NoteRecord>,
    },
    /// Another note links here.
    In(NoteRecord),
}

/// The graph around one note: what it links to, and what links back.
///
/// Attached to the Notes list rather than being a fifth `BookTab` — the graph is
/// consulted while standing on a note (a reflection, most of the time, since
/// that is the hub), and a tab would have to be entered from somewhere else and
/// then told which note it meant.
pub struct LinksPane {
    /// The note the pane is centred on. Held whole, not by id, so the pane can
    /// centre on a note that is not in this book's list at all — following a
    /// link across books is the point of the graph.
    pub note: NoteRecord,
    /// Outbound rows first, then inbound. Grouped rather than interleaved, so
    /// the two directions read as two answers even before the arrows are seen.
    pub rows: Vec<LinkRow>,
    pub state: ListState,
}

impl LinksPane {
    pub fn out_count(&self) -> usize {
        self.rows
            .iter()
            .filter(|r| matches!(r, LinkRow::Out { .. }))
            .count()
    }

    pub fn in_count(&self) -> usize {
        self.rows.len() - self.out_count()
    }

    /// The row the cursor is on, if the pane has any.
    fn selected(&self) -> Option<&LinkRow> {
        self.state.selected().and_then(|i| self.rows.get(i))
    }
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
    /// What the editor's border calls this. A reflection and a review are named
    /// rather than both reading "edit note": they are the two notes you open by
    /// a key rather than pick off a list, so the border is the only thing that
    /// says which one is under the cursor.
    pub fn title(&self) -> &'static str {
        match &self.target {
            NoteTarget::New { .. } => "new note",
            NoteTarget::Edit(n) if n.kind == NoteKind::Reflection.as_str() => "reflection",
            NoteTarget::Edit(n) if n.kind == NoteKind::Review.as_str() => "review",
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
    /// The home shelf: one entry per **open** reading.
    ///
    /// The [`Reading`] rides along beside the [`Book`] because the two are not
    /// interchangeable — `Book`'s progress fields are projections of the
    /// *current* reading, while this is specifically the open one and carries
    /// the device's own percentage, which is all a sidecar-seeded book has.
    pub reading: Vec<(Book, Reading)>,
    pub reading_state: ListState,
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
    /// The note graph around the selected note, when `L` has asked for it. It
    /// replaces the Notes list in the section pane; it is **not** a modal, so it
    /// is routed by the book view's own handler rather than by `dispatch_key`.
    pub links: Option<LinksPane>,
    pub pending_note: Option<PendingNote>,
    /// A non-blank draft set aside while its discard is confirmed.
    pub pending_discard: Option<NoteDraft>,
    /// The Google Books API-key modal, when open.
    pub api_key: Option<ApiKeyModal>,
    /// A submitted key awaiting its (async) live-check; the event loop drains
    /// this after drawing the "verifying" frame.
    pub pending_verify: Option<String>,
    pub confirm: Option<Confirm>,
    /// A saved review awaiting its rating, held while the box asks for it —
    /// the same shape as `pending_note` holding a body while its page is asked.
    pub pending_rating: Option<i64>,
    pub search_results: Vec<RankedResult>,
    pub search_state: ListState,
    /// The mounted reader's books, as the last scan found them.
    pub device: Vec<DeviceRow>,
    pub device_state: ListState,
    /// Rows marked with `x`, by index into `device`. Cleared by every scan —
    /// the indices are only meaningful against the list they were made on.
    pub device_marks: HashSet<usize>,
    /// The root the last scan walked, so `r` knows what to walk again.
    pub device_root: Option<PathBuf>,
    /// The open candidate chooser, if `l` is mid-decision.
    pub device_link: Option<LinkPicker>,
    /// A device root awaiting its (blocking) walk. Drained by the event loop
    /// *after* it has drawn the frame that says "scanning…" — the same
    /// deferred-work shape as `pending_verify`.
    pub pending_scan: Option<PathBuf>,
    /// Sidecars awaiting their pull, drained **one per loop iteration** with a
    /// redraw between. A `for` loop here would freeze the draw loop and the
    /// 20fps ticker for the whole sync and show no progress at all.
    pub pending_pull: Option<VecDeque<PathBuf>>,
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
        let scene = Scene::new(engine.images_dir().to_path_buf());
        let mut app = App {
            engine,
            // The front door is what you are reading, not a list of commands.
            screen: Screen::Home,
            menu_index: 0,
            library: Vec::new(),
            library_state: ListState::default(),
            reading: Vec::new(),
            reading_state: ListState::default(),
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
            links: None,
            pending_note: None,
            pending_discard: None,
            api_key: None,
            pending_verify: None,
            confirm: None,
            pending_rating: None,
            search_results: Vec::new(),
            search_state: ListState::default(),
            device: Vec::new(),
            device_state: ListState::default(),
            device_marks: HashSet::new(),
            device_root: None,
            device_link: None,
            pending_scan: None,
            pending_pull: None,
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
        self.library = self.engine.list_books(200, BookSort::LastModified).await?;
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
        self.refresh_reading().await?;
        self.dirty = true;
        Ok(())
    }

    /// Reload the home shelf.
    ///
    /// Folded into [`App::refresh_library`] rather than left to the home
    /// screen's own key: "state persists and is visible" means the shelf is
    /// right when you arrive at it, and every path that changes what is on it —
    /// an import, a device pull, a removal, a new book — already funnels
    /// through there. The two mutations that do *not* (progress and the
    /// finished toggle) call this directly.
    pub async fn refresh_reading(&mut self) -> Result<()> {
        self.reading = self.engine.currently_reading(HOME_LIMIT).await?;
        if !self.reading.is_empty() && self.reading_state.selected().is_none() {
            self.reading_state.select(Some(0));
        }
        if self
            .reading_state
            .selected()
            .is_some_and(|i| i >= self.reading.len())
        {
            self.reading_state
                .select((!self.reading.is_empty()).then(|| self.reading.len() - 1));
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
        // A pane left open would show the previous book's graph under this
        // book's title. Following a link *into* a book re-opens it afterwards.
        self.links = None;
        self.screen = Screen::Book;
        self.status = None;
        self.dirty = true;
        Ok(())
    }

    async fn load_view(&self, book: Book) -> Result<BookView> {
        let (notes, highlights, cards) = match book.id {
            Some(id) => (
                self.engine.list_notes(Some(id)).await?,
                self.engine.list_highlights(id).await?,
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
        if let Some(book) = self.engine.get_book(id).await? {
            self.view = Some(self.load_view(book).await?);
            self.clamp_tab_selection();
        }
        // Every mutation that reaches here can have moved the graph — saving a
        // note rewrites its edges, and writing one titled for a dangling target
        // resolves that target from the other side. A pane left as it was would
        // be showing the answer to a question that has since changed.
        self.refresh_links().await?;
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
            // The candidate chooser is a modal like the others: while a decision
            // is open the background stops drifting behind it.
            && self.device_link.is_none()
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

            (Screen::Home, Action::Up) => self.step_reading(-1),
            (Screen::Home, Action::Down) => self.step_reading(1),
            (Screen::Home, Action::Select) => self.open_selected_reading().await?,
            (Screen::Home, Action::Reflect) => self.open_reading_note(NoteKind::Reflection).await?,
            (Screen::Home, Action::Review) => self.open_reading_note(NoteKind::Review).await?,
            // The two ways a book gets onto this shelf are search and the
            // library, and `/` already means "search" everywhere else.
            (Screen::Home, Action::Query) => self.open_search(),
            // The front door: back from here is out, exactly as it used to be
            // from the menu.
            (Screen::Home, Action::Back) => self.quit = true,

            (Screen::Menu, Action::Up) => {
                self.menu_index = (self.menu_index + MENU.len() - 1) % MENU.len();
            }
            (Screen::Menu, Action::Down) => {
                self.menu_index = (self.menu_index + 1) % MENU.len();
            }
            (Screen::Menu, Action::Select) => self.activate_menu().await?,
            // Esc used to quit here, when the menu was the front door. It is
            // not any more, and a key that leaves the app from the middle of it
            // is worse than one that goes back where you came from.
            (Screen::Menu, Action::Back) => self.screen = Screen::Home,

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

            (Screen::Device, action) => self.handle_device(action).await?,

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
        // The links pane sits *in* the Notes list's place, so it takes the
        // list's keys while it is open and leaves everything else alone: `n`
        // still writes a note, `space` still stops the book. It is not a modal
        // and so is not in `dispatch_key`'s priority chain — Esc backs out of it
        // exactly one level, into the note list it replaced.
        if self.links.is_some() {
            match action {
                // `ensure_panel` for the same reason the note list needs it: with
                // the pane dismissed by tab these would otherwise move a cursor
                // nobody can see.
                Action::Up => {
                    self.ensure_panel();
                    self.step_links(-1);
                    return Ok(());
                }
                Action::Down => {
                    self.ensure_panel();
                    self.step_links(1);
                    return Ok(());
                }
                Action::Select | Action::Right => {
                    self.ensure_panel();
                    self.follow_link().await?;
                    return Ok(());
                }
                Action::Back | Action::Left | Action::Links => {
                    self.links = None;
                    self.status = None;
                    return Ok(());
                }
                _ => {}
            }
        }
        match action {
            Action::Links => self.open_links().await?,
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
            // The book view's half of item 7. `reading_id: None` is the engine's
            // "the current reading", which opens one when the book has none —
            // a reflection is written mid-book, and that is the normal case.
            Action::Reflect => {
                let book_id = self.view.as_ref().and_then(|v| v.book.id);
                if let Some(id) = book_id {
                    self.open_anchored_note(id, None, NoteKind::Reflection)
                        .await?;
                }
            }
            Action::Review => {
                let book_id = self.view.as_ref().and_then(|v| v.book.id);
                if let Some(id) = book_id {
                    self.open_anchored_note(id, None, NoteKind::Review).await?;
                }
            }
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

    // ---- the home screen ---------------------------------------------------

    fn step_reading(&mut self, delta: isize) {
        if self.reading.is_empty() {
            return;
        }
        let len = self.reading.len() as isize;
        let cur = self.reading_state.selected().unwrap_or(0) as isize;
        self.reading_state
            .select(Some(((cur + delta).rem_euclid(len)) as usize));
    }

    /// The selected row's book id and the id of the reading it is showing.
    ///
    /// Both, not just the book: this screen is a list of *readings*, so a
    /// reflection opened from it belongs to the one on screen rather than to
    /// whichever the engine would resolve as current.
    fn selected_reading(&self) -> Option<(i64, i64)> {
        self.reading_state
            .selected()
            .and_then(|i| self.reading.get(i))
            .and_then(|(b, r)| b.id.map(|id| (id, r.id)))
    }

    async fn open_selected_reading(&mut self) -> Result<()> {
        let book = self
            .reading_state
            .selected()
            .and_then(|i| self.reading.get(i))
            .map(|(b, _)| b.clone());
        if let Some(book) = book {
            self.open_book(book).await?;
        }
        Ok(())
    }

    /// Open the selected row's reflection or review.
    async fn open_reading_note(&mut self, kind: NoteKind) -> Result<()> {
        let Some((book_id, reading_id)) = self.selected_reading() else {
            self.status = Some("nothing open to write about — / to find a book".into());
            return Ok(());
        };
        self.open_anchored_note(book_id, Some(reading_id), kind)
            .await
    }

    /// Open a reading's reflection or review into the in-house editor.
    ///
    /// The engine finds the existing note before creating one, which is what
    /// "accretes" means: pressing the key twice on the same reading opens the
    /// same file, mid-book and after it, for ever. There is no `$EDITOR` path
    /// in this crate — deliberately, after a broken vim once killed the TUI —
    /// so it goes straight into [`TextEditor`] like every other note here.
    async fn open_anchored_note(
        &mut self,
        book_id: i64,
        reading_id: Option<i64>,
        kind: NoteKind,
    ) -> Result<()> {
        let opened = match kind {
            NoteKind::Review => self.engine.open_review_record(book_id, reading_id).await,
            _ => {
                self.engine
                    .open_reflection_record(book_id, reading_id)
                    .await
            }
        };
        let record = match opened {
            Ok(record) => record,
            Err(e) => {
                self.status = Some(format!("could not open it: {e}"));
                return Ok(());
            }
        };
        let body = self.engine.note_body(&record).unwrap_or_default();
        self.note_editor = Some(NoteDraft {
            target: NoteTarget::Edit(record),
            editor: TextEditor::new(&body),
        });
        // The note exists from the moment it is opened, not from the moment it
        // is saved, so the Notes list behind the editor is already out of date.
        self.reload_view().await?;
        self.dirty = true;
        Ok(())
    }

    /// Open the search screen onto a fresh query box.
    fn open_search(&mut self) {
        self.screen = Screen::Search;
        self.search_results.clear();
        self.search_state.select(None);
        self.start_input(InputContext::SearchQuery, "search", "");
    }

    /// Open the highlighted section into the right pane.
    fn enter_section(&mut self) {
        self.in_section = true;
        self.clamp_tab_selection();
    }

    async fn activate_menu(&mut self) -> Result<()> {
        match MENU[self.menu_index].0 {
            MenuItem::Home => {
                self.refresh_reading().await?;
                self.screen = Screen::Home;
            }
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
            MenuItem::Search => self.open_search(),
            MenuItem::AddIsbn => self.start_input(InputContext::IsbnAdd, "isbn", ""),
            MenuItem::ImportKo => self.start_input(InputContext::KoPath, "koreader path", ""),
            MenuItem::Device => self.open_device(),
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

    // ---- the device screen -------------------------------------------------

    /// Open the device screen, and start a scan if we can tell which volume is
    /// meant.
    ///
    /// One mounted reader is the ordinary case and needs no question asked.
    /// Zero and several both open the screen anyway with the path box up: a
    /// menu entry that refuses to go anywhere is exactly the dead end the axiom
    /// rules out, and a library directory is a perfectly good thing to point it
    /// at besides.
    fn open_device(&mut self) {
        self.screen = Screen::Device;
        self.device_link = None;
        let mut mounts = readingbuddy::candidate_mounts();
        match mounts.len() {
            1 => self.start_scan(mounts.remove(0)),
            0 => {
                self.status =
                    Some("no reader mounted — give me a path (a library directory works)".into());
                self.start_input(InputContext::DevicePath, "device path", "");
            }
            n => {
                // Picking one would be a guess about which reader was meant,
                // and both are plugged in.
                self.status = Some(format!(
                    "{n} readers mounted: {}",
                    mounts
                        .iter()
                        .map(|m| m.display().to_string())
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
                self.start_input(
                    InputContext::DevicePath,
                    "device path",
                    &mounts[0].display().to_string(),
                );
            }
        }
    }

    /// A reader arrived or left while the app was running.
    ///
    /// **Scanning, never syncing.** `docs/decisions.md` makes mount → import
    /// automatic and read-only and everything that writes explicit; a watcher
    /// that pulled forty books in because a cable was plugged in would be the
    /// second of those wearing the first one's clothes. So the most this does is
    /// queue the same read-only walk `r` does, and only when the device screen
    /// is the thing being looked at.
    ///
    /// Everywhere else it is a status line naming the move. Yanking the user
    /// off the book they are reading because a device was plugged in is the
    /// other way to get this wrong, and it is worse: `m` is one keypress and an
    /// interrupted screen is not.
    pub fn on_mount_event(&mut self, event: MountEvent) {
        self.dirty = true;
        match event {
            MountEvent::Arrived(mount) => {
                // A prompt asking which device to look at has just been answered
                // by the world. Closing it is not taking a decision off the
                // user — it is the decision they were being asked for.
                let asking = self
                    .input
                    .as_ref()
                    .is_some_and(|i| i.context == InputContext::DevicePath);
                if asking {
                    self.input = None;
                }
                // Not while another device is on the screen: replacing the list
                // under the user would lose their marks and their place, and the
                // reader they were looking at is still plugged in.
                let looking = self.screen == Screen::Device
                    && (asking
                        || self.device_root.is_none()
                        || self.device_root.as_deref() == Some(mount.as_path()));
                if looking {
                    self.start_scan(mount);
                } else {
                    self.status = Some(format!(
                        "reader mounted: {} — m → device to bring anything across",
                        mount.display()
                    ));
                }
            }
            MountEvent::Departed(mount) => {
                if self.device_root.as_deref() == Some(mount.as_path()) {
                    // The rows describe files that are no longer there, and
                    // pressing enter on one would fail per row. The root stays,
                    // so `r` after plugging it back in means something.
                    self.device.clear();
                    self.device_marks.clear();
                    self.device_state.select(None);
                    self.device_link = None;
                }
                self.status = Some(format!("{} was unplugged", mount.display()));
            }
        }
    }

    /// Queue a walk of `root`. The work itself happens in [`App::finish_scan`],
    /// after the loop has drawn the frame this status line belongs to.
    fn start_scan(&mut self, root: PathBuf) {
        self.status = Some(format!("scanning {}…", root.display()));
        self.pending_scan = Some(root);
        self.dirty = true;
    }

    /// Walk the device. Called by the event loop, not a key handler.
    pub async fn finish_scan(&mut self, root: &Path) -> Result<()> {
        self.dirty = true;
        let scan = match self.engine.scan_device(root).await {
            Ok(scan) => scan,
            Err(e) => {
                self.status = Some(format!("scan failed: {e}"));
                return Ok(());
            }
        };
        // A scan is a fresh list, so the marks made against the old one mean
        // nothing — they are indices, and row 3 is a different book now.
        self.device_marks.clear();
        self.device = scan.books.into_iter().map(DeviceRow::new).collect();
        self.device_state
            .select((!self.device.is_empty()).then_some(0));
        self.device_root = Some(scan.root);

        let syncable = self
            .device
            .iter()
            .filter(|r| r.book.state.is_syncable())
            .count();
        self.status = Some(match scan.warnings.first() {
            // The interesting warning here is "no sidecars found", and an empty
            // list with no explanation reads as a broken screen.
            Some(w) => w.detail.clone(),
            None => format!(
                "{} book(s) on the device · {syncable} to bring across ({} read, {} unchanged)",
                self.device.len(),
                scan.parsed,
                scan.cached
            ),
        });
        Ok(())
    }

    fn step_device(&mut self, delta: isize) {
        if self.device.is_empty() {
            return;
        }
        let len = self.device.len() as isize;
        let cur = self.device_state.selected().unwrap_or(0) as isize;
        self.device_state
            .select(Some(((cur + delta).rem_euclid(len)) as usize));
    }

    /// Device-screen keys. The candidate chooser, when open, takes the
    /// directions and Enter for itself; everything else is the list.
    async fn handle_device(&mut self, action: Action) -> Result<()> {
        if self.device_link.is_some() {
            return self.handle_link_picker(action).await;
        }
        match action {
            Action::Up => self.step_device(-1),
            Action::Down => self.step_device(1),
            Action::Back => self.screen = Screen::Menu,
            Action::Select => self.pull_selected(),
            Action::Mark => self.toggle_mark(),
            Action::Sync => self.sync_marked(),
            Action::Link => self.open_link_picker(),
            Action::Rescan => match self.device_root.clone() {
                Some(root) => self.start_scan(root),
                None => self.start_input(InputContext::DevicePath, "device path", ""),
            },
            Action::Query => self.start_input(
                InputContext::DevicePath,
                "device path",
                &self
                    .device_root
                    .as_ref()
                    .map(|r| r.display().to_string())
                    .unwrap_or_default(),
            ),
            _ => self.dirty = false,
        }
        Ok(())
    }

    fn selected_row(&self) -> Option<&DeviceRow> {
        self.device_state
            .selected()
            .and_then(|i| self.device.get(i))
    }

    fn toggle_mark(&mut self) {
        let Some(i) = self.device_state.selected() else {
            return;
        };
        if !self.device_marks.remove(&i) {
            self.device_marks.insert(i);
        }
    }

    /// Queue the selected row's sidecar.
    fn pull_selected(&mut self) {
        let Some(row) = self.selected_row() else {
            return;
        };
        if let DeviceState::Unreadable(d) = &row.book.state {
            // Refusing here rather than letting `sync_device` fail: a scan
            // degrades on a bad sidecar, a sync errors on one, and the reason
            // is already in the row.
            self.status = Some(format!("can't read that one — {}", d.detail));
            return;
        }
        let (path, title) = (row.book.path.clone(), row.book.display_title());
        self.status = Some(format!("bringing {title} across…"));
        self.queue_pull(vec![path]);
    }

    /// Queue every marked row, or — with nothing marked — everything the scan
    /// said there is something to do about.
    fn sync_marked(&mut self) {
        let chosen: Vec<PathBuf> = if self.device_marks.is_empty() {
            self.device
                .iter()
                .filter(|r| r.book.state.is_syncable())
                .map(|r| r.book.path.clone())
                .collect()
        } else {
            // Sorted, so the queue runs in the order the rows are shown.
            let mut marks: Vec<usize> = self.device_marks.iter().copied().collect();
            marks.sort_unstable();
            marks
                .into_iter()
                .filter_map(|i| self.device.get(i))
                .filter(|r| !matches!(r.book.state, DeviceState::Unreadable(_)))
                .map(|r| r.book.path.clone())
                .collect()
        };
        if chosen.is_empty() {
            self.status = Some("nothing to bring across — mark a row with x".into());
            return;
        }
        self.status = Some(format!("bringing {} book(s) across…", chosen.len()));
        self.queue_pull(chosen);
    }

    fn queue_pull(&mut self, paths: Vec<PathBuf>) {
        // An empty queue must never be `Some`: `has_deferred` would then keep
        // the loop's ready-arm firing forever on work that does not exist.
        if paths.is_empty() {
            return;
        }
        self.pending_pull
            .get_or_insert_with(VecDeque::new)
            .extend(paths);
        self.dirty = true;
    }

    /// Take the next sidecar to pull, dropping the queue once it runs dry — see
    /// [`App::queue_pull`] for why an emptied queue cannot be left in place.
    fn next_pull(&mut self) -> Option<PathBuf> {
        let next = self.pending_pull.as_mut().and_then(|q| q.pop_front());
        if self.pending_pull.as_ref().is_some_and(|q| q.is_empty()) {
            self.pending_pull = None;
        }
        next
    }

    /// Pull **one** sidecar. Called by the event loop, once per iteration, so a
    /// forty-book sync redraws between books instead of freezing.
    pub async fn finish_pull(&mut self, path: &Path) -> Result<()> {
        self.dirty = true;
        let left = self.pending_pull.as_ref().map_or(0, |q| q.len());
        let reports = match self
            .engine
            .sync_device(std::slice::from_ref(&path.to_path_buf()))
            .await
        {
            Ok(reports) => reports,
            Err(e) => {
                let msg = format!("pull failed: {e}");
                if let Some(row) = self.row_for_mut(path) {
                    row.note = Some(msg.clone());
                }
                self.status = Some(msg);
                return Ok(());
            }
        };

        let mut line = String::new();
        for report in reports {
            let s = &report.stats;
            line = format!(
                "{}: {} new, {} refreshed, {} already here",
                s.book_title, s.inserted, s.updated, s.skipped
            );
            if let Some(row) = self.row_for_mut(path) {
                row.note = Some(line.clone());
                row.notices = report.warnings.clone();
                row.book.book_id = Some(s.book_id);
                row.book.matched_by = Some(s.matched_by);
                // Everything the sidecar had is here now; the next scan is what
                // re-derives this properly.
                row.book.state = DeviceState::Unchanged;
            }
        }
        // Marks are a selection for work that has now been done.
        if let Some(i) = self.index_of(path) {
            self.device_marks.remove(&i);
        }
        self.status = Some(if left > 0 {
            format!("{line}  ·  {left} to go")
        } else {
            line
        });
        if self.pending_pull.is_none() {
            // The shelf gained books; the library list is what shows them.
            self.refresh_library().await?;
        }
        Ok(())
    }

    fn index_of(&self, path: &Path) -> Option<usize> {
        self.device.iter().position(|r| r.book.path == path)
    }

    fn row_for_mut(&mut self, path: &Path) -> Option<&mut DeviceRow> {
        self.device.iter_mut().find(|r| r.book.path == path)
    }

    /// Offer the engine's candidate band for the selected row.
    fn open_link_picker(&mut self) {
        let Some(row) = self.selected_row() else {
            return;
        };
        let candidates = match &row.book.state {
            DeviceState::New { candidates } => candidates.clone(),
            // Already linked: `l` would repoint it, and doing that silently on
            // a keypress is not something to offer.
            _ => {
                self.status = Some(format!(
                    "{} is already linked — enter brings it across",
                    row.book.display_title()
                ));
                return;
            }
        };
        if candidates.is_empty() {
            self.status = Some(
                "nothing in the library looks like it — enter pulls it in as a new book".into(),
            );
            return;
        }
        let mut state = ListState::default();
        state.select(Some(0));
        self.device_link = Some(LinkPicker {
            path: row.book.path.clone(),
            title: row.book.display_title(),
            candidates,
            state,
        });
        self.status = None;
    }

    async fn handle_link_picker(&mut self, action: Action) -> Result<()> {
        let Some(picker) = self.device_link.as_mut() else {
            return Ok(());
        };
        let len = picker.candidates.len() as isize;
        match action {
            Action::Up | Action::Down => {
                let delta = if action == Action::Up { -1 } else { 1 };
                let cur = picker.state.selected().unwrap_or(0) as isize;
                picker
                    .state
                    .select(Some((cur + delta).rem_euclid(len.max(1)) as usize));
            }
            Action::Back | Action::Left => {
                self.device_link = None;
                self.status = Some("left it unlinked".into());
            }
            Action::Select | Action::Right => self.commit_link().await?,
            _ => self.dirty = false,
        }
        Ok(())
    }

    /// Record the user's choice, then bring the book across.
    ///
    /// `Engine::link_sidecar` writes the mapping as `Manual` — it *repoints*,
    /// unlike the scan's own `link_device_book`, which must never relabel a
    /// decision the user made. Importing straight afterwards is what stops
    /// linking from looking like a key that did nothing.
    async fn commit_link(&mut self) -> Result<()> {
        let Some(picker) = self.device_link.take() else {
            return Ok(());
        };
        let Some(candidate) = picker
            .state
            .selected()
            .and_then(|i| picker.candidates.get(i))
        else {
            return Ok(());
        };
        match self
            .engine
            .link_sidecar(&picker.path, candidate.book_id)
            .await
        {
            Ok(_) => {
                self.status = Some(format!(
                    "linked {} → {} · bringing it across…",
                    picker.title, candidate.title
                ));
                if let Some(row) = self.row_for_mut(&picker.path) {
                    row.book.book_id = Some(candidate.book_id);
                }
                self.queue_pull(vec![picker.path.clone()]);
            }
            Err(e) => self.status = Some(format!("could not link it: {e}")),
        }
        Ok(())
    }

    // ---- deferred work -----------------------------------------------------

    /// Is there work waiting that the loop must not block on `select!` for?
    pub fn has_deferred(&self) -> bool {
        self.pending_verify.is_some() || self.pending_scan.is_some() || self.pending_pull.is_some()
    }

    /// Do **one** unit of deferred work, and say whether anything was done.
    ///
    /// Every engine call in this crate is awaited inline in a key handler, and
    /// that is fine for a lookup and ruinous for a device sync. The work that
    /// blocks the loop lives here instead: the key handler queues it, the loop
    /// draws the frame that announces it, and only then is it run — one book at
    /// a time, so `select!` is reachable between books and the user sees each
    /// one land.
    pub async fn pump_deferred(&mut self) -> Result<bool> {
        if let Some(key) = self.pending_verify.take() {
            self.finish_verify(key).await;
            return Ok(true);
        }
        if let Some(root) = self.pending_scan.take() {
            self.finish_scan(&root).await?;
            return Ok(true);
        }
        if let Some(path) = self.next_pull() {
            self.finish_pull(&path).await?;
            return Ok(true);
        }
        Ok(false)
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

    // ---- the links pane ----------------------------------------------------

    /// Open the graph around the selected note.
    ///
    /// Only from inside the open Notes section: the pane is *about* a note, and
    /// "which one?" has no answer standing anywhere else. Refusing with the way
    /// in beats guessing at the most recent note, which would silently show the
    /// graph of something the user was not looking at.
    async fn open_links(&mut self) -> Result<()> {
        let note = (self.book_tab == BookTab::Notes && self.in_section)
            .then(|| self.selected_note())
            .flatten();
        match note {
            Some(note) => {
                // Asking for the graph is asking for the pane it draws in.
                self.ensure_panel();
                self.show_links_for(note).await
            }
            None => {
                self.status = Some("open Notes and pick one — L then shows its links".into());
                Ok(())
            }
        }
    }

    /// The note the Notes list is standing on.
    fn selected_note(&self) -> Option<NoteRecord> {
        self.tab_state
            .selected()
            .and_then(|i| self.view.as_ref().and_then(|v| v.notes.get(i)))
            .cloned()
    }

    /// Centre the pane on `note`, reading both directions from the engine.
    ///
    /// Two calls, not one: they are the same edge set read from opposite ends,
    /// and the engine keeps them apart precisely so neither side can claim an
    /// edge the other denies.
    async fn show_links_for(&mut self, note: NoteRecord) -> Result<()> {
        let outgoing = self.engine.outgoing_links(note.id).await?;
        let inbound = self.engine.backlinks(note.id).await?;
        let mut rows: Vec<LinkRow> = outgoing
            .into_iter()
            .map(|l| LinkRow::Out {
                // A resolved edge shows the target's own title; a dangling one
                // has nothing but the text the body wrote, which is exactly what
                // it is worth showing.
                title: l
                    .to
                    .as_ref()
                    .map_or_else(|| l.target_title.clone(), |n| n.title.clone()),
                to: l.to,
            })
            .collect();
        rows.extend(inbound.into_iter().map(LinkRow::In));

        let mut state = ListState::default();
        if !rows.is_empty() {
            state.select(Some(0));
        }
        self.links = Some(LinksPane { note, rows, state });
        self.dirty = true;
        Ok(())
    }

    /// Re-read the open pane's graph after a mutation. A no-op when it is shut.
    async fn refresh_links(&mut self) -> Result<()> {
        let Some(note) = self.links.as_ref().map(|p| p.note.clone()) else {
            return Ok(());
        };
        let cursor = self.links.as_ref().and_then(|p| p.state.selected());
        self.show_links_for(note).await?;
        // Keep the cursor where it was when the graph is still that long; the
        // row under it may be a different edge, but jumping to the top on every
        // save reads as the pane resetting itself.
        if let (Some(pane), Some(i)) = (self.links.as_mut(), cursor)
            && i < pane.rows.len()
        {
            pane.state.select(Some(i));
        }
        Ok(())
    }

    fn step_links(&mut self, delta: isize) {
        let Some(pane) = self.links.as_mut() else {
            return;
        };
        let len = pane.rows.len();
        if len == 0 {
            return;
        }
        let cur = pane.state.selected().unwrap_or(0) as isize;
        pane.state
            .select(Some((cur + delta).rem_euclid(len as isize) as usize));
    }

    /// Enter on a pane row: go to that note, or say why there is none.
    ///
    /// A dangling target is not a dead end and is not an error — it is the note
    /// worth writing next. Saying so is the whole difference between a forward
    /// reference and a broken link.
    async fn follow_link(&mut self) -> Result<()> {
        let Some(row) = self.links.as_ref().and_then(|p| p.selected()).cloned() else {
            return Ok(());
        };
        match row {
            LinkRow::Out { title, to: None } => {
                self.status = Some(format!(
                    "“{title}” has no note yet — write one and this link resolves itself"
                ));
                self.dirty = true;
                Ok(())
            }
            LinkRow::Out { to: Some(note), .. } | LinkRow::In(note) => self.goto_note(note).await,
        }
    }

    /// Follow the graph to `note`: its book, its row in the Notes list, and the
    /// pane re-centred on it so the walk can continue.
    ///
    /// Crossing into another book is the point rather than an edge case —
    /// `docs/decisions.md` has book-to-book connection running
    /// reflection-to-reflection, so a link that could not leave its own book
    /// would be a link that could not do its job.
    async fn goto_note(&mut self, note: NoteRecord) -> Result<()> {
        let here = self.view.as_ref().and_then(|v| v.book.id);
        if let Some(target_book) = note.book_id
            && here != Some(target_book)
            && let Some(book) = self.engine.get_book(target_book).await?
        {
            // Clears the pane and the tab state; both are set again below.
            self.open_book(book).await?;
        }
        self.book_tab = BookTab::Notes;
        self.in_section = true;
        match self
            .view
            .as_ref()
            .and_then(|v| v.notes.iter().position(|n| n.id == note.id))
        {
            Some(i) => {
                self.tab_state.select(Some(i));
                self.status = None;
            }
            // A note filed under no book (or under one that has since gone) has
            // no row to stand on. The pane still centres on it — it holds the
            // record whole — so the graph stays walkable either way.
            None => {
                self.status = Some(format!("“{}” is not filed under this book", note.title));
            }
        }
        self.show_links_for(note).await
    }

    /// Ask to delete the highlighted note — only in the open Notes section.
    fn ask_delete_selected_note(&mut self) {
        // With the pane open the Notes list is not on screen, and after a
        // cross-book jump its selection is not what the pane is showing either.
        // `d` would then delete a note the user cannot see.
        if self.links.is_some() {
            self.dirty = false;
            return;
        }
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
                // A review is the only note kind that carries a rating, so
                // saving one asks for it — the same "ask after the body" shape
                // as a new note's page anchor.
                if note.kind == NoteKind::Review.as_str() {
                    self.pending_rating = Some(note.id);
                    self.start_input(InputContext::ReviewRating, "rating (enter to skip)", "");
                }
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
                // An ordinary note floats free of any one reading; reflections
                // and reviews are the anchored pair, and they have no TUI
                // surface yet.
                reading_id: None,
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
            .update_progress(id, None, Some(!finished))
            .await?;
        self.reload_view().await?;
        // Finishing a book closes its reading, so it leaves the home shelf —
        // and unfinishing reopens the same one, so it comes back.
        self.refresh_reading().await?;
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
        let vault = self.engine.vault_dir();
        let dir = vault.parent().unwrap_or(vault).to_path_buf();
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
                } else if context == InputContext::ReviewRating {
                    // The review is already saved; the rating was optional.
                    self.pending_rating = None;
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
        // Same for the rating: an empty one has to reach its own handler so the
        // pending review is dropped rather than left waiting for a box that has
        // already closed.
        if context == InputContext::ReviewRating {
            return self.commit_rating(text).await;
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
            InputContext::DevicePath => {
                self.screen = Screen::Device;
                self.start_scan(PathBuf::from(text));
            }
            InputContext::ProgressPage => self.commit_progress(text).await?,
            InputContext::AccentHex => match theme::parse_hex(&text) {
                Some(rgb) => self.set_accent_rgb(rgb),
                None => self.status = Some(format!("not a #RRGGBB color: {text}")),
            },
            InputContext::NotePage | InputContext::ReviewRating => {
                unreachable!("handled above")
            }
        }
        Ok(())
    }

    /// Rate the review that was just saved.
    ///
    /// The value goes through `Storage::set_review_rating`, which snaps it with
    /// [`readingbuddy::RatingScale::canonical`] — the one quantizer both sides
    /// of the rating machinery go through. A value that is not a point on the
    /// scale comes back as an error naming the scale's bounds and step, and is
    /// **shown, never rounded**; so is a value the Goodreads map has no entry
    /// for, because Goodreads takes integers 0–5 and what a half means is the
    /// user's call, not ours.
    async fn commit_rating(&mut self, text: String) -> Result<()> {
        let Some(note_id) = self.pending_rating.take() else {
            return Ok(());
        };
        let text = text.trim();
        if text.is_empty() {
            return Ok(());
        }
        let Ok(value) = text.parse::<f64>() else {
            self.status = Some(format!("'{text}' isn't a number"));
            return Ok(());
        };
        if let Err(e) = self.engine.set_rating(note_id, value).await {
            self.status = Some(format!("{e}"));
            return Ok(());
        }
        self.status = Some(match self.engine.goodreads_rating(note_id).await {
            Ok(Some(g)) => format!("rated {value} · goodreads {g}"),
            Ok(None) => format!("rated {value}"),
            Err(EngineError::UnmappedRating { value, scale }) => {
                format!("rated {value} · no goodreads mapping on '{scale}' for it yet")
            }
            Err(e) => format!("rated {value}, but: {e}"),
        });
        Ok(())
    }

    async fn commit_progress(&mut self, text: String) -> Result<()> {
        let Some(id) = self.view.as_ref().and_then(|v| v.book.id) else {
            return Ok(());
        };
        match text.parse::<i64>() {
            Ok(page) => {
                self.engine.update_progress(id, Some(page), None).await?;
                self.reload_view().await?;
                // Progress opens a reading when the book had none, so a page
                // typed here is what puts a book on the home shelf.
                self.refresh_reading().await?;
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

/// The next reader to arrive or leave, or never.
///
/// Never, rather than `None`, on purpose: a `select!` arm that resolves
/// immediately and forever is a spin, and a watcher whose source has died is
/// exactly that. The rest of the app carries on without it — a mount that has to
/// be found by pressing `r` is what we had yesterday.
async fn next_mount(watcher: &mut Option<MountWatcher>) -> MountEvent {
    match watcher {
        Some(w) => match w.next().await {
            Some(event) => event,
            None => std::future::pending().await,
        },
        None => std::future::pending().await,
    }
}

/// The event loop: crossterm events, a 20fps animation tick, and the mount
/// watcher — redrawing only when something actually changed.
///
/// `mounts` is an `Option` because not every machine can watch, and one that
/// cannot is a machine that still runs the app.
pub async fn run<B: Backend>(
    terminal: &mut Terminal<B>,
    app: &mut App,
    mut mounts: Option<MountWatcher>,
) -> Result<()> {
    let mut events = EventStream::new();
    let mut ticker = tokio::time::interval(TICK);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    redraw(terminal, app)?;

    loop {
        // `biased`, so a keypress is still seen first while a sync is running;
        // the last arm is what stops the loop *waiting* for one when there is
        // queued work — without it a forty-book sync would advance one book per
        // 50ms tick.
        tokio::select! {
            biased;
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
            // Above the ready-arm, so a device plugged in mid-sync is still
            // noticed; below the keypress, so it never gets in front of one. The
            // handler only queues work — the walk itself goes through
            // `pending_scan` like every other one.
            event = next_mount(&mut mounts) => app.on_mount_event(event),
            _ = std::future::ready(()), if app.has_deferred() => {}
        }

        if app.quit {
            break;
        }
        if app.dirty {
            redraw(terminal, app)?;
        }
        // Deferred work runs *after* the frame that announced it — "verifying…",
        // "scanning…", "bringing 12 books across…" — and one unit at a time, so
        // the next iteration comes straight back here with a redraw between.
        if app.pump_deferred().await? {
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
    } else if let Some(action) = crate::event::map_key_on(app.screen, key) {
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
            files_dir: tmp.join("files"),
            vault_dir: tmp.join("vault"),
            log_dir: tmp.join("logs"),
            google_api_key: None,
            // The TUI has no calibre surface yet; detection falls through to
            // `PATH`, which costs two `stat` sweeps and finds nothing to do.
            calibre_bin_dir: None,
        };
        let engine = Engine::open(config).await.expect("engine");
        let book = engine
            .save_book(&Book {
                title: Some("Station Eleven".into()),
                authors: vec!["Emily St. John Mandel".into()],
                page_count: Some(333),
                isbn_13: Some("9781447268963".into()),
                ..Book::default()
            })
            .await
            .expect("save");
        let id = book.id.expect("id");
        // Progress is a reading, not a book column, since engine migration
        // `0005` — setting `current_page` on the `Book` above would be silently
        // ignored and every progress-bearing layout would render blank.
        engine
            .update_progress(id, Some(120), None)
            .await
            .expect("seed progress");
        // Seed one of each so the tab lists render with content.
        engine
            .storage()
            .insert_highlight(
                id,
                &readingbuddy::storage::NewHighlight {
                    text: "survival is insufficient".into(),
                    chapter: Some("1".into()),
                    page: Some(58),
                    pos0: Some("/body/p[1]".into()),
                    pos1: None,
                    ko_datetime: Some("2026-01-01 10:00:00".into()),
                    ko_datetime_updated: None,
                    color: None,
                    note: None,
                    source: "koreader".into(),
                },
            )
            .await
            .expect("highlight");
        engine
            .storage()
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
        let mut app = App::new(engine).await.expect("app");
        // The device screen is seeded from a real fixture tree, scanned through
        // the real engine: every state below is one the scan actually reached,
        // so a change to the four-state logic shows up here rather than in a
        // hand-built list that agrees with nothing.
        let root = write_device_tree(&tmp.join("device"));
        app.finish_scan(&root).await.expect("scan");
        app
    }

    /// An open reading, for the dev aids that need rows without a database
    /// behind them. Never used by an assertion — everything that checks
    /// behaviour goes through the real engine.
    fn sample_reading(ko_percent: Option<f64>) -> Reading {
        Reading {
            id: 1,
            book_id: 1,
            started_at: None,
            finished_at: None,
            status: "reading".into(),
            source: "manual".into(),
            current_page: None,
            ko_status: None,
            ko_percent,
            ko_rating: None,
            created_at: 0,
            last_modified: 0,
        }
    }

    /// Where a menu row is, by identity rather than by index — the array grew a
    /// row for the home screen, and a hard-coded index is how that becomes a
    /// test that opens something else and still passes.
    fn menu_row(item: MenuItem) -> usize {
        MENU.iter()
            .position(|(it, ..)| *it == item)
            .unwrap_or_else(|| panic!("no {item:?} menu row"))
    }

    /// A miniature device: one book the library already has with an annotation
    /// it does not (Updated), one it has never seen (New), and one sidecar that
    /// is not Lua at all (Unreadable).
    fn write_device_tree(root: &std::path::Path) -> PathBuf {
        let sidecar = |dir: &str, title: &str, authors: &str, md5: &str, text: &str| {
            let d = root.join(dir);
            std::fs::create_dir_all(&d).expect("sdr dir");
            std::fs::write(
                d.join("metadata.epub.lua"),
                format!(
                    r#"return {{
    ["annotations"] = {{
        [1] = {{
            ["chapter"] = "One",
            ["datetime"] = "2026-02-02 09:00:00",
            ["pageno"] = 12,
            ["pos0"] = "/body/DocFragment[1]/body/p[3]/text().0",
            ["pos1"] = "/body/DocFragment[1]/body/p[3]/text().20",
            ["text"] = "{text}",
        }},
    }},
    ["doc_props"] = {{
        ["authors"] = "{authors}",
        ["title"] = "{title}",
    }},
    ["percent_finished"] = 0.42,
    ["partial_md5_checksum"] = "{md5}",
}}
"#
                ),
            )
            .expect("sidecar");
        };
        sidecar(
            "Station Eleven.sdr",
            "Station Eleven",
            "Emily St. John Mandel",
            "1111111111111111111111111111aaaa",
            "the Museum of Civilization",
        );
        sidecar(
            "Piranesi.sdr",
            "Piranesi",
            "Susanna Clarke",
            "2222222222222222222222222222bbbb",
            "the Beauty of the House is immeasurable",
        );
        let broken = root.join("Broken.sdr");
        std::fs::create_dir_all(&broken).expect("sdr dir");
        std::fs::write(broken.join("metadata.epub.lua"), "not lua {{{ &&&").expect("broken");
        root.to_path_buf()
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
    /// A `CrosstermBackend` whose size comes from the test, not from the OS.
    ///
    /// The byte-budget tests below have to use a *real* `CrosstermBackend` —
    /// counting what ratatui actually puts on the wire is the entire point, and
    /// `TestBackend` writes no bytes at all. But `CrosstermBackend::size()` is
    /// an ioctl on the process's terminal, and `redraw` calls `terminal.size()`
    /// on every frame (`app.rs`, the resize hook that invalidates a transmitted
    /// image). A fixed `Viewport` does not help: it changes what ratatui asks
    /// for, not what `redraw` asks for.
    ///
    /// So on a machine with no terminal at all — a CI runner, a container — the
    /// ioctl fails with `NotFound` and the test dies on its first draw. It
    /// passed for a year only because it had never been run anywhere without a
    /// tty, which is precisely what widening the CI gate to the workspace
    /// exposed on the first run.
    ///
    /// Everything except `size` and `window_size` delegates, so the bytes being
    /// counted are still the real terminal's.
    struct FixedSize<B> {
        inner: B,
        size: ratatui::layout::Size,
    }

    impl<B: Backend> Backend for FixedSize<B> {
        fn draw<'a, I>(&mut self, content: I) -> std::io::Result<()>
        where
            I: Iterator<Item = (u16, u16, &'a ratatui::buffer::Cell)>,
        {
            self.inner.draw(content)
        }
        fn hide_cursor(&mut self) -> std::io::Result<()> {
            self.inner.hide_cursor()
        }
        fn show_cursor(&mut self) -> std::io::Result<()> {
            self.inner.show_cursor()
        }
        fn get_cursor_position(&mut self) -> std::io::Result<Position> {
            self.inner.get_cursor_position()
        }
        fn set_cursor_position<P: Into<Position>>(&mut self, position: P) -> std::io::Result<()> {
            self.inner.set_cursor_position(position)
        }
        fn clear(&mut self) -> std::io::Result<()> {
            self.inner.clear()
        }
        fn size(&self) -> std::io::Result<ratatui::layout::Size> {
            Ok(self.size)
        }
        fn window_size(&mut self) -> std::io::Result<ratatui::backend::WindowSize> {
            Ok(ratatui::backend::WindowSize {
                columns_rows: self.size,
                // Zero means "unknown" to every caller in this crate; a pixel
                // geometry invented here would be a lie the rich path could act
                // on.
                pixels: ratatui::layout::Size {
                    width: 0,
                    height: 0,
                },
            })
        }
        fn flush(&mut self) -> std::io::Result<()> {
            self.inner.flush()
        }
    }

    /// The terminal the byte-budget tests measure through: a real
    /// `CrosstermBackend` writing into a counted `Vec`, with its size pinned.
    type CountedTestTerminal = ratatui::Terminal<
        FixedSize<ratatui::backend::CrosstermBackend<crate::perf::CountingWriter<Vec<u8>>>>,
    >;

    /// A counted terminal of exactly `w x h`, independent of the machine.
    fn counted_terminal(w: u16, h: u16) -> (crate::perf::Meter, CountedTestTerminal) {
        use crate::perf::{ByteClass, CountingWriter, Meter};
        use ratatui::backend::CrosstermBackend;
        use ratatui::{TerminalOptions, Viewport};

        let meter = Meter::new();
        let backend = FixedSize {
            inner: CrosstermBackend::new(CountingWriter::new(
                Vec::new(),
                meter.clone(),
                ByteClass::Text,
            )),
            size: ratatui::layout::Size {
                width: w,
                height: h,
            },
        };
        let terminal = ratatui::Terminal::with_options(
            backend,
            TerminalOptions {
                viewport: Viewport::Fixed(ratatui::layout::Rect::new(0, 0, w, h)),
            },
        )
        .expect("terminal");
        (meter, terminal)
    }

    /// the baseline the hybrid's motion frames have to match.
    async fn glyph_spin_bytes(w: u16, h: u16, ticks: u32) -> u64 {
        let mut app = test_app().await;
        let book = app.library.first().cloned().expect("seeded book");
        app.open_book(book).await.expect("open");
        app.render_mode = RenderMode::Glyph;
        app.spinning = true;

        let (meter, mut terminal) = counted_terminal(w, h);

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
        let mut app = test_app().await;
        app.screen = Screen::Menu;
        app.ambient = crate::ambient::Ambient::new(motif);

        let (meter, mut terminal) = counted_terminal(w, h);

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

        let mut app = test_app().await;
        let book = app.library.first().cloned().expect("seeded book");
        app.open_book(book).await.expect("open");
        app.render_mode = RenderMode::Rich;
        app.spinning = true;

        let (meter, mut terminal) = counted_terminal(50, 26);
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
                Screen::Home,
                Screen::Menu,
                Screen::Library,
                Screen::Book,
                Screen::Search,
                Screen::Settings,
                Screen::Device,
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
            // The home screen with nothing open: its own branch of `home::draw`
            // (the empty box), and the one likeliest to underflow at 1x1.
            app.screen = Screen::Home;
            let held = std::mem::take(&mut app.reading);
            terminal
                .draw(|f| ui::draw(f, app))
                .expect("draw empty home");
            app.reading = held;
            // With the device screen's candidate chooser open, and with a row
            // marked — both are overlay/gutter arithmetic that 1x1 tests.
            app.screen = Screen::Device;
            app.device_marks.insert(0);
            app.device_link = Some(LinkPicker {
                path: PathBuf::from("/mnt/Piranesi.sdr/metadata.epub.lua"),
                title: "Piranesi".into(),
                candidates: vec![MatchCandidate {
                    book_id: 1,
                    title: "Piranesi's House".into(),
                    score: 0.71,
                }],
                state: ListState::default(),
            });
            terminal.draw(|f| ui::draw(f, app)).expect("draw link");
            app.device_link = None;
            app.device_marks.clear();
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
            // With the links pane standing in for the Notes list — populated,
            // then empty, since the two take different branches and 1x1 is where
            // either one's arithmetic would go wrong. Hand-built like the
            // candidate chooser above: the sweep stays synchronous, and what is
            // being exercised is the layout, not the engine's query.
            app.book_tab = BookTab::Notes;
            app.in_section = true;
            for rows in [
                vec![
                    LinkRow::Out {
                        title: "Symphony".into(),
                        to: Some(sample_note(2, "Symphony")),
                    },
                    LinkRow::Out {
                        title: "Nowhere".into(),
                        to: None,
                    },
                    LinkRow::In(sample_note(3, "Doctor Eleven")),
                ],
                Vec::new(),
            ] {
                let mut state = ListState::default();
                if !rows.is_empty() {
                    state.select(Some(0));
                }
                app.links = Some(LinksPane {
                    note: sample_note(1, "Hub"),
                    rows,
                    state,
                });
                terminal.draw(|f| ui::draw(f, app)).expect("draw links");
            }
            app.links = None;
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

    /// A note record with nothing but an id and a title — enough for a row.
    fn sample_note(id: i64, title: &str) -> NoteRecord {
        NoteRecord {
            id,
            book_id: Some(1),
            reading_id: None,
            highlight_id: None,
            page: None,
            location: None,
            file_path: format!("book/{id}.md"),
            title: title.into(),
            kind: "note".into(),
            created_at: None,
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
        let path = app.engine.vault_dir().join(&note.file_path);
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
        let path = app.engine.vault_dir().join(&note.file_path);
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
        let path = app.engine.note_path(&app.view.as_ref().unwrap().notes[0]);
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

    // ---- the links pane ----------------------------------------------------

    /// A small graph in the open book, built through the real engine so the
    /// edges are the ones `write_links` actually resolves.
    ///
    /// Order is load-bearing: `Symphony` exists *before* the hub links to it, so
    /// that edge resolves at insert; `Nowhere` never exists, so its edge stays
    /// text; `Doctor Eleven` is written last and links back at the hub by title.
    /// Returns `(hub, symphony, inbound)` note ids.
    async fn seed_graph(app: &mut App) -> (i64, i64, i64) {
        let book_id = app.view.as_ref().and_then(|v| v.book.id);
        let make = async |title: &str, body: &str| {
            app.engine
                .create_note(NewNoteInput {
                    book_id,
                    title: Some(title.into()),
                    body: body.into(),
                    ..NewNoteInput::default()
                })
                .await
                .expect("note")
                .id
        };
        let symphony = make("Symphony", "the travelling orchestra").await;
        let hub = make("Hub", "See [[Symphony]] and [[Nowhere]].").await;
        let inbound = make("Doctor Eleven", "Back to [[Hub]].").await;
        app.reload_view().await.expect("reload");
        (hub, symphony, inbound)
    }

    /// Seed the graph, stand on the hub in the Notes list, and press `L`.
    async fn open_graph_pane(app: &mut App) -> (i64, i64, i64) {
        let ids = seed_graph(app).await;
        app.book_tab = BookTab::Notes;
        app.in_section = true;
        let i = app
            .view
            .as_ref()
            .unwrap()
            .notes
            .iter()
            .position(|n| n.id == ids.0)
            .expect("hub is in the book's notes");
        app.tab_state.select(Some(i));
        app.handle(Action::Links).await.expect("open links");
        ids
    }

    /// The two directions are two sets, not one list with a flag — and the
    /// dangling target is in the outbound one rather than dropped.
    #[tokio::test]
    async fn the_pane_reads_both_directions_and_keeps_a_dangling_target() {
        let mut app = test_app().await;
        let book = app.library.first().cloned().expect("seeded book");
        app.open_book(book).await.expect("open");
        let (hub, symphony, inbound) = open_graph_pane(&mut app).await;

        let pane = app.links.as_ref().expect("pane open");
        assert_eq!(pane.note.id, hub);
        assert_eq!((pane.out_count(), pane.in_count()), (2, 1));

        // Outbound first, in the order the body wrote them.
        match &pane.rows[0] {
            LinkRow::Out { title, to: Some(n) } => {
                assert_eq!(title, "Symphony");
                assert_eq!(n.id, symphony);
            }
            other => panic!("first row is the resolved outbound link, got {other:?}"),
        }
        match &pane.rows[1] {
            LinkRow::Out { title, to: None } => assert_eq!(title, "Nowhere"),
            other => panic!("the dangling target is kept as text, got {other:?}"),
        }
        match &pane.rows[2] {
            LinkRow::In(n) => assert_eq!(n.id, inbound),
            other => panic!("inbound comes after outbound, got {other:?}"),
        }
    }

    /// Enter on a dangling target says what it is. Doing nothing would make the
    /// forward reference — the one row that is *about* a note not written yet —
    /// the only dead end in the view.
    #[tokio::test]
    async fn a_dangling_target_names_itself_rather_than_doing_nothing() {
        let mut app = test_app().await;
        let book = app.library.first().cloned().expect("seeded book");
        app.open_book(book).await.expect("open");
        let (hub, _, _) = open_graph_pane(&mut app).await;

        app.handle(Action::Down).await.expect("down"); // onto "Nowhere"
        app.handle(Action::Select).await.expect("follow");

        let status = app.status.clone().expect("said something");
        assert!(status.contains("Nowhere"), "{status}");
        assert_eq!(
            app.links.as_ref().expect("pane still open").note.id,
            hub,
            "a dangling target must not move the pane"
        );
    }

    /// Following a resolved edge lands on that note: selected in the list, and
    /// the pane re-centred so the walk can carry on from there.
    #[tokio::test]
    async fn following_a_resolved_link_moves_to_that_note() {
        let mut app = test_app().await;
        let book = app.library.first().cloned().expect("seeded book");
        app.open_book(book).await.expect("open");
        let (hub, symphony, inbound) = open_graph_pane(&mut app).await;

        // Row 0 is the resolved outbound edge.
        app.handle(Action::Select).await.expect("follow out");
        let pane = app.links.as_ref().expect("pane open");
        assert_eq!(pane.note.id, symphony);
        let selected = app
            .tab_state
            .selected()
            .and_then(|i| app.view.as_ref().unwrap().notes.get(i))
            .map(|n| n.id);
        assert_eq!(selected, Some(symphony), "the notes list follows too");

        // Symphony is linked to from the hub and from the book's seeded note,
        // so walking back inbound is how the graph is read from the other end.
        let pane = app.links.as_ref().unwrap();
        assert_eq!(pane.out_count(), 0);
        assert!(pane.in_count() >= 1);
        let back: Vec<i64> = pane
            .rows
            .iter()
            .filter_map(|r| match r {
                LinkRow::In(n) => Some(n.id),
                _ => None,
            })
            .collect();
        assert!(back.contains(&hub), "the hub links here: {back:?}");
        assert!(!back.contains(&inbound));
    }

    /// Esc backs out one level — into the note list the pane replaced, not out
    /// of the section and not out of the book.
    #[tokio::test]
    async fn esc_closes_the_pane_back_onto_the_note_list() {
        let mut app = test_app().await;
        let book = app.library.first().cloned().expect("seeded book");
        app.open_book(book).await.expect("open");
        open_graph_pane(&mut app).await;

        app.handle(Action::Back).await.expect("esc");
        assert!(app.links.is_none());
        assert!(app.in_section && app.book_tab == BookTab::Notes);
        assert_eq!(app.screen, Screen::Book);

        // And `L` is a toggle from the other side as well.
        app.handle(Action::Links).await.expect("reopen");
        assert!(app.links.is_some());
        app.handle(Action::Links).await.expect("close");
        assert!(app.links.is_none());

        // Opening it with the section pane dismissed brings the pane back —
        // a cursor moving inside a panel nobody can see is a dead end wearing
        // the costume of a working key.
        app.layout.panel = false;
        app.handle(Action::Links).await.expect("open with no panel");
        assert!(app.links.is_some() && app.layout.panel);
    }

    /// `L` outside the open Notes section has no note to be about, and says so
    /// with the way in rather than guessing at one.
    #[tokio::test]
    async fn the_pane_refuses_with_a_next_move_when_no_note_is_selected() {
        let mut app = test_app().await;
        let book = app.library.first().cloned().expect("seeded book");
        app.open_book(book).await.expect("open");

        app.book_tab = BookTab::Info;
        app.in_section = true;
        app.handle(Action::Links).await.expect("L on Info");
        assert!(app.links.is_none());
        assert!(app.status.as_deref().is_some_and(|s| s.contains("Notes")));

        // On the section menu (not entered) it is the same answer.
        app.status = None;
        app.book_tab = BookTab::Notes;
        app.in_section = false;
        app.handle(Action::Links).await.expect("L on the menu");
        assert!(app.links.is_none());
        assert!(app.status.is_some());
    }

    /// A note with no edges at all still says something. An empty box is the
    /// shape of a dead end even when nothing is wrong.
    #[tokio::test]
    async fn a_note_with_no_links_renders_a_reason_rather_than_a_blank_box() {
        let mut app = test_app().await;
        let book = app.library.first().cloned().expect("seeded book");
        app.open_book(book).await.expect("open");
        let lonely = app
            .engine
            .create_note(NewNoteInput {
                book_id: app.view.as_ref().and_then(|v| v.book.id),
                title: Some("Alone".into()),
                body: "no links in this one".into(),
                ..NewNoteInput::default()
            })
            .await
            .expect("note");
        app.reload_view().await.expect("reload");
        let i = app
            .view
            .as_ref()
            .unwrap()
            .notes
            .iter()
            .position(|n| n.id == lonely.id)
            .expect("in the list");
        app.book_tab = BookTab::Notes;
        app.in_section = true;
        app.tab_state.select(Some(i));
        app.handle(Action::Links).await.expect("open links");

        assert!(app.links.as_ref().expect("pane").rows.is_empty());
        assert!(
            screen_text(&mut app, 110, 32).contains("wikilink"),
            "the empty pane must explain what makes an edge"
        );
    }

    /// The rendered pane, which is where "distinguishable" has to be true: the
    /// direction is an arrow in the text and the dangling target is words, so
    /// both survive a `REVERSED` selection and a monochrome terminal.
    #[tokio::test]
    async fn the_rendered_pane_distinguishes_the_directions_and_the_dangling_row() {
        let mut app = test_app().await;
        let book = app.library.first().cloned().expect("seeded book");
        app.open_book(book).await.expect("open");
        open_graph_pane(&mut app).await;

        let text = screen_text(&mut app, 110, 32);
        assert!(text.contains("→ Symphony"), "{text}");
        assert!(text.contains("→ Nowhere"), "{text}");
        assert!(text.contains("(no note yet)"), "{text}");
        assert!(text.contains("← Doctor Eleven"), "{text}");
        assert!(text.contains("2 out · 1 in"), "{text}");
        // The section header renames itself, so the pane is not mistaken for the
        // note list it stands in for.
        assert!(text.contains("Links"), "{text}");
    }

    /// The whole rendered buffer as one string, rows joined by newlines.
    fn screen_text(app: &mut App, w: u16, h: u16) -> String {
        let mut t = ratatui::Terminal::new(TestBackend::new(w, h)).expect("terminal");
        t.draw(|f| ui::draw(f, app)).expect("draw");
        let buf = t.backend().buffer();
        (0..h)
            .map(|y| (0..w).map(|x| buf[(x, y)].symbol()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n")
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

    // ---- the home screen ---------------------------------------------------

    /// The front door is what you are reading, and the menu is still one key
    /// away — which is what makes moving it off the front door cost nothing.
    #[tokio::test]
    async fn the_app_opens_onto_what_you_are_reading() {
        let mut app = test_app().await;
        assert_eq!(app.screen, Screen::Home);
        assert_eq!(app.reading.len(), 1, "the seeded book has an open reading");
        assert_eq!(app.reading_state.selected(), Some(0));

        let text = screen_text(&mut app, 80, 20);
        assert!(text.contains("Station Eleven"), "{text}");
        // Its progress, from the reading — 120 of 333.
        assert!(text.contains("36%"), "{text}");

        // `m` reaches the menu, and Esc comes back rather than quitting.
        app.handle(Action::Menu).await.expect("menu");
        assert_eq!(app.screen, Screen::Menu);
        app.handle(Action::Back).await.expect("back");
        assert_eq!(app.screen, Screen::Home);
        assert!(!app.quit, "esc out of the menu killed the app");

        // And the menu carries a row back here, so it is findable.
        app.screen = Screen::Menu;
        app.menu_index = menu_row(MenuItem::Home);
        app.handle(Action::Select).await.expect("home");
        assert_eq!(app.screen, Screen::Home);
    }

    /// The screen counts nothing. `docs/decisions.md` bans task-completion
    /// framing by name, and a tally of unfinished books is exactly the thing a
    /// currently-reading screen invites.
    #[tokio::test]
    async fn the_home_screen_greets_you_with_no_numbers() {
        let mut app = test_app().await;
        // The fixture's device scan leaves its own report in the status line;
        // arriving here at startup there is none.
        app.status = None;
        let text = screen_text(&mut app, 80, 20);
        // The one number on screen is this row's own progress; nothing counts
        // rows, and nothing says how many anything there are.
        for line in text.lines() {
            let digits: String = line.chars().filter(|c| c.is_ascii_digit()).collect();
            assert!(
                digits.is_empty() || line.contains("Station Eleven"),
                "a number that is not a book's own progress: {line}"
            );
        }
    }

    /// Nothing open is still a place: it says where the books come from, and
    /// the key it names actually works.
    #[tokio::test]
    async fn the_empty_shelf_offers_a_way_onward() {
        let mut app = test_app().await;
        let id = app.library[0].id.expect("id");
        // Finishing closes the reading, which is the ordinary way this screen
        // empties.
        app.engine
            .update_progress(id, None, Some(true))
            .await
            .expect("finish");
        app.refresh_reading().await.expect("refresh");
        assert!(app.reading.is_empty());

        let text = screen_text(&mut app, 80, 20);
        assert!(text.contains("nothing open"), "{text}");
        assert!(text.contains("search"), "no way onward: {text}");
        assert!(text.contains("menu"), "no way onward: {text}");

        // And the key it advertises does what it says.
        app.handle(Action::Query).await.expect("search");
        assert_eq!(app.screen, Screen::Search);
        assert!(app.input.is_some(), "/ opened no query box");
    }

    /// The round trip the whole item is for: home → book → the reflection, in
    /// the in-house editor, and back to a Notes list that shows it as one.
    #[tokio::test]
    async fn home_opens_a_book_and_its_reflection() {
        let mut app = test_app().await;
        app.handle(Action::Select).await.expect("open the book");
        assert_eq!(app.screen, Screen::Book);
        assert_eq!(
            app.view.as_ref().unwrap().book.display_title(),
            "Station Eleven"
        );

        // `e` opens the reflection straight into the editor — no `$EDITOR`.
        app.handle(Action::Reflect).await.expect("reflect");
        let draft = app.note_editor.as_ref().expect("the editor opened");
        assert_eq!(draft.title(), "reflection");

        for c in "the museum of civilization".chars() {
            app.on_editor_key(KeyEvent::from(KeyCode::Char(c)))
                .await
                .expect("type");
        }
        app.on_editor_key(KeyEvent::from(KeyCode::Enter))
            .await
            .expect("save");
        assert!(app.note_editor.is_none());

        let notes = &app.view.as_ref().unwrap().notes;
        let reflection = notes
            .iter()
            .find(|n| n.kind == "reflection")
            .expect("the reflection is in the Notes list");
        assert!(reflection.reading_id.is_some(), "anchored to the reading");
        assert_eq!(
            app.engine.note_body(reflection).unwrap().trim(),
            "the museum of civilization"
        );

        // It is marked in the list rather than hidden in a tab of its own.
        app.book_tab = BookTab::Notes;
        app.in_section = true;
        app.clamp_tab_selection();
        assert!(screen_text(&mut app, 120, 40).contains('◆'));
    }

    /// The accretion rule, from the frontend: a reflection is one note per
    /// reading, opened again and again.
    #[tokio::test]
    async fn opening_a_reflection_twice_is_the_same_note() {
        let mut app = test_app().await;
        app.handle(Action::Reflect).await.expect("first");
        let first = match &app.note_editor.as_ref().unwrap().target {
            NoteTarget::Edit(n) => n.id,
            _ => panic!("a reflection opens as an edit of the note that exists"),
        };
        app.note_editor = None;

        app.handle(Action::Reflect).await.expect("second");
        let second = match &app.note_editor.as_ref().unwrap().target {
            NoteTarget::Edit(n) => n.id,
            _ => panic!("edit"),
        };
        assert_eq!(first, second, "a second press started a second reflection");

        let id = app.library[0].id.expect("id");
        let notes = app.engine.list_notes(Some(id)).await.expect("notes");
        assert_eq!(notes.iter().filter(|n| n.kind == "reflection").count(), 1);
    }

    /// A review is the one kind that carries a rating, so saving one asks —
    /// and an answer that is not on the scale is reported, never rounded.
    #[tokio::test]
    async fn a_review_asks_for_its_rating_and_never_rounds_one() {
        let mut app = test_app().await;
        app.handle(Action::Review).await.expect("review");
        assert_eq!(app.note_editor.as_ref().unwrap().title(), "review");
        for c in "worth your time".chars() {
            app.on_editor_key(KeyEvent::from(KeyCode::Char(c)))
                .await
                .expect("type");
        }
        app.on_editor_key(KeyEvent::from(KeyCode::Enter))
            .await
            .expect("save");
        assert_eq!(
            app.input.as_ref().map(|i| i.context),
            Some(InputContext::ReviewRating),
            "saving a review did not ask for its rating"
        );

        // 4.25 is not a point on the seeded 0–5 step-0.5 scale: said, not
        // snapped.
        for c in "4.25".chars() {
            app.on_input_key(KeyEvent::from(KeyCode::Char(c)))
                .await
                .expect("type");
        }
        app.on_input_key(KeyEvent::from(KeyCode::Enter))
            .await
            .expect("commit");
        let status = app.status.clone().unwrap_or_default();
        assert!(
            status.contains("4.25") && status.contains("scale"),
            "{status}"
        );

        // 4.5 *is* on the scale and has no Goodreads meaning, which is the
        // other half of the same rule: kept, and said.
        let note_id = app
            .engine
            .list_notes(Some(app.library[0].id.unwrap()))
            .await
            .expect("notes")
            .into_iter()
            .find(|n| n.kind == "review")
            .expect("the review")
            .id;
        app.pending_rating = Some(note_id);
        app.commit_rating("4.5".into()).await.expect("rate");
        let status = app.status.clone().unwrap_or_default();
        assert!(status.contains("rated 4.5"), "{status}");
        assert!(status.contains("goodreads"), "{status}");

        // A whole star maps, and says so.
        app.pending_rating = Some(note_id);
        app.commit_rating("4".into()).await.expect("rate");
        assert_eq!(app.status.as_deref(), Some("rated 4 · goodreads 4"));
    }

    /// Finishing a book takes it off the shelf, and unfinishing puts it back —
    /// the home screen is a view of `readings`, not a list it keeps by hand.
    #[tokio::test]
    async fn finishing_a_book_takes_it_off_the_shelf() {
        let mut app = test_app().await;
        app.handle(Action::Select).await.expect("open");
        app.handle(Action::ToggleFinished).await.expect("finish");
        assert!(app.reading.is_empty(), "a finished book is still 'reading'");
        app.handle(Action::ToggleFinished).await.expect("unfinish");
        assert_eq!(app.reading.len(), 1, "unfinishing reopened nothing");
    }

    /// The screen navigates and is never a dead end.
    #[tokio::test]
    async fn the_home_screen_navigates_and_reaches_the_menu() {
        let mut app = test_app().await;
        app.handle(Action::Up).await.expect("up");
        assert_eq!(app.reading_state.selected(), Some(0), "one row wraps to it");
        app.handle(Action::Menu).await.expect("menu");
        assert_eq!(app.screen, Screen::Menu);
    }

    // ---- the device screen -------------------------------------------------

    /// The scan produces all four states against a fixture tree, and each row
    /// is the one the tree deserves.
    #[tokio::test]
    async fn a_scan_seeds_one_row_per_book_in_the_state_it_deserves() {
        let app = test_app().await;
        assert_eq!(app.device.len(), 3, "one row per sidecar");
        assert!(app.device_root.is_some());
        assert_eq!(app.device_state.selected(), Some(0));

        let by_title = |t: &str| {
            app.device
                .iter()
                .find(|r| r.book.display_title() == t)
                .unwrap_or_else(|| panic!("no row for {t}"))
        };
        // Already in the library, with an annotation the library has not seen.
        assert!(matches!(
            by_title("Station Eleven").book.state,
            DeviceState::Updated {
                new_highlights: 1,
                ..
            }
        ));
        // Never seen.
        assert!(matches!(
            by_title("Piranesi").book.state,
            DeviceState::New { .. }
        ));
        // Not Lua. Falls back to the `.sdr` name, which is the only thing an
        // unreadable sidecar can be identified by at all.
        assert!(matches!(
            by_title("Broken").book.state,
            DeviceState::Unreadable(_)
        ));
    }

    /// Marking, then syncing the marks; and the drain is **one book per loop
    /// iteration**, which is what gives a forty-book sync visible progress.
    #[tokio::test]
    async fn a_sync_drains_one_book_per_iteration() {
        let mut app = test_app().await;
        app.screen = Screen::Device;

        // Mark both syncable rows (the unreadable one is left alone).
        let syncable: Vec<usize> = app
            .device
            .iter()
            .enumerate()
            .filter(|(_, r)| r.book.state.is_syncable())
            .map(|(i, _)| i)
            .collect();
        for i in syncable {
            app.device_state.select(Some(i));
            app.toggle_mark();
        }
        assert_eq!(app.device_marks.len(), 2);

        app.handle(Action::Sync).await.expect("sync");
        assert_eq!(
            app.pending_pull.as_ref().map(|q| q.len()),
            Some(2),
            "the whole selection is queued, not run"
        );
        assert!(app.has_deferred(), "the loop must not block on select!");

        // One unit of work per pump, with the queue shrinking each time — the
        // loop redraws between these two calls.
        assert!(app.pump_deferred().await.expect("pump"));
        assert_eq!(
            app.pending_pull.as_ref().map(|q| q.len()),
            Some(1),
            "a sync-all ran more than one book before redrawing"
        );
        assert!(app.pump_deferred().await.expect("pump"));
        assert!(
            app.pending_pull.is_none(),
            "the queue drops when it empties"
        );
        assert!(!app.pump_deferred().await.expect("pump"), "nothing left");

        // Both rows report what their pull did, and neither is still marked.
        for row in &app.device {
            if row.book.state == DeviceState::Unchanged && row.note.is_some() {
                assert!(row.note.as_deref().unwrap().contains("new"));
            }
        }
        assert!(app.device_marks.is_empty());
        // The new book landed in the library.
        assert_eq!(app.library.len(), 2);
    }

    /// With nothing marked, `s` takes every row there is something to do about
    /// — and never the unreadable one, which a sync would error on.
    #[tokio::test]
    async fn an_unmarked_sync_takes_every_syncable_row() {
        let mut app = test_app().await;
        app.screen = Screen::Device;
        app.handle(Action::Sync).await.expect("sync");
        let queued = app.pending_pull.as_ref().expect("queued");
        assert_eq!(queued.len(), 2);
        assert!(
            !queued
                .iter()
                .any(|p| p.to_string_lossy().contains("Broken")),
            "an unreadable sidecar was queued for a sync that would error on it"
        );
    }

    /// Enter pulls exactly the selected row, and refuses the unreadable one
    /// with the reason already on the row rather than an engine error.
    #[tokio::test]
    async fn enter_pulls_one_row_and_refuses_an_unreadable_one() {
        let mut app = test_app().await;
        app.screen = Screen::Device;
        let broken = app
            .device
            .iter()
            .position(|r| matches!(r.book.state, DeviceState::Unreadable(_)))
            .expect("a broken row");
        app.device_state.select(Some(broken));
        app.handle(Action::Select).await.expect("enter");
        assert!(
            app.pending_pull.is_none(),
            "queued a sidecar it cannot read"
        );
        assert!(app.status.as_deref().unwrap().contains("can't read"));

        let piranesi = app
            .device
            .iter()
            .position(|r| r.book.display_title() == "Piranesi")
            .expect("piranesi");
        app.device_state.select(Some(piranesi));
        app.handle(Action::Select).await.expect("enter");
        assert_eq!(app.pending_pull.as_ref().map(|q| q.len()), Some(1));

        app.pump_deferred().await.expect("pull");
        assert!(app.library.iter().any(|b| b.display_title() == "Piranesi"));
        assert!(
            app.device[piranesi].note.is_some(),
            "the row says what it did"
        );
    }

    /// `l` offers the engine's own candidate band, and linking records the
    /// choice *and* brings the book across — a link that appeared to do nothing
    /// is the failure this ordering exists to avoid.
    #[tokio::test]
    async fn linking_offers_candidates_and_then_imports() {
        let mut app = test_app().await;
        app.screen = Screen::Device;
        let piranesi = app
            .device
            .iter()
            .position(|r| r.book.display_title() == "Piranesi")
            .expect("piranesi");
        app.device_state.select(Some(piranesi));

        // The fixture's own band is empty (nothing in the library is close), so
        // `l` says so rather than opening an empty chooser.
        app.handle(Action::Link).await.expect("link");
        assert!(app.device_link.is_none());
        assert!(
            app.status
                .as_deref()
                .unwrap()
                .contains("nothing in the library")
        );

        // Given a candidate, `l` opens the chooser and Enter commits it.
        let book_id = app.library[0].id.expect("seeded book id");
        app.device[piranesi].book.state = DeviceState::New {
            candidates: vec![MatchCandidate {
                book_id,
                title: "Station Eleven".into(),
                score: 0.72,
            }],
        };
        app.handle(Action::Link).await.expect("link");
        assert!(app.device_link.is_some(), "the chooser opened");

        app.handle(Action::Select).await.expect("commit link");
        assert!(app.device_link.is_none());
        assert_eq!(app.device[piranesi].book.book_id, Some(book_id));
        assert_eq!(
            app.pending_pull.as_ref().map(|q| q.len()),
            Some(1),
            "linking queued no import, so it would look like a key that did nothing"
        );

        // The highlights land on the book that was chosen, not on a new one.
        app.pump_deferred().await.expect("import");
        assert_eq!(app.library.len(), 1, "linking created a second book");
    }

    /// Esc leaves the chooser without linking, and leaves the screen reachable.
    #[tokio::test]
    async fn the_candidate_chooser_can_be_left_alone() {
        let mut app = test_app().await;
        app.screen = Screen::Device;
        app.device_link = Some(LinkPicker {
            path: app.device[0].book.path.clone(),
            title: "Piranesi".into(),
            candidates: vec![MatchCandidate {
                book_id: 1,
                title: "Something".into(),
                score: 0.7,
            }],
            state: ListState::default(),
        });
        // While the decision is open the background stops drifting behind it,
        // exactly as it does under the other modals.
        app.ambient = crate::ambient::Ambient::new(crate::ambient::Motif::Motes);
        assert!(app.ambient_visible(), "the layer should still be drawn");
        assert!((0..40).all(|_| !app.tick()));

        app.handle(Action::Back).await.expect("esc");
        assert!(app.device_link.is_none());
        assert!(app.pending_pull.is_none());
        assert_eq!(app.screen, Screen::Device, "esc left the screen entirely");
    }

    /// A rescan is queued, not run in the handler, and it drops the marks —
    /// they are indices, and row 1 is a different book after a walk.
    #[tokio::test]
    async fn rescanning_defers_the_walk_and_drops_the_marks() {
        let mut app = test_app().await;
        app.screen = Screen::Device;
        app.device_marks.insert(1);

        app.handle(Action::Rescan).await.expect("rescan");
        assert!(
            app.pending_scan.is_some(),
            "the walk ran in the key handler"
        );
        assert!(app.status.as_deref().unwrap().contains("scanning"));

        app.pump_deferred().await.expect("scan");
        assert!(app.pending_scan.is_none());
        assert_eq!(app.device.len(), 3);
        assert!(app.device_marks.is_empty(), "stale marks survived a rescan");
    }

    /// A reader plugged in while the device screen is open refreshes it — and
    /// does so through `pending_scan`, so the walk still happens after the frame
    /// that announced it rather than inside the handler.
    #[tokio::test]
    async fn a_reader_arriving_refreshes_the_device_screen_it_is_looking_at() {
        let mut app = test_app().await;
        let root = app.device_root.clone().expect("a root");
        app.screen = Screen::Device;
        app.pending_scan = None;

        app.on_mount_event(MountEvent::Arrived(root.clone()));
        assert_eq!(app.pending_scan.as_deref(), Some(root.as_path()));
        assert!(app.dirty);
    }

    /// **The read-only claim, at the frontend end of it.** `docs/decisions.md`
    /// makes mount → import automatic and read-only and everything that writes
    /// explicit; the watcher may scan and may not sync. A cable is not consent
    /// to change the library.
    #[tokio::test]
    async fn a_reader_arriving_never_brings_anything_across_by_itself() {
        let mut app = test_app().await;
        let root = app.device_root.clone().expect("a root");
        let before = app.library.len();
        app.screen = Screen::Device;

        app.on_mount_event(MountEvent::Arrived(root));
        assert!(app.pending_pull.is_none(), "a mount queued a sync");
        // Drain everything it did queue, and the shelf is still the shelf.
        while app.pump_deferred().await.expect("pump") {}
        assert!(app.pending_pull.is_none(), "the scan went on to sync");
        assert_eq!(app.library.len(), before);
    }

    /// Somewhere else in the app it is a line, not a screen change. Being pulled
    /// off the book you are reading because a cable was plugged in is the other
    /// way to get this wrong, and it is the worse one.
    #[tokio::test]
    async fn a_reader_arriving_elsewhere_only_says_so() {
        let mut app = test_app().await;
        let root = app.device_root.clone().expect("a root");
        app.screen = Screen::Home;
        app.pending_scan = None;

        app.on_mount_event(MountEvent::Arrived(root.clone()));
        assert_eq!(app.screen, Screen::Home, "the screen was taken over");
        assert!(app.pending_scan.is_none(), "a scan ran unasked");
        let status = app.status.clone().expect("a status line");
        assert!(status.contains(&root.display().to_string()));
        // Never a dead end: the line names the key that gets there.
        assert!(
            status.contains('m'),
            "{status:?} names no way to the device"
        );
    }

    /// The path box asks which device to look at. Plugging one in answers it —
    /// closing the prompt there is not taking the decision away, it *is* the
    /// decision it was asking for.
    #[tokio::test]
    async fn an_arriving_reader_answers_the_path_prompt() {
        let mut app = test_app().await;
        let root = app.device_root.clone().expect("a root");
        app.screen = Screen::Device;
        app.device_root = None;
        app.pending_scan = None;
        app.start_input(InputContext::DevicePath, "device path", "");

        app.on_mount_event(MountEvent::Arrived(root.clone()));
        assert!(app.input.is_none(), "the prompt outlived its own answer");
        assert_eq!(app.pending_scan.as_deref(), Some(root.as_path()));
    }

    /// Two readers: the one on the screen keeps the screen. Replacing the list
    /// would lose the marks and the place, and the device being looked at is
    /// still plugged in.
    #[tokio::test]
    async fn a_second_reader_does_not_take_the_screen_from_the_first() {
        let mut app = test_app().await;
        app.screen = Screen::Device;
        app.pending_scan = None;
        app.device_marks.insert(1);

        app.on_mount_event(MountEvent::Arrived(PathBuf::from("/Volumes/OtherReader")));
        assert!(app.pending_scan.is_none(), "the other reader took over");
        assert_eq!(app.device.len(), 3, "the shown list was replaced");
        assert!(app.device_marks.contains(&1), "the marks were dropped");
        assert!(app.status.as_deref().unwrap().contains("OtherReader"));
    }

    /// Unplugging empties the shelf it was showing: those rows name files that
    /// are not there, and enter on one would fail per row. The root stays, so
    /// `r` after plugging it back in still means something.
    #[tokio::test]
    async fn unplugging_the_shown_reader_empties_the_shelf_but_keeps_the_root() {
        let mut app = test_app().await;
        let root = app.device_root.clone().expect("a root");
        app.screen = Screen::Device;
        app.device_marks.insert(0);

        app.on_mount_event(MountEvent::Departed(root.clone()));
        assert!(app.device.is_empty());
        assert!(app.device_marks.is_empty());
        assert_eq!(app.device_state.selected(), None);
        assert_eq!(app.device_root.as_deref(), Some(root.as_path()));
        assert!(app.status.as_deref().unwrap().contains("unplugged"));
    }

    /// Another device leaving does not clear the one on the screen.
    #[tokio::test]
    async fn unplugging_a_different_reader_leaves_the_shelf_alone() {
        let mut app = test_app().await;
        app.screen = Screen::Device;

        app.on_mount_event(MountEvent::Departed(PathBuf::from("/Volumes/OtherReader")));
        assert_eq!(app.device.len(), 3);
    }

    /// `x` toggles, and the marks address the selected row.
    #[tokio::test]
    async fn marking_toggles_the_selected_row() {
        let mut app = test_app().await;
        app.screen = Screen::Device;
        app.device_state.select(Some(2));
        app.handle(Action::Mark).await.expect("mark");
        assert_eq!(
            app.device_marks.iter().copied().collect::<Vec<_>>(),
            vec![2]
        );
        app.handle(Action::Mark).await.expect("unmark");
        assert!(app.device_marks.is_empty());
    }

    /// The screen is never a dead end: `m` returns to the menu from it, and the
    /// up/down keys wrap.
    #[tokio::test]
    async fn the_device_screen_navigates_and_returns_to_the_menu() {
        let mut app = test_app().await;
        app.screen = Screen::Device;
        app.handle(Action::Up).await.expect("up");
        assert_eq!(app.device_state.selected(), Some(2), "wraps backwards");
        app.handle(Action::Down).await.expect("down");
        assert_eq!(app.device_state.selected(), Some(0));
        app.handle(Action::Menu).await.expect("menu");
        assert_eq!(app.screen, Screen::Menu);
    }

    /// The menu row exists and reaches the screen. With no reader plugged in it
    /// still opens, with the path box up — a menu entry that goes nowhere is
    /// the dead end the design rules out.
    #[tokio::test]
    async fn the_menu_opens_the_device_screen() {
        let mut app = test_app().await;
        app.screen = Screen::Menu;
        app.menu_index = menu_row(MenuItem::Device);
        app.handle(Action::Select).await.expect("open device");
        assert_eq!(app.screen, Screen::Device);
        // Either a mount was found and queued, or the box is asking for a path.
        assert!(app.pending_scan.is_some() || app.input.is_some());
    }

    /// A typed path scans it: this is the same screen for a library directory
    /// as for a mounted reader.
    #[tokio::test]
    async fn a_typed_path_is_scanned() {
        let mut app = test_app().await;
        let root = app.device_root.clone().expect("a root");
        app.device.clear();
        app.screen = Screen::Menu;

        app.start_input(InputContext::DevicePath, "device path", "");
        for c in root.to_string_lossy().chars() {
            app.on_input_key(KeyEvent::from(KeyCode::Char(c)))
                .await
                .expect("type");
        }
        app.on_input_key(KeyEvent::from(KeyCode::Enter))
            .await
            .expect("commit");
        assert_eq!(app.screen, Screen::Device);
        app.pump_deferred().await.expect("scan");
        assert_eq!(app.device.len(), 3);
    }

    #[tokio::test]
    async fn quitting_and_navigation_move_between_screens() {
        let mut app = test_app().await;
        // The app opens onto what you are reading, not onto a list of commands.
        assert_eq!(app.screen, Screen::Home);
        app.handle(Action::Menu).await.expect("menu");
        app.menu_index = menu_row(MenuItem::Library);
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
        // A couple more open readings, so the home shelf shows what a shelf
        // looks like rather than a box with one row in it. The second has the
        // device's percentage and no page, which is the commonest row there.
        for (book, percent) in [("Piranesi", Some(0.42)), ("The Overstory", None)] {
            app.reading.push((
                Book {
                    title: Some(book.into()),
                    authors: vec!["Someone".into()],
                    page_count: Some(300),
                    ..Book::default()
                },
                sample_reading(percent),
            ));
        }
        app.reading_state.select(Some(0));
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

        // A marked row, so the gutter shows in the dump too.
        app.device_marks.insert(1);

        let (w, h) = (96u16, 16u16);
        // The device screen twice: the shelf, then the candidate chooser over
        // it. `linking` is what opens the second one.
        // Home twice as well: the shelf, and the empty state, which is a
        // different branch of the same draw and the one worth looking at.
        for (screen, linking, empty) in [
            (Screen::Home, false, false),
            (Screen::Home, false, true),
            (Screen::Library, false, false),
            (Screen::Search, false, false),
            (Screen::Device, false, false),
            (Screen::Device, true, false),
        ] {
            app.screen = screen;
            let held = empty.then(|| std::mem::take(&mut app.reading));
            if linking {
                let mut state = ListState::default();
                state.select(Some(1));
                app.device_link = Some(LinkPicker {
                    path: app.device[1].book.path.clone(),
                    title: app.device[1].book.display_title(),
                    candidates: vec![
                        MatchCandidate {
                            book_id: 1,
                            title: "Piranesi's House".into(),
                            score: 0.78,
                        },
                        MatchCandidate {
                            book_id: 2,
                            title: "Piranesi (illustrated)".into(),
                            score: 0.64,
                        },
                    ],
                    state,
                });
            }
            let mut t = ratatui::Terminal::new(TestBackend::new(w, h)).unwrap();
            t.draw(|f| ui::draw(f, &mut app)).unwrap();
            println!(
                "=== {screen:?}{} {w}x{h} ===",
                if empty { " (empty)" } else { "" }
            );
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
            if let Some(rows) = held {
                app.reading = rows;
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
        // Open the reflection so there is one to look at: it is a row in the
        // Notes list, not a section of its own, so the only way to judge that
        // choice is to see it beside an ordinary note.
        rt.block_on(app.handle(Action::Reflect)).unwrap();
        app.note_editor = None;
        // Wide cases too: the object is centred on the window (with the panel
        // beside it) once the width affords the panel its floor, and that is the
        // only place to see it. The `notes` case opens the section; the last is
        // tab — the pane dismissed, the one layout where the object has the
        // whole window.
        for (w, h, panel, notes) in [
            (110, 32, true, false),
            (44, 26, true, false),
            (86, 30, true, false),
            (110, 32, true, true),
            (180, 44, true, false),
            (180, 44, false, false),
        ] {
            app.layout.panel = panel;
            app.book_tab = if notes { BookTab::Notes } else { BookTab::Info };
            app.in_section = notes;
            app.clamp_tab_selection();
            print_frame(
                &mut app,
                w,
                h,
                &format!(
                    "{w}x{h}{}{}",
                    if notes { " · notes" } else { "" },
                    if panel { "" } else { " · no panel" }
                ),
            );
        }

        // The links pane, in the Notes section's place, over a graph read out of
        // the real engine rather than hand-built — a dump is only worth looking
        // at if it is showing what the app would show. Two widths, because the
        // panel is where a row runs out of room first.
        rt.block_on(open_graph_pane(&mut app));
        app.layout.panel = true;
        for (w, h) in [(110, 32), (44, 26)] {
            print_frame(&mut app, w, h, &format!("links pane {w}x{h}"));
        }
    }

    /// One composed frame as ASCII, with the book's two half-block glyphs
    /// flattened to `#` so the object reads as a silhouette.
    fn print_frame(app: &mut App, w: u16, h: u16, label: &str) {
        let mut t = ratatui::Terminal::new(TestBackend::new(w, h)).unwrap();
        t.draw(|f| ui::draw(f, app)).unwrap();
        println!("=== {label} ===");
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
