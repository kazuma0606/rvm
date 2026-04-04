// forge-compiler: Phase 1 結合テスト

use forge_compiler::ast::*;
use forge_compiler::parser::parse_source;

fn parse_ok(src: &str) -> Module {
    parse_source(src).unwrap_or_else(|e| panic!("parse failed: {}", e))
}

#[test]
fn test_full_parse_hello() {
    let src = "let msg = \"Hello!\"\nprint(msg)";
    let module = parse_ok(src);
    assert_eq!(module.stmts.len(), 2);
    assert!(matches!(&module.stmts[0], Stmt::Let { name, .. } if name == "msg"));
    assert!(matches!(&module.stmts[1], Stmt::Expr(Expr::Call { .. })));
}

#[test]
fn test_full_parse_fn_with_match() {
    let src = r#"
fn describe(n: number) -> string {
    match n {
        0 => "zero",
        _ => "other"
    }
}
"#;
    let module = parse_ok(src);
    assert_eq!(module.stmts.len(), 1);
    match &module.stmts[0] {
        Stmt::Fn { name, body, .. } => {
            assert_eq!(name, "describe");
            assert!(matches!(
                body.as_ref(),
                Expr::Block { tail: Some(t), .. } if matches!(t.as_ref(), Expr::Match { .. })
            ));
        }
        other => panic!("expected Fn, got {:?}", other),
    }
}

#[test]
fn test_full_parse_closures() {
    let src = r#"
let double = x => x * 2
let add = (a, b) => a + b
let greet = () => print("Hello!")
let items = [1, 2, 3]
let doubled = items.map(x => x * 2)
"#;
    let module = parse_ok(src);
    assert_eq!(module.stmts.len(), 5);
    assert!(matches!(
        &module.stmts[0],
        Stmt::Let {
            value: Expr::Closure { .. },
            ..
        }
    ));
    assert!(matches!(
        &module.stmts[1],
        Stmt::Let {
            value: Expr::Closure { .. },
            ..
        }
    ));
    assert!(matches!(
        &module.stmts[2],
        Stmt::Let {
            value: Expr::Closure { .. },
            ..
        }
    ));
    assert!(matches!(
        &module.stmts[4],
        Stmt::Let {
            value: Expr::MethodCall { .. },
            ..
        }
    ));
}

#[test]
fn test_full_parse_all_literals() {
    let src = r#"
let a = 42
let b = 3.14
let c = true
let d = false
let e = "hello"
"#;
    let module = parse_ok(src);
    assert_eq!(module.stmts.len(), 5);
    assert!(matches!(
        &module.stmts[0],
        Stmt::Let {
            value: Expr::Literal(Literal::Int(42), _),
            ..
        }
    ));
    assert!(matches!(
        &module.stmts[1],
        Stmt::Let {
            value: Expr::Literal(Literal::Float(_), _),
            ..
        }
    ));
    assert!(matches!(
        &module.stmts[2],
        Stmt::Let {
            value: Expr::Literal(Literal::Bool(true), _),
            ..
        }
    ));
    assert!(matches!(
        &module.stmts[3],
        Stmt::Let {
            value: Expr::Literal(Literal::Bool(false), _),
            ..
        }
    ));
    assert!(matches!(
        &module.stmts[4],
        Stmt::Let {
            value: Expr::Literal(Literal::String(_), _),
            ..
        }
    ));
}
