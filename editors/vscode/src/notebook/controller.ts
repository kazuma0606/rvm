import * as fs from "fs/promises";
import * as path from "path";
import * as vscode from "vscode";
import { spawn, ChildProcessWithoutNullStreams } from "child_process";
import { resolveNotebookCommand } from "../client";

type KernelResponse = {
  id: number;
  status: string;
  outputs: OutputItem[];
  duration_ms?: number;
};

type OutputItem =
  | { type: "text"; value: string }
  | { type: "html"; value: string }
  | { type: "json"; value: unknown }
  | { type: "table"; columns: string[]; rows: unknown[][] }
  | { type: "image"; mime: string; data: string }
  | { type: "markdown"; value: string }
  | {
      type: "pipeline_trace";
      pipeline_name: string;
      source_snippet: string;
      stages: Array<{
        name: string;
        in: number;
        out: number;
        corrupted: number;
        line?: number;
      }>;
      total_records: number;
      total_corrupted: number;
      corruptions: Array<{ stage: string; index: number; reason: string }>;
      records_by_stage?: Record<string, unknown[]>;
    }
  | { type: "error"; message: string; line?: number }
  | { type: string; [key: string]: unknown };

type NotebookOutput = {
  version: number;
  file: string;
  executed_at: string;
  cells: Array<{
    index: number;
    name: string;
    status: string;
    outputs: OutputItem[];
    duration_ms: number;
  }>;
};

const NOTEBOOK_TYPE = "fnb";
const TABLE_MIME = "application/vnd.forge.table+json";
const HTML_MIME = "application/vnd.forge.html+json";
const PIPELINE_TRACE_MIME = "application/vnd.forge.pipeline-trace+json";

export class FnbKernelController implements vscode.Disposable {
  private readonly controller: vscode.NotebookController;
  private kernel: KernelClient | undefined;
  private readonly disposables: vscode.Disposable[] = [];

  constructor(private readonly context: vscode.ExtensionContext) {
    this.controller = vscode.notebooks.createNotebookController(
      "forge-notebook-controller",
      NOTEBOOK_TYPE,
      "Forge Notebook"
    );
    this.controller.supportedLanguages = ["forge"];
    this.controller.executeHandler = async cells => {
      await this.executeCells(cells);
    };
    this.disposables.push(
      vscode.workspace.onDidOpenNotebookDocument(document => {
        if (document.notebookType === NOTEBOOK_TYPE) {
          void this.restoreOutputs(document);
        }
      }),
      vscode.window.onDidChangeVisibleNotebookEditors(editors => {
        for (const editor of editors) {
          if (editor.notebook.notebookType === NOTEBOOK_TYPE) {
            void this.restoreOutputs(editor.notebook);
          }
        }
      })
    );
  }

  dispose(): void {
    void this.kernel?.dispose();
    for (const disposable of this.disposables) {
      disposable.dispose();
    }
    this.controller.dispose();
  }

  private async executeCells(
    cells: readonly vscode.NotebookCell[]
  ): Promise<void> {
    if (cells.length === 0) {
      return;
    }

    const kernel = await this.getKernel();
    const notebook = cells[0].notebook;
    const outputCells: NotebookOutput["cells"] = [];

    for (const cell of cells) {
      if (cell.kind !== vscode.NotebookCellKind.Code) {
        continue;
      }

      const execution = this.controller.createNotebookCellExecution(cell);
      execution.start(Date.now());
      execution.clearOutput();

      const custom = (cell.metadata?.custom ?? {}) as Record<string, unknown>;
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
      } catch (error) {
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

  private async getKernel(): Promise<KernelClient> {
    if (!this.kernel) {
      this.kernel = await KernelClient.spawn(this.context);
    }
    return this.kernel;
  }

  private async restoreOutputs(
    notebook: vscode.NotebookDocument
  ): Promise<void> {
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

function resolveCellName(cell: vscode.NotebookCell): string {
  const custom = (cell.metadata?.custom ?? {}) as Record<string, unknown>;
  if (typeof custom.name === "string" && custom.name.trim().length > 0) {
    return custom.name;
  }
  return `cell_${cell.index}`;
}

function resolveCellSource(cell: vscode.NotebookCell): string {
  if (cell.metadata?.forgeSourceHidden === true) {
    const hidden = cell.metadata?.forgeHiddenSource;
    if (typeof hidden === "string") {
      return hidden;
    }
  }
  return cell.document.getText();
}

function mapOutputs(outputs: OutputItem[]): vscode.NotebookCellOutput[] {
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
        vscode.NotebookCellOutputItem.json(
          {
            columns: output.columns,
            rows: output.rows
          },
          TABLE_MIME
        ),
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
        vscode.NotebookCellOutputItem.text(
          renderPipelineTraceAsText(output),
          "text/plain"
        )
      ]);
    }

    return new vscode.NotebookCellOutput([
      vscode.NotebookCellOutputItem.json(output, "application/json")
    ]);
  });
}

