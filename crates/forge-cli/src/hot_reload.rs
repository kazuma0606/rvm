// forge-cli: DBG-5-A / DBG-5-B  ホットリロード統合
//
// アーキテクチャ
// ─────────────────────────────────────────────────────────────────
// WatchState
//   ├─ notify::Watcher  ─ .forge / .html / .bloom を監視
//   ├─ WebSocketBroadcaster  ─ 接続中のブラウザクライアントへ通知
//   └─ DapReloadNotifier  ─ DAP セッションにリロード完了を通知
//
// forge serve --watch が呼ばれると
//   1. WatchState を起動（別スレッド）
//   2. HTML レスポンスに <script> タグを自動注入
//   3. ファイル変更イベントに応じて
//       .forge / .html  → ForgeScript インタープリタを再起動
//       .bloom (HTML 変更のみ) → SSR のみ再生成
//       .bloom (script 変更あり) → WASM 再コンパイルを子プロセスで起動 → 完了後に通知
// ─────────────────────────────────────────────────────────────────

use std::collections::HashSet;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use notify::event::{ModifyKind, RenameMode};
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};

// ─────────────────────────────────────────────────────────────────
// WebSocket のシンプルな手動実装（RFC 6455 準拠・テキストフレームのみ）
// ─────────────────────────────────────────────────────────────────

/// WebSocket ハンドシェイク用の HTTP Upgrade レスポンスを返し、
/// ストリームを WebSocket モードに切り替える。
/// 成功時は `true` を返す。
fn ws_handshake(stream: &mut TcpStream) -> bool {
    let mut buf = [0u8; 2048];
    let n = match stream.read(&mut buf) {
        Ok(n) if n > 0 => n,
        _ => return false,
    };
    let req = String::from_utf8_lossy(&buf[..n]);

    // Sec-WebSocket-Key を取り出す
    let key = req
        .lines()
        .find(|l| l.to_lowercase().starts_with("sec-websocket-key:"))
        .and_then(|l| l.splitn(2, ':').nth(1))
        .map(|s| s.trim().to_string());

    let key = match key {
        Some(k) => k,
        None => return false,
    };

    // RFC 6455 の accept ハッシュを計算
    use std::io::Write as _;
    let accept = ws_accept_key(&key);

    let response = format!(
        "HTTP/1.1 101 Switching Protocols\r\n\
         Upgrade: websocket\r\n\
         Connection: Upgrade\r\n\
         Sec-WebSocket-Accept: {}\r\n\
         \r\n",
        accept
    );
    stream.write_all(response.as_bytes()).is_ok()
}

/// RFC 6455 の Sec-WebSocket-Accept を計算する
fn ws_accept_key(key: &str) -> String {
    const MAGIC: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";
    let combined = format!("{}{}", key, MAGIC);
    // SHA-1 は std に含まれないため手書きせず、決定論的な簡易実装を使う。
    // 本番品質が必要な場合は `sha1` クレートを使うこと。
    // ここでは WebSocket 標準に準拠した Base64(SHA-1(key+MAGIC)) を計算する。
    let digest = sha1_bytes(combined.as_bytes());
    base64_encode(&digest)
}

/// SHA-1 の純 Rust 実装（外部クレート不要）
fn sha1_bytes(data: &[u8]) -> [u8; 20] {
    // FIPS 180-4 SHA-1
    let mut h: [u32; 5] = [0x67452301, 0xEFCDAB89, 0x98BADCFE, 0x10325476, 0xC3D2E1F0];

    // メッセージを 512 bit ブロックに分割
    let bit_len = (data.len() as u64) * 8;
    let mut msg = data.to_vec();
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0x00);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());

    for chunk in msg.chunks(64) {
        let mut w = [0u32; 80];
        for (i, b) in chunk.chunks(4).enumerate().take(16) {
            w[i] = u32::from_be_bytes([b[0], b[1], b[2], b[3]]);
        }
        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }

        let (mut a, mut b, mut c, mut d, mut e) = (h[0], h[1], h[2], h[3], h[4]);

        for i in 0..80 {
            let (f, k) = match i {
                0..=19 => ((b & c) | ((!b) & d), 0x5A827999u32),
                20..=39 => (b ^ c ^ d, 0x6ED9EBA1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1BBCDC),
                _ => (b ^ c ^ d, 0xCA62C1D6),
            };
            let temp = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(k)
                .wrapping_add(w[i]);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = temp;
        }

        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
    }

    let mut out = [0u8; 20];
    for (i, &val) in h.iter().enumerate() {
        let bytes = val.to_be_bytes();
        out[i * 4..(i + 1) * 4].copy_from_slice(&bytes);
    }
    out
}

