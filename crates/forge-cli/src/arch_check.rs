use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

use crate::forge_toml::{ArchitectureSection, ForgeToml, NamingRulesMode};
use forge_compiler::ast::{Stmt, UsePath};
use forge_compiler::parser::parse_source_with_file;

#[derive(Debug, Default)]
pub struct ArchCheckReport {
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone)]
struct SourceFile {
    abs_path: PathBuf,
    rel_path: String,
    stmts: Vec<Stmt>,
}

#[derive(Debug, Clone)]
struct Edge {
    from: String,
    to: String,
}

pub fn check_project_architecture(
    project_dir: &Path,
    forge_toml: &ForgeToml,
) -> Result<ArchCheckReport, String> {
    let Some(architecture) = forge_toml.architecture.as_ref() else {
        return Ok(ArchCheckReport::default());
    };

    let files = collect_forge_files(project_dir)?;
    let file_set = files
        .iter()
        .map(|file| file.rel_path.clone())
        .collect::<BTreeSet<_>>();

    let mut report = ArchCheckReport::default();
    let mut edges = Vec::new();

    for file in &files {
        for stmt in &file.stmts {
            if let Stmt::UseDecl { path, .. } = stmt {
                if let Some(target) = resolve_use_target(project_dir, file, path, &file_set) {
                    edges.push(Edge {
                        from: file.rel_path.clone(),
                        to: target,
                    });
                }
            }
        }
    }

    report
        .errors
        .extend(check_layer_violations(architecture, &edges));
    report
        .errors
        .extend(check_cycles(&files, &edges, architecture));
    check_naming_rules(architecture, &files, &mut report);

    Ok(report)
}

fn collect_forge_files(project_dir: &Path) -> Result<Vec<SourceFile>, String> {
    let mut paths = Vec::new();
    collect_forge_file_paths(project_dir, project_dir, &mut paths)?;
    paths.sort();

    let mut files = Vec::with_capacity(paths.len());
    for abs_path in paths {
        let rel_path = relative_slash_path(project_dir, &abs_path)?;
        let source = fs::read_to_string(&abs_path)
            .map_err(|e| format!("{} を読み込めませんでした: {}", rel_path, e))?;
        let module = parse_source_with_file(&source, rel_path.clone())
            .map_err(|e| format!("{} の構文エラー: {}", rel_path, e))?;
        files.push(SourceFile {
            abs_path,
            rel_path,
            stmts: module.stmts,
        });
    }

    Ok(files)
}

fn collect_forge_file_paths(
    project_dir: &Path,
    dir: &Path,
    out: &mut Vec<PathBuf>,
) -> Result<(), String> {
    let entries = fs::read_dir(dir)
        .map_err(|e| format!("{} を読み込めませんでした: {}", dir.display(), e))?;
    for entry in entries {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if path.is_dir() {
            if should_skip_dir(project_dir, &path) {
                continue;
            }
            collect_forge_file_paths(project_dir, &path, out)?;
        } else if path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.eq_ignore_ascii_case("forge"))
            .unwrap_or(false)
        {
            out.push(path);
        }
    }
    Ok(())
}

fn should_skip_dir(project_dir: &Path, path: &Path) -> bool {
    let Ok(rel) = path.strip_prefix(project_dir) else {
        return false;
    };
    matches!(
        rel.components()
            .next()
            .and_then(|component| component.as_os_str().to_str()),
        Some("target") | Some(".git")
    )
}

fn resolve_use_target(
    project_dir: &Path,
    file: &SourceFile,
    path: &UsePath,
    file_set: &BTreeSet<String>,
) -> Option<String> {
    match path {
        UsePath::Stdlib(_) => None,
        UsePath::Local(use_path) => {
            let current_dir = file.abs_path.parent().unwrap_or(project_dir);
            let local_base = current_dir.join(use_path);
            resolve_module_path(project_dir, &local_base, file_set).or_else(|| {
                resolve_module_path(
                    project_dir,
                    &project_dir.join("src").join(use_path),
                    file_set,
                )
            })
        }
        UsePath::External(use_path) => resolve_module_path(
            project_dir,
            &project_dir.join("src").join(use_path),
            file_set,
        ),
    }
}

fn resolve_module_path(
    project_dir: &Path,
    base: &Path,
    file_set: &BTreeSet<String>,
) -> Option<String> {
    let mut candidates = Vec::new();
    candidates.push(base.to_path_buf());
    if base.extension().is_none() {
        candidates.push(base.with_extension("forge"));
        candidates.push(base.join("mod.forge"));
    }

    for candidate in candidates {
        if let Ok(rel) = relative_slash_path(project_dir, &candidate) {
            if file_set.contains(&rel) {
                return Some(rel);
            }
        }
    }
    None
}

