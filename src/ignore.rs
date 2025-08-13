pub(crate) fn matches_path(ignore_globs: &[String], path: &str) -> bool {
    ignore_globs.iter().any(|rule| glob_match(rule, path))
}

fn glob_match(glob: &str, path: &str) -> bool {
    let glob = glob::Pattern::new(glob).unwrap();
    glob.matches(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_matches_path() {
        assert!(matches_path(&["**/*.rs".to_string()], "src/main.rs"));
        assert!(matches_path(&["src/**".to_string()], "src/main.rs"));
    }
}