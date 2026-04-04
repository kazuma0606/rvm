// forge-vm: 環境（変数スコープ）
// Phase 2-B で実装する

use std::collections::HashMap;
use crate::value::Value;

/// 変数環境（スコープチェーン）
pub struct Env {
    // Phase 2-B で Rc<RefCell<Env>> による親スコープ参照を追加
    #[allow(dead_code)]
    vars: HashMap<String, Value>,
}

impl Env {
    pub fn new() -> Self {
        Self { vars: HashMap::new() }
    }
}

impl Default for Env {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_env_stub_compiles() {
        let _env = Env::new();
    }
}
