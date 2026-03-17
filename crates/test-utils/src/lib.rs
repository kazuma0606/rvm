//! Test utilities for RVM / ForgeScript

use std::fs;
use std::path::{Path, PathBuf};

/// Error type for test utilities
#[derive(Debug)]
pub enum TestError {
    FixtureNotFound { path: PathBuf, context: String },
    IoError { source: std::io::Error, path: PathBuf },
    GoldenMismatch { expected: String, actual: String },
}

impl std::fmt::Display for TestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TestError::FixtureNotFound { path, context } => {
                write!(
                    f,
                    "Fixture not found: {} (context: {})",
                    path.display(),
                    context
                )
            }
            TestError::IoError { source, path } => {
                write!(f, "IO error at {}: {}", path.display(), source)
            }
            TestError::GoldenMismatch { expected, actual } => {
                write!(f, "Golden test mismatch:\nExpected:\n{}\nActual:\n{}", expected, actual)
            }
        }
    }
}

impl std::error::Error for TestError {}

/// Load a fixture file from the fixtures directory
///
/// # Arguments
/// * `fixture_name` - Name of the fixture file (relative to fixtures directory)
/// * `context` - Context information for error reporting
///
/// # Returns
/// The contents of the fixture file or an error with diagnostic information
pub fn load_fixture(fixture_name: &str, context: &str) -> Result<String, TestError> {
    let fixtures_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("fixtures");
    
    let fixture_path = fixtures_dir.join(fixture_name);
    
    if !fixture_path.exists() {
        return Err(TestError::FixtureNotFound {
            path: fixture_path,
            context: context.to_string(),
        });
    }
    
    fs::read_to_string(&fixture_path).map_err(|source| TestError::IoError {
        source,
        path: fixture_path,
    })
}

/// Compare actual output with golden file
///
/// If the golden file doesn't exist, this function will create it with the actual content.
/// If it exists, it will compare and return an error if there's a mismatch.
///
/// # Arguments
/// * `golden_name` - Name of the golden file (relative to golden directory)
/// * `actual` - The actual output to compare
///
/// # Returns
/// Ok if content matches, or error with difference information
pub fn assert_golden(golden_name: &str, actual: &str) -> Result<(), TestError> {
    let golden_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("golden");
    
    fs::create_dir_all(&golden_dir).ok();
    
    let golden_path = golden_dir.join(golden_name);
    
    if !golden_path.exists() {
        fs::write(&golden_path, actual).map_err(|source| TestError::IoError {
            source,
            path: golden_path.clone(),
        })?;
        eprintln!("Created new golden file: {}", golden_path.display());
        return Ok(());
    }
    
    let expected = fs::read_to_string(&golden_path).map_err(|source| TestError::IoError {
        source,
        path: golden_path,
    })?;
    
    if expected != actual {
        return Err(TestError::GoldenMismatch {
            expected,
            actual: actual.to_string(),
        });
    }
    
    Ok(())
}

/// Compare two error messages, ignoring minor formatting differences
pub fn assert_error_eq(expected: &str, actual: &str) -> bool {
    let normalize = |s: &str| -> String {
        s.lines()
            .map(|line| line.trim())
            .filter(|line| !line.is_empty())
            .collect::<Vec<_>>()
            .join("\n")
    };
    
    normalize(expected) == normalize(actual)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fixture_not_found() {
        let result = load_fixture("nonexistent.txt", "test context");
        assert!(result.is_err());
        
        if let Err(TestError::FixtureNotFound { path, context }) = result {
            assert!(path.to_string_lossy().contains("nonexistent.txt"));
            assert_eq!(context, "test context");
        } else {
            panic!("Expected FixtureNotFound error");
        }
    }

    #[test]
    fn test_error_eq_ignores_whitespace() {
        let expected = "Error: something wrong\n  at line 10\n";
        let actual = "  Error: something wrong  \nat line 10";
        assert!(assert_error_eq(expected, actual));
    }

    #[test]
    fn test_error_eq_detects_difference() {
        let expected = "Error: something wrong";
        let actual = "Error: something different";
        assert!(!assert_error_eq(expected, actual));
    }

    #[test]
    fn test_golden_mismatch_detection() {
        use std::fs;
        use std::io::Write;
        
        let test_dir = std::env::temp_dir().join("rvm-test-golden");
        fs::create_dir_all(&test_dir).unwrap();
        
        let golden_file = test_dir.join("test_golden.txt");
        let mut file = fs::File::create(&golden_file).unwrap();
        file.write_all(b"expected content").unwrap();
        drop(file);
        
        let actual = "different content";
        
        let golden_dir_backup = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("golden");
        
        fs::create_dir_all(&golden_dir_backup).ok();
        let test_golden = golden_dir_backup.join("test_mismatch.txt");
        fs::write(&test_golden, "expected content").ok();
        
        let result = assert_golden("test_mismatch.txt", actual);
        assert!(result.is_err());
        
        if let Err(TestError::GoldenMismatch { expected, actual: act }) = result {
            assert_eq!(expected, "expected content");
            assert_eq!(act, "different content");
        } else {
            panic!("Expected GoldenMismatch error");
        }
        
        fs::remove_file(test_golden).ok();
    }
}
