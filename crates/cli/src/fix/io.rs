use std::io::Write;
use std::path::Path;

use tempfile::NamedTempFile;

/// Read a source file, validate it is within the project root, and detect line endings.
///
/// Returns `None` (with a warning) if the path is outside the project root or unreadable.
pub(super) fn read_source(root: &Path, path: &Path) -> Option<(String, &'static str)> {
    if !path.starts_with(root) {
        tracing::warn!(path = %path.display(), "Skipping fix for path outside project root");
        return None;
    }
    let content = std::fs::read_to_string(path).ok()?;
    let line_ending = if content.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    Some((content, line_ending))
}

/// Join modified lines, preserve the original trailing newline, and atomically write the result.
pub(super) fn write_fixed_content(
    path: &Path,
    lines: &[String],
    line_ending: &str,
    original_content: &str,
) -> std::io::Result<()> {
    let mut result = lines.join(line_ending);
    if original_content.ends_with(line_ending) && !result.ends_with(line_ending) {
        result.push_str(line_ending);
    }
    atomic_write(path, result.as_bytes())
}

/// Atomically write content to a file via a temporary file and rename.
pub(super) fn atomic_write(path: &Path, content: &[u8]) -> std::io::Result<()> {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    let mut tmp = NamedTempFile::new_in(dir)?;
    tmp.write_all(content)?;
    tmp.as_file().sync_all()?;
    tmp.persist(path).map_err(|e| e.error)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atomic_write_creates_file_with_content() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.ts");
        atomic_write(&path, b"hello world").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "hello world");
    }

    #[test]
    fn atomic_write_overwrites_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.ts");
        std::fs::write(&path, "old content").unwrap();
        atomic_write(&path, b"new content").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "new content");
    }

    #[test]
    fn atomic_write_no_leftover_temp_on_success() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.ts");
        atomic_write(&path, b"data").unwrap();
        // Only the target file should exist — no stray temp files
        let entries: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(Result::ok)
            .collect();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].file_name(), "test.ts");
    }
}
