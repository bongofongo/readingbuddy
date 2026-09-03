-- readingbuddy's KOReader plugin.
--
-- Item 15a established the *link* and nothing else: readingbuddy installs this
-- directory over USB and writes `pairing.lua` beside this file, and the reader
-- can say who it is paired with. There is still no network code here — not
-- stubbed, not commented out — because there is still no listener to talk to.
--
-- Item 15b's first stage makes `pairing.lua` a **list**. A reader can be paired
-- with more than one computer, and while the file held a single entry the
-- second readingbuddy install silently overwrote the first one's — so this
-- plugin has to be able to name which computer it means before it can push to
-- one. Two shapes are read: the list under `computers`, and item 15a's flat
-- single entry, which is a one-element list. Detected by shape, because a
-- version number nothing branches on is decoration.
--
-- Stage 2 gives it a network. **Push is a tap and never a hook**: the user
-- opens Tools → readingbuddy → Push to <computer>, and nothing happens by
-- itself. `onAnnotationsModified` and `onCloseDocument` exist and are
-- deliberately not used — wifi is off by default and comes up transiently, so
-- "push the moment a highlight is made" really means "push next time the radio
-- is up", which is a different feature with a different failure mode.
--
-- No `pairing.lua` entry carries a host or a port. A missing endpoint is *not
-- configured*, and the plugin then does nothing at all, which is
-- `docs/decisions.md`'s "fails closed" in its degenerate case. What the reader
-- *learns* lands in `endpoint.lua` — written by this plugin, never by the
-- installer, never hashed into `installed.lua`, and removed by uninstall — and
-- the rule stays: cannot reach the app → say so once, and never block the UI.
--
-- The token never crosses the wire. It keys an HMAC-SHA256 challenge, and the
-- key is the **64 hex characters as `pairing.lua` holds them** — not the 32
-- bytes they encode. `ffi/sha2` computes correct HMAC either way and the two
-- answers differ completely with neither side looking wrong, so the choice is
-- written down here and in `crates/engine/src/wireless.rs`, with the vectors in
-- `docs/spec-15b-the-wireless-link.md`.

local InfoMessage = require("ui/widget/infomessage")
local UIManager = require("ui/uimanager")
local WidgetContainer = require("ui/widget/container/widgetcontainer")
local logger = require("logger")
local T = require("ffi/util").template
local _ = require("gettext")

-- The fixed rendezvous port. A constant on both sides, and checked rather than
-- picked: IANA has no assignment at or above 61000, and Linux's default
-- ephemeral range stops at 60999.
local RENDEZVOUS_PORT = 61862
local PROTOCOL_VERSION = 1

local ReadingBuddy = WidgetContainer:extend{
    name = "readingbuddy",
    is_doc_only = false,
}

-- Normalise whatever `pairing.lua` returned into a list of computers.
--
-- A **pure function of a table**, deliberately: it is the only logic in this
-- plugin worth being wrong about, and a function that reaches for `self` or for
-- a `require` is one no test can execute. `crates/engine/src/plugin.rs`'s
-- `the_plugin_reads_every_pairing_shape_the_installer_can_write` runs exactly
-- this function under mlua against exactly the bytes the installer writes.
--
-- Entries missing an id or a token are dropped rather than repaired: half a
-- credential proves nothing, and an entry that cannot answer a challenge is a
-- menu line that fails after the user taps it.
function ReadingBuddy.normalisePairings(raw)
    if type(raw) ~= "table" then
        return {}
    end
    local entries = raw.computers
    if type(entries) ~= "table" then
        -- Item 15a's flat shape: one computer, written before there was a list.
        entries = { raw }
    end
    local out = {}
    for _, entry in ipairs(entries) do
        if type(entry) == "table"
            and type(entry.device_id) == "string" and entry.device_id ~= ""
            and type(entry.token) == "string" and entry.token ~= "" then
            table.insert(out, entry)
        end
    end
    return out
end

-- What to call a computer in the menu.
--
-- The name the installer wrote, and otherwise the first bytes of the id — never
-- "unknown", because the id is a real handle the user can match against what
-- readingbuddy shows on the desktop.
function ReadingBuddy.computerName(entry)
    if type(entry.name) == "string" and entry.name ~= "" then
        return entry.name
    end
    return tostring(entry.device_id):sub(1, 8)
end

-- ---- the discovery ladder, as a pure function ------------------------------

