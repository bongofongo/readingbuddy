# readingbuddy

A reading companion that lives in a terminal pane: your library, your notes, and
a ray-traced 3D book rendered in Unicode block glyphs.

It reads what you already own rather than asking you to re-enter it — KOReader
highlights and reading state off a plugged-in device, calibre's library, a
Goodreads export, epubs — and keeps notes as plain Markdown in a folder you can
open in Obsidian.

## Install

```sh
curl -fsSL https://raw.githubusercontent.com/bongofongo/readingbuddy/main/install.sh | sh
```

Prebuilt for x86_64 and arm64 on both Linux (glibc 2.35+) and macOS (11.0+).
The script downloads the release archive for your machine, checks it against the
release's `SHA256SUMS`, and puts two binaries in `~/.local/bin`:

| | |
|---|---|
| `readingbuddy-tui` | the reading room — run it in a tmux pane |
| `readingbuddy` | the command line — `readingbuddy --help` |

It edits no shell profile and installs nothing system-wide. Knobs, all
environment variables:

```sh
RB_VERSION=v0.1.0          # pin a version (default: the latest release)
RB_INSTALL_DIR=~/bin       # where the binaries go (default: ~/.local/bin)
RB_NO_VERIFY=1             # skip the checksum (default: verify, and refuse on mismatch)
```

Prefer to read it first? It is one file: [`install.sh`](install.sh).

### Then set where your library lives

```sh
export READINGBUDDY_DATA_DIR="$HOME/.local/share/readingbuddy"
```

Put that in your shell profile. The data root otherwise defaults to the
**current directory**, so without it every directory you launch from quietly
becomes a separate library — and the first symptom is an empty shelf on a
machine that has one.

### From source

```sh
git clone https://github.com/bongofongo/readingbuddy
cd readingbuddy && cargo build --release --workspace
```

Needs a C compiler: Lua and SQLite are vendored and built from source, so there
are no system libraries to install.

## Where to start

[`TUTORIAL.md`](TUTORIAL.md) walks through every feature in the order you would
naturally meet them. In the TUI, `m` opens the menu from anywhere and the key
bar at the bottom of a screen is always the answer to "what can I press".

## Licence

GPL-3.0 — see [`LICENSE`](LICENSE). readingbuddy links the `epub` crate, which
is GPL-3.0, so a distributed binary is GPL-3.0 as a whole.
