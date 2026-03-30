// forge-compiler: 型チェッカー モジュール
// Phase 4-A 実装

pub mod types;
pub mod checker;

pub use types::Type;
pub use checker::{TypeChecker, TypeError, type_check_source};
