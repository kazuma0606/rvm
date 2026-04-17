import * as path from "path";
import * as vscode from "vscode";
import {
  LanguageClient,
  LanguageClientOptions,
  ServerOptions,
  TransportKind
} from "vscode-languageclient/node";

let client: LanguageClient | undefined;

function resolveForgeCommand(): string {
  const configured = vscode.workspace
    .getConfiguration("forge")
    .get<string>("languageServer.path")
    ?.trim();

  if (configured) {
    return configured;
  }

  return "forge";
}

function resolveServerOptions(context: vscode.ExtensionContext): ServerOptions {
  const command = resolveForgeCommand();
  const cwd =
    vscode.workspace.workspaceFolders?.[0]?.uri.fsPath ??
    path.resolve(context.extensionPath, "..", "..");

  return {
    command,
    args: ["lsp"],
    transport: TransportKind.stdio,
    options: { cwd }
  };
}

export async function startClient(
  context: vscode.ExtensionContext
): Promise<LanguageClient> {
  if (client) {
    return client;
  }

  const serverOptions = resolveServerOptions(context);
  const clientOptions: LanguageClientOptions = {
    documentSelector: [{ language: "forge" }],
    synchronize: {
      fileEvents: vscode.workspace.createFileSystemWatcher("**/*.forge")
    }
  };

  client = new LanguageClient(
    "forge-lsp",
    "Forge Language Server",
    serverOptions,
    clientOptions
  );

  context.subscriptions.push({
    dispose: () => {
      void stopClient();
    }
  });
  await client.start();
  return client;
}

export async function stopClient(): Promise<void> {
  if (!client) {
    return;
  }

  const running = client;
  client = undefined;
  await running.stop();
}
