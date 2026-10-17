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
        let line = line.trim_start();
        let marker = line.chars().next().unwrap_or(' ');
        let width = line.chars().take_while(|ch| *ch == marker).count();
        (marker == '`' || marker == '~')
            && width >= 3
            && line[width..].split_whitespace().next() == table.source_format.as_deref()
    }) else {
        return;
    };
    let opener = opener.trim_start();
    let marker = opener.chars().next().unwrap_or(' ');
    let width = opener.chars().take_while(|ch| *ch == marker).count();
    let fence = marker.to_string().repeat(width);
    let mut payload = String::new();
    for line in lines {
        if line.trim() == fence {
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
    }
}
