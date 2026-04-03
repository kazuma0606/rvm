// forge-compiler: 依存関係マネージャー
// Phase M-3-B で実装
// 外部クレート名を収集し、Cargo.toml の [dependencies] へ自動追記する

use std::collections::HashSet;
use std::path::Path;

/// 依存関係の収集・管理エラー
#[derive(Debug)]
pub enum DepsError {
    /// ファイル I/O エラー
    Io(std::io::Error),
    /// Cargo.toml のパース・フォーマットエラー
    Format(String),
}

impl std::fmt::Display for DepsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DepsError::Io(e) => write!(f, "I/O エラー: {}", e),
            DepsError::Format(msg) => write!(f, "フォーマットエラー: {}", msg),
        }
    }
}

impl From<std::io::Error> for DepsError {
    fn from(e: std::io::Error) -> Self {
        DepsError::Io(e)
    }
}

/// 外部クレートの依存関係マネージャー
///
/// ForgeScript ソース中の `use serde` / `use reqwest.{Client}` 等から
/// 外部クレート名を収集し、`Cargo.toml` の `[dependencies]` セクションへ
/// べき等に追記する。
pub struct DepsManager {
    /// 収集した外部クレート名（重複なし）
    crates: HashSet<String>,
}

impl DepsManager {
    /// 新しい `DepsManager` を作成する
    pub fn new() -> Self {
        Self {
            crates: HashSet::new(),
        }
    }

    /// 外部クレート名を追加する（重複は無視）
    pub fn add(&mut self, crate_name: &str) {
        // スラッシュ区切りのパス（`reqwest/client`）の場合はトップレベル名を取得
        let top_level = crate_name.split('/').next().unwrap_or(crate_name);
        self.crates.insert(top_level.to_string());
    }

    /// 収集したクレート名の集合への参照を返す
    pub fn crates(&self) -> &HashSet<String> {
        &self.crates
    }

    /// `Cargo.toml` の `[dependencies]` セクションへ未記載のクレートを追記する
    ///
    /// 動作:
    /// 1. `cargo_toml_path` を読み込む
    /// 2. `[dependencies]` セクションを探す
    /// 3. 各クレートが既に記載されていなければ `crate_name = "*"` を追記
    /// 4. 同じクレートが複数回 `use` されても1回だけ追記（べき等）
    pub fn update_cargo_toml(&self, cargo_toml_path: &Path) -> Result<(), DepsError> {
        // ファイルを読み込む
        let content = std::fs::read_to_string(cargo_toml_path)?;

        let updated = Self::insert_missing_deps(&content, &self.crates)?;

        if updated != content {
            std::fs::write(cargo_toml_path, &updated)?;
        }

        Ok(())
    }

    /// Cargo.toml の内容文字列に対して依存クレートを挿入した新しい文字列を返す
    ///
    /// `[dependencies]` セクションが存在しない場合はファイル末尾に追記する。
    fn insert_missing_deps(
        content: &str,
        crates: &HashSet<String>,
    ) -> Result<String, DepsError> {
        if crates.is_empty() {
            return Ok(content.to_string());
        }

        let lines: Vec<&str> = content.lines().collect();

        // [dependencies] セクションの開始行を探す
        let deps_section_idx = lines.iter().position(|l| l.trim() == "[dependencies]");

        match deps_section_idx {
            Some(start_idx) => {
                // [dependencies] セクション内の既存クレート名を収集する
                // セクションの終わりは次の `[` で始まる行、またはファイル末尾
                let section_end_idx = lines[start_idx + 1..]
                    .iter()
                    .position(|l| l.trim_start().starts_with('['))
                    .map(|rel| start_idx + 1 + rel)
                    .unwrap_or(lines.len());

                let existing: HashSet<String> = lines[start_idx + 1..section_end_idx]
                    .iter()
                    .filter_map(|l| {
                        let trimmed = l.trim();
                        if trimmed.is_empty() || trimmed.starts_with('#') {
                            return None;
                        }
                        // `crate_name = "version"` または `crate_name = { ... }` の形式
                        trimmed.split('=').next().map(|s| s.trim().to_string())
                    })
                    .collect();

                // まだ記載されていないクレートのみを追記行として準備
                let mut new_lines: Vec<String> = crates
                    .iter()
                    .filter(|c| !existing.contains(*c))
                    .map(|c| format!("{} = \"*\"", c))
                    .collect();

                if new_lines.is_empty() {
                    return Ok(content.to_string());
                }

                // アルファベット順にソートして出力を安定させる
                new_lines.sort();

                // section_end_idx の直前（セクション末尾）に挿入する
                let mut result_lines: Vec<String> = lines.iter().map(|s| s.to_string()).collect();

                // 挿入位置: section_end_idx（次のセクションの直前）または末尾
                let insert_at = if section_end_idx < lines.len() {
                    // 次のセクション行の直前に空行を入れて挿入
                    // 既存の空行を1つ確認して重複しないようにする
                    let before = section_end_idx.saturating_sub(1);
                    if result_lines[before].trim().is_empty() {
                        // すでに空行あり: その前に挿入
                        before
                    } else {
                        section_end_idx
                    }
                } else {
                    lines.len()
                };

                for (i, line) in new_lines.into_iter().enumerate() {
                    result_lines.insert(insert_at + i, line);
                }

                Ok(result_lines.join("\n") + if content.ends_with('\n') { "\n" } else { "" })
            }
            None => {
                // [dependencies] セクションがない場合: ファイル末尾に追記
                let mut result = content.to_string();
                if !result.ends_with('\n') {
                    result.push('\n');
                }
                result.push_str("\n[dependencies]\n");

                let mut new_lines: Vec<String> = crates
                    .iter()
                    .map(|c| format!("{} = \"*\"", c))
                    .collect();
                new_lines.sort();

                for line in new_lines {
                    result.push_str(&line);
                    result.push('\n');
                }

                Ok(result)
            }
        }
    }
}

