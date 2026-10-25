use super::*;

enum Event<'a> {
    Heading(u8, &'a str),
    Paragraph(&'a [&'a str]),
}

struct Events<'a> {
    lines: &'a [&'a str],
    next: usize,
}

impl<'a> Iterator for Events<'a> {
    type Item = Event<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        while self.next < self.lines.len() && is_blank_line(self.lines[self.next]) {
            self.next += 1;
        }
        let first = *self.lines.get(self.next)?;
        if let Some((level, text)) = detect_heading(first) {
            self.next += 1;
            return Some(Event::Heading(level, text));
        }
        let start = self.next;
        self.next += 1;
        while self.next < self.lines.len() {
            let line = self.lines[self.next];
            if is_blank_line(line) || detect_heading(line).is_some() {
                break;
            }
            self.next += 1;
        }
        Some(Event::Paragraph(&self.lines[start..self.next]))
    }
}

fn supported_line(line: &str) -> bool {
    if is_blank_line(line) {
        return true;
    }
    if line.starts_with('#') {
        return detect_heading(line).is_some();
    }
    line.as_bytes().first().is_some_and(u8::is_ascii_alphabetic) && !is_list_marker(line)
}

/// Materialize the public nodes from borrowed line events. A complete lexical
/// gate runs first, so an unsupported line never leaves a partly built tree.
pub(super) fn parse_blocks(lines: &[&str], options: &Options<'_>) -> Option<Vec<BlockNode>> {
    if options.positions || options.source_lines || !options.extensions.is_empty() {
        return None;
    }
    if !lines.iter().copied().all(supported_line) {
        return None;
    }
    let mut out = Vec::new();
    for event in (Events { lines, next: 0 }) {
        match event {
            Event::Heading(level, text) => out.push(BlockNode::Heading(Heading {
                attrs: None,
                level,
                children: parse_event_inlines(trim_ascii_end(text), options),
                pos: None,
            })),
            Event::Paragraph(lines) => {
                let joined = lines
                    .iter()
                    .map(|line| trim_ascii_end(trim_ascii_start(line)))
                    .collect::<Vec<_>>()
                    .join("\n");
                out.push(BlockNode::Paragraph(Paragraph {
                    attrs: None,
                    children: parse_event_inlines(&joined, options),
                    at_content_column: true,
                    block_image: false,
                    pos: None,
                }));
            }
        }
    }
    Some(out)
}

