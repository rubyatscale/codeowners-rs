use std::path::Path;

use serde::Deserialize;
use wax::Glob;

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

impl<'a> Config {
    pub fn compile(&'a self) -> CompiledConfig<'a> {
        CompiledConfig {
            owned_file_globs: Self::compile_glob(&self.owned_globs),
            ruby_package_paths: Self::compile_glob(&self.ruby_package_paths),
            javascript_package_paths: Self::compile_glob(&self.javascript_package_paths),
            team_file_glob: Self::compile_glob(&self.team_file_glob),
            unowned_globs: Self::compile_glob(&self.unowned_globs),
            vendored_gems_path: Path::new(&self.vendored_gems_path),
        }
    }

    fn compile_glob(globs: &[String]) -> Vec<Glob> {
        globs.iter().map(|g| Glob::new(g).unwrap()).collect()
    }
}

pub struct CompiledConfig<'a> {
    pub owned_file_globs: Vec<Glob<'a>>,
    pub ruby_package_paths: Vec<Glob<'a>>,
    pub javascript_package_paths: Vec<Glob<'a>>,
    pub team_file_glob: Vec<Glob<'a>>,
    pub unowned_globs: Vec<Glob<'a>>,
    pub vendored_gems_path: &'a Path,
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
    vec![]
}

fn vendored_dependencies() -> String {
    "components".to_owned()
}
