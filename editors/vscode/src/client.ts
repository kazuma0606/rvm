import * as fs from "fs";
import * as path from "path";
import * as vscode from "vscode";
import {
  LanguageClient,
  LanguageClientOptions,
  ServerOptions,
  TransportKind
} from "vscode-languageclient/node";

let client: LanguageClient | undefined;

type CommandSpec = {
  command: string;
  args: string[];
};

function configuredValue(key: string): string | undefined {
  const inspected = vscode.workspace
    .getConfiguration("forge")
    .inspect<string>(key);
  const explicit =
    inspected?.workspaceFolderValue ??
    inspected?.workspaceValue ??
    inspected?.globalValue;

  if (typeof explicit !== "string") {
    return undefined;
  }

  const trimmed = explicit.trim();
  return trimmed.length > 0 ? trimmed : undefined;
}

function workspaceRoot(context: vscode.ExtensionContext): string {
  return (
    vscode.workspace.workspaceFolders?.[0]?.uri.fsPath ??
    path.resolve(context.extensionPath, "..", "..")
  );
}

function isDirectServerBinary(command: string): boolean {
  const normalized = path.basename(command).toLowerCase();
  return normalized === "forge-lsp" || normalized === "forge-lsp.exe";
}

function repoBinaryCandidates(
  context: vscode.ExtensionContext,
  binaryName: string
): string[] {
  const roots = new Set<string>([
    workspaceRoot(context),
    path.resolve(context.extensionPath, "..", "..")
  ]);

  return [...roots].map(root => path.join(root, "target", "debug", binaryName));
}

function firstExisting(paths: string[]): string | undefined {
  return paths.find(candidate => fs.existsSync(candidate));
}

export function resolveLanguageServerCommand(
  context: vscode.ExtensionContext
): CommandSpec {
  const configured = configuredValue("languageServer.path");
  if (configured) {
    return {
      command: configured,
      args: isDirectServerBinary(configured) ? [] : ["lsp"]
    };
  }

  const localServer = firstExisting(
    repoBinaryCandidates(context, "forge-lsp.exe")
  );
  if (localServer) {
    return { command: localServer, args: [] };
  }

  const localCli = firstExisting(repoBinaryCandidates(context, "forge-new.exe"));
  if (localCli) {
    return { command: localCli, args: ["lsp"] };
  }

  return { command: "forge", args: ["lsp"] };
}

export function resolveNotebookCommand(
  context: vscode.ExtensionContext
): CommandSpec {
  const configured = configuredValue("notebook.path");
  if (configured) {
    return {
      command: configured,
      args: ["notebook", "--kernel"]
    };
  }

  const localCli = firstExisting(repoBinaryCandidates(context, "forge-new.exe"));
  if (localCli) {
    return { command: localCli, args: ["notebook", "--kernel"] };
  }

  const configuredLsp = configuredValue("languageServer.path");
  if (configuredLsp && !isDirectServerBinary(configuredLsp)) {
    return { command: configuredLsp, args: ["notebook", "--kernel"] };
  }

  return { command: "forge", args: ["notebook", "--kernel"] };
}

export function resolveDapExecutable(
  context: vscode.ExtensionContext
): string {
  const configured = configuredValue("dap.path");
  if (configured) {
    return configured;
  }

  const localDap = firstExisting(
    repoBinaryCandidates(context, "forge-dap.exe")
  ) || firstExisting(
    repoBinaryCandidates(context, "forge-dap")
  );
  if (localDap) {
    return localDap;
  }

  return "forge-dap";
}

function resolveServerOptions(context: vscode.ExtensionContext): ServerOptions {
  const launch = resolveLanguageServerCommand(context);
  const cwd = workspaceRoot(context);

  return {
    command: launch.command,
    args: launch.args,
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
