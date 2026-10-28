use super::*;

const MIXED_QUOTE_DEPTH_LIMIT: usize = 32;

#[cfg(test)]
mod mixed_definition_owner_tests {
    use super::*;

    #[test]
    fn keeps_following_root_blocks_and_declines_continuations() {
        for (source, start) in [
            ("# heading\n\n:: term\n:  definition\n\n# tail\n", 2),
            (":: a\n:  alpha\n:: b\n:  beta\n\nroot\n", 0),
        ] {
            let lines: Vec<&str> = source.lines().collect();
            assert!(
                parse_simple_definition_list_prefix(&lines, start, &Options::default(), false)
                    .is_some(),
                "{source:?}"
            );
            let (candidate, used) = parse_document(source).expect("event document");
            assert!(used, "{source:?}");
            assert_eq!(candidate, parse_with_options(source, &Options::default()));
        }
        for source in [
            ":: term\n:  definition\n a\n",
            ":: t\n:  head\n\n     %%%\n     a\n",
            ":: term\n:  definition\nroot\n",
            ":: a\n:  x\n\n:: b\n:  y\n",
        ] {
            let lines: Vec<&str> = source.lines().collect();
            assert!(
                parse_simple_definition_list_prefix(&lines, 0, &Options::default(), false)
                    .is_none(),
                "{source:?}"
            );
        }
    }

    #[test]
    fn repeated_mixed_definitions_match_full_document() {
        let source = (0..300)
            .map(|index| format!(":: term{index}\n:  definition{index}\n\nroot{index}\n\n"))
            .collect::<String>();
        let lines: Vec<&str> = source.lines().collect();
        let direct = lines
            .iter()
            .enumerate()
            .filter(|(_, line)| line.starts_with(":: "))
            .filter(|(at, _)| {
                parse_simple_definition_list_prefix(&lines, *at, &Options::default(), false)
                    .is_some()
            })
            .count();
        assert_eq!(direct, 300);
        let (candidate, used) = parse_document(&source).expect("event document");
        assert!(used);
        assert_eq!(candidate, parse_with_options(&source, &Options::default()));
    }
}

#[cfg(test)]
mod root_fence_owner_tests {
    use super::*;

    #[test]
    fn closed_root_code_fences_match_full_document() {
        for source in [
            "```\ncode\n```\n",
            "```rust\ncode\n```\n",
            "~~~~\n\n~~~~\n",
            "```\n```\n",
            "```\ncode\n```\n\n# next\n",
        ] {
            let lines: Vec<&str> = source.lines().collect();
            let open = detect_fence_open(lines[0]).unwrap();
            let (block, end) = parse_direct_root_code_fence(&lines, 0, open, &Options::default())
                .unwrap_or_else(|| panic!("{source:?}"));
            assert!(matches!(block, BlockNode::CodeBlock(_)));
            assert!(end >= 2);
            let (candidate, used) = parse_document(source).unwrap();
            assert!(used);
            assert_eq!(candidate, parse_with_options(source, &Options::default()));
        }
    }

    #[test]
    fn raw_caption_and_unclosed_fences_decline_direct_owner() {
        for source in [
            "```=html\n<b>x</b>\n```\n",
            "```\nx\n```\n^ caption\n",
            "```\nx\n```\n\n^ caption\n",
            "```\nx\n",
        ] {
            let lines: Vec<&str> = source.lines().collect();
            let open = detect_fence_open(lines[0]).unwrap();
            assert!(
                parse_direct_root_code_fence(&lines, 0, open, &Options::default()).is_none(),
                "{source:?}"
            );
            if let Some((candidate, _)) = parse_document(source) {
                assert_eq!(candidate, parse_with_options(source, &Options::default()));
            }
        }
    }

    #[test]
    fn root_fence_matrix_matches_full_document() {
        let mut direct = 0;
        for marker in ['`', '~'] {
            for count in [3, 4] {
                let fence = marker.to_string().repeat(count);
                for info in ["", " rust", " \"title\""] {
                    for payload in ["", "x\n", "x\n\n", "```\ninner\n"] {
                        for tail in ["", "\n", "# after\n", "^ caption\n"] {
                            let source = format!("{fence}{info}\n{payload}{fence}\n{tail}");
                            let lines: Vec<&str> = source.lines().collect();
                            let Some(open) = detect_fence_open(lines[0]) else {
                                continue;
                            };
                            direct += usize::from(
                                parse_direct_root_code_fence(&lines, 0, open, &Options::default())
                                    .is_some(),
                            );
                            if let Some((candidate, _)) = parse_document(&source) {
                                assert_eq!(
                                    candidate,
                                    parse_with_options(&source, &Options::default()),
                                    "{source:?}"
                                );
                            }
                        }
                    }
                }
            }
        }
        assert!(direct > 0);
    }

    #[test]
    fn repeated_root_code_fences_match_full_document() {
        let source = (0..300)
            .map(|index| format!("```rust\nlet item_{index} = {index};\n```\n\n"))
            .collect::<String>();
        let (candidate, used) = parse_document(&source).unwrap();
        assert!(used);
        assert_eq!(candidate, parse_with_options(&source, &Options::default()));
    }

    #[test]
    fn fenced_code_inside_quotes_and_containers_matches_full_document() {
        for source in [
            "> ```rust\n> code\n> ```\n",
            "::: note\n```rust\ncode\n```\n:::\n",
            "- ```rust\n  code\n  ```\n",
        ] {
            if let Some((candidate, _)) = parse_document(source) {
                assert_eq!(candidate, parse_with_options(source, &Options::default()));
            }
        }
    }

    #[test]
    fn direct_fence_waits_for_position_and_source_line_support() {
        let lines = ["```", "code", "```"];
        let open = detect_fence_open(lines[0]).unwrap();
        let positioned = Options {
            positions: true,
            ..Options::default()
        };
        assert!(parse_direct_root_code_fence(&lines, 0, open, &positioned).is_none());
        let source_lines = Options {
            source_lines: true,
            ..Options::default()
        };
        assert!(parse_direct_root_code_fence(&lines, 0, open, &source_lines).is_none());
    }
}

#[cfg(test)]
mod continuation_owner_tests {
    use super::*;

    #[test]
    fn non_marker_plus_keeps_direct_document_parity() {
        for source in [
            "plain a+b\n",
            "plain a + b\n",
            "code `+` inline\n",
            "- item + detail\n",
        ] {
            let _guard = continuation_marker::Guard::install(source);
            assert!(!continuation_marker::active(), "{source:?}");
            let (candidate, used) = parse_document(source).unwrap();
            assert!(used, "{source:?}");
            assert_eq!(candidate, parse_with_options(source, &Options::default()));
        }
    }

    #[test]
    fn owned_list_continuations_match_full_ast() {
        for source in ["- +\nx\n", "- a\n+\nb\n", "1. - +\npara\n"] {
            let _guard = continuation_marker::Guard::install(source);
            let lines: Vec<&str> = source.lines().collect();
            let line_map: Vec<Option<usize>> = (1..=lines.len()).map(Some).collect();
            let (_, consumed) =
                parse_unified_list_mapped(&lines, 0, &Options::default(), Some(&line_map))
                    .unwrap_or_else(|| panic!("{source:?}"));
            assert_eq!(consumed, lines.len(), "{source:?}");
            let (candidate, used) = parse_document(source).unwrap();
            assert!(used, "{source:?}");
            assert_eq!(
                candidate,
                parse_with_options(source, &Options::default()),
                "{source:?}"
            );
        }
    }

    #[test]
    fn refused_list_continuations_keep_the_standard_ast() {
        for source in [
            "- +\n  x\n",
            "- a\n  +\n  b\n",
            "- a\n  +\nb\n",
            "- a\n  > q\n  +\n  para\n",
            "- +\n\n",
        ] {
            if let Some((candidate, _)) = parse_document(source) {
                assert_eq!(
                    candidate,
                    parse_with_options(source, &Options::default()),
                    "{source:?}"
                );
            }
        }
    }

    #[test]
    fn list_continuation_owner_matrix_matches_full_document() {
        let mut direct = 0;
        let mut checked = 0;
        for marker in ["-", "1."] {
            for nested in ["", "- ", "1. "] {
                for suffix in ["", " ", "\t"] {
                    for follower in ["x", "# h", "> q", "- x", "```\ncode\n```", "***", "___"] {
                        for indent in 0..=4 {
                            let prefix = " ".repeat(indent);
                            let follower = follower
                                .lines()
                                .map(|line| format!("{prefix}{line}\n"))
                                .collect::<String>();
                            let source = format!("{marker} {nested}+{suffix}\n{follower}");
                            let _guard = continuation_marker::Guard::install(&source);
                            let lines: Vec<&str> = source.lines().collect();
                            let line_map: Vec<Option<usize>> =
                                (1..=lines.len()).map(Some).collect();
                            direct += usize::from(
                                parse_unified_list_mapped(
                                    &lines,
                                    0,
                                    &Options::default(),
                                    Some(&line_map),
                                )
                                .is_some(),
                            );
                            if let Some((candidate, _)) = parse_document(&source) {
                                assert_eq!(
                                    candidate,
                                    parse_with_options(&source, &Options::default()),
                                    "{source:?}"
                                );
                            }
                            checked += 1;
                        }
                    }
                }
            }
        }
        assert_eq!(checked, 630);
        assert!(direct > 0);
    }

