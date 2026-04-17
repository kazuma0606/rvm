pub mod backend;

use tower_lsp::{LspService, Server};

pub use backend::Backend;

pub async fn serve_stdio() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();
    let (service, socket) = LspService::new(Backend::with_client);
    Server::new(stdin, stdout, socket).serve(service).await;
}

pub fn run_stdio_blocking() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("forge-lsp runtime");
    runtime.block_on(serve_stdio());
}
