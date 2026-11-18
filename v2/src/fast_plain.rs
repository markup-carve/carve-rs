//! A conservative streaming path for source with only plain text blocks.
//!
//! Classification finishes before output starts. An unfamiliar spelling or
//! option leaves the input to the regular parser and renderer.

use crate::options::{HeadingIds, Options, Profile};
use crate::resolve::slug;
use crate::tables::TASK_STATES;

#[derive(Clone, Copy)]
struct Task {
    state: Option<char>,
    checked: bool,
}

#[derive(Clone, Copy)]
enum Line<'a> {
    Blank,
    Heading(u8, &'a str),
    Item(Option<Task>, &'a str),
    Text(&'a str),
}

fn plain(s: &str) -> bool {
    let b = s.as_bytes();
    let Some(first) = b.first() else {
        return false;
    };
    if !first.is_ascii_alphabetic() {
        return false;
    }
    let first_word = b.iter().take_while(|c| c.is_ascii_alphabetic()).count();
    // A letter or Roman numeral followed by a dot can open an ordered list.
    if b.get(first_word) == Some(&b'.') && b.get(first_word + 1) == Some(&b' ') {
        return false;
    }
    if b.ends_with(b" ") || b.windows(2).any(|w| w == b"..") {
        return false;
    }
    b.iter()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, b' ' | b'.' | b','))
}

fn classify(line: &str) -> Option<Line<'_>> {
    if line.is_empty() {
        return Some(Line::Blank);
    }
    if line.starts_with('#') {
        let level = line.as_bytes().iter().take_while(|b| **b == b'#').count();
        let text = line.get(level + 1..)?;
        if (1..=6).contains(&level) && line.as_bytes().get(level) == Some(&b' ') && plain(text) {
            return Some(Line::Heading(u8::try_from(level).ok()?, text));
        }
        return None;
    }
    if let Some(rest) = line.strip_prefix("- ") {
        if let Some(marked) = rest.strip_prefix('[') {
            let bytes = marked.as_bytes();
            if bytes.get(1) != Some(&b']') || bytes.get(2) != Some(&b' ') {
                return None;
            }
            let state = TASK_STATES.iter().find(|(box_char, ..)| {
                bytes.first().is_some_and(|b| char::from(*b) == *box_char)
            })?;
            let text = marked.get(3..)?;
            return plain(text).then_some(Line::Item(
                Some(Task {
                    state: state.2,
                    checked: state.3,
                }),
                text,
            ));
        }
        return plain(rest).then_some(Line::Item(None, rest));
    }
    plain(line).then_some(Line::Text(line))
}

fn lines(input: &str) -> Option<Vec<Line<'_>>> {
    let mut out = Vec::new();
    for source_line in input.split_terminator('\n') {
        out.push(classify(source_line)?);
    }
    if matches!(out.last(), Some(Line::Blank)) {
        out.pop();
    }
    if matches!(out.first(), Some(Line::Blank)) {
        return None;
    }
    for pair in out.windows(2) {
        if matches!(
            pair,
            [Line::Blank, Line::Blank]
                | [Line::Text(_), Line::Heading(..) | Line::Item(..)]
                | [
                    Line::Heading(..),
                    Line::Heading(..) | Line::Item(..) | Line::Text(_)
                ]
                | [Line::Item(..), Line::Heading(..) | Line::Text(_)]
        ) {
            return None;
        }
    }
    Some(out)
}

fn line(out: &mut String, depth: usize, content: &str) {
    if !out.is_empty() {
        out.push('\n');
    }
    for _ in 0..depth {
        out.push_str("  ");
    }
    out.push_str(content);
}

fn heading(out: &mut String, stack: &mut Vec<u8>, taken: &mut slug::Taken, level: u8, text: &str) {
    while stack.last().is_some_and(|n| *n >= level) {
        stack.pop();
        line(out, stack.len(), "</section>");
    }
    let id = taken.unique(&slug::slug(text, HeadingIds::KeepCase));
    let mut section = String::with_capacity(id.len() + 16);
    section.push_str("<section id=\"");
    section.push_str(&id);
    section.push_str("\">");
    line(out, stack.len(), &section);
    stack.push(level);
    let tag = char::from(b'0' + level);
    let mut head = String::with_capacity(text.len() + 9);
    head.push_str("<h");
    head.push(tag);
    head.push('>');
    head.push_str(text);
    head.push_str("</h");
    head.push(tag);
    head.push('>');
    line(out, stack.len(), &head);
}

