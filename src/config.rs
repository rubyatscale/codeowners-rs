use serde::Deserialize;

#[derive(Deserialize, Debug)]
pub struct Config {
    #[serde(default = "owned_globs")]
    pub owned_globs: Vec<String>,

    #[serde(default = "ruby_package_paths")]
    pub ruby_package_paths: Vec<String>,

    #[serde(default = "javascript_package_paths")]
    pub javascript_package_paths: Vec<String>,

    #[serde(default = "team_file_glob")]
    pub team_file_glob: Vec<String>,

    #[serde(default = "unowned_globs")]
    pub unowned_globs: Vec<String>,

    #[serde(default = "vendored_dependencies")]
    pub vendored_gems_path: String,
}

fn ruby_package_paths() -> Vec<String> {
    vec!["packs/**".to_string()]
}

fn javascript_package_paths() -> Vec<String> {
    vec![
        "frontend/javascripts/packages/**".to_owned(),
        "frontend/storybook/**".to_owned(),
        "components/gusto-apis/**".to_owned(),
    ]
}

fn owned_globs() -> Vec<String> {
    vec!["{app,components,config,frontend,lib,packs,spec,danger,script}/**/*.{rb,arb,erb,rake,js,jsx,ts,tsx}".to_owned()]
}

fn team_file_glob() -> Vec<String> {
    vec!["config/teams/**/*.yml".to_owned()]
}

fn unowned_globs() -> Vec<String> {
    vec![
        "frontend/javascripts/**/__generated__/**/*".to_owned(),
        "frontend/javascripts/packages/graphql-subgraph-mock-server/**/*".to_owned(),
        "frontend/javascripts/vendor/**/*".to_owned(),
        "frontend/javascripts/lib/extensions/jquery/**/*".to_owned(),
        "config/environments/development_overrides.rb".to_owned(),
    ]
}

fn vendored_dependencies() -> String {
    "components".to_owned()
}
