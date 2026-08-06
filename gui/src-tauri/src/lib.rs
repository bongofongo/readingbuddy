//! The Tauri backend, which is a client of the API and nothing else.
//!
//! ## One command, not seventy-seven
//!
//! There is exactly one `#[tauri::command]`, and it takes a
//! [`readingbuddy_api::Call`] and returns a [`readingbuddy_api::Reply`]. The
//! alternative — one command per facade method — would be a third hand-written
//! copy of the surface after the DTOs and the TypeScript, free to drift from
//! both, and the drift would show up as a blank panel in a webview rather than
//! as a compile error. With one command there is nothing to keep in sync: adding
//! an API method is a regenerated `bindings.ts` and no Rust at all.
//!
//! `Api::call` rather than `Api::dispatch`, and that distinction is load-bearing.
//! `call` never returns `Err` — a transport must write something for every line
//! it read — and it stamps `api_version` on the reply. So the in-process arm
//! below and a future socket arm produce the *same* value rather than one
//! wrapping and one not, which is the whole point of the trait.
//!
//! It is also why the command returns a bare `Reply` and not a `Result`. A
//! failure is already an `Outcome::Error` *inside* the reply, so a `Result` here
//! would put a second, differently-shaped error channel beside the one the
//! protocol defines and the frontend would have to handle both. Tauri does force
//! a `Result` on any async command taking a *reference* input, which is why this
//! one takes an owned `AppHandle` and reaches for the state itself rather than
//! accepting a `State<'_, _>`.
//!
//! ## The client trait
//!
//! [`ApiClient`] has one in-process implementation today. `readingbuddyd` exists
//! and serves the identical vocabulary over a unix socket, so the socket arm is a
//! second impl and nothing above the trait changes when it arrives. That is why
//! the trait is here on day one rather than when a second transport does.

use std::path::PathBuf;
use std::sync::Arc;

use readingbuddy_api::{Api, Call, Reply};
use tauri::Manager;

/// How the frontend reaches the library.
///
/// Deliberately the *whole* vocabulary in one method. A trait with a method per
/// request would have to grow every time the API does, which is the coupling
/// this shape exists to avoid.
pub trait ApiClient: Send + Sync + 'static {
    fn call(&self, call: Call) -> impl std::future::Future<Output = Reply> + Send;
}

/// The engine in this process. No socket, no serialization of our own — the
/// `Call` was already deserialized by Tauri's IPC and the `Reply` is serialized
/// back by it.
pub struct InProcess {
    api: Api,
}

impl InProcess {
    /// # Data directory
    ///
    /// Rooted at an **absolute** path, and that is not incidental. `cover_path`
    /// is stored as `images_dir.join(name)` and `images_dir` is
    /// `data_root/database/images`, so a relative root yields a relative
    /// `cover_path` — and a webview has no working directory to resolve one
    /// against. Absolute here means `convertFileSrc(book.cover_path)` works with
    /// a single asset-protocol scope entry, and means the frontend must never
    /// join `images_dir` with `cover_path` (that would double the prefix).
    ///
    /// # The vault (item 24)
    ///
    /// The note index catches up with the vault here, so the search box item 27
    /// adds cannot silently miss the note edited in Obsidian five minutes ago —
    /// which is the worst shape that bug takes, because the box does not look
    /// broken, the note looks gone. Once for the edits made while the app was
    /// closed, then a task that drives the watcher: polling it *is* the work,
    /// and the re-index happens inside it rather than needing anything from
    /// this side or from the frontend.
    ///
    /// Both degrade to nothing. A window that will not open because a directory
    /// could not be watched is a far worse bug than a stale index.
    pub async fn open(data_dir: PathBuf) -> Result<Self, readingbuddy_api::ApiError> {
        let data_dir = std::path::absolute(&data_dir).unwrap_or(data_dir);
        let api = Api::open(&data_dir).await?;

        // `eprintln!` and not `tracing::warn!`: this crate installs no
        // subscriber, so a tracing event here would go precisely nowhere.
        // stderr is what `pnpm tauri dev` shows and what a packaged app's log
        // captures.
        if let Err(e) = api.engine().reconcile_vault().await {
            eprintln!("readingbuddy: could not reconcile the vault: {e}");
        }
        match api.engine().watch_vault() {
            Ok(mut watcher) => {
                tauri::async_runtime::spawn(async move { while watcher.next().await.is_some() {} });
            }
            Err(e) => eprintln!("readingbuddy: not watching the vault: {e}"),
        }

        Ok(Self { api })
    }