-- The addresses to try, in order, for one computer.
--
-- **Pure**, and that is the point: this is the part with bugs in it, and a
-- ladder that can only be exercised by moving a laptop between subnets is a
-- ladder with no tests. `crates/engine/src/plugin.rs` runs exactly this under
-- mlua. The rungs, and what each one is for:
--
--   1. the **cached** address, straight from `endpoint.lua`. The common case:
--      a renewed DHCP lease is usually the same address.
--   2. **broadcast** to 255.255.255.255. Wins when the address changed and the
--      subnet did not. `plugins/calibre.koplugin/wireless.lua` does exactly
--      this shape, so it is known to work on these devices.
--   3. **directed broadcast** to the cached /24's .255, for the drivers and
--      APs that pass 192.168.1.255 while filtering 255.255.255.255.
--   4. the **hostname**, resolved by DNS. The best rung for a roaming laptop:
--      most consumer routers register DHCP hostnames, so a machine that
--      changes address keeps its name — and no code of ours goes on the wire.
--
-- mDNS is deliberately absent. KOReader has none anywhere, so it would be a
-- hundred lines of hand-rolled DNS packet code, and e-reader wifi power-save
-- drops multicast often. It is held in reserve, not built first.
function ReadingBuddy.ladder(cached, name)
    local rungs = {}
    if type(cached) == "table" and type(cached.host) == "string" and cached.host ~= "" then
        table.insert(rungs, { kind = "cached", host = cached.host, port = cached.port })
        -- The /24 the cached address sat on. Only ever derived from a dotted
        -- quad: a hostname has no subnet to broadcast into.
        local a, b, c = cached.host:match("^(%d+)%.(%d+)%.(%d+)%.%d+$")
        if a then
            table.insert(rungs, { kind = "broadcast", host = "255.255.255.255" })
            table.insert(rungs, { kind = "broadcast", host = a .. "." .. b .. "." .. c .. ".255" })
        else
            table.insert(rungs, { kind = "broadcast", host = "255.255.255.255" })
        end
    else
        table.insert(rungs, { kind = "broadcast", host = "255.255.255.255" })
    end
    if type(name) == "string" and name ~= "" and not name:match("^%d+%.%d+%.%d+%.%d+$") then
        -- Bare first, because a router that registers the short name usually
        -- also answers it; the suffixes are for the ones that need them.
        for _, suffix in ipairs({ "", ".lan", ".home.arpa" }) do
            table.insert(rungs, { kind = "dns", host = name .. suffix })
        end
    end
    return rungs
end

-- What `endpoint.lua` returned, normalised. Absent, unreadable and empty are
-- all the same answer, and the answer is a table — never a nil the caller has
-- to test for.
function ReadingBuddy.parseEndpoints(raw)
    if type(raw) ~= "table" then
        return {}
    end
    local out = {}
    for device_id, entry in pairs(raw) do
        if type(device_id) == "string" and type(entry) == "table"
            and type(entry.host) == "string" and entry.host ~= "" then
            out[device_id] = { host = entry.host, port = tonumber(entry.port), seen_at = tonumber(entry.seen_at) }
        end
    end
    return out
end

-- The source text of an `endpoint.lua`. Written by us, read by us, and never
-- hashed — see `UNHASHED_FILES` in `crates/engine/src/plugin.rs` for why the
-- installer has to know about this file even though it never writes one.
function ReadingBuddy.serialiseEndpoints(endpoints)
    local lines = {
        "-- Written by the readingbuddy plugin on this device.",
        "-- Where each paired computer was last actually reached. Safe to delete:",
        "-- it is a cache, and discovery rebuilds it.",
        "return {",
    }
    -- Sorted, so a file rewritten with the same content has the same bytes and
    -- a person diffing two readers sees only what differs.
    local ids = {}
    for id in pairs(endpoints) do
        table.insert(ids, id)
    end
    table.sort(ids)
    for _, id in ipairs(ids) do
        local e = endpoints[id]
        table.insert(lines, string.format(
            "    [%q] = { host = %q, port = %d, seen_at = %d },",
            id, e.host, e.port or 0, e.seen_at or 0))
    end
    table.insert(lines, "}")
    table.insert(lines, "")
    return table.concat(lines, "\n")
end

-- `self.path` is set by `pluginloader.lua` to this directory. Our own state
-- lives inside it rather than in `DataStorage:getSettingsDir()` where kosync
-- keeps its credentials: readingbuddy promises that uninstalling is exact —
-- one directory removed, nothing left behind — and a token in the settings
-- directory would quietly break that promise.
function ReadingBuddy:readPairings()
    local ok, raw = pcall(dofile, self.path .. "/pairing.lua")
    if not ok then
        return {}
    end
    return ReadingBuddy.normalisePairings(raw)
end

function ReadingBuddy:readEndpoints()
    local ok, raw = pcall(dofile, self.path .. "/endpoint.lua")
    if not ok then
        return {}
    end
    return ReadingBuddy.parseEndpoints(raw)
