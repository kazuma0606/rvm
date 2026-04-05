// forge-transpiler: builtin call translation table
// Note: ForgeScript's print(x) adds a newline (same as println), so it maps to println!("{}", x)

/// Returns Some(rendered_rust) when the call is a known builtin.
pub fn try_builtin_call(name: &str, args: &[String]) -> Option<String> {
    match name {
        "print" => {
            let arg = args.first().map(|s| s.as_str()).unwrap_or("\"\"");
            Some(format!("println!(\"{{}}\", {})", arg))
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
            Some(format!("{}.len() as i64", arg))
        }
        "type_of" => {
            let arg = args.first().map(|s| s.as_str()).unwrap_or("\"\"");
            Some(format!("std::any::type_name_of_val(&{})", arg))
        }
        "assert" => {
            let arg = args.first().map(|s| s.as_str()).unwrap_or("false");
            Some(format!("assert!({})", arg))
        }
        "assert_eq" => {
            let left = args.first().map(|s| s.as_str()).unwrap_or("()");
            let right = args.get(1).map(|s| s.as_str()).unwrap_or("()");
            Some(format!("assert_eq!({}, {})", left, right))
        }
        "assert_ne" => {
            let left = args.first().map(|s| s.as_str()).unwrap_or("()");
            let right = args.get(1).map(|s| s.as_str()).unwrap_or("()");
            Some(format!("assert_ne!({}, {})", left, right))
        }
        "assert_ok" => {
            let arg = args.first().map(|s| s.as_str()).unwrap_or("()");
            Some(format!("assert!({}.is_ok())", arg))
        }
        "assert_err" => {
            let arg = args.first().map(|s| s.as_str()).unwrap_or("()");
            Some(format!("assert!({}.is_err())", arg))
        }
        _ => None,
    }
}

/// Returns Some(rendered_rust) for Option/Result constructors.
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
