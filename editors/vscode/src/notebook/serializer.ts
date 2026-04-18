import * as vscode from "vscode";

const CELL_LANGUAGE = "forge";
const HIDDEN_PLACEHOLDER = "";

type FenceAttrs = {
  hidden: boolean;
  name?: string;
  skip: boolean;
};

export class FnbSerializer implements vscode.NotebookSerializer {
  async deserializeNotebook(
    content: Uint8Array,
    _token: vscode.CancellationToken
  ): Promise<vscode.NotebookData> {
    const text = new TextDecoder().decode(content);
    const cells = parseFnb(text);
    return new vscode.NotebookData(cells);
  }

  async serializeNotebook(
    data: vscode.NotebookData,
    _token: vscode.CancellationToken
  ): Promise<Uint8Array> {
    const text = serializeFnb(data);
    return new TextEncoder().encode(text);
  }
}

function parseFnb(source: string): vscode.NotebookCellData[] {
  const normalized = source.replace(/\r\n/g, "\n");
  const lines = normalized.split("\n");
  const cells: vscode.NotebookCellData[] = [];
  let markdown: string[] = [];

  const flushMarkdown = (): void => {
    if (markdown.length === 0) {
      return;
    }
    const value = markdown.join("\n");
    markdown = [];
    if (value.trim().length === 0) {
      return;
    }
    cells.push(
      new vscode.NotebookCellData(
        vscode.NotebookCellKind.Markup,
        value,
        "markdown"
      )
    );
  };

  for (let index = 0; index < lines.length; ) {
    const attrs = parseFence(lines[index]);
    if (!attrs) {
      markdown.push(lines[index]);
      index += 1;
      continue;
    }

    flushMarkdown();
    const code: string[] = [];
    index += 1;
    while (index < lines.length && lines[index].trim() !== "```") {
      code.push(lines[index]);
      index += 1;
    }
    if (index < lines.length && lines[index].trim() === "```") {
      index += 1;
    }

    const cell = new vscode.NotebookCellData(
      vscode.NotebookCellKind.Code,
      attrs.hidden ? HIDDEN_PLACEHOLDER : code.join("\n"),
      CELL_LANGUAGE
    );
    cell.metadata = {
      inputCollapsed: attrs.hidden,
      forgeHiddenSource: attrs.hidden ? code.join("\n") : undefined,
      forgeSourceHidden: attrs.hidden,
      custom: {
        hidden: attrs.hidden,
        name: attrs.name,
        skip: attrs.skip
      }
    };
    cells.push(cell);
  }

  flushMarkdown();
  return cells;
}

function serializeFnb(data: vscode.NotebookData): string {
  const parts: string[] = [];

  for (const cell of data.cells) {
    if (cell.kind === vscode.NotebookCellKind.Markup) {
      parts.push(cell.value);
      continue;
    }

    const custom = (cell.metadata?.custom ?? {}) as Record<string, unknown>;
    const attrs: string[] = [];
    if (typeof custom.name === "string" && custom.name.trim().length > 0) {
      attrs.push(`name="${custom.name}"`);
    }
    if (custom.hidden === true || cell.metadata?.inputCollapsed === true) {
      attrs.push("hidden=true");
    }
    if (custom.skip === true) {
      attrs.push("skip=true");
    }

    const source =
      custom.hidden === true && cell.metadata?.forgeSourceHidden === true
        ? String(cell.metadata?.forgeHiddenSource ?? "")
        : cell.value;

    const open = attrs.length > 0 ? `\`\`\`forge ${attrs.join(" ")}` : "```forge";
    parts.push(`${open}\n${source}\n\`\`\``);
  }

  return parts.join("\n\n");
}

function parseFence(line: string): FenceAttrs | null {
  const trimmed = line.trimStart();
  if (!trimmed.startsWith("```forge")) {
    return null;
  }

  const attrs: FenceAttrs = {
    hidden: false,
    skip: false
  };
  const rest = trimmed.slice("```forge".length).trim();
  if (rest.length === 0) {
    return attrs;
  }

  for (const token of rest.split(/\s+/)) {
    const [key, rawValue] = token.split("=", 2);
    if (!key || rawValue === undefined) {
      continue;
    }
    switch (key) {
      case "name":
        attrs.name = rawValue.replace(/^"/, "").replace(/"$/, "");
        break;
      case "hidden":
        attrs.hidden = rawValue === "true";
        break;
      case "skip":
        attrs.skip = rawValue === "true";
        break;
      default:
        break;
    }
  }

  return attrs;
}