fn item(out: &mut String, depth: usize, task: Option<Task>, text: &str, loose: bool) {
    let mut s = String::with_capacity(text.len() + 160);
    if let Some(task) = task {
        s.push_str("<li");
        if let Some(state) = task.state {
            s.push_str(" data-task-state=\"");
            if state == '>' {
                s.push_str("&gt;");
            } else {
                s.push(state);
            }
            s.push('"');
        }
        s.push_str(" class=\"task\" style=\"list-style: none\"><input type=\"checkbox\"");
        if task.checked {
            s.push_str(" checked");
        }
        s.push_str(" disabled aria-label=\"");
        s.push_str(text);
        s.push_str("\"> ");
    } else {
        s.push_str("<li>");
    }
    if loose {
        s.push_str("<p>");
    }
    s.push_str(text);
    if loose {
        s.push_str("</p>");
    }
    s.push_str("</li>");
    line(out, depth, &s);
}

fn paragraph(lines: &[Line<'_>], start: usize, out: &mut String, depth: usize) -> usize {
    let mut end = start;
    while matches!(lines.get(end), Some(Line::Text(_))) {
        end += 1;
    }
    let mut p = String::from("<p>");
    for (k, part) in lines.get(start..end).unwrap_or_default().iter().enumerate() {
        if k > 0 {
            p.push('\n');
        }
        if let Line::Text(text) = part {
            p.push_str(text);
        }
    }
    p.push_str("</p>");
    line(out, depth, &p);
    end
}

fn list(lines: &[Line<'_>], start: usize, out: &mut String, depth: usize) -> usize {
    let mut end = start;
    let mut loose = false;
    let mut tasks = false;
    loop {
        match lines.get(end) {
            Some(Line::Item(task, _)) => {
                tasks |= task.is_some();
                end += 1;
            }
            Some(Line::Blank) if matches!(lines.get(end + 1), Some(Line::Item(..))) => {
                loose = true;
                end += 1;
            }
            _ => break,
        }
    }
    line(
        out,
        depth,
        if tasks {
            "<ul class=\"task-list\">"
        } else {
            "<ul>"
        },
    );
    for entry in lines.get(start..end).unwrap_or_default() {
        if let Line::Item(task, text) = entry {
            item(out, depth + 1, *task, text, loose);
        }
    }
    line(out, depth, "</ul>");
    end
}

/// The HTML of a plain source when every block can be streamed safely.
pub(super) fn render(input: &str, opts: &Options) -> Option<String> {
    if !opts.sections
        || opts.heading_ids != HeadingIds::KeepCase
        || opts.profile != Profile::Full
        || opts.source_lines
        || opts.roundtrip
        || opts.includes.is_some()
    {
        return None;
    }
    let lines = lines(input)?;
    let mut out = String::with_capacity(input.len().saturating_mul(3));
    let mut stack = Vec::new();
    let mut taken = slug::Taken::default();
    let mut i = 0;
    while i < lines.len() {
        match lines.get(i).copied() {
            Some(Line::Blank) => i += 1,
            Some(Line::Heading(level, text)) => {
                heading(&mut out, &mut stack, &mut taken, level, text);
                i += 1;
            }
            Some(Line::Text(_)) => i = paragraph(&lines, i, &mut out, stack.len()),
            Some(Line::Item(..)) => i = list(&lines, i, &mut out, stack.len()),
            None => break,
        }
    }
    while stack.pop().is_some() {
        line(&mut out, stack.len(), "</section>");
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn generic(input: &str, opts: &Options) -> String {
        let arena = crate::Arena::for_input(input);
        super::super::render_parts(
            input,
            opts,
            &arena,
            &mut Vec::new(),
            &mut super::super::RenderDetails::default(),
        )
        .map_or_else(|_| "render error".to_string(), |(html, _)| html)
    }

    #[test]
    fn plain_blocks_match_the_generic_path() {
        for input in [
            "One paragraph.\nIt wraps, too.\n\nAnother paragraph.\n",
            "# A\n\nFirst.\n\n## B\n\nSecond.\n\n# A\n\nThird.\n",
            "## List\n\n- one\n- two\n\n## Task\n\n- [ ] todo\n- [x] done\n- [/] progress\n",
            "- [ ] todo\n- [x] done\n\n- plain\n",
        ] {
            let opts = Options::default();
            assert_eq!(render(input, &opts), Some(generic(input, &opts)));
        }
    }

    #[test]
    fn syntax_and_options_outside_the_subset_fall_back() {
        let opts = Options::default();
        for input in [
            "A /word/.",
            "One... two.",
            "- one\n  continuation",
            "# Heading {#id}",
            "one\n- two",
            "one  \ntwo",
            "a\n\n\nb",
            "Résumé",
            "one\r\ntwo",
        ] {
            assert!(render(input, &opts).is_none(), "{input:?}");
        }
        let opts = Options {
            roundtrip: true,
            ..Options::default()
        };
        assert!(render("Plain text", &opts).is_none());
    }
}
