import * as path from "node:path";
import * as vscode from "vscode";
import type { BrokerManager } from "./broker";
import type { OpResult } from "./broker/message";
import { sessionLabel, type KnownSession, type SessionRegistry } from "./sessions";

const LUAU_LANGUAGES = new Set(["luau", "lua"]);
const LUAU_EXTENSIONS = new Set([".luau", ".lua"]);

interface SessionPick extends vscode.QuickPickItem {
  session: KnownSession;
}

export function looksLikeLuau(document: vscode.TextDocument): boolean {
  return (
    LUAU_LANGUAGES.has(document.languageId) ||
    LUAU_EXTENSIONS.has(path.extname(document.fileName).toLowerCase())
  );
}

function isLuau(document: vscode.TextDocument): boolean {
  return document.isUntitled || looksLikeLuau(document);
}

function pickFor(session: KnownSession, target?: KnownSession): SessionPick {
  const isTarget = session.id === target?.id;
  const marks: string[] = [];
  if (isTarget) {
    marks.push("current target");
  }
  if (session.active) {
    marks.push("mirrored by the inspector");
  }

  return {
    session,
    label: `${isTarget ? "$(check)" : "$(vm)"} ${sessionLabel(session)}`,
    description: session.peer,
    detail: marks.length > 0 ? marks.join(" · ") : undefined,
  };
}

export async function promptForTarget(sessions: SessionRegistry): Promise<KnownSession | undefined> {
  const connected = sessions.list();
  if (connected.length === 0) {
    vscode.window.showErrorMessage("roprobe: no clients are connected");
    return;
  }

  const target = sessions.target;
  const picked = await vscode.window.showQuickPick(
    connected.map((session) => pickFor(session, target)),
    {
      title: "roprobe: target client",
      placeHolder: "Pick the client to run code on",
      matchOnDescription: true,
    },
  );
  if (!picked) {
    return;
  }

  sessions.setTarget(picked.session.id);
  return picked.session;
}

async function resolveTarget(
  sessions: SessionRegistry,
  log: vscode.LogOutputChannel,
): Promise<KnownSession | undefined> {
  const alwaysAsk = vscode.workspace
    .getConfiguration("roprobe.run")
    .get<boolean>("alwaysAsk", false);

  const connected = sessions.list();
  const target = sessions.target;
  log.info(
    `Run: ${connected.length} client(s) connected, target is ` +
      `${target ? `${sessionLabel(target)} (session ${target.id})` : "unset"}, alwaysAsk=${alwaysAsk}`,
  );

  if (target && !(alwaysAsk && connected.length > 1)) {
    return target;
  }

  return promptForTarget(sessions);
}

function report(broker: BrokerManager, session: KnownSession, name: string, result: OpResult): boolean {
  const channel = session.id.toString();
  broker.executionChannels.show(channel);

  switch (result.type) {
    case "Err":
      broker.executionChannels.append(channel, {
        level: "error",
        content: `${name} failed: ${result.content}`,
      });
      return false;
    case "Output":
      if (result.content.length === 0) {
        broker.executionChannels.append(channel, { level: "info", content: `${name} ran, no output` });
        return true;
      }
      for (const line of result.content.split("\n")) {
        broker.executionChannels.append(channel, { level: "print", content: line });
      }
      return true;
    default:
      broker.executionChannels.append(channel, { level: "info", content: `${name} ran` });
      return true;
  }
}

export async function runActiveFile(
  broker: BrokerManager,
  sessions: SessionRegistry,
  log: vscode.LogOutputChannel,
): Promise<void> {
  const editor = vscode.window.activeTextEditor;
  if (!editor) {
    log.error("Run: no active editor");
    vscode.window.showErrorMessage("roprobe: no active editor to run");
    return;
  }

  const document = editor.document;
  log.info(
    `Run: active document ${document.uri.toString()} (language ${document.languageId}, ` +
      `${document.isUntitled ? "untitled" : "saved"}${document.isDirty ? ", unsaved edits" : ""})`,
  );

  if (!isLuau(document)) {
    log.error(`Run: ${document.uri.toString()} is not Luau, refusing to run it`);
    vscode.window.showErrorMessage("roprobe: the active file is not Luau");
    return;
  }

  const target = await resolveTarget(sessions, log);
  if (!target) {
    log.warn("Run: no target client, giving up");
    return;
  }

  const source = document.getText();
  const name = document.isUntitled ? "the active buffer" : path.basename(document.fileName);
  const label = sessionLabel(target);

  const channel = target.id.toString();
  if (!broker.executionChannels.setActive(channel)) {
    log.warn(`Run: session ${target.id} has no output channel, its output will go nowhere`);
  }
  broker.executionChannels.append(channel, {
    level: "info",
    content: `running ${name} on ${label} (${target.peer})`,
  });

  log.info(`Run: sending ${name} (${source.length} bytes) to ${label} / session ${target.id}`);
  const result = await vscode.window.withProgress(
    { location: vscode.ProgressLocation.Window, title: `roprobe: running ${name} on ${label}` },
    () => broker.runCode(target.id, source),
  );
  log.info(`Run: ${name} on ${label} came back as ${result.type}`);

  if (report(broker, target, name, result)) {
    vscode.window.setStatusBarMessage(`roprobe: ran ${name} on ${label}`, 5_000);
  } else {
    const summary =
      result.type === "Err" ? result.content.split("\n", 1)[0] : "see its output channel";
    vscode.window.showErrorMessage(`roprobe: ${name} failed on ${label} — ${summary}`);
  }
}
