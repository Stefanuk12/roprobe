// Checks that a captured call renders back into Luau that means what the client
// saw: this is where a wrong escape or a lost cycle shows up.
import assert from "node:assert/strict";
import { describe, it } from "node:test";
import type { LuaValue, RemoteCall } from "../../broker/message";
import { callDocument, callSnippet, instancePath, luauString, summarise } from "../../remote_format";

function bytes(text: string): Uint8Array {
    return new TextEncoder().encode(text);
}

/** The call `rendered` strips back off, so only the one argument is left. */
const PREFIX = "game:GetService(\"ReplicatedStorage\").Remotes.Fire:FireServer(";

/** One value on its own, as `callSnippet` renders it in an argument list. */
function rendered(value: LuaValue): string {
    return callSnippet(call([value])).slice(PREFIX.length, -1);
}

function call(args: LuaValue[], overrides: Partial<RemoteCall> = {}): RemoteCall {
    return {
        id: 0,
        direction: "Outgoing",
        remote: {
            id: "r",
            class: "RemoteEvent",
            name: "Fire",
            path: "ReplicatedStorage.Remotes.Fire",
        },
        method: "FireServer",
        arguments: args,
        returns: undefined,
        source: {
            script: undefined, functionName: undefined, chunk: undefined,
            line: undefined, isExecutor: false, actor: undefined,
        },
        timestamp: 0,
        ...overrides,
    };
}

describe("luauString", () => {
    it("keeps printable ASCII as it is", () => {
        assert.equal(luauString(bytes("hello world")), "\"hello world\"");
    });

    it("escapes the characters that would end or reinterpret the literal", () => {
        assert.equal(luauString(bytes("a\"b\\c")), "\"a\\\"b\\\\c\"");
        assert.equal(luauString(bytes("a\nb\tc")), "\"a\\nb\\tc\"");
    });

    it("escapes arbitrary bytes decimally rather than decoding them", () => {
        // A UTF-8 decode would replace these with U+FFFD and stop round-tripping.
        assert.equal(luauString(new Uint8Array([0, 128, 255])), "\"\\0\\128\\255\"");
    });

    it("escapes bytes that are valid UTF-8 but not ASCII", () => {
        assert.equal(luauString(bytes("é")), "\"\\195\\169\"");
    });

    it("pads an escape that a following digit would otherwise be read as part of", () => {
        // Luau takes up to three digits after a backslash, so "\5" before "1"
        // would read as byte 51 rather than byte 5 followed by the character 1.
        assert.equal(luauString(new Uint8Array([5, 0x31])), "\"\\0051\"");
        assert.equal(luauString(new Uint8Array([0, 0x39, 0])), "\"\\0009\\0\"");
    });
});

describe("instancePath", () => {
    it("roots a path at the service that holds it", () => {
        assert.equal(
            instancePath("ReplicatedStorage.Remotes.Fire"),
            "game:GetService(\"ReplicatedStorage\").Remotes.Fire",
        );
    });

    it("brackets a name that is not a bare identifier", () => {
        assert.equal(
            instancePath("ReplicatedStorage.my remote.2nd"),
            "game:GetService(\"ReplicatedStorage\")[\"my remote\"][\"2nd\"]",
        );
    });

    it("brackets a name that would read as a keyword", () => {
        assert.equal(instancePath("Workspace.end"), "game:GetService(\"Workspace\")[\"end\"]");
    });

    it("answers nil for a path with no service to root it", () => {
        assert.equal(instancePath(""), "nil");
    });
});

