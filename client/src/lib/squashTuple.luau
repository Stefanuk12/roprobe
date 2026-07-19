-- Dependencies
local Squash = require("@pkgs/squash")

-- Types
type SerDes<T...> = Squash.SerDes<T...>

--- The value carried by a tuple variant.
---
--- NOTE: Copies that strip metatables lose the `unpack` method; `table.clone` keeps it.
export type Tuple<T...> = { unpack: (self: any) -> T... }

--- A tuple `SerDes` with order-typed helpers.
---
--- - `pack` builds a [`Tuple`] from positional values.
--- - `unpack` returns them back in declaration order.
export type TupleSerDes<T...> = SerDes<Tuple<T...>> & {
    pack: (T...) -> Tuple<T...>,
    unpack: (Tuple<T...>) -> T...,
}

type SquashTuple =
    (<A>(SerDes<A>) -> TupleSerDes<A>)
    & (<A, B>(SerDes<A>, SerDes<B>) -> TupleSerDes<A, B>)
    & (<A, B, C>(SerDes<A>, SerDes<B>, SerDes<C>) -> TupleSerDes<A, B, C>)
    & (<A, B, C, D>(SerDes<A>, SerDes<B>, SerDes<C>, SerDes<D>) -> TupleSerDes<A, B, C, D>)
    & (<A, B, C, D, E>(
        SerDes<A>,
        SerDes<B>,
        SerDes<C>,
        SerDes<D>,
        SerDes<E>
    ) -> TupleSerDes<A, B, C, D, E>)
    & (<A, B, C, D, E, F>(
        SerDes<A>,
        SerDes<B>,
        SerDes<C>,
        SerDes<D>,
        SerDes<E>,
        SerDes<F>
    ) -> TupleSerDes<A, B, C, D, E, F>)
    & (<A, B, C, D, E, F, G>(
        SerDes<A>,
        SerDes<B>,
        SerDes<C>,
        SerDes<D>,
        SerDes<E>,
        SerDes<F>,
        SerDes<G>
    ) -> TupleSerDes<A, B, C, D, E, F, G>)
    & (<A, B, C, D, E, F, G, H>(
        SerDes<A>,
        SerDes<B>,
        SerDes<C>,
        SerDes<D>,
        SerDes<E>,
        SerDes<F>,
        SerDes<G>,
        SerDes<H>
    ) -> TupleSerDes<A, B, C, D, E, F, G, H>)
    & ((...SerDes<any>) -> TupleSerDes<...any>)

--- Stores tuples within an array, reserving the declaration order.
---
--- To do that, values can only be built with `pack` and read with `unpack` (mixed types supported).
---
--- # Example
---
--- ```lua
--- local moveTuple = squashTuple(Squash.f64(), Squash.string())
--- local msg = squashEnum({
---   { type = "move" :: "move", content = Squash.T(moveTuple) },
--- })
---
--- msg.ser(cursor, { type = "move", content = moveTuple.pack(1.5, "north") })
---
--- local x, direction = msg.des(cursor).content:unpack()
--- ```
---
--- NOTE: Element types are tracked per arity overload up to 8 elements;
--- longer tuples fall back to untyped values.
local function squashTuple(...: SerDes<any>): TupleSerDes<...any>
    local count = select("#", ...)
    local inner = (Squash.tuple(...) :: any) :: SerDes<...any>

    local function unpack(values: { any })
        return table.unpack(values, 1, count)
    end

    local metatable = { __index = { unpack = unpack } }

    return {
        ser = function(cursor, values)
            inner.ser(cursor, table.unpack(values, 1, count))
        end,

        des = function(cursor)
            return setmetatable({ inner.des(cursor) }, metatable)
        end,

        pack = function(...)
            return setmetatable({ ... }, metatable)
        end,

        unpack = unpack,
    } :: any
end

return (squashTuple :: any) :: SquashTuple
