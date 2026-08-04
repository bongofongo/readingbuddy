# crates/api (package `readingbuddy-api`)

Moved here from the root `CLAUDE.md` unchanged.

**This crate is the boundary, and the GUI is its first semantic client.**
`readingbuddyd` links it but never names a method — it moves bytes. So until the
GUI exists, the 77 request variants in `protocol.rs` are exercised only by
`tests/api.rs`. Expect the GUI to find gaps; a gap is an **engine item**, never
a workaround in the frontend. See `docs/gui/spec-gui-17-28.md` item 25.

Siblings: [`../daemon/CLAUDE.md`](../daemon/CLAUDE.md) (the transport) ·
[`../engine/CLAUDE.md`](../engine/CLAUDE.md) (what this wraps)

**the versioned surface, and the boundary `docs/decisions.md` names.** The daemon is a transport wrapper; *this* is the API, which is what keeps iOS — no daemons, ever — able to link it in-process. Depends on the engine, `serde` and `serde_json` and deliberately nothing else: no socket, no runtime flavour, no CLI parser.

- `dto.rs` — serde mirrors with `From<domain>`. **The domain types stay serde-free**, and the reasons are structural rather than stylistic: `Book` carries `OffsetDateTime`, half the reports carry `PathBuf` and `Diagnostic` carries a `Duration`, so a derive would pick each wire encoding *by accident* and then that accident is the API; and every field name would become a public promise, making a rename of `ko_percent` a breaking change rather than a refactor. `DiagnosticKind` is mirrored **in full**, all seventeen variants — flattening it to `{kind, detail}` would throw away exactly what made `Diagnostic` stop being a `String`. A `PathBuf` crosses as `to_string_lossy`, so a non-UTF-8 filename does not round-trip; JSON is UTF-8 and the alternative is base64 on every path.
- `error.rs` — `ApiError { code, message }`, the pattern `Diagnostic` set: a typed `Copy` classification beside a human string, with no source error inside it. `ErrorCode` is **appended to, never renamed**, and `#[serde(other)]` on `Internal` makes an unknown code degrade instead of failing to parse — asserted, not hoped.
- `protocol.rs` — `Request` is **named** (`{"method":"get_book","params":{"id":3}}`), one variant per facade method; `Response` is **shaped**, ~30 variants, because a reply is already tied to its call by `Call::id` and sixty single-use response names would be sixty things to keep in sync for no information. `Reply::to_line` owns the framing: `serde_json::to_string` escapes newlines inside strings, so a note body cannot break a line-delimited frame, and the guarantee lives in the same function as the terminator.
- `lib.rs` — `Api` over an `Arc<Engine>`, typed methods, and `dispatch` as **pure fan-out**: one arm per request, unpacking arguments and calling the typed method of the same name. A rule implemented in `dispatch` is a rule the in-process caller never meets, so `dispatch_and_the_typed_method_agree` is asserted for both a read and a write. **Handles do not cross**: `update_note_body`/`delete_note`/`file_path` take an id and re-read the row, because a client echoing back a stale `NoteRecord` would write to a path that had moved. **The mount watcher is deliberately absent** — it is a stream, request/response has no shape for one, and a polling wrapper would give the far side a different debounce from the one `watch.rs` guarantees.
