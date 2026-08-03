import * as vscode from "vscode";
import type { BrokerManager } from "./broker";
import type { ServerMessage, SessionId, SessionInfo } from "./broker/message";

/// A connected client, with the stable ordinal the UI names it by.
export interface KnownSession extends SessionInfo {
  /// 1-based, assigned on first sight and held for as long as the client stays
  /// connected — session ids are random u32s, which nobody can read at a glance.
  ordinal: number;
}

/// The human-facing name of a client: the Roblox account it is playing on, or
/// `Client 2` for one that did not name itself (a client predating the username
/// handshake, or one whose local player had not replicated when it connected).
export function sessionLabel(session: KnownSession): string {
  return session.username ?? `Client ${session.ordinal}`;
}

/// The connected clients, and which one commands act on.
///
/// Only a client's own `RequestActive` can move the active slot without telling
/// the control connection, so every membership change is followed by a
/// `ListSessions` to freshen the flag.
export class SessionRegistry implements vscode.Disposable {
  private readonly sessions = new Map<SessionId, KnownSession>();
  private readonly subscriptions: vscode.Disposable[] = [];
  private targetId?: SessionId;
  private nextOrdinal = 1;

  private readonly _onDidChange = new vscode.EventEmitter<void>();
  /// Fires when the client list or the target changes.
  readonly onDidChange = this._onDidChange.event;

  constructor(
    private readonly broker: BrokerManager,
    private readonly log: vscode.LogOutputChannel,
  ) {
    this.subscriptions.push(
      broker.onMessage((message) => this.track(message)),
      broker.onConnectionChanged((connected) => {
        if (connected) {
          return;
        }

        // The ids belong to a broker we are no longer talking to.
        this.log.info("Sessions: broker connection dropped, forgetting every client");
        this.sessions.clear();
        this.targetId = undefined;
        this.nextOrdinal = 1;
        this._onDidChange.fire();
      }),
    );
  }

  dispose() {
    for (const subscription of this.subscriptions) {
      subscription.dispose();
    }
    this._onDidChange.dispose();
  }

  /// Every connected client, in the order they connected.
  list(): KnownSession[] {
    return [...this.sessions.values()].sort((a, b) => a.ordinal - b.ordinal);
  }

  /// The client commands act on, if one is connected.
  get target(): KnownSession | undefined {
    return this.targetId === undefined
      ? undefined
      : this.sessions.get(this.targetId);
  }

  /// Aim commands at `id`, ignoring an id no client is behind.
  setTarget(id: SessionId): boolean {
    if (!this.sessions.has(id) || this.targetId === id) {
      if (!this.sessions.has(id)) {
        this.log.warn(`Sessions: asked to target ${id}, which no client is behind`);
      }
      return this.sessions.has(id);
    }

    this.log.info(`Sessions: target is now session ${id}`);
    this.targetId = id;
    this._onDidChange.fire();
    return true;
  }

  private track(message: ServerMessage) {
    switch (message.type) {
      case "Sessions":
        this.replace(message.content);
        break;
      case "NewSession":
        this.remember(message.content);
        // Catches up on any active flag that moved while we were not looking.
        this.broker.send({ type: "ListSessions" });
        break;
      case "RemoveSession":
        if (!this.sessions.delete(message.content)) {
          return;
        }
        break;
      default:
        return;
    }

    // Ordinals only have to be unique among the clients on screen, so an empty
    // list starts the numbering over rather than drifting up forever.
    if (this.sessions.size === 0) {
      this.nextOrdinal = 1;
    }

    const before = this.targetId;
    this.settleTarget();
    this.log.info(
      `Sessions: ${message.type} leaves ${this.sessions.size} client(s) ` +
        `[${this.list().map((s) => `${sessionLabel(s)}=${s.id}`).join(", ")}], ` +
        `target ${this.targetId ?? "unset"}${this.targetId === before ? "" : " (changed)"}`,
    );
    this._onDidChange.fire();
  }

  /// Adopt the broker's list, keeping the ordinals already handed out.
  private replace(infos: SessionInfo[]) {
    const stale = new Set(this.sessions.keys());
    for (const info of infos) {
      stale.delete(info.id);
      this.remember(info);
    }
    for (const id of stale) {
      this.sessions.delete(id);
    }
  }

  private remember(info: SessionInfo) {
    const known = this.sessions.get(info.id);
    this.sessions.set(info.id, {
      ...info,
      ordinal: known?.ordinal ?? this.nextOrdinal++,
    });
  }

  /// Keep the target on a live client, defaulting to the first one so a single
  /// connected client needs no picking at all.
  private settleTarget() {
    if (this.targetId !== undefined && this.sessions.has(this.targetId)) {
      return;
    }

    this.targetId = this.list()[0]?.id;
  }
}
