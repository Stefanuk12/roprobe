import { Squash, type SerDes } from "squash-ts";
import { boolean, f64, optStr, rawBuffer, serdeArray, str, taggedUnion, u16, u32, u8 } from "./serde";
import { domValue, type DomId, type DomValue } from "./variant";

/** Which way a captured call crossed the network boundary. */
export type RemoteDirection = "Outgoing" | "Incoming";

/**
 * A remote a call crossed, or an instance one of its arguments referenced. `id`
 * shares the client's DOM id scheme, and is `""` for an instance it could not name.
 */
export interface InstanceRef {
    id: DomId;
    class: string;
    name: string;
    /** `GetFullName()`, or the name alone when that call was blocked. */
    path: string;
}

/** The call site a captured call came from. */
export interface CallSource {
    script: string | undefined;
    functionName: string | undefined;
    chunk: string | undefined;
    line: number | undefined;
    /** Whether the call came from our own executor thread rather than the game. */
    isExecutor: boolean;
    /** The `Actor` a call was captured under, when it came from an actor VM. */
    actor: string | undefined;
}

/** A function value, which cannot cross the wire, described well enough to find it. */
export interface FunctionRef {
    name: string | undefined;
    chunk: string | undefined;
    line: number | undefined;
}

/** One `key -> value` pair of a captured table's hash part. */
export interface LuaEntry {
    key: LuaValue;
    value: LuaValue;
}

/** A captured Luau table, split the way Luau stores one. */
export interface LuaTable {
    /** Per-call ordinal a `Cycle` elsewhere in the same call points back at. */
    id: number;
    array: LuaValue[];
    entries: LuaEntry[];
    /** Whether the capture caps (depth, entry count) dropped part of this table. */
    truncated: boolean;
    metatable: boolean;
}

/** One argument (or return value) of a captured call; Roblox datatypes ride `DomValue`. */
export type LuaValue =
    | { type: "Nil" }
    | { type: "Bool"; content: boolean }
    | { type: "Number"; content: number }
    /** Arbitrary bytes: a Luau string is not required to be UTF-8. */
    | { type: "String"; content: Uint8Array }
    | { type: "Table"; content: LuaTable }
    | { type: "Instance"; content: InstanceRef }
    | { type: "Roblox"; content: DomValue }
    | { type: "Buffer"; content: Uint8Array }
    | { type: "Function"; content: FunctionRef }
    /** A back-reference to the `LuaTable.id` of a table already sent in this call. */
    | { type: "Cycle"; content: number }
    /** Anything else (`thread`, foreign userdata), by `typeof`. */
    | { type: "Opaque"; content: string };

/** One captured remote call, as the client saw it. */
export interface RemoteCall {
    /** Per-session, monotonic. */
    id: number;
    direction: RemoteDirection;
    remote: InstanceRef;
    /** The method that carried it (`FireServer`, `OnClientEvent`, …). */
    method: string;
    arguments: LuaValue[];
    /** What a `RemoteFunction`/`BindableFunction` answered with, if anything. */
    returns: LuaValue[] | undefined;
    source: CallSource;
    /** The client's `os.clock()` at capture — no shared epoch with our clock. */
    timestamp: number;
}

/** What the client's spy should capture. */
export interface SpyConfig {
    /** Whether to hook at all; off leaves the game's metamethods untouched. */
    enabled: boolean;
    outgoing: boolean;
    incoming: boolean;
    /** Capture `BindableEvent`/`BindableFunction` alongside the networked ones. */
    bindables: boolean;
    maxDepth: number;
    maxEntries: number;
    maxBytes: number;
}

const optU32 = Squash.opt(u32);

/** A plain struct, so `ser` runs forward and `des` in reverse. */
export const instanceRef: SerDes<InstanceRef> = {
    ser(cursor, value) {
        str.ser(cursor, value.id);
        str.ser(cursor, value.class);
        str.ser(cursor, value.name);
        str.ser(cursor, value.path);
    },

    des(cursor) {
        const path = str.des(cursor);
        const name = str.des(cursor);
        const cls = str.des(cursor);
        return { id: str.des(cursor), class: cls, name, path };
    },
};

/** A plain struct, so `ser` runs forward and `des` in reverse. */
export const callSource: SerDes<CallSource> = {
    ser(cursor, value) {
        optStr.ser(cursor, value.script);
        optStr.ser(cursor, value.functionName);
        optStr.ser(cursor, value.chunk);
        optU32.ser(cursor, value.line);
        boolean.ser(cursor, value.isExecutor);
        optStr.ser(cursor, value.actor);
    },

    des(cursor) {
        const actor = optStr.des(cursor);
        const isExecutor = boolean.des(cursor);
        const line = optU32.des(cursor);
        const chunk = optStr.des(cursor);
        const functionName = optStr.des(cursor);
        return { script: optStr.des(cursor), functionName, chunk, line, isExecutor, actor };
    },
};

