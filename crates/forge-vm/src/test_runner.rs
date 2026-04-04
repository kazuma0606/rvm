// forge-vm: テスト実行サポート（FT-1-D）

/// テスト1件の実行結果
#[derive(Debug, Clone)]
pub struct TestResult {
    pub name: String,
    pub passed: bool,
    pub failure_message: Option<String>,
}