function isTextOutput(output: OutputItem): output is { type: "text"; value: string } {
  return output.type === "text" && typeof output.value === "string";
}

function isErrorOutput(
  output: OutputItem
): output is { type: "error"; message: string; line?: number } {
  return output.type === "error" && typeof output.message === "string";
}

function isHtmlOutput(output: OutputItem): output is { type: "html"; value: string } {
  return output.type === "html" && typeof output.value === "string";
}

function isMarkdownOutput(
  output: OutputItem
): output is { type: "markdown"; value: string } {
  return output.type === "markdown" && typeof output.value === "string";
}

function isJsonOutput(output: OutputItem): output is { type: "json"; value: unknown } {
  return output.type === "json" && "value" in output;
}

function isTableOutput(
  output: OutputItem
): output is { type: "table"; columns: string[]; rows: unknown[][] } {
  return (
    output.type === "table" &&
    Array.isArray(output.columns) &&
    Array.isArray(output.rows)
  );
}

function isImageOutput(
  output: OutputItem
): output is { type: "image"; mime: string; data: string } {
  return (
    output.type === "image" &&
    typeof output.mime === "string" &&
    typeof output.data === "string"
  );
}

function isPipelineTraceOutput(
  output: OutputItem
): output is Extract<OutputItem, { type: "pipeline_trace" }> {
  return output.type === "pipeline_trace";
}

function renderTableAsMarkdown(output: {
  columns: string[];
  rows: unknown[][];
}): string {
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

function renderPipelineTraceAsText(
  output: Extract<OutputItem, { type: "pipeline_trace" }>
): string {
  const flow = output.stages
    .map(stage =>
      stage.corrupted > 0
        ? `${stage.name}(${stage.out}) !${stage.corrupted}`
        : `${stage.name}(${stage.out})`
    )
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

function inferColumns(rows: unknown[][]): string[] {
  const width = rows.reduce((max, row) => Math.max(max, row.length), 0);
  return Array.from({ length: width }, (_, index) => `col_${index}`);
}

function stringifyTableValue(value: unknown): string {
  if (value === null || value === undefined) {
    return "";
  }
  if (typeof value === "string") {
    return value.replace(/\|/g, "\\|");
  }
  return String(value).replace(/\|/g, "\\|");
}

function notebookImageOutput(output: {
  mime: string;
  data: string;
}): vscode.NotebookCellOutputItem | undefined {
  try {
    const bytes = Uint8Array.from(Buffer.from(output.data, "base64"));
    return new vscode.NotebookCellOutputItem(bytes, output.mime);
  } catch {
    return undefined;
  }
}

async function saveNotebookOutput(
  uri: vscode.Uri,
  cells: NotebookOutput["cells"]
): Promise<void> {
  const output: NotebookOutput = {
    version: 1,
    file: path.basename(uri.fsPath),
    executed_at: new Date().toISOString(),
    cells
  };
  const outputPath = `${uri.fsPath}.out.json`;
  await fs.writeFile(outputPath, JSON.stringify(output, null, 2), "utf8");
}

async function loadNotebookOutput(
  uri: vscode.Uri
): Promise<NotebookOutput | undefined> {
  const outputPath = `${uri.fsPath}.out.json`;
  try {
    const text = await fs.readFile(outputPath, "utf8");
    return JSON.parse(text) as NotebookOutput;
  } catch {
    return undefined;
  }
}

class KernelClient {
  private nextId = 1;
  private pending = new Map<
    number,
    {
      partial: OutputItem[];
      resolve: (value: KernelResponse) => void;
      reject: (reason?: unknown) => void;
    }
  >();
  private buffer = "";

  private constructor(private readonly process: ChildProcessWithoutNullStreams) {
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

  static async spawn(
    context: vscode.ExtensionContext
  ): Promise<KernelClient> {
    const cwd =
      vscode.workspace.workspaceFolders?.[0]?.uri.fsPath ??
      path.resolve(context.extensionPath, "..", "..");
    const launch = resolveNotebookCommand(context);
    const process = spawn(launch.command, launch.args, {
      cwd
    });
    return new KernelClient(process);
  }

  async execute(code: string): Promise<KernelResponse> {
    return this.send("execute", { code });
  }

  async dispose(): Promise<void> {
    if (!this.process.killed) {
      try {
        await this.send("shutdown", {});
      } catch {
        this.process.kill();
      }
    }
  }

  private async send(
    method: string,
    params: Record<string, unknown>
  ): Promise<KernelResponse> {
    const id = this.nextId++;
    const payload = JSON.stringify({ id, method, params });
    const promise = new Promise<KernelResponse>((resolve, reject) => {
      this.pending.set(id, {
        partial: [],
        resolve,
        reject
      });
    });
    this.process.stdin.write(`${payload}\n`);
    return promise;
  }

  private flushBuffer(): void {
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

  private handleMessage(line: string): void {
    const response = JSON.parse(line) as KernelResponse;
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
      ] as OutputItem[];
    }
    this.pending.delete(response.id);
    pending.resolve(response);
  }
}
