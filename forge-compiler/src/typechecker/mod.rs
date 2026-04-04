// forge-compiler: 型チェッカー モジュール
// Phase 4-A 実装

pub mod checker;
pub mod types;

pub use checker::{type_check_source, TypeChecker, TypeError};
pub use types::Type;
