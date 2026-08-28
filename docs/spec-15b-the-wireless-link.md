# Item 15b — the wireless link

Slice b of item 15. Its own file rather than more of `docs/spec-11-16.md`
because a protocol is its own subject, which is the same rule that gave
`storage/`, `providers/`, `ui/` and `render3d/` their own `CLAUDE.md`s.
`docs/spec-11-16.md` item 15 points here.

Everything below about KOReader was read from a checked-out `koreader/master`
and cited by file and line. `docs/koreader-format.md` §1 is the order of
authority: the source is the spec, a fixture is only ever evidence. Correct
anything here that the source contradicts.

## What slice a left

A reader holding `readingbuddy.koplugin/` with a `pairing.lua` that carries a
`device_id`, a 32-byte `token` and a `paired_at` — **and no host and no port**,
because there was no listener to name. Our half is one `paired_devices` row.
The plugin registers one menu entry and has no network code at all.

So 15b starts with a paired reader that requires **nothing typed**: the reader
was in the user's hand once, over USB, and that was the pairing. Every design
below is judged against whether it preserves that.

## The two verbs, and the one thing under them

The user drives both. Neither happens by itself.

- **Push** — on the reader: *Tools → readingbuddy → Push to <computer>*. The
  reader sends. This is the primary workflow, and it is the one nothing else in
  this space does.
- **Pull** — on the desktop: the devices page fetches from a reader that is on
  the LAN with its own listening window open.

Each verb has a side that is **open** and a side that **seeks**, and they swap.
That is one rendezvous protocol used twice, not two designs:

> **Whoever is open answers probes and beacons. Whoever seeks probes and
> listens.**

The consequence that makes the desktop's toggle a feature rather than a
limitation: **discovery only works while the other side is open.** With the
desktop's listener off there is no service to find, nothing to fingerprint, and
nothing to leak — and the reader's failure message writes itself: *"No
readingbuddy answered. Is it listening?"* That is `decisions.md`'s **fails
closed** at the transport layer.

## The desktop listener: three states

`crates/daemon` is unix-socket-only by a written argument (`server.rs:9-21`) —
"HTTP … would put a listening TCP port on a laptop for no gain". **This item is
the gain**, and the toggle is the mitigation. Overturn the argument
deliberately, in that file, with a note; do not quietly grow a second listener
next to it.

| state | what is bound | who chooses |
|---|---|---|
| **Off** | nothing. The default. | — |
| **Open for a window** | TCP + UDP, closing on a timer (proposal: 5 minutes) or on the first completed push | a button: *Listen now* |
| **Always** | TCP + UDP, for the daemon's life | a setting, for the desktop that lives on the LAN |

**The UDP responder lives and dies with the TCP listener.** One state, not two —
an announcer that outlives the thing it announces is a device telling the user
to try again against a closed port.

The mode is the **daemon's** state, not the engine's: it persists next to the
socket in the data dir the daemon already owns, and the API gets
`StartListening { minutes }` / `StopListening` / `ListenerStatus`. The engine
database stays about readers. *(If the thread concludes this wants a settings
table in the engine instead, that is a defensible reading — say so rather than
building one silently.)*

## The rendezvous protocol

One fixed **UDP** port, hard-coded. Not a config knob, for `MOUNT_QUIET`'s
reason: a knob means the tested value and the shipped value can differ. The
**TCP** port is not fixed — it is announced in the reply, so a busy port is a
runtime fact rather than a support conversation.

```
seeker  → broadcast   HELLO   {nonce}
opener  → unicast     HERE    {name, tcp_port, device_id?, hmac(token, nonce)}
seeker  → tcp connect         (verify hmac before sending one byte)
```

**HMAC-SHA256 is available on the device**: `require("ffi/sha2")` exports `hmac`
alongside `sha256`, `bin_to_hex` and `hex_to_bin` (koreader-base `ffi/sha2.lua`,
verified — `sha.hmac` at `:4787`, exported at `:5625`, and `block_size_for_HMAC`
initialised at the foot of the module). So the token minted in 15a becomes a
challenge-response credential and **never crosses the wire**. Both sides prove
possession; neither transmits it.

