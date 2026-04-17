"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.activate = activate;
exports.deactivate = deactivate;
const client_1 = require("./client");
async function activate(context) {
    await (0, client_1.startClient)(context);
}
async function deactivate() {
    await (0, client_1.stopClient)();
}
//# sourceMappingURL=extension.js.map