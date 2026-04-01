// forge-transpiler: CodeGenerator (AST → Rust コード文字列)
// Phase B-1〜B-4

use forge_compiler::ast::{
    BinOp, Expr, InterpPart, Literal, MatchArm, Module, Param, Pattern, Stmt, TypeAnn, UnaryOp,
};

use crate::builtin::{try_builtin_call, try_constructor_call};

/// Rust コードを生成するコンテキスト情報
pub struct CodeGenerator {
    /// インデントレベル
    indent: usize,
    /// anyhow を使う必要があるか
    needs_anyhow: bool,
}

impl CodeGenerator {
    pub fn new() -> Self {
        Self {
            indent: 0,
            needs_anyhow: false,
        }
    }

    /// Module 全体を Rust ソースコードに変換する
    pub fn generate_module(&mut self, module: &Module) -> String {
        // anyhow が必要かどうか事前スキャン
        self.needs_anyhow = self.scan_needs_anyhow(module);

        let mut out = String::new();

        if self.needs_anyhow {
            out.push_str("use anyhow;\n\n");
        }

        // トップレベル文を fn 定義とそれ以外に分離する
        let mut fns: Vec<&Stmt> = Vec::new();
        let mut main_body: Vec<&Stmt> = Vec::new();

        for stmt in &module.stmts {
            match stmt {
                Stmt::Fn { .. } => fns.push(stmt),
                _ => main_body.push(stmt),
            }
        }

        // fn 定義を先に出力
        for stmt in &fns {
            out.push_str(&self.gen_stmt(stmt));
            out.push('\n');
        }

        // トップレベルのコードを main に包む
        if !main_body.is_empty() {
            out.push_str("fn main() -> Result<(), anyhow::Error> {\n");
            self.indent += 1;
            for stmt in &main_body {
                out.push_str(&self.gen_stmt(stmt));
            }
            // Ok(()) を自動追加
            out.push_str(&self.indent_str());
            out.push_str("Ok(())\n");
            self.indent -= 1;
            out.push_str("}\n");
        }

        out
    }

    /// anyhow が必要かどうかスキャン（T! 型や err() の使用を検出）
    fn scan_needs_anyhow(&self, module: &Module) -> bool {
        module.stmts.iter().any(|s| self.stmt_needs_anyhow(s))
    }

    fn stmt_needs_anyhow(&self, stmt: &Stmt) -> bool {
        match stmt {
            Stmt::Let { type_ann, value, .. } => {
                type_ann_needs_anyhow(type_ann) || self.expr_needs_anyhow(value)
            }
            Stmt::State { type_ann, value, .. } => {
                type_ann_needs_anyhow(type_ann) || self.expr_needs_anyhow(value)
            }
            Stmt::Const { type_ann, value, .. } => {
                type_ann_needs_anyhow(type_ann) || self.expr_needs_anyhow(value)
            }
            Stmt::Fn { return_type, body, .. } => {
                type_ann_needs_anyhow(return_type) || self.expr_needs_anyhow(body)
            }
            Stmt::Return(Some(expr), _) => self.expr_needs_anyhow(expr),
            Stmt::Return(None, _) => false,
            Stmt::Expr(expr) => self.expr_needs_anyhow(expr),
            // T-1/T-2: struct/impl/enum は現時点ではトランスパイル非対応
            Stmt::StructDef { .. } | Stmt::ImplBlock { .. } | Stmt::EnumDef { .. } => false,
        }
    }

