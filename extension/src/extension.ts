import * as vscode from "vscode";
import { BrokerManager } from "./broker";

let broker: BrokerManager | undefined;

export async function activate(context: vscode.ExtensionContext) {
	console.log("roprobe activating")
	context.subscriptions.push(
		vscode.commands.registerCommand("roprobe.restartBroker", async () => {
			try {
				await broker?.restart();
				vscode.window.showInformationMessage("roprobe: broker restarted");
			} catch (err) {
				vscode.window.showErrorMessage(`roprobe: broker failed to start — ${String(err)}`);
			}
		}),
	);

	broker = new BrokerManager(context);
	context.subscriptions.push(broker);

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