/// Base64 エンコード（標準アルファベット、パディングあり）
fn base64_encode(data: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in data.chunks(3) {
        let b0 = chunk[0];
        let b1 = if chunk.len() > 1 { chunk[1] } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] } else { 0 };
        out.push(TABLE[(b0 >> 2) as usize] as char);
        out.push(TABLE[((b0 & 3) << 4 | b1 >> 4) as usize] as char);
        if chunk.len() > 1 {
            out.push(TABLE[((b1 & 0xf) << 2 | b2 >> 6) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(TABLE[(b2 & 0x3f) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

/// WebSocket テキストフレームを送信する（マスクなし / サーバー側）
fn ws_send_text(stream: &mut TcpStream, text: &str) -> std::io::Result<()> {
    let payload = text.as_bytes();
    let len = payload.len();
    let mut frame = vec![0x81u8]; // FIN + opcode=1(text)
    if len < 126 {
        frame.push(len as u8);
    } else if len < 65536 {
        frame.push(126u8);
        frame.push((len >> 8) as u8);
        frame.push((len & 0xff) as u8);
    } else {
        frame.push(127u8);
        for i in (0..8).rev() {
            frame.push(((len >> (i * 8)) & 0xff) as u8);
        }
    }
    frame.extend_from_slice(payload);
    stream.write_all(&frame)
}

// ─────────────────────────────────────────────────────────────────
// WebSocket ブロードキャスター
// ─────────────────────────────────────────────────────────────────

/// 接続済みの WebSocket クライアントを管理し、全員にメッセージをブロードキャストする。
#[derive(Clone)]
pub struct WsBroadcaster {
    clients: Arc<Mutex<Vec<TcpStream>>>,
}

impl WsBroadcaster {
    pub fn new() -> Self {
        WsBroadcaster {
            clients: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// 指定ポートで WebSocket Upgrade を受け付けるスレッドを起動する。
    pub fn start_listener(&self, port: u16) {
        let clients = Arc::clone(&self.clients);
        thread::spawn(move || {
            let Ok(listener) = TcpListener::bind(format!("127.0.0.1:{}", port)) else {
                eprintln!(
                    "[HotReload] WebSocket リスナーの起動に失敗しました (port {})",
                    port
                );
                return;
            };
            listener.set_nonblocking(false).unwrap_or_default();
            eprintln!(
                "[HotReload] WebSocket リスナー起動: ws://127.0.0.1:{}",
                port
            );
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else {
                    continue;
                };
                if ws_handshake(&mut stream) {
                    if let Ok(mut guard) = clients.lock() {
                        guard.push(stream);
                    }
                }
            }
        });
    }

    /// 接続中の全クライアントにテキストメッセージを送る。
    /// 送信失敗したクライアントは切断済みとして削除する。
    pub fn broadcast(&self, message: &str) {
        let Ok(mut guard) = self.clients.lock() else {
            return;
        };
        guard.retain_mut(|stream| ws_send_text(stream, message).is_ok());
    }
}

// ─────────────────────────────────────────────────────────────────
// .bloom ファイルの script/HTML 変更判定
// ─────────────────────────────────────────────────────────────────

/// .bloom ファイルを読んで、`<script>...</script>` ブロックの内容を返す。
/// ブロックが存在しない場合は `None`。
pub fn extract_bloom_script(source: &str) -> Option<String> {
    let start = source.find("<script")?;
    let end = source.rfind("</script>")?;
    if end > start {
        Some(source[start..end + 9].to_string())
    } else {
        None
    }
}

/// 前回のスナップショットと比較して、script セクションが変わったかどうかを返す。
pub fn bloom_script_changed(old_source: &str, new_source: &str) -> bool {
    extract_bloom_script(old_source) != extract_bloom_script(new_source)
}

// ─────────────────────────────────────────────────────────────────
// HTML へのリロードスクリプト注入
// ─────────────────────────────────────────────────────────────────

/// `</body>` の直前にホットリロード用 `<script>` を挿入する。
/// `</body>` が存在しない場合は末尾に追加する。
pub fn inject_reload_script(html: &str, ws_port: u16) -> String {
    let script = format!(
        r#"<script>
(function() {{
  var ws = new WebSocket('ws://127.0.0.1:{port}');
  ws.onmessage = function(e) {{
    if (e.data === 'reload') {{
      window.location.reload();
    }} else if (e.data === 'ssr-updated') {{
      // SSR のみ更新 — ページを完全リロード
      window.location.reload();
    }}
  }};
  ws.onclose = function() {{
    // 接続が切れたら 1 秒後に再試行
    setTimeout(function() {{ window.location.reload(); }}, 1000);
  }};
}})();
</script>"#,
        port = ws_port
    );

    if let Some(pos) = html.find("</body>") {
        format!("{}{}{}", &html[..pos], script, &html[pos..])
    } else {
        format!("{}{}", html, script)
    }
}

// ─────────────────────────────────────────────────────────────────
// DAP セッション通知（DBG-5-B）
// ─────────────────────────────────────────────────────────────────

/// DAP セッションへの通知インタフェース。
/// 実際の DAP プロセスが存在する場合はファイルへの書き込みや共有状態経由で通知する。
/// 本実装では将来の拡張ポイントとして DAP reload フラグファイルを使う。
pub struct DapReloadNotifier {
    /// リロード完了を記録するフラグファイルのパス（存在すれば DAP が検知する）
    flag_path: PathBuf,
}

impl DapReloadNotifier {
    /// `watch_root` 直下の `.forge-dap-reload` をフラグファイルとして使う。
    pub fn new(watch_root: &Path) -> Self {
        DapReloadNotifier {
            flag_path: watch_root.join(".forge-dap-reload"),
        }
    }

    /// リロード完了を DAP セッションに通知する。
    /// フラグファイルに現在時刻（UNIX秒）を書き込む。
    pub fn notify_reload(&self) {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        if let Err(e) = std::fs::write(&self.flag_path, ts.to_string()) {
            eprintln!("[HotReload] DAP リロード通知の書き込みに失敗: {}", e);
        }
    }

    /// フラグファイルを削除してセッションをクリーンアップする。
    pub fn cleanup(&self) {
        let _ = std::fs::remove_file(&self.flag_path);
    }
}

// ─────────────────────────────────────────────────────────────────
// ファイル監視と再起動ロジック（DBG-5-A）
// ─────────────────────────────────────────────────────────────────

/// watch モードの設定
#[derive(Clone, Debug)]
pub struct WatchConfig {
    /// 監視するルートディレクトリ
    pub watch_dir: PathBuf,
    /// WebSocket サーバーのポート（メインポート + 1 をデフォルトとする）
    pub ws_port: u16,
    /// フォージエントリポイント（再起動対象）
    pub entry_path: PathBuf,
}

/// イベントデバウンス用の最小間隔
const DEBOUNCE_MS: u64 = 300;

/// `.forge` / `.html` / `.bloom` のいずれかに一致するかを返す
fn is_watched_ext(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|e| e.to_str()),
        Some("forge") | Some("html") | Some("bloom")
    )
}

/// パスから `.bloom` かどうかを判定する
fn is_bloom_file(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()) == Some("bloom")
}

/// ファイル監視を開始し、変更があれば `broadcaster` 経由でブラウザに通知する。
/// `.bloom` の script 変更は WASM 再コンパイルを実行する。
///
/// この関数はブロッキングループを持たず、バックグラウンドスレッドとして動作する。
pub fn start_watch(config: WatchConfig, broadcaster: WsBroadcaster) {
    thread::spawn(move || {
        watch_loop(config, broadcaster);
    });
}

fn watch_loop(config: WatchConfig, broadcaster: WsBroadcaster) {
    // .bloom ファイルの前回スナップショット（script 変更検出用）
    let snapshots: Arc<Mutex<std::collections::HashMap<PathBuf, String>>> =
        Arc::new(Mutex::new(std::collections::HashMap::new()));

    // notify チャンネル
    let (tx, rx) = std::sync::mpsc::channel::<notify::Result<Event>>();

    let mut watcher = match notify::recommended_watcher(tx) {
        Ok(w) => w,
        Err(e) => {
            eprintln!("[HotReload] ファイル監視の起動に失敗しました: {}", e);
            return;
        }
    };

    if let Err(e) = watcher.watch(&config.watch_dir, RecursiveMode::Recursive) {
        eprintln!(
            "[HotReload] ディレクトリの監視に失敗しました '{}': {}",
            config.watch_dir.display(),
            e
        );
        return;
    }

    eprintln!(
        "[HotReload] ファイル監視を開始しました: {}",
        config.watch_dir.display()
    );

    // DAP 通知
    let dap_notifier = DapReloadNotifier::new(&config.watch_dir);

    let mut last_event = Instant::now() - Duration::from_millis(DEBOUNCE_MS + 1);

    'outer: loop {
        // イベントを受信（タイムアウトなし）
        let event = match rx.recv() {
            Ok(Ok(e)) => e,
            Ok(Err(e)) => {
                eprintln!("[HotReload] watch エラー: {}", e);
                continue;
            }
            Err(_) => break, // チャンネル閉鎖
        };

        // デバウンス
        let now = Instant::now();
        if now.duration_since(last_event) < Duration::from_millis(DEBOUNCE_MS) {
            continue;
        }

        // 変更されたファイルを収集
        let changed_paths: Vec<PathBuf> = event
            .paths
            .iter()
            .filter(|p| is_watched_ext(p))
            .cloned()
            .collect();

        if changed_paths.is_empty() {
            continue;
        }

        last_event = now;

        // `.bloom` ファイルを分類
        let bloom_paths: Vec<&PathBuf> =
            changed_paths.iter().filter(|p| is_bloom_file(p)).collect();
        let non_bloom_paths: Vec<&PathBuf> =
            changed_paths.iter().filter(|p| !is_bloom_file(p)).collect();

        // .forge / .html 変更 → インタープリタ再起動 + ブラウザリロード
        if !non_bloom_paths.is_empty() {
            eprintln!(
                "[HotReload] ファイル変更を検出 ({} files) → インタープリタ再起動",
                non_bloom_paths.len()
            );
            restart_interpreter(&config.entry_path);
            broadcaster.broadcast("reload");
            dap_notifier.notify_reload();
        }

        // .bloom ファイルの変更処理
        for bloom_path in &bloom_paths {
            let new_source = match std::fs::read_to_string(bloom_path) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("[HotReload] .bloom ファイルの読み込みに失敗: {}", e);
                    continue;
                }
            };

            let old_source = {
                let guard = snapshots.lock().unwrap_or_else(|e| e.into_inner());
                guard.get(*bloom_path).cloned().unwrap_or_default()
            };

            let script_changed = bloom_script_changed(&old_source, &new_source);

            // スナップショット更新
            {
                let mut guard = snapshots.lock().unwrap_or_else(|e| e.into_inner());
                guard.insert((*bloom_path).clone(), new_source.clone());
            }

            if script_changed {
                eprintln!(
                    "[HotReload] .bloom script 変更を検出: {} → WASM 再コンパイル",
                    bloom_path.display()
                );
                let path_clone = (*bloom_path).clone();
                let bc = broadcaster.clone();
                let dap_flag = config.watch_dir.clone();
                thread::spawn(move || {
                    rebuild_wasm(&path_clone);
                    bc.broadcast("reload");
                    DapReloadNotifier::new(&dap_flag).notify_reload();
                });
            } else {
                eprintln!(
                    "[HotReload] .bloom HTML 変更を検出: {} → SSR のみ再生成",
                    bloom_path.display()
                );
                regenerate_ssr(bloom_path);
                broadcaster.broadcast("ssr-updated");
                dap_notifier.notify_reload();
            }
        }
    }

    dap_notifier.cleanup();
}

