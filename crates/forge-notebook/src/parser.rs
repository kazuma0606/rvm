#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Cell {
    Code(CodeCell),
    Markdown(MarkdownCell),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeCell {
    pub index: usize,
    pub name: String,
    pub hidden: bool,
    pub skip: bool,
    pub source: String,
    pub start_line: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkdownCell {
    pub index: usize,
    pub content: String,
}

#[derive(Debug, Default)]
struct FenceAttrs {
    name: Option<String>,
    hidden: bool,
    skip: bool,
}

pub fn parse_notebook(src: &str) -> Vec<Cell> {
    let normalized = src.replace("\r\n", "\n");
    let lines: Vec<&str> = normalized.split('\n').collect();
    let mut cells = Vec::new();
    let mut markdown_lines = Vec::new();
    let mut line_index = 0;

    while line_index < lines.len() {
        let line = lines[line_index];
        if let Some(attrs) = parse_fence_open(line) {
            push_markdown_cell(&mut cells, &mut markdown_lines);

            let start_line = line_index + 1;
            line_index += 1;
            let mut code_lines = Vec::new();

            while line_index < lines.len() {
                let current = lines[line_index];
                if is_fence_close(current) {
                    line_index += 1;
                    break;
                }
                code_lines.push(current);
                line_index += 1;
            }

            let index = cells.len();
            cells.push(Cell::Code(CodeCell {
                index,
                name: attrs.name.unwrap_or_else(|| format!("cell_{index}")),
                hidden: attrs.hidden,
                skip: attrs.skip,
                source: code_lines.join("\n"),
                start_line,
            }));
            continue;
        }

        markdown_lines.push(line);
        line_index += 1;
    }

    push_markdown_cell(&mut cells, &mut markdown_lines);
    cells
}

fn push_markdown_cell(cells: &mut Vec<Cell>, markdown_lines: &mut Vec<&str>) {
    if markdown_lines.is_empty() {
        return;
    }

    let content = markdown_lines.join("\n");
    markdown_lines.clear();

    if content.trim().is_empty() {
        return;
    }

    let index = cells.len();
    cells.push(Cell::Markdown(MarkdownCell { index, content }));
}

fn parse_fence_open(line: &str) -> Option<FenceAttrs> {
    let trimmed = line.trim_start();
    let rest = trimmed.strip_prefix("```forge")?;
    let mut attrs = FenceAttrs::default();

    for token in rest.split_whitespace() {
        let (key, value) = token.split_once('=')?;
        match key {
            "name" => attrs.name = Some(unquote(value)),
            "hidden" => attrs.hidden = value == "true",
            "skip" => attrs.skip = value == "true",
            _ => {}
        }
    }

    Some(attrs)
}

fn is_fence_close(line: &str) -> bool {
    line.trim() == "```"
}

fn unquote(value: &str) -> String {
    value.trim_matches('"').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_empty_notebook() {
        let cells = parse_notebook("");
        assert!(cells.is_empty());
    }

    #[test]
    fn test_parse_code_cells() {
        let cells = parse_notebook(
            "```forge\nlet a = 1\n```\n\n```forge\nlet b = 2\n```\n\n```forge\nlet c = 3\n```",
        );

        assert_eq!(cells.len(), 3);
        assert!(matches!(&cells[0], Cell::Code(_)));
        assert!(matches!(&cells[1], Cell::Code(_)));
        assert!(matches!(&cells[2], Cell::Code(_)));
    }

    #[test]
    fn test_parse_fence_attrs() {
        let cells = parse_notebook("```forge name=\"setup\" hidden=true skip=true\nlet x = 1\n```");

        let Cell::Code(cell) = &cells[0] else {
            panic!("expected code cell");
        };
        assert_eq!(cell.name, "setup");
        assert!(cell.hidden);
        assert!(cell.skip);
    }

    #[test]
    fn test_parse_default_name() {
        let cells = parse_notebook("```forge\nlet x = 1\n```\n```forge\nlet y = 2\n```");

        let Cell::Code(first) = &cells[0] else {
            panic!("expected code cell");
        };
        let Cell::Code(second) = &cells[1] else {
            panic!("expected code cell");
        };

        assert_eq!(first.name, "cell_0");
        assert_eq!(second.name, "cell_1");
    }
}
