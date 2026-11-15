//! skillstar package-skills: preview a project's package.json skills field.
//!
//! The command never installs, never uninstalls, and never copies skill files
//! into node_modules. Uninstall stays a plan that requires both a recorded
//! install and an explicit confirmation; this command passes neither.

use std::path::PathBuf;

use ss_skills::projects::preview_package_skills;

pub fn cmd_package_skills(project: Option<&str>, json: bool) {
    let dir = match project
        .map(PathBuf::from)
        .or_else(|| std::env::current_dir().ok())
    {
        Some(dir) => dir,
        None => {
            eprintln!("package-skills: could not read the current directory");
            std::process::exit(1);
        }
    };
    let installed = ss_skills::installer::installed_names();
    let preview = preview_package_skills(&dir, &installed, &[], false);
    if json {
        match serde_json::to_string_pretty(&preview) {
            Ok(text) => println!("{text}"),
            Err(error) => {
                eprintln!("package-skills: {error}");
                std::process::exit(1);
            }
        }
    } else {
        print_preview(&preview);
    }
    if !preview.errors.is_empty() {
        std::process::exit(1);
    }
}

fn print_preview(preview: &ss_skills::projects::PackageSkillsPreview) {
    println!("package.json skills preview — writes nothing, including node_modules");
    println!("project: {}", preview.project);
    if preview.remote.is_empty() {
        println!("remote: none");
    } else {
        println!("remote:");
        for request in &preview.remote {
            let skills = if request.skills.is_empty() {
                "all (not fetched)".to_string()
            } else {
                request.skills.join(", ")
            };
            println!(
                "  {}  ref={}  subpath={}  skills={skills}",
                request.source,
                request.git_ref.as_deref().unwrap_or("-"),
                request.subpath.as_deref().unwrap_or("-")
            );
        }
    }
    if preview.npm.is_empty() {
        println!("npm: none");
    } else {
        println!("npm:");
        for request in &preview.npm {
            let state = match &request.resolved {
                Some(path) => format!("read {path}; shipped {}", request.shipped.join(", ")),
                None => "not installed; will not copy into node_modules".to_string(),
            };
            println!("  {}  {state}", request.package);
        }
    }
    if preview.errors.is_empty() {
        println!("errors: none");
    } else {
        println!("errors:");
        for error in &preview.errors {
            println!("  {error}");
        }
    }
    println!(
        "already installed: {}",
        display_names(&preview.already_installed)
    );
    println!("missing named: {}", display_names(&preview.missing_named));
    println!("uninstall: {}", preview.uninstall.summary());
}

fn display_names(names: &[String]) -> String {
    if names.is_empty() {
        "none".to_string()
    } else {
        names.join(", ")
    }
}
