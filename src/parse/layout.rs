use super::*;
use std::ops::Range;

/// A block family accepted by the borrowed layout scanner.
///
/// Keeping this taxonomy typed makes every widening measurable: a new scanner
/// branch must explicitly publish the family it accepted, rather than merely
/// making the fallback rate move for an unknown reason.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LayoutEvent {
    Heading,
    Paragraph,
    BlockQuote,
    CodeFence,
    UnorderedListItem,
    OrderedListItem,
    TableRow,
    LinkDefinition,
}

/// One accepted block-layout decision.
///
/// `consumed` and `active_definition` are intentionally independent. A link
/// definition consumes a source line and changes later inline resolution,
/// while an ordinary block only consumes source. Keeping those facts separate
/// prevents a future widening from treating "seen" as "active".
#[derive(Clone, Debug, Eq, PartialEq)]
struct BlockLayout {
    event: LayoutEvent,
    consumed: Range<usize>,
    active_definition: bool,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct AcceptanceCounters {
    headings: usize,
    paragraphs: usize,
    block_quotes: usize,
    code_fences: usize,
    unordered_list_items: usize,
    ordered_list_items: usize,
    table_rows: usize,
    link_definitions: usize,
    consumed_lines: usize,
    active_definitions: usize,
}

impl AcceptanceCounters {
    fn record(&mut self, layout: BlockLayout) {
        debug_assert!(layout.consumed.start <= layout.consumed.end);
        debug_assert_eq!(
            layout.active_definition,
            layout.event == LayoutEvent::LinkDefinition
        );
        self.consumed_lines += layout.consumed.end - layout.consumed.start;
        self.active_definitions += usize::from(layout.active_definition);
        let counter = match layout.event {
            LayoutEvent::Heading => &mut self.headings,
            LayoutEvent::Paragraph => &mut self.paragraphs,
            LayoutEvent::BlockQuote => &mut self.block_quotes,
            LayoutEvent::CodeFence => &mut self.code_fences,
            LayoutEvent::UnorderedListItem => &mut self.unordered_list_items,
            LayoutEvent::OrderedListItem => &mut self.ordered_list_items,
            LayoutEvent::TableRow => &mut self.table_rows,
            LayoutEvent::LinkDefinition => &mut self.link_definitions,
        };
        *counter += 1;
    }
}

struct LayoutOutput {
    html: String,
    accepted: AcceptanceCounters,
}

/// Borrowed block-layout facade for the common, stateless core grammar.
///
/// It is deliberately conservative: any shape whose block boundaries require
/// the full collector returns `None` before publishing output, and `to_html`
/// falls back to the authoritative AST pipeline. Inline syntax is never
/// reimplemented; accepted events use the normal inline parser and renderer.
pub(crate) fn try_layout_html(source: &str, options: &Options<'_>) -> Option<String> {
    try_layout(source, options).map(|output| output.html)
}

fn try_layout(source: &str, options: &Options<'_>) -> Option<LayoutOutput> {
    if !source.is_ascii()
        || source.contains(['\0', '\t', '\u{0b}', '\u{0c}', '\r'])
        || source.starts_with("---")
        || source.contains("[^")
        || source.contains("[@")
        || source.contains("</#")
        || source.contains("![")
        || source.contains("%%")
        || source.contains(":::")
    {
        return None;
    }

    let lines: Vec<&str> = source.lines().collect();
    if lines
        .iter()
        .any(|line| trim_ascii_end(line).len() != line.len())
    {
        return None;
    }
    let (defs, definition_lines) = layout_link_defs(&lines)?;
    let (rendered, _, _) =
        with_active_link_defs(defs, || render_layout_body(&lines, source.len(), options));
    let mut output = rendered?;
    for line in definition_lines {
        output.accepted.record(BlockLayout {
            event: LayoutEvent::LinkDefinition,
            consumed: line..line + 1,
            active_definition: true,
        });
    }
    Some(output)
}

fn layout_link_defs(lines: &[&str]) -> Option<(BTreeMap<String, LinkDef>, Vec<usize>)> {
    let mut defs = BTreeMap::new();
    let mut definition_lines = Vec::new();
    let Some(last_candidate) = lines.iter().rposition(|line| line.contains("]:")) else {
        return Some((defs, definition_lines));
    };
    let mut fence: Option<FenceOpen> = None;
    for (index, line) in lines[..=last_candidate].iter().copied().enumerate() {
        if let Some(open) = fence {
            if is_fence_close(line, open) {
                fence = None;
            }
            continue;
        }
        if let Some(open) = detect_fence_open(line) {
            fence = Some(open);
            continue;
        }
        if !line.contains("]:") {
            continue;
        }
        let (label, target) = parse_link_def_line(line)?;
        if label.starts_with('@') || target.trim().is_empty() {
            return None;
        }
        // This facade currently accepts definitions as standalone top-level
        // blocks. Hosted/lazy definitions retain the authoritative collector.
        if index > 0 && !is_blank_line(lines[index - 1]) {
            return None;
        }
        if index + 1 < lines.len() && !is_blank_line(lines[index + 1]) {
            return None;
        }
        let def = parse_link_def_target_with_attrs(target.trim());
        if def.attrs.is_some() {
            return None;
        }
        defs.insert(label.to_string(), def);
        definition_lines.push(index);
    }
    Some((defs, definition_lines))
}

fn render_layout_body(
    lines: &[&str],
    source_len: usize,
    options: &Options<'_>,
) -> Option<LayoutOutput> {
    // Section wrappers and indentation make representative HTML about 2.7x
    // the source size. Reserve once so the hot path does not copy the entire
    // nearly-complete document during geometric growth.
    let mut out = String::with_capacity(source_len.saturating_mul(3));
    let mut sections: Vec<usize> = Vec::new();
    let mut heading_counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut accepted = AcceptanceCounters::default();
    let mut i = 0;
    let mut wrote = false;
    while i < lines.len() {
        let line = lines[i];
        if is_blank_line(line) || is_definition_placeholder(line) {
            i += 1;
            continue;
        }
        if parse_link_def_line(line).is_some() {
            i += 1;
            continue;
        }
        if line.starts_with('{') || line.starts_with("::") {
            return None;
        }
        if let Some((level, text)) = detect_heading(line) {
            let section_level = usize::from(level);
            while sections.last().is_some_and(|open| *open >= section_level) {
                let depth = sections.len() - 1;
                out.push('\n');
                layout_indent(&mut out, depth);
                out.push_str("</section>");
                sections.pop();
            }
            if wrote {
                out.push('\n');
            }
            let title = trim_ascii_end(text);
            // Markup-bearing headings need the authoritative derived-display
            // clone. Plain headings can slug and render directly as borrowed
            // events without building an inline tree solely to flatten it.
            if title
                .bytes()
                .any(|byte| matches!(byte, b'*' | b'/' | b'`' | b'['))
                || title.matches(':').count() >= 2
            {
                return None;
            }
            let base = slugify_parse(title, options.lowercase_heading_ids);
            let count = heading_counts.entry(base.clone()).or_insert(0);
            *count += 1;
            let id = if *count == 1 {
                base
            } else {
                format!("{base}-{}", *count)
            };
            layout_indent(&mut out, sections.len());
            out.push_str("<section id=\"");
            out.push_str(&crate::escape::escape_attr(&id));
            out.push_str("\">\n");
            layout_indent(&mut out, sections.len() + 1);
            use std::fmt::Write as _;
            write!(&mut out, "<h{level}>").ok()?;
            render_layout_inline(&mut out, title, options)?;
            write!(&mut out, "</h{level}>").ok()?;
            sections.push(section_level);
            accepted.record(BlockLayout {
                event: LayoutEvent::Heading,
                consumed: i..i + 1,
                active_definition: false,
            });
            wrote = true;
            i += 1;
            continue;
        }

        if wrote {
            out.push('\n');
        }
        let depth = sections.len();
        if let Some(open) = detect_fence_open(line) {
            if open.fence_char != b'`' || line.starts_with([' ', '>']) {
                return None;
            }
            let close = lines[i + 1..]
                .iter()
                .position(|candidate| is_fence_close(candidate, open))?
                + i
                + 1;
            layout_indent(&mut out, depth);
            out.push_str("<pre><code");
            let info = line[open.fence_len..].trim();
            if !info.is_empty() {
                if !info.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') {
                    return None;
                }
                out.push_str(" class=\"language-");
                out.push_str(info);
                out.push('"');
            }
            out.push('>');
            for content in &lines[i + 1..close] {
                crate::escape::write_escaped_text(&mut out, content);
                out.push('\n');
            }
            out.push_str("</code></pre>");
            accepted.record(BlockLayout {
                event: LayoutEvent::CodeFence,
                consumed: i..close + 1,
                active_definition: false,
            });
            i = close + 1;
            wrote = true;
            continue;
        }
        if line.starts_with("- ") {
            i = render_layout_list(lines, i, 0, depth, options, &mut out, &mut accepted)?;
            wrote = true;
            continue;
        }
        if decimal_list_item(line).is_some() {
            i = render_layout_ordered_list(lines, i, depth, options, &mut out, &mut accepted)?;
            wrote = true;
            continue;
        }
        if line.starts_with("> ") {
            layout_indent(&mut out, depth);
            out.push_str("<blockquote><p>");
            let mut end = i;
            while let Some(text) = lines[end].strip_prefix("> ") {
                if !is_layout_quote_line(text) {
                    return None;
                }
                if end > i {
                    out.push('\n');
                }
                render_layout_inline(&mut out, text, options)?;
                end += 1;
                if end == lines.len() {
                    break;
                }
            }
            if lines.get(end).is_some_and(|next| !is_blank_line(next)) {
                // A dedented nonblank line may lazily continue the quote.
                return None;
            }
            out.push_str("</p></blockquote>");
            accepted.record(BlockLayout {
                event: LayoutEvent::BlockQuote,
                consumed: i..end,
                active_definition: false,
            });
            i = end;
            wrote = true;
            continue;
        }
        if line.starts_with('|') {
            i = render_layout_table(lines, i, depth, options, &mut out, &mut accepted)?;
            wrote = true;
            continue;
        }
        if !is_layout_paragraph_line(line) {
            return None;
        }
        layout_indent(&mut out, depth);
        out.push_str("<p>");
        let mut end = i;
        while end < lines.len() && !is_blank_line(lines[end]) {
            if !is_layout_paragraph_line(lines[end]) {
                return None;
            }
            if end > i {
                out.push('\n');
            }
            render_layout_inline(&mut out, lines[end], options)?;
            end += 1;
        }
        out.push_str("</p>");
        accepted.record(BlockLayout {
            event: LayoutEvent::Paragraph,
            consumed: i..end,
            active_definition: false,
        });
        i = end;
        wrote = true;
    }
    while !sections.is_empty() {
        let depth = sections.len() - 1;
        out.push('\n');
        layout_indent(&mut out, depth);
        out.push_str("</section>");
        sections.pop();
    }
    Some(LayoutOutput {
        html: out,
        accepted,
    })
}

