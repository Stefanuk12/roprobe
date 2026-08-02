import * as vscode from "vscode";
import { BrokerManager } from "./broker";

let broker: BrokerManager | undefined;

export async function activate(context: vscode.ExtensionContext) {
  const log = vscode.window.createOutputChannel("roprobe", { log: true }); 

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
      log.info(`got message ${JSON.stringify(message)}`);
      switch (message.type) {
        case "NewSession":
          broker?.executionChannels.addChannel(message.content.toString());
          break;
        case "RemoveSession":
          broker?.executionChannels.removeChannel(message.content.toString());
          break;
        case "Sessions":
          for (const session of message.content) {
            broker?.executionChannels.addChannel(session.id.toString());
          }
          break;
      }
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
