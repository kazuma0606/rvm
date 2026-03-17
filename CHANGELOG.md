# Changelog

All notable changes to this project will be documented in this file.

## [0.1.0] - 2026-03-17

### Added - MVP Release

#### Language Features
- Variable declarations with `let` keyword
- Integer and string literals
- Binary operations: `+`, `-`, `*`, `/` with correct precedence
- String concatenation with `+`
- Variable references
- Expression statements

#### Compiler & Runtime
- Complete lexer with token generation and span tracking
- Recursive descent parser with error recovery
- AST to bytecode compiler
- Stack-based bytecode interpreter
- 10 bytecode instructions implemented
- Native function registration system
- Comprehensive error handling with position information

#### CLI
- `forge run <file>` command to execute ForgeScript files
- Proper exit codes for success/failure
- Error diagnostics to stderr

#### Infrastructure
- Cargo workspace with 11 crates
- Test utilities for fixtures and golden tests
- 113 tests (unit, integration, E2E)
- CI-equivalent gate checks script
- Disassembler for debugging bytecode

#### Documentation
- MVP specification (`MVP_SPEC.md`)
- Completion report (`MVP_COMPLETION.md`)
- Development tasks tracking (`dev/tasks.md`)
- Architecture planning (`dev/plan.md`)

### Testing
- 85 unit tests
- 21 integration tests
- 7 end-to-end tests
- All tests passing

### Known Limitations
- No function definitions
- No control flow (if, while, for)
- No arrays or objects
- No module system (planned for Phase 7)
- No optimization passes
- Limited standard library

## [Unreleased]

### Planned for Future Releases
- Phase 7: Module system and standard library
- Phase 8: HIR and semantic analysis
- Phase 9: Enhanced memory management (GC)
- Phase 10: JIT compilation with Cranelift
- Phase 10: AOT compilation
- Phase 10: WebAssembly target
