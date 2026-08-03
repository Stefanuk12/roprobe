-- Dependencies
local ConsoleRelay = require("@features/consoleRelay")
local DomSync = require("@features/broker/DomSync")
local DomWatch = require("@features/broker/DomWatch")
local Logger = require("@engine/Logger")
local Messages = require("@lib/Messages")
local Operations = require("@features/broker/operations")
local Trove = require("@pkgs/trove")
local WebSocketClient = require("@engine/WebSocketClient")

-- Services
local HttpService = cloneref(game:GetService("HttpService"))
local Players = cloneref(game:GetService("Players"))

--- Default path to the broker handshake lockfile (port + auth token).
local DEFAULT_LOCKFILE = "roprobe/broker.json"

--- Manages the single link to the broker via WebSocket.
local Broker = {}
Broker.__index = Broker

-- Types
export type Upstream = Messages.Upstream
export type Config = {
    host: string?,
    port: number?,
    token: string?,
    lockfile: string?,
    retryInterval: number?,

    username: string?,

    lazy: boolean?,
    scope: Instance?,
    verde: boolean?,
    luauLsp: boolean?,
    securityLevel: string?,
}
export type BrokerData = {
    _trove: Trove.Trove,
    _connectionTrove: Trove.Trove,
    _config: Config,
    _sync: DomSync.DomSync,
    _watch: DomWatch.DomWatch?,
    _ws: WebSocketClient.WebSocketClient?,
    _reconnectThread: thread?,
    _upstreams: { [Upstream]: boolean },

    isRunning: boolean,
    isConnected: boolean,
}
export type Broker = setmetatable<BrokerData, typeof(Broker)>

--- Resolve `host, port, token` from `config`, or `nil` until the broker publishes both port and token.
local function resolveEndpoint(config: Config): (string?, number?, string?)
    local host = config.host
    local port = config.port
    local token = config.token

    -- Check if already loaded
    local injected = getgenv().roprobe_config
    if type(injected) == "table" then
        host = host or injected.host
        port = port or injected.port
        token = token or injected.token
    end
    host = host or "127.0.0.1"

    -- No previous client, try to read the lockfile
    if not (port and token) then
        local path = config.lockfile or DEFAULT_LOCKFILE
        local ok, handshake = pcall(function()
            if not isfile(path) then
                return nil
            end
            return HttpService:JSONDecode(readfile(path))
        end)

        if ok and type(handshake) == "table" then
            port = port or handshake.port
            token = token or handshake.token
        end
    end

    if port and token then
        return host, port, token
    end
    return nil, nil, nil
end

--- The account this client is playing on.
local function resolvePlayerName(config: Config): string?
    if config.username then
        return config.username
    end

    local player = Players.LocalPlayer
    if player then
        return player.Name
    end
    return nil
end

--- Create a new [`Broker`] object; nothing connects until [`Broker.Start`].
function Broker.new(config: Config?): Broker
    local resolved: Config = config or {}

    local trove = Trove.new()
    local self: BrokerData = {
        _trove = trove,
        _connectionTrove = trove:Extend(),
        _config = resolved,
        _sync = DomSync.new(),
        _watch = nil,
        _ws = nil,
        _reconnectThread = nil,
        _upstreams = {
            Verde = resolved.verde ~= false,
            LuauLsp = resolved.luauLsp == true,
        },

        isRunning = true,
        isConnected = false,
    }
    return setmetatable(self, Broker)
end

--- Destroy this object and clean up.
function Broker.Destroy(self: Broker)
    self.isRunning = false
    self.isConnected = false

    if self._reconnectThread then
        task.cancel(self._reconnectThread)
        self._reconnectThread = nil
    end

    if self._ws then
        pcall(function()
            self._ws:Destroy()
        end)
        self._ws = nil
    end

    self._trove:Destroy()
end

--- Begin driving the connection and kick off the first attempt.
function Broker.Start(self: Broker)
    self:_attempt()
end

