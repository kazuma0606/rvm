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
exports.activate = activate;
exports.deactivate = deactivate;
const vscode = __importStar(require("vscode"));
const client_1 = require("./client");
const serializer_1 = require("./notebook/serializer");
const controller_1 = require("./notebook/controller");
const hiddenCells_1 = require("./notebook/hiddenCells");
class ForgeDebugAdapterDescriptorFactory {
    constructor(context) {
        this.context = context;
    }
    createDebugAdapterDescriptor(session) {
        let dapPath = (0, client_1.resolveDapExecutable)(this.context);
        if (session.configuration.dapPath) {
            dapPath = session.configuration.dapPath;
        }
        return new vscode.DebugAdapterExecutable(dapPath, []);
    }
}
async function activate(context) {
    // デバッガー登録
    context.subscriptions.push(vscode.debug.registerDebugAdapterDescriptorFactory("forge", new ForgeDebugAdapterDescriptorFactory(context)));
    // ノートブック登録
    context.subscriptions.push(vscode.workspace.registerNotebookSerializer("fnb", new serializer_1.FnbSerializer(), {
        transientOutputs: false,
        transientCellMetadata: {
            inputCollapsed: false,
            forgeHiddenSource: true,
            forgeSourceHidden: true
        }
    }));
    context.subscriptions.push(new controller_1.FnbKernelController(context));
    context.subscriptions.push(...(0, hiddenCells_1.registerHiddenCellSupport)(context));
    // LSP起動
    (0, client_1.startClient)(context).catch(e => console.error("LSP Start Failed", e));
}
async function deactivate() {
    await (0, client_1.stopClient)();
}
//# sourceMappingURL=extension.js.map