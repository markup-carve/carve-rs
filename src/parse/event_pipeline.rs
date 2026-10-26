use super::*;

const MIXED_QUOTE_DEPTH_LIMIT: usize = 32;

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
fn parse_mixed_blocks<const DEFINITIONS: bool, const ABBREVIATIONS: bool>(
    lines: &[&str],
    options: &Options<'_>,
) -> Option<Vec<BlockNode>> {
    parse_mixed_blocks_mapped::<DEFINITIONS, ABBREVIATIONS>(lines, options, None)
}

fn parse_mixed_blocks_mapped<const DEFINITIONS: bool, const ABBREVIATIONS: bool>(
    lines: &[&str],
    options: &Options<'_>,
    line_map: Option<&[Option<usize>]>,
) -> Option<Vec<BlockNode>> {
    if line_map.is_none()
        && lines.iter().copied().any(|line| {
            let line = trim_ascii(line);
            line == "+" || line.ends_with(" +")
        })
    {
        return None;
    }
    let mut cur = LineCursor::new_with_cols(lines, line_map, None);
    cur.at_document_level = true;
    let mut out = Vec::new();
    let mut leading_attrs: Vec<(usize, Attrs)> = Vec::new();
    while cur.peek().is_some() {
        if DEFINITIONS && cur.peek().is_some_and(is_definition_placeholder) {
            cur.consume();
            continue;
        }
        let invalid_brace_line = if cur.peek().is_some_and(|line| line.starts_with('{')) {
            if let Some(attrs) = parse_standalone_attrs_block(&mut cur) {
                leading_attrs.push((out.len(), attrs));
                continue;
            }
            true
        } else {
            false
        };
        let line = cur.peek().unwrap();
        if is_blank_line(line) {
            cur.consume();
            continue;
        }
        if ABBREVIATIONS {
            if let Some(definition) = detect_abbreviation_def(line) {
                cur.consume();
                out.push(BlockNode::AbbreviationDef(definition));
                continue;
            }
        }
        if let Some((level, text)) = detect_heading(line) {
            out.push(*parse_heading_block(&mut cur, level, text, options));
            continue;
        }
        if is_list_marker(line) && !line.starts_with([' ', '\t']) {
            if let Some((list, end)) = parse_simple_list(lines, cur.pos, options) {
                cur.pos = end;
                out.push(BlockNode::List(list));
            } else if let Some((list, end)) = parse_unified_list(lines, cur.pos, options) {
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
            } else if let Some((quote, end)) = parse_simple_lazy_quote(lines, cur.pos, options) {
                cur.pos = end;
                out.push(BlockNode::BlockQuote(quote));
            } else {
                out.push(*parse_blockquote(&mut cur, options));
            }
            continue;
        }
        if let Some(fence) = detect_fence_open(line) {
            let mut block = *parse_fenced_block(&mut cur, fence, options);
            let has_leading_attrs = leading_attrs.last().is_some_and(|(start, _)| {
                out[*start..].iter().all(|node| {
                    matches!(node, BlockNode::AbbreviationDef(_) | BlockNode::Comment(_))
                })
            });
            if !has_leading_attrs {
                resolve_code_title(&mut block);
            }
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
        if !line.starts_with([' ', '\t']) && is_table_start(line) {
            out.push(parse_table(&mut cur, options));
            continue;
        }
        if !may_start_plain_paragraph(line) && !invalid_brace_line {
            if line.starts_with(":::") {
                if detect_line_block_open(line).is_some() {
                    out.push(parse_line_block(&mut cur, options));
                    continue;
                }
                if detect_hardbreaks_block_open(line).is_some() {
                    out.push(parse_hardbreaks_block(&mut cur, options));
                    continue;
                }
                if detect_quote_block_open(line).is_some() {
                    out.push(parse_quote_block(&mut cur, options));
                    continue;
                }
                if detect_container_open(line).is_some() {
                    out.push(*parse_event_container(&mut cur, options));
                    continue;
                }
            }
            return None;
        }
        let mut paragraph_lines = Vec::new();
        while let Some(next) = cur.peek() {
            if DEFINITIONS && is_definition_placeholder(next) {
                break;
            }
            if ABBREVIATIONS
                && !paragraph_lines.is_empty()
                && detect_abbreviation_def(next).is_some()
            {
                break;
            }
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
            if !may_continue_plain_paragraph(next)
                && !is_list_marker(next)
                && !(paragraph_lines.is_empty() && invalid_brace_line)
            {
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
    if !leading_attrs.is_empty() {
        let mut next_visible = vec![None; out.len() + 1];
        for index in (0..out.len()).rev() {
            next_visible[index] = if matches!(
                out[index],
                BlockNode::AbbreviationDef(_) | BlockNode::Comment(_)
            ) {
                next_visible[index + 1]
            } else {
                Some(index)
            };
        }
        let mut target = None;
        let mut pending = None;
        let mut slots = None;
        for (start, attrs) in leading_attrs {
            let Some(next_target) = next_visible[start] else {
                continue;
            };
            if target != Some(next_target) {
                if let (Some(index), Some(attrs)) = (target, pending.take()) {
                    apply_event_leading_attrs(&mut out[index], attrs);
                }
                target = Some(next_target);
                slots = None;
            }
            merge_attrs_indexed(&mut pending, attrs, &mut slots);
        }
        if let (Some(index), Some(attrs)) = (target, pending) {
            apply_event_leading_attrs(&mut out[index], attrs);
        }
    }
    Some(out)
}

enum UnifiedListBlock<'a> {
    Paragraph(Vec<&'a str>),
    List(UnifiedListNode<'a>),
    Quote(BlockQuote),
}

struct UnifiedListItem<'a> {
    blocks: Vec<UnifiedListBlock<'a>>,
}

struct UnifiedListNode<'a> {
    kind: SimpleListKind,
    tight: bool,
    items: Vec<UnifiedListItem<'a>>,
}

struct UnifiedListFrame<'a> {
    node: UnifiedListNode<'a>,
    kind: SimpleListKind,
    indent: usize,
    content_indent: usize,
    paragraph: Vec<&'a str>,
}

fn flush_unified_list_paragraph<'a>(frame: &mut UnifiedListFrame<'a>) -> Option<()> {
    if frame.paragraph.is_empty() {
        return Some(());
    }
    frame
        .node
        .items
        .last_mut()?
        .blocks
        .push(UnifiedListBlock::Paragraph(std::mem::take(
            &mut frame.paragraph,
        )));
    Some(())
}

fn close_unified_list_frame(frames: &mut Vec<UnifiedListFrame<'_>>) -> Option<()> {
    let mut child = frames.pop()?;
    flush_unified_list_paragraph(&mut child)?;
    frames
        .last_mut()?
        .node
        .items
        .last_mut()?
        .blocks
        .push(UnifiedListBlock::List(child.node));
    Some(())
}

fn build_unified_list(node: UnifiedListNode<'_>, options: &Options<'_>) -> List {
    let mut list = deep_list_node(node.kind);
    list.tight = node.tight;
    list.items = node
        .items
        .into_iter()
        .map(|item| ListItem {
            attrs: None,
            checked: None,
            task_state: None,
            children: item
                .blocks
                .into_iter()
                .map(|block| match block {
                    UnifiedListBlock::Paragraph(lines) => {
                        let inlines = if lines.len() == 1 {
                            parse_event_inlines(lines[0], options)
                        } else {
                            parse_event_inlines(&lines.join("\n"), options)
                        };
                        BlockNode::Paragraph(Paragraph {
                            attrs: None,
                            children: inlines,
                            at_content_column: true,
                            block_image: false,
                            pos: None,
                        })
                    }
                    UnifiedListBlock::List(child) => {
                        BlockNode::List(build_unified_list(child, options))
                    }
                    UnifiedListBlock::Quote(quote) => BlockNode::BlockQuote(quote),
                })
                .collect(),
            pos: None,
        })
        .collect();
    list
}

#[cold]
#[inline(never)]
fn parse_unified_list_quote(
    lines: &[&str],
    start: usize,
    content_indent: usize,
    base_depth: usize,
    frame_depth: usize,
    options: &Options<'_>,
) -> Option<(BlockQuote, usize)> {
    let mut quote_lines = Vec::new();
    let mut end = start;
    while let Some(&candidate) = lines.get(end) {
        let candidate_indent = candidate.bytes().take_while(|byte| *byte == b' ').count();
        if candidate_indent != content_indent {
            break;
        }
        let candidate = &candidate[candidate_indent..];
        if strip_blockquote_prefix(trim_ascii_end(candidate)).is_none() {
            break;
        }
        quote_lines.push(candidate);
        end += 1;
    }
    if lines.get(end).is_some_and(|next| {
        !is_blank_line(next)
            && next.bytes().take_while(|byte| *byte == b' ').count() >= content_indent
    }) {
        return None;
    }
    let quote_depth = base_depth.saturating_add(frame_depth.saturating_mul(2));
    if quote_depth >= MAX_NESTING_DEPTH
        || quote_lines.iter().any(|line| {
            let mut rest = *line;
            let mut marker_depth = 0usize;
            while let Some(next) = strip_blockquote_prefix(rest) {
                marker_depth += 1;
                rest = next;
            }
            // Reserve the quote's inner block, list, and item scopes.
            quote_depth.saturating_add(marker_depth).saturating_add(3) >= MAX_NESTING_DEPTH
        })
    {
        return None;
    }
    let (quote, consumed) = {
        let _depth = DepthScope::set(quote_depth);
        parse_marked_quote(&quote_lines, 0, options)?
    };
    (consumed == quote_lines.len()).then_some((quote, end))
}

#[inline(never)]
fn parse_unified_list(
    lines: &[&str],
    start: usize,
    options: &Options<'_>,
) -> Option<(List, usize)> {
    let base_depth = NESTING_DEPTH.with(|depth| depth.get());
    // Leave room for the standard parser's list item and paragraph scopes.
    if base_depth.saturating_add(2) >= MAX_NESTING_DEPTH {
        return None;
    }
    let mut frames: Vec<UnifiedListFrame<'_>> = Vec::new();
    let mut pos = start;
    while let Some(&line) = lines.get(pos) {
        if is_blank_line(line) {
            let mut after = pos;
            while lines.get(after).is_some_and(|line| is_blank_line(line)) {
                after += 1;
            }
            let Some(&next) = lines.get(after) else { break };
            if after - pos != 1 {
                if !next.starts_with([' ', '\t']) && !is_list_marker(next) {
                    while frames.len() > 1 {
                        close_unified_list_frame(&mut frames)?;
                    }
                    break;
                }
                return None;
            }
            if frames.len() > 1 {
                if next.starts_with([' ', '\t']) {
                    return None;
                }
                while frames.len() > 1 {
                    close_unified_list_frame(&mut frames)?;
                }
            }
            let frame = frames.last_mut()?;
            if let Some((kind, _, _)) = simple_list_marker(next) {
                if kind != frame.kind {
                    break;
                }
                frame.node.tight = false;
                pos = after;
                continue;
            }
            let next_indent = next.bytes().take_while(|byte| *byte == b' ').count();
            if next_indent == frame.content_indent
                && simple_list_marker(&next[next_indent..]).is_some()
            {
                pos = after;
                continue;
            }
            if is_list_marker(next) {
                return None;
            }
            if next_indent == frame.content_indent {
                flush_unified_list_paragraph(frame)?;
                if strip_blockquote_prefix(&next[next_indent..]).is_none() {
                    frame.node.tight = false;
                }
                pos = after;
                continue;
            }
            if next.starts_with([' ', '\t']) {
                return None;
            }
            break;
        }
        let indent = line.bytes().take_while(|byte| *byte == b' ').count();
        let marker = simple_list_marker(&line[indent..]);
        if let Some((kind, ordinal, content)) = marker {
            while frames.len() > 1 && indent < frames.last()?.indent {
                close_unified_list_frame(&mut frames)?;
            }
            if frames.is_empty() {
                if indent != 0 || kind == SimpleListKind::Ordered && ordinal != 1 {
                    return None;
                }
                frames.push(UnifiedListFrame {
                    node: UnifiedListNode {
                        kind,
                        tight: true,
                        items: Vec::new(),
                    },
                    kind,
                    indent,
                    content_indent: indent
                        + if kind == SimpleListKind::Ordered {
                            3
                        } else {
                            2
                        },
                    paragraph: Vec::new(),
                });
            } else if indent == frames.last()?.content_indent {
                if base_depth.saturating_add(frames.len() + 2) >= MAX_NESTING_DEPTH {
                    return None;
                }
                flush_unified_list_paragraph(frames.last_mut()?)?;
                if kind == SimpleListKind::Ordered && ordinal != 1 {
                    return None;
                }
                frames.push(UnifiedListFrame {
                    node: UnifiedListNode {
                        kind,
                        tight: true,
                        items: Vec::new(),
                    },
                    kind,
                    indent,
                    content_indent: indent
                        + if kind == SimpleListKind::Ordered {
                            3
                        } else {
                            2
                        },
                    paragraph: Vec::new(),
                });
            } else if indent != frames.last()?.indent {
                return None;
            }
            let frame = frames.last_mut()?;
            if frame.kind != kind {
                if frames.len() == 1 && indent == 0 {
                    break;
                }
                return None;
            }
            if kind == SimpleListKind::Ordered && ordinal != frame.node.items.len() + 1 {
                return None;
            }
            flush_unified_list_paragraph(frame)?;
            frame
                .node
                .items
                .push(UnifiedListItem { blocks: Vec::new() });
            frame.paragraph.push(content);
            pos += 1;
            continue;
        }
        if indent == 0 && frames.len() == 1 {
            return None;
        }
        let active_frame_depth = frames.len();
        let frame = frames.last_mut()?;
        if indent != frame.content_indent {
            return None;
        }
        let content = trim_ascii_end(&line[indent..]);
        if content.starts_with('>') {
            let (quote, end) = parse_unified_list_quote(
                lines,
                pos,
                frame.content_indent,
                base_depth,
                active_frame_depth,
                options,
            )?;
            flush_unified_list_paragraph(frame)?;
            frame
                .node
                .items
                .last_mut()?
                .blocks
                .push(UnifiedListBlock::Quote(quote));
            pos = end;
            continue;
        }
        if !(if frame.paragraph.is_empty() {
            may_start_plain_paragraph(content)
        } else {
            may_continue_plain_paragraph(content)
        }) || is_list_marker(content)
            || detect_fence_open(content).is_some()
            || thematic_break_marker(content).is_some()
            || detect_heading(content).is_some()
        {
            return None;
        }
        frame.paragraph.push(content);
        pos += 1;
    }
    while frames.len() > 1 {
        close_unified_list_frame(&mut frames)?;
    }
    let mut root = frames.pop()?;
    flush_unified_list_paragraph(&mut root)?;
    Some((build_unified_list(root.node, options), pos))
}

#[cfg(test)]
mod unified_list_tests {
    use super::*;

    #[test]
    fn shared_owner_builds_marked_quote() {
        for source in ["- one\n  > q\n", "- one\n\n  > q\n"] {
            let lines: Vec<&str> = source.lines().collect();
            let (list, consumed) = parse_unified_list(&lines, 0, &Options::default()).unwrap();
            assert_eq!(consumed, lines.len());
            assert!(list.tight);
            assert!(matches!(
                list.items[0].children[1],
                BlockNode::BlockQuote(_)
            ));
        }
        let lazy = "- one\n  > q\n  lazy continuation\n";
        let lines: Vec<&str> = lazy.lines().collect();
        assert!(parse_unified_list(&lines, 0, &Options::default()).is_none());

        let trailing_tab = "- one\n\n  >\t\n";
        let lines: Vec<&str> = trailing_tab.lines().collect();
        assert!(parse_unified_list(&lines, 0, &Options::default()).is_none());
    }

    #[test]
    fn shared_owner_accepts_loose_siblings_and_declines_lazy_lines() {
        let options = Options::default();
        for source in [
            "- a\n\n  second\n",
            "- a\n  - nested\n\n- b\n",
            "1. a\n   - nested\n\n2. b\n",
            "- a\n\n\n",
            "- a\n  - nested\n\n\nPara\n",
            "- root\n  * middle\n    - leaf\n",
            "- a\n  more\n  - child\n",
            "* a\n  more\n  - child\n",
            "1. a\n   more\n   - child\n",
        ] {
            let lines: Vec<&str> = source.lines().collect();
            assert!(
                parse_unified_list(&lines, 0, &options).is_some(),
                "{source:?}"
            );
        }
        for source in [
            "- a\nlazy line\n",
            "- a\n\n   wrong indent\n",
            "- a\n\n\n- b\n",
            "- a\n  - n\n\n\n  x\n",
        ] {
            let lines: Vec<&str> = source.lines().collect();
            assert!(
                parse_unified_list(&lines, 0, &options).is_none(),
                "{source:?}"
            );
        }
        let source = "- a\n  - b\n    - c\n";
        let lines: Vec<&str> = source.lines().collect();
        let _depth = DepthScope::set(MAX_NESTING_DEPTH - 5);
        assert!(parse_unified_list(&lines, 0, &options).is_some());
        drop(_depth);
        let _depth = DepthScope::set(MAX_NESTING_DEPTH - 4);
        assert!(parse_unified_list(&lines, 0, &options).is_none());
    }
}

fn parse_event_container(cur: &mut LineCursor<'_>, options: &Options<'_>) -> Box<BlockNode> {
    let (mut node, body) = open_container(cur, options);
    if !body.in_group && !body.inner.marker_hosted {
        let source = &body.inner.source.inner.lines[body.inner.start..body.inner.end];
        if !source.iter().any(|line| line.text.contains(":::")) {
            let lines: Vec<&str> = source.iter().map(|line| line.text.as_str()).collect();
            let _depth = DepthScope::set(body.depth);
            if let Some(_inner_depth) = DepthGuard::enter() {
                if let Some(children) = parse_blocks(&lines, options)
                    .or_else(|| parse_mixed_blocks::<false, false>(&lines, options))
                {
                    fill_container_children(&mut node, children);
                    return node;
                }
            }
        }
    }
    let mut out = vec![*node];
    resolve_pending_bodies(
        &mut out,
        vec![PendingBody {
            index: 0,
            body: Body::Colon(body),
        }],
        options,
    );
    Box::new(out.pop().expect("container body filled"))
}

fn apply_event_leading_attrs(block: &mut BlockNode, attrs: Attrs) {
    apply_attrs_to_block(block, attrs);
    resolve_code_title(block);
    if let BlockNode::CodeBlock(code) = block {
        if let Some(table) = delimited_table::from_code(code, &[]) {
            *block = BlockNode::Table(table);
        }
    }
    if let BlockNode::Figure(figure) = block {
        if let FigureTarget::CodeBlock(code) = &*figure.target {
            let mut code = code.clone();
            if let Some(attrs) = &figure.attrs {
                let mut target = BlockNode::CodeBlock(code.clone());
                apply_attrs_to_block(&mut target, attrs.clone());
                if let BlockNode::CodeBlock(with_attrs) = target {
                    code = with_attrs;
                }
            }
            if let Some(mut table) = delimited_table::from_code(&code, &[]) {
                table.caption = Some(figure.caption.clone());
                table.pos = figure.pos.clone();
                *block = BlockNode::Table(table);
            }
        }
    }
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
        parts.push(content);
        end += 1;
    }
    if parts.is_empty() {
        return None;
    }
    // A final blank quote marker can become a synthetic payload newline.
    if parts.last().is_some_and(|line| is_blank_line(line)) {
        let mut open = None;
        for line in &parts {
            if let Some(fence) = open {
                if is_fence_close(line, fence) {
                    open = None;
                }
            } else {
                open = detect_fence_open(line);
            }
        }
        if open.is_some() {
            return None;
        }
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
    let children = parse_blocks(&parts, options)
        .or_else(|| {
            let (list, consumed) = parse_simple_list(&parts, 0, options)?;
            (consumed == parts.len()).then(|| vec![BlockNode::List(list)])
        })
        .or_else(|| {
            if NESTING_DEPTH.with(|depth| depth.get() >= MIXED_QUOTE_DEPTH_LIMIT) {
                return None;
            }
            let _depth = DepthGuard::enter()?;
            parse_mixed_blocks::<false, false>(&parts, options)
        })?;
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

fn parse_simple_lazy_quote(
    lines: &[&str],
    start: usize,
    options: &Options<'_>,
) -> Option<(BlockQuote, usize)> {
    let mut parts = Vec::new();
    let mut end = start;
    let mut lazy = false;
    while let Some(line) = lines.get(end) {
        let content = if !lazy { line.strip_prefix("> ") } else { None };
        let text = content.unwrap_or(line);
        if !may_continue_plain_paragraph(text)
            || detect_heading(text).is_some()
            || is_list_marker(text)
            || is_definition_list_start(text)
            || (content.is_none() && detect_abbreviation_def(text).is_some())
            || detect_fence_open(text).is_some()
            || thematic_break_marker(text).is_some()
        {
            break;
        }
        if content.is_none() {
            lazy = true;
        }
        parts.push(trim_ascii_end(text));
        end += 1;
    }
    if !lazy || parts.is_empty() {
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
    Some((
        BlockQuote {
            attrs: None,
            children: vec![BlockNode::Paragraph(Paragraph {
                attrs: None,
                children: parse_event_inlines(&parts.join("\n"), options),
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
    fn fenced_quote_content_uses_the_recursive_quote_path() {
        let options = Options::default();
        for source in ["> ```\n> code\n> ```\n", "> ```=html\n> x\n> ```\n"] {
            let lines: Vec<&str> = source.lines().collect();
            let (quote, end) = parse_marked_quote(&lines, 0, &options).unwrap();
            assert_eq!(end, lines.len(), "{source:?}");
            let expected = parse_with_options(source, &options);
            assert_eq!(
                BlockNode::BlockQuote(quote),
                expected.children[0],
                "{source:?}"
            );
        }
        let unclosed = "> ```=html\n>\n";
        let lines: Vec<&str> = unclosed.lines().collect();
        assert!(parse_marked_quote(&lines, 0, &options).is_none());
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
    if let Some(inner) = text
        .strip_prefix("**")
        .and_then(|text| text.strip_suffix("**"))
    {
        if !inner.is_empty()
            && inner
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b' ')
        {
            return vec![InlineNode::text(text.to_string())];
        }
    }
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

#[cold]
#[inline(never)]
fn parse_simple_definition_list(
    lines: &[&str],
    start: usize,
    options: &Options<'_>,
) -> Option<(DefinitionList, usize)> {
    if start != 0 || options.positions || options.source_lines || !options.extensions.is_empty() {
        return None;
    }
    let mut pos = start;
    let mut entries = Vec::new();
    while let Some(line) = lines.get(pos) {
        let term = trim_ascii_end(trim_ascii_start(line.strip_prefix(":: ")?));
        if !term.as_bytes().first().is_some_and(u8::is_ascii_alphabetic) {
            return None;
        }
        pos += 1;
        let def = trim_ascii_end(trim_ascii_start(lines.get(pos)?.strip_prefix(":  ")?));
        if !def.as_bytes().first().is_some_and(u8::is_ascii_alphabetic) || is_list_marker(def) {
            return None;
        }
        pos += 1;
        entries.push((term, def));
        if !lines.get(pos).is_some_and(|line| line.starts_with(":: ")) {
            break;
        }
    }
    if entries.is_empty() || lines[pos..].iter().any(|line| !is_blank_line(line)) {
        return None;
    }
    let items = entries
        .into_iter()
        .map(|(term, def)| DefinitionItem {
            terms: vec![DefinitionTerm {
                attrs: None,
                children: parse_event_inlines(term, options),
                pos: None,
            }],
            definitions: vec![DefinitionDef {
                attrs: None,
                children: vec![BlockNode::Paragraph(Paragraph {
                    attrs: None,
                    children: parse_event_inlines(def, options),
                    at_content_column: true,
                    block_image: false,
                    pos: None,
                })],
                pos: None,
            }],
            pos: None,
        })
        .collect();
    Some((
        DefinitionList {
            attrs: None,
            items,
            loose: false,
            pos: None,
        },
        pos,
    ))
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
    let mut pending_items = Vec::new();
    let mut pos = start;
    while let Some((item_kind, ordinal, content)) =
        lines.get(pos).and_then(|line| simple_list_marker(line))
    {
        if item_kind != kind {
            break;
        }
        if kind == SimpleListKind::Ordered && ordinal != pending_items.len() + 1 {
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
        pending_items.push(item_lines);
        if lines.get(pos).is_some_and(|line| is_blank_line(line)) {
            let mut next = pos;
            while lines.get(next).is_some_and(|line| is_blank_line(line)) {
                next += 1;
            }
            if lines
                .get(next)
                .is_some_and(|line| is_list_marker(line) || line.starts_with([' ', '\t']))
            {
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
    let items = pending_items
        .into_iter()
        .map(|item_lines: Vec<&str>| {
            let inlines = if item_lines.len() == 1 {
                parse_event_inlines(item_lines[0], options)
            } else {
                parse_event_inlines(&item_lines.join("\n"), options)
            };
            ListItem {
                attrs: None,
                checked: None,
                task_state: None,
                children: vec![BlockNode::Paragraph(Paragraph {
                    attrs: None,
                    children: inlines,
                    at_content_column: true,
                    block_image: false,
                    pos: None,
                })],
                pos: None,
            }
        })
        .collect();
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

fn deep_list_node(kind: SimpleListKind) -> List {
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
        items: Vec::new(),
        pos: None,
    }
}

/// Match only the characters normalized by the event route.
fn needs_event_normalization(source: &str) -> bool {
    let bytes = source.as_bytes();
    if memchr::memchr2(b'\r', b'\0', bytes).is_some() {
        return true;
    }
    let mut start = 0;
    while let Some(offset) = memchr::memchr2(0xee, 0xef, &bytes[start..]) {
        let at = start + offset;
        if bytes[at..].starts_with(b"\xef\xbb\xbf")
            || bytes[at..].starts_with(b"\xee\x80\x85")
            || bytes[at..].starts_with(b"\xee\x80\x86")
        {
            return true;
        }
        start = at + 1;
    }
    false
}

fn has_fence_candidate_without_final_newline(source: &str) -> bool {
    if source.ends_with('\n') || !source.contains(['`', '~']) {
        return false;
    }
    if source.contains("[^") && (source.contains("```") || source.contains("~~~")) {
        return true;
    }
    source.lines().any(|line| {
        let mut content = trim_ascii_start(line);
        loop {
            if detect_fence_open(content).is_some() {
                return true;
            }
            if let Some(body) = strip_blockquote_prefix(content) {
                content = trim_ascii_start(body);
            } else if let Some(marker) = detect_list_marker_full(content) {
                content = trim_ascii_start(marker.content);
            } else {
                return false;
            }
        }
    })
}

/// The experimental route keeps unsupported source and option shapes on the
/// standard parser until their direct construction matches its public output.
pub(super) fn parse_document(source: &str) -> Option<(Document, bool)> {
    let source_len = source.len();
    let normalized = if needs_event_normalization(source) {
        if source.contains(['\u{e005}', '\u{e006}']) {
            return None;
        }
        Some(normalize_source(source))
    } else {
        None
    };
    let source = normalized.as_deref().unwrap_or(source);
    let may_have_link_defs = source.contains("]:");
    let may_have_abbrevs = source.contains("*[");
    if source.contains("[^") {
        return parse_document_with_footnotes(source, source_len);
    }
    if has_fence_candidate_without_final_newline(source) {
        return None;
    }
    let options = Options::default();
    let _placeholders = PlaceholderGuard::install(pick_definition_placeholders(source));
    let _continuation_markers = continuation_marker::Guard::install(source);
    let _source_ending = if !options.positions && source.ends_with('\n') {
        SourceEndingGuard::fragment()
    } else {
        SourceEndingGuard::install(source)
    };
    let (frontmatter, frontmatter_raw, body) = split_frontmatter(source, false);
    let (stripped_body, link_defs) = if may_have_link_defs {
        let mut budget = probe_budget_for(body.len());
        extract_link_defs_guarded(body, &options, &mut budget)
    } else {
        (String::new(), BTreeMap::new())
    };
    let has_link_defs = !link_defs.is_empty();
    let body_source = if has_link_defs {
        stripped_body.as_str()
    } else {
        body
    };
    let body_start_line = source[..body.as_ptr() as usize - source.as_ptr() as usize]
        .bytes()
        .filter(|byte| *byte == b'\n')
        .count()
        + 1;
    let root_line_map: Option<Vec<Option<usize>>> = body_source.contains('+').then(|| {
        (body_start_line..body_start_line + body_source.lines().count())
            .map(Some)
            .collect()
    });
    let (direct, link_defs, needs_late_resolution) = {
        let _depth = DepthGuard::enter()?;
        let lines: Vec<&str> = body_source.lines().collect();
        if has_link_defs {
            with_active_link_defs(link_defs, || {
                parse_blocks(&lines, &options).or_else(|| {
                    if may_have_abbrevs {
                        parse_mixed_blocks_mapped::<true, true>(
                            &lines,
                            &options,
                            root_line_map.as_deref(),
                        )
                    } else {
                        parse_mixed_blocks_mapped::<true, false>(
                            &lines,
                            &options,
                            root_line_map.as_deref(),
                        )
                    }
                })
            })
        } else {
            (
                parse_blocks(&lines, &options).or_else(|| {
                    if lines
                        .first()
                        .is_some_and(|line| is_definition_list_start(line))
                    {
                        if let Some((list, _)) = parse_simple_definition_list(&lines, 0, &options) {
                            return Some(vec![BlockNode::DefinitionList(list)]);
                        }
                    }
                    if may_have_abbrevs {
                        parse_mixed_blocks_mapped::<false, true>(
                            &lines,
                            &options,
                            root_line_map.as_deref(),
                        )
                    } else {
                        parse_mixed_blocks_mapped::<false, false>(
                            &lines,
                            &options,
                            root_line_map.as_deref(),
                        )
                    }
                }),
                link_defs,
                false,
            )
        }
    };
    if has_link_defs && direct.is_none() {
        return None;
    }
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
        source_len,
        ingest_payload_len: 0,
    };
    if needs_late_resolution || source.contains("][") {
        let headings = heading_reference_index_for_document(&doc, options.heading_id_options());
        resolve_reference_links(&mut doc, &link_defs, &headings);
    }
    append_link_reference_definitions(&mut doc, &link_defs, source, None);
    if may_have_abbrevs {
        apply_abbreviations(&mut doc);
    }
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

fn parse_document_with_footnotes(source: &str, source_len: usize) -> Option<(Document, bool)> {
    let may_have_abbrevs = source.contains("*[");
    if needs_event_normalization(source) || has_fence_candidate_without_final_newline(source) {
        return None;
    }
    if source.lines().any(|line| {
        line.contains("]:")
            && !trim_ascii_start(line).starts_with("[^")
            && detect_abbreviation_def(line).is_none()
    }) {
        return None;
    }
    let options = Options::default();
    let _placeholders = PlaceholderGuard::install(pick_definition_placeholders(source));
    let _continuation_markers = continuation_marker::Guard::install(source);
    let _source_ending = if !options.positions && source.ends_with('\n') {
        SourceEndingGuard::fragment()
    } else {
        SourceEndingGuard::install(source)
    };
    let (frontmatter, frontmatter_raw, body) = split_frontmatter(source, false);
    let body_start_line = source[..body.as_ptr() as usize - source.as_ptr() as usize]
        .bytes()
        .filter(|byte| *byte == b'\n')
        .count()
        + 1;
    let (mapped, footnote_sources, footnote_def_pos, note_links) =
        extract_footnote_defs(body, body_start_line, false, false, None, &options);
    if mapped
        .source
        .lines()
        .any(|line| line.starts_with([' ', '\t']) && is_definition_placeholder(line))
        || mapped
            .source
            .lines()
            .any(|line| line.contains("]:") && detect_abbreviation_def(line).is_none())
        || !note_links.is_empty()
        || footnote_sources
            .values()
            .any(|note| note.source.contains("[^") || note.source.contains("*["))
    {
        return None;
    }
    let children = {
        let _depth = DepthGuard::enter()?;
        let lines: Vec<&str> = mapped.source.lines().collect();
        parse_blocks(&lines, &options).or_else(|| {
            if may_have_abbrevs {
                parse_mixed_blocks::<true, true>(&lines, &options)
            } else {
                parse_mixed_blocks::<true, false>(&lines, &options)
            }
        })?
    };
    let footnote_defs = footnote_sources
        .into_iter()
        .map(|(label, note)| {
            let children = if note.source.trim_end_matches([' ', '\t', '\n']) == "{empty}" {
                Vec::new()
            } else {
                let lines: Vec<&str> = note.source.lines().collect();
                parse_blocks(&lines, &options)
                    .unwrap_or_else(|| parse_mapped_source(&note, &options))
            };
            (label, children)
        })
        .collect();
    let mut doc = Document {
        frontmatter,
        frontmatter_raw,
        footnote_defs,
        footnote_def_pos,
        children,
        source_len,
        ingest_payload_len: 0,
    };
    if source.contains("][") {
        let headings = heading_reference_index_for_document(&doc, options.heading_id_options());
        resolve_reference_links(&mut doc, &BTreeMap::new(), &headings);
    }
    if may_have_abbrevs {
        apply_abbreviations(&mut doc);
    }
    if source.contains('^') {
        number_crossref_captions(&mut doc);
    }
    stamp_generated_heading_ids(&mut doc, options.heading_id_options());
    let _ = crate::render::collect_footnotes(&mut doc, false);
    if source.contains("![") {
        promote_block_images(&mut doc.children, false, true);
        for blocks in doc.footnote_defs.values_mut() {
            promote_block_images(blocks, false, true);
        }
    }
    coalesce_text_runs(&mut doc);
    if source.contains("</#") {
        fill_crossref_hrefs(&mut doc, options.heading_id_options());
    }
    Some((doc, true))
}

#[cfg(test)]
mod link_definition_tests {
    use super::*;

    #[test]
    fn nested_parse_installs_its_own_definition_placeholders() {
        let _outer = PlaceholderGuard::install(['\u{e007}', '\u{e008}']);
        let source = "```\n%%\u{e007}\n```\n\n[ref]: /target\n\nSee [ref][]\n";
        let (actual, admitted) = parse_document(source).unwrap();
        assert!(admitted);
        let expected = parse_with_options(source, &Options::default());
        assert_eq!(
            crate::ast_json::to_json(&actual),
            crate::ast_json::to_json(&expected)
        );
    }

    #[test]
    fn nested_parse_without_definitions_resets_placeholder_pair() {
        let _outer = PlaceholderGuard::install(['\u{e007}', '\u{e008}']);
        let source = "A paragraph\n\n%%\u{e007}\n";
        let (actual, admitted) = parse_document(source).unwrap();
        assert!(!admitted);
        let expected = parse_with_options(source, &Options::default());
        assert_eq!(
            crate::ast_json::to_json(&actual),
            crate::ast_json::to_json(&expected)
        );
    }
}