    fn expr_needs_anyhow(&self, expr: &Expr) -> bool {
        match expr {
            Expr::Call { callee, args, .. } => {
                if let Expr::Ident(name, _) = callee.as_ref() {
                    if name == "err" {
                        return true;
                    }
                }
                args.iter().any(|a| self.expr_needs_anyhow(a))
            }
            Expr::BinOp { left, right, .. } => {
                self.expr_needs_anyhow(left) || self.expr_needs_anyhow(right)
            }
            Expr::If { cond, then_block, else_block, .. } => {
                self.expr_needs_anyhow(cond)
                    || self.expr_needs_anyhow(then_block)
                    || else_block.as_ref().map_or(false, |e| self.expr_needs_anyhow(e))
            }
            Expr::Block { stmts, tail, .. } => {
                stmts.iter().any(|s| self.stmt_needs_anyhow(s))
                    || tail.as_ref().map_or(false, |e| self.expr_needs_anyhow(e))
            }
            Expr::Match { scrutinee, arms, .. } => {
                self.expr_needs_anyhow(scrutinee)
                    || arms.iter().any(|a| self.expr_needs_anyhow(&a.body))
            }
            _ => false,
        }
    }

    // ── インデント ──────────────────────────────────────────────────────────

    fn indent_str(&self) -> String {
        "    ".repeat(self.indent)
    }

    // ── Stmt 変換 ───────────────────────────────────────────────────────────

    fn gen_stmt(&mut self, stmt: &Stmt) -> String {
        match stmt {
            Stmt::Let { name, type_ann, value, .. } => {
                let ty = type_ann.as_ref().map(|t| format!(": {}", type_ann_to_rust(t)));
                let val = self.gen_expr(value, false);
                format!("{}let {}{} = {};\n", self.indent_str(), name, ty.unwrap_or_default(), val)
            }
            Stmt::State { name, type_ann, value, .. } => {
                let ty = type_ann.as_ref().map(|t| format!(": {}", type_ann_to_rust(t)));
                let val = self.gen_expr(value, false);
                format!(
                    "{}let mut {}{} = {};\n",
                    self.indent_str(),
                    name,
                    ty.unwrap_or_default(),
                    val
                )
            }
            Stmt::Const { name, type_ann, value, .. } => {
                // const は型注釈が必須（なければ推論に任せる）
                let ty = type_ann.as_ref().map(|t| format!(": {}", type_ann_to_rust(t)));
                let val = self.gen_expr(value, false);
                format!(
                    "{}const {}{} = {};\n",
                    self.indent_str(),
                    name,
                    ty.unwrap_or_default(),
                    val
                )
            }
            Stmt::Fn { name, params, return_type, body, .. } => {
                self.gen_fn(name, params, return_type, body)
            }
            Stmt::Return(Some(expr), _) => {
                let val = self.gen_expr(expr, false);
                format!("{}return {};\n", self.indent_str(), val)
            }
            Stmt::Return(None, _) => {
                format!("{}return;\n", self.indent_str())
            }
            Stmt::Expr(expr) => {
                let s = self.gen_expr(expr, false);
                // ブロック式などはセミコロン不要な場合があるが、文として使う場合は付ける
                format!("{}{};\n", self.indent_str(), s)
            }
            // T-1: struct/impl は現時点ではトランスパイル非対応（スタブ）
            Stmt::StructDef { name, .. } => {
                format!("{}// struct {} (transpile pending)\n", self.indent_str(), name)
            }
            Stmt::ImplBlock { target, .. } => {
                format!("{}// impl {} (transpile pending)\n", self.indent_str(), target)
            }
            // T-2: enum は現時点ではトランスパイル非対応（スタブ）
            Stmt::EnumDef { name, .. } => {
                format!("{}// enum {} (transpile pending)\n", self.indent_str(), name)
            }
        }
    }

    // ── fn 変換 ──────────────────────────────────────────────────────────────