/// Stream simple top-level blocks as events while handing list and definition
/// bodies to the existing readers. A construct outside this subset declines the
/// whole route before it can publish a partial AST.
fn parse_mixed_blocks(lines: &[&str], options: &Options<'_>) -> Option<Vec<BlockNode>> {
    if lines.iter().copied().any(|line| {
        let line = trim_ascii(line);
        line == "+" || line.ends_with(" +")
    }) {
        return None;
    }
    let mut cur = LineCursor::new_with_cols(lines, None, None);
    cur.at_document_level = true;
    let mut out = Vec::new();
    while let Some(line) = cur.peek() {
        if is_blank_line(line) {
            cur.consume();
            continue;
        }
        if let Some((level, text)) = detect_heading(line) {
            out.push(*parse_heading_block(&mut cur, level, text, options));
            continue;
        }
        if is_list_marker(line) && !line.starts_with([' ', '\t']) {
            if let Some((list, end)) = parse_simple_list(lines, cur.pos, options) {
                cur.pos = end;
                out.push(BlockNode::List(list));
            } else {
                let mut item_bodies = Vec::new();
                let list = parse_list(&mut cur, options, &mut item_bodies);
                out.push(*drained_list_bodies(list, item_bodies, options));
            }
            continue;
        }
        if is_definition_list_start(line) {
            out.push(parse_definition_list(&mut cur, options));
            continue;
        }
        if strip_blockquote_prefix(line).is_some() {
            if let Some((quote, end)) = parse_simple_quote(lines, cur.pos, options) {
                cur.pos = end;
                out.push(BlockNode::BlockQuote(quote));
            } else if let Some((quote, end)) = parse_marked_quote(lines, cur.pos, options) {
                cur.pos = end;
                out.push(BlockNode::BlockQuote(quote));
            } else {
                return None;
            }
            continue;
        }
        if let Some(fence) = detect_fence_open(line) {
            let mut block = *parse_fenced_block(&mut cur, fence, options);
            resolve_code_title(&mut block);
            out.push(block);
            continue;
        }
        if let Some(marker) = thematic_break_marker(line) {
            cur.consume();
            out.push(BlockNode::ThematicBreak(ThematicBreak {
                marker: (marker != '-').then_some(marker),
                ..Default::default()
            }));
            continue;
        }
        if !may_start_plain_paragraph(line) {
            return None;
        }
        let mut paragraph_lines = Vec::new();
        while let Some(next) = cur.peek() {
            if is_blank_line(next) || detect_heading(next).is_some() {
                break;
            }
            if !paragraph_lines.is_empty() {
                if let Some(open) = detect_fence_open(next) {
                    if lines[cur.pos + 1..]
                        .iter()
                        .any(|line| is_fence_close(line, open))
                    {
                        break;
                    }
                }
            }
            if !paragraph_lines.is_empty()
                && (is_definition_list_start(next)
                    || detect_fence_open(next).is_some()
                    || thematic_break_marker(next).is_some())
            {
                return None;
            }
            if !may_continue_plain_paragraph(next) && !is_list_marker(next) {
                return None;
            }
            paragraph_lines.push(trim_ascii_end(trim_ascii_start(next)));
            cur.consume();
        }
        let joined = paragraph_lines.join("\n");
        out.push(BlockNode::Paragraph(Paragraph {
            attrs: None,
            children: parse_event_inlines(&joined, options),
            at_content_column: true,
            block_image: false,
            pos: None,
        }));
    }
    Some(out)
}

fn parse_simple_quote(
    lines: &[&str],
    start: usize,
    options: &Options<'_>,
) -> Option<(BlockQuote, usize)> {
    let mut parts = Vec::new();
    let mut end = start;
    while let Some(line) = lines.get(end) {
        let Some(content) = line.strip_prefix("> ") else {
            break;
        };
        if content.is_empty()
            || !may_start_plain_paragraph(content)
            || detect_heading(content).is_some()
            || is_list_marker(content)
            || is_definition_list_start(content)
            || detect_fence_open(content).is_some()
            || thematic_break_marker(content).is_some()
        {
            return None;
        }
        parts.push(trim_ascii_end(content));
        end += 1;
    }
    if parts.is_empty() {
        return None;
    }
    let mut after = end;
    while lines.get(after).is_some_and(|line| is_blank_line(line)) {
        after += 1;
    }
    if after == end && end < lines.len()
        || lines.get(after).is_some_and(|line| line.starts_with('^'))
    {
        return None;
    }
    let joined = parts.join("\n");
    Some((
        BlockQuote {
            attrs: None,
            children: vec![BlockNode::Paragraph(Paragraph {
                attrs: None,
                children: parse_event_inlines(&joined, options),
                at_content_column: true,
                block_image: false,
                pos: None,
            })],
            fenced: false,
            pos: None,
        },
        end,
    ))
}

fn parse_marked_quote(
    lines: &[&str],
    start: usize,
    options: &Options<'_>,
) -> Option<(BlockQuote, usize)> {
    let mut parts = Vec::new();
    let mut end = start;
    while let Some(line) = lines.get(end) {
        let Some(content) = strip_blockquote_prefix(line) else {
            break;
        };
        if detect_fence_open(content).is_some() {
            return None;
        }
        parts.push(content);
        end += 1;
    }
    if parts.is_empty() {
        return None;
    }
    let mut after = end;
    while lines.get(after).is_some_and(|line| is_blank_line(line)) {
        after += 1;
    }
    if (after == end && end < lines.len())
        || lines.get(after).is_some_and(|line| line.starts_with('^'))
    {
        return None;
    }
    let children = parse_blocks(&parts, options)?;
    Some((
        BlockQuote {
            attrs: None,
            children,
            fenced: false,
            pos: None,
        },
        end,
    ))
}

