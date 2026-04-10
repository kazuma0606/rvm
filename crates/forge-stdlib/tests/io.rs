use std::io::Cursor;

use forge_stdlib::io::{read_line_from, read_stdin_from, write_error_to};

#[test]
fn test_read_line_returns_none_on_eof() {
    let mut cursor = Cursor::new("");
    assert_eq!(read_line_from(&mut cursor).unwrap(), None);
}

#[test]
fn test_read_line_trims_newline() {
    let mut cursor = Cursor::new("value\r\n");
    assert_eq!(
        read_line_from(&mut cursor).unwrap(),
        Some("value".to_string())
    );
}

#[test]
fn test_read_stdin_from_reads_all() {
    let mut cursor = Cursor::new("line1\nline2");
    assert_eq!(
        read_stdin_from(&mut cursor).unwrap(),
        "line1\nline2".to_string()
    );
}

#[test]
fn test_eprintln_writes_to_stderr() {
    let mut buffer = Vec::new();
    write_error_to(&mut buffer, "error", true).unwrap();
    assert_eq!(buffer, b"error\n");
}

#[test]
fn test_eprint_writes_without_newline() {
    let mut buffer = Vec::new();
    write_error_to(&mut buffer, "info", false).unwrap();
    assert_eq!(buffer, b"info");
}
