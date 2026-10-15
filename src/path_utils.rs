use std::path::{Component, Path, PathBuf};

/// Return `path` relative to `root` if possible; otherwise return `path` unchanged.
pub fn relative_to<'a>(root: &'a Path, path: &'a Path) -> &'a Path {
    path.strip_prefix(root).unwrap_or(path)
}

/// Like `relative_to`, but returns an owned `PathBuf`.
pub fn relative_to_buf(root: &Path, path: &Path) -> PathBuf {
    relative_to(root, path).to_path_buf()
}

/// Reduce a caller-supplied `path` to the project-relative form that
/// [`crate::project::Project::relative_path`] produces for walked files.
///
/// Unlike [`relative_to`], which passes an unstrippable path through unchanged, this
/// reports failure. A path that cannot be placed inside the project is not a path the
/// per-file checks can say anything about, and silently treating it as relative is how
/// `/var/...` came to be compared against project-relative paths and matched nothing.
///
/// Purely lexical — no filesystem access, so it is safe on a path that no longer exists
/// (a deleted file in a changeset). `.` components are dropped and `..` pops the
/// preceding component, so `./a/b.rb` and `a/c/../b.rb` both reduce to `a/b.rb`.
///
/// Returns `None` when `path` is absolute and does not lie under `root`, when it escapes
/// `root` via `..`, or when it *is* `root`. The absolute case is not necessarily final:
/// `cli.rs` canonicalizes `--project-root`, so on macOS a root of `/private/var/...` will
/// not strip a caller-supplied `/var/...`. A caller that gets `None` for an absolute path
/// should retry with a canonicalized copy.
pub fn project_relative(root: &Path, path: &Path) -> Option<PathBuf> {
    let relative = if path.is_absolute() { path.strip_prefix(root).ok()? } else { path };

    let normalized = lexically_normalize(relative);
    if normalized.as_os_str().is_empty() || normalized.starts_with("..") {
        return None;
    }

    Some(normalized)
}

/// Like [`project_relative`], but consults the filesystem when the lexical attempt fails.
///
/// An absolute path only strips if it and `root` agree about symlinks, and there is no
/// guarantee they do. `cli.rs` canonicalizes `--project-root`, but a library caller building
/// its own `RunConfig` (which is how the `code_ownership` gem calls in) does not. So on
/// macOS, where `TMPDIR` lives under `/var`, a symlink to `/private/var`, *either* side can
/// be the unresolved one, and in a symlinked checkout the same is true generally. Resolving
/// only one side leaves the other failing exactly as silently, so the retry resolves both.
///
/// It resolves the **parent** and re-attaches the file name, rather than canonicalizing the
/// whole path. Canonicalizing the leaf would follow a symlinked *file*, and the project walk
/// records the symlink path rather than its target — so an absolute path naming a symlink
/// would be checked as a different file than the caller asked about, and pass or fail on
/// that file's ownership instead. A symlinked *ancestor* is still resolved, unavoidably:
/// that is the whole point in the `/var` case, and the walk does not follow symlinked
/// directories anyway, so such a path names no walked file under either spelling.
///
/// `canonical_root` is the resolved `root`, passed in rather than computed so a caller
/// normalizing a whole changeset pays for it once instead of once per path.
pub fn resolve_project_relative(root: &Path, canonical_root: Option<&Path>, path: &Path) -> Option<PathBuf> {
    if let Some(relative) = project_relative(root, path) {
        return Some(relative);
    }

    // Only an absolute path can be rescued. A relative path is interpreted against the
    // project root by contract -- that is what `--help` promises -- and the lexical pass is
    // the whole of that interpretation, so failure means it escapes the root. Retrying would
    // resolve it against the process CWD instead, quietly switching interpretation frames:
    // the same arguments would then mean different files depending on where the command was
    // run from. It also spends a syscall per path to reach that wrong answer.
    if !path.is_absolute() {
        return None;
    }

    let resolved = path.parent()?.canonicalize().ok()?.join(path.file_name()?);

    project_relative(canonical_root.unwrap_or(root), &resolved)
}

