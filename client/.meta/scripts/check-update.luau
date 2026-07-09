-- Dependencies
local fs = require("@lune/fs")
local net = require("@lune/net")
local process = require("@lune/process")
local serde = require("@lune/serde")
local stdio = require("@lune/stdio")

-- Constants
-- CLAUDE.md is pulled out to the monorepo root (governs broker + client + extension);
-- this script and its baseline lock live here in client/, run via client's mise.
local FILE = "../.claude/CLAUDE.md"
local LOCK = ".meta/.claude-md.lock"
local RAW_URL = "https://raw.githubusercontent.com/Stefanuk12/roprobe/main/.claude/CLAUDE.md"
local VERSION_PATTERN = "claude%-md%-version:%s*(%d+)"

--- Parse the integer version out of a CLAUDE.md's content, or nil if absent.
local function parseVersion(content: string): number?
    local raw = content:match(VERSION_PATTERN)
    return if raw ~= nil then tonumber(raw) else nil
end

--- Record the current local file as the pristine baseline (version + content hash).
local function writeLock(content: string, version: number?)
    fs.writeFile(LOCK, serde.encode("json", { version = version, hash = serde.hash("sha256", content) }, true))
end

-- Need a local, versioned CLAUDE.md to work with
if not fs.isFile(FILE) then
    return
end
local localContent = fs.readFile(FILE)
local localVersion = parseVersion(localContent)
if localVersion == nil then
    return
end

-- Maintainer helper: `lune run .meta/scripts/check-update rebaseline` re-records the
-- lock from the current CLAUDE.md. Run after editing it and bumping the version marker.
if process.args[1] == "rebaseline" then
    writeLock(localContent, localVersion)
    stdio.write(`[claude-md] lock re-baselined at v{localVersion}.\n`)
    return
end

-- Baseline hash recorded at last install/update (pristine, unedited content)
local baselineHash: string? = nil
if fs.isFile(LOCK) then
    local ok, decoded = pcall(function()
        return serde.decode("json", fs.readFile(LOCK))
    end)
    if ok and typeof(decoded) == "table" then
        baselineHash = decoded.hash
    end
end

-- Fetch upstream; stay silent on any network/HTTP failure so sessions never block
local ok, response = pcall(net.request, RAW_URL)
if not ok or not response.ok then
    return
end
local remoteContent = response.body
local remoteVersion = parseVersion(remoteContent)
if remoteVersion == nil or remoteVersion <= localVersion then
    return
end

-- Overwrite only when the local file is byte-identical to its baseline — i.e. the
-- user hasn't edited it. Any edit (even to the version line) changes the hash.
if baselineHash ~= serde.hash("sha256", localContent) then
    stdio.write(`[claude-md] v{remoteVersion} available upstream; local {FILE} is edited — keeping yours.\n`)
    return
end

fs.writeFile(FILE, remoteContent)
writeLock(remoteContent, remoteVersion)
stdio.write(`[claude-md] updated {FILE}: v{localVersion} -> v{remoteVersion}.\n`)
