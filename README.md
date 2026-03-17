# RVM / ForgeScript

Rust Virtual Machine (RVM) and ForgeScript (FS) - A script language and VM that simplifies Rust development.

## Overview

ForgeScript is a script language that abstracts away Rust's ownership, lifetimes, and mutability while integrating seamlessly with Cargo's ecosystem. RVM is the virtual machine that executes ForgeScript bytecode.

## Project Structure

```
rvm/
├── crates/           # Workspace crates
│   ├── fs-ast/       # ForgeScript AST definitions
│   ├── fs-lexer/     # Lexical analyzer
│   ├── fs-parser/    # Parser
│   ├── fs-bytecode/  # Bytecode instruction set
│   ├── fs-compiler/  # AST to bytecode compiler
│   ├── rvm-core/     # Core VM types
│   ├── rvm-runtime/  # Bytecode interpreter
│   ├── rvm-host/     # Host interface abstractions
│   ├── fs-repl/      # Interactive REPL
│   └── fs-cli/       # CLI tool (forge)
├── dev/              # Development documentation
└── idea/             # Project notes

```

## Building

```bash
cargo build --workspace
```

## Testing

```bash
cargo test --workspace
```

## Quick Start

```bash
# Start interactive REPL
cargo run -p fs-cli
# Or simply: forge

# Run a ForgeScript file
cargo run -p fs-cli -- run fixtures/e2e/arithmetic.fs

# Or use the built binary
./target/release/forge run fixtures/e2e/variables.fs

# Show help
forge help
```

### REPL Mode

ForgeScript includes an interactive REPL (Read-Eval-Print Loop) for experimenting with code:

```bash
$ forge
ForgeScript REPL v0.1.0
Type 'exit' or 'quit' to exit, 'help' for help

>>> let x = 10
>>> let y = 20
>>> let sum = x + y
>>> print(sum)
30
>>> exit
Goodbye!
```

## Example Program

```fs
let x = 10
let y = 20
let sum = x + y
let result = 2 + 3 * 4
let greeting = "Hello" + " " + "World"
```

## Development Status

**✅ MVP COMPLETE** (Milestone 4 achieved)

- ✅ Phase 1: Workspace setup
- ✅ Phase 2: AST / Lexer / Parser
- ✅ Phase 3: Bytecode / Compiler
- ✅ Phase 4: RVM Core / Runtime
- ✅ Phase 5: Host / CLI
- ✅ Phase 6: MVP E2E

**Test Status:** 125/125 tests passing (12 crates)

See `dev/tasks.md` for detailed task breakdown and `MVP_COMPLETION.md` for full report.

## Documentation

Detailed manuals are available in [`usage/`](usage/README.md):

- [Quick Start](usage/quick-start.md)
- [CLI Reference](usage/cli.md)
- [REPL Guide](usage/repl.md)
- [Language Reference](usage/language-reference.md)
- [Errors & Troubleshooting](usage/errors.md)
- [Examples](usage/examples.md)

## License

MIT OR Apache-2.0
