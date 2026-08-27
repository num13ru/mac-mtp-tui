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
/// - On Windows: characters illegal in Windows filenames (`* ? " < > |`),
///   the `:` stream separator (NTFS alternate data streams), names ending
///   in a dot or space, and reserved device names
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

    // Reject parent directory traversal before parsing, so traversal names
    // are reported as traversal rather than as generic separators.
    // (`..` can never be a valid single-component filename.)
    if filename == ".." || filename.contains("../") {
        anyhow::bail!("filename cannot contain parent directory references");
    }

    // Reject any forward slash before parsing. `Path::components()`
    // normalizes away trailing separators and `.` components, so names
    // such as `file/` or `file/.` would otherwise be accepted despite the
    // single-filename invariant and then passed to MTP rename/mkdir.
    if filename.contains('/') {
        anyhow::bail!("filename cannot contain path separators");
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

    // On Windows, enforce extra rules that go beyond path structure.
    #[cfg(windows)]
    reject_windows_unsafe_name(filename)?;

    Ok(filename)
}

/// Windows-specific filename rules.
///
/// Windows filenames have restrictions beyond path structure:
///
/// - `* ? " < > |` are illegal in file names.
/// - `:` addresses NTFS alternate data streams, so a name such as
///   `victim.txt:stream` must not be treated as a plain filename.
/// - Names ending in a dot or space are silently normalized by the Windows
///   APIs, which can alias a validated name to a different existing file
///   (or, after normalization, become empty).
/// - The base name `CON`, `PRN`, `AUX`, `NUL`, `COM1`-`COM9`, and
///   `LPT1`-`LPT9` is a reserved device name regardless of extension and
///   case.
///
/// The helper is compiled on all hosts so it can be unit-tested everywhere,
/// but is only enforced by `validate_device_filename` on Windows.
#[cfg_attr(not(windows), allow(dead_code))]
fn reject_windows_unsafe_name(filename: &str) -> Result<()> {
    if filename
        .chars()
        .any(|c| matches!(c, '*' | '?' | '"' | '<' | '>' | '|' | ':'))
    {
        anyhow::bail!("filename contains a character that is not allowed on Windows");
    }

    // Windows silently strips trailing dots and spaces when resolving names,
    // so reject them to keep the validated name exactly as written.
    let stripped = filename.trim_end_matches(['.', ' ']);
    if stripped.is_empty() {
        anyhow::bail!("filename reduces to an empty name on Windows");
    }
    if !filename.ends_with(stripped) {
        anyhow::bail!("filename cannot end with a dot or a space on Windows");
    }

    // Reserved device names are invalid with any extension and any case.
    let stem = filename
        .split('.')
        .next()
        .unwrap_or(filename)
        .to_ascii_uppercase();
    const RESERVED_NAMES: [&str; 22] = [
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
        "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ];
    if RESERVED_NAMES.contains(&stem.as_str()) {
        anyhow::bail!("filename is a reserved Windows device name");
    }

    Ok(())
}

/// Checks whether `path` is a safe target for a host download.
///
/// A path is safe when it either does not exist at all, or exists as a
/// regular file. Symlinks are rejected even when dangling, because
/// `File::create` follows them and would write to (or create) the link
/// target, which may lie outside the selected directory. Dangling symlinks
/// additionally slip past `Path::exists()`, hiding them from overwrite
/// prompts, so they must be caught here.
pub fn is_safe_download_target(path: &Path) -> bool {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata.file_type().is_file(),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => true,
        Err(_) => false,
    }
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

        // Trailing separators and trailing `.` are normalized away by
        // Path::components() and must still be rejected: the raw name is
        // what gets sent to MTP rename/mkdir.
        assert!(validate_device_filename("file/").is_err());
        assert!(validate_device_filename("file//").is_err());
        assert!(validate_device_filename("file/.").is_err());
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

    // The Windows rules are compiled on all hosts so these tests run in
    // cross-platform CI; on Windows they are also exercised through
    // `validate_device_filename` itself.

    #[test]
    fn windows_forbidden_characters_and_data_streams() {
        // NTFS alternate data stream separators must be rejected.
        assert!(reject_windows_unsafe_name("victim.txt:stream").is_err());
        assert!(reject_windows_unsafe_name("file:name.txt").is_err());
        assert!(reject_windows_unsafe_name("a:b").is_err());

        // Characters illegal in Windows filenames.
        for name in ["a*b", "a?b", "a\"b", "a<b", "a>b", "a|b"] {
            assert!(
                reject_windows_unsafe_name(name).is_err(),
                "{name} should be rejected"
            );
        }

        // Plain names are fine.
        assert!(reject_windows_unsafe_name("normal.txt").is_ok());
        assert!(reject_windows_unsafe_name("track 12").is_ok());
    }

    #[test]
    fn windows_trailing_dots_and_spaces() {
        // Trailing dots/spaces would be silently normalized away by the
        // Windows APIs, aliasing the name to a different existing file.
        assert!(reject_windows_unsafe_name("victim.txt.").is_err());
        assert!(reject_windows_unsafe_name("victim.txt ").is_err());
        assert!(reject_windows_unsafe_name("victim.txt. ").is_err());

        // Names that normalize to empty are rejected as well.
        assert!(reject_windows_unsafe_name("...").is_err());
        assert!(reject_windows_unsafe_name(".").is_err());

        // Interior spaces and dots are legitimate.
        assert!(reject_windows_unsafe_name("my file.txt").is_ok());
        assert!(reject_windows_unsafe_name("archive.v2.tar").is_ok());
    }

    #[test]
    fn windows_reserved_device_names() {
        for name in [
            "CON", "con", "Con.txt", "PRN.log", "aux", "NUL", "COM1", "com9", "Com3.bin", "LPT1",
            "lpt9", "LPT8.txt",
        ] {
            assert!(
                reject_windows_unsafe_name(name).is_err(),
                "{name} should be rejected"
            );
        }

        // Similar-but-not-reserved names must still be accepted.
        assert!(reject_windows_unsafe_name("COM10.txt").is_ok());
        assert!(reject_windows_unsafe_name("CONSOLE.txt").is_ok());
        assert!(reject_windows_unsafe_name("NULLED.txt").is_ok());
    }

    // Windows-targeted: on Windows, the full validator must enforce the
    // same rules end to end before any join happens.
    #[test]
    fn is_safe_download_target_rejects_symlinks() {
        use std::io::Write;
        use std::os::unix::fs::symlink;

        let dir = tempfile::tempdir().expect("tempdir");
        let outside = dir.path().join("outside.txt");
        let outside_name = outside.display().to_string();
        {
            let mut f = std::fs::File::create(&outside).expect("create outside");
            writeln!(f, "outside").expect("write outside");
        }

        // No entry yet: safe.
        let missing = dir.path().join("missing.txt");
        assert!(is_safe_download_target(&missing));

        // Regular file: safe (overwrite is handled by the UI prompt).
        let regular = dir.path().join("regular.txt");
        std::fs::write(&regular, b"data").expect("write regular");
        assert!(is_safe_download_target(&regular));

        // Live symlink: rejected, even though it points inside the directory
        // here; the target could equally well be outside it.
        let live_link = dir.path().join("live.lnk");
        symlink("regular.txt", &live_link).expect("live symlink");
        assert!(!is_safe_download_target(&live_link));

        // Dangling symlink: rejected (Path::exists() would report false).
        let dangling = dir.path().join("dangling.lnk");
        symlink("no-such-target", &dangling).expect("dangling symlink");
        assert!(!is_safe_download_target(&dangling));
        assert!(!dangling.exists());

        // Symlink to an absolute path outside the directory: rejected.
        let absolute_link = dir.path().join("absolute.lnk");
        symlink(outside_name, &absolute_link).expect("absolute symlink");
        assert!(!is_safe_download_target(&absolute_link));

        // Directory: rejected.
        assert!(!is_safe_download_target(dir.path()));
    }

    #[cfg(windows)]
    #[test]
    fn validate_device_filename_enforces_windows_rules_on_windows() {
        for name in [
            "victim.txt:stream",
            "a|b",
            "victim.txt.",
            "victim.txt ",
            "CON",
            "nul.txt",
        ] {
            assert!(
                validate_device_filename(name).is_err(),
                "{name} should be rejected"
            );
        }
        assert!(validate_device_filename("normal.txt").is_ok());
    }
}
