use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    process::Command,
};

pub(crate) fn find_tracked_files(base_path: &Path) -> Option<HashMap<PathBuf, bool>> {
    let output = Command::new("git")
        .args(["ls-files", "-z", "--", "."])
        .current_dir(base_path)
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let results: HashMap<PathBuf, bool> = output
        .stdout
        .split(|&b| b == b'\0')
        .filter(|chunk| !chunk.is_empty())
        .map(|rel| std::str::from_utf8(rel).ok().map(|s| (base_path.join(s), true)))
        .collect::<Option<HashMap<PathBuf, bool>>>()?;

    Some(results)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_untracked_files() {
        let tmp_dir = tempfile::tempdir().unwrap();
        assert!(find_tracked_files(tmp_dir.path()).is_none());

        std::process::Command::new("git")
            .arg("init")
            .current_dir(tmp_dir.path())
            .output()
            .expect("failed to run git init");

        std::fs::write(tmp_dir.path().join("test.txt"), "test").unwrap();
        let tracked = find_tracked_files(tmp_dir.path()).unwrap();
        assert!(tracked.is_empty());

        std::process::Command::new("git")
            .arg("add")
            .arg("test.txt")
            .current_dir(tmp_dir.path())
            .output()
            .expect("failed to add test.txt");

        let tracked = find_tracked_files(tmp_dir.path()).unwrap();
        assert!(tracked.len() == 1);
        assert!(tracked.get(&tmp_dir.path().join("test.txt")).unwrap());
    }

    #[test]
    fn test_tracked_files_from_subdirectory() {
        let tmp_dir = tempfile::tempdir().unwrap();
        let backend_dir = tmp_dir.path().join("backend");
        let tracked_file = backend_dir.join("app/models/foo.rb");

        std::process::Command::new("git")
            .arg("init")
            .current_dir(tmp_dir.path())
            .output()
            .expect("failed to run git init");

        std::fs::create_dir_all(tracked_file.parent().unwrap()).unwrap();
        std::fs::write(&tracked_file, "class Foo; end").unwrap();
        std::fs::write(tmp_dir.path().join("README.md"), "readme").unwrap();

        std::process::Command::new("git")
            .args(["add", "--all"])
            .current_dir(tmp_dir.path())
            .output()
            .expect("failed to add tracked files");

        let tracked = find_tracked_files(&backend_dir).unwrap();
        assert_eq!(tracked.len(), 1);
        assert!(tracked.get(&tracked_file).unwrap());
        assert!(!tracked.contains_key(&backend_dir.join("backend/app/models/foo.rb")));
    }
}
