// Drives a real broker process over WebSockets with the extension's codecs, so
// a drift the byte pins can't see (a variant the broker rejects, a field it
// reads back differently) shows up as a failing exchange.
import assert from "node:assert/strict";
import { spawn, type ChildProcess } from "node:child_process";
import * as fs from "node:fs";
import * as os from "node:os";
import * as path from "node:path";
import { after, before, describe, it } from "node:test";
import {
    decodeServer,
    encodeClient,
    type ClientMessage,
    type DomValue,
    type ServerMessage,
} from "../../broker/message";

/**
 * A cargo build first, falling back to the copy esbuild stages for packaging —
 * this suite exists to check the broker in this repo, and the staged copy is
 * only refreshed on a bundle, so preferring it would test a stale binary.
 *
 * Anchored on the working directory rather than `__dirname`, which points at the
 * bundle under `dist/` by the time this runs.
 */
function brokerBinary(): string | undefined {
    const exe = process.platform === "win32" ? "broker.exe" : "broker";
    const extension = process.cwd();
    const candidates = [
        path.join(extension, "..", "broker", "target", "debug", exe),
        path.join(extension, "..", "broker", "target", "release", exe),
        path.join(extension, "bin", `${process.platform}-${process.arch}`, exe),
    ];
    return candidates.find((candidate) => fs.existsSync(candidate));
}

/** A broker connection that queues decoded messages so a test can await them in order. */
class TestClient {
    private readonly queue: ServerMessage[] = [];
    private waiter?: (message: ServerMessage) => void;
    readonly decodeErrors: string[] = [];

    private constructor(private readonly ws: WebSocket) {
        ws.addEventListener("message", (ev: MessageEvent) => {
            if (typeof ev.data !== "string") {
                this.decodeErrors.push(`non-text frame (${typeof ev.data})`);
                return;
            }
            let message: ServerMessage;
            try {
                message = decodeServer(ev.data);
            } catch (err) {
                this.decodeErrors.push(`${String(err)} decoding ${ev.data}`);
                return;
            }
            const waiter = this.waiter;
            if (waiter) {
                this.waiter = undefined;
                waiter(message);
            } else {
                this.queue.push(message);
            }
        });
    }

    static open(url: string): Promise<TestClient> {
        return new Promise((resolve, reject) => {
            const ws = new WebSocket(url);
            const client = new TestClient(ws);
            ws.addEventListener("open", () => resolve(client), { once: true });
            ws.addEventListener("error", () => reject(new Error(`could not connect to ${url}`)), { once: true });
        });
    }

    send(message: ClientMessage): void {
        this.ws.send(encodeClient(message));
    }

    next(label: string, timeoutMs = 5_000): Promise<ServerMessage> {
        const queued = this.queue.shift();
        if (queued) {
            return Promise.resolve(queued);
        }
        return new Promise((resolve, reject) => {
            const timer = setTimeout(() => {
                this.waiter = undefined;
                reject(new Error(`timed out waiting for ${label}`));
            }, timeoutMs);
            this.waiter = (message) => {
                clearTimeout(timer);
                resolve(message);
            };
        });
    }

    close(): void {
        this.ws.close();
    }
}

function sleep(ms: number): Promise<void> {
    return new Promise((resolve) => setTimeout(resolve, ms));
}

