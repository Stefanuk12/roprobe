// Pins the broker's wire format against the TS codecs. Every expected byte
// vector here is copied from a `#[test]` in `broker/src/protocol/*.rs`, so this
// file failing means the two sides have drifted.
import assert from "node:assert/strict";
import { describe, it } from "node:test";
import type { SerDes } from "squash-ts";
import {
    clientMessage,
    domUpdate,
    domValue,
    fromBytes,
    luaTable,
    luaValue,
    operation,
    opResult,
    remoteCall,
    serverMessage,
    spyConfig,
    tagChange,
    toBytes,
    type CallSource,
    type CFrameComponents,
    type ClientMessage,
    type DomValue,
    type InstanceRef,
    type LuaTable,
    type LuaValue,
    type Operation,
    type OpResult,
    type RemoteCall,
    type ServerMessage,
    type SpyConfig,
    type TagChange,
} from "../../broker/message";

/** The byte of a one-character string literal, for readability in the vectors below. */
function b(char: string): number {
    return char.charCodeAt(0);
}

function f32le(...values: number[]): number[] {
    const out: number[] = [];
    for (const value of values) {
        const bytes = new Uint8Array(4);
        new DataView(bytes.buffer).setFloat32(0, value, true);
        out.push(...bytes);
    }
    return out;
}

/** Tuple-variant and array fields land on the wire reversed. */
function f32rev(values: number[]): number[] {
    return f32le(...[...values].reverse());
}

function i16rev(values: number[]): number[] {
    const out: number[] = [];
    for (const value of [...values].reverse()) {
        const bytes = new Uint8Array(2);
        new DataView(bytes.buffer).setInt16(0, value, true);
        out.push(...bytes);
    }
    return out;
}

function f64le(value: number): number[] {
    const bytes = new Uint8Array(8);
    new DataView(bytes.buffer).setFloat64(0, value, true);
    return [...bytes];
}

/** Assert the frame `value` encodes to, then that the same frame decodes and re-encodes identically. */
function pin<T>(serdes: SerDes<T>, value: T, expected: number[]): void {
    assert.deepEqual(Array.from(toBytes(serdes, value)), expected, "encoded frame");
    const decoded = fromBytes(serdes, new Uint8Array(expected));
    assert.deepEqual(Array.from(toBytes(serdes, decoded)), expected, "frame after a round-trip");
}

// Remote-spy fixtures, shared by the message pins and the shape pins below.

const REMOTE: InstanceRef = { id: "i", class: "C", name: "N", path: "P" };
const REMOTE_BYTES = [b("i"), 1, b("C"), 1, b("N"), 1, b("P"), 1];

const NO_SOURCE: CallSource = {
    script: undefined, functionName: undefined, chunk: undefined,
    line: undefined, isExecutor: false, actor: undefined,
};
const NO_SOURCE_BYTES = [0, 0, 0, 0, 0, 0];

const CALL: RemoteCall = {
    id: 1,
    direction: "Incoming",
    remote: REMOTE,
    method: "M",
    arguments: [{ type: "Nil" }],
    returns: undefined,
    source: NO_SOURCE,
    timestamp: 0,
};
const CALL_BYTES = [
    1, 0, 0, 0,                 // id (u32 LE)
    1,                          // direction (Outgoing=0, Incoming=1)
    ...REMOTE_BYTES,
    b("M"), 1,                  // method
    0, 1,                       // arguments: [Nil] + VLQ count
    0,                          // returns: None
    ...NO_SOURCE_BYTES,
    ...f64le(0),                // timestamp
];

/** Mirrors `SpyConfig::default()` on the broker side. */
const DEFAULT_SPY: SpyConfig = {
    enabled: false, outgoing: true, incoming: true, bindables: false,
    maxDepth: 6, maxEntries: 128, maxBytes: 4096,
};
const SPY_BYTES = [0, 1, 1, 0, 6, 128, 0, 0, 16, 0, 0];

