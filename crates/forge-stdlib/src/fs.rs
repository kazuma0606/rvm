use std::fs;
use std::path::{Path, PathBuf};

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

pub fn list_dir(path: impl AsRef<str>) -> Result<Vec<String>, String> {
    let path = Path::new(path.as_ref());
    let mut entries = Vec::new();
    for entry in fs::read_dir(path).map_err(|err| format!("failed to read dir: {}", err))? {
        let entry = entry.map_err(|err| format!("failed to read entry: {}", err))?;
        entries.push(entry.file_name().to_string_lossy().into_owned());
    }
    Ok(entries)
}

pub fn make_dir(path: impl AsRef<str>) -> Result<(), String> {
    fs::create_dir_all(path.as_ref()).map_err(|err| format!("failed to create dir: {}", err))
}

pub fn delete_file(path: impl AsRef<str>) -> Result<(), String> {
    fs::remove_file(path.as_ref()).map_err(|err| format!("failed to delete file: {}", err))
}

pub fn path_absolute(path: impl AsRef<str>) -> Result<String, String> {
    let path = Path::new(path.as_ref());
    fs::canonicalize(path)
        .map_err(|err| format!("failed to canonicalize '{}': {}", path.display(), err))
        .map(|p| p.to_string_lossy().into_owned())
}

pub fn path_join(base: impl AsRef<str>, parts: Vec<String>) -> String {
    let mut buf = PathBuf::from(base.as_ref());
    for part in parts {
        buf.push(part);
    }
    buf.to_string_lossy().into_owned()
}

pub fn path_exists(path: impl AsRef<str>) -> bool {
    Path::new(path.as_ref()).exists()
}

pub fn path_is_dir(path: impl AsRef<str>) -> bool {
    Path::new(path.as_ref()).is_dir()
}

pub fn path_ext(path: impl AsRef<str>) -> Option<String> {
    Path::new(path.as_ref())
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_string())
}

pub fn path_stem(path: impl AsRef<str>) -> Option<String> {
    Path::new(path.as_ref())
        .file_stem()
        .and_then(|stem| stem.to_str())
        .map(|stem| stem.to_string())
}
