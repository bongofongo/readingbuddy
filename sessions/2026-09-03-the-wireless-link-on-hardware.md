---
title: The wireless link, on a real Kindle — five bugs no test could have caught
date: 2026-09-03
scope: `crates/engine/koplugin/readingbuddy.koplugin/`, `plugin.rs`, `wireless.rs`,
       `crates/cli/src/commands/ko.rs`, `docs/spec-15b`, `docs/decisions.md`
---

# The wireless link, on hardware

The Kindle was plugged in and the whole of 15b met a device for the first time.
Every one of the five bugs below was invisible to the suite that shipped it, and
four of them were invisible *because* of how the suite was shaped. Push and pull
both work now, both idempotently, both verified end to end.

## Where it started

`main.lua` on the device was still **15a's 73-line plugin**, and `ko plugin
status` called it *"v1 (up to date)"* — the first bug, and the reason to look.

## Decisions locked

- **The plugin punches its own firewall hole on a Kindle**, transient and
  automatic, `SSH.koplugin`/`httpinspector.koplugin`'s rule verbatim. The user's
  requirement drove it: *no manual step, and the device keeps a normal user's
  configuration.* A rule that exists for the 2s of a probe and is removed on
  every path meets both.
- **The installer stamps `endpoint.lua`** with this computer's LAN address, so a
  reader's first rung is *unicast* — which needs no hole and no DNS. The spec had
  already asked for it ("re-stamp on every USB connection"); the firewall finding
  is what made it load-bearing rather than an optimisation.
- **The address is a parameter, not ambient state.** `plugin::install` takes it
  exactly as it takes `paired_at`. Read inside the function it would stamp
  whatever address the test machine happened to have — passing on a laptop,
  differently on a runner, asserting nothing on either.
- **`endpoint.lua` now has two writers and they must agree byte-for-byte**,
  asserted against the plugin's own `serialiseEndpoints`. Disagreeing formatting
  would mean each undoing the other's file on every connection — a pointless
  FAT32 write per cable, and a diff that never settles.
- Session ran with a **passwordless SSH root shell** on the reader (KOReader's
  own SSH plugin). That is what turned a replug-per-iteration loop into a
  read-the-rules-directly loop; it is also why the last instruction of the
  session is to switch it off again.

## Bugs found

All five pre-existing. In the order they were hit:

1. **`_meta.lua` was never bumped past 1.** 15b rewrote `main.lua` from 73 lines
   to 778 and left the version alone, so every surface — CLI, devices page —
   reported a plugin *with no network code in it* as up to date, and a user with
   15a would never have been told to reinstall. `PLUGIN_VERSION = 2`.
2. **The menu threw while KOReader was building it.** `for _, entry in
   ipairs(self.pairings)` shadows the module's gettext `_`, and the next line
   calls it. The failure is not a broken item, it is **no readingbuddy entry in
   Tools at all**, and it fires only on a reader with ≥1 pairing — every real
   device, and no test. The device's own log named the line my harness had
   already named: `main.lua:731: attempt to call local '_' (a number value)`.
3. **The DNS rungs had never put a packet on the wire.** `udp:sendto` takes an
   address; the ladder handed it `rung.host`, which for rungs 4–6 is a *name*.
   On the device: `sendto(msg, "arch-fongo", …)` → `nil, "Name or service not
   known"`, `socket.dns.toip("arch-fongo")` → `192.168.1.63` one line later.
   The spec said resolve first and the code skipped it.
4. **The Kindle firewall hits push, not pull.** See gotchas.
5. **A pull request was never framed.** `SimpleTCPServer` reads lines with a
   100 ms timeout and calls its callback **only on a blank line**, closing
   silently otherwise. The desktop sent `open` with one newline and waited, so
   the first real pull answered *"the reader said nothing"* — about a reader
   that was working perfectly and had not been told the request was over. The
   reader's own handler already stripped from the first `\r\n`, i.e. the two
   halves of one protocol were written against different pictures of it and the
   half nobody could run was the wrong one.

## Technical gotchas

The highest-value section, as always.

