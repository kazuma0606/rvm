// forge-dap: DAP サーバーエントリポイント（DBG-4-B）
//
// 使用方法:
//   forge-dap
//
// VS Code からは launch.json の debugger として起動される。
// stdin/stdout で DAP JSON-over-stdio プロトコルをやり取りする。

use std::io::{BufReader, BufWriter};

use forge_dap::adapter::DapServer;

fn main() {
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut reader = BufReader::new(stdin.lock());
    let mut writer = BufWriter::new(stdout.lock());

    let mut server = DapServer::new();
    if let Err(e) = server.run(&mut reader, &mut writer) {
        eprintln!("[forge-dap] fatal: {}", e);
        std::process::exit(1);
    }
}