> **Correction, measured (item 15b, stage 1).** Three things about that module
> are not what a caller assumes, and the third is a live trap.
>
> 1. `sha.sha256(msg)` returns a **64-character hex string**, not raw bytes.
> 2. `sha.hmac(hash_func, key, msg)` therefore also returns **hex**, lowercase.
>    Its signature is `hmac(hash_func, key, message)` — the hash *function*
>    first, so it is `sha.hmac(sha.sha256, token, nonce)` and never
>    `sha.hmac(token, nonce)`.
> 3. **The key encoding is a silent fork and must be pinned.** Our token is 32
>    random bytes written into `pairing.lua` as 64 hex characters, so "the
>    token" is two different keys depending on which end you ask. Both produce a
>    perfectly valid MAC and neither side looks wrong:
>
>    ```
>    token = "0123456789abcdef" x4   nonce = "nonce-0001"
>    key = the 64 hex CHARS  -> a38b20860c54b13a06e1fac37207b4ca1120db8946e82848818567a07d7a7cae
>    key = the 32 raw BYTES  -> 26ae9d255e7a30de01b32de4544430f32a4bdf4dea6907f07c3c604311a599fe
>    ```
>
>    Both were produced by koreader-base's own `sha2.lua` under lua5.4 and both
>    agree byte-for-byte with a reference HMAC-SHA256, so the module is correct
>    and the choice is ours alone. **Take the hex characters as the key** — the
>    top line — because that is the string as `pairing.lua` literally holds it,
>    so the Lua side needs no `hex_to_bin` and cannot get the decode wrong on a
>    device nobody can debug. Rust's side is then `Hmac::new_from_slice(token
>    .as_bytes())`, on a `&str` rather than on decoded bytes, and the vector
>    above is what pins it. `partial_md5.rs`'s `agrees_with_the_device` is the
>    pattern: a vector produced by the device's own implementation, not one we
>    computed and then agreed with ourselves about.

That buys the property that matters more than confidentiality: **the reader
verifies identity, not address.** A rogue responder on a café LAN that answers a
broadcast first cannot make the reader send it a single highlight.

TLS is therefore deferred, with the reason written down: LuaSec is present
(`socketutil.lua:8` requires `ssl.https`, `httpasync.lua` does non-blocking
TLS), but a LAN listener means a self-signed certificate and a pinned
fingerprint in `pairing.lua`. HMAC over plaintext on a home LAN is the posture
kosync already has, and the payload is the same bytes that cross a USB cable
unencrypted today.

### It is not the daemon's protocol

The wireless listener speaks a **small closed protocol with three messages** and
maps them onto engine calls internally. It must **never** accept
`readingbuddy_api::Call`. A reader is not a trusted local client, and exposing
sixty methods to a LAN peer to serve two verbs is the opposite of `server.rs`'s
own rule that the transport names no method.

### The payload is the sidecar

The reader sends **the sidecar bytes it already has**, not a bespoke delta.

This is the decision most likely to be re-opened by an eager thread, so the
argument: `koreader.rs` already parses that format, the parse is fuzzed, the
import is idempotent, the goldens cover it, and the tier-1/tier-2 corpora are
made of it. A second wire format would be a second parser for the same
information, tested by nobody, and the first divergence between them would show
up as highlights that import differently depending on which cable they came
down. Sending the sidecar makes wireless a **transport for the import we have**
rather than a second import.

Statistics stay out, exactly as they are out of `sync_device`: measured minutes
are a different datum with a different verb (`0020`'s argument, one layer down).

## Discovery, for a laptop that leaves and rejoins

A ladder, tried in order, each rung with a short timeout, first hit cached. All
of it **off the UI thread** — `httpasync`'s coroutines or
`Trapper:dismissableRunInSubprocess` — because three rungs at a second each is
three seconds of frozen e-ink otherwise.

| # | rung | wins when | fails when |
|---|---|---|---|
| 1 | **Cached endpoint**, one TCP connect | the common case: a renewed DHCP lease is usually the same address | the laptop moved subnets or took a new lease |
| 2 | **UDP broadcast** to `255.255.255.255:<fixed>` | address changed, same subnet | AP isolation (guest wifi, some mesh kit), split 2.4/5 GHz subnets, a VPN owning the laptop's default route |
| 3 | **Directed broadcast** to the cached `/24`'s `.255` | drivers and APs that pass `192.168.1.255` while filtering `255.255.255.255` | a different subnet than the cached one |
| 4 | **Hostname** — `socket.dns.toip("<name>")`, then `.lan`, `.home.arpa` | **the best rung for a roaming laptop.** Most consumer routers (dnsmasq, OpenWrt, AVM) register DHCP hostnames, so a machine that moves address keeps its name. One DNS query, and no code of ours on the wire | the router does not register hostnames; corporate DNS |
| 5 | **mDNS**, hand-rolled | broadcast filtered, multicast not | **KOReader has no mDNS, zeroconf or Bonjour anywhere** — grep over `frontend`, `plugins` and `base` returns nothing — so this is ~100 lines of DNS packet code in Lua, and e-reader wifi power-save drops multicast often. Hold it in reserve; do not build it first |
| 6 | **Typed address**, cached forever | everything else failed | typing an IP on e-ink is what USB pairing existed to avoid. **No QR** — the reader has no camera |

Rung 2 is not speculative: it is `plugins/calibre.koplugin/wireless.lua:119-139`
verbatim in shape — `socket.udp4()`, `setoption("broadcast", true)`,
`setsockname("*", port)`, `sendto("255.255.255.255", port)`, `receivefrom()`,
three-second timeout, five well-known ports.

Two refreshes sit under the whole ladder and cost nothing:

- **Re-stamp the endpoint on every USB connection.** The wired path already
  runs, and for a laptop that comes and goes the reader gets plugged in anyway.
- **Re-stamp on every successful push.** The device learns the current address
  as a side effect of working.

For an actual desktop, rungs 1 and 4 make the rest theoretical: a DHCP
reservation and it never misses. The ladder exists for the laptop.

## Pull, and why the reader beacons

The reader's toggle is *listen while wifi is on* — a short window it opens
deliberately, and it knows the moment it opened. So the **reader announces**
(one small UDP packet every couple of seconds for the window's life) rather than
the desktop scanning. The desktop sits on the fixed port and the devices page
can say *"Kindle is ready — pull now"* the instant it appears, with no scanning,
no ARP sweep, and no discovery code on the desktop beyond a socket read.

The reader's server side is core, not ours to invent:
`frontend/ui/message/simpletcpserver.lua` is a LuaSocket HTTP server
(`socket.bind`, `settimeout(0.01)`, header lines to a blank line), and
`plugins/httpinspector.koplugin` runs exactly it on port 8080. Copy that shape.

> **Correction, read from the source (item 15b, stage 1).** `SimpleTCPServer`
> **has no run loop**. It exposes `start`, `stop`, `send` and a `waitEvent`
> that does exactly one non-blocking `accept`; whoever uses it must poll it, and
> `httpinspector` is where to read how. Worse for us: once a client *does*
> connect, `waitEvent` sets a 100 ms socket timeout and reads header lines **in
> line**, then 500 ms for the response — so a stalled peer blocks whatever
> thread is polling for up to six tenths of a second. That is fine inside
> `UIManager`'s own loop and is *not* fine as a claim that the server is
> non-blocking. The spec's "the UI must never block" therefore binds the pull
> side too, not just the push side's HTTP client.

**The window must close itself** — on its timer, and on `onEnterStandby`. The
device will suspend under a transfer otherwise; `autosuspend` and
`onEnterStandby` are what `httpinspector` already handles.

## Three collisions with what 15a shipped

Settle these before any Lua is written. Each is a small change to code that is
currently correct, and each will otherwise be discovered as a bug.

1. **The plugin cannot write a cache file into its own directory.**
   `plugin::inspect` (`plugin.rs:388-405`) skips exactly two names —
   `installed.lua` and `pairing.lua` — and anything else absent from the
   manifest lands in `unrecognised`, which makes `refuse_if_obstructed`
   (`plugin.rs:411-434`) fail **both install and uninstall**. A plugin that
   caches its learned endpoint next to itself bricks its own installer. The fix
   is a third skipped, never-hashed, device-writable `endpoint.lua`, removed by
   uninstall — small, deliberate, and it needs a test that install and uninstall
   both still work with one present.

2. **`pairing.lua` holds one computer, and a second one silently steals the
   reader.** `Engine::install_plugin_at` (`lib.rs:1226-1241`) reads the existing
   `device_id`, and when *this* machine has no row for it mints a fresh id and
   token and overwrites the file. Plug a reader into a second readingbuddy
   install and the first machine's token is dead while its `paired_devices` row
   still claims the pairing. *"Push to the connected computer"* presumes a
   choice the file cannot express, so **`pairing.lua` becomes a list** — id,
   token, label, endpoint hint per computer — and the reader's menu names which
   one it is pushing to. The installer then adds or updates *its own* entry and
   leaves the others alone, which is `uninstall` is exact applied to a file
   rather than a directory.

3. **The daemon's TCP argument is overturned, not ignored** — see *The desktop
   listener* above.

### What stage 1 actually found

Built, landed, and reported here because the spec had two of these wrong and
missed a third entirely.

- **The list entry's `label` is the *computer's* name, not the reader's.**
  `paired_devices.label` is what you call the Kobo; this is what you call the
  laptop, and they sit two lines apart in any screen that shows both. Shipped as
  `name` for that reason. It is the **hostname**, which is not a detail: it is
  the same string the router registers, so the menu label and rung 4 of the
  ladder are one value rather than two that can disagree.
- **The entry carries no endpoint hint, and the spec asked for one.** *"id,
  token, label, endpoint hint per computer"* contradicts the `endpoint.lua`
  half of the same stage. The two files have two authors — `pairing.lua` is what
  we told the reader over the cable, `endpoint.lua` is what the reader learned —
  and collapsing them would put a device-written value inside a file whose whole
  point is that we wrote every byte of it. The wired hint is the hostname; the
  learned address is `endpoint.lua`'s.
- **The identity question the spec never asks: there is no *computer* id.**
  `device_id` is the id of the **reader**, minted per computer, so two computers
  hold two different ids for one Kobo — which means it doubles as the handle a
  computer recognises its own entry by, and no new identity was needed. But the
  installer cannot resolve it alone: `plugin::inspect` has no database, so
  *which of these entries is ours* is answered on the facade by looking each id
  up, and `plugin::install` is told the answer. Anything in stage 2 that wants
  to know which computer a message is from gets `device_id` and looks it up the
  same way.
- **A hostname is the first value in that file that is not hex**, so the raw
  `format!("\"{x}\"")` interpolation that had been safe by accident stopped
  being safe. A name carrying a quote writes a `pairing.lua` that does not
  parse; the reader reports *not paired* and the installer, reading its own
  output back, reports a fresh device — two wrong answers from one missing
  escape.
- **`PluginStatus.device_id` had to change meaning**, and the old meaning was
  already wrong: it held whatever id the file named, so a reader belonging to
  another install reported a stranger's id under a heading that reads *our* id
  everywhere else. Harmless with one pairing, unreadable with several. No DTO
  changed — the field kept its name and type — so `bindings.ts` was not
  regenerated and the GUI needs no edit.

## Migration `0021`

Two columns on `paired_devices`, both breadcrumbs, neither ever joined on —
`last_mount_path`'s rule (`0019`) applies to both:

- `last_wireless_at` — when this reader last reached us over the LAN. It is a
  third timestamp beside three that already exist and it means none of them:
  `installed_at` is when the relationship started, `last_seen_at` is when the
  reader was last **in hand**, `last_synced_at` (`0020`) is when **everything**
  it had was brought across.
- `last_lan_addr` — the address it came from, so the desktop's pull has a rung-1
  hint of its own.

A completed *push of everything* stamps `last_synced_at` too, because that
column's meaning is about the data and not about the cable. A push of one book
does not, for `0020`'s stated reason.

No back-fill; NULL means *not since we started recording*, which every caller
handles already.

## Testing

The engine's standards apply unchanged, and one of them looks like it forbids
this item. It does not:

- **"No network in tests, ever"** means nothing may leave the machine. Loopback
  is already ordinary here — `tests/provider_http.rs` runs `wiremock`, which
  binds a real local port. So the listener is testable against `127.0.0.1`.
- **The broadcast leg must never emit a packet in a test.** Put the rendezvous
  transport behind a trait and inject it, exactly as `watch.rs` injects
  `MountStir`s rather than driving `notify`: "a watcher that can only be driven
  by plugging in real hardware is a watcher with no tests" is the same sentence
  about discovery. The ladder's *ordering and fallback* is then a unit test with
  a scripted resolver, which is the part that actually has bugs.
- **The listener state machine** — off / window / always, expiry, close-on-first-push,
  UDP dying with TCP — is a test with a paused clock
  (`#[tokio::test(start_paused = true)]`), the same instrument the debounce uses.
