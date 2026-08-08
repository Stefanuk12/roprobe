import * as vscode from "vscode";
import { BrokerManager, CONFIG_SECTION, readSpyConfig } from "./broker";
import type { SessionId } from "./broker/message";
import { callSnippet } from "./remote_format";
import { REMOTE_SCHEME, RemoteSpyView } from "./remote_spy_view";
import { RemoteRegistry, type TrackedCall } from "./remotes";
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
      } else if (message.type === "SessionRemotes") {
        // Summarised, not dumped: `JSON.stringify` expands the `Uint8Array`
        // arguments into one key per byte.
        log.info(
          `session ${message.content.id} relayed ${message.content.calls.length} remote call(s)`,
        );
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

  registerRemoteSpy(context, broker, sessions, log);

  try {
    await broker.start();
  } catch (err) {
    console.error("roprobe: broker failed to start —", err);
  }
}

/// Wire the remote spy: the registry mirroring the broker's buffer, the tree it
/// renders as, and the commands that arm, clear and read it.
function registerRemoteSpy(
  context: vscode.ExtensionContext,
  broker: BrokerManager,
  sessions: SessionRegistry,
  log: vscode.LogOutputChannel,
) {
  const remotes = new RemoteRegistry(broker, log);
  const view = new RemoteSpyView(remotes, sessions);
  context.subscriptions.push(remotes, view);

  const tree = vscode.window.createTreeView("roprobe.remotes", { treeDataProvider: view });
  context.subscriptions.push(tree);
  context.subscriptions.push(
    remotes.onDidChange(() => {
      const { shown, total } = remotes.counts;
      tree.badge = total === 0 ? undefined : { value: shown, tooltip: `${total} remote call(s) captured` };
    }),
  );

  context.subscriptions.push(
    vscode.workspace.registerTextDocumentContentProvider(REMOTE_SCHEME, view),
    vscode.commands.registerCommand("roprobe.showRemoteCall", (entry: TrackedCall) => view.show(entry)),
    vscode.commands.registerCommand("roprobe.copyRemoteCall", async (entry: TrackedCall) => {
      await vscode.env.clipboard.writeText(callSnippet(entry.call));
      vscode.window.showInformationMessage("roprobe: copied the call to the clipboard");
    }),
    vscode.commands.registerCommand("roprobe.clearRemotes", () => broker.send({ type: "ClearRemotes" })),
    vscode.commands.registerCommand("roprobe.toggleRemoteSpy", async () => {
      const settings = vscode.workspace.getConfiguration(CONFIG_SECTION);
      const enabled = !readSpyConfig().enabled;
      // The view is reachable with no workspace open, where a workspace-scoped
      // write would throw.
      const target = vscode.workspace.workspaceFolders?.length
        ? vscode.ConfigurationTarget.Workspace
        : vscode.ConfigurationTarget.Global;
      // The write is what arms it: the manager watches this section and pushes
      // the change on, so the toggle and the setting cannot disagree.
      await settings.update("spy.enabled", enabled, target);
      vscode.window.showInformationMessage(`roprobe: remote spy ${enabled ? "armed" : "disarmed"}`);
    }),
  );
}

// This method is called when your extension is deactivated.
export function deactivate() {
  broker?.dispose();
  broker = undefined;
}