    fn gen_fn(
        &mut self,
        name: &str,
        params: &[Param],
        return_type: &Option<TypeAnn>,
        body: &Expr,
    ) -> String {
        let params_str = params
            .iter()
            .map(|p| {
                let ty = p
                    .type_ann
                    .as_ref()
                    .map(|t| type_ann_to_rust(t))
                    .unwrap_or_else(|| "_".to_string());
                format!("{}: {}", p.name, ty)
            })
            .collect::<Vec<_>>()
            .join(", ");

        let ret_str = if name == "main" {
            " -> Result<(), anyhow::Error>".to_string()
        } else {
            match return_type {
                None => String::new(),
                Some(t) => format!(" -> {}", type_ann_to_rust(t)),
            }
        };

        let body_str = self.gen_block_body(body, name == "main");

        format!("{}fn {}({}){}  {{\n{}{}}}\n", self.indent_str(), name, params_str, ret_str, body_str, self.indent_str())
    }

    /// ブロック式の中身を生成する（{ } は呼び出し側で出力する場合に使う）
    fn gen_block_body(&mut self, expr: &Expr, is_main: bool) -> String {
        self.indent += 1;
        let mut out = String::new();

        match expr {
            Expr::Block { stmts, tail, .. } => {
                for stmt in stmts {
                    out.push_str(&self.gen_stmt(stmt));
                }
                if is_main {
                    if let Some(t) = tail {
                        out.push_str(&self.gen_stmt(&Stmt::Expr(t.as_ref().clone())));
                    }
                    out.push_str(&self.indent_str());
                    out.push_str("Ok(())\n");
                } else if let Some(t) = tail {
                    // tail expr はセミコロンなし（戻り値）
                    let val = self.gen_expr(t, false);
                    out.push_str(&self.indent_str());
                    out.push_str(&val);
                    out.push('\n');
                }
            }
            other => {
                // 単一式
                if is_main {
                    out.push_str(&self.gen_stmt(&Stmt::Expr(other.clone())));
                    out.push_str(&self.indent_str());
                    out.push_str("Ok(())\n");
                } else {
                    let val = self.gen_expr(other, false);
                    out.push_str(&self.indent_str());
                    out.push_str(&val);
                    out.push('\n');
                }
            }
        }

        self.indent -= 1;
        out
    }

    // ── Expr 変換 ───────────────────────────────────────────────────────────

    /// `needs_parens`: 演算子の優先順位等でカッコが必要な場合 true
    fn gen_expr(&mut self, expr: &Expr, needs_parens: bool) -> String {
        let inner = self.gen_expr_inner(expr);
        if needs_parens {
            format!("({})", inner)
        } else {
            inner
        }
    }

    fn gen_expr_inner(&mut self, expr: &Expr) -> String {
        match expr {
            Expr::Literal(lit, _) => gen_literal(lit),
            Expr::Ident(name, _) => name.clone(),

            Expr::BinOp { op, left, right, .. } => {
                let lhs = self.gen_expr(left, binop_needs_parens(left, op));
                let rhs = self.gen_expr(right, binop_needs_parens(right, op));
                format!("{} {} {}", lhs, binop_to_rust(op), rhs)
            }

            Expr::UnaryOp { op, operand, .. } => {
                let inner = self.gen_expr(operand, true);
                match op {
                    UnaryOp::Neg => format!("-{}", inner),
                    UnaryOp::Not => format!("!{}", inner),
                }
            }

            Expr::Assign { name, value, .. } => {
                let val = self.gen_expr(value, false);
                format!("{} = {}", name, val)
            }

            Expr::If { cond, then_block, else_block, .. } => {
                self.gen_if(cond, then_block, else_block)
            }

            Expr::While { cond, body, .. } => {
                self.gen_while(cond, body)
            }

            Expr::For { var, iter, body, .. } => {
                self.gen_for(var, iter, body)
            }

            Expr::Match { scrutinee, arms, .. } => {
                self.gen_match(scrutinee, arms)
            }

            Expr::Block { stmts, tail, .. } => {
                self.gen_block(stmts, tail)
            }

            Expr::Call { callee, args, .. } => {
                self.gen_call(callee, args)
            }

            Expr::MethodCall { object, method, args, .. } => {
                self.gen_method_call(object, method, args)
            }

            Expr::Field { object, field, .. } => {
                let obj = self.gen_expr(object, false);
                format!("{}.{}", obj, field)
            }

            Expr::Index { object, index, .. } => {
                let obj = self.gen_expr(object, false);
                let idx = self.gen_expr(index, false);
                format!("{}[{}]", obj, idx)
            }

            Expr::Closure { params, body, .. } => {
                self.gen_closure(params, body)
            }

            Expr::Interpolation { parts, .. } => {
                self.gen_interpolation(parts)
            }

            Expr::Range { start, end, inclusive, .. } => {
                let s = self.gen_expr(start, false);
                let e = self.gen_expr(end, false);
                if *inclusive {
                    format!("{}..={}", s, e)
                } else {
                    format!("{}..{}", s, e)
                }
            }

            Expr::List(items, _) => {
                self.gen_list(items)
            }

            Expr::Question(inner, _) => {
                let val = self.gen_expr(inner, false);
                format!("{}?", val)
            }

            // T-1: struct 関連は現時点ではトランスパイル非対応（スタブ）
            Expr::StructInit { name, .. } => {
                format!("/* StructInit({}) */todo!()", name)
            }
            Expr::FieldAssign { object, field, value, .. } => {
                let obj = self.gen_expr(object, false);
                let val = self.gen_expr(value, false);
                format!("{}.{} = {}", obj, field, val)
            }
            // T-2: enum インスタンス化は現時点ではトランスパイル非対応（スタブ）
            Expr::EnumInit { enum_name, variant, .. } => {
                format!("/* EnumInit({}::{}) */todo!()", enum_name, variant)
            }
        }
    }