describe("callSnippet", () => {
    it("renders an outgoing call as the call that made it", () => {
        assert.equal(
            callSnippet(call([{ type: "Number", content: 1 }, { type: "String", content: bytes("x") }])),
            "game:GetService(\"ReplicatedStorage\").Remotes.Fire:FireServer(1, \"x\")",
        );
    });

    it("renders an incoming call as what arrived, since it is not one you can make", () => {
        const incoming = callSnippet(call([{ type: "Bool", content: true }], {
            direction: "Incoming",
            method: "OnClientEvent",
        }));
        assert.match(incoming, /^-- RemoteEvent\.OnClientEvent delivered:/);
        assert.match(incoming, /local arguments = table\.pack\(true\)/);
    });

    it("keeps the numbers Luau spells out in words", () => {
        assert.equal(rendered({ type: "Number", content: Infinity }), "math.huge");
        assert.equal(rendered({ type: "Number", content: -Infinity }), "-math.huge");
        assert.equal(rendered({ type: "Number", content: NaN }), "0/0");
    });

    it("rebuilds a buffer argument from its bytes", () => {
        assert.equal(
            rendered({ type: "Buffer", content: new Uint8Array([1, 2]) }),
            "buffer.fromstring(\"\\1\\2\")",
        );
    });

    it("renders a Roblox datatype as the constructor that rebuilds it", () => {
        assert.equal(
            rendered({ type: "Roblox", content: { type: "Vector3", content: { X: 1, Y: 2, Z: 3 } } }),
            "Vector3.new(1, 2, 3)",
        );
    });

    it("comments out what cannot be reproduced rather than emitting something that runs", () => {
        assert.equal(
            rendered({ type: "Function", content: { name: "send", chunk: "Main", line: 12 } }),
            "--[[ function send @ Main:12 ]] nil",
        );
        assert.equal(rendered({ type: "Opaque", content: "thread" }), "--[[ thread ]] nil");
    });

    it("renders an instance argument as the path that finds it again", () => {
        assert.equal(
            rendered({
                type: "Instance",
                content: { id: "p", class: "Part", name: "Brick", path: "Workspace.Brick" },
            }),
            "game:GetService(\"Workspace\").Brick",
        );
    });

    it("writes a table's array part in order and its string keys bare", () => {
        const snippet = callSnippet(call([
            {
                type: "Table",
                content: {
                    id: 0,
                    array: [{ type: "Number", content: 1 }, { type: "Number", content: 2 }],
                    entries: [
                        { key: { type: "String", content: bytes("name") }, value: { type: "String", content: bytes("x") } },
                        { key: { type: "String", content: bytes("two words") }, value: { type: "Bool", content: true } },
                        { key: { type: "Number", content: 7 }, value: { type: "Nil" } },
                    ],
                    truncated: false,
                    metatable: false,
                },
            },
        ]));

        // A multi-line argument puts the whole list on its own indented lines.
        assert.match(snippet, /:FireServer\(\n {4}\{\n/);
        assert.match(snippet, /\n {4}\}\n\)$/);
        assert.match(snippet, /^ {8}1,$/m);
        assert.match(snippet, /^ {8}2,$/m);
        assert.match(snippet, /^ {8}name = "x",$/m);
        assert.match(snippet, /^ {8}\["two words"\] = true,$/m);
        assert.match(snippet, /^ {8}\[7\] = nil,$/m);
    });

    it("renders an empty table without wrapping it over lines", () => {
        assert.equal(
            rendered({
                type: "Table",
                content: { id: 0, array: [], entries: [], truncated: false, metatable: false },
            }),
            "{}",
        );
    });

    it("says so when the capture caps cut a table short", () => {
        const snippet = rendered({
            type: "Table",
            content: { id: 0, array: [{ type: "Nil" }], entries: [], truncated: true, metatable: true },
        });
        assert.match(snippet, /-- truncated: the capture caps cut this table short/);
        assert.match(snippet, /-- this table carried a metatable, which is not captured/);
    });

    it("breaks a cycle instead of recursing into it", () => {
        const snippet = rendered({
            type: "Table",
            content: {
                id: 4,
                array: [{ type: "Cycle", content: 4 }],
                entries: [],
                truncated: false,
                metatable: false,
            },
        });
        assert.match(snippet, /--\[\[ cyclic reference to table #4 \]\] nil/);
    });

    it("renders no arguments as an empty list", () => {
        assert.equal(
            callSnippet(call([])),
            "game:GetService(\"ReplicatedStorage\").Remotes.Fire:FireServer()",
        );
    });
});

describe("summarise", () => {
    it("collapses a call's arguments onto one line", () => {
        assert.equal(
            summarise(call([{ type: "Number", content: 1 }, { type: "Bool", content: false }])),
            "(1, false)",
        );
        assert.equal(summarise(call([])), "()");
    });

    it("truncates a long argument rather than widening the row", () => {
        const long = summarise(call([{ type: "String", content: bytes("x".repeat(200)) }]));
        assert.ok(long.length <= 30, long);
        assert.ok(long.endsWith("…)"), long);
    });
});

describe("callDocument", () => {
    it("heads the document with where the call came from", () => {
        const document = callDocument(
            call([], {
                source: {
                    script: "Players.Builder.PlayerScripts.Main",
                    functionName: "send",
                    chunk: "Main",
                    line: 40,
                    isExecutor: false,
                    actor: undefined,
                },
                timestamp: 12.5,
            }),
            "Builder Man",
        );

        assert.match(document, /-- RemoteEvent Fire \(client -> server\)/);
        assert.match(document, /-- path {6}ReplicatedStorage\.Remotes\.Fire/);
        assert.match(document, /-- session {3}Builder Man/);
        assert.match(document, /-- captured {2}12\.500s/);
        assert.match(document, /-- script {4}Players\.Builder\.PlayerScripts\.Main/);
        assert.match(document, /-- caller {4}send @ Main:40/);
    });

    it("calls out a call our own executor made, which the game did not", () => {
        const document = callDocument(
            call([], {
                source: {
                    script: undefined, functionName: undefined, chunk: undefined,
                    line: undefined, isExecutor: true, actor: undefined,
                },
            }),
            "Builder Man",
        );
        assert.match(document, /this call came from an executor thread/);
    });

    it("names the actor a call was captured under", () => {
        const document = callDocument(
            call([], {
                source: {
                    script: undefined, functionName: undefined, chunk: undefined,
                    line: undefined, isExecutor: false, actor: "Workspace.Actor",
                },
            }),
            "Builder Man",
        );
        assert.match(document, /-- actor {5}Workspace\.Actor/);
    });

    it("shows what an invoke answered with", () => {
        const document = callDocument(
            call([], { method: "InvokeServer", returns: [{ type: "Bool", content: true }] }),
            "Builder Man",
        );
        assert.match(document, /-- answered with:/);
        assert.match(document, /local returned = table\.pack\(true\)/);
    });
});