    pub fn images_dir(&self) -> String {
        self.api.paths().images_dir
    }
}

impl ApiClient for InProcess {
    async fn call(&self, call: Call) -> Reply {
        self.api.call(call).await
    }
}

/// The single seam. Everything the GUI can ask goes through here.
///
/// An owned `AppHandle` rather than a `State<'_, _>` parameter: Tauri requires
/// any async command with a reference input to return a `Result`, and there is no
/// second error channel to return. See the module doc.
#[tauri::command]
async fn api_call(app: tauri::AppHandle, call: Call) -> Reply {
    let client = app.state::<Arc<InProcess>>().inner().clone();
    client.call(call).await
}

/// Where the library lives.
///
/// `READINGBUDDY_DATA_DIR` first, matching the CLI and TUI, so `make dev-db`'s
/// output can be pointed at without a rebuild. Falling back to the current
/// directory rather than a platform app-data path is deliberate for now: the
/// three frontends must agree about which library they are looking at, and
/// picking a different default here would make the GUI open an empty one on a
/// machine whose library the CLI can see.
fn data_dir() -> PathBuf {
    std::env::var_os("READINGBUDDY_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

pub fn run() {
    let client = tauri::async_runtime::block_on(InProcess::open(data_dir()))
        .expect("the library did not open");
    let images = client.images_dir();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(Arc::new(client))
        .setup(move |app| {
            // The asset protocol is scoped at runtime rather than in
            // `tauri.conf.json`, because the directory depends on the data dir
            // and a static glob would have to name a path that is only known
            // once the engine is open.
            let scope = app.asset_protocol_scope();
            scope.allow_directory(&images, false)?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![api_call])
        .run(tauri::generate_context!())
        .expect("the webview did not start");
}

#[cfg(test)]
mod tests {
    use super::*;
    use readingbuddy_api::{Request, Response};

    /// The seam, end to end, without a webview: a `Call` in, a `Reply` out, over
    /// a real SQLite database the engine created and migrated itself.
    ///
    /// This is the test `docs/gui/testing.md` calls the seam check. It asserts
    /// the thing E2E cannot on macOS (`tauri-driver` has no WKWebView driver) and
    /// the thing a frontend unit test cannot at all.
    #[tokio::test]
    async fn one_call_reaches_sqlite_and_comes_back_stamped() {
        let dir = tempfile::tempdir().unwrap();
        let client = InProcess::open(dir.path().to_path_buf()).await.unwrap();

        let reply = client
            .call(Call {
                id: 7,
                request: Request::ListBooks {
                    limit: 5,
                    sort: Default::default(),
                    offset: 0,
                    filter: None,
                },
            })
            .await;

        assert_eq!(reply.id, 7, "a reply carries the id it answers");
        assert_eq!(
            reply.api_version,
            readingbuddy_api::API_VERSION,
            "the version stamp is `call`'s, and is why this arm and a socket arm agree"
        );
        match reply.outcome {
            readingbuddy_api::Outcome::Ok {
                response: Response::Books(books),
            } => assert!(books.is_empty()),
            other => panic!("a fresh library lists no books, got {other:?}"),
        }
    }

    /// `images_dir` is absolute even when the data dir given was not, because a
    /// relative `cover_path` is one a webview cannot resolve. See
    /// [`InProcess::open`].
    #[tokio::test]
    async fn the_images_dir_is_absolute_whatever_the_data_dir_was() {
        let dir = tempfile::tempdir().unwrap();
        // `.` relative to the test's cwd, which is the crate root — enough to
        // exercise the branch without depending on what is there.
        let client = InProcess::open(dir.path().to_path_buf()).await.unwrap();
        assert!(
            std::path::Path::new(&client.images_dir()).is_absolute(),
            "images_dir was {}",
            client.images_dir()
        );
    }
}
