use crate::templates::{self, write_template};
use std::env;
use std::io::{self, ErrorKind, Write};
use std::path::Path;
use std::process::Command;

pub fn run(name: Option<&str>, template: &str) -> Result<(), io::Error> {
    run_with_options(name, template, false)
}

pub fn run_with_options(name: Option<&str>, template: &str, git: bool) -> Result<(), io::Error> {
    let project_name = resolve_project_name(name)?;
    let template = templates::get_template(template).ok_or_else(|| {
        io::Error::new(
            ErrorKind::InvalidInput,
            format!(
                "不明なテンプレートです: {} (利用可能: {})",
                template,
                templates::available_template_names().join(", ")
            ),
        )
    })?;

    let dest = env::current_dir()?.join(&project_name);
    let version = "0.1.0".to_string();
    let forge_version = env!("CARGO_PKG_VERSION").to_string();
    let vars = [
        ("name", project_name.as_str()),
        ("version", version.as_str()),
        ("forge_version", forge_version.as_str()),
    ];

    write_template(&dest, template, &vars)?;

    if git {
        init_git(&dest)?;
    }

    println!("Created {}/", project_name);
    println!("cd {} && forge run {}", project_name, template.entry);
    Ok(())
}

fn resolve_project_name(name: Option<&str>) -> Result<String, io::Error> {
    match name.map(str::trim).filter(|name| !name.is_empty()) {
        Some(name) => Ok(name.to_string()),
        None => prompt_project_name(),
    }
}

fn prompt_project_name() -> Result<String, io::Error> {
    let mut stdout = io::stdout();
    write!(stdout, "project name: ")?;
    stdout.flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let name = input.trim();
    if name.is_empty() {
        return Err(io::Error::new(
            ErrorKind::InvalidInput,
            "プロジェクト名が空です",
        ));
    }
    Ok(name.to_string())
}

fn init_git(dest: &Path) -> Result<(), io::Error> {
    let status = Command::new("git").arg("init").current_dir(dest).status()?;
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other("git init に失敗しました"))
    }
}
