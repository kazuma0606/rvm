use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForgeToml {
    pub package: PackageSection,
    pub build: Option<BuildSection>,
    pub dependencies: BTreeMap<String, DependencyValue>,
    pub architecture: Option<ArchitectureSection>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageSection {
    pub name: String,
    pub version: String,
    pub forge: Option<String>,
    pub entry: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildSection {
    pub output: Option<String>,
    pub edition: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchitectureSection {
    pub layers: Vec<String>,
    pub naming_rules: NamingRulesMode,
    pub naming: BTreeMap<String, NamingRule>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NamingRulesMode {
    Warn,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NamingRule {
    pub suffix: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DependencyValue {
    Version(String),
    Detailed {
        version: String,
        features: Vec<String>,
    },
    /// ローカルパス依存: `{ path = "../../packages/anvil" }`
    LocalPath(PathBuf),
}

impl ForgeToml {
    pub fn load(dir: &Path) -> Result<Self, String> {
        let path = dir.join("forge.toml");
        let content = fs::read_to_string(&path)
            .map_err(|e| format!("forge.toml を読み込めませんでした: {}", e))?;
        let value = content
            .parse::<toml::Value>()
            .map_err(|e| format!("forge.toml のパースに失敗しました: {}", e))?;
        Self::from_value(&value)
    }

    pub fn find(start: &Path) -> Option<PathBuf> {
        let mut current = if start.is_dir() {
            start.to_path_buf()
        } else {
            start.parent()?.to_path_buf()
        };

        loop {
            let candidate = current.join("forge.toml");
            if candidate.is_file() {
                return Some(candidate);
            }
            if !current.pop() {
                return None;
            }
        }
    }

    /// ローカルパス依存の `(name, absolute_path)` ペアを返す
    ///
    /// `project_dir` を基準に相対パスを解決する。
    pub fn local_dep_paths(&self, project_dir: &Path) -> Vec<(String, PathBuf)> {
        self.dependencies
            .iter()
            .filter_map(|(name, dep)| {
                if let DependencyValue::LocalPath(rel) = dep {
                    let abs = project_dir.join(rel);
                    Some((name.clone(), abs))
                } else {
                    None
                }
            })
            .collect()
    }

    fn from_value(value: &toml::Value) -> Result<Self, String> {
        let root = value
            .as_table()
            .ok_or_else(|| "forge.toml のルートはテーブルである必要があります".to_string())?;

        let package = parse_package(root.get("package"))?;
        let build = parse_build(root.get("build"))?;
        let dependencies = parse_dependencies(root.get("dependencies"))?;
        let architecture = parse_architecture(root.get("architecture"))?;

        Ok(Self {
            package,
            build,
            dependencies,
            architecture,
        })
    }
}

fn parse_package(value: Option<&toml::Value>) -> Result<PackageSection, String> {
    let table = value
        .and_then(toml::Value::as_table)
        .ok_or_else(|| "[package] セクションが必要です".to_string())?;

    let name = get_required_string(table, "name", "[package]")?;
    let version = get_required_string(table, "version", "[package]")?;
    let forge = get_optional_string(table, "forge")?;
    let entry =
        get_optional_string(table, "entry")?.unwrap_or_else(|| "src/main.forge".to_string());

    Ok(PackageSection {
        name,
        version,
        forge,
        entry,
    })
}

fn parse_build(value: Option<&toml::Value>) -> Result<Option<BuildSection>, String> {
    let Some(value) = value else {
        return Ok(None);
    };
    let table = value
        .as_table()
        .ok_or_else(|| "[build] セクションはテーブルである必要があります".to_string())?;

    let output = get_optional_string(table, "output")?;
    let edition = get_optional_string(table, "edition")?.unwrap_or_else(|| "2021".to_string());

    Ok(Some(BuildSection { output, edition }))
}

fn parse_dependencies(
    value: Option<&toml::Value>,
) -> Result<BTreeMap<String, DependencyValue>, String> {
    let mut deps = BTreeMap::new();
    let Some(value) = value else {
        return Ok(deps);
    };
    let table = value
        .as_table()
        .ok_or_else(|| "[dependencies] セクションはテーブルである必要があります".to_string())?;

    for (name, dep_value) in table {
        let parsed = if let Some(version) = dep_value.as_str() {
            DependencyValue::Version(version.to_string())
        } else if let Some(dep_table) = dep_value.as_table() {
            // path-only dependency: { path = "..." }
            if dep_table.contains_key("path") && !dep_table.contains_key("version") {
                let path_str = dep_table
                    .get("path")
                    .and_then(toml::Value::as_str)
                    .ok_or_else(|| {
                        format!("[dependencies.{}].path は文字列である必要があります", name)
                    })?;
                DependencyValue::LocalPath(PathBuf::from(path_str))
            } else {
                let version = get_required_string(dep_table, "version", "[dependencies]")?;
                let features = match dep_table.get("features") {
                    None => Vec::new(),
                    Some(features) => {
                        let arr = features.as_array().ok_or_else(|| {
                            format!(
                                "[dependencies.{}].features は配列である必要があります",
                                name
                            )
                        })?;
                        let mut out = Vec::with_capacity(arr.len());
                        for item in arr {
                            let feature = item.as_str().ok_or_else(|| {
                                format!(
                                    "[dependencies.{}].features の要素は文字列である必要があります",
                                    name
                                )
                            })?;
                            out.push(feature.to_string());
                        }
                        out
                    }
                };
                DependencyValue::Detailed { version, features }
            }
        } else {
            return Err(format!(
                "[dependencies.{}] は文字列またはテーブルである必要があります",
                name
            ));
        };
        deps.insert(name.clone(), parsed);
    }

    Ok(deps)
}

fn parse_architecture(value: Option<&toml::Value>) -> Result<Option<ArchitectureSection>, String> {
    let Some(value) = value else {
        return Ok(None);
    };
    let table = value
        .as_table()
        .ok_or_else(|| "[architecture] セクションはテーブルである必要があります".to_string())?;

    let layers_value = table
        .get("layers")
        .ok_or_else(|| "[architecture] に必須フィールド 'layers' がありません".to_string())?;
    let layer_items = layers_value
        .as_array()
        .ok_or_else(|| "[architecture].layers は配列である必要があります".to_string())?;
    let mut layers = Vec::with_capacity(layer_items.len());
    for item in layer_items {
        let layer = item.as_str().ok_or_else(|| {
            "[architecture].layers の要素は文字列である必要があります".to_string()
        })?;
        layers.push(normalize_manifest_path(layer));
    }

    let naming_rules = match get_optional_string(table, "naming_rules")?.as_deref() {
        None | Some("warn") => NamingRulesMode::Warn,
        Some("error") => NamingRulesMode::Error,
        Some(other) => {
            return Err(format!(
                "[architecture].naming_rules は 'warn' または 'error' である必要があります: {}",
                other
            ))
        }
    };

    let mut naming = BTreeMap::new();
    if let Some(naming_value) = table.get("naming") {
        let naming_table = naming_value.as_table().ok_or_else(|| {
            "[architecture.naming] セクションはテーブルである必要があります".to_string()
        })?;
        for (layer, rule_value) in naming_table {
            let rule_table = rule_value.as_table().ok_or_else(|| {
                format!(
                    "[architecture.naming.{}] はテーブルである必要があります",
                    layer
                )
            })?;
            let suffix_value = rule_table.get("suffix").ok_or_else(|| {
                format!(
                    "[architecture.naming.{}] に必須フィールド 'suffix' がありません",
                    layer
                )
            })?;
            let suffix_items = suffix_value.as_array().ok_or_else(|| {
                format!(
                    "[architecture.naming.{}].suffix は配列である必要があります",
                    layer
                )
            })?;
            let mut suffix = Vec::with_capacity(suffix_items.len());
            for item in suffix_items {
                let value = item.as_str().ok_or_else(|| {
                    format!(
                        "[architecture.naming.{}].suffix の要素は文字列である必要があります",
                        layer
                    )
                })?;
                suffix.push(value.to_string());
            }
            naming.insert(normalize_manifest_path(layer), NamingRule { suffix });
        }
    }

    Ok(Some(ArchitectureSection {
        layers,
        naming_rules,
        naming,
    }))
}

fn normalize_manifest_path(path: &str) -> String {
    path.trim_matches('/')
        .replace('\\', "/")
        .trim_start_matches("./")
        .to_string()
}

fn get_required_string(
    table: &toml::map::Map<String, toml::Value>,
    key: &str,
    section: &str,
) -> Result<String, String> {
    table
        .get(key)
        .and_then(toml::Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| format!("{} に必須フィールド '{}' がありません", section, key))
}

fn get_optional_string(
    table: &toml::map::Map<String, toml::Value>,
    key: &str,
) -> Result<Option<String>, String> {
    match table.get(key) {
        None => Ok(None),
        Some(value) => value
            .as_str()
            .map(|s| Some(s.to_string()))
            .ok_or_else(|| format!("'{}' は文字列である必要があります", key)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unique_temp_dir() -> PathBuf {
        static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let seq = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        std::env::temp_dir().join(format!("forge_toml_test_{}_{}", std::process::id(), seq))
    }

    fn write_forge_toml(content: &str) -> PathBuf {
        let dir = unique_temp_dir();
        fs::create_dir_all(&dir).expect("create temp dir");
        fs::write(dir.join("forge.toml"), content).expect("write forge.toml");
        dir
    }

    #[test]
    fn test_parse_minimal_forge_toml() {
        let dir = write_forge_toml(
            r#"
[package]
name = "demo"
version = "0.1.0"
"#,
        );

        let parsed = ForgeToml::load(&dir).expect("load forge.toml");
        assert_eq!(parsed.package.name, "demo");
        assert_eq!(parsed.package.version, "0.1.0");
        assert_eq!(parsed.package.entry, "src/main.forge");
        assert!(parsed.build.is_none());
        assert!(parsed.architecture.is_none());
    }

    #[test]
    fn test_default_entry_is_used() {
        let dir = write_forge_toml(
            r#"
[package]
name = "demo"
version = "0.1.0"

[build]
output = "target/demo"
"#,
        );

        let parsed = ForgeToml::load(&dir).expect("load forge.toml");
        assert_eq!(parsed.package.entry, "src/main.forge");
        assert_eq!(parsed.build.expect("build").edition, "2021");
    }

    #[test]
    fn test_parse_string_dependency() {
        let dir = write_forge_toml(
            r#"
[package]
name = "demo"
version = "0.1.0"

[dependencies]
serde = "1.0"
"#,
        );

        let parsed = ForgeToml::load(&dir).expect("load forge.toml");
        assert_eq!(
            parsed.dependencies.get("serde"),
            Some(&DependencyValue::Version("1.0".to_string()))
        );
    }

    #[test]
    fn test_parse_table_dependency() {
        let dir = write_forge_toml(
            r#"
[package]
name = "demo"
version = "0.1.0"

[dependencies]
tokio = { version = "1", features = ["rt", "macros"] }
"#,
        );

        let parsed = ForgeToml::load(&dir).expect("load forge.toml");
        assert_eq!(
            parsed.dependencies.get("tokio"),
            Some(&DependencyValue::Detailed {
                version: "1".to_string(),
                features: vec!["rt".to_string(), "macros".to_string()],
            })
        );
    }

    #[test]
    fn test_parse_local_path_dependency() {
        let dir = write_forge_toml(
            r#"
[package]
name = "demo"
version = "0.1.0"

[dependencies]
anvil = { path = "../../packages/anvil" }
"#,
        );

        let parsed = ForgeToml::load(&dir).expect("load forge.toml");
        assert_eq!(
            parsed.dependencies.get("anvil"),
            Some(&DependencyValue::LocalPath(PathBuf::from(
                "../../packages/anvil"
            )))
        );
    }

    #[test]
    fn test_local_dep_paths_resolves_absolute() {
        let dir = write_forge_toml(
            r#"
[package]
name = "demo"
version = "0.1.0"

[dependencies]
anvil = { path = "packages/anvil" }
"#,
        );

        let parsed = ForgeToml::load(&dir).expect("load forge.toml");
        let paths = parsed.local_dep_paths(&dir);
        assert_eq!(paths.len(), 1);
        assert_eq!(paths[0].0, "anvil");
        assert_eq!(paths[0].1, dir.join("packages/anvil"));
    }

    #[test]
    fn test_invalid_toml_returns_error() {
        let dir = write_forge_toml(
            r#"
[package
name = "demo"
"#,
        );

        let err = ForgeToml::load(&dir).expect_err("should fail");
        assert!(err.contains("パース"), "err: {}", err);
    }

    #[test]
    fn test_find_forge_toml_walks_parents() {
        let dir = write_forge_toml(
            r#"
[package]
name = "demo"
version = "0.1.0"
"#,
        );
        let nested = dir.join("src").join("deep");
        fs::create_dir_all(&nested).expect("create nested");

        let found = ForgeToml::find(&nested).expect("find forge.toml");
        assert_eq!(found, dir.join("forge.toml"));
    }

    #[test]
    fn test_parse_architecture_section() {
        let dir = write_forge_toml(
            r#"
[package]
name = "demo"
version = "0.1.0"

[architecture]
layers = ["src/domain", "src/usecase"]
naming_rules = "error"

[architecture.naming]
"src/usecase" = { suffix = ["UseCase", "Service"] }
"#,
        );

        let parsed = ForgeToml::load(&dir).expect("load forge.toml");
        let arch = parsed.architecture.expect("architecture");
        assert_eq!(arch.layers, vec!["src/domain", "src/usecase"]);
        assert_eq!(arch.naming_rules, NamingRulesMode::Error);
        assert_eq!(
            arch.naming.get("src/usecase").expect("naming").suffix,
            vec!["UseCase".to_string(), "Service".to_string()]
        );
    }

    #[test]
    fn test_parse_architecture_default_naming_rules_warn() {
        let dir = write_forge_toml(
            r#"
[package]
name = "demo"
version = "0.1.0"

[architecture]
layers = ["src/domain"]
"#,
        );

        let parsed = ForgeToml::load(&dir).expect("load forge.toml");
        let arch = parsed.architecture.expect("architecture");
        assert_eq!(arch.naming_rules, NamingRulesMode::Warn);
    }
}
