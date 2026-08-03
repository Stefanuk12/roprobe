import * as vscode from "vscode";
import { BrokerManager } from "./broker";
import type { SessionId } from "./broker/message";
import { promptForTarget, runActiveFile } from "./run";
import { sessionLabel, SessionRegistry } from "./sessions";
import { RunStatusItem, SessionStatusItem } from "./status_bar";

let broker: BrokerManager | undefined;

export async function activate(context: vscode.ExtensionContext) {
  const log = vscode.window.createOutputChannel("roprobe", { log: true });
  log.info(
    `Activating roprobe ${context.extension.packageJSON.version} from ${context.extensionPath}`,
  );

  context.subscriptions.push(
    vscode.commands.registerCommand("roprobe.restartBroker", async () => {
      try {
        await broker?.restart();
        vscode.window.showInformationMessage("roprobe: broker restarted");
      } catch (err) {
        vscode.window.showErrorMessage(
          `roprobe: broker failed to start — ${String(err)}`,
        );
      }
    }),
  );

  broker = new BrokerManager(context, log);
  context.subscriptions.push(broker);
  context.subscriptions.push(
    broker.onMessage((message) => {
      if (message.type === "SessionLog") {
        log.info(
          `session ${message.content.id} relayed ${message.content.entries.length} console line(s)`,
        );
        for (const entry of message.content.entries) {
          broker?.executionChannels.append(message.content.id.toString(), entry);
        }
      } else {
        log.info(`got message ${JSON.stringify(message)}`);
      }
    }),
  );

  const sessions = new SessionRegistry(broker, log);
  let shownFor: SessionId | undefined;
  context.subscriptions.push(sessions);
  context.subscriptions.push(
    sessions.onDidChange(() => {
      const channels = broker?.executionChannels;
      if (!channels) {
        return;
      }

      const live = new Set<string>();
      for (const session of sessions.list()) {
        const id = session.id.toString();
        live.add(id);
        channels.addChannel(id, sessionLabel(session));
      }
      for (const id of channels.ids()) {
        if (!live.has(id)) {
          channels.removeChannel(id);
        }
      }
    }),
  );
  context.subscriptions.push(new SessionStatusItem(broker, sessions));
  context.subscriptions.push(new RunStatusItem(broker, sessions));
  context.subscriptions.push(
    vscode.commands.registerCommand("roprobe.selectClient", () =>
      promptForTarget(sessions),
    ),
    vscode.commands.registerCommand("roprobe.runActiveFile", () =>
      runActiveFile(broker!, sessions, log),
    ),
    sessions.onDidChange(() => {
      const target = sessions.target;
      if (!target || target.id === shownFor) {
        return;
      }
      shownFor = target.id;
      broker?.executionChannels.setActive(target.id.toString());
    }),
  );

  try {
    await broker.start();
  } catch (err) {
    console.error("roprobe: broker failed to start —", err);
  }
}

// This method is called when your extension is deactivated.
export function deactivate() {
  broker?.dispose();
  broker = undefined;
}