    // ── if 変換 ──────────────────────────────────────────────────────────────

    fn gen_if(
        &mut self,
        cond: &Expr,
        then_block: &Expr,
        else_block: &Option<Box<Expr>>,
    ) -> String {
        let cond_str = self.gen_expr(cond, false);
        let then_str = self.gen_inline_block(then_block);
        match else_block {
            None => format!("if {} {}", cond_str, then_str),
            Some(else_expr) => {
                // else if チェーン対応
                match else_expr.as_ref() {
                    Expr::If { cond: ec, then_block: et, else_block: ee, .. } => {
                        let else_if_str = self.gen_if(ec, et, ee);
                        format!("if {} {} else {}", cond_str, then_str, else_if_str)
                    }
                    other => {
                        let else_str = self.gen_inline_block(other);
                        format!("if {} {} else {}", cond_str, then_str, else_str)
                    }
                }
            }
        }
    }

    /// ブロック式をインライン形式 `{ ... }` で生成する
    fn gen_inline_block(&mut self, expr: &Expr) -> String {
        self.indent += 1;
        let mut inner = String::new();

        match expr {
            Expr::Block { stmts, tail, .. } => {
                inner.push_str("{\n");
                for stmt in stmts {
                    inner.push_str(&self.gen_stmt(stmt));
                }
                if let Some(t) = tail {
                    let val = self.gen_expr(t, false);
                    inner.push_str(&self.indent_str());
                    inner.push_str(&val);
                    inner.push('\n');
                }
            }
            other => {
                inner.push_str("{\n");
                let val = self.gen_expr(other, false);
                inner.push_str(&self.indent_str());
                inner.push_str(&val);
                inner.push('\n');
            }
        }

        self.indent -= 1;
        inner.push_str(&self.indent_str());
        inner.push('}');
        inner
    }

    // ── while 変換 ─────────────────────────────────────────────────────────

    fn gen_while(&mut self, cond: &Expr, body: &Expr) -> String {
        let cond_str = self.gen_expr(cond, false);
        let body_str = self.gen_inline_block(body);
        format!("while {} {}", cond_str, body_str)
    }

    // ── for 変換 ───────────────────────────────────────────────────────────

