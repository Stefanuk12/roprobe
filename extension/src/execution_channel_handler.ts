import * as vscode from "vscode";
import type { LogEntry } from "./broker/message";

export class ExecutionChannel implements vscode.Disposable {
  private disposed: boolean = false;
  private log: vscode.LogOutputChannel;

  constructor(id: string) {
    this.log = vscode.window.createOutputChannel(
      `roprobe - output redirection (${id})`,
      { log: true },
    );
  }

  dispose() {
    this.log.dispose();
    this.disposed = true;
  }

  show() {
    if (this.disposed) {
      return;
    }

    this.log.show(true);
  }

  append(entry: LogEntry) {
    if (this.disposed) {
      return;
    }

    const log = this.log;
    switch (entry.level) {
      case "print":
        log.info(entry.content);
        break;
      case "info":
        log.info(entry.content);
        break;
      case "warn":
        log.warn(entry.content);
        break;
      case "error":
        log.error(entry.content);
        break;
    }
  }
}

export class ExectionChannelHandler implements vscode.Disposable {
  private selectedChannel?: string;
  private channels: Map<string, ExecutionChannel>;

  constructor(private readonly ctx: vscode.ExtensionContext) {
    this.channels = new Map();
  }

  dispose() {
    for (const [_, channel] of this.channels) {
      channel.dispose();
    }
  }

  currentChannel(): ExecutionChannel | undefined {
    const id = this.selectedChannel;
    if (!id) {
      return;
    }

    let channel = this.channels.get(id);
    if (channel) {
      return channel;
    }

    return;
  }

  addChannel(id: string): ExecutionChannel {
    let channel = this.channels.get(id);
    if (channel) {
      return channel;
    }

    channel = new ExecutionChannel(id);
    this.channels.set(id, channel);

    if (!this.selectedChannel) {
      this.selectedChannel = id;
      this.refresh();
    }

    return channel;
  }

  removeChannel(id: string): boolean {
    const channel = this.channels.get(id);
    if (!channel) {
      return false;
    }

    channel.dispose();
    this.channels.delete(id);

    if (this.selectedChannel === id) {
      this.selectedChannel = undefined;
      this.refresh();
    }

    return true;
  }

  setActive(id: string): boolean {
    if (!this.channels.has(id)) {
      return false;
    }

    this.selectedChannel = id;
    this.refresh();
    return true;
  }

  append(id: string, entry: LogEntry) {
    this.channels.get(id)?.append(entry);
  }

  refresh() {
    const channel = this.currentChannel();
    if (!channel) {
      return;
    }

    channel.show();
  }
}
