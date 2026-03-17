//! RVM host interface abstractions

use rvm_core::VmError;
use std::io;

/// Host error type
#[derive(Debug)]
pub enum HostError {
    IoError(io::Error),
    ModuleNotFound { path: String },
    OutputError { message: String },
}

impl std::fmt::Display for HostError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HostError::IoError(e) => write!(f, "I/O error: {}", e),
            HostError::ModuleNotFound { path } => write!(f, "Module not found: {}", path),
            HostError::OutputError { message } => write!(f, "Output error: {}", message),
        }
    }
}

impl std::error::Error for HostError {}

impl From<io::Error> for HostError {
    fn from(e: io::Error) -> Self {
        HostError::IoError(e)
    }
}

impl From<HostError> for VmError {
    fn from(e: HostError) -> Self {
        VmError::UndefinedGlobal {
            name: format!("Host error: {}", e),
        }
    }
}

/// Output abstraction for VM
pub trait Output {
    fn write(&mut self, text: &str) -> Result<(), HostError>;
    fn write_err(&mut self, text: &str) -> Result<(), HostError>;
}

/// Standard output implementation
pub struct StdOutput;

impl Output for StdOutput {
    fn write(&mut self, text: &str) -> Result<(), HostError> {
        print!("{}", text);
        Ok(())
    }

    fn write_err(&mut self, text: &str) -> Result<(), HostError> {
        eprint!("{}", text);
        Ok(())
    }
}

/// Captured output for testing
pub struct CapturedOutput {
    pub stdout: String,
    pub stderr: String,
}

impl CapturedOutput {
    pub fn new() -> Self {
        Self {
            stdout: String::new(),
            stderr: String::new(),
        }
    }
}

impl Default for CapturedOutput {
    fn default() -> Self {
        Self::new()
    }
}

impl Output for CapturedOutput {
    fn write(&mut self, text: &str) -> Result<(), HostError> {
        self.stdout.push_str(text);
        Ok(())
    }

    fn write_err(&mut self, text: &str) -> Result<(), HostError> {
        self.stderr.push_str(text);
        Ok(())
    }
}

/// Module loader abstraction
pub trait ModuleLoader {
    fn load(&self, path: &str) -> Result<String, HostError>;
}

/// File system based module loader
pub struct FsModuleLoader {
    base_path: std::path::PathBuf,
}

impl FsModuleLoader {
    pub fn new(base_path: std::path::PathBuf) -> Self {
        Self { base_path }
    }
}

impl ModuleLoader for FsModuleLoader {
    fn load(&self, path: &str) -> Result<String, HostError> {
        let full_path = self.base_path.join(path);
        
        if !full_path.exists() {
            return Err(HostError::ModuleNotFound {
                path: path.to_string(),
            });
        }
        
        std::fs::read_to_string(&full_path).map_err(HostError::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smoke_test() {
        assert!(true);
    }

    #[test]
    fn test_captured_output() {
        let mut output = CapturedOutput::new();
        output.write("Hello").unwrap();
        output.write(", ").unwrap();
        output.write("World").unwrap();

        assert_eq!(output.stdout, "Hello, World");
    }

    #[test]
    fn test_captured_output_stderr() {
        let mut output = CapturedOutput::new();
        output.write("stdout").unwrap();
        output.write_err("stderr").unwrap();

        assert_eq!(output.stdout, "stdout");
        assert_eq!(output.stderr, "stderr");
    }

    #[test]
    fn test_module_loader_not_found() {
        let loader = FsModuleLoader::new(std::path::PathBuf::from("/nonexistent"));
        let result = loader.load("test.fs");

        assert!(result.is_err());
        match result {
            Err(HostError::ModuleNotFound { path }) => {
                assert_eq!(path, "test.fs");
            }
            _ => panic!("Expected ModuleNotFound error"),
        }
    }

    #[test]
    fn test_host_error_display() {
        let err = HostError::ModuleNotFound {
            path: "test.fs".to_string(),
        };
        let msg = format!("{}", err);
        assert!(msg.contains("test.fs"));
    }

    #[test]
    fn test_output_error() {
        let err = HostError::OutputError {
            message: "write failed".to_string(),
        };
        let msg = format!("{}", err);
        assert!(msg.contains("write failed"));
    }
}