describe("ClientMessage", () => {
    it("encodes unit variants as a single tag byte", () => {
        pin<ClientMessage>(clientMessage, { type: "Shutdown" }, [0]);
        pin<ClientMessage>(clientMessage, { type: "RequestActive" }, [5]);
        pin<ClientMessage>(clientMessage, { type: "ListSessions" }, [9]);
        pin<ClientMessage>(clientMessage, { type: "RequestLogs" }, [11]);
        pin<ClientMessage>(clientMessage, { type: "RequestRemotes" }, [13]);
        pin<ClientMessage>(clientMessage, { type: "ClearRemotes" }, [14]);
    });

    it("encodes a console batch", () => {
        // Vec elements reversed with the VLQ count last; each entry is a plain
        // struct written forward (level tag byte, then content).
        pin<ClientMessage>(
            clientMessage,
            {
                type: "Log",
                content: [
                    { level: "print", content: "a" },
                    { level: "error", content: "b" },
                ],
            },
            [3, b("b"), 1, 0, b("a"), 1, 2, 6],
        );
    });

    it("reverses struct-variant fields", () => {
        pin<ClientMessage>(clientMessage, { type: "SetUpstream", content: { upstream: "Verde", enabled: false } }, [0, 0, 1]);
        pin<ClientMessage>(
            clientMessage,
            { type: "OperationResult", content: { id: 1, result: { type: "Ok" } } },
            [0, 1, 0, 0, 0, 4],
        );
        // SessionId is a newtype over u32.
        pin<ClientMessage>(clientMessage, { type: "SwapActive", content: 7 }, [7, 0, 0, 0, 8]);
        pin<ClientMessage>(clientMessage, { type: "SetSecurity", content: { id: 7, level: 3 } }, [3, 7, 0, 0, 0, 10]);
        pin<ClientMessage>(
            clientMessage,
            { type: "RunCode", content: { session: 7, request: 1, source: "s" } },
            [b("s"), 1, 1, 0, 0, 0, 7, 0, 0, 0, 12],
        );
    });

    it("encodes a batch of captured remote calls", () => {
        pin<ClientMessage>(clientMessage, { type: "RemoteCalls", content: [CALL] }, [...CALL_BYTES, 1, 7]);
    });

    it("encodes the spy config", () => {
        pin<ClientMessage>(clientMessage, { type: "SetSpy", content: DEFAULT_SPY }, [...SPY_BYTES, 15]);
    });

    it("encodes the enum catalog", () => {
        pin<ClientMessage>(
            clientMessage,
            { type: "EnumFamilies", content: [{ name: "A", items: [{ name: "X", value: 1 }] }] },
            [b("A"), 1, b("X"), 1, 1, 0, 0, 0, 1, 1, 3],
        );
    });

    it("encodes a DOM patch", () => {
        pin<ClientMessage>(
            clientMessage,
            {
                type: "UpdateDom",
                content: {
                    upserts: [
                        {
                            id: "i", parent: "p", class: "C", name: "N", hasChildren: true,
                            properties: new Map<string, DomValue>([["K", { type: "Int", content: 7 }]]),
                            attributes: new Map<string, DomValue>([["A", { type: "Bool", content: true }]]),
                            tags: ["t"],
                        },
                        {
                            id: "j", parent: undefined, class: "D", name: "M", hasChildren: false,
                            properties: new Map(), attributes: new Map(), tags: undefined,
                        },
                    ],
                    removals: ["r", "s"],
                    updates: [],
                },
            },
            [
                // upserts: elements reversed, VLQ count last; each a plain struct in declaration order.
                b("j"), 1, 0, b("D"), 1, b("M"), 1, 0, 0, 0, 0,
                b("i"), 1, b("p"), 1, 1, b("C"), 1, b("N"), 1, 1,
                7, 0, 0, 0, 0, 0, 0, 0, 2, b("K"), 1, 1, // properties
                1, 0, b("A"), 1, 1, // attributes
                b("t"), 1, 1, 1, // tags
                2, // upserts count
                b("s"), 1, b("r"), 1, 2, // removals
                0, // updates count
                2, // ClientMessage tag
            ],
        );
    });
});

