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
-- No entry carries a host or a port. A missing endpoint is *not configured*,
-- and the plugin then does nothing at all, which is `docs/decisions.md`'s
-- "fails closed" in its degenerate case. When the endpoint arrives it lands in
-- `endpoint.lua` — written by *this* plugin, never by the installer, and never
-- hashed into `installed.lua` — and the rule stays: cannot reach the app → do
-- nothing, silently, and never block or slow the reader's UI.

local InfoMessage = require("ui/widget/infomessage")
local UIManager = require("ui/uimanager")
local WidgetContainer = require("ui/widget/container/widgetcontainer")
local logger = require("logger")
local T = require("ffi/util").template
local _ = require("gettext")

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

function ReadingBuddy:init()
    self.pairings = self:readPairings()
    logger.dbg("readingbuddy: paired with", #self.pairings, "computer(s)")
    self.ui.menu:registerToMainMenu(self)
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
    return T(_("Paired with readingbuddy on %1.\n\nThis reader has no address for it yet, so nothing is sent."),
        table.concat(names, ", "))
end

function ReadingBuddy:addToMainMenu(menu_items)
    menu_items.readingbuddy = {
        text = _("readingbuddy"),
        sorting_hint = "tools",
        keep_menu_open = true,
        callback = function()
            UIManager:show(InfoMessage:new{ text = self:statusText() })
        end,
    }
end

return ReadingBuddy
