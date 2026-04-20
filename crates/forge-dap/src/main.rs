// forge-dap: DAP サーバーエントリポイント
use forge_dap::adapter::DapServer;

fn main() {
    let mut server = DapServer::new();
    // 通信ループを開始（内部で非同期処理を行う）
    if let Err(e) = server.run_stdio() {
        eprintln!("[forge-dap] fatal: {}", e);
        std::process::exit(1);
    }
}