    #[test]
    fn standalone_list_continuation_matrix_matches_full_document() {
        for marker in ["-", "1."] {
            for plus_indent in 0..=4 {
                for follower_indent in 0..=4 {
                    for follower in ["x", "# h", "> q", "- x", "```\ncode\n```", "***", "___"] {
                        let prefix = " ".repeat(follower_indent);
                        let follower = follower
                            .lines()
                            .map(|line| format!("{prefix}{line}\n"))
                            .collect::<String>();
                        let source =
                            format!("{marker} a\n{}+\n{follower}", " ".repeat(plus_indent));
                        if let Some((candidate, _)) = parse_document(&source) {
                            assert_eq!(
                                candidate,
                                parse_with_options(&source, &Options::default()),
                                "{source:?}"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn repeated_owned_list_continuations_match_full_document() {
        let source = (0..300)
            .map(|index| format!("- item {index}\n+\ncontinuation {index}\n\n"))
            .collect::<String>();
        let _guard = continuation_marker::Guard::install(&source);
        let lines: Vec<&str> = source.lines().collect();
        let line_map: Vec<Option<usize>> = (1..=lines.len()).map(Some).collect();
        let (_, consumed) =
            parse_unified_list_mapped(&lines, 0, &Options::default(), Some(&line_map))
                .expect("shared owner accepts the repeated list");
        assert!(lines[consumed..].iter().all(|line| is_blank_line(line)));
        let (candidate, used) = parse_document(&source).unwrap();
        assert!(used);
        assert_eq!(candidate, parse_with_options(&source, &Options::default()));
    }

    #[test]
    fn owned_list_continuation_marker_events_match_standard() {
        for source in ["- +\nx\n", "- a\n+\nb\n", "1. - +\npara\n"] {
            let (_, standard_events) =
                collecting_lint_events(|| parse_with_options(source, &Options::default()));
            let (_, event_events) = collecting_lint_events(|| parse_document(source));
            let markers = |events: Vec<LintEvent>| {
                events
                    .into_iter()
                    .filter_map(|event| match event {
                        LintEvent::Marker {
                            line,
                            column,
                            host,
                            reason,
                            first_block,
                        } => Some((line, column, host, reason, first_block)),
                        LintEvent::Fence { .. } => None,
                    })
                    .collect::<Vec<_>>()
            };
            assert_eq!(
                markers(event_events),
                markers(standard_events),
                "{source:?}"
            );
        }
    }

    #[test]
    fn continuation_followed_by_indented_content_declines_direct_owner() {
        for source in [
            "- +\nx\n  - c\n",
            "- +\nx\n  > q\n",
            "- +\nx\n  # h\n",
            "- +\nx\n  ```\n  ```\n",
            "- - +\nx\n\n    x\n",
            "- a\n+\nb\n  - c\n",
        ] {
            let _guard = continuation_marker::Guard::install(source);
            let lines: Vec<&str> = source.lines().collect();
            let line_map: Vec<Option<usize>> = (1..=lines.len()).map(Some).collect();
            assert!(
                parse_unified_list_mapped(&lines, 0, &Options::default(), Some(&line_map))
                    .is_none(),
                "{source:?}"
            );
            if let Some((candidate, _)) = parse_document(source) {
                assert_eq!(candidate, parse_with_options(source, &Options::default()));
            }
        }
    }
}

enum Event<'a> {
    Heading(usize, u8, &'a str),
    Paragraph(usize, &'a [&'a str]),
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
            let start = self.next;
            self.next += 1;
            return Some(Event::Heading(start, level, text));
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
        Some(Event::Paragraph(start, &self.lines[start..self.next]))
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
    parse_blocks_mapped(lines, options, None)
}

fn parse_blocks_mapped(
    lines: &[&str],
    options: &Options<'_>,
    line_map: Option<&[Option<usize>]>,
) -> Option<Vec<BlockNode>> {
    if options.positions
        || (options.source_lines && line_map.is_none())
        || !options.extensions.is_empty()
    {
        return None;
    }
    if !lines.iter().copied().all(supported_line) {
        return None;
    }
    let mut out = Vec::new();
    for event in (Events { lines, next: 0 }) {
        match event {
            Event::Heading(start, level, text) => {
                let mut node = BlockNode::Heading(Heading {
                    attrs: None,
                    level,
                    children: parse_event_inlines(trim_ascii_end(text), options),
                    pos: None,
                });
                if options.source_lines {
                    stamp_source_line(&mut node, line_map?.get(start).copied().flatten()?);
                }
                out.push(node);
            }
            Event::Paragraph(start, lines) => {
                let joined = lines
                    .iter()
                    .map(|line| trim_ascii_end(trim_ascii_start(line)))
                    .collect::<Vec<_>>()
                    .join("\n");
                let mut node = BlockNode::Paragraph(Paragraph {
                    attrs: None,
                    children: parse_event_inlines(&joined, options),
                    at_content_column: true,
                    block_image: false,
                    pos: None,
                });
                if options.source_lines {
                    stamp_source_line(&mut node, line_map?.get(start).copied().flatten()?);
                }
                out.push(node);
            }
        }
    }
    Some(out)
}

#[cfg(feature = "experimental-event-parser-options")]
fn parse_positioned_simple_blocks(
    lines: &[&str],
    options: &Options<'_>,
    line_map: &[Option<usize>],
) -> Option<Vec<BlockNode>> {
    if !options.positions || !lines.iter().copied().all(supported_line) {
        return None;
    }
    let col_map = vec![Some(0); lines.len()];
    let mut cur = LineCursor::new_with_cols(lines, Some(line_map), Some(&col_map));
    cur.at_document_level = true;
    let mut out = Vec::new();
    for event in (Events { lines, next: 0 }) {
        let (start, end, mut node) = match event {
            Event::Heading(start, level, text) => {
                cur.pos = start;
                let node = *parse_heading_block(&mut cur, level, text, options);
                (start, start + 1, node)
            }
            Event::Paragraph(start, paragraph_lines) => {
                cur.pos = start;
                let node = parse_paragraph(&mut cur, options);
                (start, start + paragraph_lines.len(), node)
            }
        };
        if cur.pos != end {
            return None;
        }
        if options.source_lines {
            stamp_source_line(&mut node, line_map.get(start).copied().flatten()?);
        }
        out.push(node);
    }
    Some(out)
}

/// Stream simple top-level blocks as events while handing list and definition
/// bodies to the existing readers. A construct outside this subset declines the
/// whole route before it can publish a partial AST.
fn parse_mixed_blocks<const DEFINITIONS: bool, const ABBREVIATIONS: bool>(
    lines: &[&str],
    options: &Options<'_>,
    at_document_level: bool,
) -> Option<Vec<BlockNode>> {
    parse_mixed_blocks_mapped::<DEFINITIONS, ABBREVIATIONS>(lines, options, None, at_document_level)
}

fn event_paragraph_needs_reader(
    next: &str,
    has_lines: bool,
    invalid_brace_line: bool,
    root_indented_text: bool,
) -> bool {
    (has_lines
        && (is_definition_list_start(next)
            || detect_fence_open(next).is_some()
            || thematic_break_marker(next).is_some()))
        || (!may_continue_plain_paragraph(next)
            && !is_list_marker(next)
            && !(!has_lines && invalid_brace_line)
            && !(!has_lines && root_indented_text))
}

fn refused_right_plus_at(cur: &LineCursor<'_>, line: &str) -> bool {
    trim_ascii(line) == "+"
        && cur
            .source_line(cur.pos)
            .and_then(continuation_marker::decision)
            .is_some_and(refused_right_list_plus)
}

fn refused_right_list_plus(decision: continuation_marker::Decision) -> bool {
    decision.owner == Some(continuation_marker::Kind::List)
        && decision.refusal == Some(continuation_marker::Refusal::Right)
        && decision.sibling_interrupt
}

fn parse_mixed_blocks_mapped<const DEFINITIONS: bool, const ABBREVIATIONS: bool>(
    lines: &[&str],
    options: &Options<'_>,
    line_map: Option<&[Option<usize>]>,
    at_document_level: bool,
) -> Option<Vec<BlockNode>> {
    if line_map.is_none()
        && (!at_document_level || continuation_marker::active())
        && lines.iter().copied().any(|line| {
            let line = trim_ascii(line);
            line == "+" || line.ends_with(" +")
        })
    {
        return None;
    }
    let col_map = options.positions.then(|| vec![Some(0); lines.len()]);
    let mut cur = LineCursor::new_with_cols(lines, line_map, col_map.as_deref());
    cur.at_document_level = at_document_level;
    let mut out = Vec::new();
    let mut block_start_indices = Vec::new();
    let current_block_start = std::cell::Cell::new(0usize);
    #[cfg(feature = "experimental-event-parser-options")]
    let mut source_lines = Vec::new();
    macro_rules! emit {
        ($line:expr, $node:expr) => {{
            if options.positions {
                block_start_indices.push(current_block_start.get());
            }
            #[cfg(feature = "experimental-event-parser-options")]
            if options.source_lines {
                source_lines.push($line);
            }
            out.push($node);
        }};
    }
    let mut leading_attrs: Vec<(usize, Attrs)> = Vec::new();
    let mut marker_fence_exit_at = None;
    'blocks: while cur.peek().is_some() {
        if options.positions {
            current_block_start.set(cur.pos);
        }
        let _block_source_line = cur.source_line(cur.pos);
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
        match take_comment_block(&mut cur, options) {
            CommentBlock::NotAComment => {}
            CommentBlock::Consumed(None) => continue,
            CommentBlock::Consumed(Some(node)) => {
                emit!(_block_source_line, *node);
                continue;
            }
        }
        let line = cur.peek().unwrap();
        if ABBREVIATIONS {
            if let Some(definition) = detect_abbreviation_def(line) {
                cur.consume();
                emit!(_block_source_line, BlockNode::AbbreviationDef(definition));
                continue;
            }
        }
        if let Some((level, text)) = detect_heading(line) {
            emit!(
                _block_source_line,
                *parse_heading_block(&mut cur, level, text, options)
            );
            continue;
        }
        if is_list_marker(line) && !line.starts_with([' ', '\t']) {
            let blank_then_indented = lines
                .get(cur.pos + 1)
                .is_some_and(|line| is_blank_line(line))
                && lines[cur.pos + 2..]
                    .iter()
                    .find(|line| !is_blank_line(line))
                    .is_some_and(|line| line.starts_with([' ', '\t']));
            let direct = if options.source_lines || options.positions {
                None
            } else if blank_then_indented {
                parse_unified_list_mapped_with_exit(lines, cur.pos, options, line_map)
            } else {
                parse_simple_list(lines, cur.pos, options)
                    .map(|(list, end)| UnifiedListRead {
                        list,
                        end,
                        marker_fence_exit: false,
                    })
                    .or_else(|| {
                        parse_unified_list_mapped_with_exit(lines, cur.pos, options, line_map)
                    })
            };
            if let Some(read) = direct {
                cur.pos = read.end;
                marker_fence_exit_at = read.marker_fence_exit.then_some(read.end);
                emit!(_block_source_line, BlockNode::List(read.list));
            } else {
                let mut item_bodies = Vec::new();
                let list = parse_list(&mut cur, options, &mut item_bodies);
                emit!(
                    _block_source_line,
                    *drained_list_bodies(list, item_bodies, options)
                );
            }
            continue;
        }
        if is_definition_list_start(line) {
            let direct = if options.source_lines || options.positions {
                None
            } else {
                parse_simple_definition_list_prefix(lines, cur.pos, options, false)
            };
            if let Some((list, end)) = direct {
                cur.pos = end;
                emit!(_block_source_line, BlockNode::DefinitionList(list));
            } else {
                emit!(_block_source_line, parse_definition_list(&mut cur, options));
            }
            continue;
        }
        if strip_blockquote_prefix(line).is_some() {
            let direct = if options.source_lines || options.positions || line.contains("![") {
                None
            } else {
                parse_simple_quote(lines, cur.pos, options)
                    .or_else(|| parse_marked_quote(lines, cur.pos, options))
                    .or_else(|| parse_simple_lazy_quote(lines, cur.pos, options))
            };
            if let Some((quote, end)) = direct {
                cur.pos = end;
                emit!(_block_source_line, BlockNode::BlockQuote(quote));
            } else {
                emit!(_block_source_line, *parse_blockquote(&mut cur, options));
            }
            continue;
        }
        if let Some(fence) = detect_fence_open(line) {
            let mut block = if let Some((block, end)) =
                parse_direct_root_code_fence(lines, cur.pos, fence, options)
            {
                cur.pos = end;
                block
            } else {
                *parse_fenced_block(&mut cur, fence, options)
            };
            let has_leading_attrs = leading_attrs.last().is_some_and(|(start, _)| {
                out[*start..].iter().all(|node| {
                    matches!(node, BlockNode::AbbreviationDef(_) | BlockNode::Comment(_))
                })
            });
            if !has_leading_attrs {
                resolve_code_title(&mut block);
            }
            emit!(_block_source_line, block);
            continue;
        }
        if let Some(marker) = thematic_break_marker(line) {
            let at = cur.pos;
            cur.consume();
            emit!(
                _block_source_line,
                BlockNode::ThematicBreak(ThematicBreak {
                    marker: (marker != '-').then_some(marker),
                    pos: span_of(&cur, at, cur.pos, options),
                    ..Default::default()
                })
            );
            continue;
        }
        if !line.starts_with([' ', '\t']) && is_table_start(line) {
            emit!(_block_source_line, parse_table(&mut cur, options));
            continue;
        }
        if let Some(mut image) = detect_block_image(line) {
            if image_is_block(&mut cur) {
                let image_at = cur.pos;
                cur.consume();
                image.pos = span_of(&cur, image_at, image_at + 1, options);
                if let Some(caption) = consume_caption(&mut cur, options) {
                    emit!(
                        _block_source_line,
                        BlockNode::Figure(Figure {
                            attrs: None,
                            target: Box::new(FigureTarget::Image(image)),
                            rendered_target: None,
                            caption,
                            short_caption: None,
                            pos: span_of(&cur, image_at, cur.pos, options),
                        })
                    );
                } else {
                    emit!(_block_source_line, BlockNode::BlockImage(image));
                }
                continue;
            }
        }
        let root_indented_text = if line.starts_with(' ') {
            let content = trim_ascii_start(line);
            content
                .as_bytes()
                .first()
                .is_some_and(|byte| *byte == b'>' || byte.is_ascii_alphabetic())
                && !is_list_marker(content)
        } else {
            false
        };
        if trim_ascii_start(line).starts_with("![") && detect_block_image(line).is_some() {
            let mut item_bodies = Vec::new();
            let block = parse_block(&mut cur, options, &mut item_bodies)?;
            if !item_bodies.is_empty() {
                return None;
            }
            emit!(_block_source_line, *block);
            continue;
        }
        if line.starts_with("![")
            && lines
                .get(cur.pos + 1)
                .is_some_and(|next| is_blank_line(next))
            && lines
                .get(cur.pos + 2)
                .is_some_and(|next| caption_content(next).is_some())
        {
            emit!(_block_source_line, parse_paragraph(&mut cur, options));
            continue;
        }
        if !may_start_plain_paragraph(line) && !invalid_brace_line && !root_indented_text {
            if line.starts_with(":::") {
                if detect_line_block_open(line).is_some() {
                    emit!(_block_source_line, parse_line_block(&mut cur, options));
                    continue;
                }
                if detect_hardbreaks_block_open(line).is_some() {
                    emit!(
                        _block_source_line,
                        parse_hardbreaks_block(&mut cur, options)
                    );
                    continue;
                }
                if detect_quote_block_open(line).is_some() {
                    emit!(_block_source_line, parse_quote_block(&mut cur, options));
                    continue;
                }
                if detect_container_open(line).is_some() {
                    emit!(
                        _block_source_line,
                        *parse_event_container(&mut cur, options)
                    );
                    continue;
                }
            }
            if trim_ascii_start(line).starts_with("$$`") {
                if let Some(equation) = parse_equation_block(&mut cur, options) {
                    emit!(_block_source_line, equation);
                    continue;
                }
            }
            if is_list_marker(trim_ascii_start(line))
                || trim_ascii(line) == "+"
                    && !(marker_fence_exit_at == Some(cur.pos) && refused_right_plus_at(&cur, line))
                || line.ends_with(" +")
            {
                return None;
            }
            emit!(_block_source_line, parse_paragraph(&mut cur, options));
            continue;
        }
        if options.positions {
            emit!(_block_source_line, parse_paragraph(&mut cur, options));
            continue;
        }
        let paragraph_start = cur.pos;
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
            if event_paragraph_needs_reader(
                next,
                !paragraph_lines.is_empty(),
                invalid_brace_line,
                root_indented_text,
            ) {
                if !at_document_level {
                    return None;
                }
                cur.pos = paragraph_start;
                emit!(_block_source_line, parse_paragraph(&mut cur, options));
                continue 'blocks;
            }
            paragraph_lines.push(trim_ascii_end(trim_ascii_start(next)));
            cur.consume();
        }
        let joined = paragraph_lines.join("\n");
        emit!(
            _block_source_line,
            BlockNode::Paragraph(Paragraph {
                attrs: None,
                children: parse_event_inlines(&joined, options),
                at_content_column: !line.starts_with([' ', '\t']),
                block_image: false,
                pos: None,
            })
        );
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
                    apply_event_leading_attrs(
                        &mut out[index],
                        attrs,
                        &cur,
                        block_start_indices.get(index).copied().unwrap_or(0),
                        options,
                    );
                }
                target = Some(next_target);
                slots = None;
            }
            merge_attrs_indexed(&mut pending, attrs, &mut slots);
        }
        if let (Some(index), Some(attrs)) = (target, pending) {
            apply_event_leading_attrs(
                &mut out[index],
                attrs,
                &cur,
                block_start_indices.get(index).copied().unwrap_or(0),
                options,
            );
        }
    }
    #[cfg(feature = "experimental-event-parser-options")]
    if options.source_lines {
        for (node, line) in out.iter_mut().zip(source_lines) {
            if let Some(line) = line {
                stamp_source_line(node, line);
            }
        }
    }
    Some(out)
}

#[cold]
#[inline(never)]
fn parse_direct_root_code_fence(
    lines: &[&str],
    start: usize,
    open: FenceOpen,
    options: &Options<'_>,
) -> Option<(BlockNode, usize)> {
    if options.positions || options.source_lines || !options.extensions.is_empty() {
        return None;
    }
    let first = *lines.get(start)?;
    if first[open.lang_start..].trim().starts_with('=') {
        return None;
    }
    let close = (start + 1..lines.len()).find(|&at| {
        is_fence_close(lines[at], open)
            || lines[at].starts_with(LAZY) && is_fence_close(strip_lazy(lines[at]), open)
    })?;
    if lines[close].starts_with(LAZY) {
        return None;
    }
    if lines[start + 1..close]
        .iter()
        .any(|line| line.starts_with(LAZY) || is_definition_placeholder(line))
    {
        return None;
    }
    let next = close + 1;
    if lines
        .get(next)
        .and_then(|line| caption_content(line))
        .is_some()
        || lines.get(next).is_some_and(|line| is_blank_line(line))
            && lines
                .get(next + 1)
                .and_then(|line| caption_content(line))
                .is_some()
    {
        return None;
    }
    let mut content = lines[start + 1..close].join("\n");
    if close > start + 1 {
        content.push('\n');
    }
    let lang = (open.lang_start < open.lang_end)
        .then(|| first[open.lang_start..open.lang_end].to_string());
    let title = open
        .title_start
        .zip(open.title_end)
        .map(|(begin, end)| first[begin..end].to_string());
    let label = open
        .label_start
        .zip(open.label_end)
        .map(|(begin, end)| first[begin..end].to_string());
    Some((
        BlockNode::CodeBlock(CodeBlock {
            attrs: None,
            lang,
            title,
            label,
            content,
            pos: None,
        }),
        next,
    ))
}

enum UnifiedListBlock<'a> {
    Paragraph(Vec<&'a str>),
    List(UnifiedListNode<'a>),
    Block(Box<BlockNode>),
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
    continuation_indent: Option<usize>,
    marker_fence_end: Option<usize>,
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

fn resume_nested_list_after_blank(frames: &mut [UnifiedListFrame<'_>], next: &str) -> Option<()> {
    let next_indent = next.bytes().take_while(|byte| *byte == b' ').count();
    let frame = frames.last_mut()?;
    if next_indent == frame.content_indent {
        let content = &next[next_indent..];
        if strip_blockquote_prefix(content).is_some()
            || detect_fence_open(content).is_some()
            || detect_heading(content).is_some()
        {
            flush_unified_list_paragraph(frame)?;
            return Some(());
        }
        if !is_list_marker(content) && may_start_plain_paragraph(content) {
            flush_unified_list_paragraph(frame)?;
            frame.node.tight = false;
            return Some(());
        }
    }
    if next_indent == frame.indent
        && unified_list_marker(&next[next_indent..]).is_some_and(|(kind, _, _)| kind == frame.kind)
    {
        frame.node.tight = false;
        return Some(());
    }
    None
}

fn consume_pending_list_continuation<'a>(
    frames: &mut [UnifiedListFrame<'a>],
    line: &'a str,
    following: &[&str],
) -> Option<bool> {
    let Some(required_indent) = frames.last().and_then(|frame| frame.continuation_indent) else {
        return Some(false);
    };
    let indent = line.bytes().take_while(|byte| *byte == b' ').count();
    if indent != required_indent {
        return None;
    }
    if following
        .iter()
        .find(|next| !is_blank_line(next))
        .is_some_and(|next| next.starts_with([' ', '\t']))
    {
        return None;
    }
    let content = trim_ascii_end(&line[indent..]);
    if !may_start_plain_paragraph(content)
        || is_list_marker(content)
        || detect_fence_open(content).is_some()
        || detect_heading(content).is_some()
        || thematic_break_marker(content).is_some()
    {
        return None;
    }
    let frame = frames.last_mut()?;
    frame.continuation_indent = None;
    frame.paragraph.push(content);
    Some(true)
}

fn start_standalone_list_continuation(
    frames: &mut [UnifiedListFrame<'_>],
    indent: usize,
    decision: Option<continuation_marker::Decision>,
) -> Option<()> {
    let frame = frames.last_mut()?;
    if indent != frame.indent
        || !decision.is_some_and(|decision| {
            decision.owner == Some(continuation_marker::Kind::List)
                && decision.refusal.is_none()
                && !decision.first_block
        })
    {
        return None;
    }
    flush_unified_list_paragraph(frame)?;
    frame.continuation_indent = Some(indent);
    Some(())
}

fn start_marker_line_list_continuation(
    frames: &mut [UnifiedListFrame<'_>],
    decision: Option<continuation_marker::Decision>,
) -> Option<()> {
    owned_list_first_block(decision).then_some(())?;
    let follow_indent = frames.first()?.indent;
    frames.last_mut()?.continuation_indent = Some(follow_indent);
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
                    UnifiedListBlock::Block(block) => *block,
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

#[cold]
#[inline(never)]
fn parse_unified_list_fence(
    lines: &[&str],
    start: usize,
    content_indent: usize,
    options: &Options<'_>,
) -> Option<(BlockNode, usize)> {
    let read = read_unified_list_fence(lines, start, content_indent, None, options)?;
    Some((read.block, read.end))
}

fn marker_fence_boundary(
    frames: &[UnifiedListFrame<'_>],
    line: &str,
    line_map: Option<&[Option<usize>]>,
    pos: usize,
) -> Option<bool> {
    if frames.len() != 1 || frames[0].marker_fence_end != Some(pos) {
        return None;
    }
    let frame = &frames[0];
    if line.bytes().take_while(|byte| *byte == b' ').count() >= frame.content_indent {
        return None;
    }
    let decision = list_continuation_decision(line_map, pos);
    Some(trim_ascii(line) == "+" && decision.is_some_and(refused_right_list_plus))
}

struct ListFenceRead {
    block: BlockNode,
    end: usize,
}

#[cold]
#[inline(never)]
fn read_unified_list_fence(
    lines: &[&str],
    start: usize,
    content_indent: usize,
    marker_content: Option<&str>,
    options: &Options<'_>,
) -> Option<ListFenceRead> {
    let prefix = " ".repeat(content_indent);
    let first = marker_content.or_else(|| lines.get(start)?.strip_prefix(prefix.as_str()))?;
    let open = detect_fence_open(first)?;
    let mut collected = vec![first];
    let mut end = start + 1;
    let mut closed = false;
    while let Some(&line) = lines.get(end) {
        let content = if is_blank_line(line) {
            if line.len() > content_indent || !line.bytes().all(|byte| byte == b' ') {
                return None;
            }
            ""
        } else {
            match line.strip_prefix(prefix.as_str()) {
                Some(content) => content,
                None => break,
            }
        };
        collected.push(content);
        end += 1;
        if is_fence_close(content, open) {
            closed = true;
            break;
        }
    }
    if marker_content.is_some() && (closed || end == lines.len())
        || marker_content.is_none() && !closed
    {
        return None;
    }
    let mut cur = LineCursor::new_with_cols(&collected, None, None);
    let mut block = *parse_fenced_block(&mut cur, open, options);
    if cur.pos != collected.len() {
        return None;
    }
    resolve_code_title(&mut block);
    Some(ListFenceRead { block, end })
}

#[inline(never)]
#[cfg(test)]
fn parse_unified_list(
    lines: &[&str],
    start: usize,
    options: &Options<'_>,
) -> Option<(List, usize)> {
    parse_unified_list_mapped(lines, start, options, None)
}

fn direct_list_image_line_is_supported(content: &str) -> Option<()> {
    detect_block_image(content).is_none().then_some(())
}

struct UnifiedListRead {
    list: List,
    end: usize,
    marker_fence_exit: bool,
}

fn unified_list_paragraph_accepts(content: &str, has_paragraph: bool) -> bool {
    (if has_paragraph {
        may_continue_plain_paragraph(content)
    } else {
        may_start_plain_paragraph(content)
    }) && !is_list_marker(content)
        && detect_fence_open(content).is_none()
        && thematic_break_marker(content).is_none()
}

enum BlankListStep {
    Continue(usize),
    Break,
}

fn advance_unified_list_blank(
    lines: &[&str],
    pos: usize,
    frames: &mut Vec<UnifiedListFrame<'_>>,
) -> Option<BlankListStep> {
    let mut after = pos;
    while lines.get(after).is_some_and(|line| is_blank_line(line)) {
        after += 1;
    }
    let Some(&next) = lines.get(after) else {
        return Some(BlankListStep::Break);
    };
    if after - pos != 1 {
        if !next.starts_with([' ', '\t']) && !is_list_marker(next) {
            while frames.len() > 1 {
                close_unified_list_frame(frames)?;
            }
            return Some(BlankListStep::Break);
        }
        return None;
    }
    if frames.len() > 1 {
        if next.starts_with([' ', '\t']) {
            resume_nested_list_after_blank(frames, next)?;
            return Some(BlankListStep::Continue(after));
        }
        while frames.len() > 1 {
            close_unified_list_frame(frames)?;
        }
    }
    let frame = frames.last_mut()?;
    if let Some((kind, _, _)) = unified_list_marker(next) {
        if kind != frame.kind {
            return Some(BlankListStep::Break);
        }
        frame.node.tight = false;
        return Some(BlankListStep::Continue(after));
    }
    let next_indent = next.bytes().take_while(|byte| *byte == b' ').count();
    if next_indent == frame.content_indent && unified_list_marker(&next[next_indent..]).is_some() {
        return Some(BlankListStep::Continue(after));
    }
    if is_list_marker(next) {
        return None;
    }
    if next_indent == frame.content_indent {
        flush_unified_list_paragraph(frame)?;
        if strip_blockquote_prefix(&next[next_indent..]).is_none()
            && detect_fence_open(&next[next_indent..]).is_none()
            && detect_heading(&next[next_indent..]).is_none()
        {
            frame.node.tight = false;
        }
        return Some(BlankListStep::Continue(after));
    }
    if next.starts_with([' ', '\t']) {
        if next.as_bytes().get(next_indent) != Some(&b'\t')
            && next_indent < frames[0].content_indent
        {
            while frames.len() > 1 {
                close_unified_list_frame(frames)?;
            }
            return Some(BlankListStep::Break);
        }
        return None;
    }
    Some(BlankListStep::Break)
}

#[cfg(test)]
fn parse_unified_list_mapped(
    lines: &[&str],
    start: usize,
    options: &Options<'_>,
    line_map: Option<&[Option<usize>]>,
) -> Option<(List, usize)> {
    let read = parse_unified_list_mapped_with_exit(lines, start, options, line_map)?;
    Some((read.list, read.end))
}

fn parse_unified_list_mapped_with_exit(
    lines: &[&str],
    start: usize,
    options: &Options<'_>,
    line_map: Option<&[Option<usize>]>,
) -> Option<UnifiedListRead> {
    let base_depth = NESTING_DEPTH.with(|depth| depth.get());
    // Leave room for the standard parser's list item and paragraph scopes.
    if base_depth.saturating_add(2) >= MAX_NESTING_DEPTH {
        return None;
    }
    let mut frames: Vec<UnifiedListFrame<'_>> = Vec::new();
    let mut pos = start;
    let mut marker_fence_exit = false;
    while let Some(&line) = lines.get(pos) {
        if let Some(should_close) = marker_fence_boundary(&frames, line, line_map, pos) {
            if should_close {
                marker_fence_exit = true;
                break;
            }
            return None;
        }
        if consume_pending_list_continuation(&mut frames, line, &lines[pos + 1..])? {
            pos += 1;
            continue;
        }
        if is_blank_line(line) {
            match advance_unified_list_blank(lines, pos, &mut frames)? {
                BlankListStep::Continue(after) => {
                    pos = after;
                    continue;
                }
                BlankListStep::Break => break,
            }
        }
        let indent = line.bytes().take_while(|byte| *byte == b' ').count();
        let decision = trim_ascii_end(line)
            .ends_with('+')
            .then(|| list_continuation_decision(line_map, pos))
            .flatten();
        if is_plus_marker(&line[indent..]) {
            start_standalone_list_continuation(&mut frames, indent, decision)?;
            pos += 1;
            continue;
        }
        let marker = unified_list_marker(&line[indent..])
            .or_else(|| {
                let marker = strict_list_marker(&line[indent..])?;
                detect_fence_open(marker.2).map(|_| marker)
            })
            .or_else(|| plus_first_block_marker(&line[indent..], decision));
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
                    continuation_indent: None,
                    marker_fence_end: None,
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
                    continuation_indent: None,
                    marker_fence_end: None,
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
            let mut content = content;
            while let Some((nested_kind, nested_ordinal, nested_content)) =
                strict_list_marker(content)
            {
                if nested_kind == SimpleListKind::Ordered && nested_ordinal != 1 {
                    return None;
                }
                if base_depth.saturating_add(frames.len() + 2) >= MAX_NESTING_DEPTH {
                    return None;
                }
                let nested_indent = frames.last()?.content_indent;
                frames.push(UnifiedListFrame {
                    node: UnifiedListNode {
                        kind: nested_kind,
                        tight: true,
                        items: vec![UnifiedListItem { blocks: Vec::new() }],
                    },
                    kind: nested_kind,
                    indent: nested_indent,
                    content_indent: nested_indent
                        + if nested_kind == SimpleListKind::Ordered {
                            3
                        } else {
                            2
                        },
                    paragraph: Vec::new(),
                    continuation_indent: None,
                    marker_fence_end: None,
                });
                content = nested_content;
            }
            if is_plus_marker(content) {
                start_marker_line_list_continuation(&mut frames, decision)?;
            } else if detect_fence_open(content).is_some() {
                if frames.len() != 1 {
                    return None;
                }
                let frame = frames.last_mut()?;
                let read = read_unified_list_fence(
                    lines,
                    pos,
                    frame.content_indent,
                    Some(content),
                    options,
                )?;
                frame
                    .node
                    .items
                    .last_mut()?
                    .blocks
                    .push(UnifiedListBlock::Block(Box::new(read.block)));
                frame.marker_fence_end = Some(read.end);
                pos = read.end;
                continue;
            } else {
                direct_list_image_line_is_supported(content)?;
                frames.last_mut()?.paragraph.push(content);
            }
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
        direct_list_image_line_is_supported(content)?;
        if detect_fence_open(content).is_some() {
            let (block, end) = parse_unified_list_fence(lines, pos, frame.content_indent, options)?;
            flush_unified_list_paragraph(frame)?;
            frame
                .node
                .items
                .last_mut()?
                .blocks
                .push(UnifiedListBlock::Block(Box::new(block)));
            pos = end;
            continue;
        }
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
                .push(UnifiedListBlock::Block(Box::new(BlockNode::BlockQuote(
                    quote,
                ))));
            pos = end;
            continue;
        }
        if let Some((level, text)) = detect_heading(content) {
            flush_unified_list_paragraph(frame)?;
            frame
                .node
                .items
                .last_mut()?
                .blocks
                .push(UnifiedListBlock::Block(Box::new(BlockNode::Heading(
                    Heading {
                        attrs: None,
                        level,
                        children: parse_event_inlines(trim_ascii_end(text), options),
                        pos: None,
                    },
                ))));
            pos += 1;
            continue;
        }
        if !unified_list_paragraph_accepts(content, !frame.paragraph.is_empty()) {
            return None;
        }
        frame.paragraph.push(content);
        pos += 1;
    }
    if frames
        .iter()
        .any(|frame| frame.continuation_indent.is_some())
    {
        return None;
    }
    while frames.len() > 1 {
        close_unified_list_frame(&mut frames)?;
    }
    let mut root = frames.pop()?;
    flush_unified_list_paragraph(&mut root)?;
    Some(UnifiedListRead {
        list: build_unified_list(root.node, options),
        end: pos,
        marker_fence_exit,
    })
}

#[cfg(test)]
mod unified_list_tests {
    use super::*;

    #[test]
    fn marker_fence_frame_stops_at_refused_plus() {
        let source = "- ```\n+\n  x\n";
        let _markers = continuation_marker::Guard::install(source);
        let lines: Vec<&str> = source.lines().collect();
        let line_map: Vec<Option<usize>> = (1..=lines.len()).map(Some).collect();
        let (list, end) =
            parse_unified_list_mapped(&lines, 0, &Options::default(), Some(&line_map)).unwrap();
        assert_eq!(end, 1);
        assert!(matches!(list.items[0].children[0], BlockNode::CodeBlock(_)));
    }

    #[test]
    fn shared_owner_builds_item_headings() {
        for source in ["- a\n  # H\n", "- a\n\n  # H\n"] {
            let lines: Vec<&str> = source.lines().collect();
            let (list, consumed) = parse_unified_list(&lines, 0, &Options::default()).unwrap();
            assert_eq!(consumed, lines.len());
            assert!(list.tight);
            assert!(matches!(list.items[0].children[1], BlockNode::Heading(_)));
        }
    }

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
    fn post_blank_tab_does_not_close_list_at_space_count() {
        let source = "- one\n\n \t> q\n";
        let lines: Vec<&str> = source.lines().collect();
        assert!(parse_unified_list(&lines, 0, &Options::default()).is_none());
    }

    #[test]
    fn shared_owner_builds_closed_fences() {
        for source in [
            "- a\n  ```rs\n  let x = 1;\n  ```\n",
            "- a\n\n  ```rs\n  let x = 1;\n  ```\n",
            "- a\n\n  ```rs\n  let x = 1;\n  ```\n\n- b\n",
            "- a\n  ```rs\n  let x = 1;\n  ```\n\n  following\n",
            "- a\n  ```=latex\n  \\x\n  ```\n",
            "- a\n  ```rs\n  let x = 1;\n  ```\n- b\n",
        ] {
            let lines: Vec<&str> = source.lines().collect();
            let (list, consumed) = parse_unified_list(&lines, 0, &Options::default()).unwrap();
            assert_eq!(consumed, lines.len());
            let actual = BlockNode::List(list);
            let expected = parse_with_options(source, &Options::default());
            assert_eq!(actual, expected.children[0], "{source:?}");
        }
        let mut checked = 0;
        for marker in ["```", "~~~~", "````"] {
            for before in ["", "\n"] {
                for after in ["", "\n", "\n- b\n"] {
                    for body in ["code", "", "code\n  more"] {
                        let source =
                            format!("- a\n{before}  {marker}rs\n  {body}\n  {marker}\n{after}");
                        let lines: Vec<&str> = source.lines().collect();
                        let Some((list, consumed)) =
                            parse_unified_list(&lines, 0, &Options::default())
                        else {
                            continue;
                        };
                        if consumed != lines.len() {
                            continue;
                        }
                        let expected = parse_with_options(&source, &Options::default());
                        assert_eq!(BlockNode::List(list), expected.children[0], "{source:?}");
                        checked += 1;
                    }
                }
            }
        }
        for source in [
            "- a\n  ```rs\n  code\n  ```",
            "- a\n  ```=latex\n  \\x\n  ```\n- b\n",
            "- a\n  ```csv\n  x,y\n  1,2\n  ```\n",
            "- a\n  ```\n\n  code\n  ```\n",
        ] {
            let lines: Vec<&str> = source.lines().collect();
            let (list, consumed) = parse_unified_list(&lines, 0, &Options::default())
                .unwrap_or_else(|| panic!("{source:?}"));
            assert_eq!(consumed, lines.len(), "{source:?}");
            let expected = parse_with_options(source, &Options::default());
            assert_eq!(BlockNode::List(list), expected.children[0], "{source:?}");
            checked += 1;
        }
        assert!(checked >= 20);
        for blank in ["   ", "      ", "\t", "\x0c"] {
            let source = format!("- a\n  ```\n  x\n{blank}\n  y\n  ```\n");
            let lines: Vec<&str> = source.lines().collect();
            assert!(
                parse_unified_list(&lines, 0, &Options::default()).is_none(),
                "{source:?}"
            );
            let (actual, _) = parse_document(&source).unwrap();
            let expected = parse_with_options(&source, &Options::default());
            assert_eq!(
                crate::ast_json::to_json(&actual),
                crate::ast_json::to_json(&expected)
            );
        }
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

    #[test]
    fn shared_owner_keeps_blank_separated_nested_item_content() {
        let options = Options::default();
        for source in [
            "- - A\n\n    second\n  - B\n",
            "- - A\n\n  - B\n",
            "- - A\n\n    second\n\n  - B\n",
        ] {
            let lines: Vec<&str> = source.lines().collect();
            let (list, consumed) =
                parse_unified_list(&lines, 0, &options).unwrap_or_else(|| panic!("{source:?}"));
            assert_eq!(consumed, lines.len(), "{source:?}");
            assert_eq!(
                BlockNode::List(list),
                parse_with_options(source, &options).children[0]
            );
        }
    }

    #[test]
    fn raw_fence_does_not_loosen_nested_list() {
        let source = "- a\n\n  - b\n\n    ```=latex\n    \\x\n    ```\n";
        let lines: Vec<&str> = source.lines().collect();
        let direct = parse_unified_list(&lines, 0, &Options::default());
        assert!(direct.is_some());
        let (candidate, _) = parse_document(source).unwrap();
        let standard = parse_with_options(source, &Options::default());
        assert_eq!(candidate, standard);
    }

    #[test]
    fn marker_line_nested_owner_matrix_matches_full_ast() {
        let options = Options::default();
        let mut checked = 0;
        for outer in ["-", "1."] {
            for inner in ["-", "1."] {
                let content_indent = outer.len() + inner.len() + 2;
                let sibling = if inner == "1." { "2." } else { "-" };
                for blank in ["", "\n"] {
                    for continuation in ["more", "# heading", "> quote", "```\ncode\n```"] {
                        for has_sibling in [false, true] {
                            let prefix = " ".repeat(content_indent);
                            let continuation = continuation
                                .lines()
                                .map(|line| format!("{prefix}{line}\n"))
                                .collect::<String>();
                            let mut source = format!("{outer} {inner} a\n{blank}{continuation}");
                            if has_sibling {
                                source.push_str(&format!(
                                    "{}{} b\n",
                                    " ".repeat(outer.len() + 1),
                                    sibling
                                ));
                            }
                            let lines: Vec<&str> = source.lines().collect();
                            if let Some((_, consumed)) = parse_unified_list(&lines, 0, &options) {
                                if consumed == lines.len() {
                                    checked += 1;
                                    let expected = parse_with_options(&source, &options);
                                    assert_eq!(expected.children.len(), 1, "{source:?}");
                                    let (candidate, used) = parse_document(&source).unwrap();
                                    assert!(used, "{source:?}");
                                    assert_eq!(candidate, expected, "{source:?}");
                                }
                            }
                        }
                    }
                }
            }
        }
        assert!(checked >= 20, "only {checked} generated cases were owned");
    }
}

fn parse_event_container(cur: &mut LineCursor<'_>, options: &Options<'_>) -> Box<BlockNode> {
    let (mut node, body) = open_container(cur, options);
    if !options.positions && !options.source_lines && !body.in_group && !body.inner.marker_hosted {
        let source = &body.inner.source.inner.lines[body.inner.start..body.inner.end];
        if !source.iter().any(|line| line.text.contains(":::")) {
            let lines: Vec<&str> = source.iter().map(|line| line.text.as_str()).collect();
            let _depth = DepthScope::set(body.depth);
            if let Some(_inner_depth) = DepthGuard::enter() {
                if let Some(children) = parse_blocks(&lines, options)
                    .or_else(|| parse_mixed_blocks::<false, false>(&lines, options, false))
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

fn apply_event_leading_attrs(
    block: &mut BlockNode,
    attrs: Attrs,
    cur: &LineCursor<'_>,
    start_line: usize,
    options: &Options<'_>,
) {
    apply_attrs_to_block(block, attrs);
    resolve_code_title(block);
    promote_delimited_code(block, cur, start_line, options);
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
            parse_mixed_blocks::<false, false>(&parts, options, false)
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
    matches!(
        first,
        b'/' | b'_' | b'~' | b'=' | b'\\' | b'[' | b'!' | b'#'
    )
}

fn may_continue_plain_paragraph(line: &str) -> bool {
    (may_start_plain_paragraph(line) && detect_heading(line).is_none())
        || line.starts_with(": ")
        || line.starts_with('(')
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
    let plain = (if text.len() < 128 {
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
    }) || leading_double_star_literal_inline(text);
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

/// A leading double-star label and later balanced double-star pairs are
/// literal when the rest of the run contains only literal inline bytes.
fn leading_double_star_literal_inline(text: &str) -> bool {
    if !text.starts_with("**") || text.starts_with("***") {
        return false;
    }
    let bytes = text.as_bytes();
    let mut index = 0;
    let mut pairs = 0;
    while let Some(&byte) = bytes.get(index) {
        if byte.is_ascii_alphanumeric()
            || !byte.is_ascii()
            || matches!(
                byte,
                b' ' | b'\n' | b'.' | b',' | b';' | b'!' | b'?' | b'(' | b')'
            )
        {
            index += 1;
            continue;
        }
        if byte == b'*'
            && bytes.get(index + 1) == Some(&b'*')
            && bytes.get(index + 2) != Some(&b'*')
            && (index == 0 || bytes[index - 1] != b'*')
        {
            pairs += 1;
            index += 2;
            continue;
        }
        if byte == b':'
            && index > 0
            && bytes[index - 1] == b'\n'
            && bytes.get(index + 1) == Some(&b' ')
        {
            index += 1;
            continue;
        }
        if byte == b'-' && bytes.get(index + 1) != Some(&b'-') {
            index += 1;
            continue;
        }
        return false;
    }
    pairs > 0
        && pairs % 2 == 0
        && !text.contains("..")
        && !text.contains("(c)")
        && !text.contains("(r)")
        && !text.contains("(tm)")
}

/// The long route accepts only bytes that the current inline dispatch treats
/// as literal. Review this gate when inline constructs or symbol boundaries change.
fn long_literal_inline(text: &str) -> bool {
    if text.len() < 128 {
        return false;
    }
    let bytes = text.as_bytes();
    for (index, &byte) in bytes.iter().enumerate() {
        if byte.is_ascii_alphanumeric() || !byte.is_ascii() {
            continue;
        }
        let literal = match byte {
            b'\n' | b' ' | b'.' | b',' | b';' | b'?' | b'&' | b'(' | b')' => true,
            b'!' => bytes.get(index + 1) != Some(&b'='),
            b'-' => bytes.get(index + 1) != Some(&b'-'),
            b':' | b'#' | b'@' => {
                index > 0 && (bytes[index - 1].is_ascii_alphanumeric() || bytes[index - 1] == b'_')
            }
            b'\\' | b'%' | b'`' | b'$' | b'{' | b'}' | b'[' | b']' | b'<' | b'>' | b'/' | b'*'
            | b'_' | b'~' | b'=' | b'^' | b'+' | b'\'' | b'"' => false,
            _ => false,
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
    use super::{leading_double_star_literal_inline, long_literal_inline, parse_event_inlines};
    use crate::parse::parse_inline_with_options;
    use crate::Options;

    #[test]
    fn leading_double_star_gate_matches_general_scanner() {
        let options = Options::default();
        for first in [
            "**a**",
            "**a b**",
            "**éclair**",
            "**a*b**",
            "**a***",
            "***a***",
        ] {
            for tail in [
                "",
                " and more.",
                " and **other**.",
                "\n: A definition follows.",
                "\nMore text.",
                " (c)",
                " ...",
                " `code`",
                " [link](url)",
                " -- dash",
            ] {
                let text = format!("{first}{tail}");
                if leading_double_star_literal_inline(&text) {
                    assert_eq!(
                        parse_event_inlines(&text, &options),
                        parse_inline_with_options(&text, &options),
                        "{text:?}"
                    );
                }
            }
        }
        assert!(leading_double_star_literal_inline(
            "**Core syntax**\n: The specification defines it."
        ));
        assert!(leading_double_star_literal_inline(
            "**Tier 2** and **Tier 3**."
        ));
        assert!(!leading_double_star_literal_inline("***strong***"));
        assert!(!leading_double_star_literal_inline("**label** `code`"));
        for left in [
            "", "a", " ", ".", "-", "!", "?", "(", ")", ": ", "\n: ", "**b**", "é",
        ] {
            for right in [
                "", "a", " ", ".", "-", "!", "?", "(", ")", ": ", "\n: ", "**b**", "é",
            ] {
                let text = format!("**a**{left}{right}");
                if leading_double_star_literal_inline(&text) {
                    assert_eq!(
                        parse_event_inlines(&text, &options),
                        parse_inline_with_options(&text, &options),
                        "{text:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn long_literal_gate_admits_only_safe_text() {
        assert!(!long_literal_inline(&"a".repeat(127)));
        assert!(long_literal_inline(&"a".repeat(128)));
        for safe in [
            "café, 日本語",
            "carve#529",
            "foo:bar",
            "a@b",
            "a & b",
            "-x",
            "x!",
        ] {
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
            "\t",
            "\r",
            "|",
            "é:x",
        ] {
            assert!(
                !long_literal_inline(&format!("{} {syntax}", "a".repeat(128))),
                "{syntax}"
            );
        }
        for marker in ["#tag", ":symbol:", "@mention"] {
            assert!(!long_literal_inline(&format!(
                "{marker} {}",
                "a".repeat(128)
            )));
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
    if start != 0 {
        return None;
    }
    parse_simple_definition_list_prefix(lines, start, options, true)
}

#[cold]
#[inline(never)]
fn parse_simple_definition_list_prefix(
    lines: &[&str],
    start: usize,
    options: &Options<'_>,
    require_end: bool,
) -> Option<(DefinitionList, usize)> {
    if options.positions || options.source_lines || !options.extensions.is_empty() {
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
    if entries.is_empty()
        || (require_end && lines[pos..].iter().any(|line| !is_blank_line(line)))
        || (!require_end && lines.get(pos).is_some_and(|line| !is_blank_line(line)))
        || (!require_end
            && lines[pos..]
                .iter()
                .find(|line| !is_blank_line(line))
                .is_some_and(|line| line.starts_with([' ', '\t', ':'])))
    {
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
    let (kind, ordinal, content) = strict_list_marker(line)?;
    simple_list_content(content).then_some((kind, ordinal, content))
}

fn simple_list_content(content: &str) -> bool {
    !(content.starts_with('[')
        || is_list_marker(content)
        || detect_heading(content).is_some()
        || detect_fence_open(content).is_some()
        || thematic_break_marker(content).is_some()
        || !may_start_plain_paragraph(content))
}

fn strict_list_marker(line: &str) -> Option<(SimpleListKind, usize, &str)> {
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
    if content.is_empty() {
        return None;
    }
    Some((kind, ordinal, content))
}

fn unified_list_marker(line: &str) -> Option<(SimpleListKind, usize, &str)> {
    let marker = strict_list_marker(line)?;
    if simple_list_content(marker.2) {
        return Some(marker);
    }
    let mut content = marker.2;
    let mut nested = false;
    while let Some((kind, ordinal, inner)) = strict_list_marker(content) {
        if kind == SimpleListKind::Ordered && ordinal != 1 {
            return None;
        }
        nested = true;
        content = inner;
    }
    (nested && simple_list_content(content)).then_some(marker)
}

fn list_continuation_decision(
    line_map: Option<&[Option<usize>]>,
    pos: usize,
) -> Option<continuation_marker::Decision> {
    line_map
        .and_then(|map| map.get(pos).copied().flatten())
        .and_then(continuation_marker::decision)
}

fn plus_first_block_marker(
    line: &str,
    decision: Option<continuation_marker::Decision>,
) -> Option<(SimpleListKind, usize, &str)> {
    let decision = decision?;
    if decision.owner != Some(continuation_marker::Kind::List)
        || decision.refusal.is_some()
        || !decision.first_block
    {
        return None;
    }
    let first = strict_list_marker(line)?;
    let mut content = first.2;
    while let Some((kind, ordinal, inner)) = strict_list_marker(content) {
        if kind == SimpleListKind::Ordered && ordinal != 1 {
            return None;
        }
        content = inner;
    }
    is_plus_marker(content).then_some(first)
}

fn owned_list_first_block(decision: Option<continuation_marker::Decision>) -> bool {
    decision.is_some_and(|decision| {
        decision.owner == Some(continuation_marker::Kind::List)
            && decision.refusal.is_none()
            && decision.first_block
    })
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
    let root_line_map: Option<Vec<Option<usize>>> = continuation_marker::active().then(|| {
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
                            true,
                        )
                    } else {
                        parse_mixed_blocks_mapped::<true, false>(
                            &lines,
                            &options,
                            root_line_map.as_deref(),
                            true,
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
                            true,
                        )
                    } else {
                        parse_mixed_blocks_mapped::<false, false>(
                            &lines,
                            &options,
                            root_line_map.as_deref(),
                            true,
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
    let crossref_index = if source.contains("</#") {
        let (index, ids) =
            crossref_index_and_heading_ids_for_document(&doc, options.heading_id_options());
        stamp_generated_heading_ids_from(&mut doc, ids);
        Some(index)
    } else {
        stamp_generated_heading_ids(&mut doc, options.heading_id_options());
        None
    };
    let _ = crate::render::collect_footnotes(&mut doc, false);
    if source.contains("![") {
        promote_block_images(&mut doc.children, false, true);
    }
    coalesce_text_runs(&mut doc);
    if let Some(index) = crossref_index {
        fill_crossref_hrefs_from_index(&mut doc, index);
    }
    Some((doc, used_event_route))
}

fn parse_document_with_footnotes(source: &str, source_len: usize) -> Option<(Document, bool)> {
    let may_have_abbrevs = source.contains("*[");
    if needs_event_normalization(source) || has_fence_candidate_without_final_newline(source) {
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
    let (mapped, mut footnote_sources, mut footnote_def_pos, mut note_links) =
        extract_footnote_defs(body, body_start_line, false, false, None, &options);
    if footnote_sources
        .values()
        .any(|note| note.source.contains("[^"))
    {
        extract_nested_footnote_defs(
            &mut footnote_sources,
            &mut footnote_def_pos,
            &mut note_links,
            &options,
        );
    }
    let mut budget = probe_budget_for(mapped.source.len());
    let (body_source, mut link_defs) =
        extract_link_defs_guarded(&mapped.source, &options, &mut budget);
    for def in link_defs.values_mut() {
        if let Some(line) = def.line {
            def.line = mapped
                .line_map
                .get(line)
                .and_then(|mapped| *mapped)
                .map(|authored| authored - 1)
                .or(def.line);
        }
    }
    for (label, def) in note_links {
        link_defs.entry(label).or_insert(def);
    }
    let mapped = remap_source(body_source, &mapped);
    let (parsed, link_defs, needs_late_resolution) = with_active_link_defs(link_defs, || {
        let _depth = DepthGuard::enter()?;
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
        let lines: Vec<&str> = mapped.source.lines().collect();
        let children = parse_blocks(&lines, &options).or_else(|| {
            if may_have_abbrevs {
                parse_mixed_blocks::<true, true>(&lines, &options, true)
            } else {
                parse_mixed_blocks::<true, false>(&lines, &options, true)
            }
        })?;
        Some((children, footnote_defs))
    });
    let (children, footnote_defs) = parsed?;
    let mut doc = Document {
        frontmatter,
        frontmatter_raw,
        footnote_defs,
        footnote_def_pos,
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

#[cfg(feature = "experimental-event-parser-options")]
#[cold]
#[inline(never)]
pub(super) fn parse_document_with_options(
    source: &str,
    options: &Options<'_>,
) -> Option<(Document, bool)> {
    if !options.extensions.is_empty() {
        return None;
    }
    parse_document_with_custom_options(source, options)
}

#[cfg(feature = "experimental-event-parser-options")]
fn parse_document_with_custom_options(
    source: &str,
    options: &Options<'_>,
) -> Option<(Document, bool)> {
    let original_source = source;
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
        return parse_document_with_footnotes_options(source, original_source, source_len, options);
    }
    if has_fence_candidate_without_final_newline(source) {
        return None;
    }
    let _placeholders = PlaceholderGuard::install(pick_definition_placeholders(source));
    let _continuation_markers = continuation_marker::Guard::install(source);
    let _source_ending = if !options.positions && source.ends_with('\n') {
        SourceEndingGuard::fragment()
    } else {
        SourceEndingGuard::install(source)
    };
    let (frontmatter, frontmatter_raw, body) = split_frontmatter(source, options.positions);
    let (stripped_body, mut link_defs) = if may_have_link_defs {
        let mut budget = probe_budget_for(body.len());
        extract_link_defs_guarded(body, options, &mut budget)
    } else {
        (String::new(), BTreeMap::new())
    };
    let has_link_defs = !link_defs.is_empty();
    if options.positions && may_have_abbrevs {
        return None;
    }
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
    if options.positions && has_link_defs {
        for def in link_defs.values_mut() {
            def.line = def.line.map(|line| line + body_start_line - 1);
        }
    }
    let root_line_map: Option<Vec<Option<usize>>> =
        (continuation_marker::active() || options.source_lines || options.positions).then(|| {
            (body_start_line..body_start_line + body_source.lines().count())
                .map(Some)
                .collect()
        });
    let (direct, link_defs, needs_late_resolution) = {
        let _depth = DepthGuard::enter()?;
        let lines: Vec<&str> = body_source.lines().collect();
        if options.positions {
            with_active_link_defs(link_defs, || {
                if has_link_defs {
                    parse_mixed_blocks_mapped::<true, false>(
                        &lines,
                        options,
                        root_line_map.as_deref(),
                        true,
                    )
                } else {
                    parse_positioned_simple_blocks(&lines, options, root_line_map.as_deref()?)
                        .or_else(|| {
                            parse_mixed_blocks_mapped::<false, false>(
                                &lines,
                                options,
                                root_line_map.as_deref(),
                                true,
                            )
                        })
                }
            })
        } else if has_link_defs {
            with_active_link_defs(link_defs, || {
                parse_blocks_mapped(&lines, options, root_line_map.as_deref()).or_else(|| {
                    if may_have_abbrevs {
                        parse_mixed_blocks_mapped::<true, true>(
                            &lines,
                            options,
                            root_line_map.as_deref(),
                            true,
                        )
                    } else {
                        parse_mixed_blocks_mapped::<true, false>(
                            &lines,
                            options,
                            root_line_map.as_deref(),
                            true,
                        )
                    }
                })
            })
        } else {
            (
                parse_blocks_mapped(&lines, options, root_line_map.as_deref()).or_else(|| {
                    if lines
                        .first()
                        .is_some_and(|line| is_definition_list_start(line))
                    {
                        if let Some((list, _)) = parse_simple_definition_list(&lines, 0, options) {
                            return Some(vec![BlockNode::DefinitionList(list)]);
                        }
                    }
                    if may_have_abbrevs {
                        parse_mixed_blocks_mapped::<false, true>(
                            &lines,
                            options,
                            root_line_map.as_deref(),
                            true,
                        )
                    } else {
                        parse_mixed_blocks_mapped::<false, false>(
                            &lines,
                            options,
                            root_line_map.as_deref(),
                            true,
                        )
                    }
                }),
                link_defs,
                false,
            )
        }
    };
    let used_event_route = direct.is_some();
    let mut children = match direct {
        Some(children) => children,
        None if has_link_defs || options.positions => return None,
        None => {
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
            parse_mapped_source_at_document_level(&mapped, options)
        }
    };
    if options.positions {
        let line_starts = CodepointLineStarts::original(original_source);
        fill_offsets(&mut children, &line_starts);
        narrow_to_last_placed_child(&mut children, &original_source.lines().collect::<Vec<_>>());
    }
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
    let original_line_starts = options
        .positions
        .then(|| CodepointLineStarts::original(original_source));
    append_link_reference_definitions(&mut doc, &link_defs, source, original_line_starts.as_ref());
    if may_have_abbrevs {
        apply_abbreviations(&mut doc);
    }
    if source.contains('^') {
        number_crossref_captions(&mut doc);
    }
    let crossref_index = if source.contains("</#") {
        let (index, ids) =
            crossref_index_and_heading_ids_for_document(&doc, options.heading_id_options());
        stamp_generated_heading_ids_from(&mut doc, ids);
        Some(index)
    } else {
        stamp_generated_heading_ids(&mut doc, options.heading_id_options());
        None
    };
    let _ = crate::render::collect_footnotes(&mut doc, false);
    if source.contains("![") {
        promote_block_images(&mut doc.children, false, true);
    }
    coalesce_text_runs(&mut doc);
    if let Some(index) = crossref_index {
        fill_crossref_hrefs_from_index(&mut doc, index);
    }
    Some((doc, used_event_route))
}

#[cfg(feature = "experimental-event-parser-options")]
fn parse_document_with_footnotes_options(
    source: &str,
    original_source: &str,
    source_len: usize,
    options: &Options<'_>,
) -> Option<(Document, bool)> {
    let may_have_abbrevs = source.contains("*[");
    if needs_event_normalization(source) || has_fence_candidate_without_final_newline(source) {
        return None;
    }
    let _placeholders = PlaceholderGuard::install(pick_definition_placeholders(source));
    let _continuation_markers = continuation_marker::Guard::install(source);
    let _source_ending = if !options.positions && source.ends_with('\n') {
        SourceEndingGuard::fragment()
    } else {
        SourceEndingGuard::install(source)
    };
    let (frontmatter, frontmatter_raw, body) = split_frontmatter(source, options.positions);
    let body_start_line = source[..body.as_ptr() as usize - source.as_ptr() as usize]
        .bytes()
        .filter(|byte| *byte == b'\n')
        .count()
        + 1;
    let (mapped, mut footnote_sources, mut footnote_def_pos, mut note_links) =
        extract_footnote_defs(
            body,
            body_start_line,
            options.positions,
            false,
            None,
            options,
        );
    if footnote_sources
        .values()
        .any(|note| note.source.contains("[^"))
    {
        extract_nested_footnote_defs(
            &mut footnote_sources,
            &mut footnote_def_pos,
            &mut note_links,
            options,
        );
    }
    if ((options.positions || options.source_lines) && may_have_abbrevs)
        || (options.positions && mapped.col_map.iter().any(|column| *column != Some(0)))
    {
        return None;
    }
    let mut budget = probe_budget_for(mapped.source.len());
    let (body_source, mut link_defs) =
        extract_link_defs_guarded(&mapped.source, options, &mut budget);
    for def in link_defs.values_mut() {
        if let Some(line) = def.line {
            def.line = mapped
                .line_map
                .get(line)
                .and_then(|mapped| *mapped)
                .map(|authored| authored - 1)
                .or(def.line);
        }
    }
    for (label, def) in note_links {
        link_defs.entry(label).or_insert(def);
    }
    let mapped = remap_source(body_source, &mapped);
    let (parsed, link_defs, needs_late_resolution) = with_active_link_defs(link_defs, || {
        let _depth = DepthGuard::enter()?;
        let lines: Vec<&str> = mapped.source.lines().collect();
        let children = if options.positions || options.source_lines {
            parse_mixed_blocks_mapped::<true, false>(&lines, options, Some(&mapped.line_map), true)?
        } else {
            parse_blocks(&lines, options).or_else(|| {
                if may_have_abbrevs {
                    parse_mixed_blocks::<true, true>(&lines, options, true)
                } else {
                    parse_mixed_blocks::<true, false>(&lines, options, true)
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
                    if options.positions || options.source_lines {
                        parse_mapped_source(&note, options)
                    } else {
                        parse_blocks(&lines, options)
                            .unwrap_or_else(|| parse_mapped_source(&note, options))
                    }
                };
                (label, children)
            })
            .collect();
        Some((children, footnote_defs))
    });
    let (children, footnote_defs) = parsed?;
    let mut doc = Document {
        frontmatter,
        frontmatter_raw,
        footnote_defs,
        footnote_def_pos,
        children,
        source_len,
        ingest_payload_len: 0,
    };
    if options.positions {
        let starts = CodepointLineStarts::original(original_source);
        fill_offsets(&mut doc.children, &starts);
        let source_lines: Vec<&str> = original_source.lines().collect();
        narrow_to_last_placed_child(&mut doc.children, &source_lines);
        for blocks in doc.footnote_defs.values_mut() {
            fill_offsets(blocks, &starts);
            narrow_to_last_placed_child(blocks, &source_lines);
        }
        for pos in doc.footnote_def_pos.values_mut() {
            apply_offsets(pos, &starts);
        }
        for (label, pos) in &mut doc.footnote_def_pos {
            let last = doc
                .footnote_defs
                .get(label)
                .and_then(|blocks| blocks.iter().rev().find_map(crate::ast_json::block_pos))
                .cloned();
            end_at_last_placed_child(pos, last);
        }
    }
    if needs_late_resolution || source.contains("][") {
        let headings = heading_reference_index_for_document(&doc, options.heading_id_options());
        resolve_reference_links(&mut doc, &link_defs, &headings);
    }
    let original_line_starts = options
        .positions
        .then(|| CodepointLineStarts::original(original_source));
    append_link_reference_definitions(&mut doc, &link_defs, source, original_line_starts.as_ref());
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
        assert!(admitted);
        let expected = parse_with_options(source, &Options::default());
        assert_eq!(
            crate::ast_json::to_json(&actual),
            crate::ast_json::to_json(&expected)
        );
    }
}
