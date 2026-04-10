use std::fmt::Display;

fn as_usize(value: i64) -> Result<usize, String> {
    usize::try_from(value).map_err(|_| format!("value {} is not a valid usize", value))
}

pub fn trim(value: impl AsRef<str>) -> String {
    value.as_ref().trim().to_string()
}

pub fn trim_start(value: impl AsRef<str>) -> String {
    value.as_ref().trim_start().to_string()
}

pub fn trim_end(value: impl AsRef<str>) -> String {
    value.as_ref().trim_end().to_string()
}

pub fn split(value: impl AsRef<str>, separator: impl AsRef<str>) -> Vec<String> {
    value
        .as_ref()
        .split(separator.as_ref())
        .map(|part| part.to_string())
        .collect()
}

pub fn join(list: Vec<String>, separator: impl AsRef<str>) -> String {
    list.join(separator.as_ref())
}

pub fn starts_with(value: impl AsRef<str>, prefix: impl AsRef<str>) -> bool {
    value.as_ref().starts_with(prefix.as_ref())
}

pub fn ends_with(value: impl AsRef<str>, suffix: impl AsRef<str>) -> bool {
    value.as_ref().ends_with(suffix.as_ref())
}

pub fn contains(value: impl AsRef<str>, substring: impl AsRef<str>) -> bool {
    value.as_ref().contains(substring.as_ref())
}

pub fn index_of(value: impl AsRef<str>, substring: impl AsRef<str>) -> Option<i64> {
    value
        .as_ref()
        .find(substring.as_ref())
        .map(|pos| pos as i64)
}

pub fn replace(value: impl AsRef<str>, from: impl AsRef<str>, to: impl AsRef<str>) -> String {
    value.as_ref().replace(from.as_ref(), to.as_ref())
}

pub fn replace_first(value: impl AsRef<str>, from: impl AsRef<str>, to: impl AsRef<str>) -> String {
    value.as_ref().replacen(from.as_ref(), to.as_ref(), 1)
}

pub fn to_upper(value: impl AsRef<str>) -> String {
    value.as_ref().to_uppercase()
}

pub fn to_lower(value: impl AsRef<str>) -> String {
    value.as_ref().to_lowercase()
}

pub fn repeat(value: impl AsRef<str>, times: i64) -> Result<String, String> {
    let count = as_usize(times)?;
    Ok(value.as_ref().repeat(count))
}

pub fn pad_left(
    value: impl AsRef<str>,
    width: i64,
    pad: impl AsRef<str>,
) -> Result<String, String> {
    pad_to(value.as_ref(), width, pad.as_ref(), PadDirection::Left)
}

pub fn pad_right(
    value: impl AsRef<str>,
    width: i64,
    pad: impl AsRef<str>,
) -> Result<String, String> {
    pad_to(value.as_ref(), width, pad.as_ref(), PadDirection::Right)
}

pub fn is_empty(value: impl AsRef<str>) -> bool {
    value.as_ref().is_empty()
}

pub fn char_count(value: impl AsRef<str>) -> i64 {
    value.as_ref().chars().count() as i64
}

fn pad_to(value: &str, width: i64, pad: &str, direction: PadDirection) -> Result<String, String> {
    let width = as_usize(width)?;
    let current = value.chars().count();
    if current >= width {
        return Ok(value.to_string());
    }

    if pad.is_empty() {
        return Err("pad string cannot be empty".to_string());
    }

    let needed = width - current;
    let mut filler = String::with_capacity(needed * pad.len());
    while filler.chars().count() < needed {
        filler.push_str(pad);
    }
    filler = filler.chars().take(needed).collect::<String>();

    match direction {
        PadDirection::Left => Ok(format!("{}{}", filler, value)),
        PadDirection::Right => Ok(format!("{}{}", value, filler)),
    }
}

enum PadDirection {
    Left,
    Right,
}
