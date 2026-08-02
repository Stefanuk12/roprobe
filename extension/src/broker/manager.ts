import * as vscode from "vscode";
import { spawn, type ChildProcess } from "node:child_process";
import * as fs from "node:fs";
import * as os from "node:os";
import * as path from "node:path";
import { decodeServer, encodeClient, type ClientMessage, type ServerMessage } from "./message";

interface BrokerHandshake {
    port: number;
    token: string;
    pid?: number;
}

/** Cross-process runtime state, so a broker started elsewhere is discoverable. */
const LOCKFILE = path.join(os.tmpdir(), "roprobe", "broker.json");

const HANDSHAKE_TIMEOUT_MS = 10_000;
const CONNECT_TIMEOUT_MS = 5_000;
const RECONNECT_DELAY_MS = 1_000;

function brokerBinaryPath(ctx: vscode.ExtensionContext): string {
    const exe = process.platform === "win32" ? "broker.exe" : "broker";
    const p = ctx.asAbsolutePath(path.join("bin", `${process.platform}-${process.arch}`, exe));

    if (process.platform !== "win32") {
        try {
            fs.chmodSync(p, 0o755); // VSIX extraction can drop the executable bit
        } catch {
            // already executable, or missing — a missing binary surfaces on spawn
        }
    }

    return p;
}

export class BrokerManager implements vscode.Disposable {
    private ws?: WebSocket;
    private child?: ChildProcess;
    private spawnedHere = false;
    private disposed = false;
    private reconnectTimer?: ReturnType<typeof setTimeout>;
    private readonly log: vscode.OutputChannel;

    private readonly _onMessage = new vscode.EventEmitter<ServerMessage>();
    /// Fires for every decoded message the broker sends.
    readonly onMessage = this._onMessage.event;

    constructor(private readonly ctx: vscode.ExtensionContext) {
        this.log = vscode.window.createOutputChannel("roprobe");
    }

    /// Attach or spawn a broker.
    async start(): Promise<void> {
        if (this.disposed || this.ws) {
            return;
        }

        // Check if there's a broker already via lockfile.
        const existing = this.readLockfile();
        if (existing) {
            try {
                await this.connect(existing.port, existing.token);
                this.spawnedHere = false;
                this.log.appendLine(`Attached to existing broker on :${existing.port}`);
                return;
            } catch (err) {
                this.log.appendLine(`Could not attach to lockfile broker (${String(err)}); spawning a new one`);
                this.removeLockfile();
            }
        }

        // There isn't, make a new one.
        const handshake = await this.spawnAndHandshake();
        await this.connect(handshake.port, handshake.token);
        this.spawnedHere = true;
        this.log.appendLine(`Spawned broker (pid ${this.child?.pid}) on :${handshake.port}`);
    }

    /// Send a message to the broker.
    ///
    /// NOTE: does nothing, if not connected.
    send(message: ClientMessage): void {
        if (this.ws && this.ws.readyState === WebSocket.OPEN) {
            this.ws.send(encodeClient(message));
        } else {
            this.log.appendLine(`Dropping ${message.type}: broker not connected`);
        }
    }

    /// Kill the current broker, and spawn a new one.
    async restart(): Promise<void> {
        this.teardownConnection();
        this.killOwnedChild();
        await this.start();
    }

    dispose(): void {
        this.disposed = true;
        if (this.reconnectTimer) {
            clearTimeout(this.reconnectTimer);
            this.reconnectTimer = undefined;
        }
        this.teardownConnection();
        
        // Only stop the broker if we started it — a shared one outlives this window.
        this.killOwnedChild();
        this._onMessage.dispose();
        this.log.dispose();
    }

