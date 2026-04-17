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
exports.startClient = startClient;
exports.stopClient = stopClient;
const path = __importStar(require("path"));
const vscode = __importStar(require("vscode"));
const node_1 = require("vscode-languageclient/node");
let client;
function resolveForgeCommand() {
    const configured = vscode.workspace
        .getConfiguration("forge")
        .get("languageServer.path")
        ?.trim();
    if (configured) {
        return configured;
    }
    return "forge";
}
function resolveServerOptions(context) {
    const command = resolveForgeCommand();
    const cwd = vscode.workspace.workspaceFolders?.[0]?.uri.fsPath ??
        path.resolve(context.extensionPath, "..", "..");
    return {
        command,
        args: ["lsp"],
        transport: node_1.TransportKind.stdio,
        options: { cwd }
    };
}
async function startClient(context) {
    if (client) {
        return client;
    }
    const serverOptions = resolveServerOptions(context);
    const clientOptions = {
        documentSelector: [{ language: "forge" }],
        synchronize: {
            fileEvents: vscode.workspace.createFileSystemWatcher("**/*.forge")
        }
    };
    client = new node_1.LanguageClient("forge-lsp", "Forge Language Server", serverOptions, clientOptions);
    context.subscriptions.push({
        dispose: () => {
            void stopClient();
        }
    });
    await client.start();
    return client;
}
async function stopClient() {
    if (!client) {
        return;
    }
    const running = client;
    client = undefined;
    await running.stop();
}
//# sourceMappingURL=client.js.map