/** Every `DomValue` variant with a plausible Roblox-side value, to prove the broker parses them all. */
function everyDomValue(): Map<string, DomValue> {
    return new Map<string, DomValue>([
        ["Anchored", { type: "Bool", content: true }],
        ["Mass", { type: "Float", content: 12.5 }],
        ["Count", { type: "Int", content: -4503599627370496 }],
        ["Label", { type: "String", content: "Brick ✓ ünïcode" }],
        ["PrimaryPart", { type: "Ref", content: "root" }],
        ["Material", { type: "Enum", content: { family: 3, value: 288 } }],
        ["Offset2", { type: "Vector2", content: { X: 1, Y: -2.5 } }],
        ["Size", { type: "Vector3", content: { X: 4, Y: 1.2, Z: -8 } }],
        ["Color", { type: "Color3", content: { R: 1, G: 0.5, B: 0 } }],
        ["Pad", { type: "UDim", content: { Scale: 0.25, Offset: 6 } }],
        ["Position", { type: "UDim2", content: { X: { Scale: 0.5, Offset: 10 }, Y: { Scale: 0, Offset: -4 } } }],
        ["Lifetime", { type: "NumberRange", content: { Min: 0.5, Max: 2 } }],
        ["Slice", { type: "Rect", content: { Min: { X: 0, Y: 0 }, Max: { X: 16, Y: 16 } } }],
        ["Brick", { type: "BrickColor", content: { Number: 194 } }],
        ["CFrame", { type: "CFrame", content: [1, 2, 3, 1, 0, 0, 0, 1, 0, 0, 0, 1] }],
        ["Alpha", { type: "Float32", content: 0.25 }],
        ["Layer", { type: "Int32", content: -7 }],
        ["Texture", { type: "ContentId", content: "rbxassetid://42" }],
        ["Blob", { type: "BinaryString", content: new Uint8Array([0, 127, 255]) }],
        ["Color8", { type: "Color3uint8", content: { R: 1, G: 0, B: 0.25 } }],
        ["Grid2", { type: "Vector2int16", content: { X: 3, Y: -4 } }],
        ["Grid3", { type: "Vector3int16", content: { X: 3, Y: -4, Z: 5 } }],
        ["Aim", { type: "Ray", content: { Origin: { X: 0, Y: 0, Z: 0 }, Direction: { X: 0, Y: 1, Z: 0 } } }],
        ["Bounds", { type: "Region3", content: { Position: { X: 0, Y: 0, Z: 0 }, Size: { X: 2, Y: 4, Z: 6 } } }],
        ["Chunk", { type: "Region3int16", content: { Min: { X: -1, Y: -2, Z: -3 }, Max: { X: 1, Y: 2, Z: 3 } } }],
        [
            "Axes",
            {
                type: "Axes",
                content: { X: true, Y: false, Z: true, Top: false, Bottom: false, Left: true, Right: true, Back: true, Front: true },
            },
        ],
        ["Faces", { type: "Faces", content: { Back: false, Bottom: false, Front: true, Left: false, Right: false, Top: true } }],
        ["Font", { type: "Font", content: { Family: "rbxasset://fonts/families/Arial.json", Weight: 700, Style: 1 } }],
        [
            "Widths",
            {
                type: "NumberSequence",
                content: [
                    { Time: 0, Value: 1, Envelope: 0 },
                    { Time: 1, Value: 0.5, Envelope: 0 },
                ],
            },
        ],
        ["Tint", { type: "ColorSequence", content: [{ Time: 0, Value: { R: 1, G: 0, B: 0 } }] }],
        [
            "Physics",
            {
                type: "PhysicalProperties",
                content: {
                    Density: 0.7, Friction: 0.3, Elasticity: 0.5,
                    FrictionWeight: 1, ElasticityWeight: 1, AcousticAbsorption: 0.25,
                },
            },
        ],
        ["Pivot", { type: "OptionalCFrame", content: undefined }],
        ["Asset", { type: "Content", content: { type: "Uri", content: "rbxassetid://123" } }],
    ]);
}

const binary = brokerBinary();