    private spawnAndHandshake(): Promise<BrokerHandshake> {
        const bin = brokerBinaryPath(this.ctx);
        let child: ChildProcess;
        try {
            child = spawn(bin, ["--handshake=stdout"], { stdio: ["ignore", "pipe", "pipe"] });
        } catch (err) {
            return Promise.reject(new Error(`failed to launch broker at ${bin}: ${String(err)}`));
        }
        this.child = child;
        child.stderr?.on("data", (d: Buffer) => this.log.append(`[broker] ${d}`));
        child.on("exit", (code, signal) => this.log.appendLine(`Broker exited (code=${code}, signal=${signal})`));

        return new Promise<BrokerHandshake>((resolve, reject) => {
            let buf = "";
            let settled = false;
            const finish = (fn: () => void) => {
                if (settled) {
                    return;
                }
                settled = true;
                clearTimeout(timer);
                fn();
            };
            const timer = setTimeout(
                () => finish(() => reject(new Error("timed out waiting for broker handshake on stdout"))),
                HANDSHAKE_TIMEOUT_MS,
            );

            const onData = (chunk: Buffer) => {
                buf += chunk.toString("utf8");
                let nl: number;
                while ((nl = buf.indexOf("\n")) >= 0) {
                    const line = buf.slice(0, nl).trim();
                    buf = buf.slice(nl + 1);
                    if (!line.startsWith("{")) {
                        if (line) {
                            this.log.appendLine(`[broker] ${line}`);
                        }
                        continue;
                    }
                    try {
                        const hs = JSON.parse(line) as BrokerHandshake;
                        if (typeof hs.port === "number" && typeof hs.token === "string") {
                            child.stdout?.off("data", onData);
                            // Forward any further stdout to the log.
                            child.stdout?.on("data", (d: Buffer) => this.log.append(`[broker] ${d}`));
                            finish(() => resolve(hs));
                            return;
                        }
                    } catch {
                        // not the handshake line; keep reading
                    }
                }
            };

            child.stdout?.on("data", onData);
            child.on("error", (err) => finish(() => reject(new Error(`failed to launch broker at ${bin}: ${err.message}`))));
            child.once("exit", () => finish(() => reject(new Error("broker exited before handshake"))));
        });
    }

    private connect(port: number, token: string): Promise<void> {
        // The standard WebSocket client API can't set headers, so auth rides in the query string.
        const url = `ws://127.0.0.1:${port}/?token=${encodeURIComponent(token)}`;
        return new Promise<void>((resolve, reject) => {
            const ws = new WebSocket(url);
            const timer = setTimeout(() => {
                reject(new Error("connection timed out"));
                try {
                    ws.close();
                } catch {
                    // ignore
                }
            }, CONNECT_TIMEOUT_MS);

            ws.addEventListener(
                "open",
                () => {
                    clearTimeout(timer);
                    this.ws = ws;
                    this.log.appendLine("Broker connection open");
                    ws.addEventListener("message", (ev: MessageEvent) => this.receive(ev.data));
                    ws.addEventListener("close", () => {
                        this.log.appendLine("Broker connection closed");
                        this.ws = undefined;
                        this.scheduleReconnect();
                    });
                    resolve();
                },
                { once: true },
            );

            ws.addEventListener(
                "error",
                () => {
                    clearTimeout(timer);
                    reject(new Error("connection error"));
                },
                { once: true },
            );
        });
    }

    /// Decode one inbound frame and fan it out, dropping (with a log line) anything malformed.
    private receive(data: unknown): void {
        // The broker only ever sends base64 text frames — the executor's socket rejects binary ones.
        if (typeof data !== "string") {
            this.log.appendLine(`Dropping non-text frame (${typeof data})`);
            return;
        }

        let message: ServerMessage;
        try {
            message = decodeServer(data);
        } catch (err) {
            this.log.appendLine(`Dropping undecodable frame: ${String(err)}`);
            return;
        }

        this._onMessage.fire(message);
    }

    private scheduleReconnect(): void {
        if (this.disposed || this.reconnectTimer) {
            return;
        }
        this.reconnectTimer = setTimeout(() => {
            this.reconnectTimer = undefined;
            this.start().catch((err) => {
                this.log.appendLine(`Reconnect failed: ${String(err)}`);
                this.scheduleReconnect();
            });
        }, RECONNECT_DELAY_MS);
    }

    private teardownConnection(): void {
        if (this.ws) {
            try {
                this.ws.close();
            } catch {
                // ignore
            }
            this.ws = undefined;
        }
    }

    private killOwnedChild(): void {
        if (this.spawnedHere && this.child) {
            this.child.kill();
            this.child = undefined;
        }
    }

    private readLockfile(): BrokerHandshake | undefined {
        try {
            const hs = JSON.parse(fs.readFileSync(LOCKFILE, "utf8")) as BrokerHandshake;
            if (typeof hs.port === "number" && typeof hs.token === "string") {
                return hs;
            }
        } catch {
            // missing or malformed
        }
        return undefined;
    }

    private removeLockfile(): void {
        try {
            fs.rmSync(LOCKFILE);
        } catch {
            // ignore
        }
    }
}
