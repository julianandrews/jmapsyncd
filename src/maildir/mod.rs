use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

pub mod filename;

/// Create the standard Maildir directory structure (cur/, new/, tmp/) under `base`.
pub fn create_maildir_dirs(base: &Path) -> Result<()> {
    for sub in ["cur", "new", "tmp"] {
        std::fs::create_dir_all(base.join(sub))
            .with_context(|| format!("creating maildir subdirectory {sub} under {}", base.display()))?;
    }
    Ok(())
}

/// Check if `base` is a valid Maildir (has cur/, new/, tmp/ subdirectories).
pub fn is_maildir(base: &Path) -> bool {
    base.join("cur").is_dir() && base.join("new").is_dir() && base.join("tmp").is_dir()
}

/// List all message files in the `cur/` directory of a Maildir.
pub fn list_cur(base: &Path) -> Result<Vec<PathBuf>> {
    list_files(&base.join("cur"))
}

/// List all message files in the `new/` directory of a Maildir.
pub fn list_new(base: &Path) -> Result<Vec<PathBuf>> {
    list_files(&base.join("new"))
}

fn list_files(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    if dir.is_dir() {
        for entry in std::fs::read_dir(dir).with_context(|| format!("reading directory {}", dir.display()))? {
            let entry = entry?;
            let path = entry.path();
            if path.is_file() {
                files.push(path);
            }
        }
    }
    files.sort();
    Ok(files)
}

/// Move a message from `new/` to `cur/`, applying the given flags to the filename.
/// Returns the resulting path in `cur/`.
pub fn move_to_cur(base: &Path, filename: &str, flags: &str) -> Result<PathBuf> {
    let src = base.join("new").join(filename);
    let base_name = filename.split(":2,").next().unwrap_or(filename);
    let suffix = filename::flags_to_suffix(flags);
    let dest_name = format!("{base_name}{suffix}");
    let dest = base.join("cur").join(&dest_name);
    std::fs::rename(&src, &dest)
        .with_context(|| format!("moving {} to {}", src.display(), dest.display()))?;
    Ok(dest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn create_maildir_dirs_creates_structure() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("Maildir");
        create_maildir_dirs(&path).unwrap();
        assert!(path.join("cur").is_dir());
        assert!(path.join("new").is_dir());
        assert!(path.join("tmp").is_dir());
    }

    #[test]
    fn create_maildir_dirs_is_idempotent() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("Maildir");
        create_maildir_dirs(&path).unwrap();
        create_maildir_dirs(&path).unwrap();
        assert!(path.join("cur").is_dir());
    }

    #[test]
    fn is_maildir_true() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("Maildir");
        create_maildir_dirs(&path).unwrap();
        assert!(is_maildir(&path));
    }

    #[test]
    fn is_maildir_false() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(!is_maildir(tmp.path()));
    }

    #[test]
    fn list_cur_returns_files() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("Maildir");
        create_maildir_dirs(&path).unwrap();

        let cur = path.join("cur");
        let f1 = cur.join("1700000000.host1:2,S");
        let f2 = cur.join("1700000001.host2:2,");
        std::fs::write(&f1, "content").unwrap();
        std::fs::write(&f2, "content").unwrap();

        let files = list_cur(&path).unwrap();
        assert_eq!(files.len(), 2);
    }

    #[test]
    fn list_cur_empty() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("Maildir");
        create_maildir_dirs(&path).unwrap();
        let files = list_cur(&path).unwrap();
        assert!(files.is_empty());
    }

    #[test]
    fn list_cur_non_maildir_returns_empty() {
        let tmp = tempfile::tempdir().unwrap();
        let files = list_cur(tmp.path()).unwrap();
        assert!(files.is_empty());
    }

    #[test]
    fn list_new_returns_files() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("Maildir");
        create_maildir_dirs(&path).unwrap();

        let new = path.join("new");
        let f1 = new.join("1700000000.host1");
        std::fs::write(&f1, "content").unwrap();

        let files = list_new(&path).unwrap();
        assert_eq!(files.len(), 1);
    }

    #[test]
    fn list_new_empty() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("Maildir");
        create_maildir_dirs(&path).unwrap();
        let files = list_new(&path).unwrap();
        assert!(files.is_empty());
    }

    #[test]
    fn move_to_cur_moves_and_adds_flags() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("Maildir");
        create_maildir_dirs(&path).unwrap();

        let new_file = path.join("new").join("1700000000.host1");
        let mut f = std::fs::File::create(&new_file).unwrap();
        f.write_all(b"content").unwrap();
        drop(f);

        let cur_path = move_to_cur(&path, "1700000000.host1", "S").unwrap();
        assert_eq!(
            cur_path,
            path.join("cur").join("1700000000.host1:2,S")
        );
        assert!(cur_path.exists());
        assert!(!new_file.exists());
    }

    #[test]
    fn move_to_cur_no_flags() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("Maildir");
        create_maildir_dirs(&path).unwrap();

        let new_file = path.join("new").join("1700000000.host1");
        std::fs::write(&new_file, "content").unwrap();

        let cur_path = move_to_cur(&path, "1700000000.host1", "").unwrap();
        assert_eq!(
            cur_path,
            path.join("cur").join("1700000000.host1:2,")
        );
        assert!(cur_path.exists());
    }

    #[test]
    fn move_to_cur_normalizes_flags() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("Maildir");
        create_maildir_dirs(&path).unwrap();

        let new_file = path.join("new").join("1700000000.host1");
        std::fs::write(&new_file, "content").unwrap();

        let cur_path = move_to_cur(&path, "1700000000.host1", "SR").unwrap();
        assert_eq!(
            cur_path,
            path.join("cur").join("1700000000.host1:2,RS")
        );
    }

    #[test]
    fn list_only_files_not_dirs() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("Maildir");
        create_maildir_dirs(&path).unwrap();

        let cur = path.join("cur");
        std::fs::write(cur.join("msg1:2,S"), "content").unwrap();
        std::fs::create_dir(cur.join("subdir")).unwrap();

        let files = list_cur(&path).unwrap();
        assert_eq!(files.len(), 1);
    }
}