fn is_layout_quote_line(text: &str) -> bool {
    detect_heading(text).is_none()
        && detect_fence_open(text).is_none()
        && detect_container_open(text).is_none()
        && thematic_break_marker(text).is_none()
        && !is_list_marker(text)
        && !text.starts_with(['>', '|', '{'])
}

fn is_layout_paragraph_line(line: &str) -> bool {
    thematic_break_marker(line).is_none()
        && !is_list_marker(line)
        && !is_definition_list_start(line)
        && detect_heading(line).is_none()
        && detect_fence_open(line).is_none()
        && detect_container_open(line).is_none()
        && !trim_ascii_start(line).starts_with("$$`")
        && !line.starts_with([' ', '#', '*', '+', '>', '|', '{'])
        && parse_link_def_line(line).is_none()
}

fn layout_indent(out: &mut String, level: usize) {
    for _ in 0..level {
        out.push_str("  ");
    }
}

fn render_layout_inline(out: &mut String, text: &str, options: &Options<'_>) -> Option<()> {
    if layout_inline_needs_authoritative(text)
        || options.smart_typography != crate::extension::SmartTypographyMode::Glyph
    {
        return None;
    }
    let bytes = text.as_bytes();
    let mut i = 0;
    let mut plain = 0;
    while i < bytes.len() {
        let delimiter = bytes[i];
        if !matches!(delimiter, b'*' | b'/' | b'`' | b'[') {
            i += 1;
            continue;
        }
        crate::escape::write_escaped_text(out, &text[plain..i]);
        match delimiter {
            b'*' | b'/' => {
                let close = text[i + 1..].find(delimiter as char)? + i + 1;
                if close == i + 1
                    || (i > 0 && text.as_bytes()[i - 1].is_ascii_alphanumeric())
                    || text
                        .as_bytes()
                        .get(close + 1)
                        .is_some_and(|byte| byte.is_ascii_alphanumeric())
                    || text.as_bytes()[i + 1].is_ascii_whitespace()
                    || text.as_bytes()[close - 1].is_ascii_whitespace()
                {
                    return None;
                }
                let tag = if delimiter == b'*' { "strong" } else { "em" };
                out.push('<');
                out.push_str(tag);
                out.push('>');
                render_layout_inline(out, &text[i + 1..close], options)?;
                out.push_str("</");
                out.push_str(tag);
                out.push('>');
                i = close + 1;
            }
            b'`' => {
                let close = text[i + 1..].find('`')? + i + 1;
                let code = &text[i + 1..close];
                if code.as_bytes().first().is_some_and(u8::is_ascii_whitespace)
                    || code.as_bytes().last().is_some_and(u8::is_ascii_whitespace)
                {
                    return None;
                }
                out.push_str("<code>");
                crate::escape::write_escaped_text(out, code);
                out.push_str("</code>");
                i = close + 1;
            }
            b'[' => {
                let label_end = text[i + 1..].find(']')? + i + 1;
                let label = &text[i + 1..label_end];
                let tail = text.as_bytes().get(label_end + 1).copied()?;
                let (href, title, end) = if tail == b'(' {
                    let close = text[label_end + 2..].find(')')? + label_end + 2;
                    let href = &text[label_end + 2..close];
                    if href.is_empty()
                        || href.contains('(')
                        || href.bytes().any(|byte| byte.is_ascii_whitespace())
                    {
                        return None;
                    }
                    (href.to_string(), None, close + 1)
                } else if tail == b'[' {
                    let close = text[label_end + 2..].find(']')? + label_end + 2;
                    let reference = &text[label_end + 2..close];
                    if reference.is_empty() {
                        return None;
                    }
                    let def = ACTIVE_LINK_DEFS.with(|active| {
                        active
                            .borrow()
                            .last()
                            .and_then(|context| context.defs.get(reference).cloned())
                    })?;
                    if def.attrs.is_some() {
                        return None;
                    }
                    (def.href, def.title, close + 1)
                } else {
                    return None;
                };
                out.push_str("<a href=\"");
                crate::escape::write_escaped_attr(out, &crate::escape::sanitize_url(&href));
                out.push('"');
                if let Some(title) = title {
                    out.push_str(" title=\"");
                    crate::escape::write_escaped_attr(out, &title);
                    out.push('"');
                }
                out.push('>');
                render_layout_inline(out, label, options)?;
                out.push_str("</a>");
                i = end;
            }
            _ => unreachable!(),
        }
        plain = i;
    }
    crate::escape::write_escaped_text(out, &text[plain..]);
    Some(())
}

