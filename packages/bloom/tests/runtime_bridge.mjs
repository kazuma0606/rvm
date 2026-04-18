import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import vm from "node:vm";
import zlib from "node:zlib";

const root = path.resolve(import.meta.dirname, "..");
const source = fs.readFileSync(path.join(root, "forge.min.js"), "utf8");

const elements = new Map();
const receivedEvents = [];

function makeElement(id) {
  return {
    id,
    textContent: "",
    attributes: new Map(),
    listeners: new Map(),
    setAttribute(name, value) {
      this.attributes.set(name, value);
    },
    addEventListener(name, listener) {
      this.listeners.set(name, listener);
    },
    removeEventListener(name) {
      this.listeners.delete(name);
    },
  };
}

elements.set("title", makeElement("title"));
elements.set("btn", makeElement("btn"));

const context = {
  TextDecoder,
  Uint8Array,
  Int32Array,
  WebAssembly,
  console,
  document: {
    getElementById(id) {
      return elements.get(id) ?? null;
    },
  },
  globalThis: null,
};
context.globalThis = context;

vm.runInNewContext(source, context, { filename: "forge.min.js" });

const memory = new WebAssembly.Memory({ initial: 1 });
const encoder = new TextEncoder();
const bytes = new Uint8Array(memory.buffer);
bytes.set(encoder.encode("title"), 0);
bytes.set(encoder.encode("Hello, Bloom!"), 5);
bytes.set(encoder.encode("btn"), 18);
bytes.set(encoder.encode("click"), 21);

const runtime = context.ForgeBloom.createRuntime(memory, {
  __forge_receive_events(kind, targetId) {
    receivedEvents.push({ kind, targetId });
  },
});

runtime.applyCommands(
  new Int32Array([
    1, 0, 5, 5, 13,
    3, 18, 3, 21, 5, 7,
  ]),
);

assert.equal(elements.get("title").textContent, "Hello, Bloom!");
assert.equal(typeof elements.get("btn").listeners.get("click"), "function");

elements.get("btn").listeners.get("click")();

assert.deepEqual(receivedEvents, [{ kind: 1, targetId: "btn" }]);

const gzipped = zlib.gzipSync(Buffer.from(source, "utf8"));
assert.ok(gzipped.byteLength < 5 * 1024, `gzip size too large: ${gzipped.byteLength}`);

console.log(
  JSON.stringify({
    domUpdated: elements.get("title").textContent,
    listenerAttached: true,
    gzipBytes: gzipped.byteLength,
  }),
);