describe(
    "live broker",
    { skip: binary ? false : "no broker binary — run `cargo build` in broker/" },
    () => {
        let child: ChildProcess;
        let stderr = "";
        let control: TestClient;
        let client: TestClient;
        let sessionId = -1;

        before(async () => {
            child = spawn(binary!, ["run", "--no-verde", "--no-luau-lsp", "--handshake=stdout"], {
                stdio: ["ignore", "pipe", "pipe"],
                env: {
                    ...process.env,
                    // The assertions below read the broker's own decode logging.
                    RUST_LOG: "debug",
                    // The lockfile lives under the temp dir, so a private one keeps
                    // this broker from attaching to a developer's running instance
                    // (which would leave the assertions reading the wrong stderr).
                    TMPDIR: fs.mkdtempSync(path.join(os.tmpdir(), "roprobe-test-")),
                },
            });
            child.stderr!.on("data", (chunk: Buffer) => (stderr += chunk.toString()));

            const handshake = await new Promise<{ port: number; token: string }>((resolve, reject) => {
                let buffered = "";
                const timer = setTimeout(() => reject(new Error("no handshake on stdout")), 10_000);
                child.stdout!.on("data", (chunk: Buffer) => {
                    buffered += chunk.toString();
                    const line = buffered.split("\n").find((candidate) => candidate.trim().startsWith("{"));
                    if (line) {
                        clearTimeout(timer);
                        resolve(JSON.parse(line));
                    }
                });
            });

            const base = `ws://127.0.0.1:${handshake.port}/?token=${encodeURIComponent(handshake.token)}`;
            // Control first, so it witnesses the syncing client joining.
            control = await TestClient.open(`${base}&control=1`);
            client = await TestClient.open(base);
        });

        after(async () => {
            control?.send({ type: "Shutdown" });
            await sleep(300);
            if (child?.exitCode === null) {
                child.kill();
            }
        });

        it("greets a syncing client with Hello", async () => {
            assert.equal((await client.next("Hello")).type, "Hello");
        });

        it("announces the new session on the control connection", async () => {
            const added = await control.next("NewSession");
            assert.equal(added.type, "NewSession");
            sessionId = added.type === "NewSession" ? added.content : -1;
        });

        it("round-trips SetUpstream through UpstreamChanged", async () => {
            client.send({ type: "SetUpstream", content: { upstream: "Verde", enabled: true } });
            const enabled = await client.next("UpstreamChanged");
            assert.deepEqual(enabled, { type: "UpstreamChanged", content: { upstream: "Verde", enabled: true } });

            client.send({ type: "SetUpstream", content: { upstream: "LuauLsp", enabled: false } });
            const disabled = await client.next("UpstreamChanged");
            assert.deepEqual(disabled, { type: "UpstreamChanged", content: { upstream: "LuauLsp", enabled: false } });
        });

        it("parses an enum catalog and a patch carrying every DomValue variant", async () => {
            client.send({
                type: "EnumFamilies",
                content: [
                    { name: "Enum.Material", items: [{ name: "Plastic", value: 256 }, { name: "Neon", value: 288 }] },
                    { name: "Enum.PartType", items: [{ name: "Ball", value: 0 }] },
                ],
            });
            client.send({
                type: "UpdateDom",
                content: {
                    upserts: [
                        {
                            id: "root", parent: undefined, class: "Folder", name: "Stuff", hasChildren: true,
                            properties: new Map(), attributes: new Map(), tags: undefined,
                        },
                        {
                            id: "part", parent: "root", class: "Part", name: "Brick", hasChildren: false,
                            properties: everyDomValue(),
                            attributes: new Map<string, DomValue>([["Health", { type: "Float", content: 50 }]]),
                            tags: ["Enemy", "Spawner"],
                        },
                    ],
                    removals: ["gone"],
                    updates: [
                        {
                            id: "part",
                            properties: new Map<string, DomValue | undefined>([["Transparency", { type: "Float", content: 0.5 }]]),
                            attributes: new Map<string, DomValue | undefined>([["Health", undefined]]),
                            tags: { type: "Delta", content: { add: ["Tagged"], remove: ["Enemy"] } },
                        },
                    ],
                },
            });
            await sleep(500);

            assert.match(stderr, /client sent enum catalog.*count=2/);
            assert.match(stderr, /client dom patch.*upserts=2 removals=1/);
        });

        it("services RequestActive without wedging the sessions lock", async () => {
            client.send({ type: "RequestActive" });
            await sleep(300);
            assert.match(stderr, /client requested the active slot/);

            // Before the PostHandle fix this deadlocked, and the reply below never arrived.
            control.send({ type: "ListSessions" });
            assert.equal((await control.next("Sessions")).type, "Sessions");
        });

        it("reports the session over the control connection", async () => {
            control.send({ type: "ListSessions" });
            const listed = await control.next("Sessions");
            assert.equal(listed.type, "Sessions");
            if (listed.type !== "Sessions") {
                return;
            }

            const info = listed.content.find((session) => session.id === sessionId);
            assert.ok(info, `session ${sessionId} missing from ${JSON.stringify(listed.content)}`);
            assert.ok(info.peer.startsWith("127.0.0.1"), info.peer);
            assert.equal(info.active, true);
            assert.equal(info.securityLevel, 2, "the LocalUser default");
        });

        it("applies SwapActive and SetSecurity", async () => {
            control.send({ type: "SwapActive", content: sessionId });
            control.send({ type: "SetSecurity", content: { id: sessionId, level: 3 } });
            await sleep(300);

            control.send({ type: "ListSessions" });
            const listed = await control.next("Sessions");
            assert.equal(listed.type, "Sessions");
            if (listed.type !== "Sessions") {
                return;
            }

            const info = listed.content.find((session) => session.id === sessionId);
            assert.equal(info?.active, true);
            assert.equal(info?.securityLevel, 3);
        });

        it("relays a console batch to the control connection", async () => {
            client.send({
                type: "Log",
                content: [
                    { level: "print", content: "hello" },
                    { level: "error", content: "boom" },
                ],
            });

            const relayed = await control.next("SessionLog");
            assert.deepEqual(relayed, {
                type: "SessionLog",
                content: {
                    id: sessionId,
                    entries: [
                        { level: "print", content: "hello" },
                        { level: "error", content: "boom" },
                    ],
                },
            });
        });

        it("replays the console history only when asked", async () => {
            // Nothing arrives unbidden: the live batch above was the last push.
            control.send({ type: "ListSessions" });
            assert.equal((await control.next("Sessions")).type, "Sessions");

            control.send({ type: "RequestLogs" });
            const replayed = await control.next("SessionLog");
            assert.deepEqual(replayed, {
                type: "SessionLog",
                content: {
                    id: sessionId,
                    entries: [
                        { level: "print", content: "hello" },
                        { level: "error", content: "boom" },
                    ],
                },
            });
        });

        it("announces the removal when the client disconnects", async () => {
            client.close();
            const removed = await control.next("RemoveSession");
            assert.deepEqual(removed, { type: "RemoveSession", content: sessionId });
        });

        it("never logged a frame it could not decode", () => {
            assert.doesNotMatch(stderr, /undecodable/);
            assert.doesNotMatch(stderr, /non-base64/);
            assert.doesNotMatch(stderr, /unexpected message/);
            assert.deepEqual([...client.decodeErrors, ...control.decodeErrors], []);
        });
    },
);
