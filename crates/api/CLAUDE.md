# crates/api (package `readingbuddy-api`)

Moved here from the root `CLAUDE.md` unchanged.

**This crate is the boundary, and the GUI is its first semantic client.**
`readingbuddyd` links it but never names a method — it moves bytes. The GUI
landed (spec item 25) and found gaps on its first two screens; a gap is an
**engine item**, never a workaround in the frontend.

Three things that pass added here, and each is load-bearing:

- **`Api::open(data_dir)`.** `Api::new` needs an `Arc<Engine>`, so every caller of
  it depends on `readingbuddy`. Harmless for the daemon, which names no method and
  is tempted by nothing; fatal for a semantic client, whose whole discipline is
  that a missing request must be a *compile error* rather than a `use` away.
  `gui/src-tauri/Cargo.toml` lists this crate and not the engine, and CI's plain
  `cargo check --workspace` covers it because the GUI is a workspace member. The
  extra `EngineConfig` knobs are deliberately not parameters: a client that needs
  one is asking for a configuration surface, which is a request on this protocol.
- **`BookDto.reading_status`** — the current reading's own `status`, beside
  `finished` rather than replacing it. Without it *reading*, *abandoned* and
  *never opened* are one `finished: false` with a `current_page`, because
  `abandon_reading` deliberately leaves the reading open. The two ways to recover
  the distinction above this layer are one request per row, or a client-side join
  of `currently_reading` — row-state derivation, which `gui/CLAUDE.md` bans by
  name. A `String`, so an importer can write a status this build does not know.
- **The `ts` feature and `make ts`.** Off by default: it is a *build-time tool*,
  not a capability of the surface, and iOS links this crate in-process with no use
  for a TypeScript emitter. 77 types carry `#[cfg_attr(feature = "ts", …)]` and
  `scripts/gen-ts.sh` emits one `bindings.ts`. Two things about it are written
  down there rather than rediscovered: **`bigint` is widened to `number`** (Tauri
  IPC is JSON, so an `i64` arrives as a `number` and `JSON.stringify(3n)` throws
  — a `bigint` id is a runtime failure tsc calls correct), and **ts-rs drops
  `#[serde(other)]` on `ErrorCode::Internal`**, so the generated union is
  exhaustive over today's codes while the wire is not. The four warnings that
  names on every run are not silenced on purpose.

Siblings: [`../daemon/CLAUDE.md`](../daemon/CLAUDE.md) (the transport) ·
[`../engine/CLAUDE.md`](../engine/CLAUDE.md) (what this wraps)

**the versioned surface, and the boundary `docs/decisions.md` names.** The daemon is a transport wrapper; *this* is the API, which is what keeps iOS — no daemons, ever — able to link it in-process. Depends on the engine, `serde` and `serde_json` and deliberately nothing else: no socket, no runtime flavour, no CLI parser.

- `dto.rs` — serde mirrors with `From<domain>`. **The domain types stay serde-free**, and the reasons are structural rather than stylistic: `Book` carries `OffsetDateTime`, half the reports carry `PathBuf` and `Diagnostic` carries a `Duration`, so a derive would pick each wire encoding *by accident* and then that accident is the API; and every field name would become a public promise, making a rename of `ko_percent` a breaking change rather than a refactor. `DiagnosticKind` is mirrored **in full**, all seventeen variants — flattening it to `{kind, detail}` would throw away exactly what made `Diagnostic` stop being a `String`. A `PathBuf` crosses as `to_string_lossy`, so a non-UTF-8 filename does not round-trip; JSON is UTF-8 and the alternative is base64 on every path.
- `error.rs` — `ApiError { code, message }`, the pattern `Diagnostic` set: a typed `Copy` classification beside a human string, with no source error inside it. `ErrorCode` is **appended to, never renamed**, and `#[serde(other)]` on `Internal` makes an unknown code degrade instead of failing to parse — asserted, not hoped.
- `protocol.rs` — `Request` is **named** (`{"method":"get_book","params":{"id":3}}`), one variant per facade method; `Response` is **shaped**, ~30 variants, because a reply is already tied to its call by `Call::id` and sixty single-use response names would be sixty things to keep in sync for no information. `Reply::to_line` owns the framing: `serde_json::to_string` escapes newlines inside strings, so a note body cannot break a line-delimited frame, and the guarantee lives in the same function as the terminator.
- **The surfacing item closed the gap items 21/29/30/31/32 left.** Everything those five built was engine-only: no DTO, no request. Now `enrich_book`/`set_book_fields`/`field_provenance` (items 29–30), `table_of_contents` (item 32), `import_device_statistics` (item 31), and the activity log's four (`refill_reading_events`, `reading_events`, `activity_summary`, `activity_by_day`, item 21). Two rules from that work are worth keeping. **The two aggregates validate through the engine's own `DayRange`** rather than passing two strings down, because an inverted span makes every aggregate report a confident, wrong zero and this layer must not be able to route around the refusal. And **`series_index` with no `series` is not refused here**, though `rb set` refuses it: a rule implemented at the seam is a rule the in-process caller never meets, which is this crate's own argument about `dispatch` applied to a DTO.
- `lib.rs` — `Api` over an `Arc<Engine>`, typed methods, and `dispatch` as **pure fan-out**: one arm per request, unpacking arguments and calling the typed method of the same name. A rule implemented in `dispatch` is a rule the in-process caller never meets, so `dispatch_and_the_typed_method_agree` is asserted for both a read and a write. **Handles do not cross**: `update_note_body`/`delete_note`/`file_path` take an id and re-read the row, because a client echoing back a stale `NoteRecord` would write to a path that had moved. **The mount watcher is deliberately absent** — it is a stream, request/response has no shape for one, and a polling wrapper would give the far side a different debounce from the one `watch.rs` guarantees.
