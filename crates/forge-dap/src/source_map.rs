// forge-dap: Bloom ソースマップ統合（DBG-4-F）
//
// `.bloom` → `.forge` の行番号変換テーブルを管理する。
// `.bloom.map` ファイルは bloom-compiler が生成する JSON ファイルで、以下の形式を取る:
//
// {
//   "bloom_file": "src/components/counter.bloom",
//   "forge_file": "dist/generated/components/counter_page.forge",
//   "mappings": [
//     { "bloom_line": 8, "forge_line": 24 },
//     ...
//   ]
// }

use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LineMapping {
    pub bloom_line: usize,
    pub forge_line: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BloomSourceMapData {
    pub bloom_file: String,
    pub forge_file: String,
    pub mappings: Vec<LineMapping>,
}

/// `.bloom` ↔ `.forge` の行番号変換テーブル
#[derive(Debug, Clone)]
pub struct BloomSourceMap {
    /// forge_file → bloom_file
    pub forge_to_bloom_file: HashMap<String, String>,
    /// (forge_file, forge_line) → (bloom_file, bloom_line)
    pub forge_to_bloom_map: HashMap<(String, usize), (String, usize)>,
    /// (bloom_file, bloom_line) → (forge_file, forge_line)
    pub bloom_to_forge_map: HashMap<(String, usize), (String, usize)>,
}

impl BloomSourceMap {
    pub fn new() -> Self {
        BloomSourceMap {
            forge_to_bloom_file: HashMap::new(),
            forge_to_bloom_map: HashMap::new(),
            bloom_to_forge_map: HashMap::new(),
        }
    }

    /// `.bloom.map` ファイルを読み込む
    pub fn load(path: &Path) -> std::io::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let data: BloomSourceMapData = serde_json::from_str(&content)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        let mut map = Self::new();
        map.add_data(&data);
        Ok(map)
    }

    /// ソースマップデータを追加する
    pub fn add_data(&mut self, data: &BloomSourceMapData) {
        self.forge_to_bloom_file
            .insert(data.forge_file.clone(), data.bloom_file.clone());
        for m in &data.mappings {
            self.forge_to_bloom_map.insert(
                (data.forge_file.clone(), m.forge_line),
                (data.bloom_file.clone(), m.bloom_line),
            );
            self.bloom_to_forge_map.insert(
                (data.bloom_file.clone(), m.bloom_line),
                (data.forge_file.clone(), m.forge_line),
            );
        }
    }

    /// forge ファイル・行番号を bloom 側に変換する
    pub fn forge_to_bloom(&self, forge_file: &str, forge_line: usize) -> Option<(String, usize)> {
        self.forge_to_bloom_map
            .get(&(forge_file.to_string(), forge_line))
            .cloned()
    }

    /// bloom ファイル・行番号を forge 側に変換する
    pub fn bloom_to_forge(&self, bloom_file: &str, bloom_line: usize) -> Option<(String, usize)> {
        self.bloom_to_forge_map
            .get(&(bloom_file.to_string(), bloom_line))
            .cloned()
    }
}

impl Default for BloomSourceMap {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_source_map_roundtrip() {
        let mut map = BloomSourceMap::new();
        let data = BloomSourceMapData {
            bloom_file: "counter.bloom".to_string(),
            forge_file: "counter_page.forge".to_string(),
            mappings: vec![
                LineMapping {
                    bloom_line: 8,
                    forge_line: 24,
                },
                LineMapping {
                    bloom_line: 15,
                    forge_line: 40,
                },
            ],
        };
        map.add_data(&data);

        assert_eq!(
            map.forge_to_bloom("counter_page.forge", 24),
            Some(("counter.bloom".to_string(), 8))
        );
        assert_eq!(
            map.bloom_to_forge("counter.bloom", 8),
            Some(("counter_page.forge".to_string(), 24))
        );
        assert_eq!(map.forge_to_bloom("counter_page.forge", 99), None);
    }
}
