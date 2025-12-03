use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    process::Command,
};

pub(crate) fn find_tracked_files(base_path: &Path) -> Option<HashMap<PathBuf, bool>> {
    let output = Command::new("git")
        .args(["ls-files", "--full-name", "-z", "--", "."])
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
        .map(|rel| {
            let rel_str = std::str::from_utf8(rel).ok()?;
            let absolute_path = base_path.join(rel_str);
            Some((absolute_path, true))
        })
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
}