end

-- Remember where a computer actually answered.
--
-- Best-effort by design: a read-only filesystem, a full device or a `.sdr` the
-- user has chmod'ed is not a reason to fail a push that worked. The cost of
-- losing this file is one extra broadcast next time.
function ReadingBuddy:rememberEndpoint(device_id, host, port)
    local endpoints = self:readEndpoints()
    endpoints[device_id] = { host = host, port = port, seen_at = os.time() }
    local f = io.open(self.path .. "/endpoint.lua", "w")
    if not f then
        logger.dbg("readingbuddy: could not cache the endpoint")
        return
    end
    f:write(ReadingBuddy.serialiseEndpoints(endpoints))
    f:close()
end

function ReadingBuddy:init()
    self.pairings = self:readPairings()
    logger.dbg("readingbuddy: paired with", #self.pairings, "computer(s)")
    self.ui.menu:registerToMainMenu(self)
end

-- ---- the wire -------------------------------------------------------------

local function hmac(token, message)
    local sha = require("ffi/sha2")
    -- `sha.hmac(hash_func, key, message)` — the hash *function* first, and the
    -- key is the token's hex characters. Returns lowercase hex.
    return sha.hmac(sha.sha256, token, message)
end

local function nonce()
    -- Not cryptographic randomness, and it does not need to be: the nonce is a
    -- freshness token the far side records, never a secret. What must not
    -- repeat within one open door is the pair, and the clock plus a counter
    -- gives that on a device with one process.
    ReadingBuddy._nonce_seq = (ReadingBuddy._nonce_seq or 0) + 1
    return string.format("%d-%d", os.time(), ReadingBuddy._nonce_seq)
end

-- ---- the Kindle's firewall -------------------------------------------------
--
-- **A Kindle drops the answer to a broadcast probe, and this is not a guess.**
-- Its INPUT policy is DROP, with a per-interface conntrack accept:
--
--     -P INPUT DROP
--     -A INPUT -i wlan0 -p udp -m state --state ESTABLISHED -j ACCEPT
--
-- A datagram sent to 255.255.255.255 makes a conntrack entry whose reply tuple
-- is *from* 255.255.255.255, so a unicast `HERE` from one computer does not
-- match it, arrives as NEW, and meets the policy. A unicast probe's answer does
-- match, and passes. Measured on the device, the two rungs seconds apart: the
-- broadcast timed out, the unicast was answered, and the desktop's own log
-- showed it had replied to *both*.
--
-- So this is not the pull-only problem item 15b assumed it was. Discovery is a
-- UDP request/**reply**, the reply is inbound, and push needs the hole first.
--
-- `SSH.koplugin` and `httpinspector.koplugin` both punch exactly this, and the
-- rules below are theirs. Two rather than one: this device has `-P OUTPUT
-- ACCEPT`, so the second is inert here, but it is what plugins known to work on
-- Kindle firmware do, and this is not the place to be inventive.

-- The rule, as a string. **Pure**, so a test can assert the exact command with
-- no Kindle to run it on — and `nil` for anything that is not a port, which is
-- what keeps a nil from becoming `--dport nil` on somebody's device.
function ReadingBuddy.firewallRule(action, chain, proto, port)
    port = tonumber(port)
    if not port or port <= 0 or port > 65535 then return nil end
    if action ~= "A" and action ~= "D" then return nil end
    if proto ~= "tcp" and proto ~= "udp" then return nil end
    if chain == "INPUT" then
        return string.format(
            "iptables -%s INPUT -p %s --dport %d -m conntrack --ctstate NEW,ESTABLISHED -j ACCEPT",
            action, proto, port)
    elseif chain == "OUTPUT" then
        return string.format(
            "iptables -%s OUTPUT -p %s --sport %d -m conntrack --ctstate ESTABLISHED -j ACCEPT",
            action, proto, port)
    end
    return nil
end

-- Open or close a hole, on a Kindle and nowhere else.
--
-- Every other device KOReader runs on either has nothing in the way or is not
-- ours to reconfigure, which is the line `SSH.koplugin` draws too.
local function firewall(action, proto, port)
    local Device = require("device")
    if not Device.isKindle or not Device:isKindle() then
        return
    end
    for _, chain in ipairs({ "INPUT", "OUTPUT" }) do
        local rule = ReadingBuddy.firewallRule(action, chain, proto, port)
        if rule then
            os.execute(rule)
        end
    end
end

-- Rung 1/3/4 of the ladder: ask one address and wait for a `HERE`.
--
-- The shape is `plugins/calibre.koplugin/wireless.lua`'s `find_calibre_server`
-- — `socket.udp4()`, `setoption("broadcast")`, `setsockname("*", port)`,
-- `sendto`, `receivefrom`, a short timeout — because that is code known to work
-- on these devices and inventing a second dialect of it would be guessing.
--
-- **The reply is verified before it is believed.** A rogue on a café LAN that
-- answers first cannot make this reader send a highlight: the MAC covers the
-- nonce *and the announced port*, so a captured reply cannot even be
-- re-advertised pointing somewhere else.
local function ask(udp, entry, host)
    local json = require("json")
    local n = nonce()
    local hello = json.encode({ v = PROTOCOL_VERSION, device_id = entry.device_id, nonce = n })
    local _, err = udp:sendto(hello, host, RENDEZVOUS_PORT)
    if err then
        logger.dbg("readingbuddy: could not send to", host, err)
        return nil
    end
    -- More than one answer is ordinary on a broadcast: other readingbuddys are
    -- entitled to be on the LAN. Keep reading until one proves the token.
    for _ = 1, 4 do
        local dgram, from = udp:receivefrom()
        if not dgram then
            -- `from` is the reason: "timeout" is the ordinary one, and it is
            -- worth a line, because the difference between *nothing answered*
            -- and *something answered wrongly* is the whole of a support
            -- conversation about a reader that will not push.
            logger.dbg("readingbuddy: no answer from", host, tostring(from))
            return nil
        end
        local ok, here = pcall(json.decode, dgram)
        if ok and type(here) == "table" and here.v == PROTOCOL_VERSION
            and type(here.tcp_port) == "number" and type(here.mac) == "string" then
            local want = hmac(entry.token,
                string.format("here:%d:%s:%d", PROTOCOL_VERSION, n, here.tcp_port))
            if want == here.mac then
                logger.dbg("readingbuddy: answered by", tostring(from), "port", here.tcp_port)
                return { host = from, port = here.tcp_port, name = here.name, nonce = n }
            end
            logger.dbg("readingbuddy: an answer from", tostring(from), "did not prove the token")
        end
    end
    return nil
end

local function probe(entry, host, timeout)
    local socket = require("socket")
    local udp = socket.udp4()
    if not udp then
        return nil
    end
    udp:setoption("broadcast", true)
    udp:setsockname("*", 0)
    udp:settimeout(timeout or 2)
    -- **The hole is for this socket's own port**, read back from the kernel
    -- rather than fixed. Fixed would be simpler and is wrong twice: it would
    -- collide with the responder an open pull window binds to the rendezvous
    -- port, and a rule is worth the narrowest port it can name. The ask is
    -- wrapped rather than inlined so that the hole is closed on *every* exit —
    -- a rule left behind is a permanently open port on somebody's reader.
    local _, myport = udp:getsockname()
    firewall("A", "udp", myport)
    local found = ask(udp, entry, host)
    firewall("D", "udp", myport)
    udp:close()
    return found
end

-- Walk the ladder. First proven answer wins.
--
-- **A `dns` rung is resolved here, and skipping that made it dead code.**
-- `udp:sendto` takes an address and not a name — measured on a Kindle, where
-- `sendto(msg, "arch-fongo", …)` answers `nil, "Name or service not known"`
-- while `socket.dns.toip("arch-fongo")` answers `192.168.1.63` on the same
-- resolver a second later. So the three hostname rungs built a probe that never
-- left the device: the rung the spec calls *the best one for a roaming laptop*,
-- silently doing nothing since it was written, and looking correct in a ladder
-- test that only ever asserted the rungs' order.
--
-- It is resolved *here* rather than inside `probe` because this is where a
-- rung's kind is known; `probe` stays about the wire and takes an address.
local function discover(entry, cached)
    local socket = require("socket")
    for _, rung in ipairs(ReadingBuddy.ladder(cached, entry.name)) do
        local host = rung.host
        if rung.kind == "dns" then
            local ip, err = socket.dns.toip(host)
            if ip then
                logger.dbg("readingbuddy:", host, "resolves to", ip)
            else
                -- Ordinary: most routers register a DHCP hostname and some do
                -- not, which is exactly why this is a rung and not the design.
                logger.dbg("readingbuddy: could not resolve", host, tostring(err))
            end
            host = ip
        end
        -- A cached address is tried by a *probe* rather than a bare connect, so
        -- rung 1 proves the token exactly as the others do — otherwise the
        -- cheapest rung would be the only one that trusts an address.
        local found = host and probe(entry, host, rung.kind == "cached" and 1 or 2)
        if found then
            return found
        end
    end
    return nil
end

-- Send one session: open, one frame per sidecar, done.
local function send(entry, found, sidecars)
    local socket = require("socket")
    local json = require("json")
    local sha = require("ffi/sha2")

    local tcp = socket.tcp()
    tcp:settimeout(10)
    local ok = tcp:connect(found.host, found.port)
    if not ok then
        return nil, _("Could not connect.")
    end

    local function line(tbl)
        return tcp:send(json.encode(tbl) .. "\n")
    end
    local function ack()
        local raw = tcp:receive("*l")
        if not raw then
            return nil
        end
        local parsed, got = pcall(json.decode, raw)
        return parsed and got or nil
    end

    line({
        v = PROTOCOL_VERSION,
        device_id = entry.device_id,
        nonce = found.nonce,
        mac = hmac(entry.token, string.format("open:%d:%s", PROTOCOL_VERSION, found.nonce)),
    })
    local reply = ack()
    if not reply or not reply.ok then
        tcp:close()
        return nil, (reply and reply.error) or _("The computer refused.")
    end

    local sent, failed = 0, 0
    for _, sc in ipairs(sidecars) do
        local body = sc.body
        -- `sha256` returns hex, so it goes into the challenge as-is — the same
        -- string the far side hashes the bytes into.
        local digest = sha.sha256(body)
        line({
            kind = "entry",
            name = sc.name,
            len = #body,
            sha256 = digest,
            mac = hmac(entry.token,
                string.format("body:%d:%s:%s", PROTOCOL_VERSION, found.nonce, digest)),
        })
        tcp:send(body)
        local said = ack()
        if said and said.ok then
            sent = sent + 1
        else
            -- One unreadable sidecar does not end a session carrying good
            -- ones; the far side says so and keeps reading.
            failed = failed + 1
        end
    end
    line({ kind = "done" })
    ack()
    tcp:close()
    return { sent = sent, failed = failed, host = found.host, port = found.port }
end

-- Every sidecar this device holds, as name-and-body pairs.
--
-- The payload is **the sidecar bytes the reader already has**, not a bespoke
-- delta. readingbuddy parses that format already, the parse is fuzzed, the
-- import is idempotent and the goldens cover it — so wireless is a transport
-- for an import that exists rather than a second import whose first divergence
-- shows up as highlights landing differently depending on which cable they came
-- down.
-- **There is no API that enumerates a device's sidecars**, which is the one
-- thing the design assumed and KOReader does not offer. `DocSettings` answers
-- *where is this document's sidecar* (`findSidecarFile(doc_path)`) and nothing
-- wider; the only whole-device list is `findSidecarFilesInHashLocation()`,
-- which covers one of the three sidecar layouts.
--
-- So the source of books is **`readhistory`**, which is what
-- `plugins/exporter.koplugin/clip.lua:390` uses for the same job — the honest
-- reading of *what this reader has* is what it has opened. The cost is
-- explicit: a book whose history entry has been cleared is not pushed until it
-- is opened again, and pushing over a cable remains the way to get everything.
function ReadingBuddy:collectSidecars()
    local DocSettings = require("docsettings")
    local out = {}
    local seen = {}
    for _, item in ipairs(require("readhistory").hist) do
        local sidecar = item.file and DocSettings:findSidecarFile(item.file)
        if sidecar and not seen[sidecar] then
            seen[sidecar] = true
            local f = io.open(sidecar, "r")
            if f then
                local body = f:read("*a")
                f:close()
                if body and body ~= "" then
                    -- The last two path segments: enough for a person to
                    -- recognise the file in a warning, and not the reader's
                    -- whole directory layout. It is a label, never a handle —
                    -- nothing on the far side opens it.
                    table.insert(out, {
                        name = sidecar:match("([^/]+/[^/]+)$") or sidecar,
                        body = body,
                    })
                end
            end
        end
    end
    return out
end

function ReadingBuddy:statusText()
    if #self.pairings == 0 then
        -- The plugin is installed but `pairing.lua` is absent, unreadable or
        -- names nobody. Reinstalling from readingbuddy is the whole repair, so
        -- say that rather than reporting a fault the reader cannot act on.
        return _("This reader is not paired.\n\nReinstall the plugin from readingbuddy on your computer to pair it.")
    end
    local names = {}
    for _, entry in ipairs(self.pairings) do
        table.insert(names, ReadingBuddy.computerName(entry))
    end
    -- `%1` is KOReader's own interpolation, applied by `ffi/util.template`.
    -- `string.format` does not understand it.
    return T(_("Paired with readingbuddy on %1.\n\nOpen the door there, then choose Push."),
        table.concat(names, ", "))
end

-- Push to one computer.
--
-- **Off the UI thread, and off it for the whole of it.** Discovery is up to
-- four rungs at a second or two each and the transfer is however long a library
-- takes, so doing any of it inline freezes e-ink for seconds. `Trapper` wraps
-- the lot in a dismissable subprocess, which is what `httpinspector` and the
-- calibre plugin both do; the subprocess returns a small result and *this* side
-- writes `endpoint.lua`, so nothing but the parent touches our directory.
--
-- **Wifi is asked for, never assumed.** `NetworkMgr:runWhenOnline` prompts or
-- fails according to the user's own setting, which is the only correct place
-- for that decision — a plugin that turned the radio on unasked would be making
-- a power decision on somebody's e-reader.
function ReadingBuddy:push(entry)
    local NetworkMgr = require("ui/network/manager")
    local Trapper = require("ui/trapper")
    NetworkMgr:runWhenOnline(function()
        Trapper:wrap(function()
            local cached = self:readEndpoints()[entry.device_id]
            local sidecars = self:collectSidecars()
            if #sidecars == 0 then
                UIManager:show(InfoMessage:new{ text = _("There is nothing on this reader to send yet.") })
                return
            end
            local completed, result = Trapper:dismissableRunInSubprocess(function()
                local found = discover(entry, cached)
                if not found then
                    return { error = "notfound" }
                end
                local sent, err = send(entry, found, sidecars)
                if not sent then
                    return { error = err or "failed" }
                end
                return sent
            end, T(_("Looking for %1…"), ReadingBuddy.computerName(entry)))

            if not completed then
                return -- the user dismissed it; say nothing.
            end
            if type(result) ~= "table" or result.error then
                -- **Fails closed and says which way.** "No readingbuddy
                -- answered" is a different problem from "it refused", and only
                -- the first one has an action the reader can take.
                local text = _("Could not reach readingbuddy.\n\nIs the door open on your computer?")
                if result and result.error and result.error ~= "notfound" then
                    text = T(_("readingbuddy refused: %1"), tostring(result.error))
                end
                UIManager:show(InfoMessage:new{ text = text })
                return
            end
            -- The device learns the current address as a side effect of the
            -- push working, which is the refresh that costs nothing and keeps
            -- rung 1 useful for a laptop that moves.
            self:rememberEndpoint(entry.device_id, result.host, result.port)
            UIManager:show(InfoMessage:new{
                text = T(_("Sent %1 to %2."), result.sent, ReadingBuddy.computerName(entry)),
            })
        end)
    end)
end

-- ---- the reader's window: pull (stage 3) -----------------------------------
--
-- The mirror of the desktop's door. The user opens it deliberately, it closes
-- itself, and while it is shut there is nothing on the network to find.
--
-- **`SimpleTCPServer` has no run loop.** It exposes `start`, `stop`, `send` and
-- a `waitEvent` that does exactly one non-blocking `accept`; `UIManager` polls
-- it because `insertZMQ` puts it in the list the main loop drains each cycle,
-- which is how `httpinspector` runs the same class. That is also why this
-- window must be short and must close itself: once a client connects,
-- `waitEvent` reads header lines **in line** with a 100 ms socket timeout and
-- then holds 500 ms for the response, so a stalled peer stalls the UI thread
-- for up to six tenths of a second per connection. Nothing here may be left
-- open "just in case".
--
-- The UDP responder is polled from the same place, for the same reason the
-- desktop's dies with its TCP listener: an announcer that outlives the thing it
-- announces sends a computer to a closed port.
local WINDOW_SECONDS = 120

-- A `waitEvent`-shaped object `UIManager` can poll, wrapping the rendezvous
-- socket. It answers a `HELLO` from a computer we are paired with and is silent
-- to everything else — a probe from a stranger learns nothing, which is what
-- makes "there is nothing to find" true rather than merely intended.
local function makeResponder(entries, tcp_port)
    local socket = require("socket")
    local json = require("json")
    local udp = socket.udp4()
    if not udp then
        return nil
    end
    udp:setoption("broadcast", true)
    if not udp:setsockname("*", RENDEZVOUS_PORT) then
        -- Another program holds the port. Fail closed and silently: the window
        -- simply cannot be found, which is a state the desktop already words.
        udp:close()
        return nil
    end
    -- Zero, not a small number: this is polled from the UI loop, and any
    -- blocking read at all is a stutter on every frame.
    udp:settimeout(0)
    return {
        socket = udp,
        stop = function(self) self.socket:close() end,
        waitEvent = function(self)
            local dgram, host, port = self.socket:receivefrom()
            if not dgram then
                return
            end
            local ok, hello = pcall(json.decode, dgram)
            if not ok or type(hello) ~= "table" or hello.v ~= PROTOCOL_VERSION then
                return
            end
            local entry = entries[hello.device_id]
            if not entry or type(hello.nonce) ~= "string" then
                return
            end
            self.socket:sendto(json.encode({
                v = PROTOCOL_VERSION,
                name = entry.reader_name,
                tcp_port = tcp_port,
                mac = hmac(entry.token,
                    string.format("here:%d:%s:%d", PROTOCOL_VERSION, hello.nonce, tcp_port)),
            }), host, port)
        end,
    }
end

function ReadingBuddy:windowOpen()
    return self.window_server ~= nil
end

-- Serve one pulled session. The desktop opened it, so it sends `OPEN` and we
-- send the entries — the same frames as a push with the dial reversed, which is
-- why nothing about the payload or its MACs changes.
function ReadingBuddy:onPullRequest(data, client)
    local json = require("json")
    local sha = require("ffi/sha2")
    local server = self.window_server

    local function line(tbl)
        client:send(json.encode(tbl) .. "\n")
    end

    local open = nil
    local ok, parsed = pcall(json.decode, (data:gsub("\r\n.*$", "")))
    if ok then open = parsed end
    if type(open) ~= "table" or open.v ~= PROTOCOL_VERSION then
        line({ ok = false, error = "bad request" })
        return server:send("", client)
    end
    local entry = self.pairings_by_id[open.device_id]
    if not entry
        or type(open.nonce) ~= "string"
        or hmac(entry.token, string.format("open:%d:%s", PROTOCOL_VERSION, open.nonce)) ~= open.mac then
        -- No detail. Which byte differed is exactly what an attacker wants.
        line({ ok = false, error = "refused" })
        return server:send("", client)
    end
    line({ ok = true })

    for _, sc in ipairs(self:collectSidecars()) do
        local digest = sha.sha256(sc.body)
        line({
            kind = "entry",
            name = sc.name,
            len = #sc.body,
            sha256 = digest,
            mac = hmac(entry.token,
                string.format("body:%d:%s:%s", PROTOCOL_VERSION, open.nonce, digest)),
        })
        client:send(sc.body)
        -- The far side acks each entry. We do not branch on it: one sidecar it
        -- could not parse is its business and must not stop the rest, which is
        -- the same rule the desktop applies in the other direction.
        client:receive("*l")
    end
    line({ kind = "done" })
    client:receive("*l")
    -- A completed session closes the window, exactly as a completed push closes
    -- the desktop's door: the user asked for one transfer, not for a service.
    UIManager:nextTick(function() self:closeWindow("done") end)
    return server:send("", client)
end

function ReadingBuddy:openWindow()
    if self:windowOpen() then
        return true
    end
    self.pairings_by_id = {}
    for _, entry in ipairs(self.pairings) do
        entry.reader_name = entry.reader_name or "reader"
        self.pairings_by_id[entry.device_id] = entry
    end

    local ServerClass = require("ui/message/simpletcpserver")
    local server = ServerClass:new{
        host = "*",
        -- Port 0 asks the kernel for a free one, which is announced in every
        -- `HERE`. Nothing on this side is well-known except the UDP port.
        port = 0,
        receiveCallback = function(data, client) return self:onPullRequest(data, client) end,
    }
    local started, err = server:start()
    if not started then
        UIManager:show(InfoMessage:new{ text = T(_("Could not open the window: %1"), tostring(err)) })
        return false
    end
    local _, port = server.server:getsockname()
    port = tonumber(port)

    local responder = makeResponder(self.pairings_by_id, port)
    if not responder then
        server:stop()
        UIManager:show(InfoMessage:new{
            text = _("Could not open the window: something else is using readingbuddy's port."),
        })
        return false
    end

    -- **Both halves of the window need a hole, and for different reasons.** The
    -- desktop's `HELLO` is unsolicited, so no conntrack entry can exist for it
    -- and the responder never hears a thing; the dial that follows is an
    -- inbound TCP connection, which is `httpinspector`'s own case exactly. Push
    -- needed one rule for a moment; a window needs two for as long as it is
    -- open, and `closeWindow` is what takes them away again.
    firewall("A", "udp", RENDEZVOUS_PORT)
    firewall("A", "tcp", port)
    self.window_port = port

    self.window_server = server
    self.window_zmq = UIManager:insertZMQ(server)
    self.window_responder = responder
    self.window_responder_zmq = UIManager:insertZMQ(responder)
    -- The window closes itself. `scheduleIn` rather than a deadline checked on
    -- each poll, so a device that goes to sleep and wakes has already been
    -- closed by the standby hook rather than reopening on a stale clock.
    self.window_timer = function() self:closeWindow("timeout") end
    UIManager:scheduleIn(WINDOW_SECONDS, self.window_timer)
    return true
end

-- Close it. Idempotent, because four separate hooks call it and a device can
-- reach two of them in a row.
function ReadingBuddy:closeWindow(why)
    if not self:windowOpen() then
        return
    end
    logger.dbg("readingbuddy: closing the window,", why)
    if self.window_timer then
        UIManager:unschedule(self.window_timer)
        self.window_timer = nil
    end
    -- The responder goes first: an announcer that outlives its listener sends a
    -- computer to a closed port.
    if self.window_responder then
        UIManager:removeZMQ(self.window_responder_zmq)
        self.window_responder:stop()
        self.window_responder, self.window_responder_zmq = nil, nil
    end
    UIManager:removeZMQ(self.window_zmq)
    self.window_server:stop()
    self.window_server, self.window_zmq = nil, nil
    -- The holes go last, after the sockets they were for are gone: the reverse
    -- of the order they were opened in, so there is no instant where a rule
    -- stands with nothing behind it. Four hooks reach this function and a device
    -- can hit two in a row, which is why the port is cleared as it is used.
    if self.window_port then
        firewall("D", "tcp", self.window_port)
        self.window_port = nil
    end
    firewall("D", "udp", RENDEZVOUS_PORT)
end

-- **Four hooks, not one.** The spec named `onEnterStandby`; `httpinspector`
-- handles all four and it is right — a device suspends, it also exits, and a
-- widget closing under an open socket leaks it. A window left open across a
-- suspend is a listening port on a device in somebody's bag.
function ReadingBuddy:onEnterStandby() self:closeWindow("standby") end
function ReadingBuddy:onSuspend() self:closeWindow("suspend") end
function ReadingBuddy:onExit() self:closeWindow("exit") end
function ReadingBuddy:onCloseWidget() self:closeWindow("close") end

function ReadingBuddy:addToMainMenu(menu_items)
    -- `sorting_hint = "tools"` must name a menu id that exists, or
    -- `menusorter.lua` indexes a nil and KOReader crashes on startup. "tools"
    -- resolves in both the file-manager and the reader order tables.
    local sub = {
        {
            text = _("About this pairing"),
            keep_menu_open = true,
            callback = function()
                UIManager:show(InfoMessage:new{ text = self:statusText() })
            end,
        },
    }
    -- One entry per paired computer, named. The single-computer file could not
    -- express the choice at all, which is why stage 1 had to come first.
    --
    -- **The index is named, and must stay named.** `for _, entry` is the idiom
    -- everywhere else in this file and is wrong on exactly this loop: `_` is
    -- gettext, bound at the top of the module, and the loop body is the one
    -- place that calls it. Shadowing it makes the next line `(a number)(...)`,
    -- which throws while KOReader is *building* the menu — so the failure is
    -- not a broken entry, it is **no readingbuddy entry in Tools at all**, on a
    -- reader that is paired and looks fine otherwise. `_idx` goes unused, which
    -- Lua is content with; assigning `_ = i` to silence that would *also* be
    -- the bug, since `_` here is the module's upvalue and the assignment would
    -- replace gettext with a number for the rest of the process.
    for _idx, entry in ipairs(self.pairings) do
        table.insert(sub, {
            text = T(_("Push to %1"), ReadingBuddy.computerName(entry)),
            keep_menu_open = true,
            callback = function()
                self:push(entry)
            end,
        })
    end
    -- The other direction: let the computer come and get it. One entry, not one
    -- per computer — the window is open to every paired computer at once, and
    -- which of them dials is the desktop's choice rather than the reader's.
    if #self.pairings > 0 then
        table.insert(sub, {
            text_func = function()
                return self:windowOpen() and _("Close the window") or _("Open the window for a pull")
            end,
            keep_menu_open = true,
            callback = function()
                if self:windowOpen() then
                    self:closeWindow("menu")
                    UIManager:show(InfoMessage:new{ text = _("The window is closed.") })
                    return
                end
                -- Wifi must already be up. `runWhenOnline` would prompt for the
                -- radio and then open a window the user walks away from, which
                -- is the opposite of a door you hold open on purpose.
                local NetworkMgr = require("ui/network/manager")
                if not NetworkMgr:isOnline() then
                    UIManager:show(InfoMessage:new{
                        text = _("Turn wifi on first, then open the window."),
                    })
                    return
                end
                if self:openWindow() then
                    UIManager:show(InfoMessage:new{
                        text = _("The window is open for two minutes. Choose Pull on your computer.\n\nIt closes itself, and when the reader sleeps."),
                    })
                end
            end,
        })
    end
    menu_items.readingbuddy = {
        text = _("readingbuddy"),
        sorting_hint = "tools",
        sub_item_table = sub,
    }
end

return ReadingBuddy