fn layout_inline_needs_authoritative(text: &str) -> bool {
    let bytes = text.as_bytes();
    let mut colons = 0;
    for (i, byte) in bytes.iter().copied().enumerate() {
        if matches!(
            byte,
            b'{' | b'}'
                | b'^'
                | b'\\'
                | b'<'
                | b'>'
                | b'_'
                | b'~'
                | b'!'
                | b'@'
                | b'$'
                | b'='
                | b'#'
                | b'\''
                | b'"'
        ) {
            return true;
        }
        match byte {
            b':' => {
                colons += 1;
                if colons == 2 {
                    return true;
                }
            }
            b'-' if bytes.get(i + 1) == Some(&b'-') => return true,
            b'.' if bytes.get(i + 1..i + 3) == Some(b"..") => return true,
            b'/' if bytes.get(i + 1) == Some(&b'*') => return true,
            b'*' if bytes.get(i + 1) == Some(&b'/') => return true,
            b'`' if bytes.get(i + 1) == Some(&b'`') => return true,
            b'+' if bytes.get(i + 1) == Some(&b'-') => return true,
            b'(' => {
                let rest = &bytes[i..];
                if rest.starts_with(b"(c)") || rest.starts_with(b"(r)") || rest.starts_with(b"(tm)")
                {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

fn render_layout_list(
    lines: &[&str],
    mut i: usize,
    indent: usize,
    depth: usize,
    options: &Options<'_>,
    out: &mut String,
    accepted: &mut AcceptanceCounters,
) -> Option<usize> {
    layout_indent(out, depth);
    out.push_str("<ul>");
    while i < lines.len() {
        let line = lines[i];
        let leading = line.bytes().take_while(|byte| *byte == b' ').count();
        if leading < indent {
            break;
        }
        if leading != indent || !line[leading..].starts_with("- ") {
            return None;
        }
        let text = &line[leading + 2..];
        if text.starts_with(' ')
            || text.is_empty()
            || text == "+"
            || detect_heading(text).is_some()
            || detect_fence_open(text).is_some()
            || detect_container_open(text).is_some()
            || thematic_break_marker(text).is_some()
            || is_list_marker(text)
            || text.starts_with(['>', '|', '{'])
        {
            return None;
        }
        out.push('\n');
        layout_indent(out, depth + 1);
        out.push_str("<li>");
        accepted.record(BlockLayout {
            event: LayoutEvent::UnorderedListItem,
            consumed: i..i + 1,
            active_definition: false,
        });
        render_layout_inline(out, text, options)?;
        i += 1;
        if i < lines.len() {
            let next = lines[i];
            let next_indent = next.bytes().take_while(|byte| *byte == b' ').count();
            if next_indent > indent {
                if next_indent != indent + 2 || !next[next_indent..].starts_with("- ") {
                    return None;
                }
                out.push('\n');
                i = render_layout_list(lines, i, indent + 2, depth + 2, options, out, accepted)?;
                out.push('\n');
                layout_indent(out, depth + 1);
            }
        }
        out.push_str("</li>");
        if i >= lines.len() {
            break;
        }
        if is_blank_line(lines[i]) {
            if lines[i + 1..]
                .iter()
                .find(|line| !is_blank_line(line))
                .is_some_and(|next| {
                    let next_indent = next.bytes().take_while(|byte| *byte == b' ').count();
                    next_indent == indent && next[next_indent..].starts_with("- ")
                })
            {
                return None;
            }
            break;
        }
        let next_indent = lines[i].bytes().take_while(|byte| *byte == b' ').count();
        if next_indent < indent {
            break;
        }
    }
    out.push('\n');
    layout_indent(out, depth);
    out.push_str("</ul>");
    Some(i)
}

fn decimal_list_item(line: &str) -> Option<(usize, &str)> {
    let dot = line.find('.')?;
    let number = line[..dot].parse::<usize>().ok()?;
    let text = line.get(dot + 1..)?.strip_prefix(' ')?;
    if number == 0 || text.is_empty() || text.starts_with(' ') {
        return None;
    }
    Some((number, text))
}

fn render_layout_ordered_list(
    lines: &[&str],
    mut i: usize,
    depth: usize,
    options: &Options<'_>,
    out: &mut String,
    accepted: &mut AcceptanceCounters,
) -> Option<usize> {
    let (start, _) = decimal_list_item(lines.get(i)?)?;
    layout_indent(out, depth);
    out.push_str("<ol");
    if start != 1 {
        use std::fmt::Write as _;
        write!(out, " start=\"{start}\"").ok()?;
    }
    out.push('>');

    let mut expected = start;
    while i < lines.len() {
        let Some((number, text)) = decimal_list_item(lines[i]) else {
            break;
        };
        if number != expected
            || detect_heading(text).is_some()
            || detect_fence_open(text).is_some()
            || detect_container_open(text).is_some()
            || thematic_break_marker(text).is_some()
            || is_list_marker(text)
            || text.starts_with(['>', '|', '{'])
        {
            return None;
        }
        out.push('\n');
        layout_indent(out, depth + 1);
        out.push_str("<li>");
        render_layout_inline(out, text, options)?;
        out.push_str("</li>");
        accepted.record(BlockLayout {
            event: LayoutEvent::OrderedListItem,
            consumed: i..i + 1,
            active_definition: false,
        });
        expected = expected.checked_add(1)?;
        i += 1;
    }
    if lines.get(i).is_some_and(|line| !is_blank_line(line)) {
        // Indented, lazy, mixed-dialect, and nested continuations stay on the
        // authoritative list collector.
        return None;
    }
    out.push('\n');
    layout_indent(out, depth);
    out.push_str("</ol>");
    Some(i)
}

fn layout_pipe_cells(line: &str) -> Option<Vec<&str>> {
    let line = line.trim();
    if !line.starts_with('|') || !line.ends_with('|') || line.contains("\\|") {
        return None;
    }
    Some(line[1..line.len() - 1].split('|').map(str::trim).collect())
}

fn layout_alignment(cell: &str) -> Option<Option<&'static str>> {
    let left = cell.starts_with(':');
    let right = cell.ends_with(':');
    let core = cell.trim_matches(':').trim();
    if core.len() < 3 || !core.bytes().all(|byte| byte == b'-') {
        return None;
    }
    Some(match (left, right) {
        (true, true) => Some("center"),
        (false, true) => Some("right"),
        (true, false) => Some("left"),
        (false, false) => None,
    })
}

fn render_layout_table(
    lines: &[&str],
    start: usize,
    depth: usize,
    options: &Options<'_>,
    out: &mut String,
    accepted: &mut AcceptanceCounters,
) -> Option<usize> {
    let headers = layout_pipe_cells(lines.get(start)?)?;
    let delimiter = layout_pipe_cells(lines.get(start + 1)?)?;
    if headers.is_empty() || headers.len() != delimiter.len() {
        return None;
    }
    let alignments: Vec<Option<&str>> = delimiter
        .iter()
        .map(|cell| layout_alignment(cell))
        .collect::<Option<_>>()?;
    layout_indent(out, depth);
    out.push_str("<table>\n");
    layout_indent(out, depth + 1);
    out.push_str("<thead><tr>");
    accepted.record(BlockLayout {
        event: LayoutEvent::TableRow,
        consumed: start..start + 2,
        active_definition: false,
    });
    for (cell, alignment) in headers.iter().zip(&alignments) {
        out.push_str("<th scope=\"col\"");
        if let Some(alignment) = alignment {
            out.push_str(" style=\"text-align: ");
            out.push_str(alignment);
            out.push_str(";\"");
        }
        out.push('>');
        render_layout_inline(out, cell, options)?;
        out.push_str("</th>");
    }
    out.push_str("</tr></thead>");
    let mut i = start + 2;
    let mut rows = Vec::new();
    while i < lines.len() && lines[i].trim_start().starts_with('|') {
        let cells = layout_pipe_cells(lines[i])?;
        if cells.len() != headers.len() {
            return None;
        }
        rows.push(cells);
        i += 1;
    }
    if rows.is_empty() {
        return None;
    }
    out.push('\n');
    layout_indent(out, depth + 1);
    out.push_str("<tbody>");
    for (row_index, cells) in rows.into_iter().enumerate() {
        accepted.record(BlockLayout {
            event: LayoutEvent::TableRow,
            consumed: start + 2 + row_index..start + 3 + row_index,
            active_definition: false,
        });
        out.push('\n');
        layout_indent(out, depth + 2);
        out.push_str("<tr>");
        for (cell, alignment) in cells.iter().zip(&alignments) {
            out.push_str("<td");
            if let Some(alignment) = alignment {
                out.push_str(" style=\"text-align: ");
                out.push_str(alignment);
                out.push_str(";\"");
            }
            out.push('>');
            render_layout_inline(out, cell, options)?;
            out.push_str("</td>");
        }
        out.push_str("</tr>");
    }
    out.push('\n');
    layout_indent(out, depth + 1);
    out.push_str("</tbody>\n");
    layout_indent(out, depth);
    out.push_str("</table>");
    Some(i)
}

#[cfg(test)]
mod layout_html_tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    fn authoritative(source: &str) -> String {
        let options = Options::default();
        let (doc, index) = parse_with_render_index(source, &options);
        crate::render::render_html_owned_with_index(
            doc,
            &options,
            index,
            source.contains("[@"),
            source.contains("[^") || source.contains("^["),
        )
        .unwrap()
    }

    #[test]
    fn every_accepted_layout_matches_the_authoritative_pipeline() {
        let cases = [
            "# Heading\n\nPlain text.\n",
            "A paragraph spanning\nthree plain lines\nwithout an interrupt.\n",
            "# Heading\n\nParagraph with *strong*, /emphasis/, and `code`.\n",
            "[site]: https://example.com \"Example\"\n\n# Links\n\nA [direct](https://example.com/x) and [reference][site].\n",
            "# Lists\n\n- first\n- second\n  - nested *strong*\n  - nested two\n",
            "3. third\n4. fourth\n",
            "# Quote\n\n> One quoted /line/.\n",
            "> A quoted paragraph\n> spanning two lines.\n",
            "# Code\n\n```rs\nfn main() {\n}\n```\n",
            "# Table\n\n| A | B | C |\n| --- | ---: | :---: |\n| x | 1 | *z* |\n| y | 2 | `q` |\n",
            "# One\n\n## Two\n\ntext\n\n## Two\n\ntext\n",
        ];
        for source in cases {
            let fast = try_layout_html(source, &Options::default())
                .expect("the parity fixture is part of the accepted layout subset");
            assert_eq!(fast, authoritative(source), "source:\n{source}");
        }
    }

    #[test]
    fn benchmark_core_shape_reports_each_accepted_event_family() {
        // This is the Tier-1 shape used by carve-bench, kept small enough to
        // explain a routing change when a benchmark moves. Counts are part of
        // the scanner contract: widening one family must not silently consume
        // a neighbouring block as the same event.
        let source = concat!(
            "[site]: https://example.com \"Example\"\n\n",
            "# Layout benchmark\n\n",
            "A [link][site] with *strong* and /emphasis/.\n\n",
            "> quoted text\n\n",
            "- first\n- second\n\n",
            "```rs\nlet answer = 42;\n```\n\n",
            "| Name | Value |\n| --- | ---: |\n| one | 1 |\n| two | 2 |\n",
        );
        let output = try_layout(source, &Options::default())
            .expect("the Tier-1 benchmark shape must stay on the layout path");

        assert_eq!(
            output.accepted,
            AcceptanceCounters {
                headings: 1,
                paragraphs: 1,
                block_quotes: 1,
                code_fences: 1,
                unordered_list_items: 2,
                ordered_list_items: 0,
                table_rows: 3,
                link_definitions: 1,
                consumed_lines: 13,
                active_definitions: 1,
            }
        );
        assert_eq!(output.html, authoritative(source));
    }

    #[test]
    fn every_corpus_document_accepted_by_layout_has_exact_shadow_parity() {
        let corpus = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/spec/tests/corpus");
        let mut paths: Vec<_> = fs::read_dir(&corpus)
            .unwrap_or_else(|error| panic!("read {}: {error}", corpus.display()))
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("crv"))
            .collect();
        paths.sort();

        let mut accepted = 0;
        for path in paths {
            let source = fs::read_to_string(&path)
                .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
            let Some(output) = try_layout(&source, &Options::default()) else {
                continue;
            };
            accepted += 1;
            assert_eq!(
                output.html,
                authoritative(&source),
                "layout shadow mismatch for {}",
                path.display()
            );
        }
        assert_eq!(
            accepted, 49,
            "update the pinned acceptance count only after reviewing the exact-parity widening"
        );
    }

    #[test]
    fn stateful_or_ambiguous_shapes_fall_back() {
        for source in [
            "# *marked heading*\n",
            "A “smart” quote.\n",
            "[x](java\0script:alert(1))\n",
            "[x](java-script:alert(1))\n",
            "- a\n- +\n",
            "- # H\n- next\n",
            "> # H\n\ntail\n",
            ".   \n",
            "/*x*/\n",
            "$`a``b`\n",
            "`  a  `\n",
            "> a\nb\n",
            "-   x\n",
            "=marked= here\n",
            "[^n]: note\n\nsee [^n]\n",
            "::: note\nbody\n:::\n",
            "![image](/x.png)\n",
            "{#id}\n# heading\n",
        ] {
            assert!(try_layout_html(source, &Options::default()).is_none());
        }
    }
}
