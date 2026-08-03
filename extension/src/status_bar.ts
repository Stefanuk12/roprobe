import * as vscode from "vscode";
import type { BrokerManager } from "./broker";
import type { SessionId, ServerMessage } from "./broker/message";

/// Bottom-bar count of the game clients the broker currently has attached.
export class SessionStatusItem implements vscode.Disposable {
  private readonly item: vscode.StatusBarItem;
  private readonly subscriptions: vscode.Disposable[] = [];
  private readonly sessions = new Set<SessionId>();

  constructor(broker: BrokerManager) {
    this.item = vscode.window.createStatusBarItem(
      "roprobe.sessions",
      vscode.StatusBarAlignment.Right,
      100,
    );
    this.item.name = "roprobe clients";

    this.subscriptions.push(
      broker.onMessage((message) => this.track(message)),
      broker.onConnectionChanged((connected) => {
        if (!connected) {
          this.sessions.clear();
        }
        this.render(connected);
      }),
    );

    this.render(broker.connected);
    this.item.show();
  }

  dispose() {
    for (const subscription of this.subscriptions) {
      subscription.dispose();
    }
    this.item.dispose();
  }

  private track(message: ServerMessage) {
    switch (message.type) {
      case "Sessions":
        this.sessions.clear();
        for (const session of message.content) {
          this.sessions.add(session.id);
        }
        break;
      case "NewSession":
        this.sessions.add(message.content);
        break;
      case "RemoveSession":
        this.sessions.delete(message.content);
        break;
      default:
        return;
    }

    this.render(true);
  }

  private render(connected: boolean) {
    if (!connected) {
      this.item.text = "$(debug-disconnect) roprobe";
      this.item.tooltip = "roprobe: not connected to a broker";
      return;
    }

    const count = this.sessions.size;
    this.item.text = `$(plug) roprobe ${count}`;
    this.item.tooltip = count === 1 ? "roprobe: 1 client connected" : `roprobe: ${count} clients connected`;
  }
}
