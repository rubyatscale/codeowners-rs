use serde::Deserialize;
use std::{fs::File, path::Path};

#[derive(Deserialize, Debug, Clone)]
pub struct Config {
    pub owned_globs: Vec<String>,

    #[serde(default = "ruby_package_paths")]
    pub ruby_package_paths: Vec<String>,

    #[serde(alias = "js_package_paths", default = "javascript_package_paths")]
    pub javascript_package_paths: Vec<String>,

    #[serde(default = "team_file_glob")]
    pub team_file_glob: Vec<String>,

    #[serde(default = "unowned_globs")]
    pub unowned_globs: Vec<String>,

    #[serde(alias = "unbuilt_gems_path", default = "vendored_gems_path")]
    pub vendored_gems_path: String,

    #[serde(default = "default_cache_directory")]
    pub cache_directory: String,

    #[serde(default = "default_ignore_dirs")]
    pub ignore_dirs: Vec<String>,

    #[serde(default = "default_executable_name")]
    pub executable_name: String,
}

#[allow(dead_code)]
#[derive(Deserialize, Debug)]
pub struct RubyPackageConfig {
    #[serde(alias = "pack_paths")]
    pub ruby_package_paths: Vec<String>,
}

fn ruby_package_paths() -> Vec<String> {
    vec!["packs/**/*".to_owned(), "components/**".to_owned()]
}

fn team_file_glob() -> Vec<String> {
    vec!["config/teams/**/*.yml".to_owned()]
}

fn default_cache_directory() -> String {
    String::from("tmp/cache/codeowners")
}

fn javascript_package_paths() -> Vec<String> {
    vec!["frontend/**/*".to_owned()]
}

fn unowned_globs() -> Vec<String> {
    vec![
        "frontend/**/node_modules/**/*".to_owned(),
        "frontend/**/__generated__/**/*".to_owned(),
    ]
}

fn vendored_gems_path() -> String {
    "vendored/".to_string()
}

fn default_executable_name() -> String {
    "codeowners".to_string()
}

fn default_ignore_dirs() -> Vec<String> {
    vec![
        ".cursor".to_owned(),
        ".git".to_owned(),
        ".idea".to_owned(),
        ".vscode".to_owned(),
        ".yarn".to_owned(),
        "ar_doc".to_owned(),
        "db".to_owned(),
        "helm".to_owned(),
        "log".to_owned(),
        "node_modules".to_owned(),
        "sorbet".to_owned(),
        "tmp".to_owned(),
    ]
}

impl Config {
    pub fn load_from_path(path: &Path) -> std::result::Result<Self, String> {
        let file = File::open(path).map_err(|e| format!("Can't open config file: {} ({})", path.to_string_lossy(), e))?;
        serde_yaml::from_reader(file).map_err(|e| format!("Can't parse config file: {} ({})", path.to_string_lossy(), e))
    }
}

#[cfg(test)]
mod tests {
    use std::{
        error::Error,
        fs::{self, File},
    };

    use indoc::indoc;
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn test_parse_config() -> Result<(), Box<dyn Error>> {
        let temp_dir = tempdir()?;
        let config_path = temp_dir.path().join("config.yml");
        let config_str = indoc! {"
            ---
            owned_globs:
              - \"{app,components,config,frontend,lib,packs,spec}/**/*.{rb,rake,js,jsx,ts,tsx,json,yml}\"
        "};
        fs::write(&config_path, config_str)?;
        let config_file = File::open(&config_path)?;
        let config: Config = serde_yaml::from_reader(config_file)?;
        assert_eq!(
            config.owned_globs,
            vec!["{app,components,config,frontend,lib,packs,spec}/**/*.{rb,rake,js,jsx,ts,tsx,json,yml}"]
        );
        assert_eq!(config.ruby_package_paths, vec!["packs/**/*", "components/**"]);
        assert_eq!(config.javascript_package_paths, vec!["frontend/**/*"]);
        assert_eq!(config.team_file_glob, vec!["config/teams/**/*.yml"]);
        assert_eq!(
            config.unowned_globs,
            vec!["frontend/**/node_modules/**/*", "frontend/**/__generated__/**/*"]
        );
        assert_eq!(config.vendored_gems_path, "vendored/");
        Ok(())
    }
}
