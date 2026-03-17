# ForgeScript MVP Specification

## Version
MVP 1.0 (Milestone 3)

## Scope

This document defines the **Minimum Viable Product (MVP)** scope for ForgeScript and RVM.

## Supported Features

### 1. Variable Declaration (`let`)

```fs
let x = 1
let name = "Alice"
```

- Variables are globally scoped
- Variables are immutable once declared
- Variables must be assigned a value at declaration

### 2. Literals

**Integer Literals:**
```fs
let num = 42
let negative = -10  // Not yet supported in MVP
```

**String Literals:**
```fs
let text = "Hello, World"
```

Supported escape sequences: None in MVP

### 3. Binary Operations

**Arithmetic Operations:**
```fs
let sum = 1 + 2           // Addition
let diff = 10 - 3         // Subtraction
let product = 4 * 5       // Multiplication
let quotient = 20 / 4     // Division
```

**Operator Precedence:**
- `*` and `/` have higher precedence than `+` and `-`
- Left-to-right associativity
- Parentheses are supported: `(1 + 2) * 3`

### 4. String Concatenation

```fs
let greeting = "Hello" + " " + "World"
```

### 5. Variable Reference

```fs
let x = 10
let y = x
let z = x + y
```

### 6. Expression Statements

```fs
1 + 2        // Expression statement
x + y        // Expression statement
```

### 7. Native Functions

**print:**
```fs
// Note: In MVP, print cannot be called yet as it requires
// native function registration which is implemented but
// not yet integrated with the compiler
```

## Not Supported in MVP

The following features are **explicitly not supported** in the MVP:

- Function definitions
- Control flow (`if`, `while`, `for`)
- Arrays/Lists
- Objects/Structs
- Import/Module system
- Comments
- Floating point numbers
- Boolean type and operations
- Comparison operators (`<`, `>`, `==`, etc.)
- Logical operators (`&&`, `||`, `!`)
- Unary operators (except implicit in parser)
- Multiple statements per line (separated by `;`)
- Return statements (outside functions)

## Error Handling

### Compile-Time Errors

- **Undefined Variable:** Using a variable before declaration
- **Syntax Error:** Invalid syntax in source code
- **Unexpected Token:** Parser encounters unexpected token

### Runtime Errors

- **Division by Zero:** Attempting to divide by zero
- **Type Error:** Invalid operation on incompatible types
- **Stack Overflow:** Stack size exceeds limit (1024 values)
- **Stack Underflow:** Pop from empty stack

## Exit Codes

- `0` - Success
- `1` - Error (parse, compile, or runtime)

## File Format

- Extension: `.fs`
- Encoding: UTF-8
- Line endings: LF or CRLF

## Example Programs

### Example 1: Basic Arithmetic
```fs
let x = 1
let y = 2
let sum = x + y
```

### Example 2: Operator Precedence
```fs
let result = 2 + 3 * 4
```
Expected: result = 14

### Example 3: String Operations
```fs
let greeting = "Hello" + " " + "World"
```

### Example 4: Complex Expression
```fs
let a = 10
let b = 20
let c = a + b * 2
```
Expected: c = 50

## Testing Requirements

### Unit Tests
- Each component must have unit tests
- Coverage for both success and error cases

### Integration Tests
- Parser + Lexer integration
- Compiler + Parser integration
- Runtime + Compiler integration

### E2E Tests
- Complete pipeline from source to execution
- Test success cases
- Test error cases with proper error messages

## Performance Requirements (MVP)

- Not a primary concern for MVP
- Should handle programs with:
  - Up to 100 statements
  - Up to 50 variables
  - Expression depth up to 20

## Known Limitations

1. No optimization - bytecode is generated naively
2. Limited error messages - basic position information only
3. No REPL - file execution only
4. No debugger
5. No standard library beyond `print`

## Future Extensions (Post-MVP)

- Functions and closures
- Control flow structures
- Type system
- Module system
- Package manager integration
- JIT compilation
- AOT compilation
- WASM target
