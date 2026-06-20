-- Dependencies
local Adapter = require("@modules/verde/Adapter")
local executeLuau = require("@modules/verde/execute_luau")
local ChangeHistoryHelper = require("@verde/Shared/ChangeHistoryService")
local Operations = require("@verde/Operations")
local Signal = require("@pkgs/signal")
local SoundPlayer = require("@verde/Shared/SoundPlayer")
local SoundPreview = require("@self/SoundPreview")
local WebSocketClient = require("@verde/WebSocketClient")

-- Types
type SoundInfo = SoundPreview.SoundInfo
export type Plugin = {
    Unloading: Signal.Signal<()>,
}
export type ShimsData = {
    soundPreview: SoundPreview.SoundPreview,
    plugin: Plugin,
}

--- Patches the environment so the plugin code can be translated to work on an executor.
local Shims = {}
Shims.__index = Shims
export type Shims = setmetatable<ShimsData, typeof(Shims)>

--- Create a new [`Shims`] object.
function Shims.new(): Shims
    local self: ShimsData = {
        soundPreview = SoundPreview.new(),
        plugin = {
            Unloading = Signal.new(),
        },
    }
    return setmetatable(self, Shims)
end

--- Destroy this object.
function Shims.Destroy(self: Shims)
    self.soundPreview:cleanup()
    self.plugin.Unloading:Destroy()
end

--- `WebSocketClient` -> executor WebSocket adapter.
function Shims._PatchWebSocket(_self: Shims)
    WebSocketClient.new = Adapter.new :: any
end

--- `ChangeHistoryService` -> run operations directly (no undo sadly)
function Shims._PatchChangeHistory(_self: Shims)
    function ChangeHistoryHelper.RecordOperation(_name: string, _displayName: string?, operationFunc: () -> ())
        local ok, err = pcall(operationFunc)
        if not ok then
            return false, tostring(err)
        end
        return true, ""
    end

    function ChangeHistoryHelper.CommitPendingOperations() end
    function ChangeHistoryHelper.IsRecordingInProgress()
        return false
    end
    function ChangeHistoryHelper.GetCanUndo()
        return false, nil
    end
    function ChangeHistoryHelper.GetCanRedo()
        return false, nil
    end
    function ChangeHistoryHelper.Undo() end
    function ChangeHistoryHelper.Redo() end
end

--- `SoundPlayer` -> delegate to this Shims' [`SoundPreview`] instance.
function Shims._PatchSoundPlayer(self: Shims)
    local preview = self.soundPreview

    function SoundPlayer.Init() end
    function SoundPlayer.stop(): (boolean, string?)
        return preview:stop()
    end
    function SoundPlayer.play(sourceSound: Sound, sourceId: string): (boolean, string?)
        return preview:play(sourceSound, sourceId)
    end
    function SoundPlayer.setTimePosition(timePosition: number): (boolean, string?)
        return preview:setTimePosition(timePosition)
    end
    function SoundPlayer.getCurrentInfo(): SoundInfo
        return preview:getCurrentInfo()
    end
    function SoundPlayer.isPlaying(sourceId: string): boolean
        return preview:isPlaying(sourceId)
    end
    function SoundPlayer.cleanup()
        preview:cleanup()
    end
end

--- `Operations.execute_luau` -> executor execute (no `ChangeHistoryService` recording).
function Shims._PatchOperations(_self: Shims)
    Operations.execute_luau = executeLuau
end

--- Install every patch.
function Shims.Apply(self: Shims)
    self:_PatchWebSocket()
    self:_PatchChangeHistory()
    self:_PatchSoundPlayer()
    self:_PatchOperations()
end

return Shims
