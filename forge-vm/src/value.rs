// forge-vm: Value 型定義
// Phase 2-A 実装
// 注意: Value::Nil は存在しない。Option / Unit を使う。

use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::rc::Rc;

use forge_compiler::ast::Expr;

/// クロージャがキャプチャする環境（変数名 → Value のマップ）
pub type CapturedEnv = Rc<RefCell<HashMap<String, Value>>>;

/// ネイティブ関数ラッパー（Fn トレイトオブジェクトを Rc で保持）
pub struct NativeFn(pub Rc<dyn Fn(Vec<Value>) -> Result<Value, String>>);

impl std::fmt::Debug for NativeFn {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "<native fn>")
    }
}

impl Clone for NativeFn {
    fn clone(&self) -> Self {
        NativeFn(Rc::clone(&self.0))
    }
}

/// ランタイム値
/// Nil は廃止 — Option は Value::Option(None/Some) で表現する
#[derive(Debug, Clone)]
pub enum Value {
    /// 整数 (i64)
    Int(i64),
    /// 浮動小数点 (f64)
    Float(f64),
    /// 文字列
    String(String),
    /// 真偽値
    Bool(bool),
    /// 空値（戻り値なし関数、ループ本体など）
    Unit,
    /// Option<Value>
    Option(Option<Box<Value>>),
    /// Result<Value, String>
    Result(Result<Box<Value>, String>),
    /// 共有可変リスト
    List(Rc<RefCell<Vec<Value>>>),
    /// ForgeScript クロージャ
    Closure {
        params: Vec<String>,
        body: Box<Expr>,
        env: CapturedEnv,
    },
    /// ネイティブ（Rust）関数
    NativeFunction(NativeFn),
    /// struct インスタンス
    Struct {
        type_name: String,
        fields: Rc<RefCell<HashMap<String, Value>>>,
    },
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::Int(a),    Value::Int(b))    => a == b,
            (Value::Float(a),  Value::Float(b))  => a == b,
            (Value::String(a), Value::String(b)) => a == b,
            (Value::Bool(a),   Value::Bool(b))   => a == b,
            (Value::Unit,      Value::Unit)      => true,
            (Value::Option(a), Value::Option(b)) => a == b,
            (Value::Result(Ok(a)),  Value::Result(Ok(b)))  => a == b,
            (Value::Result(Err(a)), Value::Result(Err(b))) => a == b,
            (Value::List(a), Value::List(b)) => *a.borrow() == *b.borrow(),
            (Value::Struct { type_name: ta, fields: fa }, Value::Struct { type_name: tb, fields: fb }) => {
                ta == tb && *fa.borrow() == *fb.borrow()
            }
            // クロージャ・ネイティブ関数は参照等価性なし
            _ => false,
        }
    }
}

impl Hash for Value {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // discriminant でバリアントを区別
        std::mem::discriminant(self).hash(state);
        match self {
            Value::Int(n)    => n.hash(state),
            Value::Float(f)  => f.to_bits().hash(state),
            Value::String(s) => s.hash(state),
            Value::Bool(b)   => b.hash(state),
            Value::Unit      => {},
            Value::Option(Some(v)) => v.hash(state),
            Value::Option(None)    => {},
            Value::Result(Ok(v))   => v.hash(state),
            Value::Result(Err(e))  => e.hash(state),
            Value::List(items) => {
                for item in items.borrow().iter() {
                    item.hash(state);
                }
            }
            Value::Struct { type_name, fields } => {
                type_name.hash(state);
                // フィールドをキー順でソートしてハッシュ化（決定論的）
                let borrow = fields.borrow();
                let mut pairs: Vec<(&String, &Value)> = borrow.iter().collect();
                pairs.sort_by_key(|(k, _)| k.as_str());
                for (k, v) in pairs {
                    k.hash(state);
                    v.hash(state);
                }
            }
            // クロージャ・ネイティブ関数はハッシュ不可 → ポインタアドレスで代替
            Value::Closure { .. }    => 0_u8.hash(state),
            Value::NativeFunction(_) => 1_u8.hash(state),
        }
    }
}