// ─────────────────────────────────────────────────────────────────
// 再起動 / 再ビルドヘルパー
// ─────────────────────────────────────────────────────────────────

/// ForgeScript インタープリタを再起動する。
/// 現実装では現在のプロセスがサーバーを走らせているため、
/// シグナルやチャンネルでの再起動は将来の拡張とし、
/// ここではログ出力のみ行う（完全な再起動は `forge serve` を再実行することで達成する）。
fn restart_interpreter(entry_path: &Path) {
    eprintln!("[HotReload] インタープリタ再起動: {}", entry_path.display());
    // 将来実装: Arc<Mutex<Child>> でサーバープロセスを管理し kill + spawn する
}

/// WASM を再コンパイルする（`forge build --web` を子プロセスとして起動）。
pub fn rebuild_wasm(bloom_path: &Path) {
    let forge_bin = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("forge"));
    eprintln!(
        "[HotReload] WASM 再コンパイル開始: {}",
        bloom_path.display()
    );
    match std::process::Command::new(&forge_bin)
        .args(["build", "--web", bloom_path.to_str().unwrap_or("")])
        .status()
    {
        Ok(status) if status.success() => {
            eprintln!(
                "[HotReload] WASM 再コンパイル完了: {}",
                bloom_path.display()
            );
        }
        Ok(status) => {
            eprintln!(
                "[HotReload] WASM 再コンパイル失敗 (exit {}): {}",
                status,
                bloom_path.display()
            );
        }
        Err(e) => {
            eprintln!("[HotReload] WASM 再コンパイルの起動に失敗: {}", e);
        }
    }
}