describe("DomUpdate", () => {
    it("removes a key with a None value and carries a tag delta", () => {
        pin<Parameters<typeof domUpdate.ser>[1]>(
            domUpdate,
            {
                id: "u",
                properties: new Map<string, DomValue | undefined>([["P", { type: "Bool", content: false }]]),
                attributes: new Map<string, DomValue | undefined>([["B", undefined]]),
                tags: { type: "Delta", content: { add: ["a"], remove: ["r"] } },
            },
            [b("u"), 1, 0, 0, 1, b("P"), 1, 1, 0, b("B"), 1, 1, b("r"), 1, 1, b("a"), 1, 1, 2],
        );
    });

    it("pins the other TagChange arms", () => {
        pin<TagChange>(tagChange, { type: "None" }, [0]);
        pin<TagChange>(tagChange, { type: "Replace", content: ["t"] }, [b("t"), 1, 1, 1]);
    });
});

describe("DomValue", () => {
    const identity: CFrameComponents = [1, 2, 3, 1, 0, 0, 0, 1, 0, 0, 0, 1];
    const cases: Array<[string, DomValue, number[]]> = [
        ["Ref", { type: "Ref", content: "9" }, [b("9"), 1, 4]],
        ["Enum", { type: "Enum", content: { family: 3, value: 7 } }, [7, 0, 0, 0, 3, 0, 5]],
        ["Vector2", { type: "Vector2", content: { X: 1, Y: 2.5 } }, [...f32rev([1, 2.5]), 6]],
        ["Vector3", { type: "Vector3", content: { X: 1, Y: 2.5, Z: -3 } }, [...f32rev([1, 2.5, -3]), 7]],
        ["Color3", { type: "Color3", content: { R: 128 / 255, G: 64 / 255, B: 1 } }, [255, 64, 128, 8]],
        ["UDim", { type: "UDim", content: { Scale: 0.5, Offset: 10 } }, [...f32le(10, 0.5), 9]],
        [
            "UDim2",
            { type: "UDim2", content: { X: { Scale: 0.5, Offset: 10 }, Y: { Scale: 0.25, Offset: -2 } } },
            [...f32le(-2, 0.25, 10, 0.5), 10],
        ],
        ["NumberRange", { type: "NumberRange", content: { Min: 1, Max: 2.5 } }, [...f32rev([1, 2.5]), 11]],
        [
            "Rect",
            { type: "Rect", content: { Min: { X: 0, Y: 0.5 }, Max: { X: 1, Y: 2.5 } } },
            [...f32rev([0, 0.5, 1, 2.5]), 12],
        ],
        ["BrickColor", { type: "BrickColor", content: { Number: 194 } }, [194, 0, 13]],
        ["CFrame", { type: "CFrame", content: [...identity] }, [...f32rev([...identity]), 14]],
        ["Float32", { type: "Float32", content: 1.5 }, [...f32le(1.5), 15]],
        ["Int32", { type: "Int32", content: -7 }, [249, 255, 255, 255, 16]],
        ["ContentId", { type: "ContentId", content: "r" }, [b("r"), 1, 17]],
        ["BinaryString", { type: "BinaryString", content: new Uint8Array([0, 255]) }, [0, 255, 2, 18]],
        ["Color3uint8", { type: "Color3uint8", content: { R: 1, G: 128 / 255, B: 0 } }, [0, 128, 255, 19]],
        ["Vector2int16", { type: "Vector2int16", content: { X: 1, Y: -2 } }, [...i16rev([1, -2]), 20]],
        ["Vector3int16", { type: "Vector3int16", content: { X: 1, Y: -2, Z: 3 } }, [...i16rev([1, -2, 3]), 21]],
        [
            "Ray",
            { type: "Ray", content: { Origin: { X: 1, Y: 2, Z: 3 }, Direction: { X: 0, Y: 1, Z: 0 } } },
            [...f32rev([1, 2, 3, 0, 1, 0]), 22],
        ],
        [
            "Region3",
            { type: "Region3", content: { Position: { X: 0, Y: 0, Z: 0 }, Size: { X: 2, Y: 4, Z: 6 } } },
            [...f32le(6, 4, 2, 0, 0, 0), 23],
        ],
        [
            "Region3int16",
            { type: "Region3int16", content: { Min: { X: -1, Y: -2, Z: -3 }, Max: { X: 1, Y: 2, Z: 3 } } },
            [...i16rev([-1, -2, -3, 1, 2, 3]), 24],
        ],
        [
            // Roblox couples the faces to the axes: Left/Right ride X, Back/Front ride Z.
            "Axes",
            {
                type: "Axes",
                content: { X: true, Y: false, Z: true, Top: false, Bottom: false, Left: true, Right: true, Back: true, Front: true },
            },
            [29, 5, 25],
        ],
        [
            "Faces",
            { type: "Faces", content: { Back: false, Bottom: false, Front: true, Left: false, Right: false, Top: true } },
            [36, 26],
        ],
        ["Font", { type: "Font", content: { Family: "X", Weight: 700, Style: 1 } }, [1, 188, 2, b("X"), 1, 27]],
        [
            "NumberSequence",
            {
                type: "NumberSequence",
                content: [
                    { Time: 0, Value: 1, Envelope: 0 },
                    { Time: 1, Value: 0.5, Envelope: 0 },
                ],
            },
            [...f32le(0.5, 0, 1), ...f32le(1, 0, 0), 2, 28],
        ],
        [
            // squash-rs's `fix/precision` fork writes the keypoint time as a full f32, not a quantised u8.
            "ColorSequence",
            { type: "ColorSequence", content: [{ Time: 128, Value: { R: 10 / 255, G: 20 / 255, B: 30 / 255 } }] },
            [30, 20, 10, ...f32le(128), 1, 29],
        ],
        ["PhysicalProperties (None)", { type: "PhysicalProperties", content: undefined }, [0, 30]],
        [
            "PhysicalProperties (Some)",
            {
                type: "PhysicalProperties",
                content: {
                    Density: 0.7, Friction: 0.3, Elasticity: 0.5,
                    FrictionWeight: 0.6, ElasticityWeight: 0.8, AcousticAbsorption: 0.9,
                },
            },
            [...f32rev([0.7, 0.3, 0.5, 0.6, 0.8, 0.9]), 1, 30],
        ],
        ["OptionalCFrame (None)", { type: "OptionalCFrame", content: undefined }, [0, 31]],
        ["OptionalCFrame (Some)", { type: "OptionalCFrame", content: [...identity] }, [...f32rev([...identity]), 1, 31]],
        ["Content::None", { type: "Content", content: { type: "None" } }, [0, 32]],
        ["Content::Uri (Some)", { type: "Content", content: { type: "Uri", content: "u" } }, [b("u"), 1, 1, 1, 32]],
        ["Content::Uri (None)", { type: "Content", content: { type: "Uri", content: undefined } }, [0, 1, 32]],
        ["Content::Object", { type: "Content", content: { type: "Object", content: "5" } }, [b("5"), 1, 2, 32]],
    ];

    for (const [label, value, expected] of cases) {
        it(`pins ${label}`, () => pin(domValue, value, expected));
    }
});

