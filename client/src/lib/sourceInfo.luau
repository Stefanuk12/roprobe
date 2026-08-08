-- Types
export type SourceInfo = { name: string?, chunk: string?, line: number? }

--- Where a function was defined: its `debug.info` name, chunk and line, with an
--- anonymous or C-level closure's blanks folded to `nil`.
---
--- Only closures, never stack levels: `debug.info` counts levels from whichever
--- function calls it, so one read through here would be off by this frame.
local function sourceInfo(target: (...any) -> ...any): SourceInfo
    local info: SourceInfo = { name = nil, chunk = nil, line = nil }

    pcall(function()
        local name, chunk, line = debug.info(target, "nsl")

        if type(name) == "string" and name ~= "" then
            info.name = name
        end
        if type(chunk) == "string" and chunk ~= "" then
            info.chunk = chunk
        end
        if type(line) == "number" and line >= 0 then
            info.line = line
        end
    end)

    return info
end

return sourceInfo
