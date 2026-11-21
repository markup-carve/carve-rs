//! A conservative streaming path for source with only plain text blocks:
//! paragraphs, headings and tight or flat loose bullet lists whose lines are
//! plain words, with the `*` pairs and code spans the direct inline route
//! decides inside paragraph and item lines.
//!
//! Classification finishes before output starts. An unfamiliar spelling or
//! option leaves the input to the regular parser and renderer.

use crate::inline::{Seg, line_segments};
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
    /// The nesting level (two spaces each), the task box, the text and
    /// whether it holds marks.
    Item(usize, Option<Task>, &'a str, bool),
    Text(&'a str, bool),
}

fn is_mark(c: u8) -> bool {
    matches!(c, b'*' | b'`')
}

/// Bytes of a plain line: letters, digits, inner spaces, `.` and `,`.
static WORDS: crate::chars::ByteSet = crate::chars::ByteSet::new(
    b"0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ",
    b" .,",
    None,
);

/// Whether a line is plain words; with `marks`, also `*` pairs and code
/// spans the inline route decides on this line alone. `Some(true)` when it
/// holds such marks.
fn plain(s: &str, marks: bool, segs: &mut Vec<Seg>) -> Option<bool> {
    let b = s.as_bytes();
    if !b.first()?.is_ascii_alphabetic() {
        return None;
    }
    let first_word = b.iter().take_while(|c| c.is_ascii_alphabetic()).count();
    // A letter or Roman numeral followed by a dot can open an ordered list.
    if b.get(first_word) == Some(&b'.') && b.get(first_word + 1) == Some(&b' ') {
        return None;
    }
    if b.ends_with(b" ") {
        return None;
    }
    let mut marked = false;
    let mut prev = 0;
    for &c in b {
        if WORDS.has(c) {
            if c == b'.' && prev == b'.' {
                return None;
            }
        } else if is_mark(c) {
            marked = true;
        } else {
            return None;
        }
        prev = c;
    }
    if marked && !(marks && line_segments(b, segs).is_some()) {
        return None;
    }
    Some(marked)
}

fn classify<'a>(line: &'a str, segs: &mut Vec<Seg>) -> Option<Line<'a>> {
    if line.is_empty() {
        return Some(Line::Blank);
    }
    if line.starts_with('#') {
        return heading_line(line, segs);
    }
    let indent = line.bytes().take_while(|&b| b == b' ').count();
    if indent % 2 == 0
        && let Some(rest) = line.get(indent..).and_then(|l| l.strip_prefix("- "))
    {
        return item_line(indent / 2, rest, segs);
    }
    Some(Line::Text(line, plain(line, true, segs)?))
}

fn heading_line<'a>(line: &'a str, segs: &mut Vec<Seg>) -> Option<Line<'a>> {
    let level = line.as_bytes().iter().take_while(|b| **b == b'#').count();
    let text = line.get(level + 1..)?;
    if (1..=6).contains(&level)
        && line.as_bytes().get(level) == Some(&b' ')
        && plain(text, false, segs).is_some()
    {
        return Some(Line::Heading(u8::try_from(level).ok()?, text));
    }
    None
}

fn item_line<'a>(level: usize, rest: &'a str, segs: &mut Vec<Seg>) -> Option<Line<'a>> {
    let Some(marked) = rest.strip_prefix('[') else {
        return Some(Line::Item(level, None, rest, plain(rest, true, segs)?));
    };
    let bytes = marked.as_bytes();
    if bytes.get(1) != Some(&b']') || bytes.get(2) != Some(&b' ') {
        return None;
    }
    let state = TASK_STATES
        .iter()
        .find(|(box_char, ..)| bytes.first().is_some_and(|b| char::from(*b) == *box_char))?;
    let text = marked.get(3..)?;
    let task = Task {
        state: state.2,
        checked: state.3,
    };
    Some(Line::Item(
        level,
        Some(task),
        text,
        plain(text, true, segs)?,
    ))
}

