-- Services
local RunService = cloneref(game:GetService("RunService"))

-- Dependencies
local Config = require("@verde/Shared/Config")
local Shims = require("@modules/verde/shims")
local StateController = require("@verde/States/StateController")
local States = require("@verde/States")
local Trove = require("@pkgs/trove")

-- Types
export type VerdeData = {
    _trove: Trove.Trove,
    _shims: Shims.Shims,
    _retryInterval: number,
    _reconnectThread: thread?,
    _wsUrl: string,

    running: boolean,
}

--- Drives the Verde state machine and the auto-(re)connect loop on an executor.
local Verde = {}
Verde.__index = Verde
export type Verde = setmetatable<VerdeData, typeof(Verde)>

--- Create a new [`Verde`] object.
function Verde.new(wsUrl: string?): Verde
    local trove = Trove.new()
    local shims = trove:Add(Shims.new())
    shims:Apply()

    local resolvedUrl = wsUrl or Config.WS_URL
    -- The Connecting state reads the endpoint from Config at connect time,
    -- so push the resolved URL there for it to pick up.
    Config.WS_URL = resolvedUrl

    local self: VerdeData = {
        _trove = trove,
        _shims = shims,
        _retryInterval = Config.RETRY_INTERVAL_SECONDS or 1.0,
        _wsUrl = resolvedUrl,
        running = true,
    }
    return setmetatable(self, Verde)
end

--- Destroy this object and clean up.
function Verde.Destroy(self: Verde)
    self.running = false

    if self._reconnectThread then
        task.cancel(self._reconnectThread)
        self._reconnectThread = nil
    end

    pcall(StateController.SwitchState, "Disconnected")
    pcall(StateController.Cleanup)
    pcall(function()
        self._shims.plugin.Unloading:Fire()
    end)

    self._trove:Destroy()
end

--- Schedule a single reconnect attempt after `retryInterval` seconds.
function Verde._scheduleReconnect(self: Verde)
    if self._reconnectThread then
        return
    end

    self._reconnectThread = task.delay(self._retryInterval, function()
        self._reconnectThread = nil
        if self.running and StateController.GetState() == "Disconnected" then
            StateController.SwitchState("Connecting")
        end
    end)
end

--- Begin driving the state machine and kick off the first connection.
function Verde.Start(self: Verde)
    -- Immediately cleanup StateController because its heartbeat is redudant with ours
    StateController.Cleanup()

    -- Constantly update state / reconnect, if needed
    self._trove:Add(RunService.Heartbeat:Connect(function(dt)
        if not self.running then
            return
        end

        local name = StateController.GetState()
        if not name then
            return
        end

        local state = (States :: any)[name]
        if state and state.onUpdate then
            (state.onUpdate :: any)(dt, StateController.SwitchState)
        end

        if name == "Disconnected" then
            self:_scheduleReconnect()
        end
    end))

    -- Kick off the connection.
    StateController.SwitchState("Connecting")
end

return Verde
