use std::{
    path::{Path, PathBuf},
    process::Command,
};

use core::fmt;
use error_stack::{Context, Result, ResultExt};

#[derive(Debug)]
pub enum Error {
    Io,
}

impl fmt::Display for Error {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io => fmt.write_str("Error::Io"),
        }
    }
}

impl Context for Error {}

pub(crate) fn untracked_files(base_path: &Path) -> Result<Vec<PathBuf>, Error> {
    let output = Command::new("git")
        .args(["ls-files", "--others", "--exclude-standard", "--full-name", "-z", "--", "."])
        .current_dir(base_path)
        .output()
        .change_context(Error::Io)?;

    if output.status.success() {
        let stdout = output.stdout;
        let mut results: Vec<PathBuf> = Vec::new();
        for rel in stdout.split(|b| *b == 0).filter(|s| !s.is_empty()) {
            let rel_str = std::str::from_utf8(rel).change_context(Error::Io)?;
            results.push(base_path.join(rel_str));
        }
        return Ok(results);
    }
    Ok(vec![])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_untracked_files() {
        let tmp_dir = tempfile::tempdir().unwrap();
        let untracked = untracked_files(tmp_dir.path()).unwrap();
        assert!(untracked.is_empty());

        std::process::Command::new("git")
            .arg("init")
            .current_dir(tmp_dir.path())
            .output()
            .expect("failed to run git init");

        std::fs::write(tmp_dir.path().join("test.txt"), "test").unwrap();
        let untracked = untracked_files(tmp_dir.path()).unwrap();
        assert!(untracked.len() == 1);
        let expected = tmp_dir.path().join("test.txt");
        assert!(untracked[0] == expected);
    }

    #[test]
    fn test_untracked_files_with_spaces_and_parens() {
        let tmp_dir = tempfile::tempdir().unwrap();

        std::process::Command::new("git")
            .arg("init")
            .current_dir(tmp_dir.path())
            .output()
            .expect("failed to run git init");

        // Nested dirs with spaces and parentheses
        let d1 = tmp_dir.path().join("dir with spaces");
        let d2 = d1.join("(special)");
        std::fs::create_dir_all(&d2).unwrap();

        let f1 = d1.join("file (1).txt");
        let f2 = d2.join("a b (2).rb");
        std::fs::write(&f1, "one").unwrap();
        std::fs::write(&f2, "two").unwrap();

        let mut untracked = untracked_files(tmp_dir.path()).unwrap();
        untracked.sort();

        let mut expected = vec![f1, f2];
        expected.sort();

        assert_eq!(untracked, expected);
    }

    #[test]
    fn test_untracked_files_multiple_files_order_insensitive() {
        let tmp_dir = tempfile::tempdir().unwrap();

        std::process::Command::new("git")
            .arg("init")
            .current_dir(tmp_dir.path())
            .output()
            .expect("failed to run git init");

        let f1 = tmp_dir.path().join("a.txt");
        let f2 = tmp_dir.path().join("b.txt");
        let f3 = tmp_dir.path().join("c.txt");
        std::fs::write(&f1, "A").unwrap();
        std::fs::write(&f2, "B").unwrap();
        std::fs::write(&f3, "C").unwrap();

        let mut untracked = untracked_files(tmp_dir.path()).unwrap();
        untracked.sort();

        let mut expected = vec![f1, f2, f3];
        expected.sort();

        assert_eq!(untracked, expected);
    }

    #[test]
    fn test_untracked_files_excludes_staged() {
        let tmp_dir = tempfile::tempdir().unwrap();

        std::process::Command::new("git")
            .arg("init")
            .current_dir(tmp_dir.path())
            .output()
            .expect("failed to run git init");

        let staged = tmp_dir.path().join("staged.txt");
        let unstaged = tmp_dir.path().join("unstaged.txt");
        std::fs::write(&staged, "I will be staged").unwrap();
        std::fs::write(&unstaged, "I remain untracked").unwrap();

        // Stage one file
        let add_status = std::process::Command::new("git")
            .arg("add")
            .arg("staged.txt")
            .current_dir(tmp_dir.path())
            .output()
            .expect("failed to run git add");
        assert!(
            add_status.status.success(),
            "git add failed: {}",
            String::from_utf8_lossy(&add_status.stderr)
        );

        let mut untracked = untracked_files(tmp_dir.path()).unwrap();
        untracked.sort();

        let expected = vec![unstaged];
        assert_eq!(untracked, expected);
    }
}