describe("ServerMessage", () => {
    const cases: Array<[string, ServerMessage, number[]]> = [
        ["RequestChildren (None bootstraps the top level)", { type: "RequestChildren", content: undefined }, [0, 2]],
        ["RequestChildren (Some)", { type: "RequestChildren", content: "9" }, [b("9"), 1, 1, 2]],
        ["RequestNode", { type: "RequestNode", content: "9" }, [b("9"), 1, 3]],
        ["RequestSnapshot (None)", { type: "RequestSnapshot", content: undefined }, [0, 4]],
        ["RequestSnapshot (Some)", { type: "RequestSnapshot", content: "9" }, [b("9"), 1, 1, 4]],
        ["Search", { type: "Search", content: { from: "a", query: "b" } }, [b("b"), 1, b("a"), 1, 5]],
        ["RequestNodes", { type: "RequestNodes", content: ["a", "b"] }, [b("b"), 1, b("a"), 1, 2, 6]],
        [
            "Operation",
            { type: "Operation", content: { id: 1, op: { type: "Delete", content: { node: "n" } } } },
            [b("n"), 1, 1, 1, 0, 0, 0, 7],
        ],
        ["Spy", { type: "Spy", content: DEFAULT_SPY }, [...SPY_BYTES, 8]],
        [
            "NewSession",
            { type: "NewSession", content: { id: 7, username: "N", peer: "p", active: true, securityLevel: 3 } },
            [7, 0, 0, 0, b("N"), 1, 1, b("p"), 1, 1, 3, 10],
        ],
        ["RemoveSession", { type: "RemoveSession", content: 7 }, [7, 0, 0, 0, 11]],
        [
            "Sessions",
            { type: "Sessions", content: [{ id: 7, username: "N", peer: "p", active: true, securityLevel: 3 }] },
            [7, 0, 0, 0, b("N"), 1, 1, b("p"), 1, 1, 3, 1, 9],
        ],
        [
            "Sessions (a client that did not name itself)",
            { type: "Sessions", content: [{ id: 7, username: undefined, peer: "p", active: true, securityLevel: 3 }] },
            [7, 0, 0, 0, 0, b("p"), 1, 1, 3, 1, 9],
        ],
        [
            "SessionLog",
            { type: "SessionLog", content: { id: 7, entries: [{ level: "warn", content: "a" }] } },
            [2, b("a"), 1, 1, 7, 0, 0, 0, 12],
        ],
        [
            "RunResult",
            { type: "RunResult", content: { session: 7, request: 1, result: { type: "Output", content: "x" } } },
            [b("x"), 1, 3, 1, 0, 0, 0, 7, 0, 0, 0, 13],
        ],
        [
            "SessionRemotes",
            { type: "SessionRemotes", content: { id: 7, calls: [CALL] } },
            [...CALL_BYTES, 1, 7, 0, 0, 0, 14],
        ],
        ["RemotesCleared", { type: "RemotesCleared" }, [15]],
    ];

    for (const [label, value, expected] of cases) {
        it(`pins ${label}`, () => pin(serverMessage, value, expected));
    }
});