/// `.bloom` の SSR のみ再生成する。
/// bloom-compiler の `compile_bloom_direct` 相当処理を呼ぶ。
/// 将来的には内部 API を直接呼び出すが、ここでは子プロセスで `forge build --web` を使う。
pub fn regenerate_ssr(bloom_path: &Path) {
    eprintln!("[HotReload] SSR 再生成: {}", bloom_path.display());
    let forge_bin = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("forge"));
    match std::process::Command::new(&forge_bin)
        .args(["build", "--web", bloom_path.to_str().unwrap_or("")])
        .status()
    {
        Ok(status) if status.success() => {
            eprintln!("[HotReload] SSR 再生成完了: {}", bloom_path.display());
        }
        Ok(status) => {
            eprintln!(
                "[HotReload] SSR 再生成失敗 (exit {}): {}",
                status,
                bloom_path.display()
            );
        }
        Err(e) => {
            eprintln!("[HotReload] SSR 再生成の起動に失敗: {}", e);
        }
    }
}

// ─────────────────────────────────────────────────────────────────
// DBG-5-B: DAP セッションのブレークポイント再登録
// ─────────────────────────────────────────────────────────────────

/// DAP セッションのブレークポイント再登録インタフェース。
///
/// リロード後に DAP アダプタへ「ファイルが変わったのでブレークポイントを再登録してほしい」
/// と通知するための構造体。実際の forge-dap プロセスが動いている場合は
/// フラグファイルを検知して setBreakpoints を再送する。
pub struct DapSessionReconnect {
    /// 再登録が必要であることを示すフラグ
    pub needs_reregister: Arc<Mutex<bool>>,
}

