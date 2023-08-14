use std::path::Path;

use crate::project::{Package, PackageType, Project, ProjectFile, Team, VendoredGem};

use super::Ownership;
use pretty_assertions::assert_eq;

fn build_payroll_team() -> Team {
    Team {
        path: Path::new("config/teams/payroll.yml").to_owned(),
        name: "Payroll".to_owned(),
        github_team: "@Payroll-Eng".to_owned(),
        owned_globs: vec![],
        avoid_ownership: false,
        owned_gems: vec![],
    }
}

fn build_payroll_team_with_owned_gem() -> Team {
    Team {
        path: Path::new("config/teams/payroll.yml").to_owned(),
        name: "Payroll".to_owned(),
        github_team: "@Payroll-Eng".to_owned(),
        owned_globs: vec![],
        avoid_ownership: false,
        owned_gems: vec!["payroll_calculator".to_owned()],
    }
}

fn build_annotated_file() -> ProjectFile {
    ProjectFile {
        owner: Some("Payroll".to_owned()),
        path: Path::new("packs/payroll/services/runner.rb").to_owned(),
    }
}

fn build_unannotated_file() -> ProjectFile {
    ProjectFile {
        owner: None,
        path: Path::new("packs/payroll/services/runner_helper.rb").to_owned(),
    }
}

fn build_project_with_annotated_file() -> Project {
    Project {
        base_path: Path::new("").to_owned(),
        files: vec![build_annotated_file(), build_unannotated_file()],
        packages: vec![],
        teams: vec![build_payroll_team()],
        vendored_gems: vec![],
        codeowners_file: "".to_owned(),
        directory_codeowner_files: vec![],
    }
}

fn build_payroll_team_with_owned_glob() -> Team {
    Team {
        path: Path::new("config/teams/payroll.yml").to_owned(),
        name: "Payroll".to_owned(),
        github_team: "@Payroll-Eng".to_owned(),
        owned_globs: vec!["packs/payroll/**".to_owned()],
        avoid_ownership: false,
        owned_gems: vec![],
    }
}

fn build_project_with_team_specific_owned_globs() -> Project {
    Project {
        base_path: Path::new("").to_owned(),
        files: vec![build_unannotated_file()],
        packages: vec![],
        teams: vec![build_payroll_team_with_owned_glob()],
        vendored_gems: vec![],
        codeowners_file: "".to_owned(),
        directory_codeowner_files: vec![],
    }
}

fn build_project_with_packages() -> Project {
    Project {
        base_path: Path::new("").to_owned(),
        files: vec![build_unannotated_file()],
        packages: vec![
            Package {
                path: Path::new("packs/payroll_package/package.yml").to_owned(),
                package_type: PackageType::Ruby,
                owner: "Payroll".to_owned(),
            },
            Package {
                path: Path::new("frontend/payroll_flow/package.json").to_owned(),
                package_type: PackageType::Javascript,
                owner: "Payroll".to_owned(),
            },
        ],
        teams: vec![build_payroll_team()],
        vendored_gems: vec![],
        codeowners_file: "".to_owned(),
        directory_codeowner_files: vec![],
    }
}

fn build_project_with_team_owned_gems() -> Project {
    Project {
        base_path: Path::new("").to_owned(),
        files: vec![build_unannotated_file()],
        packages: vec![],
        teams: vec![build_payroll_team_with_owned_gem()],
        vendored_gems: vec![VendoredGem {
            path: Path::new("components/payroll_calculator").to_owned(),
            name: "payroll_calculator".to_owned(),
        }],
        codeowners_file: "".to_owned(),
        directory_codeowner_files: vec![],
    }
}

#[test]
fn test_annotations_at_the_top_of_file() {
    let ownership = Ownership::build(build_project_with_annotated_file());

    assert_eq!(
        ownership.generate_file(),
        with_disclaimer(vec![
            "# Annotations at the top of file",
            "/packs/payroll/services/runner.rb @Payroll-Eng",
            "",
            "# Team-specific owned globs",
            "",
            "# Owner metadata key in package.yml",
            "",
            "# Owner metadata key in package.json",
            "",
            "# Team YML ownership",
            "/config/teams/payroll.yml @Payroll-Eng",
            "",
            "# Team owned gems",
            "",
            "# Owner in .codeowner",
            "",
        ])
        .join("\n")
    )
}

#[test]
fn test_team_specific_owned_globs() {
    let ownership = Ownership::build(build_project_with_team_specific_owned_globs());

    assert_eq!(
        ownership.generate_file(),
        with_disclaimer(vec![
            "# Annotations at the top of file",
            "",
            "# Team-specific owned globs",
            "/packs/payroll/** @Payroll-Eng",
            "",
            "# Owner metadata key in package.yml",
            "",
            "# Owner metadata key in package.json",
            "",
            "# Team YML ownership",
            "/config/teams/payroll.yml @Payroll-Eng",
            "",
            "# Team owned gems",
            "",
            "# Owner in .codeowner",
            "",
        ])
        .join("\n")
    )
}

#[test]
fn test_owner_metadata_in_package() {
    let ownership = Ownership::build(build_project_with_packages());

    assert_eq!(
        ownership.generate_file(),
        with_disclaimer(vec![
            "# Annotations at the top of file",
            "",
            "# Team-specific owned globs",
            "",
            "# Owner metadata key in package.yml",
            "/packs/payroll_package/**/** @Payroll-Eng",
            "",
            "# Owner metadata key in package.json",
            "/frontend/payroll_flow/**/** @Payroll-Eng",
            "",
            "# Team YML ownership",
            "/config/teams/payroll.yml @Payroll-Eng",
            "",
            "# Team owned gems",
            "",
            "# Owner in .codeowner",
            "",
        ])
        .join("\n")
    )
}

#[test]
fn test_team_owned_gems() {
    let ownership = Ownership::build(build_project_with_team_owned_gems());

    assert_eq!(
        ownership.generate_file(),
        with_disclaimer(vec![
            "# Annotations at the top of file",
            "",
            "# Team-specific owned globs",
            "",
            "# Owner metadata key in package.yml",
            "",
            "# Owner metadata key in package.json",
            "",
            "# Team YML ownership",
            "/config/teams/payroll.yml @Payroll-Eng",
            "",
            "# Team owned gems",
            "/components/payroll_calculator/**/** @Payroll-Eng",
            "",
            "# Owner in .codeowner",
            "",
        ])
        .join("\n")
    )
}

fn with_disclaimer(lines: Vec<&str>) -> Vec<String> {
    let mut buffer: Vec<String> = Vec::new();
    let mut disclaimer = crate::ownership::file_generator::FileGenerator::disclaimer();

    buffer.append(&mut disclaimer);
    buffer.append(&mut lines.iter().map(|l| l.to_string()).collect());

    buffer
}
