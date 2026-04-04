// forge-transpiler: トランスパイルエラー定義

#[derive(Debug, Clone, PartialEq)]
pub enum TranspileError {
    ParseError(String),
    UnsupportedFeature(String),
}

impl std::fmt::Display for TranspileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TranspileError::ParseError(msg) => write!(f, "構文解析エラー: {}", msg),
            TranspileError::UnsupportedFeature(msg) => write!(f, "未対応機能: {}", msg),
        }
    }
}

impl std::error::Error for TranspileError {}