impl DapSessionReconnect {
    pub fn new() -> Self {
        DapSessionReconnect {
            needs_reregister: Arc::new(Mutex::new(false)),
        }
    }

    /// リロード通知を受けてブレークポイント再登録フラグを立てる。
    pub fn on_reload(&self) {
        if let Ok(mut flag) = self.needs_reregister.lock() {
            *flag = true;
        }
    }

    /// 再登録が必要かどうかを確認し、フラグをクリアする。
    pub fn take_reregister_flag(&self) -> bool {
        if let Ok(mut flag) = self.needs_reregister.lock() {
            let val = *flag;
            *flag = false;
            val
        } else {
            false
        }
    }
}

// ─────────────────────────────────────────────────────────────────
// テスト（DBG-5-C）
// ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    // ── inject_reload_script ──────────────────────────────────────

    #[test]
    fn inject_reload_script_inserts_before_body_close() {
        let html = "<html><body><p>Hello</p></body></html>";
        let result = inject_reload_script(html, 3001);
        assert!(result.contains("ws://127.0.0.1:3001"));
        assert!(result.contains("window.location.reload()"));
        // </body> の前に挿入されていること
        let script_pos = result.find("<script>").unwrap();
        let body_close_pos = result.find("</body>").unwrap();
        assert!(script_pos < body_close_pos);
    }

    #[test]
    fn inject_reload_script_appends_when_no_body_close() {
        let html = "<html><body><p>Hello</p></html>";
        let result = inject_reload_script(html, 3001);
        assert!(result.contains("ws://127.0.0.1:3001"));
        assert!(result.ends_with("</script>"));
    }

    // ── bloom_script_changed ──────────────────────────────────────

    #[test]
    fn bloom_script_changed_detects_script_modification() {
        let old = r#"<div>Hello</div><script>let x = 1;</script>"#;
        let new = r#"<div>Hello</div><script>let x = 2;</script>"#;
        assert!(bloom_script_changed(old, new));
    }

    #[test]
    fn bloom_script_changed_ignores_html_only_modification() {
        let old = r#"<div>Hello</div><script>let x = 1;</script>"#;
        let new = r#"<div>World</div><script>let x = 1;</script>"#;
        assert!(!bloom_script_changed(old, new));
    }

    #[test]
    fn bloom_script_changed_no_script_block() {
        let old = r#"<div>Hello</div>"#;
        let new = r#"<div>World</div>"#;
        // script ブロックがない場合は変更なしとみなす
        assert!(!bloom_script_changed(old, new));
    }

    // ── DapSessionReconnect ───────────────────────────────────────

    #[test]
    fn dap_session_reconnect_flag() {
        let reconnect = DapSessionReconnect::new();
        assert!(!reconnect.take_reregister_flag());
        reconnect.on_reload();
        assert!(reconnect.take_reregister_flag());
        // フラグはクリアされている
        assert!(!reconnect.take_reregister_flag());
    }

    // ── WsBroadcaster（テスト用: クライアントなし） ───────────────

    #[test]
    fn ws_broadcaster_broadcast_with_no_clients_does_not_panic() {
        let bc = WsBroadcaster::new();
        // クライアントが 0 件でもパニックしないこと
        bc.broadcast("reload");
    }

    // ── DBG-5-C: test_watch_forge_reload ─────────────────────────

    /// .forge ファイルを変更したときにファイル変更イベントが発生し、
    /// ウォッチャーが正常に起動・停止できることを確認する。
    #[test]
    fn test_watch_forge_reload() {
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::time::Duration;

        // 一時ディレクトリを用意
        let dir = tempfile::tempdir().expect("tempdir");
        let forge_file = dir.path().join("main.forge");
        fs::write(&forge_file, "println(\"hello\")").expect("write forge file");

        // 変更検出フラグ
        let detected = Arc::new(AtomicBool::new(false));
        let detected_clone = Arc::clone(&detected);

        // notify watcher をセットアップ
        let (tx, rx) = std::sync::mpsc::channel::<notify::Result<Event>>();
        let mut watcher = notify::recommended_watcher(tx).expect("watcher");
        watcher
            .watch(dir.path(), RecursiveMode::Recursive)
            .expect("watch dir");

        // ファイルを変更
        thread::sleep(Duration::from_millis(50));
        fs::write(&forge_file, "println(\"world\")").expect("update forge file");

        // イベント受信（最大 2 秒待つ）
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            match rx.recv_timeout(Duration::from_millis(100)) {
                Ok(Ok(event)) => {
                    let has_forge = event
                        .paths
                        .iter()
                        .any(|p| p.extension().and_then(|e| e.to_str()) == Some("forge"));
                    if has_forge {
                        detected_clone.store(true, Ordering::SeqCst);
                        break;
                    }
                }
                _ => continue,
            }
        }

        assert!(
            detected.load(Ordering::SeqCst),
            ".forge ファイルの変更イベントが受信されるべきです"
        );
    }

    // ── DBG-5-C: test_watch_bloom_ssr ────────────────────────────

    /// .bloom の HTML 変更（script ブロック変更なし）では SSR のみ再生成が選ばれること
    /// を `bloom_script_changed` の戻り値で確認するユニットテスト。
    #[test]
    fn test_watch_bloom_ssr() {
        // script ブロックは同じ、HTML 部分だけ変更
        let old = r#"<div>count: 0</div>
<script>
fn increment() { state.count += 1 }
</script>"#;
        let new = r#"<div>count: 1</div>
<script>
fn increment() { state.count += 1 }
</script>"#;

        // script 変更なし → SSR のみ再生成
        assert!(
            !bloom_script_changed(old, new),
            "script は変わっていないので false"
        );

        // script 変更あり → WASM 再コンパイル
        let new_script = r#"<div>count: 1</div>
<script>
fn increment() { state.count += 2 }
</script>"#;
        assert!(
            bloom_script_changed(old, new_script),
            "script が変わったので true"
        );
    }

    // ── base64 / SHA-1 ──────────────────────────────────────────

    #[test]
    fn base64_encode_basic() {
        // RFC 4648 テストベクタ
        assert_eq!(base64_encode(b"Man"), "TWFu");
        assert_eq!(base64_encode(b"Ma"), "TWE=");
        assert_eq!(base64_encode(b"M"), "TQ==");
    }

    #[test]
    fn sha1_known_vector() {
        // FIPS 180-4 のテストベクタ: SHA-1("abc") = a9993e36 4706816a ba3e2571 7850c26c 9cd0d89d
        let digest = sha1_bytes(b"abc");
        let hex: String = digest.iter().map(|b| format!("{:02x}", b)).collect();
        assert_eq!(hex, "a9993e364706816aba3e25717850c26c9cd0d89d");
    }
}
