-- Dependencies
local exec = require("./helpers/exec")
local fs = require("@lune/fs")
local serde = require("@lune/serde")

-- Constants
local OUTPUT = ".meta/sourcemap.json"

type SourcemapNode = {
    filePaths: { string }?,
    children: { SourcemapNode }?,
}

--- Collapse `.` and `..` segments so re-rooted paths stay canonical
--- (e.g. `client/../vendor/x` becomes `vendor/x`).
local function normalize(path: string): string
    local parts: { string } = {}
    for segment in path:gmatch("[^/]+") do
        if segment == ".." and #parts > 0 and parts[#parts] ~= ".." then
            table.remove(parts)
        elseif segment ~= "." then
            table.insert(parts, segment)
        end
    end

    return table.concat(parts, "/")
end

-- Rojo emits filePaths relative to the project file's directory (client/), but
-- luau-lsp resolves them against the workspace root, which is the monorepo
-- root. Without re-rooting, no file maps to its sourcemap node and instance
-- requires fail with "Unknown require: unsupported path".
local function reroot(node: SourcemapNode)
    if node.filePaths then
        for i, path in node.filePaths do
            node.filePaths[i] = normalize(`client/{path}`)
        end
    end

    if node.children then
        for _, child in node.children do
            reroot(child)
        end
    end
end

local result = exec("rojo", { "sourcemap", "default.project.json" })
local sourcemap: SourcemapNode = serde.decode("json", result.stdout)
reroot(sourcemap)
fs.writeFile(OUTPUT, serde.encode("json", sourcemap))

print(`Sourcemap written to {OUTPUT} (paths re-rooted to the monorepo root)`)
