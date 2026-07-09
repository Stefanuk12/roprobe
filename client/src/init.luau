-- Dependencies
local Broker = require("@features/broker")
local globalTrove = require("@engine/globalTrove")

-- Vars
-- The broker owns the upstream (Verde / luau-lsp) connections; this client only
-- links to it, mirrors the DataModel and says which upstreams it wants on.
-- Host/port/token come from `getgenv().roprobe_config` (or the broker lockfile).
local config: Broker.Config = {
    verde = true,
    luauLsp = false,
}

-- Connect to the broker and start streaming the tree.
local broker = globalTrove:Add(Broker.new(config))
broker:Start()