--- Try to open the socket once, re-attempting on failure.
function Broker._attempt(self: Broker)
    if not self.isRunning or self._ws then
        return
    end

    local host, port, token = resolveEndpoint(self._config)
    if not (host and port and token) then
        Logger:Debug("broker", "no handshake yet, retrying")
        self:_scheduleReconnect()
        return
    end

    local url = `ws://{host}:{port}/?token={token}`
    local securityLevel = self._config.securityLevel
    if securityLevel then
        url ..= `&security={securityLevel}`
    end

    local username = resolvePlayerName(self._config)
    if username then
        url ..= `&username={HttpService:UrlEncode(username)}`
    end

    local ok, ws = pcall(WebSocketClient.new, url)
    if not ok or not ws then
        Logger:Debug("broker", `connection failed: {ws}`)
        self:_scheduleReconnect()
        return
    end

    local socket = ws
    local trove = self._connectionTrove
    trove:Add(
        socket.onMessage:Connect(function(frame: string)
            self:_onMessage(frame)
        end),
        "Disconnect"
    )
    trove:Add(
        socket.onClose:Connect(function()
            self:_onDisconnected()
        end),
        "Disconnect"
    )

    self._ws = socket
    socket:Initialise()
    Logger:Info("broker", `connecting to {url}`)
end

--- Decode and dispatch one frame from the broker, logging and dropping unknown or malformed frames rather than treating them as fatal.
function Broker._onMessage(self: Broker, frame: string)
    local ok, message = pcall(Messages.decode, frame)
    if not ok then
        Logger:Warn("broker", `undecodable frame: {message}`)
        return
    end

    -- A throw handling one frame must not escape into the socket callback and
    -- tear the connection down; log it and keep the link alive.
    local handled, err = pcall(self._dispatch, self, message)
    if not handled then
        Logger:Warn("broker", `handling {message.type} failed: {err}`)
    end
end

--- Route one decoded [`Messages.ServerMessage`] to its handler.
function Broker._dispatch(self: Broker, message: Messages.ServerMessage)
    if message.type == "Hello" then
        self:_onHello()
    elseif message.type == "UpstreamChanged" then
        local state = message.content
        local status = if state.enabled then "enabled" else "disabled"
        Logger:Info("broker", `upstream {state.upstream} {status}`)
    elseif message.type == "RequestChildren" then
        self:_onRequestChildren(message.content)
    elseif message.type == "RequestNode" then
        self:_onRequestNode(message.content)
    elseif message.type == "RequestSnapshot" then
        self:_onRequestSnapshot(message.content)
    elseif message.type == "Search" then
        self:_onSearch(message.content)
    elseif message.type == "RequestNodes" then
        self:_onRequestNodes(message.content)
    elseif message.type == "Operation" then
        self:_onOperation(message.content)
    else
        Logger:Warn("broker", `unhandled message {message.type}`)
    end
end

--- Perform an operation like changing Dom / running code
function Broker._onOperation(self: Broker, content: Messages.OperationRequest)
    Logger:Debug("broker", `operation {content.id}: {content.op.type}`)

    local result = Operations.dispatch({ sync = self._sync, watch = self._watch }, content.op)
    if result.type == "Err" then
        local summary = result.content:split("\n")[1]
        Logger:Error("broker", `operation {content.id} ({content.op.type}) failed: {summary}`)
    else
        Logger:Debug("broker", `operation {content.id} ({content.op.type}) -> {result.type}`)
    end

    self:_send({ type = "OperationResult" :: "OperationResult", content = { id = content.id, result = result } })
end

--- Mirror a node's children.
function Broker._onRequestChildren(self: Broker, id: string?)
    local watch = self._watch
    if not watch then
        return
    end

    if id == nil then
        watch:SyncChildren(nil)
        return
    end

    local instance = self._sync:InstanceOf(id)
    if instance then
        watch:SyncChildren(instance)
    else
        Logger:Debug("broker", `children requested for unknown id {id}`)
    end
end

--- Mirror a singular node.
function Broker._onRequestNode(self: Broker, id: string)
    local watch = self._watch
    if not watch then
        return
    end

    local instance = self._sync:InstanceOf(id)
    if instance then
        watch:SyncNode(instance)
    else
        Logger:Debug("broker", `node requested for unknown id {id}`)
    end
end

--- Mirror ALL nodes under a node, handling stale nodes.
function Broker._onRequestSnapshot(self: Broker, id: string?)
    local watch = self._watch
    if not watch then
        return
    end

    if id == nil then
        watch:SyncSnapshot(nil)
        return
    end

    local instance = self._sync:InstanceOf(id)
    if instance then
        watch:SyncSnapshot(instance)
    else
        Logger:Debug("broker", `snapshot requested for unknown id {id}`)
    end
