use regex::Regex;

fn compile(pattern: impl AsRef<str>) -> Result<Regex, String> {
    Regex::new(pattern.as_ref())
        .map_err(|err| format!("invalid regex '{}': {}", pattern.as_ref(), err))
}

pub fn regex_match(pattern: impl AsRef<str>, input: impl AsRef<str>) -> Result<bool, String> {
    let regex = compile(pattern)?;
    Ok(regex.is_match(input.as_ref()))
}

pub fn regex_find(
    pattern: impl AsRef<str>,
    input: impl AsRef<str>,
) -> Result<Option<String>, String> {
    let regex = compile(pattern)?;
    Ok(regex
        .find(input.as_ref())
        .map(|mat| mat.as_str().to_string()))
}

pub fn regex_find_all(
    pattern: impl AsRef<str>,
    input: impl AsRef<str>,
) -> Result<Vec<String>, String> {
    let regex = compile(pattern)?;
    Ok(regex
        .find_iter(input.as_ref())
        .map(|mat| mat.as_str().to_string())
        .collect())
}

pub fn regex_capture(
    pattern: impl AsRef<str>,
    input: impl AsRef<str>,
) -> Result<Option<Vec<String>>, String> {
    let regex = compile(pattern)?;
    if let Some(caps) = regex.captures(input.as_ref()) {
        let mut matches = Vec::new();
        for cap in caps.iter().flatten() {
            matches.push(cap.as_str().to_string());
        }
        Ok(Some(matches))
    } else {
        Ok(None)
    }
}

pub fn regex_replace(
    pattern: impl AsRef<str>,
    input: impl AsRef<str>,
    replacement: impl AsRef<str>,
) -> Result<String, String> {
    let regex = compile(pattern)?;
    Ok(regex
        .replace_all(input.as_ref(), replacement.as_ref())
        .into_owned())
}

pub fn regex_replace_first(
    pattern: impl AsRef<str>,
    input: impl AsRef<str>,
    replacement: impl AsRef<str>,
) -> Result<String, String> {
    let regex = compile(pattern)?;
    Ok(regex
        .replace(input.as_ref(), replacement.as_ref())
        .into_owned())
}