    fn gen_for(&mut self, var: &str, iter: &Expr, body: &Expr) -> String {
        let iter_str = self.gen_expr(iter, false);
        let body_str = self.gen_inline_block(body);
        // for x in &items
        format!("for {} in &{} {}", var, iter_str, body_str)
    }

    // ── match 変換 ─────────────────────────────────────────────────────────

    fn gen_match(&mut self, scrutinee: &Expr, arms: &[MatchArm]) -> String {
        let val = self.gen_expr(scrutinee, false);
        let mut out = format!("match {} {{\n", val);
        self.indent += 1;
        for arm in arms {
            let pat = gen_pattern(&arm.pattern);
            let body = self.gen_expr(&arm.body, false);
            out.push_str(&self.indent_str());
            out.push_str(&format!("{} => {},\n", pat, body));
        }
        self.indent -= 1;
        out.push_str(&self.indent_str());
        out.push('}');
        out
    }

    // ── Block 変換 ─────────────────────────────────────────────────────────

    fn gen_block(&mut self, stmts: &[Stmt], tail: &Option<Box<Expr>>) -> String {
        self.indent += 1;
        let mut inner = String::new();

        for stmt in stmts {
            inner.push_str(&self.gen_stmt(stmt));
        }
        if let Some(t) = tail {
            let val = self.gen_expr(t, false);
            inner.push_str(&self.indent_str());
            inner.push_str(&val);
            inner.push('\n');
        }

        self.indent -= 1;
        format!("{{\n{}{}}}", inner, self.indent_str())
    }

    // ── 関数呼び出し変換 ────────────────────────────────────────────────────

    fn gen_call(&mut self, callee: &Expr, args: &[Expr]) -> String {
        let args_strs: Vec<String> = args.iter().map(|a| self.gen_expr(a, false)).collect();

        // 識別子呼び出しの場合、組み込み関数・コンストラクタを変換
        if let Expr::Ident(name, _) = callee {
            // 組み込み関数チェック
            if let Some(result) = try_builtin_call(name, &args_strs) {
                return result;
            }
            // コンストラクタチェック (some/none/ok/err)
            if let Some(result) = try_constructor_call(name, &args_strs) {
                return result;
            }
        }

        let callee_str = self.gen_expr(callee, false);
        format!("{}({})", callee_str, args_strs.join(", "))
    }

    // ── メソッド呼び出し変換 ────────────────────────────────────────────────

