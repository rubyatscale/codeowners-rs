use std::{
    collections::{HashMap, HashSet},
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

/// Every tracked path and every directory above one, relative to `base_path`. A tracked path is itself a directory when it's a
/// submodule, which `git ls-files` lists without its contents. The root is the empty path, and is always included.
pub(crate) fn tracked_directories(base_path: &Path, tracked_files: &HashMap<PathBuf, bool>) -> HashSet<PathBuf> {
    let mut directories = HashSet::from([PathBuf::new()]);
    for relative_file in tracked_files.keys().filter_map(|file| file.strip_prefix(base_path).ok()) {
        for directory in relative_file.ancestors() {
            // Its ancestors were added along with it.
            if !directories.insert(directory.to_path_buf()) {
                break;
            }
        }
    }
    directories
}

#[cfg(test)]
mod tests {
    use super::*;

    include!("../tests/support/git_env.rs");

    #[test]
    fn test_untracked_files() {
        assert_git_env_isolated();
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
        assert_git_env_isolated();
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

    #[test]
    fn test_tracked_directories() {
        let base_path = Path::new("/project");
        let tracked: HashMap<PathBuf, bool> = ["app/models/user.rb", "app/models/concerns/named.rb", "vendored/submodule_gem"]
            .iter()
            .map(|file| (base_path.join(file), true))
            .collect();

        let directories = tracked_directories(base_path, &tracked);
        for directory in ["", "app", "app/models", "app/models/concerns", "vendored", "vendored/submodule_gem"] {
            assert!(directories.contains(Path::new(directory)), "{directory}");
        }
        assert!(!directories.contains(Path::new("app/assets")));
    }

    #[test]
    fn test_tracked_directories_always_include_the_root() {
        let expected: HashSet<PathBuf> = HashSet::from([PathBuf::new()]);
        assert_eq!(tracked_directories(Path::new("/project"), &HashMap::new()), expected);
    }
}
