use std::path::{Path, PathBuf};

/// Return `path` relative to `root` if possible; otherwise return `path` unchanged.
pub fn relative_to<'a>(root: &'a Path, path: &'a Path) -> &'a Path {
    path.strip_prefix(root).unwrap_or(path)
}

/// Like `relative_to`, but returns an owned `PathBuf`.
pub fn relative_to_buf(root: &Path, path: &Path) -> PathBuf {
    relative_to(root, path).to_path_buf()
}

/// Returns true if `path` matches any of the provided glob patterns.
pub(crate) fn matches_globs(path: &Path, globs: &[String]) -> bool {
    match path.to_str() {
        Some(s) => globs.iter().any(|glob| fast_glob::glob_match(glob, s)),
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_globs_double_star_matches_root_level_and_nested_files() {
        let globs = vec!["**/*".to_string()];
        assert!(matches_globs(Path::new("root.rb"), &globs));
        assert!(matches_globs(Path::new("app/models/user.rb"), &globs));
    }

    #[test]
    fn relative_to_returns_relative_when_under_root() {
        let root = Path::new("/a/b");
        let path = Path::new("/a/b/c/d.txt");
        let rel = relative_to(root, path);
        assert_eq!(rel, Path::new("c/d.txt"));
    }

    #[test]
    fn relative_to_returns_input_when_not_under_root() {
        let root = Path::new("/a/b");
        let path = Path::new("/x/y/z.txt");
        let rel = relative_to(root, path);
        assert_eq!(rel, path);
    }

    #[test]
    fn relative_to_handles_equal_paths() {
        let root = Path::new("/a/b");
        let path = Path::new("/a/b");
        let rel = relative_to(root, path);
        assert_eq!(rel, Path::new(""));
    }

    #[test]
    fn relative_to_buf_matches_relative_to() {
        let root = Path::new("/proj");
        let path = Path::new("/proj/src/lib.rs");
        let rel_ref = relative_to(root, path);
        let rel_buf = relative_to_buf(root, path);
        assert_eq!(rel_ref, rel_buf.as_path());
    }
}
