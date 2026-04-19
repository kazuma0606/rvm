// forge-vm: インタープリタ + Value 型
// Phase 2 で実装する
// 仕様: forge/spec_v0.0.1.md §2

pub mod env;
pub mod interpreter;
pub mod test_runner;
pub mod value;

pub use interpreter::vm_bloom_render_wasm;