fn lines<'a>(input: &'a str, segs: &mut Vec<Seg>) -> Option<Vec<Line<'a>>> {
    let mut out = Vec::new();
    for source_line in input.split_terminator('\n') {
        out.push(classify(source_line, segs)?);
    }
    if matches!(out.last(), Some(Line::Blank)) {
        out.pop();
    }
    if matches!(out.first(), Some(Line::Blank | Line::Item(1.., ..))) {
        return None;
    }
    for pair in out.windows(2) {
        let nested = match pair {
            [Line::Item(a, ..), Line::Item(b, ..)] => *b > a + 1,
            // A blank line in a nested list or between an item and its sublist.
            [Line::Item(a, ..), Line::Blank] | [Line::Blank, Line::Item(a, ..)] => *a > 0,
            _ => false,
        };
        if nested
            || matches!(
                pair,
                [Line::Blank, Line::Blank]
                    | [Line::Text(..), Line::Heading(..) | Line::Item(..)]
                    | [
                        Line::Heading(..),
                        Line::Heading(..) | Line::Item(..) | Line::Text(..)
                    ]
                    | [Line::Item(..), Line::Heading(..) | Line::Text(..)]
            )
        {
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

/// The HTML of a classified line: its text with the pairs and code spans.
fn inline(s: &mut String, text: &str, marked: bool, segs: &mut Vec<Seg>) -> Option<()> {
    if !marked {
        s.push_str(text);
        return Some(());
    }
    line_segments(text.as_bytes(), segs)?;
    for &seg in segs.iter() {
        match seg {
            Seg::Text(a, b) => s.push_str(text.get(a..b)?),
            Seg::Pair(c, o, k) => {
                let tag = if c == b'*' { "strong" } else { "em" };
                s.push('<');
                s.push_str(tag);
                s.push('>');
                s.push_str(text.get(o + 1..k)?);
                s.push_str("</");
                s.push_str(tag);
                s.push('>');
            }
            Seg::Code(_, _, v0, v1) => {
                s.push_str("<code>");
                s.push_str(text.get(v0..v1)?);
                s.push_str("</code>");
            }
            Seg::Break(..) => return None,
        }
    }
    Some(())
}

/// The text content of a classified line, as the task box label.
fn label(s: &mut String, text: &str, marked: bool, segs: &mut Vec<Seg>) -> Option<()> {
    if !marked {
        s.push_str(text);
        return Some(());
    }
    line_segments(text.as_bytes(), segs)?;
    for &seg in segs.iter() {
        let (a, b) = match seg {
            Seg::Text(a, b) | Seg::Code(_, _, a, b) => (a, b),
            Seg::Pair(_, o, k) => (o + 1, k),
            Seg::Break(..) => return None,
        };
        s.push_str(text.get(a..b)?);
    }
    Some(())
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

/// The opening of an item up to the end of its text; the caller closes it.
fn item(
    task: Option<Task>,
    text: &str,
    marked: bool,
    loose: bool,
    segs: &mut Vec<Seg>,
) -> Option<String> {
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
        label(&mut s, text, marked, segs)?;
        s.push_str("\"> ");
    } else {
        s.push_str("<li>");
    }
    if loose {
        s.push_str("<p>");
    }
    inline(&mut s, text, marked, segs)?;
    if loose {
        s.push_str("</p>");
    }
    Some(s)
}

fn paragraph(
    lines: &[Line<'_>],
    start: usize,
    out: &mut String,
    depth: usize,
    segs: &mut Vec<Seg>,
) -> Option<usize> {
    let mut end = start;
    while matches!(lines.get(end), Some(Line::Text(..))) {
        end += 1;
    }
    let mut p = String::from("<p>");
    for (k, part) in lines.get(start..end).unwrap_or_default().iter().enumerate() {
        if k > 0 {
            p.push('\n');
        }
        if let Line::Text(text, marked) = part {
            inline(&mut p, text, *marked, segs)?;
        }
    }
    p.push_str("</p>");
    line(out, depth, &p);
    Some(end)
}

struct Extent {
    end: usize,
    loose: bool,
    tasks: bool,
}

/// Where the list at `level` from `start` ends, whether it is loose and
/// whether it holds a task item of its own. A loose list with a sublist is
/// left to the generic path.
fn extent(lines: &[Line<'_>], start: usize, level: usize) -> Option<Extent> {
    let mut end = start;
    let mut loose = false;
    let mut tasks = false;
    let mut nested = false;
    loop {
        match lines.get(end) {
            Some(Line::Item(l, task, ..)) if *l >= level => {
                tasks |= *l == level && task.is_some();
                nested |= *l > level;
            }
            Some(Line::Blank) if matches!(lines.get(end + 1), Some(Line::Item(l, ..)) if *l >= level) =>
            {
                loose = true;
            }
            _ => break,
        }
        end += 1;
    }
    (!(loose && nested)).then_some(Extent { end, loose, tasks })
}

/// The list of the items at `level` from `start`, with the lists nested in
/// them; `depth` is the indent of its `<ul>`.
fn list(
    lines: &[Line<'_>],
    start: usize,
    out: &mut String,
    depth: usize,
    level: usize,
    segs: &mut Vec<Seg>,
) -> Option<usize> {
    let Extent { end, loose, tasks } = extent(lines, start, level)?;
    line(
        out,
        depth,
        if tasks {
            "<ul class=\"task-list\">"
        } else {
            "<ul>"
        },
    );
    let mut i = start;
    while i < end {
        let Some(&Line::Item(l, task, text, marked)) = lines.get(i) else {
            i += 1;
            continue;
        };
        if l != level {
            return None;
        }
        let mut s = item(task, text, marked, loose, segs)?;
        i += 1;
        if matches!(lines.get(i), Some(Line::Item(l, ..)) if *l > level) {
            line(out, depth + 1, &s);
            i = list(lines, i, out, depth + 2, level + 1, segs)?;
            line(out, depth + 1, "</li>");
        } else {
            s.push_str("</li>");
            line(out, depth + 1, &s);
        }
    }
    line(out, depth, "</ul>");
    Some(end)
}

fn streamable(opts: &Options) -> bool {
    opts.sections
        && opts.heading_ids == HeadingIds::KeepCase
        && opts.profile == Profile::Full
        && !opts.source_lines
        && !opts.roundtrip
        && opts.includes.is_none()
        && opts.soft_break == crate::options::SoftBreak::Newline
}

/// The HTML of a plain source when every block can be streamed safely.
pub(super) fn render(input: &str, opts: &Options) -> Option<String> {
    if !streamable(opts) {
        return None;
    }
    let mut segs = Vec::new();
    let lines = lines(input, &mut segs)?;
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
            Some(Line::Text(..)) => i = paragraph(&lines, i, &mut out, stack.len(), &mut segs)?,
            Some(Line::Item(..)) => i = list(&lines, i, &mut out, stack.len(), 0, &mut segs)?,
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
    use std::fmt::Write as _;

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

    #[test]
    fn nested_lists_and_simple_inline_match_the_generic_path() {
        let opts = Options::default();
        for input in [
            "- item 0 alpha *em* `c0`\n  - nested alpha\n  - nested two\n- item 1 `c1`\n",
            "- a\n  - b\n    - c\n  - d\n- e\n",
            "- [ ] task *a* and `c d`\n  - [x] sub\n- plain\n",
            "Para *a* `b`\nline two, done.\n\n# H\n\n- one\n\n- two\n",
        ] {
            assert_eq!(
                render(input, &opts),
                Some(generic(input, &opts)),
                "{input:?}"
            );
        }
    }

    #[test]
    fn a_long_nested_list_streams() {
        let opts = Options::default();
        let mut input = String::new();
        for i in 0..200 {
            let _ = write!(
                input,
                "- item {i} alpha delta *em* `c{i}`\n  - nested alpha\n  - nested two\n- item {i} theta *em* `c{i}`\n"
            );
        }
        assert_eq!(render(&input, &opts), Some(generic(&input, &opts)));
    }

    #[test]
    fn generated_lists_match_the_generic_path() {
        const PARTS: [&str; 14] = [
            "alpha", "b2", " ", " ", "*em*", "`c`", "` c `", "*", "`", ".", ",", "..", "/x/",
            "*a b*",
        ];
        const PREFIX: [&str; 9] = [
            "- ", "  - ", "    - ", "   - ", "- [ ] ", "  - [x] ", "", "# ", "  ",
        ];
        let opts = Options::default();
        let mut seed = 0x9e37_79b9_7f4a_7c15_u64;
        let mut next = || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            usize::try_from(seed % 1024).unwrap_or(0)
        };
        let (mut streamed, mut total) = (0, 0);
        for _ in 0..3000 {
            let mut input = String::new();
            for _ in 0..1 + next() % 6 {
                if next() % 9 == 0 {
                    input.push('\n');
                    continue;
                }
                input.push_str(
                    PREFIX
                        .get(next() % PREFIX.len())
                        .copied()
                        .unwrap_or_default(),
                );
                input.push_str("word");
                for _ in 0..next() % 5 {
                    input.push_str(PARTS.get(next() % PARTS.len()).copied().unwrap_or_default());
                }
                input.push('\n');
            }
            total += 1;
            if let Some(html) = render(&input, &opts) {
                streamed += 1;
                assert_eq!(html, generic(&input, &opts), "{input:?}");
            }
        }
        assert!(streamed > 100, "{streamed} of {total}");
    }

    #[test]
    fn nested_shapes_outside_the_subset_fall_back() {
        let opts = Options::default();
        for input in [
            "- a\n\n  - b\n",
            "- a\n  - b\n\n- c\n",
            "- a\n    - b\n",
            "- a\n   - b\n",
            "  - a\n",
            "- a *b\n",
            "- a **b**\n",
            "# A *b*\n",
        ] {
            assert!(render(input, &opts).is_none(), "{input:?}");
        }
    }
}
