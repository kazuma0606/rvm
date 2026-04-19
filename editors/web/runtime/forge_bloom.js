/**
 * ForgeBloom Browser Runtime
 *
 * WASM ハイドレーションランタイム。SSR HTML に対して WASM をアタッチし、
 * イベントとリアクティブ DOM 更新を処理する。
 *
 * 使い方:
 *   <script src="/forge.min.js" defer></script>
 *   <script>
 *     window.addEventListener("DOMContentLoaded", function() {
 *       ForgeBloom.load("/dist/counter_page.wasm");
 *     });
 *   </script>
 *
 * または data 属性による自動ロード:
 *   <script src="/forge.min.js" data-bloom-wasm="/dist/counter_page.wasm" defer></script>
 */
(function (global) {
  // ─── DOM オペコード定数 ──────────────────────────────────────────────────

  const OP_SET_TEXT      = 1;
  const OP_SET_ATTR      = 2;
  const OP_ADD_LISTENER  = 3;
  const OP_REMOVE_LISTENER = 4;
  const OP_SET_CLASS     = 5;
  const OP_INSERT_NODE   = 6;
  const OP_REMOVE_NODE   = 7;
  const OP_REPLACE_INNER = 8;
  const OP_ATTACH        = 9;

  // ─── イベント種別定数 ────────────────────────────────────────────────────

  const EVENT_CLICK  = 1;
  const EVENT_INPUT  = 2;
  const EVENT_CHANGE = 3;
  const EVENT_SUBMIT = 4;

  const decoder = new TextDecoder();
  const encoder = new TextEncoder();

  // ─── メモリ読み書きヘルパー ──────────────────────────────────────────────

  function readString(memory, offset, length) {
    return decoder.decode(new Uint8Array(memory.buffer, offset, length));
  }

  // ─── DOM ブリッジ（importObject.env） ────────────────────────────────────
  //
  // WASM が JS 関数を直接呼び出すモード向けのインポート。
  // コマンドバッファ方式と並走する（両方サポート）。

  function createEnvImports(memRef) {
    return {
      // dom_set_text(node_id_ptr: i32, node_id_len: i32, text_ptr: i32, text_len: i32)
      dom_set_text: function (nodeIdPtr, nodeIdLen, textPtr, textLen) {
        const id   = readString(memRef.current, nodeIdPtr, nodeIdLen);
        const text = readString(memRef.current, textPtr, textLen);
        var el = document.getElementById(id);
        if (el) el.textContent = text;
      },

      // dom_set_attr(node_id_ptr, node_id_len, attr_ptr, attr_len, val_ptr, val_len)
      dom_set_attr: function (nodeIdPtr, nodeIdLen, attrPtr, attrLen, valPtr, valLen) {
        const id   = readString(memRef.current, nodeIdPtr, nodeIdLen);
        const attr = readString(memRef.current, attrPtr, attrLen);
        const val  = readString(memRef.current, valPtr, valLen);
        var el = document.getElementById(id);
        if (el) el.setAttribute(attr, val);
      },

      // dom_add_event_listener(node_id_ptr, node_id_len, event_ptr, event_len, fn_idx: i32)
      // fn_idx は WASM 側でハンドラーを識別する整数インデックス
      dom_add_event_listener: function (nodeIdPtr, nodeIdLen, eventPtr, eventLen, fnIdx) {
        const id    = readString(memRef.current, nodeIdPtr, nodeIdLen);
        const event = readString(memRef.current, eventPtr, eventLen);
        var el = document.getElementById(id);
        if (!el) return;
        el.addEventListener(event, function () {
          var wasmExports = memRef.exports;
          if (wasmExports && typeof wasmExports.__forge_receive_events === "function") {
            var bytes = encoder.encode(id);
            if (typeof wasmExports.alloc === "function") {
              var ptr = wasmExports.alloc(bytes.length);
              new Uint8Array(memRef.current.buffer, ptr, bytes.length).set(bytes);
              wasmExports.__forge_receive_events(fnIdx, ptr, bytes.length);
            } else {
              wasmExports.__forge_receive_events(fnIdx, 0, 0);
            }
            // コマンドバッファを適用
            if (memRef.applyPending) memRef.applyPending();
          }
        });
      },

      // forge_log(ptr: i32, len: i32)
      forge_log: function (msgPtr, msgLen) {
        if (!memRef.current) return;
        const msg = readString(memRef.current, msgPtr, msgLen);
        console.log("[Bloom]", msg);
      },
    };
  }

  // ─── コマンドバッファ処理 ────────────────────────────────────────────────

  function createRuntime(memory, exports) {
    const listeners = new Map();
    const events    = [];

    function applyPendingCommands() {
      if (
        typeof exports.__forge_pull_commands_ptr !== "function" ||
        typeof exports.__forge_pull_commands_len !== "function"
      ) {
        return 0;
      }
      const ptr = exports.__forge_pull_commands_ptr();
      const len = exports.__forge_pull_commands_len();
      if (!ptr || !len) return 0;
      return applyCommands(new Int32Array(memory.buffer, ptr, len));
    }

    function applyCommands(buf) {
      var touched = 0;
      var i = 0;
      while (i < buf.length) {
        var opcode = buf[i++];

        if (opcode === OP_SET_TEXT) {
          var tOff = buf[i++], tLen = buf[i++];
          var vOff = buf[i++], vLen = buf[i++];
          var id    = readString(memory, tOff, tLen);
          var value = readString(memory, vOff, vLen);
          var el = document.getElementById(id);
          if (el) { el.textContent = value; touched++; }
          continue;
        }

        if (opcode === OP_SET_ATTR) {
          var tOff = buf[i++], tLen = buf[i++];
          var nOff = buf[i++], nLen = buf[i++];
          var vOff = buf[i++], vLen = buf[i++];
          var id   = readString(memory, tOff, tLen);
          var name = readString(memory, nOff, nLen);
          var val  = readString(memory, vOff, vLen);
          var el = document.getElementById(id);
          if (el) { el.setAttribute(name, val); touched++; }
          continue;
        }

        if (opcode === OP_SET_CLASS) {
          var tOff = buf[i++], tLen = buf[i++];
          var vOff = buf[i++], vLen = buf[i++];
          var id  = readString(memory, tOff, tLen);
          var cls = readString(memory, vOff, vLen);
          var el = document.getElementById(id);
          if (el) { el.className = cls; touched++; }
          continue;
        }

        if (opcode === OP_INSERT_NODE || opcode === OP_REPLACE_INNER) {
          var tOff = buf[i++], tLen = buf[i++];
          var hOff = buf[i++], hLen = buf[i++];
          var id   = readString(memory, tOff, tLen);
          var html = readString(memory, hOff, hLen);
          var el = document.getElementById(id);
          if (el) { el.innerHTML = html; touched++; }
          continue;
        }

        if (opcode === OP_REMOVE_NODE) {
          var tOff = buf[i++], tLen = buf[i++];
          var id = readString(memory, tOff, tLen);
          var el = document.getElementById(id);
          if (el) { el.innerHTML = ""; touched++; }
          continue;
        }

        if (opcode === OP_ATTACH) {
          var tOff = buf[i++], tLen = buf[i++];
          var id = readString(memory, tOff, tLen);
          var el = document.getElementById(id);
          if (el) { el.setAttribute("data-bloom-attached", "true"); touched++; }
          continue;
        }

        if (opcode === OP_ADD_LISTENER || opcode === OP_REMOVE_LISTENER) {
          var tOff = buf[i++], tLen = buf[i++];
          var eOff = buf[i++], eLen = buf[i++];
          var hOff = buf[i++], hLen = buf[i++];
          var id        = readString(memory, tOff, tLen);
          var eventName = readString(memory, eOff, eLen);
          var handler   = readString(memory, hOff, hLen);
          var key = id + ":" + eventName + ":" + handler;
          var el = document.getElementById(id);
          if (!el) continue;

          if (opcode === OP_ADD_LISTENER) {
            if (listeners.has(key)) continue;
            var listener = (function (capturedId, capturedHandler) {
              return function () {
                events.push({ kind: capturedHandler, targetId: capturedId });
                flushEvents();
              };
            })(id, handler);
            listeners.set(key, listener);
            el.addEventListener(eventName, listener);
            touched++;
          } else {
            var existing = listeners.get(key);
            if (existing) {
              el.removeEventListener(eventName, existing);
              listeners.delete(key);
              touched++;
            }
          }
          continue;
        }

        // 未知のオペコードはスキップ（将来の拡張に備えて throw しない）
        break;
      }
      return touched;
    }

    function flushEvents() {
      if (typeof exports.__forge_receive_events !== "function") return;
      while (events.length > 0) {
        var ev = events.shift();
        var kind = EVENT_CLICK;
        if (ev.kind === "input")  kind = EVENT_INPUT;
        if (ev.kind === "change") kind = EVENT_CHANGE;
        if (ev.kind === "submit") kind = EVENT_SUBMIT;
        if (typeof exports.alloc === "function") {
          var bytes = encoder.encode(ev.targetId);
          var ptr   = exports.alloc(bytes.length);
          new Uint8Array(memory.buffer, ptr, bytes.length).set(bytes);
          exports.__forge_receive_events(kind, ptr, bytes.length);
        } else {
          exports.__forge_receive_events(kind, 0, 0);
        }
        applyPendingCommands();
      }
    }

    return { applyCommands, flushEvents, applyPendingCommands };
  }

  // ─── SSR ハイドレーション ────────────────────────────────────────────────
  //
  // SSR が出力した data-on-* / data-reactive 属性から、WASM 関数を紐付ける。
  // WASM のロード完了前でもイベントをキューに積んでおき、ロード後に再生する。

  function setupSsrHydration(pendingEvents) {
    // data-on-click="handlerName" 要素にイベントリスナーを事前登録
    var clickEls = document.querySelectorAll("[data-on-click]");
    for (var i = 0; i < clickEls.length; i++) {
      (function (el) {
        var handler = el.getAttribute("data-on-click");
        el.addEventListener("click", function () {
          pendingEvents.push({ kind: "click", handler: handler, targetId: el.id });
        });
      })(clickEls[i]);
    }
    var inputEls = document.querySelectorAll("[data-on-input]");
    for (var i = 0; i < inputEls.length; i++) {
      (function (el) {
        var handler = el.getAttribute("data-on-input");
        el.addEventListener("input", function () {
          pendingEvents.push({ kind: "input", handler: handler, targetId: el.id });
        });
      })(inputEls[i]);
    }
  }

  // SSR 後のリアクティブ要素を WASM の状態で更新する
  // (WASM が __forge_init / __forge_attach を呼び出した後に apply されるため
  //  通常はコマンドバッファ経由で自動更新される)
  function refreshReactiveElements(exports, memory) {
    var els = document.querySelectorAll("[data-reactive]");
    if (!els.length) return;
    // WASM 側でリアクティブ値エクスポートがある場合に読み取る
    // 現在の generate_wasm_rust() は状態変数を直接エクスポートしないため
    // コマンドバッファ経由（OP_SET_TEXT）で更新が行われる。
    // 将来の拡張: WASM が `get_state_<name>() -> i32` をエクスポートすれば
    //            ここで直接読み取ることができる。
  }

  // ─── WASM ロード ────────────────────────────────────────────────────────

  async function load(url) {
    var pendingEvents = [];

    // SSR 属性からの事前ハイドレーションを設定
    if (typeof document !== "undefined") {
      setupSsrHydration(pendingEvents);
    }

    // memRef は importObject.env に渡す前に作り、
    // インスタンス化後に memory と exports を埋める
    var memRef = { current: null, exports: null, applyPending: null };
    var envImports = createEnvImports(memRef);

    var response = await fetch(url);
    var bytes    = await response.arrayBuffer();
    var result   = await WebAssembly.instantiate(bytes, { env: envImports });
    var inst     = result.instance;
    var memory   = inst.exports.memory;

    // memRef を完成させる
    memRef.current = memory;
    memRef.exports = inst.exports;

    var runtime = createRuntime(memory, inst.exports);
    memRef.applyPending = runtime.applyPendingCommands;

    // SSR HTML にアタッチ（リスナー登録 + 状態反映）
    var attached = 0;
    if (typeof inst.exports.__forge_attach === "function") {
      inst.exports.__forge_attach();
      attached = runtime.applyPendingCommands();
    }
    // アタッチできなかった場合は初期化
    if (!attached && typeof inst.exports.__forge_init === "function") {
      inst.exports.__forge_init();
      runtime.applyPendingCommands();
    }

    // WASM ロード前にキューに積まれたイベントを再生
    if (pendingEvents.length > 0 && typeof inst.exports.__forge_receive_events === "function") {
      for (var i = 0; i < pendingEvents.length; i++) {
        var ev    = pendingEvents[i];
        var kind  = EVENT_CLICK;
        if (ev.kind === "input")  kind = EVENT_INPUT;
        if (ev.kind === "change") kind = EVENT_CHANGE;
        if (ev.kind === "submit") kind = EVENT_SUBMIT;
        if (typeof inst.exports.alloc === "function") {
          var bytes2 = encoder.encode(ev.targetId);
          var ptr2   = inst.exports.alloc(bytes2.length);
          new Uint8Array(memory.buffer, ptr2, bytes2.length).set(bytes2);
          inst.exports.__forge_receive_events(kind, ptr2, bytes2.length);
        }
        runtime.applyPendingCommands();
      }
      pendingEvents.length = 0;
    }

    return { instance: inst, memory: memory, runtime: runtime };
  }

  // ─── data-bloom-wasm 属性による自動ロード ───────────────────────────────

  function autoLoad() {
    var script = document.currentScript || (function () {
      var scripts = document.querySelectorAll("script[data-bloom-wasm]");
      return scripts[scripts.length - 1] || null;
    })();
    if (!script) return;
    var wasmPath = script.getAttribute("data-bloom-wasm");
    if (!wasmPath) return;
    if (document.readyState === "loading") {
      document.addEventListener("DOMContentLoaded", function () { load(wasmPath); });
    } else {
      load(wasmPath);
    }
  }

  // ─── グローバル公開 ──────────────────────────────────────────────────────

  global.ForgeBloom = {
    load: load,
    createRuntime: createRuntime,
    createEnvImports: createEnvImports,
    constants: {
      OP_SET_TEXT:       OP_SET_TEXT,
      OP_SET_ATTR:       OP_SET_ATTR,
      OP_ADD_LISTENER:   OP_ADD_LISTENER,
      OP_REMOVE_LISTENER:OP_REMOVE_LISTENER,
      OP_SET_CLASS:      OP_SET_CLASS,
      OP_INSERT_NODE:    OP_INSERT_NODE,
      OP_REMOVE_NODE:    OP_REMOVE_NODE,
      OP_REPLACE_INNER:  OP_REPLACE_INNER,
      OP_ATTACH:         OP_ATTACH,
      EVENT_CLICK:       EVENT_CLICK,
      EVENT_INPUT:       EVENT_INPUT,
      EVENT_CHANGE:      EVENT_CHANGE,
      EVENT_SUBMIT:      EVENT_SUBMIT,
    },
  };

  // script タグの data-bloom-wasm 属性が設定されていれば自動ロード
  if (typeof document !== "undefined") {
    autoLoad();
  }
})(globalThis);
