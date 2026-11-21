//! A delimited table: a `csv` or `tsv` fence that the `table` attribute marks
//! (C2-B-180, C2-B-181, C2-B-209, C2-B-221, C2-A-215). The record syntax is
//! not stated by the clauses (gap G-008): fields follow RFC 4180 quoting.

use crate::Arena;
use crate::ast::{self, Node, owned::Attrs};

/// Is this code fence a delimited table, and with which separator?
pub(super) fn separator(language: Option<&str>, attrs: Option<&Attrs>) -> Option<char> {
    let sep = match language? {
        "csv" => ',',
        "tsv" => '\t',
        _ => return None,
    };
    attrs?
        .key_values
        .iter()
        .any(|(k, _)| k.eq_ignore_ascii_case("table"))
        .then_some(sep)
}

/// The records of a fence body; the final line ending is not part of a cell (C2-B-181).
fn records(body: &str, sep: char) -> Vec<Vec<String>> {
    let body = body.strip_suffix('\n').unwrap_or(body);
    if body.is_empty() {
        return Vec::new();
    }
    let mut rows = vec![Vec::new()];
    let mut cur = String::new();
    let mut it = body.chars().peekable();
    let mut at_start = true;
    while let Some(c) = it.next() {
        if at_start && c == '"' {
            quoted(&mut it, &mut cur);
            at_start = false;
            continue;
        }
        at_start = false;
        if c == sep || c == '\n' {
            if let Some(r) = rows.last_mut() {
                r.push(std::mem::take(&mut cur));
            }
            if c == '\n' {
                rows.push(Vec::new());
            }
            at_start = true;
        } else {
            cur.push(c);
        }
    }
    if let Some(r) = rows.last_mut() {
        r.push(cur);
    }
    // Gap G-008: as in the oracle, one empty last line of the body is no record.
    if body.ends_with('\n') && rows.len() > 1 && rows.last().is_some_and(|r| r == &[""]) {
        rows.pop();
    }
    rows
}

/// C2-B-181: whether a quote of the body is left open, which runs to the end.
pub(super) fn unclosed_quote_offset(body: &str, sep: char) -> Option<usize> {
    let mut inside = false;
    let mut at_start = true;
    let mut opened = None;
    let mut it = body.char_indices().peekable();
    while let Some((offset, c)) = it.next() {
        if inside {
            if c == '"' && it.peek().is_some_and(|(_, next)| *next == '"') {
                it.next();
            } else if c == '"' {
                inside = false;
                opened = None;
            }
        } else if at_start && c == '"' {
            inside = true;
            opened = Some(offset);
        } else {
            at_start = c == sep || c == '\n';
            continue;
        }
        at_start = false;
    }
    if inside { opened } else { None }
}

/// A quoted field: `""` is one quote; an unclosed quote runs to the end (C2-B-181).
fn quoted(it: &mut std::iter::Peekable<std::str::Chars<'_>>, cur: &mut String) {
    while let Some(c) = it.next() {
        if c == '"' {
            if it.peek() == Some(&'"') {
                it.next();
                cur.push('"');
            } else {
                return;
            }
        } else {
            cur.push(c);
        }
    }
}

fn row<'a>(a: &'a Arena, cells: &[String], header: bool) -> Node<'a> {
    let children = a.fill(cells.iter().map(|v| {
        let text = if v.is_empty() {
            &mut []
        } else {
            a.one(Node::Text(a.alloc(ast::Text {
                value: a.str(v),
                ..Default::default()
            })))
        };
        Node::TableCell(a.alloc(ast::TableCell {
            header,
            children: a.one(Node::CellParagraph(a.alloc(ast::CellParagraph {
                children: text,
                ..Default::default()
            }))),
            ..Default::default()
        }))
    }));
    Node::TableRow(a.alloc(ast::TableRow {
        children,
        ..Default::default()
    }))
}

/// The table node; `declared` is the `header-rows` count when it is one (C2-B-210).
pub(super) fn table<'a>(
    a: &'a Arena,
    body: &str,
    sep: char,
    declared: Option<usize>,
    footer: usize,
) -> (&'a mut [Node<'a>], usize) {
    let rows = records(body, sep);
    let n = rows.len();
    let ncols = rows.iter().map(Vec::len).max().unwrap_or(0);
    let head = declared.unwrap_or(1).min(n);
    let foot = footer.min(n - head);
    let mut groups = Vec::new();
    if head > 0 {
        let children = a.fill(rows.iter().take(head).map(|r| row(a, r, true)));
        groups.push(Node::TableHead(a.alloc(ast::TableHead {
            children,
            ..Default::default()
        })));
    }
    if n - foot > head {
        let children = a.fill(
            rows.iter()
                .take(n - foot)
                .skip(head)
                .map(|r| row(a, r, false)),
        );
        groups.push(Node::TableBody(a.alloc(ast::TableBody {
            children,
            ..Default::default()
        })));
    }
    if foot > 0 {
        let children = a.fill(rows.iter().skip(n - foot).map(|r| row(a, r, false)));
        groups.push(Node::TableFoot(a.alloc(ast::TableFoot {
            children,
            ..Default::default()
        })));
    }
    (a.nodes(&mut groups), ncols)
}

#[cfg(test)]
mod tests {
    use super::records;

    #[test]
    fn c2_b_181_unclosed_quote_runs_to_end() {
        assert_eq!(
            records("a,b\n1,\"2\n3,4\n", ','),
            vec![vec!["a", "b"], vec!["1", "2\n3,4"]]
        );
    }

    #[test]
    fn c2_b_181_unclosed_quote_is_found() {
        assert!(super::unclosed_quote_offset("a,b\n1,\"2\n3,4\n", ',').is_some());
        assert!(super::unclosed_quote_offset("a,\"b \"\"q\"\" c\"\n", ',').is_none());
        assert!(super::unclosed_quote_offset("a\"b\n", ',').is_none());
    }

    #[test]
    fn c2_b_180_doubled_quote() {
        assert_eq!(
            records("\"b \"\"q\"\" c\", d \n", ','),
            vec![vec!["b \"q\" c", " d "]]
        );
    }
}
