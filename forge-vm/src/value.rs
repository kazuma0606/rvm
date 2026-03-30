// forge-vm: Value 型定義
// Phase 2-A で実装する
// 注意: Value::Nil は存在しない。Unit を使う。

/// ランタイム値
/// Nil は廃止 — Option は Value::Option(None/Some) で表現する
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// 整数
    Int(i64),
    /// 浮動小数点
    Float(f64),
    /// 文字列
    String(String),
    /// 真偽値
    Bool(bool),
    /// 空値（戻り値なし関数など）
    Unit,
    /// Option<Value>
    Option(Option<Box<Value>>),
    /// Result<Value, Value>
    Result(Result<Box<Value>, Box<Value>>),
    /// リスト
    List(Vec<Value>),
}

impl std::fmt::Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::Int(n) => write!(f, "{}", n),
            Value::Float(n) => write!(f, "{}", n),
            Value::String(s) => write!(f, "{}", s),
            Value::Bool(b) => write!(f, "{}", b),
            Value::Unit => write!(f, "()"),
            Value::Option(Some(v)) => write!(f, "some({})", v),
            Value::Option(None) => write!(f, "none"),
            Value::Result(Ok(v)) => write!(f, "ok({})", v),
            Value::Result(Err(e)) => write!(f, "err({})", e),
            Value::List(items) => {
                write!(f, "[")?;
                for (i, item) in items.iter().enumerate() {
                    if i > 0 { write!(f, ", ")?; }
                    write!(f, "{}", item)?;
                }
                write!(f, "]")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_value_no_nil() {
        // Value::Nil が存在しないことをコンパイルで保証
        let _unit = Value::Unit;
        let _none: Value = Value::Option(None);
        let _some: Value = Value::Option(Some(Box::new(Value::Int(42))));
    }

    #[test]
    fn test_value_display() {
        assert_eq!(Value::Int(42).to_string(), "42");
        assert_eq!(Value::Bool(true).to_string(), "true");
        assert_eq!(Value::Unit.to_string(), "()");
        assert_eq!(Value::Option(None).to_string(), "none");
        assert_eq!(Value::Option(Some(Box::new(Value::Int(1)))).to_string(), "some(1)");
    }
}
