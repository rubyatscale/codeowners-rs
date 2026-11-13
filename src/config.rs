use serde::Deserialize;
use std::path::{Path, PathBuf};

/// `Config.path` for a config not read with `load_from_path`: the usual location, relative to the project root.
pub(crate) const DEFAULT_PATH: &str = "config/code_ownership.yml";

#[derive(Deserialize, Debug, Clone)]
pub struct Config {
    #[serde(deserialize_with = "crate::yaml::strings_or_nulls")]
    pub owned_globs: Vec<String>,

    #[serde(default = "ruby_package_paths", deserialize_with = "crate::yaml::strings_or_nulls")]
    pub ruby_package_paths: Vec<String>,

    #[serde(
        alias = "js_package_paths",
        default = "javascript_package_paths",
        deserialize_with = "crate::yaml::strings_or_nulls"
    )]
    pub javascript_package_paths: Vec<String>,

    #[serde(default = "team_file_glob", deserialize_with = "crate::yaml::strings_or_nulls")]
    pub team_file_glob: Vec<String>,

    #[serde(default = "unowned_globs", deserialize_with = "crate::yaml::strings_or_nulls")]
    pub unowned_globs: Vec<String>,

    #[serde(
        alias = "unbuilt_gems_path",
        default = "vendored_gems_path",
        deserialize_with = "crate::yaml::string_or_null"
    )]
    pub vendored_gems_path: String,

    #[serde(default = "default_cache_directory", deserialize_with = "crate::yaml::string_or_null")]
    pub cache_directory: String,

    #[serde(default = "default_ignore_dirs", deserialize_with = "crate::yaml::strings_or_nulls")]
    pub ignore_dirs: Vec<String>,

    #[serde(default = "default_executable_name", deserialize_with = "crate::yaml::string_or_null")]
    pub executable_name: String,

    #[serde(default = "default_codeowners_path", deserialize_with = "crate::yaml::string_or_null")]
    pub codeowners_path: String,

    #[serde(default, deserialize_with = "crate::yaml::strings_or_nulls")]
    pub optionally_owned_globs: Vec<String>,

    #[serde(default = "default_yaml_max_nodes")]
    pub yaml_max_nodes: usize,

    /// The file [`Config::load_from_path`] read this config from, or `config/code_ownership.yml` for a config
    /// deserialized another way. The hint on a file over `yaml_max_nodes` names it.
    #[serde(skip_deserializing, default = "default_path")]
    pub path: PathBuf,
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
    "codeowners generate".to_string()
}

