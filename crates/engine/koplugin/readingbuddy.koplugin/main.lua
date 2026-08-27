-- readingbuddy's KOReader plugin.
--
-- This slice (item 15a) establishes the *link* and nothing else: readingbuddy
-- installs this directory over USB and writes `pairing.lua` beside this file,
-- and the reader can say who it is paired with. There is no network code here —
-- not stubbed, not commented out — because there is no listener to talk to yet.
--
-- `pairing.lua` deliberately carries no host and no port. A missing endpoint is
-- *not configured*, and the plugin then does nothing at all, which is
-- `docs/decisions.md`'s "fails closed" in its degenerate case. When item 15b
-- gives it an address, the rule stays: cannot reach the app → do nothing,
-- silently, and never block or slow the reader's UI.

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

-- `self.path` is set by `pluginloader.lua` to this directory. Our own state
-- lives inside it rather than in `DataStorage:getSettingsDir()` where kosync
-- keeps its credentials: readingbuddy promises that uninstalling is exact —
-- one directory removed, nothing left behind — and a token in the settings
-- directory would quietly break that promise.
function ReadingBuddy:readPairing()
    local ok, pairing = pcall(dofile, self.path .. "/pairing.lua")
    if not ok or type(pairing) ~= "table" then
        return nil
    end
    return pairing
end

function ReadingBuddy:init()
    self.pairing = self:readPairing()
    if self.pairing then
        logger.dbg("readingbuddy: paired as", self.pairing.device_id)
    else
        logger.dbg("readingbuddy: not paired")
    end
    self.ui.menu:registerToMainMenu(self)
end

function ReadingBuddy:statusText()
    if not self.pairing then
        -- The plugin is installed but `pairing.lua` is absent or unreadable.
        -- Reinstalling from readingbuddy is the whole repair, so say that
        -- rather than reporting a fault the reader cannot act on.
        return _("This reader is not paired.\n\nReinstall the plugin from readingbuddy on your computer to pair it.")
    end
    local id = tostring(self.pairing.device_id or "?"):sub(1, 8)
    -- `%1` is KOReader's own interpolation, applied by `ffi/util.template`.
    -- `string.format` does not understand it.
    return T(_("Paired with readingbuddy as %1.\n\nThis reader has no address for it yet, so nothing is sent."), id)
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
