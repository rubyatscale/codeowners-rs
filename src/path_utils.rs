use std::path::{Path, PathBuf};

use path_clean::PathClean;

/// Return `path` relative to `root` if possible; otherwise return `path` unchanged.
pub fn relative_to<'a>(root: &'a Path, path: &'a Path) -> &'a Path {
    path.strip_prefix(root).unwrap_or(path)
}

/// Like `relative_to`, but returns an owned `PathBuf`.
pub fn relative_to_buf(root: &Path, path: &Path) -> PathBuf {
    relative_to(root, path).to_path_buf()
}

/// The path under `project_root` that `path` names, relative to the root, or `None` if it names nothing under it. A relative
/// `path` is relative to the root. Owners are matched against these paths, so another form of one, with a `./`, a repeated `/`,
/// a `.` or `..` segment, or through a symlink, would miss them. `..` is resolved lexically, as for `--config-path`, so after a
/// symlinked directory it names the link's parent.
pub(crate) fn project_relative_path(project_root: &Path, path: &Path) -> Option<PathBuf> {
    // `absolute` fails only for an empty root, or a relative one with no working directory, and neither can be resolved.
    let root = std::path::absolute(project_root).ok()?.clean();
    let cleaned = root.join(path).clean();
    if let Some(relative) = path_under(&root, &cleaned) {
        return Some(relative);
    }
    // The path may reach the project through a symlink, or the root may be the unresolved side, as a library caller's
    // RunConfig can be, so both are resolved. Its ancestors are resolved from the shortest until one is in the project, and the
    // rest of the path is kept as written, so it names what its plain form does: a symlink inside the project keeps its own
    // path, and a file or directory that doesn't exist, or can't be read, is still named. The file itself is never resolved,
    // since CODEOWNERS and the project walk name a symlinked file by its own path, not its target's.
    let canonical_root = root.canonicalize().ok()?;
    let ancestors: Vec<&Path> = cleaned.ancestors().skip(1).collect();
    for ancestor in ancestors.into_iter().rev() {
        if let Ok(resolved) = ancestor.canonicalize()
            && resolved.starts_with(&canonical_root)
        {
            return path_under(&canonical_root, &resolved.join(cleaned.strip_prefix(ancestor).ok()?));
        }
    }
    None
}

/// How owners are matched against a path that `project_relative_path` can't place: as given, but without the root if it's
/// absolute, so it matches as its relative form does.
pub(crate) fn unplaced_path<'a>(project_root: &'a Path, path: &'a Path) -> &'a Path {
    if path.is_absolute() {
        relative_to(project_root, path)
    } else {
        path
    }
}

// The root itself names nothing under it.
fn path_under(root: &Path, path: &Path) -> Option<PathBuf> {
    let relative = path.strip_prefix(root).ok()?;
    (!relative.as_os_str().is_empty()).then(|| relative.to_path_buf())
}

/// Returns true if `path` matches any of the provided glob patterns.
pub(crate) fn matches_globs(path: &Path, globs: &[String]) -> bool {
    match path.to_str() {
        Some(s) => globs.iter().any(|glob| fast_glob::glob_match(glob, s)),
        None => false,
    }
}

