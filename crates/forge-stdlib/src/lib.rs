// forge-stdlib: 標準ライブラリ（ネイティブ関数）
// Phase 2-C で実装する
// 仕様: forge/spec_v0.0.1.md §10

pub mod builtins;
pub mod collections;
pub mod fs;
pub mod json;
pub mod net;

pub mod forge {
    pub mod std {
        pub use crate::fs;
        pub use crate::json;
        pub use crate::net;
    }
}
