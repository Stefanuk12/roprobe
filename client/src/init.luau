-- Dependencies
local Broker = require("@features/broker")
local globalTrove = require("@engine/globalTrove")

-- Vars
-- The broker owns the upstream (Verde / luau-lsp) connections; this client just links in, mirrors the DataModel and selects which upstreams to enable.
local config: Broker.Config = {
    host = "192.168.0.11",
    verde = true,
    luauLsp = false,
    lazy = true,
}

-- Connect to the broker and start streaming the tree.
local broker = globalTrove:Add(Broker.new(config))
broker:Start()