fn check_layer_violations(architecture: &ArchitectureSection, edges: &[Edge]) -> Vec<String> {
    let mut errors = Vec::new();
    for edge in edges {
        let Some((from_index, from_layer)) = layer_for_path(architecture, &edge.from) else {
            continue;
        };
        let Some((to_index, to_layer)) = layer_for_path(architecture, &edge.to) else {
            continue;
        };
        if from_index < to_index {
            errors.push(format!(
                "エラー: 依存方向違反\n  {} が {} に依存しています\n  {} -> {} の方向は禁止されています\n  ルール: layers = [{}]",
                edge.from,
                edge.to,
                display_layer_name(from_layer),
                display_layer_name(to_layer),
                architecture.layers.join(", ")
            ));
        }
    }
    errors
}

fn layer_for_path<'a>(
    architecture: &'a ArchitectureSection,
    rel_path: &str,
) -> Option<(usize, &'a str)> {
    architecture
        .layers
        .iter()
        .enumerate()
        .find(|(_, layer)| {
            rel_path == layer.as_str() || rel_path.starts_with(&format!("{}/", layer))
        })
        .map(|(index, layer)| (index, layer.as_str()))
}

fn check_cycles(
    files: &[SourceFile],
    edges: &[Edge],
    architecture: &ArchitectureSection,
) -> Vec<String> {
    let mut graph = BTreeMap::<String, Vec<String>>::new();
    for file in files {
        graph.entry(file.rel_path.clone()).or_default();
    }
    for edge in edges {
        graph
            .entry(edge.from.clone())
            .or_default()
            .push(edge.to.clone());
    }

    let components = tarjan_scc(&graph);
    let mut errors = Vec::new();
    for component in components {
        let self_loop = component.len() == 1
            && graph
                .get(&component[0])
                .map(|neighbors| neighbors.iter().any(|neighbor| neighbor == &component[0]))
                .unwrap_or(false);
        if component.len() > 1 || self_loop {
            let mut cycle = component;
            cycle.sort();
            errors.push(format!(
                "エラー: 循環依存\n  {}\n  ルール: layers = [{}]",
                cycle.join(" -> "),
                architecture.layers.join(", ")
            ));
        }
    }
    errors
}

fn tarjan_scc(graph: &BTreeMap<String, Vec<String>>) -> Vec<Vec<String>> {
    struct Tarjan<'a> {
        graph: &'a BTreeMap<String, Vec<String>>,
        index: usize,
        stack: Vec<String>,
        on_stack: BTreeSet<String>,
        indices: HashMap<String, usize>,
        lowlinks: HashMap<String, usize>,
        components: Vec<Vec<String>>,
    }

    impl<'a> Tarjan<'a> {
        fn strong_connect(&mut self, node: &str) {
            self.indices.insert(node.to_string(), self.index);
            self.lowlinks.insert(node.to_string(), self.index);
            self.index += 1;
            self.stack.push(node.to_string());
            self.on_stack.insert(node.to_string());

            if let Some(neighbors) = self.graph.get(node) {
                for neighbor in neighbors {
                    if !self.indices.contains_key(neighbor) {
                        self.strong_connect(neighbor);
                        let lowlink = self.lowlinks[node].min(self.lowlinks[neighbor]);
                        self.lowlinks.insert(node.to_string(), lowlink);
                    } else if self.on_stack.contains(neighbor) {
                        let lowlink = self.lowlinks[node].min(self.indices[neighbor]);
                        self.lowlinks.insert(node.to_string(), lowlink);
                    }
                }
            }

            if self.lowlinks[node] == self.indices[node] {
                let mut component = Vec::new();
                while let Some(member) = self.stack.pop() {
                    self.on_stack.remove(&member);
                    component.push(member.clone());
                    if member == node {
                        break;
                    }
                }
                self.components.push(component);
            }
        }
    }

    let mut tarjan = Tarjan {
        graph,
        index: 0,
        stack: Vec::new(),
        on_stack: BTreeSet::new(),
        indices: HashMap::new(),
        lowlinks: HashMap::new(),
        components: Vec::new(),
    };

    for node in graph.keys() {
        if !tarjan.indices.contains_key(node) {
            tarjan.strong_connect(node);
        }
    }

    tarjan.components
}

fn check_naming_rules(
    architecture: &ArchitectureSection,
    files: &[SourceFile],
    report: &mut ArchCheckReport,
) {
    for file in files {
        let Some((_, layer)) = layer_for_path(architecture, &file.rel_path) else {
            continue;
        };
        let Some(rule) = architecture.naming.get(layer) else {
            continue;
        };
        if rule.suffix.is_empty() {
            continue;
        }

        for type_name in type_names(&file.stmts) {
            if rule.suffix.iter().any(|suffix| type_name.ends_with(suffix)) {
                continue;
            }
            let message = format!(
                "{}: 命名規則違反\n  {}: \"{}\" は {} で終わっていません",
                match architecture.naming_rules {
                    NamingRulesMode::Warn => "警告",
                    NamingRulesMode::Error => "エラー",
                },
                file.rel_path,
                type_name,
                rule.suffix.join(" / ")
            );
            match architecture.naming_rules {
                NamingRulesMode::Warn => report.warnings.push(message),
                NamingRulesMode::Error => report.errors.push(message),
            }
        }
    }
}

