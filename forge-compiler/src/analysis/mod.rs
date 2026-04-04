// forge-compiler: 静的解析モジュール
// Phase M-4 実装

use std::collections::{HashMap, HashSet};

use crate::ast::{Stmt, UsePath};

/// モジュール依存グラフ
///
/// ファイルパス（文字列）をノードとし、use 宣言を有向辺として管理する。
/// 循環参照の検出に使用する。
#[derive(Debug, Default)]
pub struct DependencyGraph {
    /// ファイルパス → そのファイルが use するパスのリスト
    edges: HashMap<String, Vec<String>>,
}

impl DependencyGraph {
    pub fn new() -> Self {
        Self {
            edges: HashMap::new(),
        }
    }

    /// `from` が `to` に依存する辺を追加する
    pub fn add_edge(&mut self, from: &str, to: &str) {
        self.edges
            .entry(from.to_string())
            .or_default()
            .push(to.to_string());
    }

    /// `file_path` の AST から Local な use 文を収集して辺を追加する
    pub fn build_from_stmts(&mut self, file_path: &str, stmts: &[Stmt]) {
        for stmt in stmts {
            if let Stmt::UseDecl { path, .. } = stmt {
                if let UsePath::Local(use_path) = path {
                    // "./utils/helper" のような ./ プレフィックスを除去
                    let clean_path = use_path.trim_start_matches("./").to_string();
                    self.add_edge(file_path, &clean_path);
                }
            }
        }
    }

    /// DFS でサイクルを検出する
    ///
    /// 戻り値: サイクルを構成するパスのリスト（例: `["a", "b", "a"]`）
    /// サイクルがなければ空のベクタを返す。
    pub fn detect_cycles(&self) -> Vec<Vec<String>> {
        let mut cycles = Vec::new();
        let mut visited: HashSet<String> = HashSet::new();
        let mut rec_stack: Vec<String> = Vec::new();

        let nodes: Vec<String> = self.edges.keys().cloned().collect();
        for node in &nodes {
            if !visited.contains(node) {
                self.dfs_detect(node, &mut visited, &mut rec_stack, &mut cycles);
            }
        }

        cycles
    }

    fn dfs_detect(
        &self,
        node: &str,
        visited: &mut HashSet<String>,
        rec_stack: &mut Vec<String>,
        cycles: &mut Vec<Vec<String>>,
    ) {
        visited.insert(node.to_string());
        rec_stack.push(node.to_string());

        if let Some(neighbors) = self.edges.get(node) {
            for neighbor in neighbors {
                if !visited.contains(neighbor) {
                    self.dfs_detect(neighbor, visited, rec_stack, cycles);
                } else if let Some(cycle_start) = rec_stack.iter().position(|n| n == neighbor) {
                    // サイクルを検出: rec_stack の cycle_start 以降を収集し、先頭と同じノードで閉じる
                    let mut cycle: Vec<String> = rec_stack[cycle_start..].to_vec();
                    cycle.push(neighbor.to_string());
                    cycles.push(cycle);
                }
            }
        }

        rec_stack.pop();
    }

    /// グラフの全ノードを返す
    pub fn nodes(&self) -> Vec<&String> {
        self.edges.keys().collect()
    }

    /// `from` から到達できるノードのリストを返す
    pub fn neighbors(&self, from: &str) -> &[String] {
        self.edges.get(from).map(|v| v.as_slice()).unwrap_or(&[])
    }
}

/// サイクルをわかりやすいメッセージに変換する
pub fn format_cycle(cycle: &[String]) -> String {
    cycle.join(" → ")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// M-4-E: 循環グラフでサイクルを検出する
    #[test]
    fn test_circular_ref_detection() {
        let mut graph = DependencyGraph::new();
        // a → b → a の循環
        graph.add_edge("a", "b");
        graph.add_edge("b", "a");

        let cycles = graph.detect_cycles();
        assert!(!cycles.is_empty(), "サイクルが検出されるべき");

        // サイクルの中に "a" と "b" が含まれていること
        let found = cycles.iter().any(|c| {
            c.contains(&"a".to_string()) && c.contains(&"b".to_string())
        });
        assert!(found, "a → b → a のサイクルが含まれるべき");
    }

    /// M-4-E: 非循環グラフで空を返す
    #[test]
    fn test_no_cycle_detection() {
        let mut graph = DependencyGraph::new();
        // a → b → c（循環なし）
        graph.add_edge("a", "b");
        graph.add_edge("b", "c");

        let cycles = graph.detect_cycles();
        assert!(cycles.is_empty(), "サイクルが検出されないべき: {:?}", cycles);
    }

    #[test]
    fn test_three_node_cycle() {
        let mut graph = DependencyGraph::new();
        // a → b → c → a の循環
        graph.add_edge("a", "b");
        graph.add_edge("b", "c");
        graph.add_edge("c", "a");

        let cycles = graph.detect_cycles();
        assert!(!cycles.is_empty(), "3ノードサイクルが検出されるべき");
    }

    #[test]
    fn test_build_from_stmts() {
        use crate::ast::{UsePath, UseSymbols};
        use crate::lexer::Span;

        let stmts = vec![
            Stmt::UseDecl {
                path: UsePath::Local("./utils/helper".to_string()),
                symbols: UseSymbols::All,
                is_pub: false,
                span: Span { start: 0, end: 0, line: 1, col: 1 },
            },
            Stmt::UseDecl {
                path: UsePath::External("serde".to_string()),
                symbols: UseSymbols::All,
                is_pub: false,
                span: Span { start: 0, end: 0, line: 2, col: 1 },
            },
        ];

        let mut graph = DependencyGraph::new();
        graph.build_from_stmts("main", &stmts);

        // Local のみが辺として追加される
        assert_eq!(graph.neighbors("main"), &["utils/helper"]);
    }
}
