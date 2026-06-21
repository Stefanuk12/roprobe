-- Dependencies
local InstanceTracker = require("@luau_lsp/InstanceTracker")
local Log = require("@luau_lsp/Utils/Log")
local Manager = require("@modules/luau_lsp/Manager")
local Shims = require("@modules/luau_lsp/shims")
local Trove = require("@pkgs/trove")

-- Types
type DefaultSettings = {
    host: string,
    port: number,
    include: { Instance },
}
export type LuauLSPConfig = {
    --- Language server host, defaults to `http://localhost`.
    host: string?,
    --- Language server port, defaults to `3667`.
    port: number?,
    --- Instances whose tree is tracked, defaults to the plugin's service list.
    include: { Instance }?,
    --- Connect on [`LuauLSP.Start`], defaults to `true`.
    startAutomatically: boolean?,
    --- Seconds between (re)connect attempts.
    retryInterval: number?,
}
export type LuauLSPData = {
    _trove: Trove.Trove,
    _shims: Shims.Shims,
    _manager: Manager.Manager,
    _instanceTracker: InstanceTracker.InstanceTracker,
    _retryInterval: number,
    _startAutomatically: boolean,
    _reconnectThread: thread?,

    running: boolean,
}

local DEFAULTS: DefaultSettings = {
    host = "http://localhost",
    port = 3667,
    include = {
        game:GetService("Workspace"),
        game:GetService("Players"),
        game:GetService("Lighting"),
        game:GetService("ReplicatedFirst"),
        game:GetService("ReplicatedStorage"),
        game:GetService("StarterGui"),
        game:GetService("StarterPack"),
        game:GetService("StarterPlayer"),
        game:GetService("SoundService"),
        game:GetService("Chat"),
        game:GetService("LocalizationService"),
    },
}

--- Runs the Luau LSP plugin within an executor environment.
local LuauLSP = {}
LuauLSP.__index = LuauLSP
export type LuauLSP = setmetatable<LuauLSPData, typeof(LuauLSP)>

--- Create a new [`LuauLSP`] object.
function LuauLSP.new(config: LuauLSPConfig?): LuauLSP
    local resolved: LuauLSPConfig = config or {}

    local trove = Trove.new()
    local shims = trove:Add(Shims.new())
    shims:Apply()

    local include = resolved.include or DEFAULTS.include
    local instanceTracker = InstanceTracker.new(function(): { Instance }
        return include
    end)

    local manager = Manager.new(instanceTracker)
    manager:setAddress(resolved.host or DEFAULTS.host, resolved.port or DEFAULTS.port)
    manager:init()
    manager._silenceConnectionErrors = true

    local self: LuauLSPData = {
        _trove = trove,
        _shims = shims,
        _manager = manager,
        _instanceTracker = instanceTracker,
        _retryInterval = resolved.retryInterval or 1.0,
        _startAutomatically = resolved.startAutomatically ~= false,
        running = true,
    }
    return setmetatable(self, LuauLSP)
end

--- Begin driving the connection and kick off the first attempt.
function LuauLSP.Start(self: LuauLSP)
    if not self.running or not self._startAutomatically then
        return
    end

    if self._reconnectThread then
        return
    end

    self._reconnectThread = task.spawn(function()
        while true do
            if not self.running or self._manager:isConnected() then
                task.wait(self._retryInterval)
                continue
            end

            local ok, err = pcall(function()
                self._manager:connect()
            end)
            if not ok then
                Log.debug(`Reconnect attempt errored: {err}`)
            end
            task.wait(self._retryInterval)
        end
    end)

    self._trove:Add(function()
        if self._reconnectThread then
            task.cancel(self._reconnectThread)
            self._reconnectThread = nil
        end
    end)
end

--- Destroy this object and clean up.
function LuauLSP.Destroy(self: LuauLSP)
    self.running = false

    if self._reconnectThread then
        task.cancel(self._reconnectThread)
        self._reconnectThread = nil
    end

    pcall(function()
        self._manager:unload()
    end)

    self._trove:Destroy()
end

return LuauLSP
