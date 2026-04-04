// forge-vm: Value 型定義
// Phase 2-A 実装
// 注意: Value::Nil は存在しない。Option / Unit を使う。

use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::rc::Rc;

use forge_compiler::ast::Expr;

/// クロージャがキャプチャする環境（変数名 → (Value, mutable) のマップ）
pub type CapturedEnv = Rc<RefCell<HashMap<String, (Value, bool)>>>;

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

/// enum バリアントが保持するデータ
#[derive(Debug, Clone, PartialEq)]
pub enum EnumData {
    /// データなし
    Unit,
    /// タプル形式
    Tuple(Vec<Value>),
    /// 名前付きフィールド
    Struct(HashMap<String, Value>),
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
    /// 順序付きマップ
    Map(Vec<(Value, Value)>),
    /// 順序付きセット
    Set(Vec<Value>),
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
    /// enum バリアントインスタンス
    Enum {
        type_name: String,
        variant: String,
        data: EnumData,
    },
    /// typestate インスタンス
    Typestate {
        type_name: String,
        current_state: String,
        fields: Rc<RefCell<HashMap<String, Value>>>,
    },
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::Int(a), Value::Int(b)) => a == b,
            (Value::Float(a), Value::Float(b)) => a == b,
            (Value::String(a), Value::String(b)) => a == b,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Unit, Value::Unit) => true,
            (Value::Option(a), Value::Option(b)) => a == b,
            (Value::Result(Ok(a)), Value::Result(Ok(b))) => a == b,
            (Value::Result(Err(a)), Value::Result(Err(b))) => a == b,
            (Value::List(a), Value::List(b)) => *a.borrow() == *b.borrow(),
            (Value::Map(a), Value::Map(b)) => a == b,
            (Value::Set(a), Value::Set(b)) => a == b,
            (
                Value::Struct {
                    type_name: ta,
                    fields: fa,
                },
                Value::Struct {
                    type_name: tb,
                    fields: fb,
                },
            ) => ta == tb && *fa.borrow() == *fb.borrow(),
            (
                Value::Enum {
                    type_name: ta,
                    variant: va,
                    data: da,
                },
                Value::Enum {
                    type_name: tb,
                    variant: vb,
                    data: db,
                },
            ) => ta == tb && va == vb && da == db,
            (
                Value::Typestate {
                    type_name: ta,
                    current_state: sa,
                    fields: fa,
                },
                Value::Typestate {
                    type_name: tb,
                    current_state: sb,
                    fields: fb,
                },
            ) => ta == tb && sa == sb && *fa.borrow() == *fb.borrow(),
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
            Value::Int(n) => n.hash(state),
            Value::Float(f) => f.to_bits().hash(state),
            Value::String(s) => s.hash(state),
            Value::Bool(b) => b.hash(state),
            Value::Unit => {}
            Value::Option(Some(v)) => v.hash(state),
            Value::Option(None) => {}
            Value::Result(Ok(v)) => v.hash(state),
            Value::Result(Err(e)) => e.hash(state),
            Value::List(items) => {
                for item in items.borrow().iter() {
                    item.hash(state);
                }
            }
            Value::Map(entries) => {
                for (k, v) in entries {
                    k.hash(state);
                    v.hash(state);
                }
            }
            Value::Set(items) => {
                for item in items {
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
            Value::Enum {
                type_name,
                variant,
                data,
            } => {
                type_name.hash(state);
                variant.hash(state);
                match data {
                    EnumData::Unit => 0_u8.hash(state),
                    EnumData::Tuple(items) => {
                        1_u8.hash(state);
                        for item in items {
                            item.hash(state);
                        }
                    }
                    EnumData::Struct(fields) => {
                        2_u8.hash(state);
                        let mut pairs: Vec<(&String, &Value)> = fields.iter().collect();
                        pairs.sort_by_key(|(k, _)| k.as_str());
                        for (k, v) in pairs {
                            k.hash(state);
                            v.hash(state);
                        }
                    }
                }
            }
            Value::Typestate {
                type_name,
                current_state,
                fields,
            } => {
                type_name.hash(state);
                current_state.hash(state);
                let borrow = fields.borrow();
                let mut pairs: Vec<(&String, &Value)> = borrow.iter().collect();
                pairs.sort_by_key(|(k, _)| k.as_str());
                for (k, v) in pairs {
                    k.hash(state);
                    v.hash(state);
                }
            }
            // クロージャ・ネイティブ関数はハッシュ不可 → ポインタアドレスで代替
            Value::Closure { .. } => 0_u8.hash(state),
            Value::NativeFunction(_) => 1_u8.hash(state),
        }
    }
}

