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
exports.resolveLanguageServerCommand = resolveLanguageServerCommand;
exports.resolveNotebookCommand = resolveNotebookCommand;
exports.resolveDapExecutable = resolveDapExecutable;
exports.startClient = startClient;
exports.stopClient = stopClient;
const fs = __importStar(require("fs"));
const path = __importStar(require("path"));
const vscode = __importStar(require("vscode"));
const node_1 = require("vscode-languageclient/node");
let client;
function configuredValue(key) {
    const inspected = vscode.workspace
        .getConfiguration("forge")
        .inspect(key);
    const explicit = inspected?.workspaceFolderValue ??
        inspected?.workspaceValue ??
        inspected?.globalValue;
    if (typeof explicit !== "string") {
        return undefined;
    }
    const trimmed = explicit.trim();
    return trimmed.length > 0 ? trimmed : undefined;
}
function workspaceRoot(context) {
    return (vscode.workspace.workspaceFolders?.[0]?.uri.fsPath ??
        path.resolve(context.extensionPath, "..", ".."));
}
function isDirectServerBinary(command) {
    const normalized = path.basename(command).toLowerCase();
    return normalized === "forge-lsp" || normalized === "forge-lsp.exe";
}
function repoBinaryCandidates(context, binaryName) {
    const roots = new Set([
        workspaceRoot(context),
        path.resolve(context.extensionPath, "..", "..")
    ]);
    return [...roots].map(root => path.join(root, "target", "debug", binaryName));
}
function firstExisting(paths) {
    return paths.find(candidate => fs.existsSync(candidate));
}
function resolveLanguageServerCommand(context) {
    const configured = configuredValue("languageServer.path");
    if (configured) {
        return {
            command: configured,
            args: isDirectServerBinary(configured) ? [] : ["lsp"]
        };
    }
    const localServer = firstExisting(repoBinaryCandidates(context, "forge-lsp.exe"));
    if (localServer) {
        return { command: localServer, args: [] };
    }
    const localCli = firstExisting(repoBinaryCandidates(context, "forge-new.exe"));
    if (localCli) {
        return { command: localCli, args: ["lsp"] };
    }
    return { command: "forge", args: ["lsp"] };
}
function resolveNotebookCommand(context) {
    const configured = configuredValue("notebook.path");
    if (configured) {
        return {
            command: configured,
            args: ["notebook", "--kernel"]
        };
    }
    const localCli = firstExisting(repoBinaryCandidates(context, "forge-new.exe"));
    if (localCli) {
        return { command: localCli, args: ["notebook", "--kernel"] };
    }
    const configuredLsp = configuredValue("languageServer.path");
    if (configuredLsp && !isDirectServerBinary(configuredLsp)) {
        return { command: configuredLsp, args: ["notebook", "--kernel"] };
    }
    return { command: "forge", args: ["notebook", "--kernel"] };
}
function resolveDapExecutable(context) {
    const configured = configuredValue("dap.path");
    if (configured) {
        return configured;
    }
    const localDap = firstExisting(repoBinaryCandidates(context, "forge-dap.exe")) || firstExisting(repoBinaryCandidates(context, "forge-dap"));
    if (localDap) {
        return localDap;
    }
    return "forge-dap";
}
function resolveServerOptions(context) {
    const launch = resolveLanguageServerCommand(context);
    const cwd = workspaceRoot(context);
    return {
        command: launch.command,
        args: launch.args,
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