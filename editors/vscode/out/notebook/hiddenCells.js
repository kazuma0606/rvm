"use strict";
var __createBinding = (this && this.__createBinding) || (Object.create ? (function(o, m, k, k2) {
    if (k2 === undefined) k2 = k;
    var desc = Object.getOwnPropertyDescriptor(m, k);
    if (!desc || ("get" in desc ? !m.__esModule : desc.writable || desc.configurable)) {
      desc = { enumerable: true, get: function() { return m[k]; } };
    }
    Object.defineProperty(o, k2, desc);
}) : (function(o, m, k, k2) {
    if (k2 === undefined) k2 = k;
    o[k2] = m[k];
}));
var __setModuleDefault = (this && this.__setModuleDefault) || (Object.create ? (function(o, v) {
    Object.defineProperty(o, "default", { enumerable: true, value: v });
}) : function(o, v) {
    o["default"] = v;
});
var __importStar = (this && this.__importStar) || (function () {
    var ownKeys = function(o) {
        ownKeys = Object.getOwnPropertyNames || function (o) {
            var ar = [];
            for (var k in o) if (Object.prototype.hasOwnProperty.call(o, k)) ar[ar.length] = k;
            return ar;
        };
        return ownKeys(o);
    };
    return function (mod) {
        if (mod && mod.__esModule) return mod;
        var result = {};
        if (mod != null) for (var k = ownKeys(mod), i = 0; i < k.length; i++) if (k[i] !== "default") __createBinding(result, mod, k[i]);
        __setModuleDefault(result, mod);
        return result;
    };
})();
Object.defineProperty(exports, "__esModule", { value: true });
exports.registerHiddenCellSupport = registerHiddenCellSupport;
const vscode = __importStar(require("vscode"));
const REVEAL_COMMAND = "forge.notebook.revealHiddenCell";
const HIDE_COMMAND = "forge.notebook.hideHiddenCell";
function registerHiddenCellSupport(context) {
    const reveal = vscode.commands.registerCommand(REVEAL_COMMAND, async (cell) => {
        await revealHiddenCell(cell);
    });
    const hide = vscode.commands.registerCommand(HIDE_COMMAND, async (cell) => {
        await hideHiddenCell(cell);
    });
    const provider = vscode.notebooks.registerNotebookCellStatusBarItemProvider("fnb", new HiddenCellStatusBarProvider());
    context.subscriptions.push(reveal, hide, provider);
    return [reveal, hide, provider];
}
class HiddenCellStatusBarProvider {
    provideCellStatusBarItems(cell) {
        if (cell.kind !== vscode.NotebookCellKind.Code) {
            return undefined;
        }
        const custom = (cell.metadata?.custom ?? {});
        if (custom.hidden !== true) {
            return undefined;
        }
        const hidden = cell.metadata?.forgeSourceHidden === true;
        const item = new vscode.NotebookCellStatusBarItem(hidden ? "$(eye) Reveal Source" : "$(eye-closed) Hide Source", vscode.NotebookCellStatusBarAlignment.Right);
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
async function revealHiddenCell(cell) {
    const source = typeof cell.metadata?.forgeHiddenSource === "string"
        ? cell.metadata.forgeHiddenSource
        : "";
    const metadata = {
        ...(cell.metadata ?? {}),
        forgeSourceHidden: false,
        inputCollapsed: false
    };
    await applyCellState(cell, source, metadata);
}
async function hideHiddenCell(cell) {
    const metadata = {
        ...(cell.metadata ?? {}),
        forgeHiddenSource: cell.document.getText(),
        forgeSourceHidden: true,
        inputCollapsed: true
    };
    await applyCellState(cell, "", metadata);
}
async function applyCellState(cell, source, metadata) {
    const edit = new vscode.WorkspaceEdit();
    const fullRange = new vscode.Range(cell.document.positionAt(0), cell.document.positionAt(cell.document.getText().length));
    edit.replace(cell.document.uri, fullRange, source);
    edit.set(cell.notebook.uri, [
        vscode.NotebookEdit.updateCellMetadata(cell.index, metadata)
    ]);
    await vscode.workspace.applyEdit(edit);
}
//# sourceMappingURL=hiddenCells.js.map