use serde::Deserialize;

#[derive(Deserialize, Debug)]
pub struct Config {
    pub owned_globs: Vec<String>,
    pub ruby_package_paths: Vec<String>,
    pub javascript_package_paths: Vec<String>,
    pub team_file_glob: Vec<String>,
    pub unowned_globs: Vec<String>,
    pub vendored_gems_path: String,
}