fn type_names(stmts: &[Stmt]) -> Vec<&str> {
    let mut names = Vec::new();
    for stmt in stmts {
        match stmt {
            Stmt::StructDef { name, .. }
            | Stmt::EnumDef { name, .. }
            | Stmt::TraitDef { name, .. }
            | Stmt::MixinDef { name, .. }
            | Stmt::DataDef { name, .. }
            | Stmt::TypestateDef { name, .. } => names.push(name.as_str()),
            Stmt::When { body, .. } => names.extend(type_names(body)),
            Stmt::TestBlock { body, .. } => names.extend(type_names(body)),
            _ => {}
        }
    }
    names
}

fn relative_slash_path(project_dir: &Path, path: &Path) -> Result<String, String> {
    let rel = path
        .strip_prefix(project_dir)
        .map_err(|_| format!("{} はプロジェクト外のパスです", path.display()))?;
    Ok(rel
        .components()
        .filter_map(|component| component.as_os_str().to_str())
        .collect::<Vec<_>>()
        .join("/"))
}

fn display_layer_name(layer: &str) -> &str {
    layer.rsplit('/').next().unwrap_or(layer)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static UNIQUE_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn temp_project(label: &str) -> PathBuf {
        let seq = UNIQUE_COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "forge_arch_check_{}_{}_{}",
            label,
            std::process::id(),
            seq
        ));
        fs::create_dir_all(dir.join("src/domain")).expect("domain dir");
        fs::create_dir_all(dir.join("src/usecase")).expect("usecase dir");
        dir
    }

    fn write_project_toml(dir: &Path, extra: &str) {
        fs::write(
            dir.join("forge.toml"),
            format!(
                "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n\n[architecture]\nlayers = [\"src/domain\", \"src/usecase\"]\n{}\n",
                extra
            ),
        )
        .expect("write forge.toml");
    }

    #[test]
    fn test_arch_valid() {
        let dir = temp_project("valid");
        write_project_toml(&dir, "");
        fs::write(
            dir.join("src/domain/user.forge"),
            "data User { name: string }\n",
        )
        .expect("domain");
        fs::write(
            dir.join("src/usecase/register.forge"),
            "use ./domain/user.*\nstruct RegisterUseCase { name: string }\n",
        )
        .expect("usecase");

        let toml = ForgeToml::load(&dir).expect("toml");
        let report = check_project_architecture(&dir, &toml).expect("check");
        assert!(report.errors.is_empty(), "{:?}", report);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn test_arch_violation() {
        let dir = temp_project("violation");
        write_project_toml(&dir, "");
        fs::write(
            dir.join("src/domain/user.forge"),
            "use ./usecase/register.*\ndata User { name: string }\n",
        )
        .expect("domain");
        fs::write(
            dir.join("src/usecase/register.forge"),
            "struct RegisterUseCase { name: string }\n",
        )
        .expect("usecase");

        let toml = ForgeToml::load(&dir).expect("toml");
        let report = check_project_architecture(&dir, &toml).expect("check");
        assert!(
            report.errors.iter().any(|err| err.contains("依存方向違反")),
            "{:?}",
            report
        );
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn test_arch_circular() {
        let dir = temp_project("circular");
        write_project_toml(&dir, "");
        fs::write(
            dir.join("src/domain/a.forge"),
            "use ./b.*\ndata A { name: string }\n",
        )
        .expect("a");
        fs::write(
            dir.join("src/domain/b.forge"),
            "use ./a.*\ndata B { name: string }\n",
        )
        .expect("b");

        let toml = ForgeToml::load(&dir).expect("toml");
        let report = check_project_architecture(&dir, &toml).expect("check");
        assert!(
            report.errors.iter().any(|err| err.contains("循環依存")),
            "{:?}",
            report
        );
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn test_arch_naming_warn() {
        let dir = temp_project("naming");
        write_project_toml(
            &dir,
            "\n[architecture.naming]\n\"src/usecase\" = { suffix = [\"UseCase\"] }\n",
        );
        fs::write(
            dir.join("src/usecase/register.forge"),
            "struct Register { }\n",
        )
        .expect("usecase");

        let toml = ForgeToml::load(&dir).expect("toml");
        let report = check_project_architecture(&dir, &toml).expect("check");
        assert!(report.errors.is_empty(), "{:?}", report);
        assert!(
            report
                .warnings
                .iter()
                .any(|warning| warning.contains("命名規則違反")),
            "{:?}",
            report
        );
        let _ = fs::remove_dir_all(dir);
    }
}
