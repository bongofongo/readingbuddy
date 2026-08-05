//! `readingbuddyd` — the transport wrapper.
//!
//! `docs/decisions.md` puts the boundary at the API rather than at the process,
//! so this binary is deliberately the least interesting crate in the workspace:
//! it opens an engine, binds a socket, and hands every line it reads to
//! [`readingbuddy_api::Api::call`]. **If a feature is ever added here, it is in
//! the wrong crate** — iOS links the API in-process and would not get it.
//!
//! ```text
//! readingbuddyd --data-dir ~/reading
//! printf '{"id":1,"request":{"method":"list_books","params":{"limit":5}}}\n' \
//!   | nc -U ~/reading/readingbuddyd.sock
//! ```
//!
//! Unix only, and that is the scope rather than a gap: the daemon exists for a
//! Tauri app and a menu-bar companion on the user's own machine, and the
//! platforms `device.rs` knows how to find a reader on are macOS and Linux.

#[cfg(not(unix))]
compile_error!("readingbuddyd is a unix-socket daemon; there is no Windows transport yet");

mod server;

use std::path::PathBuf;
use std::sync::Arc;

use clap::Parser;
use readingbuddy::{Engine, EngineConfig};
use readingbuddy_api::Api;

#[derive(Parser, Debug)]
#[command(
    name = "readingbuddyd",
    about = "readingbuddy's local API daemon (unix socket, newline-delimited JSON)"
)]
struct Cli {
    /// Data root: database/, vault/, logs/. Same meaning as the CLI's.
    #[arg(long, env = "READINGBUDDY_DATA_DIR", default_value = ".")]
    data_dir: PathBuf,

    /// Where to listen. Defaults to `<data-dir>/readingbuddyd.sock`, so a
    /// sandbox run with its own `--data-dir` gets its own socket for free and
    /// cannot collide with the real one.
    #[arg(long)]
    socket: Option<PathBuf>,

    /// Google Books API key. The engine takes it from `GOOGLE_BOOKS_API_KEY`
    /// too; the daemon deliberately does **not** read the CLI's config file,
    /// which is the CLI's to own.
    #[arg(long, env = "GOOGLE_BOOKS_API_KEY")]
    google_api_key: Option<String>,

    /// Log filter, e.g. `readingbuddyd=debug,readingbuddy=info`.
    #[arg(long, env = "RUST_LOG", default_value = "readingbuddyd=info")]
    log: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // The engine emits and never subscribes; a frontend — and this is one —
    // decides where the events go. stderr, so `readingbuddyd > /dev/null` still
    // shows what went wrong and a supervisor can capture it.
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::new(&cli.log))
        .with_writer(std::io::stderr)
        .init();

    let mut config = EngineConfig::rooted_at(&cli.data_dir);
    if cli.google_api_key.is_some() {
        config.google_api_key = cli.google_api_key.clone();
    }
    let socket = cli
        .socket
        .clone()
        .unwrap_or_else(|| cli.data_dir.join("readingbuddyd.sock"));

    let engine = Engine::open(config).await?;
    let engine = Arc::new(engine);

    // **Item 24's ruling, cashed out.** Because the vault watcher does the
    // re-index itself, keeping a socket client's note searches correct costs
    // this binary one task and **no wire-protocol change at all**. Had the
    // write stayed on the frontend's side, this is where the protocol would
    // have had to grow server-initiated frames — every reply here carries the
    // id it answers, and a push has no id to carry.
    //
    // Once before listening, for the edits made while nothing was running; then
    // a task that owns the watcher and drives it, since polling it *is* the
    // work. Both degrade to nothing: a daemon that cannot watch a vault is
    // still a daemon, and refusing to start over one would be the opposite of
    // what `docs/decisions.md` makes absence mean.
    if let Err(e) = engine.reconcile_vault().await {
        tracing::warn!(error = %e, "could not reconcile the vault");
    }
    match engine.watch_vault() {
        Ok(mut watcher) => {
            tokio::spawn(async move { while watcher.next().await.is_some() {} });
        }
        Err(e) => tracing::warn!(error = %e, "not watching the vault"),
    }

    let api = Api::new(engine);

    let listener = server::bind(&socket).await?;
    // After `bind`, so a refusal to start does not delete the socket of the
    // daemon that was already there.
    let _guard = server::SocketGuard(socket.clone());
    tracing::info!(socket = %socket.display(), api = readingbuddy_api::API_VERSION, "listening");

    server::serve(api, listener, shutdown()).await
}

/// Ctrl-C or SIGTERM. Both, because a supervisor sends the second and a
/// terminal sends the first, and a daemon that only handles one of them leaves
/// its socket behind half the time.
async fn shutdown() {
    let ctrl_c = async {
        tokio::signal::ctrl_c().await.ok();
    };
    let term = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut s) => {
                s.recv().await;
            }
            // No SIGTERM handler is not a reason to refuse to run; ctrl-C still
            // works and the guard still removes the socket.
            Err(e) => {
                tracing::warn!(error = %e, "cannot listen for SIGTERM");
                std::future::pending::<()>().await;
            }
        }
    };
    tokio::select! {
        () = ctrl_c => {}
        () = term => {}
    }
}
