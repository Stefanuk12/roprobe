-- Dependencies
local Debounce = require("@luau_lsp/Utils/Debounce")
local InstanceTracker = require("@luau_lsp/InstanceTracker")
local Log = require("@luau_lsp/Utils/Log")
local ServerEndpoints = require("@luau_lsp/ServerEndpoints")
local Signal = require("@luau_lsp/Utils/Signal")
local types = require("@luau_lsp/types")

-- Types
type HttpResult = types.Result<types.HttpResponse, string>
export type ManagerData = {
    _connected: boolean,
    _address: string?,
    _sendDebounce: Debounce.Debounce?,
    _instanceTracker: InstanceTracker.InstanceTracker,
    _silenceConnectionErrors: boolean,
    onConnectionChanged: Signal.Signal<boolean>,
}

--- An executor reimplementation of the plugin's `LSPManager`.
local Manager = {}
Manager.__index = Manager
export type Manager = setmetatable<ManagerData, typeof(Manager)>

--- Create a new [`Manager`] object.
function Manager.new(instanceTracker: InstanceTracker.InstanceTracker): Manager
    local self: ManagerData = {
        _connected = false,
        _address = nil,
        _sendDebounce = nil,
        _instanceTracker = instanceTracker,
        _silenceConnectionErrors = false,
        onConnectionChanged = Signal.new() :: Signal.Signal<boolean>,
    }
    return setmetatable(self, Manager)
end

--- Returns whether we are currently connected to the language server.
function Manager.isConnected(self: Manager): boolean
    return self._connected
end

--- Sets the language server address from host and port components.
function Manager.setAddress(self: Manager, host: string, port: number)
    if not host:match("^https?://") then
        host = "http://" .. host
    end
    host = host:gsub("/$", "")

    Log.debug(`Setting LSP address to {host}:{port}`)
    self._address = `{host}:{port}`
end

--- Wire up debounced sends driven by tracked instance changes.
function Manager.init(self: Manager)
    self._sendDebounce = Debounce.new(0.5, function()
        if not self._connected then
            return
        end

        local ok, err = pcall(function()
            self:sendFullDMInfo()
        end)
        if not ok then
            Log.debug(`Deferred DataModel send errored: {err}`)
            self:disconnect()
        end
    end)

    self._instanceTracker.onChanged:connect(function(_instance: Instance)
        self:deferredSendFullDMInfo()
    end)
end

--- Log an HTTP error.
function Manager._logHttpError(self: Manager, operation: string, result: any)
    local message = `{operation} on {self._address} failed: {result}`
    if self._silenceConnectionErrors and not self._connected then
        Log.debug(message)
        return
    end
    Log.warn(message)
end

--- Probe the language server for connectivity and version support.
function Manager.getFilePaths(self: Manager): { [Instance]: { string } }?
    assert(self._address, "Manager must be initialized before use")
    Log.debug("Getting file paths")

    local result = ServerEndpoints.getFilePaths(self._address) :: HttpResult
    if not result.ok then
        if string.find(result.error, "^404") then
            Log.warn(
                "Running against an older version of the language server that does not support Script Sync. Please update luau-lsp for full functionality."
            )
            return {}
        end

        self:_logHttpError("Getting file paths", result.error)
        return nil
    end

    return {}
end

--- Sends the complete DataModel tree to the language server.
function Manager.sendFullDMInfo(self: Manager): boolean
    assert(self._address, "Manager must be initialized before use")
    Log.debug("Sending full DM info")

    local filePaths = self:getFilePaths()
    if filePaths == nil then
        -- Server isn't up, don't bother encoding the tree.
        Log.debug("Server failed to respond, cancelling DataModel send")
        self:disconnect()
        return false
    end

    local tree = self._instanceTracker:encodeAll(filePaths)
    local result = (ServerEndpoints.full(self._address, tree) :: any) :: HttpResult
    if not result.ok then
        self:_logHttpError("Sending DataModel info", result.error)
        self:disconnect()
        return false
    end

    Log.debug("Successfully sent full DataModel info")
    return true
end

--- Schedules a deferred DataModel update using debouncing.
function Manager.deferredSendFullDMInfo(self: Manager): thread
    assert(self._sendDebounce, "Manager must be initialized before use")
    Log.debug("Deferring send of DataModel info")
    return self._sendDebounce:call()
end

--- Establishes a connection to the language server and starts tracking changes.
function Manager.connect(self: Manager): boolean
    if self._connected then
        return true
    end

    Log.debug("Connecting to LSP")

    local success = self:sendFullDMInfo()
    if not success then
        return false
    end

    self._instanceTracker:watchAll()

    self._connected = true
    self.onConnectionChanged:fire(true)

    Log.info("Successfully connected")
    return true
end

--- Disconnects from the language server and stops tracking changes.
function Manager.disconnect(self: Manager)
    if not self._connected then
        return
    end

    Log.debug("Disconnecting from LSP")
    self._connected = false
    self.onConnectionChanged:fire(false)

    if self._sendDebounce then
        self._sendDebounce:cancel()
    end

    self._instanceTracker:unwatchAll()

    Log.info("Successfully disconnected")
end

--- Cleans up the manager. Call this when unloading.
function Manager.unload(self: Manager)
    self:disconnect()
    self.onConnectionChanged:destroy()
    self._instanceTracker:unload()
end

return Manager