describe("LuaValue", () => {
    const cases: Array<[string, LuaValue, number[]]> = [
        ["Nil", { type: "Nil" }, [0]],
        ["Bool", { type: "Bool", content: true }, [1, 1]],
        ["Number", { type: "Number", content: 1.5 }, [...f64le(1.5), 2]],
        // A string rides raw bytes + a VLQ length, so a non-UTF-8 payload survives.
        ["String", { type: "String", content: new Uint8Array([0, 255]) }, [0, 255, 2, 3]],
        ["Instance", { type: "Instance", content: REMOTE }, [...REMOTE_BYTES, 5]],
        // A Roblox datatype nests DomValue whole: its payload, its own tag, then ours.
        ["Roblox", { type: "Roblox", content: { type: "Bool", content: true } }, [1, 0, 6]],
        ["Buffer", { type: "Buffer", content: new Uint8Array([7]) }, [7, 1, 7]],
        [
            "Function",
            { type: "Function", content: { name: "f", chunk: undefined, line: undefined } },
            [b("f"), 1, 1, 0, 0, 8],
        ],
        ["Cycle", { type: "Cycle", content: 3 }, [3, 0, 0, 0, 9]],
        ["Opaque", { type: "Opaque", content: "thread" }, [...[..."thread"].map(b), 6, 10]],
    ];

    for (const [label, value, expected] of cases) {
        it(`pins ${label}`, () => pin(luaValue, value, expected));
    }

    it("pins a table, whose fields land forward with each Vec reversed and counted last", () => {
        const table: LuaTable = {
            id: 1,
            array: [{ type: "Bool", content: false }],
            entries: [{ key: { type: "String", content: new Uint8Array([b("k")]) }, value: { type: "Nil" } }],
            truncated: true,
            metatable: false,
        };
        pin(luaTable, table, [
            1, 0, 0, 0,         // id (u32 LE)
            0, 1,               // array[0]: Bool(false)
            1,                  // array VLQ count
            b("k"), 1, 3,       // entries[0].key: String("k")
            0,                  // entries[0].value: Nil
            1,                  // entries VLQ count
            1,                  // truncated
            0,                  // metatable
        ]);

        // Nested whole inside the Table variant, tag 4 last.
        pin<LuaValue>(
            luaValue,
            { type: "Table", content: { id: 0, array: [], entries: [], truncated: false, metatable: false } },
            [0, 0, 0, 0, 0, 0, 0, 0, 4],
        );
    });
});

