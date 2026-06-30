import * as vscode from "vscode";
import { BrokerManager } from "./broker";

let broker: BrokerManager | undefined;

// This method is called when your extension is activated.
export async function activate(context: vscode.ExtensionContext) {
	broker = new BrokerManager(context);
	context.subscriptions.push(broker);

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

	// Bring the broker up on activation. Failures here are non-intrusive (logged to
	// the "roprobe" output channel) so a not-yet-built binary doesn't nag on startup.
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
