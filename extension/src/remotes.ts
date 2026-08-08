import * as vscode from "vscode";
import type { BrokerManager } from "./broker";
import type { InstanceRef, RemoteCall, ServerMessage, SessionId } from "./broker/message";

/// How many calls one remote keeps before the oldest are dropped; the broker
/// buffers far more across every remote.
const CALLS_PER_REMOTE = 500;

/// Every call that crossed one remote, newest first.
export interface RemoteGroup {
    /// Keyed by path, not id: an instance with no readable `UniqueId` reports
    /// `""`, and every one of those would collide.
    key: string;
    remote: InstanceRef;
    /// Newest first, so the tree shows the latest call without re-sorting.
    calls: TrackedCall[];
    /// Total seen, including any the cap above has since dropped.
    total: number;
    /// The `session:id` of every call still in `calls`. A batch broadcast while
    /// we were asking for the history arrives twice, live and in the replay, and
    /// the tree view rejects two rows under one id.
    seen: Set<string>;
}

/// One call, with the session it came from; a group can span sessions.
export interface TrackedCall {
    session: SessionId;
    call: RemoteCall;
}

/// The calls the broker has relayed, grouped by the remote they crossed. A view
/// of the broker's buffer, not a second copy: a `RemotesCleared` empties it too.
export class RemoteRegistry implements vscode.Disposable {
    private readonly groups = new Map<string, RemoteGroup>();
    private readonly subscriptions: vscode.Disposable[] = [];

    private readonly _onDidChange = new vscode.EventEmitter<void>();
    /// Fires when a batch lands, or when the history is dropped.
    readonly onDidChange = this._onDidChange.event;

    constructor(broker: BrokerManager, private readonly log: vscode.LogOutputChannel) {
        this.subscriptions.push(
            broker.onMessage((message) => this.track(message)),
            broker.onConnectionChanged((connected) => {
                if (connected) {
                    return;
                }

                // The buffer belonged to a broker we are no longer talking to.
                this.log.info("Remotes: broker connection dropped, forgetting the captured calls");
                this.clear();
            }),
        );
    }

    dispose() {
        for (const subscription of this.subscriptions) {
            subscription.dispose();
        }
        this._onDidChange.dispose();
    }

    /// Every remote that has been seen, busiest first.
    list(): RemoteGroup[] {
        return [...this.groups.values()].sort((a, b) => b.total - a.total);
    }

    group(key: string): RemoteGroup | undefined {
        return this.groups.get(key);
    }

    /// How many calls are on screen, and how many crossed in total.
    get counts(): { shown: number; total: number } {
        let shown = 0;
        let total = 0;
        for (const group of this.groups.values()) {
            shown += group.calls.length;
            total += group.total;
        }
        return { shown, total };
    }

    /// Drop everything, for a `RemotesCleared` or a lost connection.
    clear() {
        if (this.groups.size === 0) {
            return;
        }
        this.groups.clear();
        this._onDidChange.fire();
    }

    private track(message: ServerMessage) {
        if (message.type === "RemotesCleared") {
            this.log.info("Remotes: the broker dropped its history");
            this.clear();
            return;
        }
        if (message.type !== "SessionRemotes") {
            return;
        }

        const { id, calls } = message.content;
        for (const call of calls) {
            this.remember(id, call);
        }

        this.log.debug(`Remotes: session ${id} captured ${calls.length} call(s)`);
        this._onDidChange.fire();
    }

    private remember(session: SessionId, call: RemoteCall) {
        const key = call.remote.path;
        let group = this.groups.get(key);
        if (!group) {
            group = { key, remote: call.remote, calls: [], total: 0, seen: new Set() };
            this.groups.set(key, group);
        }

        const seenKey = `${session}:${call.id}`;
        if (group.seen.has(seenKey)) {
            return;
        }
        group.seen.add(seenKey);

        // A renamed remote keeps its group but reports itself as it is now.
        group.remote = call.remote;
        group.total += 1;
        group.calls.unshift({ session, call });
        for (const dropped of group.calls.splice(CALLS_PER_REMOTE)) {
            group.seen.delete(`${dropped.session}:${dropped.call.id}`);
        }
    }
}
