pub mod bridge;
pub mod web;

pub use bridge::{
    deserialize_dom_ops, deserialize_event_buffer, serialize_dom_ops, serialize_event_buffer,
    DomOp, EncodedDomOps, EncodedEventBuffer, EventKind, EventRecord,
};
pub use web::{
    collect_bloom_files, compile_bloom_direct, compile_bloom_to_wasm,
    compile_generated_forge_to_wasm, compile_rust_source_to_wasm, extract_script_section,
    generate_counter_wasm_rust, generate_wasm_rust, generated_forge_path, inline_critical_css,
    parse_generated_forge_to_plan, plan_from_bloom_source, plan_to_generated_forge,
    preprocess_render_calls, wasm_output_path, BloomSourceFile, WasmRenderPlan,
};
