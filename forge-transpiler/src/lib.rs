// forge-transpiler: ForgeScript → Rust トランスパイラ
// Phase B-0〜B-4

pub mod builtin;
pub mod codegen;
pub mod error;

use forge_compiler::parser::parse_source;

pub use codegen::CodeGenerator;
pub use error::TranspileError;

/// ForgeScript ソースコードを Rust コードに変換する
pub fn transpile(source: &str) -> Result<String, TranspileError> {
    let module =
        parse_source(source).map_err(|e| TranspileError::ParseError(e.to_string()))?;

    let mut gen = CodeGenerator::new();
    Ok(gen.generate_module(&module))
}
