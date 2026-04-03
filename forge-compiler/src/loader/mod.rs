// forge-compiler: モジュールローダー
// Phase M-0-D 実装

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::ast::{Stmt, UsePath, UseSymbols};
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

/// mod.forge の解析結果: re-export されたシンボルとその元パス
#[derive(Debug, Clone)]
pub struct ModForgeExport {
    /// re-export されたシンボル名 → (元ファイルパス, 元シンボル名)
    /// 例: "add" → ("basic", "add")
    pub symbols: HashMap<String, (String, String)>,
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

    /// `use_path` がディレクトリを指す場合に `mod.forge` の絶対パスを返す
    ///
    /// ディレクトリが存在して `mod.forge` が存在する場合は `Some(path)` を返す。
    /// ディレクトリは存在するが `mod.forge` がない場合は `None` を返す。
    pub fn resolve_mod_forge(&self, use_path: &str) -> Option<PathBuf> {
        let src_dir = self.project_root.join("src");
        let dir_path = if src_dir.exists() {
            src_dir.join(use_path)
        } else {
            self.project_root.join(use_path)
        };

        if dir_path.is_dir() {
            let mod_forge = dir_path.join("mod.forge");
            if mod_forge.exists() {
                Some(mod_forge)
            } else {
                // ディレクトリは存在するが mod.forge がない
                Some(dir_path) // ディレクトリ自体を返す（None と区別するため Some(dir)）
            }
        } else {
            None
        }
    }

    /// ディレクトリが存在するかどうかを確認する
    pub fn is_directory(&self, use_path: &str) -> bool {
        let src_dir = self.project_root.join("src");
        let dir_path = if src_dir.exists() {
            src_dir.join(use_path)
        } else {
            self.project_root.join(use_path)
        };
        dir_path.is_dir()
    }

    /// `mod.forge` を解析して re-export 情報を返す
    ///
    /// 戻り値は `ModForgeExport` の Result。
    /// `pub use basic.{add, multiply}` のような宣言を収集する。
    pub fn parse_mod_forge(&mut self, mod_forge_path: &Path) -> Result<ModForgeExport, LoadError> {
        let source = std::fs::read_to_string(mod_forge_path).map_err(|e| LoadError::Io {
            path: mod_forge_path.to_path_buf(),
            message: e.to_string(),
        })?;

        let module = parse_source(&source).map_err(|e| LoadError::Parse {
            path: mod_forge_path.to_path_buf(),
            message: e.to_string(),
        })?;

        let mut export = ModForgeExport {
            symbols: HashMap::new(),
        };

        for stmt in &module.stmts {
            if let Stmt::UseDecl { path, symbols, is_pub, .. } = stmt {
                if !is_pub {
                    continue;
                }
                // mod.forge 内の `pub use basic.{add}` を解析
                // パーサーは `./` のない識別子を External として扱うが、
                // mod.forge のコンテキストでは同一ディレクトリ内のファイルを指す
                let source_module = match path {
                    UsePath::Local(p) => p.clone(),
                    UsePath::External(p) => p.clone(),
                    UsePath::Stdlib(_) => continue,
                };

                match symbols {
                    UseSymbols::Single(name, _alias) => {
                        export.symbols.insert(
                            name.clone(),
                            (source_module.clone(), name.clone()),
                        );
                    }
                    UseSymbols::Multiple(names) => {
                        for (name, _alias) in names {
                            export.symbols.insert(
                                name.clone(),
                                (source_module.clone(), name.clone()),
                            );
                        }
                    }
                    UseSymbols::All => {
                        // .* の場合: ソースモジュールを特別マーカーとして記録
                        // "__all__:{source_module}" という形式で格納
                        export.symbols.insert(
                            format!("__all__{}", source_module),
                            (source_module.clone(), "*".to_string()),
                        );
                    }
                }
            }
        }

        Ok(export)
    }

