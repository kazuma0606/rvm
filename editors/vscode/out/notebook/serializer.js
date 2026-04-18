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
exports.FnbSerializer = void 0;
const vscode = __importStar(require("vscode"));
const CELL_LANGUAGE = "forge";
const HIDDEN_PLACEHOLDER = "";
class FnbSerializer {
    async deserializeNotebook(content, _token) {
        const text = new TextDecoder().decode(content);
        const cells = parseFnb(text);
        return new vscode.NotebookData(cells);
    }
    async serializeNotebook(data, _token) {
        const text = serializeFnb(data);
        return new TextEncoder().encode(text);
    }
}
exports.FnbSerializer = FnbSerializer;
function parseFnb(source) {
    const normalized = source.replace(/\r\n/g, "\n");
    const lines = normalized.split("\n");
    const cells = [];
    let markdown = [];
    const flushMarkdown = () => {
        if (markdown.length === 0) {
            return;
        }
        const value = markdown.join("\n");
        markdown = [];
        if (value.trim().length === 0) {
            return;
        }
        cells.push(new vscode.NotebookCellData(vscode.NotebookCellKind.Markup, value, "markdown"));
    };
    for (let index = 0; index < lines.length;) {
        const attrs = parseFence(lines[index]);
        if (!attrs) {
            markdown.push(lines[index]);
            index += 1;
            continue;
        }
        flushMarkdown();
        const code = [];
        index += 1;
        while (index < lines.length && lines[index].trim() !== "```") {
            code.push(lines[index]);
            index += 1;
        }
        if (index < lines.length && lines[index].trim() === "```") {
            index += 1;
        }
        const cell = new vscode.NotebookCellData(vscode.NotebookCellKind.Code, attrs.hidden ? HIDDEN_PLACEHOLDER : code.join("\n"), CELL_LANGUAGE);
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
function serializeFnb(data) {
    const parts = [];
    for (const cell of data.cells) {
        if (cell.kind === vscode.NotebookCellKind.Markup) {
            parts.push(cell.value);
            continue;
        }
        const custom = (cell.metadata?.custom ?? {});
        const attrs = [];
        if (typeof custom.name === "string" && custom.name.trim().length > 0) {
            attrs.push(`name="${custom.name}"`);
        }
        if (custom.hidden === true || cell.metadata?.inputCollapsed === true) {
            attrs.push("hidden=true");
        }
        if (custom.skip === true) {
            attrs.push("skip=true");
        }
        const source = custom.hidden === true && cell.metadata?.forgeSourceHidden === true
            ? String(cell.metadata?.forgeHiddenSource ?? "")
            : cell.value;
        const open = attrs.length > 0 ? `\`\`\`forge ${attrs.join(" ")}` : "```forge";
        parts.push(`${open}\n${source}\n\`\`\``);
    }
    return parts.join("\n\n");
}
function parseFence(line) {
    const trimmed = line.trimStart();
    if (!trimmed.startsWith("```forge")) {
        return null;
    }
    const attrs = {
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
//# sourceMappingURL=serializer.js.map