- **HMAC vectors** cross the language boundary: the same key and nonce, hashed
  by `ffi/sha2` and by our Rust, must agree. Pin a vector on both sides.
- **The token is never logged, at any level** — `0019` already says so, and this
  item is where a debug line would be reached for. `tests/tracing_redaction.rs`
  is where that is asserted.

### And the Lua stops being untested here

15a's `main.lua` is 73 lines that no test executes; the only Lua a test parses
is `_meta.lua`, through the sidecar sandbox, and that test exists to protect the
version-reading trick rather than the plugin's behaviour. That was defensible
for a menu entry. It is not defensible for a discovery ladder, a debounce and an
HMAC.

So this item owes a Lua gate: at minimum every shipped `.lua` `loadfile`s
cleanly in CI, and better, the pure functions (ladder ordering, endpoint
parsing, HMAC) run under a harness with `require` stubbed. `mlua` is already a
dependency and already sandboxes untrusted Lua — the sandbox that reads sidecars
is the obvious host for it.

### What still needs hardware

Say so in the report rather than implying coverage: broadcast behaviour on the
user's actual AP, wifi coming up via `NetworkMgr:beforeWifiAction`, suspend
mid-transfer, and whether the menu entry appears at all — which is *still*
unconfirmed from 15a.

## Constraints that eliminate designs