    fn gen_method_call(&mut self, object: &Expr, method: &str, args: &[Expr]) -> String {
        let obj = self.gen_expr(object, false);
        let args_strs: Vec<String> = args.iter().map(|a| self.gen_expr(a, false)).collect();

        match method {
            // Option/Result メソッド（パススルー）
            "is_some" | "is_none" | "is_ok" | "is_err" | "unwrap_or" => {
                format!("{}.{}({})", obj, method, args_strs.join(", "))
            }

            // コレクションメソッド変換
            "map" => {
                let f = args_strs.first().cloned().unwrap_or_default();
                format!("{}.iter().map({}).collect::<Vec<_>>()", obj, f)
            }
            "filter" => {
                let f = args_strs.first().cloned().unwrap_or_default();
                // filter のクロージャは &&T を受け取るので **x でデリファレンスが必要
                // ただし closure AST から既に |x| 形式になっているので、filter のコンテキストで変換
                format!("{}.iter().filter(|x| ({})(x)).collect::<Vec<_>>()", obj, f)
            }
            "flat_map" => {
                let f = args_strs.first().cloned().unwrap_or_default();
                format!("{}.iter().flat_map({}).collect::<Vec<_>>()", obj, f)
            }
            "fold" => {
                let init = args_strs.first().cloned().unwrap_or_else(|| "0".to_string());
                let f = args_strs.get(1).cloned().unwrap_or_default();
                format!("{}.iter().fold({}, {})", obj, init, f)
            }
            "sum" => {
                format!("{}.iter().sum::<i64>()", obj)
            }
            "count" => {
                format!("{}.len()", obj)
            }
            "any" => {
                let f = args_strs.first().cloned().unwrap_or_default();
                format!("{}.iter().any(|x| ({})(x))", obj, f)
            }
            "all" => {
                let f = args_strs.first().cloned().unwrap_or_default();
                format!("{}.iter().all(|x| ({})(x))", obj, f)
            }
            "first" => {
                format!("{}.first().copied()", obj)
            }
            "last" => {
                format!("{}.last().copied()", obj)
            }
            "take" => {
                let n = args_strs.first().cloned().unwrap_or_else(|| "0".to_string());
                format!("{}.iter().take({}).copied().collect::<Vec<_>>()", obj, n)
            }
            "skip" => {
                let n = args_strs.first().cloned().unwrap_or_else(|| "0".to_string());
                format!("{}.iter().skip({}).copied().collect::<Vec<_>>()", obj, n)
            }
            "reverse" => {
                format!("{{ let mut v = {}.clone(); v.reverse(); v }}", obj)
            }
            "distinct" => {
                format!("{{ let mut v = {}.clone(); v.dedup(); v }}", obj)
            }
            "enumerate" => {
                format!("{}.iter().enumerate()", obj)
            }
            "zip" => {
                let other = args_strs.first().cloned().unwrap_or_default();
                format!("{}.iter().zip({}.iter())", obj, other)
            }
            "len" => {
                format!("{}.len()", obj)
            }
            "to_string" => {
                format!("{}.to_string()", obj)
            }
            // その他のメソッドはそのまま渡す
            other => {
                format!("{}.{}({})", obj, other, args_strs.join(", "))
            }
        }
    }

    // ── クロージャ変換 ──────────────────────────────────────────────────────

    fn gen_closure(&mut self, params: &[String], body: &Expr) -> String {
        let params_str = params.join(", ");
        let body_str = self.gen_expr(body, false);

        // キャプチャが必要か判定（簡易: 外部変数を使う場合は move をつける）
        // Phase B-3 の本格実装前の暫定: 単純なクロージャとして変換
        format!("|{}| {}", params_str, body_str)
    }

    // ── 文字列補間変換 ──────────────────────────────────────────────────────

    fn gen_interpolation(&mut self, parts: &[InterpPart]) -> String {
        let mut fmt_str = String::new();
        let mut args: Vec<String> = Vec::new();

        for part in parts {
            match part {
                InterpPart::Literal(s) => {
                    // { と } を {{ と }} にエスケープ
                    fmt_str.push_str(&s.replace('{', "{{").replace('}', "}}"));
                }
                InterpPart::Expr(expr) => {
                    fmt_str.push_str("{}");
                    let val = self.gen_expr(expr, false);
                    args.push(val);
                }
            }
        }

        if args.is_empty() {
            format!("\"{}\"", fmt_str)
        } else {
            format!("format!(\"{}\", {})", fmt_str, args.join(", "))
        }
    }

    // ── リストリテラル変換 ──────────────────────────────────────────────────

    fn gen_list(&mut self, items: &[Expr]) -> String {
        // 単一要素が範囲の場合は collect に変換
        if items.len() == 1 {
            if let Expr::Range { start, end, inclusive, .. } = &items[0] {
                let s = self.gen_expr(start, false);
                let e = self.gen_expr(end, false);
                let suffix = if s.ends_with("_i64") { "" } else { "_i64" };
                let start_str = if s.parse::<i64>().is_ok() {
                    format!("{}{}", s, suffix)
                } else {
                    s.clone()
                };
                if *inclusive {
                    return format!("({}..={}).collect::<Vec<_>>()", start_str, e);
                } else {
                    return format!("({}..{}).collect::<Vec<_>>()", start_str, e);
                }
            }
        }

        if items.is_empty() {
            return "vec![]".to_string();
        }

        let items_strs: Vec<String> = items
            .iter()
            .enumerate()
            .map(|(i, item)| {
                let s = self.gen_expr(item, false);
                // 最初の整数リテラルに _i64 サフィックスをつけて型を明示
                if i == 0 {
                    if let Expr::Literal(Literal::Int(_), _) = item {
                        return format!("{}_i64", s);
                    }
                }
                s
            })
            .collect();

        format!("vec![{}]", items_strs.join(", "))
    }
}

