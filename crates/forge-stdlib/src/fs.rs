use std::fs;
use std::path::Path;

pub fn read_file(path: impl AsRef<str>) -> Result<String, String> {
    let path = path.as_ref();
    fs::read_to_string(path).map_err(|err| format!("failed to read '{}': {}", path, err))
}

pub fn write_file(path: impl AsRef<str>, content: impl AsRef<str>) -> Result<(), String> {
    let path = path.as_ref();
    let content = content.as_ref();
    fs::write(path, content).map_err(|err| format!("failed to write '{}': {}", path, err))
}

pub fn file_exists(path: impl AsRef<str>) -> bool {
    Path::new(path.as_ref()).is_file()
}