#[cfg(test)]
mod simple_quote_tests {
    use super::*;

    #[test]
    fn simple_marked_quote_builds_public_nodes() {
        let source = "> plain *text*\n> more words\n";
        let lines: Vec<&str> = source.lines().collect();
        let (quote, end) = parse_simple_quote(&lines, 0, &Options::default()).unwrap();
        assert_eq!(end, 2);
        let expected = parse_with_options(source, &Options::default());
        assert_eq!(BlockNode::BlockQuote(quote), expected.children[0]);
    }

    #[test]
    fn lazy_lines_and_captions_keep_the_existing_reader() {
        let options = Options::default();
        for source in [
            "> quoted\nlazy line\n",
            "> quoted\n\n^ caption\n",
            "> # Heading\n",
        ] {
            let lines: Vec<&str> = source.lines().collect();
            assert!(
                parse_simple_quote(&lines, 0, &options).is_none(),
                "{source:?}"
            );
        }
    }

    #[test]
    fn fully_marked_paragraphs_use_the_recursive_quote_path() {
        let source = "> first\n>\n> second\n";
        let lines: Vec<&str> = source.lines().collect();
        let options = Options::default();
        assert!(parse_simple_quote(&lines, 0, &options).is_none());
        let (quote, end) = parse_marked_quote(&lines, 0, &options).unwrap();
        assert_eq!(end, 3);
        let expected = parse_with_options(source, &options);
        assert_eq!(BlockNode::BlockQuote(quote), expected.children[0]);
    }

    #[test]
    fn fenced_quote_content_keeps_the_existing_reader() {
        let source = "> ```=html\n>\n";
        let lines: Vec<&str> = source.lines().collect();
        assert!(parse_marked_quote(&lines, 0, &Options::default()).is_none());
    }
}

fn may_start_plain_paragraph(line: &str) -> bool {
    line.as_bytes().first().is_some_and(|first| {
        first.is_ascii_alphanumeric()
            || matches!(first, b'*' | b'`' | b'"' | b'\'' | b'(')
            || may_start_extended_plain_paragraph(*first)
    })
}

#[cold]
#[inline(never)]
fn may_start_extended_plain_paragraph(first: u8) -> bool {
    matches!(first, b'/' | b'_' | b'~' | b'=' | b'\\' | b'[' | b'!')
}

fn may_continue_plain_paragraph(line: &str) -> bool {
    may_start_plain_paragraph(line) || line.starts_with(": ") || line.starts_with('(')
}

fn parse_event_inlines(text: &str, options: &Options<'_>) -> Vec<InlineNode> {
    let bytes = text.as_bytes();
    let plain = if text.len() < 128 {
        !text.is_empty()
            && !text.contains("..")
            && !text.contains("(c)")
            && !text.contains("(r)")
            && !text.contains("(tm)")
            && bytes.iter().all(|byte| {
                byte.is_ascii_alphanumeric()
                    || matches!(
                        byte,
                        b' ' | b'\n' | b'.' | b',' | b';' | b'!' | b'?' | b'(' | b')'
                    )
            })
    } else {
        long_literal_inline(text)
    };
    if !plain {
        return parse_inline_with_options(text, options);
    }
    let mut out = Vec::new();
    for (index, line) in text.split('\n').enumerate() {
        if index > 0 {
            out.push(InlineNode::SoftBreak(Break { pos: None }));
        }
        if !line.is_empty() {
            out.push(InlineNode::text(line.to_string()));
        }
    }
    out
}