describe("RemoteCall", () => {
    it("pins a call, a plain struct whose fields land forward", () => {
        pin(remoteCall, CALL, CALL_BYTES);
    });

    it("pins a call site, where a missing field collapses to a lone flag", () => {
        pin<RemoteCall>(
            remoteCall,
            {
                ...CALL,
                source: { ...NO_SOURCE, script: "s", line: 7, isExecutor: true },
                returns: [{ type: "Bool", content: true }],
            },
            [
                1, 0, 0, 0, 1, ...REMOTE_BYTES, b("M"), 1, 0, 1,
                1, 1, 1, 1,         // returns: Some([Bool(true)]) -> value, VLQ count, Some flag
                b("s"), 1, 1,       // script: Some
                0,                  // functionName: None
                0,                  // chunk: None
                7, 0, 0, 0, 1,      // line: Some(7)
                1,                  // isExecutor
                0,                  // actor: None
                ...f64le(0),
            ],
        );
    });

    it("pins the spy config", () => pin(spyConfig, DEFAULT_SPY, SPY_BYTES));
});

describe("Operation", () => {
    const cases: Array<[string, Operation, number[]]> = [
        ["Rename", { type: "Rename", content: { node: "n", name: "X" } }, [b("X"), 1, b("n"), 1, 0]],
        ["Move (no parent)", { type: "Move", content: { node: "n", parent: undefined } }, [0, b("n"), 1, 2]],
        [
            "SetProperty",
            { type: "SetProperty", content: { node: "n", name: "A", value: { type: "Bool", content: true } } },
            [1, 0, b("A"), 1, b("n"), 1, 6],
        ],
        ["GetProperties", { type: "GetProperties", content: { node: "n", properties: ["p"] } }, [b("p"), 1, 1, b("n"), 1, 10]],
        ["RunCode", { type: "RunCode", content: { source: "s" } }, [b("s"), 1, 11]],
    ];

    for (const [label, value, expected] of cases) {
        it(`pins ${label}`, () => pin(operation, value, expected));
    }
});

describe("OpResult", () => {
    const cases: Array<[string, OpResult, number[]]> = [
        ["Ok", { type: "Ok" }, [0]],
        ["Err", { type: "Err", content: "x" }, [b("x"), 1, 2]],
        ["Output", { type: "Output", content: "x" }, [b("x"), 1, 3]],
        [
            "Reads",
            {
                type: "Reads",
                content: {
                    properties: [{ name: "A", value: { type: "Bool", content: true } }],
                    tags: ["t"],
                    attributes: [],
                },
            },
            [b("A"), 1, 1, 0, 1, b("t"), 1, 1, 0, 1],
        ],
    ];

    for (const [label, value, expected] of cases) {
        it(`pins ${label}`, () => pin(opResult, value, expected));
    }
});