// ── ヘルパー関数 ──────────────────────────────────────────────────────────────

/// リテラルを Rust コードに変換する
fn gen_literal(lit: &Literal) -> String {
    match lit {
        Literal::Int(n) => n.to_string(),
        Literal::Float(f) => {
            // 整数値の float は .0 を付ける
            if f.fract() == 0.0 {
                format!("{:.1}", f)
            } else {
                f.to_string()
            }
        }
        Literal::String(s) => format!("\"{}\".to_string()", s.replace('\\', "\\\\").replace('"', "\\\"")),
        Literal::Bool(b) => b.to_string(),
    }
}

/// パターンを Rust コードに変換する
fn gen_pattern(pat: &Pattern) -> String {
    match pat {
        Pattern::Wildcard => "_".to_string(),
        Pattern::Ident(name) => name.clone(),
        Pattern::Literal(lit) => gen_literal(lit),
        Pattern::Some(inner) => format!("Some({})", gen_pattern(inner)),
        Pattern::None => "None".to_string(),
        Pattern::Ok(inner) => format!("Ok({})", gen_pattern(inner)),
        Pattern::Err(inner) => format!("Err({})", gen_pattern(inner)),
        Pattern::Range { start, end, inclusive } => {
            let s = gen_literal(start);
            let e = gen_literal(end);
            if *inclusive {
                format!("{}..={}", s, e)
            } else {
                format!("{}..{}", s, e)
            }
        }
    }
}

/// TypeAnn を Rust 型文字列に変換する
pub fn type_ann_to_rust(ann: &TypeAnn) -> String {
    match ann {
        TypeAnn::Number => "i64".to_string(),
        TypeAnn::Float => "f64".to_string(),
        TypeAnn::String => "String".to_string(),
        TypeAnn::Bool => "bool".to_string(),
        TypeAnn::Option(inner) => format!("Option<{}>", type_ann_to_rust(inner)),
        TypeAnn::Result(inner) => {
            format!("Result<{}, anyhow::Error>", type_ann_to_rust(inner))
        }
        TypeAnn::ResultWith(inner, err) => {
            format!("Result<{}, {}>", type_ann_to_rust(inner), type_ann_to_rust(err))
        }
        TypeAnn::List(inner) => format!("Vec<{}>", type_ann_to_rust(inner)),
        TypeAnn::Named(name) => name.clone(),
    }
}

fn type_ann_needs_anyhow(ann: &Option<TypeAnn>) -> bool {
    match ann {
        None => false,
        Some(t) => type_ann_has_result(t),
    }
}

fn type_ann_has_result(ann: &TypeAnn) -> bool {
    match ann {
        TypeAnn::Result(_) => true,
        TypeAnn::ResultWith(_, _) => true,
        TypeAnn::Option(inner) => type_ann_has_result(inner),
        TypeAnn::List(inner) => type_ann_has_result(inner),
        _ => false,
    }
}

/// 二項演算子を Rust の演算子文字列に変換する
fn binop_to_rust(op: &BinOp) -> &'static str {
    match op {
        BinOp::Add => "+",
        BinOp::Sub => "-",
        BinOp::Mul => "*",
        BinOp::Div => "/",
        BinOp::Rem => "%",
        BinOp::Eq  => "==",
        BinOp::Ne  => "!=",
        BinOp::Lt  => "<",
        BinOp::Gt  => ">",
        BinOp::Le  => "<=",
        BinOp::Ge  => ">=",
        BinOp::And => "&&",
        BinOp::Or  => "||",
    }
}

