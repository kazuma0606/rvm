#[tokio::main]
async fn main() {
    forge_lsp::serve_stdio().await;
}
