// forge-compiler: モジュールローダー
// Phase M-0-D 実装

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::ast::Stmt;
use crate::parser::parse_source;

/// モジュール読み込みエラー
#[derive(Debug, Clone, PartialEq)]
pub enum LoadError {
    /// ファイルが見つからない
    NotFound { path: PathBuf },
    /// ファイルの読み込みエラー
    Io { path: PathBuf, message: String },
    /// パースエラー
    Parse { path: PathBuf, message: String },
    /// 非公開シンボルへのアクセス
    PrivateSymbol { name: String, path: String },
}

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoadError::NotFound { path } => {
                write!(f, "モジュールが見つかりません: {}", path.display())
            }
            LoadError::Io { path, message } => {
                write!(f, "ファイル読み込みエラー '{}': {}", path.display(), message)
            }
            LoadError::Parse { path, message } => {
                write!(f, "パースエラー '{}': {}", path.display(), message)
            }
            LoadError::PrivateSymbol { name, path } => {
                write!(
                    f,
                    "`{}` は非公開です（`pub` キーワードがありません）\n  --> {}",
                    name, path
                )
            }
        }
    }
}

impl std::error::Error for LoadError {}

/// モジュールローダー
///
/// `project_root` を起点に `.forge` ファイルを解決・読み込みする。
/// パース済み AST をキャッシュして二重読み込みを防ぐ。
pub struct ModuleLoader {
    /// プロジェクトルート（`src/` の親ディレクトリ、または main.forge のディレクトリ）
    project_root: PathBuf,
    /// キャッシュ: use_path → パース済み AST
    cache: HashMap<String, Vec<Stmt>>,
}

impl ModuleLoader {
    pub fn new(project_root: PathBuf) -> Self {
        Self {
            project_root,
            cache: HashMap::new(),
        }
    }

    /// `use_path` からファイルの絶対パスを解決する
    ///
    /// `use_path` は `./` や `forge/std/` を除いたパス（`utils/helper` など）
    pub fn resolve_path(&self, use_path: &str) -> Result<PathBuf, LoadError> {
        // src/ ディレクトリが存在する場合はそちらを優先
        let src_dir = self.project_root.join("src");
        let candidates: Vec<PathBuf> = if src_dir.exists() {
            vec![
                src_dir.join(format!("{}.forge", use_path)),
            ]
        } else {
            // src/ がなければ project_root 直下
            vec![
                self.project_root.join(format!("{}.forge", use_path)),
            ]
        };

        for candidate in &candidates {
            if candidate.exists() {
                return Ok(candidate.clone());
            }
        }

        // 見つからなかった場合は最初の候補をエラーとして返す
        Err(LoadError::NotFound {
            path: candidates.into_iter().next().unwrap_or_else(|| {
                self.project_root.join(format!("{}.forge", use_path))
            }),
        })
    }

    /// `use_path` のモジュールを読み込んでパース済み AST を返す
    ///
    /// キャッシュがある場合はキャッシュを返す。
    pub fn load(&mut self, use_path: &str) -> Result<Vec<Stmt>, LoadError> {
        // キャッシュチェック
        if let Some(cached) = self.cache.get(use_path) {
            return Ok(cached.clone());
        }

        let file_path = self.resolve_path(use_path)?;

        let source = std::fs::read_to_string(&file_path).map_err(|e| LoadError::Io {
            path: file_path.clone(),
            message: e.to_string(),
        })?;

        let module = parse_source(&source).map_err(|e| LoadError::Parse {
            path: file_path.clone(),
            message: e.to_string(),
        })?;

        let stmts = module.stmts;
        self.cache.insert(use_path.to_string(), stmts.clone());
        Ok(stmts)
    }

    /// ファイルパスから `ModuleLoader` を作成する
    ///
    /// `main.forge` などのファイルパスを受け取り、プロジェクトルートを決定する。
    /// - ファイルのディレクトリが `src/` なら親をルートとする
    /// - それ以外はファイルのディレクトリ自体をルートとする
    pub fn from_file_path(file_path: &Path) -> Self {
        let dir = file_path
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."));

        let project_root = if dir.file_name().and_then(|n| n.to_str()) == Some("src") {
            dir.parent()
                .map(|p| p.to_path_buf())
                .unwrap_or(dir)
        } else {
            dir
        };

        Self::new(project_root)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn make_tmp_project() -> TempDir {
        tempfile::tempdir().expect("tmpdir")
    }

    fn write_file(base: &Path, rel_path: &str, content: &str) {
        let path = base.join(rel_path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create_dir_all");
        }
        fs::write(&path, content).expect("write_file");
    }

    #[test]
    fn test_loader_resolve_with_src_dir() {
        let tmp = make_tmp_project();
        write_file(tmp.path(), "src/utils/helper.forge", "fn add(a: number, b: number) -> number { a + b }");

        let loader = ModuleLoader::new(tmp.path().to_path_buf());
        let resolved = loader.resolve_path("utils/helper").expect("resolve");
        assert!(resolved.exists());
        assert!(resolved.to_str().unwrap().contains("helper.forge"));
    }

    #[test]
    fn test_loader_not_found() {
        let tmp = make_tmp_project();
        let loader = ModuleLoader::new(tmp.path().to_path_buf());
        let result = loader.resolve_path("nonexistent/module");
        assert!(matches!(result, Err(LoadError::NotFound { .. })));
    }

    #[test]
    fn test_loader_cache() {
        let tmp = make_tmp_project();
        write_file(tmp.path(), "src/utils/helper.forge", "fn add(a: number, b: number) -> number { a + b }");

        let mut loader = ModuleLoader::new(tmp.path().to_path_buf());
        let stmts1 = loader.load("utils/helper").expect("load 1");
        let stmts2 = loader.load("utils/helper").expect("load 2");
        // キャッシュから返されるので同一内容
        assert_eq!(stmts1.len(), stmts2.len());
    }

    #[test]
    fn test_from_file_path_src_dir() {
        let tmp = make_tmp_project();
        fs::create_dir_all(tmp.path().join("src")).expect("mkdir src");
        let main_path = tmp.path().join("src/main.forge");

        let loader = ModuleLoader::from_file_path(&main_path);
        // src/ の親がルートになるはず
        assert_eq!(loader.project_root, tmp.path().to_path_buf());
    }

    #[test]
    fn test_from_file_path_no_src_dir() {
        let tmp = make_tmp_project();
        let main_path = tmp.path().join("main.forge");

        let loader = ModuleLoader::from_file_path(&main_path);
        // src/ がないのでファイルのディレクトリがルート
        assert_eq!(loader.project_root, tmp.path().to_path_buf());
    }
}
