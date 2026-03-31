// forge-transpiler: 組み込み関数変換テーブル
// print(x) -> println!("{}", x) など

/// 組み込み関数を Rust の呼び出しに変換するかどうか判定する
/// 組み込みなら Some(変換された Rust コード文字列) を返す
pub fn try_builtin_call(name: &str, args: &[String]) -> Option<String> {
    match name {
        "print" => {
            let arg = args.first().map(|s| s.as_str()).unwrap_or("\"\"");
            Some(format!("print!(\"{{}}\\n\", {})", arg))
        }
        "println" => {
            let arg = args.first().map(|s| s.as_str()).unwrap_or("\"\"");
            Some(format!("println!(\"{{}}\", {})", arg))
        }
        "string" => {
            let arg = args.first().map(|s| s.as_str()).unwrap_or("\"\"");
            Some(format!("{}.to_string()", arg))
        }
        "number" => {
            let arg = args.first().map(|s| s.as_str()).unwrap_or("\"\"");
            Some(format!("{}.to_string().parse::<i64>()?", arg))
        }
        "float" => {
            let arg = args.first().map(|s| s.as_str()).unwrap_or("\"\"");
            Some(format!("{}.to_string().parse::<f64>()?", arg))
        }
        "len" => {
            let arg = args.first().map(|s| s.as_str()).unwrap_or("\"\"");
            Some(format!("{}.len()", arg))
        }
        "type_of" => {
            let arg = args.first().map(|s| s.as_str()).unwrap_or("\"\"");
            Some(format!("std::any::type_name_of_val(&{})", arg))
        }
        _ => None,
    }
}

/// Option/Result コンストラクタかどうか判定する
pub fn try_constructor_call(name: &str, args: &[String]) -> Option<String> {
    match name {
        "some" => {
            let arg = args.first().map(|s| s.as_str()).unwrap_or("()");
            Some(format!("Some({})", arg))
        }
        "none" => Some("None".to_string()),
        "ok" => {
            let arg = args.first().map(|s| s.as_str()).unwrap_or("()");
            Some(format!("Ok({})", arg))
        }
        "err" => {
            let arg = args.first().map(|s| s.as_str()).unwrap_or("\"error\"");
            Some(format!("Err(anyhow::anyhow!({}))", arg))
        }
        _ => None,
    }
}
