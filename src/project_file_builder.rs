use lazy_static::lazy_static;
use regex::Regex;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;

use crate::project::ProjectFile;

lazy_static! {
    static ref TEAM_REGEX: Regex =
        Regex::new(r#"^(?:#|//|<!--|<%#)\s*(?:@?team:?\s*)(.*?)\s*(?:-->|%>)?$"#).expect("error compiling regular expression");
}

pub(crate) fn build_project_file(path: &PathBuf) -> ProjectFile {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(_) => {
            return ProjectFile {
                path: path.clone(),
                owner: None,
            };
        }
    };

    let mut reader = BufReader::new(file);
    let mut first_line = String::with_capacity(256);

    match reader.read_line(&mut first_line) {
        Ok(0) | Err(_) => {
            return ProjectFile {
                path: path.clone(),
                owner: None,
            };
        }
        Ok(_) => {}
    }

    // read_line includes the newline, but .lines() doesn't, so we need to trim
    let first_line = first_line.trim_end();

    let owner = TEAM_REGEX
        .captures(first_line)
        .and_then(|cap| cap.get(1))
        .map(|m| m.as_str().to_string());

    ProjectFile { path: path.clone(), owner }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    type FirstLine = &'static str;
    type Owner = &'static str;

    #[test]
    fn test_team_regex() {
        let mut map: HashMap<FirstLine, Owner> = HashMap::new();
        map.insert("// @team Foo", "Foo");
        map.insert("// @team Foo Bar", "Foo Bar");
        map.insert("// @team Zoo", "Zoo");
        map.insert("// @team: Zoo Foo", "Zoo Foo");
        map.insert("# @team: Bap", "Bap");
        map.insert("# @team: Bap Hap", "Bap Hap");
        map.insert("<!-- @team: Zoink -->", "Zoink");
        map.insert("<!-- @team: Zoink Err -->", "Zoink Err");
        map.insert("<%# @team: Zap %>", "Zap");
        map.insert("<%# @team: Zap Zip%>", "Zap Zip");
        map.insert("<!-- @team Blast -->", "Blast");
        map.insert("<!-- @team Blast Off -->", "Blast Off");

        // New team: format (without @ symbol)
        map.insert("# team: MyTeam", "MyTeam");
        map.insert("// team: MyTeam", "MyTeam");
        map.insert("<!-- team: MyTeam -->", "MyTeam");
        map.insert("<%# team: MyTeam %>", "MyTeam");

        for (key, value) in map {
            let owner = TEAM_REGEX.captures(key).and_then(|cap| cap.get(1)).map(|m| m.as_str());
            assert_eq!(owner, Some(value));
        }
    }
}
