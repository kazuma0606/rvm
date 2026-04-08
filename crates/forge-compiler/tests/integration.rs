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
fn test_parse_const_fn() {
    let src = r#"
const fn clamp(value: number) -> number {
    if value < 0 { 0 } else { value }
}
"#;
    let module = parse_ok(src);
    match &module.stmts[0] {
        Stmt::Fn { name, is_const, .. } => {
            assert_eq!(name, "clamp");
            assert!(*is_const);
        }
        other => panic!("expected const fn, got {:?}", other),
    }
}

#[test]
fn test_parse_const_var_with_const_fn_call() {
    let src = r#"
const fn clamp(value: number) -> number {
    if value < 0 { 0 } else { value }
}
const MAX = clamp(42)
"#;
    let module = parse_ok(src);
    assert_eq!(module.stmts.len(), 2);
    match &module.stmts[1] {
        Stmt::Const { value, .. } => match value {
            Expr::Call { callee, args, .. } => {
                assert!(matches!(callee.as_ref(), Expr::Ident(name, _) if name == "clamp"));
                assert_eq!(args.len(), 1);
            }
            other => panic!("expected call, got {:?}", other),
        },
        other => panic!("expected Const, got {:?}", other),
    }
}

#[test]
fn test_parse_pipe_arrow_method() {
    let src = "let result = nums |> filter(x => x % 2 == 0)";
    let module = parse_ok(src);
    match &module.stmts[0] {
        Stmt::Let { value, .. } => match value {
            Expr::MethodCall {
                method,
                object,
                args,
                ..
            } => {
                assert_eq!(method, "filter");
                assert!(matches!(object.as_ref(), Expr::Ident(name, _) if name == "nums"));
                assert_eq!(args.len(), 1);
            }
            other => panic!("expected MethodCall, got {:?}", other),
        },
        other => panic!("expected Let, got {:?}", other),
    }
}

#[test]
fn test_parse_pipe_arrow_no_args() {
    let src = "let value = items |> reset";
    let module = parse_ok(src);
    match &module.stmts[0] {
        Stmt::Let { value, .. } => match value {
            Expr::MethodCall { method, args, .. } => {
                assert_eq!(method, "reset");
                assert!(args.is_empty());
            }
            other => panic!("expected MethodCall, got {:?}", other),
        },
        other => panic!("expected Let, got {:?}", other),
    }
}

#[test]
fn test_parse_pipe_arrow_chain() {
    let src = r#"let result = nums |> filter(x => x > 0) |> sum()"#;
    let module = parse_ok(src);
    match &module.stmts[0] {
        Stmt::Let { value, .. } => match value {
            Expr::MethodCall { method, object, .. } => {
                assert_eq!(method, "sum");
                match object.as_ref() {
                    Expr::MethodCall { method, .. } => assert_eq!(method, "filter"),
                    other => panic!("expected nested MethodCall, got {:?}", other),
                }
            }
            other => panic!("expected MethodCall, got {:?}", other),
        },
        other => panic!("expected Let, got {:?}", other),
    }
}

#[test]
fn test_parse_spawn_block() {
    let src = r#"
let handle = spawn {
    let value = 1
    value + 2
}
"#;
    let module = parse_ok(src);
    match &module.stmts[0] {
        Stmt::Let { value, .. } => match value {
            Expr::Spawn { body, .. } => match body.as_ref() {
                Expr::Block { stmts, tail, .. } => {
                    assert_eq!(stmts.len(), 1);
                    assert!(matches!(
                        tail.as_ref().unwrap().as_ref(),
                        Expr::BinOp { .. }
                    ));
                }
                other => panic!("expected Block body, got {:?}", other),
            },
            other => panic!("expected Spawn, got {:?}", other),
        },
        other => panic!("expected Let, got {:?}", other),
    }
}

#[test]
fn test_parse_optional_chain_field() {
    let src = "let city = user?.address";
    let module = parse_ok(src);
    match &module.stmts[0] {
        Stmt::Let { value, .. } => match value {
            Expr::OptionalChain { object, chain, .. } => {
                match chain {
                    ChainKind::Field(field) => assert_eq!(field, "address"),
                    _ => panic!("expected Field chain"),
                }
                match object.as_ref() {
                    Expr::Ident(name, _) => assert_eq!(name, "user"),
                    other => panic!("expected Ident, got {:?}", other),
                }
            }
            other => panic!("expected OptionalChain, got {:?}", other),
        },
        other => panic!("expected Let, got {:?}", other),
    }
}

