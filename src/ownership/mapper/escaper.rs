/// `path` with a `\` before each character that fast_glob reads as a pattern, so a glob built from it matches it literally. `!` is
/// one, since it negates a glob it starts. Nothing else is escaped, since fast_glob reads `\b`, `\n`, `\r` and `\t` as control
/// characters.
pub fn escape_glob(path: &str) -> String {
    let mut escaped = String::with_capacity(path.len());
    for c in path.chars() {
        if matches!(c, '\\' | '*' | '?' | '[' | ']' | '{' | '}' | '!') {
            escaped.push('\\');
        }
        escaped.push(c);
    }
    escaped
}

/// The glob for every file under `directory`, a project-relative path. That's `**/**` for the project root, the empty path, which
/// CODEOWNERS gets as `/**/**`.
pub fn directory_glob(directory: &str) -> String {
    if directory.is_empty() {
        "**/**".to_owned()
    } else {
        format!("{}/**/**", escape_glob(directory))
    }
}

#[cfg(test)]
mod tests {
    use fast_glob::glob_match;

    use super::*;

    #[test]
    fn test_escape_glob_escapes_each_metacharacter() {
        assert_eq!(escape_glob("a*b"), "a\\*b");
        assert_eq!(escape_glob("a?b"), "a\\?b");
        assert_eq!(escape_glob("[ab]"), "\\[ab\\]");
        assert_eq!(escape_glob("{a,b}"), "\\{a,b\\}");
        assert_eq!(escape_glob("!a"), "\\!a");
        assert_eq!(escape_glob("a!"), "a\\!");
        assert_eq!(escape_glob("a\\b"), "a\\\\b");
    }

    #[test]
    fn test_escape_glob_escapes_a_backslash_once() {
        assert_eq!(escape_glob("a\\[b"), "a\\\\\\[b");
        assert_eq!(escape_glob("a\\\\b"), "a\\\\\\\\b");
    }

    #[test]
    fn test_escape_glob_leaves_other_characters_alone() {
        let path = "app/b n r t/(x)/a,b-c^d/e.rb";
        assert_eq!(escape_glob(path), path);
    }

    #[test]
    fn test_directory_glob() {
        assert_eq!(directory_glob("packs/[ab]"), "packs/\\[ab\\]/**/**");
        assert_eq!(directory_glob(""), "**/**");
        assert!(glob_match(directory_glob(""), "x.rb"));
        assert!(glob_match(directory_glob(""), "app/x.rb"));
    }

    #[test]
    fn test_directory_glob_matches_only_its_own_directory() {
        // Each name, with directories its unescaped glob would also match.
        let names: &[(&str, &[&str])] = &[
            ("[ab]", &["a", "b"]),
            ("{a,b}", &["a", "b"]),
            ("{legacy}", &["legacy"]),
            ("a*", &["a", "ab"]),
            ("a?", &["ab"]),
            ("!x", &["legacy"]),
            ("a\\b", &[]),
            ("a]", &[]),
            ("a}", &[]),
            ("(x)", &[]),
            ("a,b", &[]),
        ];
        for (name, siblings) in names {
            let glob = directory_glob(name);
            assert!(glob_match(&glob, format!("{name}/f.rb")), "{glob} should match {name}/f.rb");
            assert!(glob_match(&glob, format!("{name}/x/f.rb")), "{glob} should match {name}/x/f.rb");
            for sibling in *siblings {
                assert!(
                    glob_match(format!("{name}/**/**"), format!("{sibling}/f.rb")),
                    "the unescaped glob for {name} should match {sibling}/f.rb"
                );
                assert!(
                    !glob_match(&glob, format!("{sibling}/f.rb")),
                    "{glob} shouldn't match {sibling}/f.rb"
                );
            }
        }
    }
}
