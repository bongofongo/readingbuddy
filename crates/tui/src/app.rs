//! Application state and the async event loop.

use std::time::Duration;

use anyhow::Result;
use crossterm::event::{Event, EventStream};
use futures::StreamExt;
use ratatui::Terminal;
use ratatui::backend::Backend;
use ratatui::widgets::ListState;
use readingbuddy::{Book, BookSort, Engine};

use crate::event::Action;
use crate::render3d::{Pose, RenderParams, Scene};
use crate::ui;

/// Idle spin: a full turn about the bottom of the spine, so the book sweeps
/// round like a slow top. Radians per tick at 20fps — about 63s for 360°.
const SPIN_SPEED: f32 = 0.005;
/// The pitch nods gently while the book turns, on its own slower cycle.
const NOD: f32 = 0.06;
const NOD_SPEED: f32 = 0.011;
/// Radians per keypress when rotating by hand.
const NUDGE: f32 = 0.10;
/// Pitch is clamped: past this the book turns into a horizon line.
const MAX_PITCH: f32 = 1.10;
const TICK: Duration = Duration::from_millis(50);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Menu,
    Library,
    Book,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuItem {
    Library,
    Continue,
    Cards,
    Quit,
}

pub const MENU: [(MenuItem, &str, &str); 4] = [
    (MenuItem::Library, "Library", "browse everything you've saved"),
    (MenuItem::Continue, "Continue reading", "jump to the most recent book"),
    (MenuItem::Cards, "Flashcards", "count the cards waiting for export"),
    (MenuItem::Quit, "Quit", ""),
];

/// A book plus the counts shown alongside it.
pub struct BookView {
    pub book: Book,
    pub highlights: usize,
    pub notes: usize,
}

