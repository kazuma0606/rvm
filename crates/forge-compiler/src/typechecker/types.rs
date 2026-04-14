// forge-compiler: 型チェッカー — Type enum 定義
// Phase 4-A 実装
// 仕様: forge/spec_v0.0.1.md §2

use crate::ast::TypeAnn;

/// ForgeScript の型（型推論・型検査で使う内部表現）
#[derive(Debug, Clone, PartialEq)]
pub enum Type {
    /// number (i64)
    Number,
    /// float (f64)
    Float,
    /// string
    String,
    /// bool
    Bool,
    /// unit（戻り値なし式・void 相当）
    Unit,
    /// T?  Option<T>
    Option(Box<Type>),
    /// T!  Result<T, String>
    Result(Box<Type>),
    /// list<T>
    List(Box<Type>),
    /// 型未解決（推論途中 / 注釈なし）
    Unknown,
}

impl Type {
    /// `TypeAnn`（AST の型注釈）から `Type` に変換する
    pub fn from_ann(ann: &TypeAnn) -> Self {
        match ann {
            TypeAnn::Number => Type::Number,
            TypeAnn::Float => Type::Float,
            TypeAnn::String => Type::String,
            TypeAnn::Bool => Type::Bool,
            TypeAnn::Option(inner) => Type::Option(Box::new(Type::from_ann(inner))),
            TypeAnn::Result(inner) => Type::Result(Box::new(Type::from_ann(inner))),
            TypeAnn::ResultWith(inner, _err) => Type::Result(Box::new(Type::from_ann(inner))),
            TypeAnn::List(inner) => Type::List(Box::new(Type::from_ann(inner))),
            TypeAnn::Generate(inner) => Type::List(Box::new(Type::from_ann(inner))),
            TypeAnn::Named(_) => Type::Unknown,
            // G-1-A: 新バリアント — 型チェッカーは Unknown で扱う
            TypeAnn::Generic { .. }
            | TypeAnn::Map(_, _)
            | TypeAnn::Set(_)
            | TypeAnn::OrderedMap(_, _)
            | TypeAnn::OrderedSet(_)
            | TypeAnn::Unit
            | TypeAnn::Fn { .. }
            | TypeAnn::AnonStruct(_)
            | TypeAnn::StringLiteralUnion(_) => Type::Unknown,
        }
    }

    /// 型名を ForgeScript 表記の文字列で返す
    pub fn name(&self) -> String {
        match self {
            Type::Number => "number".to_string(),
            Type::Float => "float".to_string(),
            Type::String => "string".to_string(),
            Type::Bool => "bool".to_string(),
            Type::Unit => "unit".to_string(),
            Type::Option(t) => format!("{}?", t.name()),
            Type::Result(t) => format!("{}!", t.name()),
            Type::List(t) => format!("list<{}>", t.name()),
            Type::Unknown => "unknown".to_string(),
        }
    }
}

impl std::fmt::Display for Type {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name())
    }
}

// ── テスト ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_type_enum_compiles() {
        // 全バリアントが構築できることを確認
        let _ = Type::Number;
        let _ = Type::Float;
        let _ = Type::String;
        let _ = Type::Bool;
        let _ = Type::Unit;
        let _ = Type::Option(Box::new(Type::Number));
        let _ = Type::Result(Box::new(Type::String));
        let _ = Type::List(Box::new(Type::Number));
        let _ = Type::Unknown;
    }

    #[test]
    fn test_type_from_ann() {
        assert_eq!(Type::from_ann(&TypeAnn::Number), Type::Number);
        assert_eq!(Type::from_ann(&TypeAnn::Float), Type::Float);
        assert_eq!(Type::from_ann(&TypeAnn::String), Type::String);
        assert_eq!(Type::from_ann(&TypeAnn::Bool), Type::Bool);
        assert_eq!(
            Type::from_ann(&TypeAnn::Option(Box::new(TypeAnn::Number))),
            Type::Option(Box::new(Type::Number))
        );
        assert_eq!(
            Type::from_ann(&TypeAnn::Result(Box::new(TypeAnn::String))),
            Type::Result(Box::new(Type::String))
        );
        assert_eq!(
            Type::from_ann(&TypeAnn::List(Box::new(TypeAnn::Bool))),
            Type::List(Box::new(Type::Bool))
        );
    }

    #[test]
    fn test_type_display() {
        assert_eq!(Type::Number.to_string(), "number");
        assert_eq!(Type::Float.to_string(), "float");
        assert_eq!(Type::Unit.to_string(), "unit");
        assert_eq!(Type::Option(Box::new(Type::String)).to_string(), "string?");
        assert_eq!(Type::Result(Box::new(Type::Number)).to_string(), "number!");
        assert_eq!(Type::List(Box::new(Type::Bool)).to_string(), "list<bool>");
        assert_eq!(
            Type::Option(Box::new(Type::Result(Box::new(Type::Number)))).to_string(),
            "number!?"
        );
    }
}
