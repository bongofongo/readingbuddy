# Prompt — Item 15a: the KOReader plugin, installed and paired

Paste into a fresh Claude Code thread at the repo root, in its own worktree
(`feat/koreader-plugin-install`).

---

Read `docs/decisions.md` (the **KOReader plugin** and **Device linking**
sections) and `docs/spec-11-16.md` (item 15) before starting. `CLAUDE.md`'s
**Engine standards** section is binding.

Owns migration **`0019`**. Nothing else is outstanding; `0018` is the highest
applied. Runs alone — no other item is in flight.

**Check your base before writing a line**: `git log --oneline -1` and
`ls crates/engine/migrations/ | tail -2`. If migrations do not stop at `0018`,
`git reset --hard main`.

## The item, and the half of it that is not this item

`docs/decisions.md` describes item 15 as the plugin *and* wireless push. This
thread builds **only the link**: readingbuddy installs its plugin onto a mounted
reader, pairs with it, and can take it off again cleanly. The device ends up
holding a plugin that knows who we are and has nothing yet to say to us.

**Explicitly not in scope**, and an eager thread will want all of them:

- no TCP listener, no HTTP, no protocol, no wire format;
- no annotation push, no two-way sync, no reading of anything the plugin writes;
- no GUI screen and no TUI screen — CLI only, so the surface is provable before
  a frontend is shaped around it;
- **no auto-install on mount.** `decisions.md` is explicit: mount → import stays
  automatic and read-only; mount → install is an explicit action that shows the
  destination path first.

## Why a new module

`crates/engine/src/plugin.rs`. **Not `device.rs`** — and the reason is the same
one that split `device.rs` out of `koreader.rs` in the first place, recorded in
`crates/engine/CLAUDE.md`: *a scan is not an import*. An install is not a scan.
`device.rs` is read-only by construction and its whole test suite rests on that;
this module is the only code in the engine that writes to somebody else's
hardware, and it should be the only file you have to read to audit that claim.

It consumes `device::{is_koreader_mount, koreader_dir, offers_reader}` — already
public, already tested — and the sha256 helper from `files.rs`.

## What the KOReader source settles

These were read from a checked-out `koreader/master`, which
`docs/koreader-format.md` §1 makes the order of authority: the source is the
spec, a fixture is only ever evidence. Do not re-derive them; do correct them if
you find them wrong.

- **`pluginloader.lua:178-200`** — plugins load from `DEFAULT_PLUGIN_PATH`
  (`plugins/`) plus `extra_plugin_paths`. On a device
  `datastorage.lua:getDataDir()` returns `"."` — cwd is the install directory —
  and the loader only registers the extra path `if data_dir ~= "."`. So on a
  device `{data_dir}/plugins/` *is* `koreader/plugins/`, and
  **we never write `extra_plugin_paths`**. Setting it would be editing the
  user's `settings.reader.lua`, which the standing rule forbids, and it would
  buy nothing.
- **A directory is a plugin if it matches `".+%.koplugin"`** and contains
  `main.lua`. `_meta.lua` is optional and its fields are merged into the module
  **except `name`**. There is no version or compatibility gating in the loader.
- **Our plugin survives a KOReader OTA update.** The update is a plain tar
  unpack with no `--delete`; the only removal is a manifest diff,
  `grep -xvFf .../ota/package.index /tmp/package.index | xargs -r rm -vf`,
  identical in the Kobo, Kindle, Cervantes and remarkable startup scripts. It
  deletes files that were in the *old* KOReader manifest and are absent from the
  *new* one. A path that never appeared in any KOReader manifest is untouched.
  Record this in the module header — it is the fact that makes the install
  target durable, and without it written down somebody re-opens it.
- **`kosync.koplugin` stores its credentials at `settings/kosync.lua`**, outside
  its own directory. That convention is the one we break: it would leave a token
  behind after an uninstall we have promised is exact.

## The plugin itself