/** A plain struct, so `ser` runs forward and `des` in reverse. */
export const functionRef: SerDes<FunctionRef> = {
    ser(cursor, value) {
        optStr.ser(cursor, value.name);
        optStr.ser(cursor, value.chunk);
        optU32.ser(cursor, value.line);
    },

    des(cursor) {
        const line = optU32.des(cursor);
        const chunk = optStr.des(cursor);
        return { name: optStr.des(cursor), chunk, line };
    },
};

/** Mirrors `RemoteDirection`. */
export const remoteDirection: SerDes<RemoteDirection> = Squash.literal<RemoteDirection>("Outgoing", "Incoming");

// `LuaValue` nests itself through tables, so the codec is declared up front and
// filled in below.
const luaValueRef: { current?: SerDes<LuaValue> } = {};
export const luaValue: SerDes<LuaValue> = {
    ser(cursor, value) {
        luaValueRef.current!.ser(cursor, value);
    },
    des(cursor) {
        return luaValueRef.current!.des(cursor);
    },
};

export const luaValues = serdeArray(luaValue);

/** A plain struct, so `ser` runs forward and `des` in reverse. */
export const luaEntry: SerDes<LuaEntry> = {
    ser(cursor, entry) {
        luaValue.ser(cursor, entry.key);
        luaValue.ser(cursor, entry.value);
    },

    des(cursor) {
        const value = luaValue.des(cursor);
        return { key: luaValue.des(cursor), value };
    },
};

const luaEntries = serdeArray(luaEntry);

/** A plain struct, so `ser` runs forward and `des` in reverse. */
export const luaTable: SerDes<LuaTable> = {
    ser(cursor, value) {
        u32.ser(cursor, value.id);
        luaValues.ser(cursor, value.array);
        luaEntries.ser(cursor, value.entries);
        boolean.ser(cursor, value.truncated);
        boolean.ser(cursor, value.metatable);
    },

    des(cursor) {
        const metatable = boolean.des(cursor);
        const truncated = boolean.des(cursor);
        const entries = luaEntries.des(cursor);
        const array = luaValues.des(cursor);
        return { id: u32.des(cursor), array, entries, truncated, metatable };
    },
};

luaValueRef.current = taggedUnion<LuaValue>([
    { type: "Nil" },
    { type: "Bool", content: boolean },
    { type: "Number", content: f64 },
    { type: "String", content: rawBuffer },
    { type: "Table", content: luaTable },
    { type: "Instance", content: instanceRef },
    { type: "Roblox", content: domValue },
    { type: "Buffer", content: rawBuffer },
    { type: "Function", content: functionRef },
    { type: "Cycle", content: u32 },
    { type: "Opaque", content: str },
]);

const optLuaValues = Squash.opt(luaValues);

/** A plain struct, so `ser` runs forward and `des` in reverse. */
export const remoteCall: SerDes<RemoteCall> = {
    ser(cursor, call) {
        u32.ser(cursor, call.id);
        remoteDirection.ser(cursor, call.direction);
        instanceRef.ser(cursor, call.remote);
        str.ser(cursor, call.method);
        luaValues.ser(cursor, call.arguments);
        optLuaValues.ser(cursor, call.returns);
        callSource.ser(cursor, call.source);
        f64.ser(cursor, call.timestamp);
    },

    des(cursor) {
        const timestamp = f64.des(cursor);
        const source = callSource.des(cursor);
        const returns = optLuaValues.des(cursor);
        const args = luaValues.des(cursor);
        const method = str.des(cursor);
        const remote = instanceRef.des(cursor);
        const direction = remoteDirection.des(cursor);
        return {
            id: u32.des(cursor),
            direction,
            remote,
            method,
            arguments: args,
            returns,
            source,
            timestamp,
        };
    },
};

/** Mirrors `Vec<RemoteCall>`. */
export const remoteCalls = serdeArray(remoteCall);

/** A plain struct, so `ser` runs forward and `des` in reverse. */
export const spyConfig: SerDes<SpyConfig> = {
    ser(cursor, config) {
        boolean.ser(cursor, config.enabled);
        boolean.ser(cursor, config.outgoing);
        boolean.ser(cursor, config.incoming);
        boolean.ser(cursor, config.bindables);
        u8.ser(cursor, config.maxDepth);
        u16.ser(cursor, config.maxEntries);
        u32.ser(cursor, config.maxBytes);
    },

    des(cursor) {
        const maxBytes = u32.des(cursor);
        const maxEntries = u16.des(cursor);
        const maxDepth = u8.des(cursor);
        const bindables = boolean.des(cursor);
        const incoming = boolean.des(cursor);
        const outgoing = boolean.des(cursor);
        return { enabled: boolean.des(cursor), outgoing, incoming, bindables, maxDepth, maxEntries, maxBytes };
    },
};