pub struct App {
    pub engine: Engine,
    pub screen: Screen,
    pub menu_index: usize,
    pub library: Vec<Book>,
    pub library_state: ListState,
    pub view: Option<BookView>,
    pub scene: Scene,
    pub params: RenderParams,
    /// Idle spin on/off.
    pub spinning: bool,
    pub show_options: bool,
    pub status: Option<String>,
    pub dirty: bool,
    pub quit: bool,
    /// Pitch the nod oscillates around; the yaw just keeps turning.
    base_pitch: f32,
    phase: f32,
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
            scene,
            params: RenderParams::default(),
            spinning: true,
            show_options: false,
            status: None,
            dirty: true,
            quit: false,
            base_pitch: Pose::default().pitch,
            phase: 0.0,
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
        self.dirty = true;
        Ok(())
    }

    /// Load a book into the viewing mode, fetching its highlight/note counts.
    pub async fn open_book(&mut self, book: Book) -> Result<()> {
        let (highlights, notes) = match book.id {
            Some(id) => (
                self.engine.storage.list_highlights(id).await?.len(),
                self.engine.list_notes(Some(id)).await?.len(),
            ),
            None => (0, 0),
        };
        self.view = Some(BookView {
            book,
            highlights,
            notes,
        });
        self.screen = Screen::Book;
        self.status = None;
        self.dirty = true;
        Ok(())
    }

    fn reset_pose(&mut self) {
        self.params.pose = Pose::default();
        self.base_pitch = self.params.pose.pitch;
        self.phase = 0.0;
    }

    /// Advance the idle animation. Returns true when something moved.
    ///
    /// The spin picks up from wherever the book currently sits, so switching it
    /// back on after a manual nudge never snaps the pose.
    fn tick(&mut self) -> bool {
        if !(self.spinning && self.screen == Screen::Book) {
            return false;
        }
        self.phase += NOD_SPEED;
        self.params.pose.yaw = wrap_angle(self.params.pose.yaw + SPIN_SPEED);
        self.params.pose.pitch = self.base_pitch + self.phase.sin() * NOD;
        true
    }

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

            (Screen::Book, Action::Back) => self.screen = Screen::Library,
            (Screen::Book, Action::Left) => self.rotate(-NUDGE),
            (Screen::Book, Action::Right) => self.rotate(NUDGE),
            (Screen::Book, Action::Up) => self.tilt(NUDGE),
            (Screen::Book, Action::Down) => self.tilt(-NUDGE),
            (Screen::Book, Action::Select | Action::ToggleSpin) => {
                self.spinning = !self.spinning;
            }
            (Screen::Book, Action::Reset) => {
                self.reset_pose();
                self.spinning = true;
            }
            (Screen::Book, Action::ToggleOptions) => self.show_options = !self.show_options,

            _ => self.dirty = false,
        }
        Ok(())
    }

    async fn activate_menu(&mut self) -> Result<()> {
        match MENU[self.menu_index].0 {
            MenuItem::Library => {
                self.refresh_library().await?;
                if self.library.is_empty() {
                    self.status =
                        Some("library is empty — add a book with `readingbuddy search`".into());
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
            MenuItem::Cards => {
                let n = self.engine.list_flashcards(false).await?.len();
                self.status = Some(format!(
                    "{n} flashcard candidate(s) pending — export with `readingbuddy cards export`"
                ));
            }
            MenuItem::Quit => self.quit = true,
        }
        Ok(())
    }

    fn step_library(&mut self, delta: isize) {
        if self.library.is_empty() {
            return;
        }
        let len = self.library.len() as isize;
        let cur = self.library_state.selected().unwrap_or(0) as isize;
        self.library_state
            .select(Some(((cur + delta).rem_euclid(len)) as usize));
    }

    /// Manual rotation takes the book off the idle spin.
    fn rotate(&mut self, delta: f32) {
        self.spinning = false;
        self.params.pose.yaw = wrap_angle(self.params.pose.yaw + delta);
    }

    fn tilt(&mut self, delta: f32) {
        self.spinning = false;
        self.base_pitch = (self.base_pitch + delta).clamp(-MAX_PITCH, MAX_PITCH);
        self.params.pose.pitch = self.base_pitch;
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
    app.dirty = false;

    loop {
        tokio::select! {
            maybe_event = events.next() => {
                match maybe_event {
                    Some(Ok(Event::Key(key))) => {
                        if let Some(action) = crate::event::map_key(key) {
                            app.handle(action).await?;
                        }
                    }
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
            app.dirty = false;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use readingbuddy::EngineConfig;

    /// An engine on an in-memory database — the same trick the engine's own
    /// tests use, so nothing here touches the user's library.
    async fn test_app() -> App {
        let tmp = std::env::temp_dir().join("readingbuddy-tui-tests");
        let config = EngineConfig {
            db_url: "sqlite::memory:".into(),
            images_dir: tmp.join("images"),
            vault_dir: tmp.join("vault"),
            google_api_key: None,
        };
        let engine = Engine::open(config).await.expect("engine");
        engine
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
        App::new(engine).await.expect("app")
    }

    /// Draw every screen at sizes from a full terminal down to a pane too small
    /// to hold anything. Layout arithmetic is the likeliest place to panic, and
    /// a panic here would wreck the user's tmux pane.
    #[tokio::test]
    async fn every_screen_draws_at_every_size() {
        let mut app = test_app().await;
        let book = app.library.first().cloned().expect("seeded book");
        app.open_book(book).await.expect("open");

        for (w, h) in [(120, 40), (80, 24), (40, 30), (30, 30), (20, 8), (4, 2), (1, 1)] {
            let mut terminal =
                ratatui::Terminal::new(TestBackend::new(w, h)).expect("terminal");
            for screen in [Screen::Menu, Screen::Library, Screen::Book] {
                app.screen = screen;
                for options in [false, true] {
                    app.show_options = options;
                    terminal.draw(|f| ui::draw(f, &mut app)).expect("draw");
                }
            }
        }
    }

    #[tokio::test]
    async fn spinning_resumes_from_wherever_the_book_was_left() {
        let mut app = test_app().await;
        let book = app.library.first().cloned().expect("seeded book");
        app.open_book(book).await.expect("open");

        app.handle(Action::Left).await.expect("rotate");
        assert!(!app.spinning, "a manual nudge stops the spin");
        let held = app.params.pose.yaw;

        app.handle(Action::ToggleSpin).await.expect("resume");
        assert!(app.spinning);
        app.tick();
        assert!(
            (app.params.pose.yaw - held).abs() < 0.05,
            "jumped from {held} to {}",
            app.params.pose.yaw
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
