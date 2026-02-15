use std::fs;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};

use crate::error::AppError;

/// Validates the output file path.
///
/// Checks (in order):
/// 1. Path is not a directory
/// 2. File does not already exist
/// 3. Parent directory exists (relative paths without parent use `.`)
pub fn validate_output_path(path: &Path) -> Result<(), AppError> {
    if path.is_dir() {
        return Err(AppError::OutputPathError {
            message: format!("{} is a directory.", path.display()),
        });
    }

    if path.try_exists().unwrap_or(false) {
        return Err(AppError::OutputPathError {
            message: format!("{} already exists.", path.display()),
        });
    }

    let parent = match path.parent() {
        Some(p) if !p.as_os_str().is_empty() => p,
        _ => Path::new("."),
    };
    if !parent.is_dir() {
        return Err(AppError::OutputPathError {
            message: format!("Directory {} does not exist.", parent.display()),
        });
    }

    Ok(())
}

/// Writes content to the specified file path.
///
/// Uses `create_new(true)` to atomically create the file, preventing TOCTOU
/// race conditions where another process creates the file between validation
/// and writing.
pub fn write_output(path: &Path, content: &str) -> Result<(), AppError> {
    use std::io::Write;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| AppError::OutputPathError {
            message: format!("{}: {e}", path.display()),
        })?;
    file.write_all(content.as_bytes())
        .map_err(|e| AppError::OutputPathError {
            message: format!("{}: {e}", path.display()),
        })
}

/// Prompts the user for an output file path via stderr/stdin.
pub fn prompt_output_path() -> Result<PathBuf, AppError> {
    eprint!("Output file path: ");
    io::stderr().flush()?;

    let mut line = String::new();
    io::stdin().lock().read_line(&mut line)?;
    let trimmed = line.trim();

    if trimmed.is_empty() {
        return Err(AppError::OutputPathError {
            message: "Please enter an output file path.".to_string(),
        });
    }

    Ok(PathBuf::from(trimmed))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn validate_rejects_directory_path() {
        let dir = TempDir::new().unwrap();
        let err = validate_output_path(dir.path()).unwrap_err();
        match err {
            AppError::OutputPathError { message } => {
                assert!(message.contains("is a directory"), "got: {message}");
            }
            other => panic!("Expected OutputPathError, got: {other:?}"),
        }
    }

    #[test]
    fn validate_rejects_existing_file() {
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("existing.xml");
        fs::write(&file_path, "content").unwrap();

        let err = validate_output_path(&file_path).unwrap_err();
        match err {
            AppError::OutputPathError { message } => {
                assert!(message.contains("already exists"), "got: {message}");
            }
            other => panic!("Expected OutputPathError, got: {other:?}"),
        }
    }

    #[test]
    fn validate_rejects_nonexistent_parent() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("nonexistent").join("package.xml");

        let err = validate_output_path(&path).unwrap_err();
        match err {
            AppError::OutputPathError { message } => {
                assert!(message.contains("does not exist"), "got: {message}");
            }
            other => panic!("Expected OutputPathError, got: {other:?}"),
        }
    }

    #[test]
    fn validate_accepts_valid_path() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("package.xml");
        assert!(validate_output_path(&path).is_ok());
    }

    #[test]
    fn validate_relative_path_without_parent() {
        // "package.xml" has parent Some(""), which should resolve to "."
        // We verify the parent-resolution logic by using a tempdir as cwd context:
        // Path::new("file.xml").parent() returns Some(""), and our code treats that as "."
        let parent = Path::new("package.xml").parent().unwrap();
        assert!(
            parent.as_os_str().is_empty(),
            "parent of bare filename should be empty"
        );
        // The fallback to "." is exercised when validate_output_path is called
        // with a bare filename. Since cwd always exists in test runners, this succeeds.
        let path = Path::new("__test_sf_pkgen_nonexistent_file__.xml");
        assert!(!path.exists(), "test precondition: file must not exist");
        assert!(validate_output_path(path).is_ok());
    }

    #[test]
    fn validate_rejects_parent_that_is_a_file() {
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("afile");
        fs::write(&file_path, "content").unwrap();
        // Try to use a file as the parent directory
        let path = file_path.join("package.xml");
        let err = validate_output_path(&path).unwrap_err();
        match err {
            AppError::OutputPathError { message } => {
                assert!(message.contains("does not exist"), "got: {message}");
            }
            other => panic!("Expected OutputPathError, got: {other:?}"),
        }
    }

    #[test]
    fn write_output_creates_file() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("output.xml");
        write_output(&path, "<xml/>").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "<xml/>");
    }

    #[test]
    fn write_output_error_on_invalid_path() {
        let path = Path::new("/nonexistent_dir_sf_pkgen/output.xml");
        let err = write_output(path, "content").unwrap_err();
        match err {
            AppError::OutputPathError { message } => {
                assert!(message.contains("/nonexistent_dir_sf_pkgen/output.xml"));
            }
            other => panic!("Expected OutputPathError, got: {other:?}"),
        }
    }
}
