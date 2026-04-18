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
exports.FnbKernelController = void 0;
const fs = __importStar(require("fs/promises"));
const path = __importStar(require("path"));
const vscode = __importStar(require("vscode"));
const child_process_1 = require("child_process");
const client_1 = require("../client");
const NOTEBOOK_TYPE = "fnb";
const TABLE_MIME = "application/vnd.forge.table+json";
const HTML_MIME = "application/vnd.forge.html+json";
const PIPELINE_TRACE_MIME = "application/vnd.forge.pipeline-trace+json";
class FnbKernelController {
    constructor(context) {
        this.context = context;
        this.disposables = [];
        this.controller = vscode.notebooks.createNotebookController("forge-notebook-controller", NOTEBOOK_TYPE, "Forge Notebook");
        this.controller.supportedLanguages = ["forge"];
        this.controller.executeHandler = async (cells) => {
            await this.executeCells(cells);
        };
        this.disposables.push(vscode.workspace.onDidOpenNotebookDocument(document => {
            if (document.notebookType === NOTEBOOK_TYPE) {
                void this.restoreOutputs(document);
            }
        }), vscode.window.onDidChangeVisibleNotebookEditors(editors => {
            for (const editor of editors) {
                if (editor.notebook.notebookType === NOTEBOOK_TYPE) {
                    void this.restoreOutputs(editor.notebook);
                }
            }
        }));
    }
    dispose() {
        void this.kernel?.dispose();
        for (const disposable of this.disposables) {
            disposable.dispose();
        }
        this.controller.dispose();
    }
    async executeCells(cells) {
        if (cells.length === 0) {
            return;
        }
        const kernel = await this.getKernel();
        const notebook = cells[0].notebook;
        const outputCells = [];
        for (const cell of cells) {
            if (cell.kind !== vscode.NotebookCellKind.Code) {
                continue;
            }
            const execution = this.controller.createNotebookCellExecution(cell);
            execution.start(Date.now());
            execution.clearOutput();
            const custom = (cell.metadata?.custom ?? {});
            if (custom.skip === true) {
                execution.end(true, Date.now());
                outputCells.push({
                    index: cell.index,
                    name: resolveCellName(cell),
                    status: "skipped",
                    outputs: [],
                    duration_ms: 0
                });
                continue;
            }
            try {
                const response = await kernel.execute(resolveCellSource(cell));
                execution.replaceOutput(mapOutputs(response.outputs));
                execution.end(response.status !== "error", Date.now());
                outputCells.push({
                    index: cell.index,
                    name: resolveCellName(cell),
                    status: response.status,
                    outputs: response.outputs,
                    duration_ms: response.duration_ms ?? 0
                });
            }
            catch (error) {
                const message = error instanceof Error ? error.message : String(error);
                execution.replaceOutput([
                    new vscode.NotebookCellOutput([
                        vscode.NotebookCellOutputItem.error({
                            name: "KernelError",
                            message
                        })
                    ])
                ]);
                execution.end(false, Date.now());
                outputCells.push({
                    index: cell.index,
                    name: resolveCellName(cell),
                    status: "error",
                    outputs: [{ type: "error", message }],
                    duration_ms: 0
                });
            }
        }
        await saveNotebookOutput(notebook.uri, outputCells);
    }
    async getKernel() {
        if (!this.kernel) {
            this.kernel = await KernelClient.spawn(this.context);
        }
        return this.kernel;
    }
    async restoreOutputs(notebook) {
        const saved = await loadNotebookOutput(notebook.uri);
        if (!saved) {
            return;
        }
        for (const cell of notebook.getCells()) {
            if (cell.kind !== vscode.NotebookCellKind.Code) {
                continue;
            }
            const savedCell = saved.cells.find(entry => entry.index === cell.index);
            if (!savedCell) {
                continue;
            }
            const execution = this.controller.createNotebookCellExecution(cell);
            execution.start(Date.now());
            execution.replaceOutput(mapOutputs(savedCell.outputs));
            execution.end(savedCell.status !== "error", Date.now());
        }
    }
}
exports.FnbKernelController = FnbKernelController;
function resolveCellName(cell) {
    const custom = (cell.metadata?.custom ?? {});
    if (typeof custom.name === "string" && custom.name.trim().length > 0) {
        return custom.name;
    }
    return `cell_${cell.index}`;
}
function resolveCellSource(cell) {
    if (cell.metadata?.forgeSourceHidden === true) {
        const hidden = cell.metadata?.forgeHiddenSource;
        if (typeof hidden === "string") {
            return hidden;
        }
    }
    return cell.document.getText();
}
function mapOutputs(outputs) {
    return outputs.map(output => {
        if (isTextOutput(output)) {
            return new vscode.NotebookCellOutput([
                vscode.NotebookCellOutputItem.text(output.value)
            ]);
        }
        if (isErrorOutput(output)) {
            return new vscode.NotebookCellOutput([
                vscode.NotebookCellOutputItem.error({
                    name: "ForgeError",
                    message: output.message
                })
            ]);
        }
        if (isHtmlOutput(output)) {
            return new vscode.NotebookCellOutput([
                vscode.NotebookCellOutputItem.json({ value: output.value }, HTML_MIME),
                vscode.NotebookCellOutputItem.text(output.value, "text/html")
            ]);
        }
        if (isMarkdownOutput(output)) {
            return new vscode.NotebookCellOutput([
                vscode.NotebookCellOutputItem.text(output.value, "text/markdown")
            ]);
        }
        if (isJsonOutput(output)) {
            return new vscode.NotebookCellOutput([
                vscode.NotebookCellOutputItem.json(output.value, "application/json")
            ]);
        }
        if (isTableOutput(output)) {
            return new vscode.NotebookCellOutput([
                vscode.NotebookCellOutputItem.json({
                    columns: output.columns,
                    rows: output.rows
                }, TABLE_MIME),
                vscode.NotebookCellOutputItem.text(renderTableAsMarkdown(output), "text/markdown")
            ]);
        }
        if (isImageOutput(output)) {
            const item = notebookImageOutput(output);
            if (item) {
                return new vscode.NotebookCellOutput([item]);
            }
        }
        if (isPipelineTraceOutput(output)) {
            return new vscode.NotebookCellOutput([
                vscode.NotebookCellOutputItem.json(output, PIPELINE_TRACE_MIME),
                vscode.NotebookCellOutputItem.text(renderPipelineTraceAsText(output), "text/plain")
            ]);
        }
        return new vscode.NotebookCellOutput([
            vscode.NotebookCellOutputItem.json(output, "application/json")
        ]);
    });
}
function isTextOutput(output) {
    return output.type === "text" && typeof output.value === "string";
}
function isErrorOutput(output) {
    return output.type === "error" && typeof output.message === "string";
}
function isHtmlOutput(output) {
    return output.type === "html" && typeof output.value === "string";
}
function isMarkdownOutput(output) {
    return output.type === "markdown" && typeof output.value === "string";
}
function isJsonOutput(output) {
    return output.type === "json" && "value" in output;
}
function isTableOutput(output) {
    return (output.type === "table" &&
        Array.isArray(output.columns) &&
        Array.isArray(output.rows));
}
function isImageOutput(output) {
    return (output.type === "image" &&
        typeof output.mime === "string" &&
        typeof output.data === "string");
}
function isPipelineTraceOutput(output) {
    return output.type === "pipeline_trace";
}
function renderTableAsMarkdown(output) {
    const columns = output.columns.length > 0 ? output.columns : inferColumns(output.rows);
    if (columns.length === 0) {
        return "";
    }
    const header = `| ${columns.join(" | ")} |`;
    const divider = `| ${columns.map(() => "---").join(" | ")} |`;
    const rows = output.rows.map(row => {
        const values = columns.map((_, index) => stringifyTableValue(row[index]));
        return `| ${values.join(" | ")} |`;
    });
    return [header, divider, ...rows].join("\n");
}
function renderPipelineTraceAsText(output) {
    const flow = output.stages
        .map(stage => stage.corrupted > 0
        ? `${stage.name}(${stage.out}) !${stage.corrupted}`
        : `${stage.name}(${stage.out})`)
        .join(" -> ");
    const lines = [`[pipeline: ${output.pipeline_name}] ${flow}`];
    if (output.total_corrupted > 0) {
        lines.push(`! ${output.total_corrupted} corrupted records detected`);
        for (const corruption of output.corruptions) {
            lines.push(`  #${corruption.index} [${corruption.stage}] ${corruption.reason}`);
        }
    }
    return lines.join("\n");
}
function inferColumns(rows) {
    const width = rows.reduce((max, row) => Math.max(max, row.length), 0);
    return Array.from({ length: width }, (_, index) => `col_${index}`);
}
function stringifyTableValue(value) {
    if (value === null || value === undefined) {
        return "";
    }
    if (typeof value === "string") {
        return value.replace(/\|/g, "\\|");
    }
    return String(value).replace(/\|/g, "\\|");
}
function notebookImageOutput(output) {
    try {
        const bytes = Uint8Array.from(Buffer.from(output.data, "base64"));
        return new vscode.NotebookCellOutputItem(bytes, output.mime);
    }
    catch {
        return undefined;
    }
}
async function saveNotebookOutput(uri, cells) {
    const output = {
        version: 1,
        file: path.basename(uri.fsPath),
        executed_at: new Date().toISOString(),
        cells
    };
    const outputPath = `${uri.fsPath}.out.json`;
    await fs.writeFile(outputPath, JSON.stringify(output, null, 2), "utf8");
}
async function loadNotebookOutput(uri) {
    const outputPath = `${uri.fsPath}.out.json`;
    try {
        const text = await fs.readFile(outputPath, "utf8");
        return JSON.parse(text);
    }
    catch {
        return undefined;
    }
}
class KernelClient {
    constructor(process) {
        this.process = process;
        this.nextId = 1;
        this.pending = new Map();
        this.buffer = "";
        this.process.stdout.setEncoding("utf8");
        this.process.stdout.on("data", chunk => {
            this.buffer += String(chunk);
            this.flushBuffer();
        });
        this.process.stderr.setEncoding("utf8");
        this.process.stderr.on("data", chunk => {
            const message = chunk.toString().trim();
            if (message.length > 0) {
                console.error(message);
            }
        });
        this.process.on("exit", () => {
            for (const entry of this.pending.values()) {
                entry.reject(new Error("forge notebook kernel exited"));
            }
            this.pending.clear();
        });
    }
    static async spawn(context) {
        const cwd = vscode.workspace.workspaceFolders?.[0]?.uri.fsPath ??
            path.resolve(context.extensionPath, "..", "..");
        const launch = (0, client_1.resolveNotebookCommand)(context);
        const process = (0, child_process_1.spawn)(launch.command, launch.args, {
            cwd
        });
        return new KernelClient(process);
    }
    async execute(code) {
        return this.send("execute", { code });
    }
    async dispose() {
        if (!this.process.killed) {
            try {
                await this.send("shutdown", {});
            }
            catch {
                this.process.kill();
            }
        }
    }
    async send(method, params) {
        const id = this.nextId++;
        const payload = JSON.stringify({ id, method, params });
        const promise = new Promise((resolve, reject) => {
            this.pending.set(id, {
                partial: [],
                resolve,
                reject
            });
        });
        this.process.stdin.write(`${payload}\n`);
        return promise;
    }
    flushBuffer() {
        while (true) {
            const newline = this.buffer.indexOf("\n");
            if (newline < 0) {
                return;
            }
            const line = this.buffer.slice(0, newline).trim();
            this.buffer = this.buffer.slice(newline + 1);
            if (line.length === 0) {
                continue;
            }
            this.handleMessage(line);
        }
    }
    handleMessage(line) {
        const response = JSON.parse(line);
        const pending = this.pending.get(response.id);
        if (!pending) {
            return;
        }
        if (response.status === "partial") {
            pending.partial.push(...response.outputs);
            return;
        }
        if (pending.partial.length > 0) {
            response.outputs = [
                ...pending.partial,
                ...response.outputs
            ];
        }
        this.pending.delete(response.id);
        pending.resolve(response);
    }
}
//# sourceMappingURL=controller.js.map