- **Wifi is off by default and comes up transiently.**
  `NetworkMgr:beforeWifiAction` prompts or fails depending on the user's
  setting; `runWhenOnline` (`manager.lua:698`) and
  `turnOnWifiAndWaitForConnection` (`manager.lua:517`) are the idioms. "Push the
  moment a highlight is made" is really "push next time the radio is up", which
  is why this item's push is a **tap**, not a hook. The hooks
  (`readerannotation.lua:515` `onAnnotationsModified`, `:245` `onCloseDocument`)
  are for a later item that has earned them.
- **The reader suspends mid-transfer.** Everything must be resumable and
  idempotent — which the sidecar payload gets for free from the existing import.
- **The UI must never block.** `httpasync` or a Trapper subprocess, never a bare
  blocking `socket.http`.
- **Never on the open internet.** LAN only, no relay, no cloud, no hole
  punching, no port forwarding, and nothing that works from a coffee shop. If
  that is wanted it is a different item with a different threat model.

## Explicitly not in this item

Auto-push on any event. Two-way sync. Writing anything to the reader over the
wire — the wireless path is **read-only toward us**, the way mount → import is,
and every write to a device stays explicit and wired. Sending books to the
reader (that is OPDS or calibre-wireless emulation, and a separate item).
Statistics. TLS. mDNS. A second frontend for any of it beyond the devices page
and the CLI.

## Build order — one thread, three stages

Not three threads: push and pull share the rendezvous protocol, and three agents
on one protocol produce three dialects of it — items 26–28's lesson.

1. **The plumbing, no network.** `pairing.lua` becomes a list, `endpoint.lua`
   becomes a permitted runtime file, migration `0021`. Entirely offline,
   entirely testable, and it is what unblocks everything else. **Landed** —
   see *What stage 1 actually found* above, and the HMAC correction, which was
   settled here rather than in stage 2 because it is the one thing that would
   otherwise have been discovered on hardware.
2. **The listener and push.** Daemon TCP + UDP responder, the three states, the
   API requests, the plugin's menu verb and its discovery ladder.
3. **Pull.** The reader's window and beacon, the desktop's seeker, the devices
   page's *ready* state.

Stage 1 has value even if 2 and 3 slip: it makes a reader paired to two
computers stop being silently broken.
