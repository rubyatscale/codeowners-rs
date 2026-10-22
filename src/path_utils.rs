use std::path::{Component, Path, PathBuf};

/// Return `path` relative to `root` if possible; otherwise return `path` unchanged.
pub fn relative_to<'a>(root: &'a Path, path: &'a Path) -> &'a Path {
    path.strip_prefix(root).unwrap_or(path)
}

/// Like `relative_to`, but returns an owned `PathBuf`.
pub fn relative_to_buf(root: &Path, path: &Path) -> PathBuf {
    relative_to(root, path).to_path_buf()
}

/// `path` without `.` components or repeated separators, or `None` if it has a `..`, a root or a prefix. Owners are matched
/// against plain project-relative paths, and no other path can be made into one without asking the file system.
pub(crate) fn plain_relative(path: &Path) -> Option<PathBuf> {
    let mut plain = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::Normal(name) => plain.push(name),
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => return None,
        }
    }
    Some(plain)
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

    const OWNED_GLOB: &str = "{app,components,config,frontend,lib,packs,spec,danger,script}/**/*.{rb,arb,erb,rake,js,jsx,ts,tsx}";

    #[test]
    fn test_matches_globs() {
        assert!(matches_globs(Path::new("script/.eslintrc.js"), &[OWNED_GLOB.to_string()]));
    }

    #[test]
    fn test_glob_match() {
        assert!(fast_glob::glob_match(OWNED_GLOB, "script/.eslintrc.js"));
    }

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

    // Compared as strings, as globs see them, since Path equality ignores a repeated separator or a `.` past the start.
    fn plain(path: &str) -> Option<String> {
        plain_relative(Path::new(path)).map(|plain| plain.to_string_lossy().into_owned())
    }

    #[test]
    fn plain_relative_returns_a_plain_path_unchanged() {
        assert_eq!(plain("a/b").as_deref(), Some("a/b"));
    }

    #[test]
    fn plain_relative_drops_dot_components_and_repeated_separators() {
        assert_eq!(plain("./a/b").as_deref(), Some("a/b"));
        assert_eq!(plain("a/./b").as_deref(), Some("a/b"));
        assert_eq!(plain("a//b").as_deref(), Some("a/b"));
    }

    #[test]
    fn plain_relative_returns_none_for_parent_dirs_and_roots() {
        assert_eq!(plain("a/../b"), None);
        assert_eq!(plain("/a/b"), None);
    }
}