impl Default for DepsManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_external_crate_detection() {
        let mut manager = DepsManager::new();
        manager.add("serde");
        manager.add("reqwest");
        manager.add("serde"); // 重複
        assert_eq!(manager.crates().len(), 2);
        assert!(manager.crates().contains("serde"));
        assert!(manager.crates().contains("reqwest"));
    }

    #[test]
    fn test_cargo_toml_update() {
        let content = r#"[package]
name = "test"
version = "0.1.0"

[dependencies]
anyhow = "1"
"#;

        // 一時ファイルに Cargo.toml を作成
        let mut tmp = tempfile::NamedTempFile::new().expect("tempfile");
        tmp.write_all(content.as_bytes()).expect("write");
        let path = tmp.path().to_path_buf();

        let mut manager = DepsManager::new();
        manager.add("serde");
        manager.add("reqwest");
        manager.add("anyhow"); // 既存クレート — スキップされるはず

        manager.update_cargo_toml(&path).expect("update");

        let updated = std::fs::read_to_string(&path).expect("read");

        // serde と reqwest が追記されているはず
        assert!(updated.contains("serde = \"*\""), "serde が追記されていません: {}", updated);
        assert!(updated.contains("reqwest = \"*\""), "reqwest が追記されていません: {}", updated);

        // anyhow は重複追記されないはず（元の `anyhow = "1"` のみ）
        let anyhow_count = updated.matches("anyhow").count();
        assert_eq!(anyhow_count, 1, "anyhow が重複しています: {}", updated);

        // [dependencies] セクションが壊れていないこと
        assert!(updated.contains("[dependencies]"));
        assert!(updated.contains("anyhow = \"1\""));
    }

    #[test]
    fn test_cargo_toml_update_no_dependencies_section() {
        let content = r#"[package]
name = "test"
version = "0.1.0"
"#;
        let mut tmp = tempfile::NamedTempFile::new().expect("tempfile");
        tmp.write_all(content.as_bytes()).expect("write");
        let path = tmp.path().to_path_buf();

        let mut manager = DepsManager::new();
        manager.add("serde");

        manager.update_cargo_toml(&path).expect("update");

        let updated = std::fs::read_to_string(&path).expect("read");

        assert!(updated.contains("[dependencies]"));
        assert!(updated.contains("serde = \"*\""));
    }

    #[test]
    fn test_cargo_toml_update_idempotent() {
        let content = r#"[package]
name = "test"
version = "0.1.0"

[dependencies]
serde = "*"
"#;
        let mut tmp = tempfile::NamedTempFile::new().expect("tempfile");
        tmp.write_all(content.as_bytes()).expect("write");
        let path = tmp.path().to_path_buf();

        let mut manager = DepsManager::new();
        manager.add("serde");

        // 2回実行してもべき等
        manager.update_cargo_toml(&path).expect("first update");
        manager.update_cargo_toml(&path).expect("second update");

        let updated = std::fs::read_to_string(&path).expect("read");
        let serde_count = updated.matches("serde").count();
        assert_eq!(serde_count, 1, "serde が重複しています: {}", updated);
    }

    #[test]
    fn test_add_strips_subpath() {
        // reqwest/client のようなサブパス付きクレートはトップレベル名のみ収集
        let mut manager = DepsManager::new();
        manager.add("reqwest/client");
        assert!(manager.crates().contains("reqwest"));
        assert!(!manager.crates().contains("reqwest/client"));
    }
}
