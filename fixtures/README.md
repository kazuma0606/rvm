# Test Fixtures

This directory contains test fixture files for the RVM / ForgeScript test suite.

## Structure

- `*.fs` - ForgeScript source files for testing
- `errors/` - Files that should produce specific errors
- `golden/` - Expected output files for golden tests

## Usage

Use the `test-utils` crate to load fixtures:

```rust
use test_utils::load_fixture;

let content = load_fixture("sample.fs", "parser test").unwrap();
```
