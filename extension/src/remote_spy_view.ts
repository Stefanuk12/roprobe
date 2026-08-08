import * as vscode from "vscode";
import type { RemoteCall, SessionId } from "./broker/message";
import { callDocument, summarise } from "./remote_format";
import type { RemoteGroup, RemoteRegistry, TrackedCall } from "./remotes";
import { sessionLabel, type SessionRegistry } from "./sessions";

/// The scheme the detail view opens its documents under, so a captured call gets
/// a real editor — searchable, copyable, Luau-highlighted — not a webview.
export const REMOTE_SCHEME = "roprobe-remote";

type Node = RemoteGroup | TrackedCall;

function isGroup(node: Node): node is RemoteGroup {
    return "calls" in node;
}

/// The URI one call's detail document lives at, identified in the query so the
/// provider holds no second copy of the history.
function callUri(entry: TrackedCall): vscode.Uri {
    const { call, session } = entry;
    return vscode.Uri.from({
        scheme: REMOTE_SCHEME,
        // A `.luau` suffix is what picks the syntax highlighting.
        path: `/${call.remote.name}.${call.method}.luau`,
        query: `session=${session}&call=${call.id}`,
    });
}

/// Renders the captured calls as a two-level tree: the remotes, and the calls
/// that crossed each of them.
export class RemoteSpyView implements vscode.TreeDataProvider<Node>, vscode.TextDocumentContentProvider {
    private readonly subscriptions: vscode.Disposable[] = [];

    private readonly _onDidChangeTreeData = new vscode.EventEmitter<Node | undefined>();
    readonly onDidChangeTreeData = this._onDidChangeTreeData.event;

    private readonly _onDidChangeDocument = new vscode.EventEmitter<vscode.Uri>();
    readonly onDidChange = this._onDidChangeDocument.event;

    constructor(
        private readonly remotes: RemoteRegistry,
        private readonly sessions: SessionRegistry,
    ) {
        this.subscriptions.push(remotes.onDidChange(() => this._onDidChangeTreeData.fire(undefined)));
    }

    dispose() {
        for (const subscription of this.subscriptions) {
            subscription.dispose();
        }
        this._onDidChangeTreeData.dispose();
        this._onDidChangeDocument.dispose();
    }

    getChildren(node?: Node): Node[] {
        if (node === undefined) {
            return this.remotes.list();
        }
        return isGroup(node) ? node.calls : [];
    }

    getTreeItem(node: Node): vscode.TreeItem {
        return isGroup(node) ? this.groupItem(node) : this.callItem(node);
    }

    /// Render one call's detail document, or a note if the history moved on.
    provideTextDocumentContent(uri: vscode.Uri): string {
        const params = new URLSearchParams(uri.query);
        const session = Number(params.get("session"));
        const id = Number(params.get("call"));

        const entry = this.find(session, id);
        if (!entry) {
            return "-- this call has aged out of the buffer\n";
        }
        return callDocument(entry.call, this.labelFor(entry.session));
    }

    /// Open one call's detail document beside whatever is focused.
    async show(entry: TrackedCall): Promise<void> {
        const uri = callUri(entry);
        // A document already open on this URI would otherwise show what was
        // cached the first time.
        this._onDidChangeDocument.fire(uri);

        const document = await vscode.workspace.openTextDocument(uri);
        await vscode.window.showTextDocument(document, { preview: true, preserveFocus: false });
    }

    private groupItem(group: RemoteGroup): vscode.TreeItem {
        const item = new vscode.TreeItem(group.remote.name, vscode.TreeItemCollapsibleState.Collapsed);
        item.id = group.key;
        item.description = `${group.remote.class} · ${group.total}`;
        item.iconPath = new vscode.ThemeIcon(group.remote.class === "RemoteFunction" ? "arrow-swap" : "radio-tower");
        item.contextValue = "roprobe.remoteGroup";

        const dropped = group.total - group.calls.length;
        item.tooltip = new vscode.MarkdownString(
            [
                `**${group.remote.path}**`,
                "",
                `- class: \`${group.remote.class}\``,
                `- calls: ${group.total}${dropped > 0 ? ` (${dropped} aged out)` : ""}`,
            ].join("\n"),
        );
        return item;
    }

    private callItem(entry: TrackedCall): vscode.TreeItem {
        const { call } = entry;
        const item = new vscode.TreeItem(`${call.method}${summarise(call)}`);
        item.id = `${entry.session}:${call.id}`;
        item.iconPath = new vscode.ThemeIcon(call.direction === "Outgoing" ? "arrow-up" : "arrow-down");
        item.contextValue = "roprobe.remoteCall";
        item.description = this.callerOf(call);
        item.tooltip = new vscode.MarkdownString(
            [
                `**${call.direction === "Outgoing" ? "client → server" : "server → client"}**`,
                "",
                "```luau",
                callDocument(call, this.labelFor(entry.session)),
                "```",
            ].join("\n"),
        );
        item.command = {
            command: "roprobe.showRemoteCall",
            title: "Show Remote Call",
            arguments: [entry],
        };
        return item;
    }

    /// The shortest useful attribution: the script if there is one, else the
    /// chunk and line `debug.info` gave us. An actor is marked, since which VM a
    /// call came from cannot be inferred from the remote or the caller.
    private callerOf(call: RemoteCall): string {
        const { script, chunk, line, isExecutor, actor } = call.source;
        const where = actor === undefined ? "" : `[actor] `;

        if (isExecutor) {
            return `${where}executor`;
        }
        if (script) {
            return `${where}${script.split(".").pop() ?? script}`;
        }
        if (chunk) {
            return `${where}${line === undefined ? chunk : `${chunk}:${line}`}`;
        }
        return where.trimEnd();
    }

    private labelFor(session: SessionId): string {
        const known = this.sessions.list().find((candidate) => candidate.id === session);
        return known ? sessionLabel(known) : `session ${session}`;
    }

    private find(session: SessionId, id: number): TrackedCall | undefined {
        for (const group of this.remotes.list()) {
            const found = group.calls.find((entry) => entry.session === session && entry.call.id === id);
            if (found) {
                return found;
            }
        }
        return undefined;
    }
}