impl std::fmt::Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::Int(n) => write!(f, "{}", n),
            Value::Float(n) => write!(f, "{}", n),
            Value::String(s) => write!(f, "{}", s),
            Value::Bool(b) => write!(f, "{}", b),
            Value::Unit => Ok(()), // 表示なし
            Value::Option(Some(v)) => write!(f, "some({})", v),
            Value::Option(None) => write!(f, "none"),
            Value::Result(Ok(v)) => write!(f, "ok({})", v),
            Value::Result(Err(e)) => write!(f, "err({})", e),
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
            Value::Map(entries) => {
                write!(f, "{{")?;
                for (i, (key, value)) in entries.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}: {}", key, value)?;
                }
                write!(f, "}}")
            }
            Value::Set(items) => {
                write!(f, "{{")?;
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", item)?;
                }
                write!(f, "}}")
            }
            Value::Closure { .. } => write!(f, "<closure>"),
            Value::NativeFunction(_) => write!(f, "<function>"),
            Value::Enum {
                type_name,
                variant,
                data,
            } => match data {
                EnumData::Unit => write!(f, "{}::{}", type_name, variant),
                EnumData::Tuple(items) => {
                    write!(f, "{}::{}(", type_name, variant)?;
                    for (i, item) in items.iter().enumerate() {
                        if i > 0 {
                            write!(f, ", ")?;
                        }
                        write!(f, "{}", item)?;
                    }
                    write!(f, ")")
                }
                EnumData::Struct(fields) => {
                    write!(f, "{}::{}{{", type_name, variant)?;
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
            },
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
            Value::Typestate {
                type_name,
                current_state,
                ..
            } => {
                write!(f, "{}<{}>", type_name, current_state)
            }
        }
    }
}

impl Value {
    /// 型名を文字列で返す（`type_of` 組み込み関数用）
    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Int(_) => "number",
            Value::Float(_) => "float",
            Value::String(_) => "string",
            Value::Bool(_) => "bool",
            Value::Unit => "unit",
            Value::Option(_) => "option",
            Value::Result(_) => "result",
            Value::List(_) => "list",
            Value::Map(_) => "map",
            Value::Set(_) => "set",
            Value::Closure { .. } => "closure",
            Value::NativeFunction(_) => "function",
            Value::Struct { .. } => "struct",
            Value::Enum { .. } => "enum",
            Value::Typestate { .. } => "typestate",
        }
    }

    /// struct / enum の場合、型名を動的に返す
    pub fn dynamic_type_name(&self) -> String {
        match self {
            Value::Struct { type_name, .. } => type_name.clone(),
            Value::Enum { type_name, .. } => type_name.clone(),
            Value::Typestate { type_name, .. } => type_name.clone(),
            _ => self.type_name().to_string(),
        }
    }

    /// struct フィールドを深くクローンする（@derive(Clone) 用）
    pub fn deep_clone(&self) -> Value {
        match self {
            Value::Struct { type_name, fields } => {
                let cloned: HashMap<String, Value> = fields
                    .borrow()
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
            Value::Map(entries) => Value::Map(
                entries
                    .iter()
                    .map(|(k, v)| (k.deep_clone(), v.deep_clone()))
                    .collect(),
            ),
            Value::Set(items) => Value::Set(items.iter().map(|v| v.deep_clone()).collect()),
            Value::Enum {
                type_name,
                variant,
                data,
            } => {
                let cloned_data = match data {
                    EnumData::Unit => EnumData::Unit,
                    EnumData::Tuple(items) => {
                        EnumData::Tuple(items.iter().map(|v| v.deep_clone()).collect())
                    }
                    EnumData::Struct(fields) => EnumData::Struct(
                        fields
                            .iter()
                            .map(|(k, v)| (k.clone(), v.deep_clone()))
                            .collect(),
                    ),
                };
                Value::Enum {
                    type_name: type_name.clone(),
                    variant: variant.clone(),
                    data: cloned_data,
                }
            }
            Value::Typestate {
                type_name,
                current_state,
                fields,
            } => {
                let cloned: HashMap<String, Value> = fields
                    .borrow()
                    .iter()
                    .map(|(k, v)| (k.clone(), v.deep_clone()))
                    .collect();
                Value::Typestate {
                    type_name: type_name.clone(),
                    current_state: current_state.clone(),
                    fields: Rc::new(RefCell::new(cloned)),
                }
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
    fn test_value_display_map() {
        let map = Value::Map(vec![
            (Value::String("a".to_string()), Value::Int(1)),
            (Value::String("b".to_string()), Value::Int(2)),
        ]);
        assert_eq!(map.to_string(), "{a: 1, b: 2}");
    }

    #[test]
    fn test_value_display_set() {
        let set = Value::Set(vec![
            Value::String("rust".to_string()),
            Value::String("forge".to_string()),
        ]);
        assert_eq!(set.to_string(), "{rust, forge}");
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
