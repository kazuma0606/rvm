// forge-stdlib: 標準ライブラリ（ネイティブ関数）
// Phase 2-C で実装する
// 仕様: forge/spec_v0.0.1.md §10

pub mod builtins;
pub mod cache;
pub mod collections;
pub mod config;
pub mod env;
pub mod event;
pub mod fs;
pub mod io;
pub mod json;
pub mod log;
pub mod metrics;
pub mod net;
pub mod pipeline;
pub mod process;
pub mod random;
pub mod regex;
pub mod retry;
pub mod string;
pub mod uuid;

pub mod forge {
    pub mod std {
        pub use crate::cache;
        pub use crate::config;
        pub use crate::env;
        pub use crate::event;
        pub use crate::fs;
        pub use crate::io;
        pub use crate::json;
        pub use crate::log;
        pub use crate::metrics;
        pub use crate::net;
        pub use crate::pipeline;
        pub use crate::process;
        pub use crate::random;
        pub use crate::regex;
        pub use crate::retry;
        pub use crate::string;
        pub use crate::uuid;
    }
}
