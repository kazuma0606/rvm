import * as vscode from "vscode";
import { startClient, stopClient } from "./client";
import { FnbSerializer } from "./notebook/serializer";
import { FnbKernelController } from "./notebook/controller";
import { registerHiddenCellSupport } from "./notebook/hiddenCells";

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
  await startClient(context);
}

export async function deactivate(): Promise<void> {
  await stopClient();
}