#[test]
fn test_parse_optional_chain_method() {
    let src = "let len = name?.len()";
    let module = parse_ok(src);
    match &module.stmts[0] {
        Stmt::Let { value, .. } => match value {
            Expr::OptionalChain { chain, .. } => match chain {
                ChainKind::Method { name, args } => {
                    assert_eq!(name, "len");
                    assert!(args.is_empty());
                }
                _ => panic!("expected Method chain"),
            },
            other => panic!("expected OptionalChain, got {:?}", other),
        },
        other => panic!("expected Let, got {:?}", other),
    }
}

#[test]
fn test_parse_null_coalesce() {
    let src = r#"let city = user?.address ?? "unknown""#;
    let module = parse_ok(src);
    match &module.stmts[0] {
        Stmt::Let { value, .. } => match value {
            Expr::NullCoalesce { value, default, .. } => {
                assert!(matches!(value.as_ref(), Expr::OptionalChain { .. }));
                assert!(matches!(
                    default.as_ref(),
                    Expr::Literal(Literal::String(_), _)
                ));
            }
            other => panic!("expected NullCoalesce, got {:?}", other),
        },
        other => panic!("expected Let, got {:?}", other),
    }
}

#[test]
fn test_parse_yield() {
    let src = r#"
fn spam() {
    yield 5
}
"#;
    let module = parse_ok(src);
    match &module.stmts[0] {
        Stmt::Fn { body, .. } => match body.as_ref() {
            Expr::Block { stmts, .. } => match &stmts[0] {
                Stmt::Yield { value, .. } => {
                    assert!(matches!(value.as_ref(), Expr::Literal(Literal::Int(5), _)));
                }
                other => panic!("expected Yield, got {:?}", other),
            },
            other => panic!("expected Block, got {:?}", other),
        },
        other => panic!("expected Fn, got {:?}", other),
    }
}

#[test]
fn test_parse_generate_return_type() {
    let src = r#"
fn fibonacci() -> generate<number> {
    state count = 0
}
"#;
    let module = parse_ok(src);
    match &module.stmts[0] {
        Stmt::Fn { return_type, .. } => assert_eq!(
            return_type,
            &Some(TypeAnn::Generate(Box::new(TypeAnn::Number)))
        ),
        other => panic!("expected Fn, got {:?}", other),
    }
}

#[test]
fn test_parse_operator_definitions() {
    let src = r#"
struct Pair { left: number, right: number }
impl Pair {
    operator +(self, other: Pair) -> Pair { Pair { left: self.left + other.left, right: self.right + other.right } }
    operator ==(self, other: Pair) -> bool { self.left == other.left && self.right == other.right }
    operator [](self, index: number) -> number { if index == 0 { self.left } else { self.right } }
    operator unary-(self) -> Pair { Pair { left: -self.left, right: -self.right } }
}
"#;
    let module = parse_ok(src);
    assert_eq!(module.stmts.len(), 2);

    match &module.stmts[1] {
        Stmt::ImplBlock {
            operators, target, ..
        } => {
            assert_eq!(target, "Pair");
            let kinds: Vec<_> = operators.iter().map(|op| op.op.clone()).collect();
            assert_eq!(
                kinds,
                vec![
                    OperatorKind::Add,
                    OperatorKind::Eq,
                    OperatorKind::Index,
                    OperatorKind::Neg
                ]
            );
            let eq_def = operators
                .iter()
                .find(|op| op.op == OperatorKind::Eq)
                .expect("eq operator missing");
            assert_eq!(eq_def.return_type, Some(TypeAnn::Bool));

            let index_def = operators
                .iter()
                .find(|op| op.op == OperatorKind::Index)
                .expect("index operator missing");
            assert_eq!(index_def.return_type, Some(TypeAnn::Number));

            let neg_def = operators
                .iter()
                .find(|op| op.op == OperatorKind::Neg)
                .expect("unary neg operator missing");
            assert_eq!(
                neg_def.return_type,
                Some(TypeAnn::Named("Pair".to_string()))
            );
        }
        other => panic!("expected ImplBlock, got {:?}", other),
    }
}

#[test]
fn test_parse_optional_chain_nested() {
    let src = "let city = user?.address?.city";
    let module = parse_ok(src);
    match &module.stmts[0] {
        Stmt::Let { value, .. } => match value {
            Expr::OptionalChain { chain, object, .. } => {
                match chain {
                    ChainKind::Field(field) => assert_eq!(field, "city"),
                    _ => panic!("expected Field chain"),
                }
                match object.as_ref() {
                    Expr::OptionalChain { chain, .. } => match chain {
                        ChainKind::Field(field) => assert_eq!(field, "address"),
                        _ => panic!("expected Field chain"),
                    },
                    other => panic!("expected nested OptionalChain, got {:?}", other),
                }
            }
            other => panic!("expected OptionalChain, got {:?}", other),
        },
        other => panic!("expected Let, got {:?}", other),
    }
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
