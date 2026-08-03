import * as path from "node:path";
import * as vscode from "vscode";
import type { BrokerManager } from "./broker";
import { looksLikeLuau } from "./run";
import { sessionLabel, type SessionRegistry } from "./sessions";

const RUN_PRIORITY = 100.09;
const SESSION_PRIORITY = 100.08;

export class RunStatusItem implements vscode.Disposable {
  private readonly item: vscode.StatusBarItem;
  private readonly subscriptions: vscode.Disposable[] = [];

  constructor(
    private readonly broker: BrokerManager,
    private readonly sessions: SessionRegistry,
  ) {
    this.item = vscode.window.createStatusBarItem(
      "roprobe.run",
      vscode.StatusBarAlignment.Right,
      RUN_PRIORITY,
    );
    this.item.name = "roprobe run";
    this.item.command = "roprobe.runActiveFile";

    this.subscriptions.push(
      sessions.onDidChange(() => this.render()),
      broker.onConnectionChanged(() => this.render()),
      vscode.window.onDidChangeActiveTextEditor(() => this.render()),
    );

    this.render();
  }

  dispose() {
    for (const subscription of this.subscriptions) {
      subscription.dispose();
    }
    this.item.dispose();
  }

  private render() {
    const document = vscode.window.activeTextEditor?.document;
    if (!this.broker.connected || !document || !looksLikeLuau(document)) {
      this.item.hide();
      return;
    }

    const name = document.isUntitled ? "the active buffer" : path.basename(document.fileName);
    const target = this.sessions.target;
    this.item.text = "$(run) Run";
    this.item.tooltip = target
      ? `roprobe: run ${name} on ${sessionLabel(target)} (${target.peer})`
      : `roprobe: run ${name} — no clients are connected`;
    this.item.show();
  }
}

export class SessionStatusItem implements vscode.Disposable {
  private readonly item: vscode.StatusBarItem;
  private readonly subscriptions: vscode.Disposable[] = [];

  constructor(
    broker: BrokerManager,
    private readonly sessions: SessionRegistry,
  ) {
    this.item = vscode.window.createStatusBarItem(
      "roprobe.sessions",
      vscode.StatusBarAlignment.Right,
      SESSION_PRIORITY,
    );
    this.item.name = "roprobe clients";
    this.item.command = "roprobe.selectClient";

    this.subscriptions.push(
      sessions.onDidChange(() => this.render(broker.connected)),
      broker.onConnectionChanged((connected) => this.render(connected)),
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

  private render(connected: boolean) {
    if (!connected) {
      this.item.text = "$(debug-disconnect) roprobe";
      this.item.tooltip = "roprobe: not connected to a broker";
      return;
    }

    const count = this.sessions.list().length;
    const target = this.sessions.target;
    if (!target) {
      this.item.text = "$(plug) roprobe: no clients";
      this.item.tooltip = "roprobe: connected to a broker, but no game client has joined";
      return;
    }

    const others = count === 1 ? "the only client" : `one of ${count} clients`;
    this.item.text = `$(vm-active) roprobe: ${sessionLabel(target)}`;
    this.item.tooltip = `roprobe: running on ${sessionLabel(target)} (${target.peer}), ${others}\nClick to switch client`;
  }
}