/// The long route accepts only bytes that the current inline dispatch treats
/// as literal. Review this gate when inline constructs or symbol boundaries change.
fn long_literal_inline(text: &str) -> bool {
    if text.len() < 128 {
        return false;
    }
    let bytes = text.as_bytes();
    for (index, &byte) in bytes.iter().enumerate() {
        let previous = index.checked_sub(1).and_then(|at| bytes.get(at)).copied();
        let literal = match byte {
            b'\n' | b' ' | b'.' | b',' | b';' | b'?' | b'&' | b'(' | b')' => true,
            b'!' => bytes.get(index + 1) != Some(&b'='),
            b'-' => bytes.get(index + 1) != Some(&b'-'),
            b':' | b'#' | b'@' => {
                previous.is_some_and(|prev| prev.is_ascii_alphanumeric() || prev == b'_')
            }
            b'\\' | b'%' | b'`' | b'$' | b'{' | b'}' | b'[' | b']' | b'<' | b'>' | b'/' | b'*'
            | b'_' | b'~' | b'=' | b'^' | b'+' | b'\'' | b'"' => false,
            _ => byte.is_ascii_alphanumeric() || !byte.is_ascii(),
        };
        if !literal {
            return false;
        }
    }
    !text.contains("...")
        && !text.contains("(c)")
        && !text.contains("(r)")
        && !text.contains("(tm)")
}

#[cfg(test)]
mod inline_lexer_tests {
    use super::long_literal_inline;

