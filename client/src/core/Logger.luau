-- Constants
local LOGGER_ENABLED = true
local PRINT_ENABLED = true

-- Vars
local LogLevelValue = table.freeze({
    trace = 0,
    debug = 1,
    info = 2,
    warn = 3,
    error = 4,
}) :: { [LogLevel]: number }

--- Handles logging stuff for debug purposes.
local Logger = {}
Logger.__index = Logger

-- Types
export type LogLevel = "trace" | "debug" | "info" | "warn" | "error"
export type LoggerData = {
    _logLevel: number,
    _logFile: string,
}
export type Logger = setmetatable<LoggerData, typeof(Logger)>

--- Create a new [`Logger`] object.
function Logger.new(): Logger
    -- Create the logging file
    local logFile = Logger.Initialise()

    -- Create and return the object
    local self: LoggerData = {
        _logLevel = LogLevelValue.trace,
        _logFile = logFile,
    }
    return setmetatable(self, Logger)
end

--- Initialise the log file
function Logger.Initialise()
    -- Get the current date and time
    local now = DateTime.now()
    local date = now:FormatUniversalTime("DD-MM-YYYY", "en-gb")
    local time = now:FormatUniversalTime("HH-mm-ss", "en-gb")
    local env = getgenv()

    local overrideFolder = env.coffeeGalaxyLogFolder
    local overrideName = env.coffeeGalaxyLogName
    if type(overrideName) == "string" and overrideName ~= "" then
        overrideName = overrideName:gsub("[^%w_%-]", "_")
    else
        overrideName = nil
    end

    -- Calculate the location of the log file
    local rootLogFolder = "roprobe-logs"
    local logFolder = if type(overrideFolder) == "string" and overrideFolder ~= ""
        then `{rootLogFolder}/{overrideFolder:gsub("[^%w_%-/]", "_")}`
        else `{rootLogFolder}/{date}`
    local logFile = `{logFolder}/{overrideName or time}.txt`

    -- Create the corrosponding folder and file, if enabled
    if LOGGER_ENABLED then
        pcall(makefolder, rootLogFolder)
        pcall(makefolder, logFolder)
        pcall(writefile, logFile, "")
        task.wait(0.2)
    end
    return logFile
end

--- Formats the logs in a nice way.
function Logger.FormatLog(level: LogLevel, prefix: string, ...)
    local args = { ... }
    local argLen = #args
    prefix = if prefix ~= "" then `, {prefix}` else ""
    local logEntry = `[{level:upper()}{prefix}: {DateTime.now():FormatUniversalTime("HH:mm:ss", "en-gb")}] `

    for i = 1, #args do
        local arg = args[i]
        logEntry ..= tostring(arg)

        if i ~= argLen then
            logEntry ..= " "
        end
    end

    return logEntry
end

--- Append the log to the file.
function Logger.append(self: Logger, logLevel: LogLevel, prefix: string?, ...): string?
    if LogLevelValue[logLevel] < self._logLevel then
        return nil
    end

    if not (LOGGER_ENABLED or PRINT_ENABLED) then
        return nil
    end

    local logEntry = Logger.FormatLog(logLevel, prefix or "", ...)
    if LOGGER_ENABLED then
        pcall(appendfile, self._logFile, logEntry .. "\n")
    end

    return logEntry
end

-- Custom loggers
function Logger.trace(self: Logger, prefix: string, ...)
    local logEntry = self:append("trace", prefix, ...)
    if logEntry and PRINT_ENABLED then
        print(logEntry)
    end
end
function Logger.debug(self: Logger, prefix: string, ...)
    local logEntry = self:append("debug", prefix, ...)
    if logEntry and PRINT_ENABLED then
        print(logEntry)
    end
end
function Logger.info(self: Logger, prefix: string, ...)
    local logEntry = self:append("info", prefix, ...)
    if logEntry and PRINT_ENABLED then
        print(logEntry)
    end
end
function Logger.warn(self: Logger, prefix: string, ...)
    local logEntry = self:append("warn", prefix, ...)
    if logEntry and PRINT_ENABLED then
        warn(logEntry)
    end
end
function Logger.error(self: Logger, prefix: string, ...)
    local logEntry = self:append("error", prefix, ...)
    if logEntry and PRINT_ENABLED then
        warn(logEntry)
    end
end

-- Return
return Logger.new()
