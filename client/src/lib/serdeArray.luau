-- Dependencies
local Squash = require("@pkgs/squash")

-- Vars
local vlq = Squash.vlq()

--- A `SerDes` for arrays that matches squash-rs's serde layout for `Vec<T>`.
local function serdeArray<T>(serdes: Squash.SerDes<T>): Squash.SerDes<{ T }>
    return {
        ser = function(cursor, values)
            for i = #values, 1, -1 do
                serdes.ser(cursor, values[i])
            end

            vlq.ser(cursor, #values)
        end,

        des = function(cursor)
            local count = vlq.des(cursor)
            local values = table.create(count)

            for i = 1, count do
                values[i] = serdes.des(cursor)
            end

            return values
        end,
    }
end

return serdeArray
