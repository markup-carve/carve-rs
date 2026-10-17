use super::{walk_all_blocks, warning, LintWarning};
use crate::ast::{BlockNode, CodeBlock, Document, FigureTarget, Table};
use crate::parse::delimited_table::rows;

pub(super) fn collect(
    source: &str,
    doc: &Document,
    to_byte: &dyn Fn(usize) -> usize,
    out: &mut Vec<LintWarning>,
) {
    let mut visit = |block: &BlockNode| match block {
        BlockNode::CodeBlock(code) => malformed(
            code,
            code.attrs
                .as_ref()
                .is_some_and(|a| a.key_values.get("table").is_some_and(String::is_empty)),
            to_byte,
            out,
        ),
        BlockNode::Figure(figure) => {
            if let FigureTarget::CodeBlock(code) = &*figure.target {
                malformed(
                    code,
                    figure
                        .attrs
                        .as_ref()
                        .is_some_and(|a| a.key_values.get("table").is_some_and(String::is_empty)),
                    to_byte,
                    out,
                );
            }
        }
        BlockNode::Table(table) if table.source_format.is_some() => {
            wide(table, source, to_byte, out)
        }
        _ => {}
    };
    walk_all_blocks(&doc.children, &mut visit);
    for body in doc.footnote_defs.values() {
        walk_all_blocks(body, &mut visit);
    }
}

fn malformed(
    code: &CodeBlock,
    marked: bool,
    to_byte: &dyn Fn(usize) -> usize,
    out: &mut Vec<LintWarning>,
) {
    if !marked {
        return;
    }
    let separator = match code.lang.as_deref() {
        Some("csv") => ',',
        Some("tsv") => '\t',
        _ => return,
    };
    if rows(&code.content, separator).is_some() {
        return;
    }
    out.push(warning(
        code.pos.clone(),
        to_byte,
        "delimited-table-malformed",
        "This marked CSV/TSV fence has malformed quoting, so it remains a code block.".into(),
    ));
}

fn wide(table: &Table, source: &str, to_byte: &dyn Fn(usize) -> usize, out: &mut Vec<LintWarning>) {
    let Some(pos) = &table.pos else {
        return;
    };
    let Some(segment) = source.get(to_byte(pos.start_offset)..to_byte(pos.end_offset)) else {
        return;
    };
    let segment = segment.replace("\r\n", "\n").replace('\r', "\n");
    let mut lines = segment.lines();
    let Some(opener) = lines.find(|line| {
        let Some(start) = line.find(['`', '~']) else {
            return false;
        };
        let fence = &line[start..];
        let marker = fence.chars().next().unwrap_or(' ');
        let width = fence.chars().take_while(|ch| *ch == marker).count();
        width >= 3 && fence[width..].split_whitespace().next() == table.source_format.as_deref()
    }) else {
        return;
    };
    let Some(start) = opener.find(['`', '~']) else {
        return;
    };
    let prefix = &opener[..start];
    let quoted_prefix = lines
        .clone()
        .next()
        .map(|line| {
            let mut bytes = 0;
            while line[bytes..].starts_with("> ") {
                bytes += 2;
            }
            &line[..bytes]
        })
        .unwrap_or("");
    let marker = opener[start..].chars().next().unwrap_or(' ');
    let width = opener[start..]
        .chars()
        .take_while(|ch| *ch == marker)
        .count();
    let quote_has_closer = !quoted_prefix.is_empty()
        && lines.clone().any(|line| {
            let Some(rest) = line.strip_prefix(quoted_prefix) else {
                return false;
            };
            let rest = rest.trim();
            rest.len() >= width && rest.chars().all(|ch| ch == marker)
        });
    let prefix = if prefix.is_empty() && quote_has_closer {
        quoted_prefix
    } else {
        prefix
    };
    let mut payload = String::new();
    for line in lines {
        let line = line.strip_prefix(prefix).unwrap_or(line);
        let trimmed = line.trim();
        if trimmed.len() >= width && trimmed.chars().all(|ch| ch == marker) {
            break;
        }
        payload.push_str(line);
        payload.push('\n');
    }
    let separator = if table.source_format.as_deref() == Some("csv") {
        ','
    } else {
        '\t'
    };
    let Some(records) = rows(&payload, separator) else {
        return;
    };
    let Some(first) = records.first() else {
        return;
    };
    if records.iter().any(|row| row.len() > first.len()) {
        out.push(warning(table.pos.clone(), to_byte, "delimited-table-wide-row",
            "A CSV/TSV record has more fields than the first record; the table keeps the extra columns.".into()));
    }
}

#[cfg(test)]
mod tests {
    use crate::lint::lint_carve;

    #[test]
    fn reports_malformed_and_wide_records() {
        let rules = |source| {
            lint_carve(source)
                .into_iter()
                .map(|w| w.rule)
                .collect::<Vec<_>>()
        };
        assert!(rules("{table}\n```csv\n\"bad\n```\n").contains(&"delimited-table-malformed"));
        assert!(
            rules("{table}\n```csv\na,b\n1\n2,3,4\n```\n").contains(&"delimited-table-wide-row")
        );
        assert!(!rules("{table}\n```csv\na,b\n1,2\n```\n")
            .iter()
            .any(|rule| rule.starts_with("delimited-table-")));
        assert!(rules("> {table}\n> ```csv\n> \"x,y\",b\n> 1,2,3\n> ```\n")
            .contains(&"delimited-table-wide-row"));
        assert!(rules("{table}\n```csv\n> x,y\n1,2,3\n```\n").contains(&"delimited-table-wide-row"));
        assert!(!rules("{table}\n```csv\na,b\n1,2\n`````\n^ x,y,z\n")
            .iter()
            .any(|rule| rule.starts_with("delimited-table-")));
    }
}
