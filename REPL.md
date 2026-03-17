# ForgeScript REPL

## Overview

ForgeScript includes an interactive REPL (Read-Eval-Print Loop) that allows you to experiment with ForgeScript code in real-time, similar to Python or Ruby's interactive shells.

## Starting the REPL

```bash
# Start REPL without arguments
forge

# Or explicitly
forge repl
```

## REPL Commands

- `exit` or `quit` - Exit the REPL
- `help` - Show help message
- `clear` - Clear the environment and start fresh

## Example Session

```
$ forge
ForgeScript REPL v0.1.0
Type 'exit' or 'quit' to exit, 'help' for help

>>> let x = 10
>>> let y = 20
>>> let sum = x + y
>>> print(sum)
30
>>> let message = "Hello"
>>> let greeting = message + " World"
>>> print(greeting)
Hello World
>>> let result = 2 + 3 * 4
>>> print(result)
14
>>> clear
Environment cleared
>>> let a = 5
>>> print(a)
5
>>> exit
Goodbye!
```

## Features

### Variable Persistence

Variables defined in the REPL persist across multiple inputs:

```
>>> let x = 10
>>> let y = x + 5
>>> print(y)
15
```

### Error Handling

Syntax and runtime errors are displayed without exiting the REPL:

```
>>> let x = undefined_var
Error: Undefined variable 'undefined_var' at 8:21

>>> let x =
Error: Unexpected end of input at 7:7

>>> print(x)
>>> let x = 10
>>> print(x)
10
```

### Native Functions

Built-in functions like `print` are available immediately:

```
>>> print(42)
42
>>> print("Hello")
Hello
```

### Arithmetic Operations

All basic arithmetic operations are supported:

```
>>> let a = 10
>>> let b = 20
>>> print(a + b)
30
>>> print(a * b)
200
>>> print(b / a)
2
>>> print(b - a)
10
```

### String Operations

String concatenation works as expected:

```
>>> let first = "Hello"
>>> let second = " World"
>>> let greeting = first + second
>>> print(greeting)
Hello World
```

## Implementation Details

The REPL is implemented in the `fs-repl` crate and provides:

- **State Preservation**: VM state persists across evaluations
- **Context Awareness**: Compiler recognizes previously defined variables
- **Error Recovery**: Continues running after errors
- **Interactive Feedback**: Immediate execution and output

## Testing

The REPL includes comprehensive tests:

```bash
# Run REPL unit tests
cargo test -p fs-repl --lib

# Run REPL integration tests
cargo test -p fs-repl --tests
```

Total: 12 tests covering:
- Basic operations
- Variable persistence
- String operations
- Complex expressions
- Error handling
- Native function calls

## Architecture

1. **Input Reading**: Reads line-by-line from stdin
2. **Parsing**: Converts input to AST using `fs-parser`
3. **Context Compilation**: Compiles AST to bytecode with existing global variables
4. **Execution**: Runs bytecode on the persistent VM
5. **Output**: Displays results or errors

The REPL maintains a single VM instance throughout the session, allowing variables and state to persist between evaluations.
