// forge-stdlib: 組み込み関数スタブ
// Phase 2-C で実装する

use forge_vm::value::Value;

/// 組み込み関数の型
pub type NativeFn = fn(Vec<Value>) -> Result<Value, String>;

/// 組み込み関数テーブル（Phase 2-C で実装）
pub fn builtin_functions() -> Vec<(&'static str, NativeFn)> {
    vec![
        // Phase 2-C で追加:
        // ("print", native_print),
        // ("println", native_println),
        // ("len", native_len),
        // ("range", native_range),
        // ("to_string", native_to_string),
        // ("parse_int", native_parse_int),
        // ("parse_float", native_parse_float),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builtins_stub_compiles() {
        let fns = builtin_functions();
        // Phase 2-C まではスタブ（空リスト）
        assert_eq!(fns.len(), 0);
    }
}