/// Resolve `.` and `..` without touching the filesystem.
///
/// Deliberately lexical: canonicalizing would also resolve symlinks, and the project walk
/// records the symlink path rather than its target, so resolving here would produce a path
/// that matches no walked file.
fn lexically_normalize(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();

    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                // A `..` that cannot pop is retained, so the caller can detect the escape.
                //
                // A retained `..` must never itself be popped by a later one: `pop()` does
                // not distinguish it from a real component, so `../../a` cancelled its own
                // escape and came out as `a` -- reporting an out-of-project path as though
                // it named a file inside the project.
                let escaped = matches!(normalized.components().next_back(), Some(Component::ParentDir));
                if escaped || !normalized.pop() {
                    normalized.push(Component::ParentDir);
                }
            }
            other => normalized.push(other),
        }
    }

    normalized
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn project_relative_passes_through_a_plain_relative_path() {
        let rel = project_relative(Path::new("/proj"), Path::new("ruby/app/a.rb"));
        assert_eq!(rel, Some(PathBuf::from("ruby/app/a.rb")));
    }

    #[test]
    fn project_relative_strips_a_leading_dot_slash() {
        // `./a.rb` and `a.rb` name the same file, but only one of them used to match a
        // walked project file -- the other was silently dropped by the owned_globs filter.
        let rel = project_relative(Path::new("/proj"), Path::new("./ruby/app/a.rb"));
        assert_eq!(rel, Some(PathBuf::from("ruby/app/a.rb")));
    }

    #[test]
    fn project_relative_resolves_interior_parent_dirs() {
        let rel = project_relative(Path::new("/proj"), Path::new("ruby/services/../app/a.rb"));
        assert_eq!(rel, Some(PathBuf::from("ruby/app/a.rb")));
    }

    #[test]
    fn project_relative_strips_the_root_from_an_absolute_path() {
        let rel = project_relative(Path::new("/proj"), Path::new("/proj/ruby/app/a.rb"));
        assert_eq!(rel, Some(PathBuf::from("ruby/app/a.rb")));
    }

    #[test]
    fn project_relative_rejects_an_absolute_path_outside_the_root() {
        // The caller retries with the parent resolved; see `resolve_project_relative`.
        assert_eq!(project_relative(Path::new("/private/proj"), Path::new("/proj/a.rb")), None);
    }

    #[test]
    fn project_relative_rejects_a_path_escaping_the_root() {
        assert_eq!(project_relative(Path::new("/proj"), Path::new("../outside/a.rb")), None);
    }

    #[test]
    fn project_relative_rejects_a_path_escaping_via_repeated_parent_dirs() {
        // `pop()` does not distinguish a retained `..` from a real component, so this used to
        // cancel its own escape and come out as `ruby/app/a.rb` -- an out-of-project path
        // silently reported as though it named a file inside the project.
        assert_eq!(project_relative(Path::new("/proj"), Path::new("../../ruby/app/a.rb")), None);
        assert_eq!(project_relative(Path::new("/proj"), Path::new("../../../a.rb")), None);
    }

    #[test]
    fn project_relative_rejects_a_path_that_climbs_back_out() {
        // Interior `..` still pops normally; the escape only has to survive once it starts.
        assert_eq!(project_relative(Path::new("/proj"), Path::new("ruby/../../a.rb")), None);
    }

    #[test]
    fn project_relative_pops_interior_parent_dirs_without_escaping() {
        let rel = project_relative(Path::new("/proj"), Path::new("ruby/app/models/../../app/a.rb"));
        assert_eq!(rel, Some(PathBuf::from("ruby/app/a.rb")));
    }

    #[test]
    fn resolve_project_relative_does_not_touch_the_filesystem_for_a_relative_path() {
        // A relative path is project-root-relative by contract, so the lexical pass is the
        // whole interpretation. Retrying would resolve it against the process CWD, making
        // the same arguments mean different files depending on where the command was run.
        assert_eq!(
            resolve_project_relative(Path::new("/proj"), Some(Path::new("/proj")), Path::new("../outside/a.rb")),
            None
        );
    }

    #[test]
    fn project_relative_rejects_the_root_itself() {
        assert_eq!(project_relative(Path::new("/proj"), Path::new("/proj")), None);
    }
}