`crates/engine/koplugin/readingbuddy.koplugin/`, two files, embedded into the
binary with `include_str!` — **not `include_dir`**. A new dependency buys
nothing here, and an explicit `const FILES: &[(&str, &str)]` makes the shipped
file list a compile-time fact, which is exactly what an exact uninstall needs.

`_meta.lua` **must stay a pure literal table with no `require`**:

```lua
return {
    fullname = "readingbuddy",
    description = "Links this reader to readingbuddy on your computer.",
    version = 1,
}
```

Every KOReader plugin writes `local _ = require("gettext")` at the top of its
`_meta.lua`. Ours must not, and this is load-bearing rather than stylistic: the
installer reads an *installed* plugin's version by evaluating its `_meta.lua` in
**the mlua sandbox `koreader.rs` already has**, and that sandbox has no
`require` — `fuzz/seeds/parse_sidecar/require.lua` exists to prove it refuses
one. A pure table is what makes "refuse to overwrite a newer version" cost no
new parser and no manifest format. Say so in a comment in the Lua, because the
next person to touch it will reach for gettext by reflex.

`main.lua` in this slice registers **one menu entry and nothing else**: whether
this reader is paired, and the short device id if so. No network code at all —
not stubbed, not commented out. `pairing.lua` carries no endpoint yet (below),
so the honest status is *paired, nothing to send to*, which is also
`decisions.md`'s "fails closed" in its degenerate case.

Copy the shape of `plugins/kosync.koplugin/main.lua` for menu registration
(`self.ui.menu:registerToMainMenu(self)`) and keep its discipline in mind for
15b — `NetworkMgr:willRerunWhenOnline`, `UIManager:scheduleIn`, a debounce, and
silent failure when non-interactive. Copy none of its networking now.

## Pairing: two files, one format

**On the device**, `readingbuddy.koplugin/pairing.lua` — written by the
installer over USB, inside our own directory because that is the only place the
safety rules permit us to write:

```lua
return {
    device_id = "…",      -- uuid, minted by us at install
    token     = "…",      -- 32 random bytes, hex
    paired_at = 1756…,    -- unix seconds
}
```

A Lua literal table, so the plugin reads it with `dofile` and the engine reads it
back with the same sandbox as `_meta.lua`. One format both sides already parse.

**Deliberately no `host` and no `port`.** We do not know our LAN address, there
is no listener to name, and inventing the endpoint's shape before the protocol
exists is how it gets designed twice. The plugin treats a missing endpoint as
*not configured* and does nothing — which is the correct behaviour for this
slice and is testable today.

**On our side**, migration `0019_paired_devices.sql`:

```sql
CREATE TABLE paired_devices (
    id              INTEGER PRIMARY KEY,
    device_id       TEXT NOT NULL UNIQUE,
    label           TEXT,
    token           TEXT NOT NULL,
    plugin_version  INTEGER NOT NULL,
    installed_at    INTEGER NOT NULL,
    last_mount_path TEXT,
    last_seen_at    INTEGER
);
```

`last_mount_path` is **advisory only** — mount points move between sessions and
between machines. Identity is `device_id`, and nothing may join on the path.
Storage code goes in `crates/engine/src/storage/paired_devices.rs`, matching its
siblings.

**The token is never logged, at any level.** Not `trace!`, not in a
`Diagnostic`, not in an error message. `CLAUDE.md`'s tracing rule names highlight
text and search queries; this joins them, and `googlebooks::scrub_key` is the
pattern if a field ever needs to carry one.

## The surface

```rust
pub struct PluginStatus {
    pub installed: bool,
    pub installed_version: Option<i64>,
    pub our_version: i64,
    pub paired: bool,
    pub device_id: Option<String>,
    pub modified: Vec<PathBuf>,   // files of ours the user edited
}

pub async fn plugin_status(&self, mount: &Path) -> Result<PluginStatus>;
pub async fn install_plugin(&self, mount: &Path) -> Result<InstallReport>;
pub async fn uninstall_plugin(&self, mount: &Path) -> Result<UninstallReport>;
```

All three on the `Engine` facade — `plugin_status` and the other two need
storage, and no frontend reaches into the module.