- **A Kindle's INPUT chain drops the answer to a broadcast probe and passes the
  answer to a unicast one.** `-P INPUT DROP` plus `-A INPUT -i wlan0 -p udp -m
  state --state ESTABLISHED -j ACCEPT`: a datagram sent to `255.255.255.255`
  creates a conntrack entry whose reply tuple is *from* `255.255.255.255`, so a
  unicast `HERE` from one host does not match it, arrives NEW, and meets the
  policy. Measured both ways in one run from the device itself. **This is why
  "the reader dials out, so push is safe" was wrong**: discovery is a
  request/*reply* and the reply is inbound.
- **The hole must name the probe socket's own port**, read back with
  `getsockname`. A fixed port is simpler and wrong twice: it collides with the
  responder an open pull window binds to the rendezvous port, and a rule wants
  the narrowest port it can name.
- **`_ = i` to silence an unused loop index is the same bug one line later** —
  `_` is the module's gettext upvalue, so the assignment replaces it with a
  number for the rest of the process. Name the index and leave it unused.
- **The plugin compiles identically under LuaJIT and Lua 5.4**, so the 5.1/5.4
  dialect worry is not where these bugs live — both accepted every version of
  the file, broken and fixed. Compiling is a floor, not a gate.
- **A stub `require` that answers every call with itself hides shadowing bugs**,
  because the stubbed `_` is callable whatever it is bound to. The harness now
  answers `gettext` and `ffi/util` for real.
- **And a swallow-everything stub makes the plugin's own `self` bottomless**:
  under it `self.window_server` is never nil, so `windowOpen()` is always true
  and the menu under test is not the menu on the device. Six lines of real
  `WidgetContainer:extend`/`new` fix it.
- **`ping` and a SYN to a closed port are not evidence about a reply.** Both are
  *unsolicited* inbound and prove only that the policy is DROP; whether a
  conntracked reply survives is a different question and needed the device.
- **A test double must be shaped like the thing it doubles.** The pull double
  accepted a bare line, so 14 loopback tests passed against a framing the real
  reader rejects. It now requires the blank line.
- **`scp` is `-P`, `ssh` is `-p`**, and dropbear on this Kindle has no `scp`
  binary at all — `ssh host "cat > /tmp/f" < file` (base64 for safety) is the
  transfer that works.
- **zsh does not word-split unquoted variables**, so `$SSHOPTS` arrives as one
  argument and ssh reports `keyword stricthostkeychecking extra arguments`.
- **`pkill -f "readingbuddy ko listen"` kills the shell running it**, because
  the pattern matches that shell's own command line. Exit 144, twice.
- Kindle wifi power-save shows as **~170 ms RTT to the gateway**; ordinary, not
  a fault, and well inside the 2s probe timeout.

## Verification

- **Push, live**: 22 sidecars over the LAN → 22 books, 491 highlights, 74
  flashcard candidates. Door shut itself on completion, as designed.
- **Push again**: 22 sidecars, every one `matched_by=md5`, `inserted=0
  updated=0`, totals byte-identical (26/491/74/28). Idempotent over the wire.
- **Pull, live** (stage 3's first hardware run): *"22 books came across: 0 new,
  0 updated"*, totals unchanged again.
- **Firewall cleanup, checked on the device**: while a window was open, `udp
  61862` + `tcp 41759` present; after it closed, both gone, socket closed, only
  Amazon's own `ppp0`/`wwan0` rules left.
- **The device re-stamped `endpoint.lua`** with `port = 44567` — the port it
  actually reached — in the installer's own format.
- **15b's HMAC vector re-run through this Kindle's own `sha2.lua`**:
  `a38b2086…7d7a7cae`, matching the pinned value. The key-encoding fork is
  settled on hardware, not just on master.
- Engine plugin suite 29 tests; wireless integration 14.
- Every one of the five bugs is now gated by a test that fails for that reason —
  mutation-checked for the menu one (reintroducing `for _,` reproduces the
  device's exact error).

## Deferred

- **`plugin status` cannot see byte drift.** Same version, different bytes reads
  as *up to date* — bug 1 in general form. `installed.lua` already records a
  sha256 per file; comparing those against the shipped bytes would catch it
  without relying on version discipline. The right fix, and bigger than this
  session.
- **`last_wireless_at` is not in the API DTO**, so the GUI devices page cannot
  show what the CLI now shows. An engine/API item, never a frontend workaround.
- **Rung order is unexamined.** Broadcast is tried before DNS because it is
  "cheap", but on a Kindle it is a guaranteed timeout without a hole while a DNS
  query is one packet that fails fast. Not reordered here — it is a spec
  decision and the stamped endpoint makes it moot in the common case.
- The OUTPUT half of the firewall rule is **inert on this device** (`-P OUTPUT
  ACCEPT`) and kept only for parity with the two shipped plugins that use it.
- Still unexercised on hardware: suspend mid-transfer, `NetworkMgr` prompting, a
  reader on a network whose router registers no hostnames.
- **Real sidecars into `tests/fixtures/koreader/real/`** — open since July.
