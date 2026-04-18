import * as vscode from "vscode";

const REVEAL_COMMAND = "forge.notebook.revealHiddenCell";
const HIDE_COMMAND = "forge.notebook.hideHiddenCell";

export function registerHiddenCellSupport(
  context: vscode.ExtensionContext
): vscode.Disposable[] {
  const reveal = vscode.commands.registerCommand(
    REVEAL_COMMAND,
    async (cell: vscode.NotebookCell) => {
      await revealHiddenCell(cell);
    }
  );

  const hide = vscode.commands.registerCommand(
    HIDE_COMMAND,
    async (cell: vscode.NotebookCell) => {
      await hideHiddenCell(cell);
    }
  );

  const provider = vscode.notebooks.registerNotebookCellStatusBarItemProvider(
    "fnb",
    new HiddenCellStatusBarProvider()
  );

  context.subscriptions.push(reveal, hide, provider);
  return [reveal, hide, provider];
}

class HiddenCellStatusBarProvider
  implements vscode.NotebookCellStatusBarItemProvider
{
  provideCellStatusBarItems(
    cell: vscode.NotebookCell
  ):
    | vscode.NotebookCellStatusBarItem
    | vscode.NotebookCellStatusBarItem[]
    | undefined {
    if (cell.kind !== vscode.NotebookCellKind.Code) {
      return undefined;
    }

    const custom = (cell.metadata?.custom ?? {}) as Record<string, unknown>;
    if (custom.hidden !== true) {
      return undefined;
    }

    const hidden = cell.metadata?.forgeSourceHidden === true;
    const item = new vscode.NotebookCellStatusBarItem(
      hidden ? "$(eye) Reveal Source" : "$(eye-closed) Hide Source",
      vscode.NotebookCellStatusBarAlignment.Right
    );
    item.command = {
      command: hidden ? REVEAL_COMMAND : HIDE_COMMAND,
      title: hidden ? "Reveal Source" : "Hide Source",
      arguments: [cell]
    };
    item.tooltip = hidden
      ? "Reveal the source for this hidden notebook cell."
      : "Hide the source for this notebook cell.";
    return item;
  }
}

async function revealHiddenCell(cell: vscode.NotebookCell): Promise<void> {
  const source =
    typeof cell.metadata?.forgeHiddenSource === "string"
      ? cell.metadata.forgeHiddenSource
      : "";
  const metadata = {
    ...(cell.metadata ?? {}),
    forgeSourceHidden: false,
    inputCollapsed: false
  };
  await applyCellState(cell, source, metadata);
}

async function hideHiddenCell(cell: vscode.NotebookCell): Promise<void> {
  const metadata = {
    ...(cell.metadata ?? {}),
    forgeHiddenSource: cell.document.getText(),
    forgeSourceHidden: true,
    inputCollapsed: true
  };
  await applyCellState(cell, "", metadata);
}

async function applyCellState(
  cell: vscode.NotebookCell,
  source: string,
  metadata: { [key: string]: unknown }
): Promise<void> {
  const edit = new vscode.WorkspaceEdit();
  const fullRange = new vscode.Range(
    cell.document.positionAt(0),
    cell.document.positionAt(cell.document.getText().length)
  );
  edit.replace(cell.document.uri, fullRange, source);
  edit.set(cell.notebook.uri, [
    vscode.NotebookEdit.updateCellMetadata(cell.index, metadata)
  ]);
  await vscode.workspace.applyEdit(edit);
}