    #[test]
    fn long_literal_gate_admits_only_safe_text() {
        assert!(!long_literal_inline(&"a".repeat(127)));
        assert!(long_literal_inline(&"a".repeat(128)));
        for safe in ["café, 日本語", "carve#529", "foo:bar", "a@b", "a & b", "-x"] {
            assert!(
                long_literal_inline(&format!("{} {safe}", "a".repeat(128))),
                "{safe}"
            );
        }
        for syntax in [
            " @user",
            " #tag",
            " :symbol:",
            " !=",
            "--",
            "...",
            "(c)",
            "*strong*",
        ] {
            assert!(
                !long_literal_inline(&format!("{} {syntax}", "a".repeat(128))),
                "{syntax}"
            );
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SimpleListKind {
    Bullet(u8),
    Ordered,
}

fn simple_list_marker(line: &str) -> Option<(SimpleListKind, usize, &str)> {
    let bytes = line.as_bytes();
    let (kind, ordinal, marker_width) = match bytes {
        [b'-' | b'*', b' ', b' ', ..] => return None,
        [b'-' | b'*', b' ', ..] => (SimpleListKind::Bullet(bytes[0]), 0, 2),
        [digit @ b'1'..=b'9', b'.', b' ', b' ', ..] => {
            let _ = digit;
            return None;
        }
        [digit @ b'1'..=b'9', b'.', b' ', ..] => {
            (SimpleListKind::Ordered, (digit - b'0') as usize, 3)
        }
        _ => return None,
    };
    let content = trim_ascii_end(&line[marker_width..]);
    if content.is_empty()
        || content.starts_with('[')
        || is_list_marker(content)
        || detect_fence_open(content).is_some()
        || thematic_break_marker(content).is_some()
        || !may_start_plain_paragraph(content)
    {
        return None;
    }
    Some((kind, ordinal, content))
}

fn parse_simple_list(lines: &[&str], start: usize, options: &Options<'_>) -> Option<(List, usize)> {
    let (kind, first_number, _) = simple_list_marker(lines.get(start)?)?;
    if kind == SimpleListKind::Ordered && first_number != 1 {
        return None;
    }
    let content_column = if kind == SimpleListKind::Ordered {
        3
    } else {
        2
    };
    let mut items = Vec::new();
    let mut pos = start;
    while let Some((item_kind, ordinal, content)) =
        lines.get(pos).and_then(|line| simple_list_marker(line))
    {
        if item_kind != kind {
            break;
        }
        if kind == SimpleListKind::Ordered && ordinal != items.len() + 1 {
            return None;
        }
        let mut item_lines = vec![content];
        pos += 1;
        while let Some(&line) = lines.get(pos) {
            if is_blank_line(line) || simple_list_marker(line).is_some() {
                break;
            }
            let prefix = &line.as_bytes()[..line.len().min(content_column)];
            if prefix.iter().all(|byte| *byte == b' ') && prefix.len() == content_column {
                let content = trim_ascii_end(trim_ascii_start(line));
                if !may_continue_plain_paragraph(content)
                    || is_list_marker(content)
                    || detect_fence_open(content).is_some()
                    || thematic_break_marker(content).is_some()
                    || detect_heading(content).is_some()
                {
                    return None;
                }
                item_lines.push(content);
                pos += 1;
            } else {
                return None;
            }
        }
        let joined = item_lines.join("\n");
        items.push(ListItem {
            attrs: None,
            checked: None,
            task_state: None,
            children: vec![BlockNode::Paragraph(Paragraph {
                attrs: None,
                children: parse_event_inlines(&joined, options),
                at_content_column: true,
                block_image: false,
                pos: None,
            })],
            pos: None,
        });
        if lines.get(pos).is_some_and(|line| is_blank_line(line)) {
            let mut next = pos;
            while lines.get(next).is_some_and(|line| is_blank_line(line)) {
                next += 1;
            }
            if lines.get(next).is_some_and(|line| is_list_marker(line)) {
                return None;
            }
            break;
        }
        if lines
            .get(pos)
            .and_then(|line| simple_list_marker(line))
            .is_none()
        {
            break;
        }
    }
    Some((
        List {
            attrs: None,
            ordered: kind == SimpleListKind::Ordered,
            start: None,
            ol_type: None,
            bare_marker: false,
            delim: None,
            bullet_char: match kind {
                SimpleListKind::Bullet(b'*') => Some('*'),
                _ => None,
            },
            tight: true,
            items,
            pos: None,
        },
        pos,
    ))
}

/// The admission gate excludes syntax that needs definition lifting, source
/// normalization, positions, or a container. The standard parser handles it.
pub(super) fn parse_document(source: &str) -> Option<(Document, bool)> {
    if source.contains(['\r', '\0', '\u{feff}', '\u{e005}', '\u{e006}'])
        || source.contains("[^")
        || source.contains("][")
        || source.contains("*[")
        || source.contains("]:")
        || (!source.ends_with('\n') && source.lines().any(|line| detect_fence_open(line).is_some()))
        || source
            .lines()
            .any(|line| line.starts_with([' ', '\t']) && is_list_marker(line))
    {
        return None;
    }
    let options = Options::default();
    let _continuation_markers = continuation_marker::Guard::install(source);
    let _source_ending = SourceEndingGuard::install(source);
    let (frontmatter, frontmatter_raw, body) = split_frontmatter(source, false);
    let direct = {
        let _depth = DepthGuard::enter()?;
        let lines: Vec<&str> = body.lines().collect();
        parse_blocks(&lines, &options).or_else(|| parse_mixed_blocks(&lines, &options))
    };
    let used_event_route = direct.is_some();
    let children = direct.unwrap_or_else(|| {
        let body_start_line = source[..body.as_ptr() as usize - source.as_ptr() as usize]
            .bytes()
            .filter(|byte| *byte == b'\n')
            .count()
            + 1;
        let line_count = body.lines().count();
        let joined = joined_source(&body.lines().collect::<Vec<_>>());
        let mapped = MappedSource {
            source: joined,
            line_map: (body_start_line..body_start_line + line_count)
                .map(Some)
                .collect(),
            col_map: Vec::new(),
            reached: Vec::new(),
            opaque_blank_lines: Vec::new(),
            discarded_outer_blank: false,
            outer_plus_after_blank: false,
            authored_base_at_start: false,
            item_marker_at_start: false,
            marker_colon_rebased: false,
            marker_tail: None,
            in_quoted_body: false,
            sublists_carry_authored_base: false,
        };
        parse_mapped_source_at_document_level(&mapped, &options)
    });
    let mut doc = Document {
        frontmatter,
        frontmatter_raw,
        footnote_defs: BTreeMap::new(),
        footnote_def_pos: BTreeMap::new(),
        children,
        source_len: source.len(),
        ingest_payload_len: 0,
    };
    if source.contains('^') {
        number_crossref_captions(&mut doc);
    }
    stamp_generated_heading_ids(&mut doc, options.heading_id_options());
    let _ = crate::render::collect_footnotes(&mut doc, false);
    if source.contains("![") {
        promote_block_images(&mut doc.children, false, true);
    }
    coalesce_text_runs(&mut doc);
    if source.contains("</#") {
        fill_crossref_hrefs(&mut doc, options.heading_id_options());
    }
    Some((doc, used_event_route))
}
