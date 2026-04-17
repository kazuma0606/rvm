import * as vscode from "vscode";
import { startClient, stopClient } from "./client";

export async function activate(context: vscode.ExtensionContext): Promise<void> {
  await startClient(context);
}

export async function deactivate(): Promise<void> {
  await stopClient();
}
