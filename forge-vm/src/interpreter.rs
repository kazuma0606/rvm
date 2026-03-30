// forge-vm: ツリーウォーキングインタープリタ
// Phase 2-B で実装する

use crate::value::Value;

/// インタープリタエラー
#[derive(Debug, Clone, PartialEq)]
pub enum RuntimeError {
    UndefinedVariable { name: String, line: usize, col: usize },
    TypeMismatch { expected: String, found: String, line: usize, col: usize },
    DivisionByZero { line: usize, col: usize },
    IndexOutOfBounds { index: i64, len: usize, line: usize, col: usize },
    Custom(String),
}

impl std::fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RuntimeError::UndefinedVariable { name, line, col } => {
                write!(f, "未定義の変数 '{}' ({}:{})", name, line, col)
            }
            RuntimeError::TypeMismatch { expected, found, line, col } => {
                write!(f, "型エラー: {} を期待しましたが {} が見つかりました ({}:{})",
                    expected, found, line, col)
            }
            RuntimeError::DivisionByZero { line, col } => {
                write!(f, "ゼロ除算エラー ({}:{})", line, col)
            }
            RuntimeError::IndexOutOfBounds { index, len, line, col } => {
                write!(f, "インデックス範囲外: {} (長さ: {}) ({}:{})", index, len, line, col)
            }
            RuntimeError::Custom(msg) => write!(f, "{}", msg),
        }
    }
}

impl std::error::Error for RuntimeError {}

/// インタープリタ本体（Phase 2-B で実装）
pub struct Interpreter {
    // Phase 2-B で環境スタックを追加
}

impl Interpreter {
    pub fn new() -> Self {
        Self {}
    }
}

impl Default for Interpreter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_interpreter_stub_compiles() {
        let _interp = Interpreter::new();
    }
}
