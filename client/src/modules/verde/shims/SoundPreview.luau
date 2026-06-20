-- Dependencies
local globalTrove = require("@core/globalTrove")

-- Services
local SoundService = cloneref(game:GetService("SoundService"))

-- Types
export type SoundInfo = {
	playing: boolean,
	timePosition: number,
	timeLength: number,
	sourceId: string?,
}
export type SoundPreviewData = {
	container: Folder?,
	currentSound: Sound?,
	currentSourceId: string?,
}

--- Previews a sound by playing it within Roblox.
---
--- TODO: look into playing within the extension directly?
local SoundPreview = {}
SoundPreview.__index = SoundPreview
export type SoundPreview = setmetatable<SoundPreviewData, typeof(SoundPreview)>

--- Create a new [`SoundPreview`] object.
function SoundPreview.new(): SoundPreview
	local self: SoundPreviewData = {
		container = nil,
		currentSound = nil,
		currentSourceId = nil,
	}
	return setmetatable(self, SoundPreview)
end

--- Lazily (re)create the hidden container that holds previewed sounds.
function SoundPreview.ensureContainer(self: SoundPreview): Folder
	local container = self.container
	if container and container.Parent then
		return container
	end

	local folder = Instance.new("Folder")
	folder.Name = "RoprobeSoundPreview"
	folder.Parent = SoundService
	self.container = folder
	return folder
end

--- Stop and destroy the currently-playing sound, if any.
function SoundPreview.stop(self: SoundPreview): (boolean, string?)
	local currentSound = self.currentSound
	if currentSound then
		local ok, err = pcall(function()
			currentSound:Stop()
			currentSound:Destroy()
		end)

		self.currentSound = nil
		self.currentSourceId = nil
		if not ok then
			return false, tostring(err)
		end
	end

	return true, nil
end

--- Clone `sourceSound` and play it, replacing whatever was playing before.
function SoundPreview.play(self: SoundPreview, sourceSound: Sound, sourceId: string): (boolean, string?)
	self:stop()

	local cloned = sourceSound:Clone()
	cloned.Name = "RoprobeSoundPreview"
	cloned.Parent = self:ensureContainer()

	self.currentSound = cloned
	self.currentSourceId = sourceId

	local ok, err = pcall(function()
		cloned:Play()
	end)
	if not ok then
		cloned:Destroy()
		self.currentSound = nil
		self.currentSourceId = nil
		return false, tostring(err)
	end

    local connection
	connection = cloned.Ended:Once(function()
        globalTrove:Remove(connection)
		if self.currentSound == cloned then
			cloned:Destroy()
			self.currentSound = nil
			self.currentSourceId = nil
		end
	end)
    globalTrove:Add(connection)

	return true, nil
end

--- Seek the currently-playing sound.
function SoundPreview.setTimePosition(self: SoundPreview, timePosition: number): (boolean, string?)
	local currentSound = self.currentSound
	if not currentSound then
		return false, "no sound currently playing"
	end

	local ok, err = pcall(function()
		currentSound.TimePosition = math.max(0, timePosition)
	end)
	if not ok then
		return false, tostring(err)
	end

	return true, nil
end

--- Snapshot of what is currently playing.
function SoundPreview.getCurrentInfo(self: SoundPreview): SoundInfo
	local currentSound = self.currentSound
	if currentSound and currentSound.Playing then
		return {
			playing = true,
			timePosition = currentSound.TimePosition,
			timeLength = currentSound.TimeLength,
			sourceId = self.currentSourceId,
		}
	end

	return { playing = false, timePosition = 0, timeLength = 0, sourceId = nil }
end

--- Whether `sourceId` is the sound currently playing.
function SoundPreview.isPlaying(self: SoundPreview, sourceId: string): boolean
	local currentSound = self.currentSound
	return self.currentSourceId == sourceId and currentSound ~= nil and currentSound.Playing
end

--- Stop playback and tear down the container.
function SoundPreview.cleanup(self: SoundPreview)
	self:stop()

	local container = self.container
	if container then
		container:Destroy()
		self.container = nil
	end
end

return SoundPreview
