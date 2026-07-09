-- Constants
local LOGGER_ENABLED = true
local PRINT_ENABLED = true
local LOG_LEVEL_ORDER = table.freeze({
    trace = 0,
    debug = 1,
    info = 2,
    warn = 3,
    error = 4,
}) :: { [LogLevel]: number }

--- Outputs all logs into a file/output.
local Logger = {}
Logger.__index = Logger

-- Types
export type LogLevel = "trace" | "debug" | "info" | "warn" | "error"
export type LoggerData = {
    _logLevel: number,
    _logFile: string,
}
export type Logger = setmetatable<LoggerData, typeof(Logger)>

--- Create the dated log file and return its path.
local function createLogFile(): string
    local now = DateTime.now()
    local date = now:FormatUniversalTime("DD-MM-YYYY", "en-gb")
    local time = now:FormatUniversalTime("HH-mm-ss", "en-gb")

    local rootFolder = "roprobe"
    local logsFolder = `{rootFolder}/logs`
    local folder = `{logsFolder}/{date}`
    local file = `{folder}/{time}.txt`

    if LOGGER_ENABLED then
        pcall(makefolder, rootFolder)
        pcall(makefolder, logsFolder)
        pcall(makefolder, folder)
        pcall(writefile, file, "")
        task.wait(0.2)
    end
    return file
end

--- Format one line: `[LEVEL, prefix: HH:mm:ss] arg1 arg2 ...`.
local function formatLine(level: LogLevel, prefix: string, ...): string
    local timestamp = DateTime.now():FormatUniversalTime("HH:mm:ss", "en-gb")
    local label = level:upper()
    if prefix ~= "" then
        label ..= `, {prefix}`
    end

    local count = select("#", ...)
    local parts = table.create(count)
    for index = 1, count do
        parts[index] = tostring((select(index, ...)))
    end
    return `[{label}: {timestamp}] {table.concat(parts, " ")}`
end

--- Create a new [`Logger`] writing to a fresh dated file.
function Logger.new(): Logger
    local self: LoggerData = {
        _logLevel = LOG_LEVEL_ORDER.trace,
        _logFile = createLogFile(),
    }
    return setmetatable(self, Logger)
end

--- Format and append a line to the file, returning it, or `nil` when the level
--- is below the threshold or every sink is disabled.
function Logger._append(self: Logger, level: LogLevel, prefix: string, ...): string?
    if LOG_LEVEL_ORDER[level] < self._logLevel then
        return nil
    end
    if not (LOGGER_ENABLED or PRINT_ENABLED) then
        return nil
    end

    local line = formatLine(level, prefix, ...)
    if LOGGER_ENABLED then
        pcall(appendfile, self._logFile, line .. "\n")
    end
    return line
end

--- Log at `trace` severity.
function Logger.Trace(self: Logger, prefix: string, ...)
    local line = self:_append("trace", prefix, ...)
    if line and PRINT_ENABLED then
        print(line)
    end
end

--- Log at `debug` severity.
function Logger.Debug(self: Logger, prefix: string, ...)
    local line = self:_append("debug", prefix, ...)
    if line and PRINT_ENABLED then
        print(line)
    end
end

--- Log at `info` severity.
function Logger.Info(self: Logger, prefix: string, ...)
    local line = self:_append("info", prefix, ...)
    if line and PRINT_ENABLED then
        print(line)
    end
end

--- Log at `warn` severity.
function Logger.Warn(self: Logger, prefix: string, ...)
    local line = self:_append("warn", prefix, ...)
    if line and PRINT_ENABLED then
        warn(line)
    end
end

--- Log at `error` severity.
function Logger.Error(self: Logger, prefix: string, ...)
    local line = self:_append("error", prefix, ...)
    if line and PRINT_ENABLED then
        warn(line)
    end
end

return Logger.new()
