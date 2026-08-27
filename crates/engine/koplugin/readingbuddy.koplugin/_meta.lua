-- readingbuddy's KOReader plugin — metadata.
--
-- **There is no `require` in this file, and that is deliberate.** Every plugin
-- shipped with KOReader opens its `_meta.lua` with
-- `local _ = require("gettext")`; this one must not, for ever.
--
-- readingbuddy reads the `version` below by evaluating this file in the
-- sandboxed Lua VM it already uses for sidecars (`koreader.rs`, `StdLib::NONE`,
-- an instruction budget, and no `require` — `fuzz/seeds/parse_sidecar/require.lua`
-- exists to prove it refuses one). A pure table literal is what makes "refuse
-- to overwrite a newer plugin than ours" cost no manifest format and no second
-- parser. `pluginloader.lua` merges every field here into the module except
-- `name` and gates on none of them, so `version` rides along inert.
return {
    fullname = "readingbuddy",
    description = "Links this reader to readingbuddy on your computer.",
    version = 1,
}
