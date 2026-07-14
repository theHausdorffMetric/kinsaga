//! File I/O for chronicles.

use crate::Chronicle;
use chrono::Utc;
use std::fs;
use std::io::Write;
use std::path::Path;
use thiserror::Error;

/// Error type for I/O operations.
#[derive(Debug, Error)]
pub enum IoError {
    #[error("failed to read file: {0}")]
    ReadError(#[from] std::io::Error),

    #[error("failed to write file: {0}")]
    WriteError(std::io::Error),

    #[error("failed to parse JSON: {0}")]
    ParseError(#[from] serde_json::Error),
}

/// Load a chronicle from a JSON file.
pub fn load<P: AsRef<Path>>(path: P) -> Result<Chronicle, IoError> {
    let content = fs::read_to_string(path)?;
    let chronicle = serde_json::from_str(&content)?;
    Ok(chronicle)
}

/// Save a chronicle to a JSON file.
/// Automatically updates the `last_updated` timestamp to the current UTC time.
///
/// The write is atomic: content goes to a temp file in the target's
/// directory, which is then renamed over the target, so a crash mid-write
/// can never leave a truncated or corrupt chronicle behind.
pub fn save<P: AsRef<Path>>(path: P, chronicle: &Chronicle) -> Result<(), IoError> {
    let path = path.as_ref();
    let mut chronicle = chronicle.clone();
    chronicle.last_updated = Some(Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string());
    let mut json = serde_json::to_string_pretty(&chronicle)?;
    json.push('\n');

    let dir = match path.parent() {
        Some(p) if !p.as_os_str().is_empty() => p,
        _ => Path::new("."),
    };
    let mut tmp = tempfile::NamedTempFile::new_in(dir).map_err(IoError::WriteError)?;
    tmp.write_all(json.as_bytes()).map_err(IoError::WriteError)?;
    tmp.as_file().sync_all().map_err(IoError::WriteError)?;
    // Temp files are created with restrictive permissions; keep the
    // target's existing ones when overwriting
    if let Ok(meta) = fs::metadata(path) {
        let _ = tmp.as_file().set_permissions(meta.permissions());
    }
    tmp.persist(path).map_err(|e| IoError::WriteError(e.error))?;
    Ok(())
}

/// Parse a chronicle from a JSON string.
pub fn from_json(json: &str) -> Result<Chronicle, IoError> {
    let chronicle = serde_json::from_str(json)?;
    Ok(chronicle)
}

/// Serialize a chronicle to a JSON string.
pub fn to_json(chronicle: &Chronicle) -> Result<String, IoError> {
    let json = serde_json::to_string_pretty(chronicle)?;
    Ok(json)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Category, Fact, Person};
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn sample_chronicle() -> Chronicle {
        let mut chronicle = Chronicle::new("1.0");
        chronicle.title = Some("Test".into());
        chronicle
            .categories
            .push(Category::new("family", "Family"));
        let mut person = Person::new("alice", "Alice Smith");
        person
            .facts
            .push(Fact::new("uuid-1", "1990", "family", "Born in Springfield"));
        chronicle.persons.push(person);
        chronicle
    }

    #[test]
    fn test_to_json_and_from_json() {
        let chronicle = sample_chronicle();
        let json = to_json(&chronicle).unwrap();

        let loaded = from_json(&json).unwrap();
        assert_eq!(loaded.version, "1.0");
        assert_eq!(loaded.persons.len(), 1);
    }

    #[test]
    fn test_save_and_load() {
        let chronicle = sample_chronicle();

        // Create a temp file
        let file = NamedTempFile::new().unwrap();
        let path = file.path().to_path_buf();

        // Save
        save(&path, &chronicle).unwrap();

        // Load
        let loaded = load(&path).unwrap();
        assert_eq!(loaded.version, "1.0");
        assert_eq!(loaded.title, Some("Test".into()));
        assert_eq!(loaded.persons[0].name, "Alice Smith");
    }

    #[test]
    fn test_save_leaves_no_temp_files_behind() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("chronicle.json");
        save(&path, &sample_chronicle()).unwrap();

        let entries: Vec<_> = std::fs::read_dir(dir.path()).unwrap().collect();
        assert_eq!(entries.len(), 1, "only the target file should exist");
    }

    #[test]
    fn test_save_appends_trailing_newline() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("chronicle.json");
        save(&path, &sample_chronicle()).unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.ends_with('\n'));
    }

    #[test]
    fn test_save_to_missing_directory_fails() {
        let result = save("/nonexistent/dir/file.json", &sample_chronicle());
        assert!(matches!(result, Err(IoError::WriteError(_))));
    }

    #[test]
    fn test_load_invalid_json() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, "not valid json").unwrap();

        let result = load(file.path());
        assert!(result.is_err());
    }

    #[test]
    fn test_load_missing_file() {
        let result = load("/nonexistent/path/file.json");
        assert!(result.is_err());
    }
}