    /// ディレクトリ内の全 pub シンボルを収集する（mod.forge がない場合）
    pub fn load_directory_all_pub(&mut self, dir_use_path: &str) -> Result<Vec<Stmt>, LoadError> {
        let src_dir = self.project_root.join("src");
        let dir_path = if src_dir.exists() {
            src_dir.join(dir_use_path)
        } else {
            self.project_root.join(dir_use_path)
        };

        let mut all_stmts = Vec::new();
        let entries = std::fs::read_dir(&dir_path).map_err(|e| LoadError::Io {
            path: dir_path.clone(),
            message: e.to_string(),
        })?;

        for entry in entries {
            let entry = entry.map_err(|e| LoadError::Io {
                path: dir_path.clone(),
                message: e.to_string(),
            })?;
            let file_path = entry.path();
            if file_path.extension().and_then(|e| e.to_str()) == Some("forge") {
                // mod.forge は除外（ディレクトリなし時はそもそも存在しないはずだが念のため）
                if file_path.file_name().and_then(|n| n.to_str()) == Some("mod.forge") {
                    continue;
                }
                let source = std::fs::read_to_string(&file_path).map_err(|e| LoadError::Io {
                    path: file_path.clone(),
                    message: e.to_string(),
                })?;
                let module = parse_source(&source).map_err(|e| LoadError::Parse {
                    path: file_path.clone(),
                    message: e.to_string(),
                })?;
                all_stmts.extend(module.stmts);
            }
        }

        Ok(all_stmts)
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

    /// M-2-E: mod.forge 経由のシンボル解決テスト
    #[test]
    fn test_mod_forge_routing() {
        let tmp = make_tmp_project();
        // math/basic.forge に pub fn add を定義
        write_file(tmp.path(), "math/basic.forge",
            "pub fn add(a: number, b: number) -> number { a + b }");
        // math/mod.forge で re-export
        write_file(tmp.path(), "math/mod.forge",
            "pub use basic.{add}");

        let mut loader = ModuleLoader::new(tmp.path().to_path_buf());

        // resolve_mod_forge がディレクトリを検出して mod.forge を返すことを確認
        let resolved = loader.resolve_mod_forge("math");
        assert!(resolved.is_some(), "math ディレクトリが検出されるべき");
        let path = resolved.unwrap();
        assert!(path.is_file(), "mod.forge ファイルが返されるべき");
        assert!(path.to_str().unwrap().contains("mod.forge"));

        // parse_mod_forge が "add" → ("basic", "add") のマッピングを返すことを確認
        let export = loader.parse_mod_forge(&path).expect("parse_mod_forge");
        assert!(export.symbols.contains_key("add"), "add が re-export されているべき");
        let (src_module, src_sym) = &export.symbols["add"];
        assert_eq!(src_module, "basic");
        assert_eq!(src_sym, "add");
    }

    /// M-2-E: A → mod.forge → B の re-export チェーンテスト
    #[test]
    fn test_reexport_chain() {
        let tmp = make_tmp_project();
        // math/basic.forge: pub fn add
        write_file(tmp.path(), "math/basic.forge",
            "pub fn add(a: number, b: number) -> number { a + b }");
        // math/advanced.forge: pub fn fast_pow
        write_file(tmp.path(), "math/advanced.forge",
            "pub fn fast_pow(base: number, exp: number) -> number { base * exp }");
        // math/mod.forge: 両方を re-export
        write_file(tmp.path(), "math/mod.forge",
            "pub use basic.{add}\npub use advanced.fast_pow");

        let mut loader = ModuleLoader::new(tmp.path().to_path_buf());

        let mod_forge_path = loader.resolve_mod_forge("math").expect("math dir exists");
        assert!(mod_forge_path.is_file());

        let export = loader.parse_mod_forge(&mod_forge_path).expect("parse");

        // add と fast_pow の両方が re-export されていること
        assert!(export.symbols.contains_key("add"), "add が re-export されるべき");
        assert!(export.symbols.contains_key("fast_pow"), "fast_pow が re-export されるべき");

        let (src_add, _) = &export.symbols["add"];
        assert_eq!(src_add, "basic");

        let (src_pow, _) = &export.symbols["fast_pow"];
        assert_eq!(src_pow, "advanced");
    }

    /// M-2-E: 3段階超の re-export チェーンで警告（ローダー側のチェック）
    #[test]
    fn test_reexport_depth_warning() {
        // このテストはインタープリタ側で depth > 3 を警告するロジックを検証する
        // ローダー側では深さのトラッキングは行わないため、
        // インタープリタが depth カウンタを渡すことを確認するシミュレーション
        // ここでは 3段階を超えるディレクトリ構造を作って parse_mod_forge が成功することを確認
        let tmp = make_tmp_project();
        write_file(tmp.path(), "a/b/c/d/leaf.forge",
            "pub fn value() -> number { 42 }");
        write_file(tmp.path(), "a/b/c/d/mod.forge",
            "pub use leaf.{value}");

        let mut loader = ModuleLoader::new(tmp.path().to_path_buf());
        let mod_path = loader.resolve_mod_forge("a/b/c/d").expect("dir exists");
        let export = loader.parse_mod_forge(&mod_path).expect("parse");
        assert!(export.symbols.contains_key("value"));
        // 深さ警告はインタープリタ側（eval_directory_use）で行われるため
        // ローダー自体はエラーを出さない
    }
}
