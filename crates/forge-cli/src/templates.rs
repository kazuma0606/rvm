use std::fs;
use std::io;
use std::path::Path;

pub struct Template {
    pub name: &'static str,
    /// forge run に渡すエントリポイント（lib テンプレートは src/lib.forge など）
    pub entry: &'static str,
    pub files: &'static [(&'static str, &'static str)],
}

const SCRIPT_FILES: &[(&str, &str)] = &[
    (".gitignore", include_str!("../templates/script/.gitignore")),
    ("forge.toml", include_str!("../templates/script/forge.toml")),
    (
        "src/main.forge",
        include_str!("../templates/script/src/main.forge"),
    ),
];

const CLI_FILES: &[(&str, &str)] = &[
    (".gitignore", include_str!("../templates/cli/.gitignore")),
    ("forge.toml", include_str!("../templates/cli/forge.toml")),
    (
        "src/main.forge",
        include_str!("../templates/cli/src/main.forge"),
    ),
];

const LIB_FILES: &[(&str, &str)] = &[
    (".gitignore", include_str!("../templates/lib/.gitignore")),
    ("forge.toml", include_str!("../templates/lib/forge.toml")),
    (
        "src/lib.forge",
        include_str!("../templates/lib/src/lib.forge"),
    ),
];

const DATA_FILES: &[(&str, &str)] = &[
    (".gitignore", include_str!("../templates/data/.gitignore")),
    ("forge.toml", include_str!("../templates/data/forge.toml")),
    (
        "src/main.forge",
        include_str!("../templates/data/src/main.forge"),
    ),
];

const ANVIL_FILES: &[(&str, &str)] = &[
    (".gitignore", include_str!("../templates/anvil/.gitignore")),
    ("forge.toml", include_str!("../templates/anvil/forge.toml")),
    (
        "src/main.forge",
        include_str!("../templates/anvil/src/main.forge"),
    ),
    (
        "src/request.forge",
        include_str!("../templates/anvil/src/request.forge"),
    ),
    (
        "src/response.forge",
        include_str!("../templates/anvil/src/response.forge"),
    ),
    (
        "src/router.forge",
        include_str!("../templates/anvil/src/router.forge"),
    ),
    (
        "src/middleware.forge",
        include_str!("../templates/anvil/src/middleware.forge"),
    ),
    (
        "src/cors.forge",
        include_str!("../templates/anvil/src/cors.forge"),
    ),
    (
        "src/auth.forge",
        include_str!("../templates/anvil/src/auth.forge"),
    ),
    (
        "settings.json.example",
        include_str!("../templates/anvil/settings.json.example"),
    ),
];

const CLEAN_ARCH_FILES: &[(&str, &str)] = &[
    (
        ".gitignore",
        include_str!("../templates/clean-arch/.gitignore"),
    ),
    (
        "forge.toml",
        include_str!("../templates/clean-arch/forge.toml"),
    ),
    (
        "src/main.forge",
        include_str!("../templates/clean-arch/src/main.forge"),
    ),
    (
        "src/domain/mod.forge",
        include_str!("../templates/clean-arch/src/domain/mod.forge"),
    ),
    (
        "src/domain/user.forge",
        include_str!("../templates/clean-arch/src/domain/user.forge"),
    ),
    (
        "src/usecase/mod.forge",
        include_str!("../templates/clean-arch/src/usecase/mod.forge"),
    ),
    (
        "src/usecase/register_user_usecase.forge",
        include_str!("../templates/clean-arch/src/usecase/register_user_usecase.forge"),
    ),
    (
        "src/interface/mod.forge",
        include_str!("../templates/clean-arch/src/interface/mod.forge"),
    ),
    (
        "src/interface/user_handler.forge",
        include_str!("../templates/clean-arch/src/interface/user_handler.forge"),
    ),
    (
        "src/infrastructure/mod.forge",
        include_str!("../templates/clean-arch/src/infrastructure/mod.forge"),
    ),
    (
        "src/infrastructure/postgres_user_repository.forge",
        include_str!("../templates/clean-arch/src/infrastructure/postgres_user_repository.forge"),
    ),
    (
        "src/infrastructure/smtp_email_service.forge",
        include_str!("../templates/clean-arch/src/infrastructure/smtp_email_service.forge"),
    ),
    (
        "tests/register_user_test.forge",
        include_str!("../templates/clean-arch/tests/register_user_test.forge"),
    ),
];

const ANVIL_CLEAN_FILES: &[(&str, &str)] = &[
    (
        ".gitignore",
        include_str!("../templates/anvil-clean/.gitignore"),
    ),
    (
        "forge.toml",
        include_str!("../templates/anvil-clean/forge.toml"),
    ),
    (
        "src/main.forge",
        include_str!("../templates/anvil-clean/src/main.forge"),
    ),
    (
        "src/domain/mod.forge",
        include_str!("../templates/anvil-clean/src/domain/mod.forge"),
    ),
    (
        "src/domain/user.forge",
        include_str!("../templates/anvil-clean/src/domain/user.forge"),
    ),
    (
        "src/usecase/mod.forge",
        include_str!("../templates/anvil-clean/src/usecase/mod.forge"),
    ),
    (
        "src/usecase/register_user_usecase.forge",
        include_str!("../templates/anvil-clean/src/usecase/register_user_usecase.forge"),
    ),
    (
        "src/interface/mod.forge",
        include_str!("../templates/anvil-clean/src/interface/mod.forge"),
    ),
    (
        "src/interface/user_handler.forge",
        include_str!("../templates/anvil-clean/src/interface/user_handler.forge"),
    ),
    (
        "src/infrastructure/mod.forge",
        include_str!("../templates/anvil-clean/src/infrastructure/mod.forge"),
    ),
    (
        "src/infrastructure/postgres_user_repository.forge",
        include_str!("../templates/anvil-clean/src/infrastructure/postgres_user_repository.forge"),
    ),
];

