pub fn escape_brackets(path: &str) -> String {
    path.replace("[", "\\[").replace("]", "\\]")
}