end

--- Search for under a node using a search query.
function Broker._onSearch(self: Broker, content: { from: string, query: string })
    local watch = self._watch
    if not watch then
        return
    end

    local instance = self._sync:InstanceOf(content.from)
    if instance then
        watch:SyncSearch(instance, content.query)
    else
        Logger:Debug("broker", `search requested from unknown id {content.from}`)
    end
end

--- Mirror multiple (unrelated) nodes in one.
function Broker._onRequestNodes(self: Broker, ids: { string })
    local watch = self._watch
    if not watch then
        return
    end

    local instances: { Instance } = {}
    for _, id in ids do
        local instance = self._sync:InstanceOf(id)
        if instance then
            table.insert(instances, instance)
        end
    end

    local unknown = #ids - #instances
    if unknown > 0 then
        Logger:Debug("broker", `nodes request included {unknown} unknown id(s)`)
    end
    if #instances > 0 then
        watch:SyncNodes(instances)
    end
end

--- The broker said hello :)
function Broker._onHello(self: Broker)
    self.isConnected = true
    self._sync:Reset()

    self:_send({ type = "EnumFamilies" :: "EnumFamilies", content = Messages.enumFamilies })

    for upstream, enabled in self._upstreams do
        self:_send({ type = "SetUpstream" :: "SetUpstream", content = { upstream = upstream, enabled = enabled } })
    end

    -- Relay console output only while connected.
    local console = self._connectionTrove:Add(ConsoleRelay.new())
    self._connectionTrove:Add(
        console.onBatch:Connect(function(entries: { Messages.LogEntry })
            local count = #entries
            Logger:Debug("broker", `relaying {count} console line(s)`)
            self:_send({ type = "Log" :: "Log", content = entries })
        end),
        "Disconnect"
    )
    console:Initialise()

    local watch = self._connectionTrove:Add(DomWatch.new(self._sync, self._config.scope, self._config.lazy))
    self._watch = watch
    self._connectionTrove:Add(
        watch.onPatch:Connect(function(patch: Messages.DomPatch)
            self:_sendPatch(patch)
        end),
        "Disconnect"
    )
    watch:Start()
end

--- Toggle an upstream service.
function Broker.SetUpstream(self: Broker, upstream: Upstream, enabled: boolean)
    self._upstreams[upstream] = enabled
    if self.isConnected then
        self:_send({ type = "SetUpstream" :: "SetUpstream", content = { upstream = upstream, enabled = enabled } })
    end
end

--- Ask the broker to make this client the active (synced) session, taking the
--- slot from whichever client currently holds it.
function Broker.RequestActive(self: Broker)
    if self.isConnected then
        self:_send({ type = "RequestActive" :: "RequestActive" })
    end
end

--- Forward a DOM patch, skipping empty patches.
function Broker._sendPatch(self: Broker, patch: Messages.DomPatch)
    if #patch.upserts == 0 and #patch.removals == 0 and #patch.updates == 0 then
        return
    end

    self:_send({ type = "UpdateDom" :: "UpdateDom", content = patch })
end

--- Send a message to the broker.
function Broker._send(self: Broker, message: Messages.ClientMessage)
    local socket = self._ws
    if not socket then
        return
    end

    local ok, err = pcall(function()
        socket:Send(Messages.encode(message))
    end)
    if not ok then
        Logger:Warn("broker", `send failed: {err}`)
    end
end

--- Connection dropped, clear everything and try to reconnect.
function Broker._onDisconnected(self: Broker)
    if not self._ws then
        return
    end

    self._ws = nil
    self.isConnected = false

    self._connectionTrove:Clean()
    self._watch = nil

    if self.isRunning then
        Logger:Info("broker", "disconnected, reconnecting")
        self:_scheduleReconnect()
    end
end

--- Schedule a single reconnect attempt after `retryInterval` seconds.
function Broker._scheduleReconnect(self: Broker)
    if self._reconnectThread or not self.isRunning then
        return
    end

    local interval = self._config.retryInterval or 1
    self._reconnectThread = task.delay(interval, function()
        self._reconnectThread = nil
        self:_attempt()
    end)
end

return Broker