/// カッコが必要かどうかの簡易判定
fn binop_needs_parens(expr: &Expr, _parent_op: &BinOp) -> bool {
    matches!(expr, Expr::BinOp { .. })
}

// ── テスト ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use forge_compiler::parser::parse_source;

    fn transpile(src: &str) -> String {
        let module = parse_source(src).expect("parse failed");
        let mut gen = CodeGenerator::new();
        gen.generate_module(&module)
    }

    #[test]
    fn let_binding() {
        let out = transpile("let x = 42");
        assert!(out.contains("let x = 42;"), "got: {}", out);
    }

    #[test]
    fn fn_definition() {
        let src = r#"
fn add(a: number, b: number) -> number {
    a + b
}
"#;
        let out = transpile(src);
        assert!(out.contains("fn add(a: i64, b: i64) -> i64"), "got: {}", out);
        assert!(out.contains("a + b"), "got: {}", out);
    }

    #[test]
    fn if_expression() {
        let src = r#"
let x = 10
let label = if x > 5 { "big" } else { "small" }
"#;
        let out = transpile(src);
        assert!(out.contains("if x > 5"), "got: {}", out);
        assert!(out.contains("\"big\""), "got: {}", out);
        assert!(out.contains("\"small\""), "got: {}", out);
    }

    #[test]
    fn for_loop() {
        let src = r#"
let items = [1, 2, 3]
for x in items {
    print(x)
}
"#;
        let out = transpile(src);
        assert!(out.contains("for x in &items"), "got: {}", out);
    }

    #[test]
    fn match_expression() {
        let src = r#"
let val = 42
match val {
    0 => print("zero"),
    _ => print("other"),
}
"#;
        let out = transpile(src);
        assert!(out.contains("match val"), "got: {}", out);
        assert!(out.contains("0 =>"), "got: {}", out);
        assert!(out.contains("_ =>"), "got: {}", out);
    }

    #[test]
    fn string_interpolation() {
        let src = r#"let name = "World"
let s = "Hello, {name}!"
"#;
        let out = transpile(src);
        assert!(out.contains("format!"), "got: {}", out);
        assert!(out.contains("Hello"), "got: {}", out);
    }

    #[test]
    fn builtin_functions() {
        let src = r#"print("hello")"#;
        let out = transpile(src);
        assert!(out.contains("println!") || out.contains("print!"), "got: {}", out);
    }

    #[test]
    fn option_result() {
        let src = r#"
fn safe_div(a: number, b: number) -> number! {
    if b == 0 { return err("division by zero") }
    ok(a / b)
}
"#;
        let out = transpile(src);
        assert!(out.contains("Result<i64, anyhow::Error>"), "got: {}", out);
        assert!(out.contains("Ok("), "got: {}", out);
        assert!(out.contains("Err("), "got: {}", out);
    }

    #[test]
    fn closure_fn() {
        let src = r#"let double = x => x * 2"#;
        let out = transpile(src);
        assert!(out.contains("|x|"), "got: {}", out);
        assert!(out.contains("x * 2"), "got: {}", out);
    }

    #[test]
    fn closure_fnmut() {
        let src = r#"
let nums = [1, 2, 3]
let doubled = nums.map(x => x * 2)
"#;
        let out = transpile(src);
        assert!(out.contains(".iter().map"), "got: {}", out);
    }

    #[test]
    fn list_literal() {
        let src = r#"let nums = [1, 2, 3]"#;
        let out = transpile(src);
        assert!(out.contains("vec!["), "got: {}", out);
        assert!(out.contains("1_i64"), "got: {}", out);
    }

    #[test]
    fn collection_methods() {
        let src = r#"
let nums = [1, 2, 3, 4, 5]
let total = nums.sum()
"#;
        let out = transpile(src);
        assert!(out.contains(".iter().sum::<i64>()"), "got: {}", out);
    }
}