impl std::fmt::Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::Int(n)    => write!(f, "{}", n),
            Value::Float(n)  => write!(f, "{}", n),
            Value::String(s) => write!(f, "{}", s),
            Value::Bool(b)   => write!(f, "{}", b),
            Value::Unit      => Ok(()), // 表示なし
            Value::Option(Some(v)) => write!(f, "some({})", v),
            Value::Option(None)    => write!(f, "none"),
            Value::Result(Ok(v))   => write!(f, "ok({})", v),
            Value::Result(Err(e))  => write!(f, "err({})", e),
            Value::List(items) => {
                let items = items.borrow();
                write!(f, "[")?;
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", item)?;
                }
                write!(f, "]")
            }
            Value::Closure { .. }    => write!(f, "<closure>"),
            Value::NativeFunction(_) => write!(f, "<function>"),
            Value::Struct { type_name, fields } => {
                write!(f, "{} {{", type_name)?;
                let fields = fields.borrow();
                let mut sorted: Vec<(&String, &Value)> = fields.iter().collect();
                sorted.sort_by_key(|(k, _)| k.as_str());
                for (i, (k, v)) in sorted.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, " {}: {}", k, v)?;
                }
                write!(f, " }}")
            }
        }
    }
}

impl Value {
    /// 型名を文字列で返す（`type_of` 組み込み関数用）
    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Int(_)            => "number",
            Value::Float(_)          => "float",
            Value::String(_)         => "string",
            Value::Bool(_)           => "bool",
            Value::Unit              => "unit",
            Value::Option(_)         => "option",
            Value::Result(_)         => "result",
            Value::List(_)           => "list",
            Value::Closure { .. }    => "closure",
            Value::NativeFunction(_) => "function",
            Value::Struct { .. }     => "struct",
        }
    }

    /// struct の場合、型名を動的に返す
    pub fn dynamic_type_name(&self) -> String {
        match self {
            Value::Struct { type_name, .. } => type_name.clone(),
            _ => self.type_name().to_string(),
        }
    }

    /// struct フィールドを深くクローンする（@derive(Clone) 用）
    pub fn deep_clone(&self) -> Value {
        match self {
            Value::Struct { type_name, fields } => {
                let cloned: HashMap<String, Value> = fields.borrow()
                    .iter()
                    .map(|(k, v)| (k.clone(), v.deep_clone()))
                    .collect();
                Value::Struct {
                    type_name: type_name.clone(),
                    fields: Rc::new(RefCell::new(cloned)),
                }
            }
            Value::List(items) => {
                let cloned: Vec<Value> = items.borrow().iter().map(|v| v.deep_clone()).collect();
                Value::List(Rc::new(RefCell::new(cloned)))
            }
            other => other.clone(),
        }
    }
}

// ── テスト ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_value_display_int() {
        assert_eq!(Value::Int(42).to_string(), "42");
    }

    #[test]
    fn test_value_display_float() {
        assert_eq!(Value::Float(3.14).to_string(), "3.14");
    }

    #[test]
    fn test_value_display_bool() {
        assert_eq!(Value::Bool(true).to_string(), "true");
        assert_eq!(Value::Bool(false).to_string(), "false");
    }

    #[test]
    fn test_value_display_string() {
        assert_eq!(Value::String("hi".to_string()).to_string(), "hi");
    }

    #[test]
    fn test_value_display_none() {
        assert_eq!(Value::Option(None).to_string(), "none");
    }

    #[test]
    fn test_value_display_some() {
        let v = Value::Option(Some(Box::new(Value::Int(1))));
        assert_eq!(v.to_string(), "some(1)");
    }

    #[test]
    fn test_value_display_list() {
        let list = Value::List(Rc::new(RefCell::new(vec![
            Value::Int(1),
            Value::Int(2),
            Value::Int(3),
        ])));
        assert_eq!(list.to_string(), "[1, 2, 3]");
    }

    #[test]
    fn test_no_nil() {
        // Value::Nil が存在しないことをコンパイルで保証
        // Unit と Option(None) が Nil の代替
        let _unit: Value = Value::Unit;
        let _none: Value = Value::Option(None);
        let _some: Value = Value::Option(Some(Box::new(Value::Int(42))));
        // type_name も確認
        assert_eq!(Value::Unit.type_name(), "unit");
        assert_eq!(Value::Option(None).type_name(), "option");
    }
}