CLI: `rb ko plugin status|install|uninstall [mount]`, in the tone
`commands/ko.rs` already uses — when something refuses, print the next move
rather than a bare error. `install` prints the exact destination path and what
it will write **before** it writes, because `decisions.md` requires it.

API: **three new `Request` variants**, `PluginStatus` / `InstallPlugin` /
`UninstallPlugin`. Additive, so `API_VERSION` stays **2**. Do not add a field to
an existing variant — `ts-rs` emits a new field as required in TypeScript
however `#[serde(default)]` the Rust is, and that breaks
`gui/src/lib/api/client.ts`. Run `make ts` and commit `bindings.ts`.

## The refusals, each of which is a test

Every one of these is a line in `decisions.md` turned into an assertion:

1. Not a KOReader mount → refuse, write nothing (`is_koreader_mount`).
2. The mount is a symlink → refuse (`offers_reader` already has that rule).
3. Installed version **greater** than ours → refuse, write nothing.
4. A file of ours modified since install → refuse to overwrite it, name it in
   the report. **Never modify a file we did not write.**
5. Uninstall removes only paths whose sha256 matches what we installed; a
   modified file is reported and *left in place*, not deleted.
6. Read-only or failing mount → a typed `Diagnostic`, never a partial tree.
7. **The whole mount is byte-identical before and after, apart from our own
   directory.** Snapshot the fake tree, install, uninstall, diff. This is the
   one test that proves "write only inside our own plugin directory" instead of
   asserting it, and it is worth more than the other six.

Use sha256 rather than mtime for (4) and (5). These are FAT32 volumes: two-second
timestamp granularity, no permissions, no symlinks, case-insensitive. Anything
resting on mtime will be subtly wrong on real hardware and perfectly fine in
`tempfile`.

Build the fake mounts the way `device.rs:588-640` already does. Extend that
helper rather than writing a second one.

## The desktop loop

`datastorage.lua:20` reads **`KO_HOME`** first, before every other branch. So
desktop KOReader can be pointed at a scratch data dir — `KO_HOME=/tmp/ko-test`
— and `$KO_HOME/plugins/` is where the loader will find a plugin. That is the
edit loop for `main.lua`, and it needs no device.

Note in your report that the device target and the desktop target are *not the
same path*, and that the tests only ever exercise the device one.

## Constraints

- Engine + CLI + API. No TUI, no GUI.
- No network anywhere, in tests or otherwise.
- Typed `Diagnostic`s, never pre-formatted strings. Add to `DiagnosticKind` for
  the refusal cases rather than reaching for `EngineError::Other`.
- Never edit an applied migration. `0019` is yours.
- Properties where an invariant exists: install → uninstall returns the tree to
  its prior state, for any tree, is the obvious one.

## Done when

`make fmt lint build-check test ts-check` is green — **not `make ci`**, which a
fresh worktree cannot run honestly: with no `gui/node_modules` the `web-check`
and `routes` legs print `SKIPPED:` and pass without checking anything. Say which
you ran.

`rb ko plugin install` against a fake mount tree writes exactly our directory;
`status` reads back the version and the pairing; `uninstall` leaves the tree as
it found it; each of the seven refusals refuses.

Run the `cargo-tester` agent before committing. If you are a subagent you cannot
launch it — run its procedure directly: `cargo fmt --all --check`,
`cargo clippy --workspace --all-targets --locked -- -D warnings`,
`cargo test --workspace`.

## Two things to send back

**The corrections this forced.** Every entry in `docs/decisions.md` records what
building the item changed about the plan, and that paragraph is the most
valuable thing the item produces. What did the KOReader source say that this
prompt has wrong? What did FAT32 or a real `.koplugin` directory make untrue?

**Push back rather than comply.** Four of five threads in the 11–16 wave did,
and each time they were right. In particular: if minting a token before any
listener exists looks like designing 15b early, say so — that is a defensible
reading and the argument for it is only that pairing over USB is what makes the
wireless step require nothing typed.
