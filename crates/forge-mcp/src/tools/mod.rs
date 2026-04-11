// forge-mcp: ツールモジュール

pub mod get_spec_section;
pub mod parse_file;
pub mod run_snippet;
pub mod search_symbol;
pub mod type_check;

use serde_json::Value;

/// ツール定義リスト（tools/list レスポンス用）
pub fn tool_list() -> Value {
    serde_json::json!([
        {
            "name": "parse_file",
            "description": "ForgeScript ファイルをパースして構文エラーを検出する",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "パースするファイルのパス"
                    }
                },
                "required": ["path"]
            }
        },
        {
            "name": "type_check",
            "description": "ForgeScript ファイルの型チェックを実行する",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "型チェックするファイルのパス"
                    }
                },
                "required": ["path"]
            }
        },
        {
            "name": "run_snippet",
            "description": "ForgeScript コードスニペットを実行する",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "code": {
                        "type": "string",
                        "description": "実行する ForgeScript コード"
                    }
                },
                "required": ["code"]
            }
        },
        {
            "name": "search_symbol",
            "description": "カレントディレクトリの .forge/.fg ファイルからシンボルを検索する",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "検索するシンボル名（部分一致）"
                    },
                    "kind": {
                        "type": "string",
                        "description": "シンボルの種類（fn / struct / enum）",
                        "enum": ["fn", "struct", "enum"]
                    }
                },
                "required": ["name"]
            }
        },
        {
            "name": "get_spec_section",
            "description": "ForgeScript 言語仕様からセクションを取得する",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "section": {
                        "type": "string",
                        "description": "取得するセクションのキーワード"
                    }
                },
                "required": ["section"]
            }
        }
    ])
}

/// ツールを名前で呼び出す
pub fn dispatch(tool_name: &str, args: &Value) -> Value {
    match tool_name {
        "parse_file" => parse_file::call(args),
        "type_check" => type_check::call(args),
        "run_snippet" => run_snippet::call(args),
        "search_symbol" => search_symbol::call(args),
        "get_spec_section" => get_spec_section::call(args),
        _ => {
            let result = serde_json::json!({"error": format!("不明なツール: {}", tool_name)});
            serde_json::json!({
                "content": [{"type": "text", "text": result.to_string()}]
            })
        }
    }
}