const SCRIPT_TEMPLATE: Template = Template {
    name: "script",
    entry: "src/main.forge",
    files: SCRIPT_FILES,
};

const CLI_TEMPLATE: Template = Template {
    name: "cli",
    entry: "src/main.forge",
    files: CLI_FILES,
};

const LIB_TEMPLATE: Template = Template {
    name: "lib",
    entry: "src/lib.forge",
    files: LIB_FILES,
};

const DATA_TEMPLATE: Template = Template {
    name: "data",
    entry: "src/main.forge",
    files: DATA_FILES,
};

const ANVIL_TEMPLATE: Template = Template {
    name: "anvil",
    entry: "src/main.forge",
    files: ANVIL_FILES,
};

const CLEAN_ARCH_TEMPLATE: Template = Template {
    name: "clean-arch",
    entry: "src/main.forge",
    files: CLEAN_ARCH_FILES,
};

const ANVIL_CLEAN_TEMPLATE: Template = Template {
    name: "anvil-clean",
    entry: "src/main.forge",
    files: ANVIL_CLEAN_FILES,
};

const BLOOM_FILES: &[(&str, &str)] = &[
    (".gitignore", include_str!("../templates/bloom/.gitignore")),
    ("forge.toml", include_str!("../templates/bloom/forge.toml")),
    (
        "src/app/layout.bloom",
        include_str!("../templates/bloom/src/app/layout.bloom"),
    ),
    (
        "src/app/page.bloom",
        include_str!("../templates/bloom/src/app/page.bloom"),
    ),
    (
        "src/components/counter.bloom",
        include_str!("../templates/bloom/src/components/counter.bloom"),
    ),
    (
        "src/stores/counter.flux.bloom",
        include_str!("../templates/bloom/src/stores/counter.flux.bloom"),
    ),
    (
        "src/lib/utils.forge",
        include_str!("../templates/bloom/src/lib/utils.forge"),
    ),
    ("public/favicon.ico", ""),
];

const BLOOM_TEMPLATE: Template = Template {
    name: "bloom",
    entry: "src/app/page.bloom",
    files: BLOOM_FILES,
};

const TEMPLATES: &[&Template] = &[
    &SCRIPT_TEMPLATE,
    &CLI_TEMPLATE,
    &LIB_TEMPLATE,
    &DATA_TEMPLATE,
    &ANVIL_TEMPLATE,
    &CLEAN_ARCH_TEMPLATE,
    &ANVIL_CLEAN_TEMPLATE,
    &BLOOM_TEMPLATE,
];

pub fn render(content: &str, vars: &[(&str, &str)]) -> String {
    let mut rendered = content.to_string();
    for (name, value) in vars {
        let needle = format!("{{{{{}}}}}", name);
        rendered = rendered.replace(&needle, value);
    }
    rendered
}

pub fn write_template(
    dest: &Path,
    template: &Template,
    vars: &[(&str, &str)],
) -> Result<(), io::Error> {
    if dest.exists() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!("対象ディレクトリは既に存在します: {}", dest.display()),
        ));
    }

    fs::create_dir_all(dest)?;
    for (relative_path, content) in template.files {
        let file_path = dest.join(relative_path);
        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(file_path, render(content, vars))?;
    }

    Ok(())
}

pub fn get_template(name: &str) -> Option<&'static Template> {
    TEMPLATES
        .iter()
        .find(|template| template.name == name)
        .copied()
}

pub fn available_template_names() -> Vec<&'static str> {
    TEMPLATES.iter().map(|template| template.name).collect()
}

#[cfg(test)]
mod tests {
    use super::{get_template, render, write_template};
    use std::fs;

    #[test]
    fn render_replaces_all_variables() {
        let rendered = render(
            "name={{name}}, version={{version}}, forge={{forge_version}}",
            &[
                ("name", "demo"),
                ("version", "0.1.0"),
                ("forge_version", "0.1.0"),
            ],
        );
        assert_eq!(rendered, "name=demo, version=0.1.0, forge=0.1.0");
    }

    #[test]
    fn write_template_creates_nested_files() {
        let mut dest = std::env::temp_dir();
        dest.push(format!(
            "forge_template_test_{}_{}",
            std::process::id(),
            unique_suffix()
        ));

        let template = get_template("script").expect("script template");
        write_template(
            &dest,
            template,
            &[
                ("name", "demo"),
                ("version", "0.1.0"),
                ("forge_version", "0.1.0"),
            ],
        )
        .expect("write template");

        assert!(dest.join("forge.toml").exists());
        assert!(dest.join("src/main.forge").exists());

        let _ = fs::remove_dir_all(dest);
    }

    fn unique_suffix() -> u64 {
        static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    }
}
