import * as vscode from "vscode";
import { startClient, stopClient } from "./client";
import { FnbSerializer } from "./notebook/serializer";
import { FnbKernelController } from "./notebook/controller";
import { registerHiddenCellSupport } from "./notebook/hiddenCells";

class ForgeDebugAdapterDescriptorFactory
  implements vscode.DebugAdapterDescriptorFactory
{
  createDebugAdapterDescriptor(
    session: vscode.DebugSession
  ): vscode.ProviderResult<vscode.DebugAdapterDescriptor> {
    // 設定 > forge.dap.path があればそれを使う。なければ PATH 上の forge-dap を使う。
    const config = vscode.workspace.getConfiguration("forge");
    let dapPath: string = config.get<string>("dap.path") || "forge-dap";

    // launch.json の dapPath フィールドで上書き可能
    if (session.configuration.dapPath) {
      dapPath = session.configuration.dapPath;
    }

    // program / mode / port は DAP launch リクエストのペイロードとして渡るため
    // バイナリへの CLI 引数は不要
    return new vscode.DebugAdapterExecutable(dapPath, []);
  }
}

export async function activate(context: vscode.ExtensionContext): Promise<void> {
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

  // forge-dap バイナリを DAP アダプターとして登録
  context.subscriptions.push(
    vscode.debug.registerDebugAdapterDescriptorFactory(
      "forge",
      new ForgeDebugAdapterDescriptorFactory()
    )
  );

  await startClient(context);
}

export async function deactivate(): Promise<void> {
  await stopClient();
}