/// The leading components of `glob` with no glob syntax in them. Every path the glob can match is under this one.
pub(crate) fn literal_glob_prefix(glob: &str) -> PathBuf {
    Path::new(glob)
        .components()
        .take_while(|component| !component.as_os_str().to_string_lossy().contains(['*', '?', '[', '{', '\\', '!']))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_literal_glob_prefix() {
        for (glob, prefix) in [
            ("config/teams/**/*.yml", "config/teams"),
            ("config/teams/*.yml", "config/teams"),
            ("config/{teams,groups}/*.yml", "config"),
            ("**/teams/*.yml", ""),
            ("config/teams/payroll.yml", "config/teams/payroll.yml"),
        ] {
            assert_eq!(literal_glob_prefix(glob), PathBuf::from(prefix), "{glob}");
        }
    }

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
    fn relative(project_root: &Path, path: impl AsRef<Path>) -> Option<String> {
        project_relative_path(project_root, path.as_ref()).map(|relative| relative.to_string_lossy().into_owned())
    }

    #[test]
    fn project_relative_path_returns_a_plain_path_unchanged() {
        assert_eq!(relative(Path::new("/proj"), "app/a.rb").as_deref(), Some("app/a.rb"));
    }

    #[test]
    fn project_relative_path_drops_dot_components_and_repeated_separators() {
        for path in ["./app/a.rb", "app//a.rb", "app/./a.rb"] {
            assert_eq!(relative(Path::new("/proj"), path).as_deref(), Some("app/a.rb"), "{path}");
        }
    }

    #[test]
    fn project_relative_path_resolves_dot_dot_inside_the_project() {
        for path in ["app/../app/a.rb", "app/models/../a.rb", "../proj/app/a.rb", "/proj/app/../app/a.rb"] {
            assert_eq!(relative(Path::new("/proj"), path).as_deref(), Some("app/a.rb"), "{path}");
        }
    }

    #[test]
    fn project_relative_path_returns_none_for_dot_dot_out_of_the_project() {
        // A real root, so the paths are also resolved.
        let temp_dir = tempfile::tempdir().unwrap();
        let root = temp_dir.path().join("proj");
        std::fs::create_dir_all(root.join("app")).unwrap();
        let absolute = |path: &str| root.join(path).to_string_lossy().into_owned();
        for path in [
            "../a.rb",
            "../../app/a.rb",
            "app/../../a.rb",
            &absolute("../a.rb"),
            &absolute("../../app/a.rb"),
        ] {
            assert_eq!(relative(&root, path), None, "{path}");
        }
    }

    #[test]
    fn project_relative_path_strips_the_root_from_an_absolute_path() {
        assert_eq!(relative(Path::new("/proj"), "/proj/app/a.rb").as_deref(), Some("app/a.rb"));
        assert_eq!(relative(Path::new("/proj/"), "/proj//app/a.rb").as_deref(), Some("app/a.rb"));
        assert_eq!(relative(Path::new("/proj"), "/elsewhere/app/a.rb"), None);
    }

    #[test]
    fn project_relative_path_resolves_dot_dot_in_the_root() {
        assert_eq!(relative(Path::new("/x/../proj"), "app/a.rb").as_deref(), Some("app/a.rb"));
        assert_eq!(relative(Path::new("/x/../proj"), "/proj/app/a.rb").as_deref(), Some("app/a.rb"));
    }

    #[test]
    fn project_relative_path_takes_a_relative_root_from_the_working_directory() {
        let cwd = std::env::current_dir().unwrap();
        assert_eq!(relative(Path::new("."), "./src/lib.rs").as_deref(), Some("src/lib.rs"));
        assert_eq!(relative(Path::new("."), cwd.join("src/lib.rs")).as_deref(), Some("src/lib.rs"));
        assert_eq!(relative(Path::new("src"), "./lib.rs").as_deref(), Some("lib.rs"));
        assert_eq!(relative(Path::new("src"), cwd.join("src/lib.rs")).as_deref(), Some("lib.rs"));
        assert_eq!(relative(Path::new("src"), "src/lib.rs").as_deref(), Some("src/lib.rs"));
        // One that doesn't exist can't be resolved, so only the working directory places it.
        assert_eq!(relative(Path::new("."), "./no/such/dir/a.rb").as_deref(), Some("no/such/dir/a.rb"));
    }

    // A project root with no symlink on its path, the same root through a symlink, and the temp dir holding both. In the
    // project, app/lib_link is a symlink to its lib directory.
    #[cfg(unix)]
    fn symlinked_project() -> (PathBuf, PathBuf, tempfile::TempDir) {
        let temp_dir = tempfile::tempdir().unwrap();
        // On macOS the temp dir is itself reached through a symlink, from /var to /private/var.
        let base = temp_dir.path().canonicalize().unwrap();
        let root = base.join("real/proj");
        std::fs::create_dir_all(root.join("app")).unwrap();
        std::fs::create_dir_all(root.join("lib")).unwrap();
        std::fs::write(root.join("app/a.rb"), "").unwrap();
        std::os::unix::fs::symlink("../lib", root.join("app/lib_link")).unwrap();
        std::os::unix::fs::symlink(base.join("real"), base.join("link")).unwrap();
        (root, base.join("link/proj"), temp_dir)
    }

    #[cfg(unix)]
    #[test]
    fn project_relative_path_resolves_a_path_through_a_symlink_to_the_project() {
        let (root, linked_root, _temp_dir) = symlinked_project();
        assert_eq!(relative(&root, linked_root.join("app/a.rb")).as_deref(), Some("app/a.rb"));
        assert_eq!(relative(&root, linked_root.join("a.rb")).as_deref(), Some("a.rb"));
        assert_eq!(relative(&root, linked_root.join("app/../app/./a.rb")).as_deref(), Some("app/a.rb"));
        assert_eq!(relative(&root, linked_root.join("../a.rb")), None);
    }

    #[cfg(unix)]
    #[test]
    fn project_relative_path_resolves_a_root_given_through_a_symlink() {
        let (root, linked_root, _temp_dir) = symlinked_project();
        assert_eq!(relative(&linked_root, root.join("app/a.rb")).as_deref(), Some("app/a.rb"));
        assert_eq!(relative(&linked_root, "./app/a.rb").as_deref(), Some("app/a.rb"));
    }

    #[cfg(unix)]
    #[test]
    fn project_relative_path_keeps_the_path_of_a_symlinked_file() {
        let (root, linked_root, _temp_dir) = symlinked_project();
        std::os::unix::fs::symlink("a.rb", root.join("app/link.rb")).unwrap();
        assert_eq!(relative(&root, linked_root.join("app/link.rb")).as_deref(), Some("app/link.rb"));
        assert_eq!(relative(&linked_root, root.join("app/link.rb")).as_deref(), Some("app/link.rb"));
        // So one outside the project names nothing in it.
        let outside_link = root.ancestors().nth(2).unwrap().join("link.rb");
        std::os::unix::fs::symlink(root.join("app/a.rb"), &outside_link).unwrap();
        assert_eq!(relative(&root, &outside_link), None);
    }

    #[cfg(unix)]
    #[test]
    fn project_relative_path_resolves_a_path_through_a_symlink_only_as_far_as_the_project() {
        let (root, linked_root, _temp_dir) = symlinked_project();
        // Symlinks to a directory out of the project and to itself, which can't be resolved, and a directory that doesn't exist.
        std::os::unix::fs::symlink(root.ancestors().nth(2).unwrap(), root.join("app/out_link")).unwrap();
        std::os::unix::fs::symlink("loop", root.join("app/loop")).unwrap();
        for path in [
            "app/lib_link/b.rb",
            "app/out_link/b.rb",
            "app/loop/b.rb",
            "app/new/b.rb",
            "app/deleted.rb",
        ] {
            assert_eq!(relative(&root, path).as_deref(), Some(path), "{path}");
            assert_eq!(relative(&root, linked_root.join(path)).as_deref(), Some(path), "{path}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn project_relative_path_resolves_dot_dot_before_any_symlink() {
        let (root, linked_root, _temp_dir) = symlinked_project();
        let base = root.ancestors().nth(2).unwrap();
        std::os::unix::fs::symlink(root.join("app"), base.join("app_link")).unwrap();
        // Each `..` names the link's parent, not its target's.
        assert_eq!(
            relative(&root, linked_root.join("app/lib_link/../a.rb")).as_deref(),
            Some("app/a.rb")
        );
        assert_eq!(relative(&root, base.join("app_link/../a.rb")), None);
    }

    #[test]
    fn project_relative_path_returns_none_for_the_root_itself() {
        let temp_dir = tempfile::tempdir().unwrap();
        let root = temp_dir.path();
        for path in [Path::new(""), Path::new("."), Path::new("app/.."), root, &root.join(".")] {
            assert_eq!(relative(root, path), None, "{}", path.display());
        }
    }
}
