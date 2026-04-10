use std::io::{self, BufRead, Read, Write};

pub fn read_line() -> Result<Option<String>, String> {
    let stdin = io::stdin();
    let mut handle = stdin.lock();
    read_line_from(&mut handle)
}

pub fn read_stdin() -> Result<String, String> {
    let mut stdin = io::stdin();
    read_stdin_from(&mut stdin)
}

pub fn eprintln(msg: impl AsRef<str>) -> Result<(), String> {
    write_error_to(&mut io::stderr(), msg.as_ref(), true)
}

pub fn eprint(msg: impl AsRef<str>) -> Result<(), String> {
    write_error_to(&mut io::stderr(), msg.as_ref(), false)
}

#[doc(hidden)]
pub fn read_line_from<R: BufRead>(reader: &mut R) -> Result<Option<String>, String> {
    let mut buffer = String::new();
    match reader.read_line(&mut buffer) {
        Ok(0) => Ok(None),
        Ok(_) => Ok(Some(trim_newline(&buffer))),
        Err(err) => Err(format!("failed to read line: {}", err)),
    }
}

#[doc(hidden)]
pub fn read_stdin_from<R: Read>(reader: &mut R) -> Result<String, String> {
    let mut buffer = String::new();
    reader
        .read_to_string(&mut buffer)
        .map_err(|err| format!("failed to read stdin: {}", err))?;
    Ok(buffer)
}

#[doc(hidden)]
pub fn write_error_to<W: Write>(writer: &mut W, msg: &str, newline: bool) -> Result<(), String> {
    writer
        .write_all(msg.as_bytes())
        .map_err(|err| format!("failed to write to stderr: {}", err))?;

    if newline {
        writer
            .write_all(b"\n")
            .map_err(|err| format!("failed to write newline to stderr: {}", err))?;
    }

    writer
        .flush()
        .map_err(|err| format!("failed to flush stderr: {}", err))?;
    Ok(())
}

fn trim_newline(value: &str) -> String {
    value
        .trim_end_matches(|c| c == '\r' || c == '\n')
        .to_string()
}
