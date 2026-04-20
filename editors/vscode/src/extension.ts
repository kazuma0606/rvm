import * as vscode from "vscode";
import { startClient, stopClient, resolveDapExecutable } from "./client";
import { FnbSerializer } from "./notebook/serializer";
import { FnbKernelController } from "./notebook/controller";
import { registerHiddenCellSupport } from "./notebook/hiddenCells";

class ForgeDebugAdapterDescriptorFactory
  implements vscode.DebugAdapterDescriptorFactory
{
  constructor(private context: vscode.ExtensionContext) {}

  createDebugAdapterDescriptor(
    session: vscode.DebugSession
  ): vscode.ProviderResult<vscode.DebugAdapterDescriptor> {
    let dapPath = resolveDapExecutable(this.context);
    if (session.configuration.dapPath) {
      dapPath = session.configuration.dapPath;
    }
    return new vscode.DebugAdapterExecutable(dapPath, []);
  }
}

export async function activate(context: vscode.ExtensionContext) {
  // デバッガー登録
  context.subscriptions.push(
    vscode.debug.registerDebugAdapterDescriptorFactory(
      "forge",
      new ForgeDebugAdapterDescriptorFactory(context)
    )
  );

  // ノートブック登録
  context.subscriptions.push(
    vscode.workspace.registerNotebookSerializer("fnb", new FnbSerializer(), {
      transientOutputs: false,
      transientCellMetadata: {
        inputCollapsed: false,
        forgeHiddenSource: true,
        forgeSourceHidden: true
      }
    })
  );
  context.subscriptions.push(new FnbKernelController(context));
  context.subscriptions.push(...registerHiddenCellSupport(context));

  // LSP起動
  startClient(context).catch(e => console.error("LSP Start Failed", e));
}

export async function deactivate(): Promise<void> {
  await stopClient();
}
