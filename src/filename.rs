use std::path::{Component, Path};

use anyhow::Result;

/// Validates a device-supplied filename before joining it to a host path.
///
/// This function enforces the invariant that device filenames cannot escape
/// the intended target directory. It rejects:
///
/// - Empty strings
/// - Strings containing NUL bytes or control characters (0x00-0x1F)
/// - Path traversal sequences (`..`)
/// - Absolute paths (root or prefix components)
/// - Current directory references (`.`)
/// - Embedded path separators
///
/// Returns the original filename if it passes all checks, otherwise returns
/// an error describing why the filename was rejected.
pub fn validate_device_filename(filename: &str) -> Result<&str> {
    // Reject empty filenames
    if filename.is_empty() {
        anyhow::bail!("filename cannot be empty");
    }

    // Reject control characters and NUL bytes (0x00-0x1F)
    // These are problematic across all platforms
    for ch in filename.chars() {
        if ch.is_control() {
            anyhow::bail!("filename contains invalid control character");
        }
    }

    // Also reject backslash for cross-platform safety (it's a path separator on Windows)
    if filename.contains('\\') {
        anyhow::bail!("filename cannot contain backslashes");
    }

    // Parse the path and verify it has exactly one Normal component
    let path = Path::new(filename);
    let mut components = path.components();

    // First, check that there's no root/prefix/parent before the first normal component
    match components.next() {
        Some(Component::RootDir | Component::Prefix(_) | Component::CurDir) => {
            anyhow::bail!("filename cannot be an absolute path or current directory reference");
        }
        Some(Component::ParentDir) => {
            anyhow::bail!("filename cannot contain parent directory references");
        }
        Some(Component::Normal(_)) => {
            // Good: starts with a normal component, now verify there's exactly one
        }
        None => {
            // Empty path after parsing
            anyhow::bail!("filename is invalid");
        }
    }

    // Verify no additional components (no embedded separators, no trailing ..)
    if components.next().is_some() {
        anyhow::bail!("filename cannot contain path separators");
    }

    Ok(filename)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_valid_filenames() {
        // Simple valid filenames
        assert!(validate_device_filename("normal.txt").is_ok());
        assert!(validate_device_filename("file-with-dashes.jpg").is_ok());
        assert!(validate_device_filename("file_with_underscores.png").is_ok());
        assert!(validate_device_filename("file with spaces.doc").is_ok());

        // Unicode filenames
        assert!(validate_device_filename("unicode-文件.png").is_ok());
        assert!(validate_device_filename("emoji-🎉.txt").is_ok());
        assert!(validate_device_filename("mixed-文件-test.txt").is_ok());

        // Hidden files (starting with .) are allowed - they're just normal filenames
        assert!(validate_device_filename(".hidden").is_ok());
        assert!(validate_device_filename(".gitignore").is_ok());

        // Long filenames
        let long_name = "a".repeat(255);
        assert!(validate_device_filename(&long_name).is_ok());
    }

    #[test]
    fn validate_empty_filenames() {
        assert!(validate_device_filename("").is_err());
    }

    #[test]
    fn validate_parent_directory_references() {
        // Parent directory traversal
        assert!(validate_device_filename("../escape").is_err());
        assert!(validate_device_filename("..").is_err());
        assert!(validate_device_filename("file/../../etc/passwd").is_err());
        assert!(validate_device_filename("../../../root").is_err());

        // Mixed with normal components
        assert!(validate_device_filename("normal/../escape").is_err());
        assert!(validate_device_filename("../normal/file.txt").is_err());
    }

    #[test]
    fn validate_absolute_paths() {
        // Unix absolute paths
        assert!(validate_device_filename("/absolute/path").is_err());
        assert!(validate_device_filename("/etc/passwd").is_err());

        // Root only
        assert!(validate_device_filename("/").is_err());
    }

    #[test]
    fn validate_current_directory_reference() {
        assert!(validate_device_filename(".").is_err());
        assert!(validate_device_filename("./file.txt").is_err());
        assert!(validate_device_filename("dir/./file.txt").is_err());
    }

    #[test]
    fn validate_embedded_path_separators() {
        // Forward slash is a path separator on all platforms
        assert!(validate_device_filename("dir/file.txt").is_err());
        assert!(validate_device_filename("a/b/c/d.txt").is_err());

        // Backslash is rejected for cross-platform safety (path separator on Windows)
        assert!(validate_device_filename("dir\\file.txt").is_err());
    }

    #[test]
    fn validate_control_characters() {
        // NUL byte
        assert!(validate_device_filename("file\0null.txt").is_err());

        // Other control characters (0x01-0x1F)
        assert!(validate_device_filename("file\x01ctrl.txt").is_err());
        assert!(validate_device_filename("file\x1Fctrl.txt").is_err());

        // Tab character
        assert!(validate_device_filename("file\ttab.txt").is_err());

        // Newline characters
        assert!(validate_device_filename("file\nnewline.txt").is_err());
        assert!(validate_device_filename("file\rreturn.txt").is_err());
    }

    #[test]
    fn validate_error_messages() {
        let err = validate_device_filename("../escape").unwrap_err();
        let msg = err.to_string().to_lowercase();
        assert!(msg.contains("parent") || msg.contains("traversal") || msg.contains("reference"));

        let err = validate_device_filename("/absolute").unwrap_err();
        let msg = err.to_string().to_lowercase();
        assert!(msg.contains("absolute") || msg.contains("root") || msg.contains("path"));

        let err = validate_device_filename("").unwrap_err();
        let msg = err.to_string().to_lowercase();
        assert!(msg.contains("empty"));

        let err = validate_device_filename("dir/file.txt").unwrap_err();
        let msg = err.to_string().to_lowercase();
        assert!(
            msg.contains("separator") || msg.contains("exactly one") || msg.contains("additional")
        );
    }
}
