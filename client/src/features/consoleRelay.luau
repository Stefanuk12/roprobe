-- Dependencies
local Logger = require("@engine/Logger")
local Messages = require("@lib/Messages")
local OutputHook = require("@engine/hooks/OutputHook")
local Signal = require("@pkgs/signal")
local Trove = require("@pkgs/trove")

-- Services
local LogService = cloneref(game:GetService("LogService"))

-- Constants
local FLUSH_INTERVAL = 0.1
local MAX_PENDING = 512
local LEVEL_BY_MESSAGE_TYPE: { [Enum.MessageType]: Messages.LogLevel } = {
    [Enum.MessageType.MessageOutput] = "print",
    [Enum.MessageType.MessageInfo] = "info",
    [Enum.MessageType.MessageWarning] = "warn",
    [Enum.MessageType.MessageError] = "error",
}

-- Types
type LogEntry = Messages.LogEntry
type LogLevel = Messages.LogLevel

--- Batches the client's console output.
local ConsoleRelay = {}
ConsoleRelay.__index = ConsoleRelay

export type ConsoleRelayData = {
    _trove: Trove.Trove,
    _outputHook: OutputHook.OutputHook,
    _pending: { LogEntry },
    _dropped: number,
    _flushThread: thread?,
    _isFlushing: boolean,

    onBatch: Signal.Signal<{ LogEntry }>,
}
export type ConsoleRelay = setmetatable<ConsoleRelayData, typeof(ConsoleRelay)>

--- Create a new [`ConsoleRelay`]; nothing is captured until [`ConsoleRelay.Initialise`].
function ConsoleRelay.new(): ConsoleRelay
    local trove = Trove.new()
    local self: ConsoleRelayData = {
        _trove = trove,
        _outputHook = trove:Add(OutputHook.new()),
        _pending = {},
        _dropped = 0,
        _flushThread = nil,
        _isFlushing = false,

        onBatch = trove:Add(Signal.new()),
    }

    local relay = setmetatable(self, ConsoleRelay)
    trove:Add(function()
        local thread = relay._flushThread
        if thread then
            task.cancel(thread)
            relay._flushThread = nil
        end
    end)
    return relay
end

--- Destroy this object, and clean up.
function ConsoleRelay.Destroy(self: ConsoleRelay)
    self._trove:Destroy()
end

--- Start capturing console output.
function ConsoleRelay.Initialise(self: ConsoleRelay)
    -- `LogService` carries the game's output ONLY since we hook ours.
    self._trove:Add(LogService.MessageOut:Connect(function(message: string, messageType: Enum.MessageType)
        local level = LEVEL_BY_MESSAGE_TYPE[messageType]
        if level then
            self:_queue(level, message)
        end
    end))

    local hook = self._outputHook
    self._trove:Add(
        hook.onOutput:Connect(function(level: LogLevel, content: string)
            self:_queue(level, content)
        end),
        "Disconnect"
    )
    hook:Initialise()
    Logger:Debug("console", "capturing output")
end

--- Queue one line, scheduling the next flush.
function ConsoleRelay._queue(self: ConsoleRelay, level: LogLevel, content: string)
    -- A listener that logs while we fire would feed its own output straight back in.
    if self._isFlushing then
        return
    end

    local pending = self._pending
    if #pending >= MAX_PENDING then
        self._dropped += 1
        return
    end
    table.insert(pending, { level = level, content = content })

    if not self._flushThread then
        self._flushThread = task.delay(FLUSH_INTERVAL, function()
            self._flushThread = nil
            self:_flush()
        end)
    end
end

--- Hand the queued lines to the listeners, emptying the queue.
function ConsoleRelay._flush(self: ConsoleRelay)
    local pending = self._pending
    if #pending == 0 then
        return
    end
    self._pending = {}

    self._isFlushing = true

    local count = #pending
    local dropped = self._dropped
    self._dropped = 0
    Logger:Debug("console", `flushing {count} line(s), {dropped} dropped`)

    self.onBatch:Fire(pending)
    self._isFlushing = false
end

return ConsoleRelay
