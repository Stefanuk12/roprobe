-- Dependencies
local IncomingMessage = require("@types/IncomingMessage")
local Logger = require("@core/Logger")
local Partial = require("@types/Partial")
local Trove = require("@pkgs/trove")
local normaliseUrl = require("@functions/normaliseUrl")

-- Services
local HttpService = cloneref(game:GetService("HttpService"))

-- Types
type WebSocketConnection = {
    OnMessage: RBXScriptSignal<string>,
    OnClose: RBXScriptSignal<()>,
    Send: (self: WebSocketConnection, message: string) -> (),
    Close: (self: WebSocketConnection) -> (),
}
type WebSocket = {
    connect: (url: string) -> WebSocketConnection,
}

type IncomingMessage = IncomingMessage.IncomingMessage
export type MessageHandlers = {
    message: (message: IncomingMessage) -> (),
    connect: () -> (),
    disconnect: () -> (),
    error: (err: any) -> (),
}
export type MessageEvent = keyof<MessageHandlers>
export type AdapterConfig = {
    debugMode: boolean,
    silentMode: boolean,
}
export type AdapterData = {
    _trove: Trove.Trove,
    _connectionTrove: Trove.Trove,

    url: string,
    client: WebSocketConnection?,
    connected: boolean,
    messageHandlers: Partial.Partial<MessageHandlers>,
    config: AdapterConfig,
}

--- This object replicates the [`WebSocketClientInstance`] object from Verde.
local Adapter = {}
Adapter.__index = Adapter
export type Adapter = setmetatable<AdapterData, typeof(Adapter)>

--- Create a new [`Adapter`] object.
function Adapter.new(url: string?, config: AdapterConfig?): Adapter
    local resolvedConfig: AdapterConfig = config or {
        debugMode = false,
        silentMode = true,
    }

    local trove = Trove.new()
    local self: AdapterData = {
        _trove = trove,
        _connectionTrove = trove:Extend(),
        url = normaliseUrl(url or "ws://localhost:33883"),
        client = nil,
        connected = false,
        messageHandlers = {},
        config = resolvedConfig,
    }
    return setmetatable(self, Adapter)
end

--- Destroy this object and clean up.
function Adapter.Destroy(self: Adapter)
    self:disconnect()
    self._trove:Destroy()
end

--- Fire a debug print.
function Adapter.debugPrint(self: Adapter, ...: any)
    if self.config.silentMode or not self.config.debugMode then
        return
    end
    Logger:debug("adapter", ...)
end

--- Fire an info print.
function Adapter.infoPrint(self: Adapter, ...: any)
    if self.config.silentMode then
        return
    end
    Logger:info("adapter", ...)
end

--- Register a handler for an event, typed from the event name.
function Adapter.on<E>(self: Adapter, event: E & MessageEvent, handler: index<MessageHandlers, E>)
    (self.messageHandlers :: any)[event] = handler
end

--- Decode and dispatch a raw message frame to the `message` handler.
function Adapter.handleMessage(self: Adapter, message: string)
    if not message or message == "" then
        return
    end

    local ok, data = pcall(function(): IncomingMessage
        return HttpService:JSONDecode(message) :: IncomingMessage
    end)

    if ok and self.messageHandlers.message then
        self.messageHandlers.message(data)
    elseif not ok then
        Logger:warn("adapter", `failed to parse message: {message}`)
    end
end

--- Open the underlying socket and wire up its events.
function Adapter.connect(self: Adapter): boolean
    if self.connected then
        return true
    end

    local ok, socket = pcall((WebSocket :: WebSocket).connect, self.url)
    if not ok or not socket then
        Logger:warn("adapter", `websocket connection failed: {socket} to {self.url}`)
        local onError: typeof(({} :: MessageHandlers).error) = self.messageHandlers.error
        if onError then
            onError(socket)
        end
        return false
    end

    self.client = socket
    self.connected = true

    local trove = self._connectionTrove
    trove:Add(
        socket.OnMessage:Connect(function(message: string)
            local handleOk, err = pcall(function()
                self:handleMessage(message)
            end)
            if not handleOk then
                Logger:warn("adapter", `error handling message: {err}`)
            end
        end),
        "Disconnect"
    )
    trove:Add(
        socket.OnClose:Connect(function()
            self:onClosed()
        end),
        "Disconnect"
    )

    self:debugPrint("connected to", self.url)
    local onConnect: (() -> ())? = self.messageHandlers.connect
    if onConnect then
        task.defer(function()
            onConnect()
        end)
    end

    return true
end

-- Internal: underlying socket dropped by the remote side.
function Adapter.onClosed(self: Adapter)
    if not self.connected then
        return
    end

    self.connected = false
    self.client = nil
    local onDisconnect: (() -> ())? = self.messageHandlers.disconnect
    if onDisconnect then
        onDisconnect()
    end
end

--- Send a message over the socket, returning `false` if not connected or the send throws.
function Adapter.send(self: Adapter, message: string): boolean
    local client = self.client
    if not self.connected or not client then
        Logger:warn("adapter", "cannot send: not connected")
        return false
    end

    local ok, err = pcall(function()
        client:Send(message)
    end)
    if not ok then
        Logger:warn("adapter", `send failed: {err}`)
        return false
    end

    return true
end

-- Locally-initiated close that skips the `disconnect` handler to avoid re-entering SwitchState (remote closes go through `onClosed`).
function Adapter.disconnect(self: Adapter)
    local client = self.client
    self.connected = false
    self.client = nil

    if client then
        pcall(function()
            client:Close()
        end)
    end

    self._connectionTrove:Clean()
end

return Adapter
