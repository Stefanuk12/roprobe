-- Dependencies
local Broker = require("@contracts/Broker")
local Signal = require("@pkgs/signal")
local Trove = require("@pkgs/trove")

-- Constants
local SCRIPT_NAME = "roprobe"

-- Types
type LogLevel = Broker.LogLevel
type Restore = {
    env: any,
    print: (...any) -> ...any,
    warn: (...any) -> ...any,
    error: (message: any, level: number?) -> ...any,
}

--- Reconstruct any arguments the same way `print` does.
local function joinArguments(...): string
    local count = select("#", ...)
    local parts = table.create(count)
    for index = 1, count do
        parts[index] = tostring((select(index, ...)))
    end
    return table.concat(parts, " ")
end

--- Shape an error the way Roblox's console does.
local function formatError(message: any, blame: number): string
    local content = tostring(message)

    -- `blame` counts frames from `error`, which is one shallower than here.
    if blame > 0 then
        local source: string?, line: number? = debug.info(blame + 1, "sl")
        if line then
            local name = source
            if name == nil or name == "" then
                name = SCRIPT_NAME
            end
            content = `{name}:{line}: {content}`
        end
    end

    -- Correct the script name in the stack trace.
    -- Our hook adds an extra level.
    local frames = (debug.traceback(nil, 3):gsub("%s+$", ""))
    local named = (frames:gsub("Script ''", `Script '{SCRIPT_NAME}'`))
    return `{content}\nStack Begin\n{named}\nStack End`
end

--- Hook the executor's `print`/`warn`/`error` to report to an event rather than console.
local OutputHook = {}
OutputHook.__index = OutputHook

export type OutputHookData = {
    _trove: Trove.Trove,
    _restore: Restore?,

    onOutput: Signal.Signal<LogLevel, string>,
}
export type OutputHook = setmetatable<OutputHookData, typeof(OutputHook)>

--- Create a new [`OutputHook`] object.
function OutputHook.new(): OutputHook
    local trove = Trove.new()
    local self: OutputHookData = {
        _trove = trove,
        _restore = nil,

        onOutput = trove:Add(Signal.new()),
    }
    return setmetatable(self, OutputHook)
end

--- Revert the hook and clean up.
function OutputHook.Destroy(self: OutputHook)
    self:Revert()
    self._trove:Destroy()
end

--- Swap the globals for wrappers, doing nothing if already hooked.
function OutputHook.Initialise(self: OutputHook)
    if self._restore then
        return
    end

    local env = getgenv()
    local originalPrint = env.print
    local originalWarn = env.warn
    local originalError = env.error

    env.print = function(...)
        self.onOutput:Fire("print" :: LogLevel, joinArguments(...))
        return originalPrint(...)
    end

    env.warn = function(...)
        self.onOutput:Fire("warn" :: LogLevel, joinArguments(...))
        return originalWarn(...)
    end

    env.error = function(message: any, level: number?)
        local blame = 2
        if level then
            blame = level
            if level > 0 then
                blame = level + 1
            end
        end

        self.onOutput:Fire("error" :: LogLevel, formatError(message, blame))
        return originalError(message, blame)
    end

    self._restore = {
        env = env,
        print = originalPrint,
        warn = originalWarn,
        error = originalError,
    }
end

--- Put the original globals back, if hooked.
function OutputHook.Revert(self: OutputHook)
    local restore = self._restore
    if not restore then
        return
    end
    self._restore = nil

    local env = restore.env
    env.print = restore.print
    env.warn = restore.warn
    env.error = restore.error
end

return OutputHook