fn default_ignore_dirs() -> Vec<String> {
    vec![
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

fn default_codeowners_path() -> String {
    ".github".to_string()
}

fn default_yaml_max_nodes() -> usize {
    crate::yaml::DEFAULT_MAX_NODES
}

fn default_path() -> PathBuf {
    PathBuf::from(DEFAULT_PATH)
}

impl Config {
    pub fn load_from_path(path: &Path) -> std::result::Result<Self, String> {
        let bytes = std::fs::read(path).map_err(|e| format!("Can't open config file: {} ({})", path.to_string_lossy(), e))?;
        // The config can't raise its own limit, so its lists can hold about 20,000 entries in all.
        let config: Self = crate::yaml::from_slice(&bytes, crate::yaml::DEFAULT_MAX_NODES)
            .map_err(|e| format!("Can't parse config file: {} ({})", path.to_string_lossy(), e))?;
        Ok(Self {
            path: path.to_path_buf(),
            ..config
        })
    }

    /// `yaml_max_nodes`, with this config's file relative to `project_root` when it is inside it.
    pub(crate) fn node_limit(&self, project_root: &Path) -> crate::yaml::NodeLimit {
        crate::yaml::NodeLimit {
            max_nodes: self.yaml_max_nodes,
            config_path: crate::path_utils::relative_to_buf(project_root, &self.path),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{error::Error, fs};

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
        let config = Config::load_from_path(&config_path)?;
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
        assert_eq!(config.executable_name, "codeowners generate");
        assert!(config.optionally_owned_globs.is_empty());
        assert_eq!(config.yaml_max_nodes, 20_000);
        Ok(())
    }

    #[test]
    fn test_parse_config_with_custom_yaml_max_nodes() -> Result<(), Box<dyn Error>> {
        let temp_dir = tempdir()?;
        let config_path = temp_dir.path().join("config.yml");
        let config_str = indoc! {"
            ---
            owned_globs:
              - \"**/*.rb\"
            yaml_max_nodes: 200000
        "};
        fs::write(&config_path, config_str)?;
        let config = Config::load_from_path(&config_path)?;
        assert_eq!(config.yaml_max_nodes, 200_000);
        Ok(())
    }

    #[test]
    fn test_node_limit_names_the_config_file() -> Result<(), Box<dyn Error>> {
        let temp_dir = tempdir()?;
        fs::create_dir(temp_dir.path().join("config"))?;
        let config_path = temp_dir.path().join("config/alt.yml");
        let config_str = indoc! {"
            ---
            owned_globs:
              - \"**/*.rb\"
            yaml_max_nodes: 30000
        "};
        fs::write(&config_path, config_str)?;
        let config = Config::load_from_path(&config_path)?;
        assert_eq!(
            config.node_limit(temp_dir.path()),
            crate::yaml::NodeLimit {
                max_nodes: 30_000,
                config_path: PathBuf::from("config/alt.yml"),
            }
        );
        // A config file outside the project root is named in full, and a config not read from a file names the default.
        assert_eq!(config.node_limit(Path::new("/elsewhere")).config_path, config_path);
        let unread: Config = crate::yaml::from_slice(b"owned_globs: []\n", crate::yaml::DEFAULT_MAX_NODES)?;
        assert_eq!(
            unread.node_limit(temp_dir.path()).config_path,
            Path::new("config/code_ownership.yml")
        );
        Ok(())
    }

    #[test]
    fn test_parse_config_with_custom_executable_name() -> Result<(), Box<dyn Error>> {
        let temp_dir = tempdir()?;
        let config_path = temp_dir.path().join("config.yml");
        let config_str = indoc! {"
            ---
            owned_globs:
              - \"**/*.rb\"
            executable_name: my-custom-codeowners
        "};
        fs::write(&config_path, config_str)?;
        let config = Config::load_from_path(&config_path)?;
        assert_eq!(config.executable_name, "my-custom-codeowners");
        Ok(())
    }

    #[test]
    fn test_executable_name_defaults_when_not_specified() -> Result<(), Box<dyn Error>> {
        let temp_dir = tempdir()?;
        let config_path = temp_dir.path().join("config.yml");
        let config_str = indoc! {"
            ---
            owned_globs:
              - \"**/*.rb\"
        "};
        fs::write(&config_path, config_str)?;
        let config = Config::load_from_path(&config_path)?;
        assert_eq!(config.executable_name, "codeowners generate");
        Ok(())
    }

    #[test]
    fn test_parse_config_with_optionally_owned_globs() -> Result<(), Box<dyn Error>> {
        let temp_dir = tempdir()?;
        let config_path = temp_dir.path().join("config.yml");
        let config_str = indoc! {"
            ---
            owned_globs:
              - \"**/*.rb\"
            optionally_owned_globs:
              - \"**/deprecated/**/*\"
        "};
        fs::write(&config_path, config_str)?;
        let config = Config::load_from_path(&config_path)?;
        assert_eq!(config.optionally_owned_globs, vec!["**/deprecated/**/*"]);
        Ok(())
    }

    #[test]
    fn test_codeowners_path_defaults_when_not_specified() -> Result<(), Box<dyn Error>> {
        let temp_dir = tempdir()?;
        let config_path = temp_dir.path().join("config.yml");
        let config_str = indoc! {"
            ---
            owned_globs:
              - \"**/*.rb\"
        "};
        fs::write(&config_path, config_str)?;
        let config = Config::load_from_path(&config_path)?;
        assert_eq!(config.codeowners_path, ".github");
        Ok(())
    }

    #[test]
    fn test_codeowners_path_can_be_customized() -> Result<(), Box<dyn Error>> {
        let temp_dir = tempdir()?;
        let config_path = temp_dir.path().join("config.yml");
        let config_str = indoc! {"
            ---
            owned_globs:
              - \"**/*.rb\"
            codeowners_path: docs
        "};
        fs::write(&config_path, config_str)?;
        let config = Config::load_from_path(&config_path)?;
        assert_eq!(config.codeowners_path, "docs");
        Ok(())
    }

    #[test]
    fn test_parse_config_with_empty_values() -> Result<(), Box<dyn Error>> {
        let temp_dir = tempdir()?;
        let config_path = temp_dir.path().join("config.yml");
        let config_str = indoc! {"
            ---
            owned_globs:
              - \"**/*.rb\"
              -
            ruby_package_paths:
              -
            javascript_package_paths:
              -
            team_file_glob:
              -
            unowned_globs:
              -
            ignore_dirs:
              -
            optionally_owned_globs:
              -
            vendored_gems_path:
            cache_directory:
            executable_name:
            codeowners_path:
        "};
        fs::write(&config_path, config_str)?;
        let config = Config::load_from_path(&config_path)?;
        assert_eq!(config.owned_globs, vec!["**/*.rb", ""]);
        for globs in [
            &config.ruby_package_paths,
            &config.javascript_package_paths,
            &config.team_file_glob,
            &config.unowned_globs,
            &config.ignore_dirs,
            &config.optionally_owned_globs,
        ] {
            assert_eq!(globs, &[""]);
        }
        for value in [
            &config.vendored_gems_path,
            &config.cache_directory,
            &config.executable_name,
            &config.codeowners_path,
        ] {
            assert_eq!(value, "");
        }
        Ok(())
    }

    #[test]
    fn test_load_from_path_reports_parse_error_on_one_line() -> Result<(), Box<dyn Error>> {
        let temp_dir = tempdir()?;
        let config_path = temp_dir.path().join("config.yml");
        fs::write(&config_path, "owned_globs: {app: true}\n")?;
        let error = Config::load_from_path(&config_path).unwrap_err();
        assert!(error.starts_with("Can't parse config file: "), "{error}");
        assert!(error.contains("line 1"), "{error}");
        assert!(!error.contains('\n'), "{error}");
        Ok(())
    }
}
