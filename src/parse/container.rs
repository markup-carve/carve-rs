use super::*;

pub(super) struct ContainerOpen {
    pub(super) fence_len: usize,
    pub(super) kind: Option<String>,
    pub(super) title: Option<String>,
    /// Codepoint offset of the title's first character within the opener line,
    /// when the title is a VERBATIM slice of it. A quoted title carrying an
    /// escape is rebuilt rather than sliced, so no column in it maps back and
    /// this stays `None` (PART 12 section 4).
    pub(super) title_col: Option<usize>,
    pub(super) label: Option<String>,
    pub(super) attrs: Option<Attrs>,
    pub(super) invalid_metadata: bool,
}

/// A container opener's label as every consumer of the node sees it: without the
/// trailing comment `CARVE-P9-041` consumes.
///
/// Cut HERE rather than at each consumer. Six render sites read the rule after
/// carve-rs#2158, and four more publishers did not - the AST serializer, and the
/// Markdown, plain-text and ANSI targets - so `--json` announced a label the HTML
/// in the same build had dropped, and the three targets printed the comment
/// (carve-rs#2163). A list of consumers is the wrong place for a rule about the
/// value: the node now carries the cut label and a consumer added later cannot
/// leak what the author hid.
///
/// carve-js and carve-php both cut at parse time, and neither reproduces the
/// comment when writing the label back either. That is the convention this
/// matches. Carrying the comment as a node instead needs the label to BE an
/// inline run, which is open as markup-carve/carve#2572.
pub(super) fn container_open_label(label: Option<String>, options: &Options<'_>) -> Option<String> {
    label.map(|label| label_without_trailing_comment(&label, options).to_string())
}

// Counts the opener readings and the characters container RECOVERY examines.
//
// A container with invalid opener metadata keeps its container and parses its
// children (markup-carve/carve#2693), so such an opener sits on the NESTING
// path, where the body walk re-reads it once per enclosing level. carve-php
// honoured the same ruling and its recovery branch then answered a bounded
// column question by walking a whole indentation run, which cost 4,666,600
// gate characters at depth 200 against 80,394 before (carve-php#2817, capped in
// #2819). Nothing in that repo's test suite or corpus moved; only a counted
// guard saw it. These counters are what `container_open_readings` reads.
// Test-only, so a release build carries none of it.
#[cfg(test)]
thread_local! {
    pub(crate) static CONTAINER_OPEN_READINGS: std::cell::Cell<u64> =
        const { std::cell::Cell::new(0) };
    pub(crate) static CONTAINER_RECOVERY_CHARS: std::cell::Cell<u64> =
        const { std::cell::Cell::new(0) };
    pub(crate) static CONTAINER_RECOVERY_ACCEPTS: std::cell::Cell<u64> =
        const { std::cell::Cell::new(0) };
}

#[cfg(test)]
fn charge_recovery(chars: usize) {
    CONTAINER_RECOVERY_CHARS.with(|c| c.set(c.get() + chars as u64));
}

pub(super) fn detect_container_open(line: &str) -> Option<ContainerOpen> {
    #[cfg(test)]
    CONTAINER_OPEN_READINGS.with(|c| c.set(c.get() + 1));
    if let Some(open) = detect_valid_container_open(line) {
        return Some(open);
    }
    if line.contains(['\r', '\n']) {
        return None;
    }
    let line = trim_ascii(line);
    let fence_len = line.bytes().take_while(|&b| b == b':').count();
    #[cfg(test)]
    charge_recovery(fence_len + usize::from(fence_len < line.len()));
    if fence_len < 3 || !line[fence_len..].starts_with(' ') {
        return None;
    }
    let after_fence = &line[fence_len..];
    let rest = after_fence.trim_start_matches(' ');
    #[cfg(test)]
    charge_recovery(after_fence.len() - rest.len());
    if !rest.starts_with(|c: char| c.is_ascii_alphanumeric() || c == '_') {
        return None;
    }
    let end = rest
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '-'))
        .unwrap_or(rest.len());
    #[cfg(test)]
    charge_recovery(end + usize::from(end < rest.len()));
    if rest[end..].starts_with(|c: char| {
        !c.is_whitespace() && c != '\u{feff}' && !matches!(c, '"' | '{' | '[' | '“' | '”')
    }) {
        return None;
    }
    #[cfg(test)]
    CONTAINER_RECOVERY_ACCEPTS.with(|c| c.set(c.get() + 1));
    Some(ContainerOpen {
        fence_len,
        kind: Some(rest[..end].to_string()),
        title: None,
        title_col: None,
        label: None,
        attrs: None,
        invalid_metadata: true,
    })
}

pub(crate) fn lint_container_fence_width(line: &str) -> Option<usize> {
    detect_container_open(line)
        .map(|open| open.fence_len)
        .or_else(|| detect_line_block_open(line))
        .or_else(|| detect_hardbreaks_block_open(line))
        .or_else(|| detect_quote_block_open(line))
}

pub(crate) fn lint_invalid_container_metadata(line: &str) -> bool {
    detect_container_open(line).is_some_and(|open| open.invalid_metadata)
}

fn detect_valid_container_open(line: &str) -> Option<ContainerOpen> {
    let trimmed = trim_ascii(line);
    let fence_len = trimmed.bytes().take_while(|b| *b == b':').count();
    if fence_len < 3 {
        return None;
    }
    let after_fence = &trimmed[fence_len..];
    let sep_len = after_fence.bytes().take_while(|b| *b == b' ').count();
    let rest = &after_fence[sep_len..];
    // STRICT (djot): the opener is the colon fence, an optional type word,
    // and an optional quoted title -- and NOTHING else. A trailing `{...}`
    // (or any other non-title text) makes the line an ordinary paragraph,
    // not a fence; attributes attach via a preceding block-attribute line.
    if rest.is_empty() {
        return Some(ContainerOpen {
            fence_len,
            kind: None,
            title: None,
            title_col: None,
            label: None,
            attrs: None,
            invalid_metadata: false,
        });
    }
    // `div_open = colon_fence, [[space], label]` - the label is either GLUED to
    // the fence or separated from it by spaces, and nothing else. This branch
    // runs before the separator check below, so it used to match against a
    // Unicode-trimmed copy and open a labelled div for a tabbed opener; #720
    // guarded it with its own copy of the separator test instead. With `rest`
    // now starting where the space run ends, `parse_bare_label`'s own
    // `starts_with('[')` decides it: a tab or any other character between the
    // fence and the bracket is simply still there, so the label does not parse.
    // The extra guard would be a check that can no longer fail (#722).
    if let Some(label) = parse_bare_label(rest) {
        return Some(ContainerOpen {
            fence_len,
            kind: None,
            title: None,
            title_col: None,
            label: Some(label),
            attrs: None,
            invalid_metadata: false,
        });
    }
    // THE SEPARATOR IS A SPACE, U+0020 and nothing else. PART 7's MARKER
    // SEPARATORS AND PADDING SLOTS is normative: this slot stands between the
    // fence run and the token that SELECTS which of the four blocks the line
    // opens, which makes it a marker separator rather than padding. A tab was
    // accepted here, so a tabbed opener opened an admonition, a div, a line
    // block or a local hard-break block where the grammar makes the line a
    // paragraph (#712, spec carve#886).
    //
    // A typeless label may still be glued to the fence (`:::[x]`), and
    // `:::note` is ordinary paragraph text. The opener's METADATA slots are
    // the other role, and PART 7 on carve main spells those `space` too - see
    // `after_kind` below.
    if sep_len == 0 {
        return None;
    }
    // A type word is a grammar identifier: `(letter | '_'), {letter | digit
    // | '_' | '-'}`. It must START with a letter or underscore, so a
    // digit-first token (`123`) or a non-identifier opener (`::: {.x}`,
    // `:::{k=v}`) is not a fence -- the line is an ordinary paragraph. A
    // leading ASCII digit is accepted because the bare kind is a class value.
    if !rest.starts_with(|c: char| c.is_ascii_alphanumeric() || c == '_') {
        return None;
    }
    let id_end = rest
        .char_indices()
        .find(|(_, c)| !(c.is_ascii_alphanumeric() || *c == '-' || *c == '_'))
        .map(|(i, _)| i)
        .unwrap_or(rest.len());
    let kind = rest[..id_end].to_string();
    let after_kind = &rest[id_end..];
    // THE TITLE SLOT IS SPACES. PART 7 decides this terminal by POSITION, not
    // by the slot's role: a tab is syntax only inside a line's leading
    // indentation run, and from the first non-whitespace character onward it is
    // not syntax at all. Every metadata slot on this line sits after the fence,
    // so `admonition_open = colon_fence, space, admonition_type,
    // [space+, quoted_title], [space+, label]` spells all of them `space`.
    //
    // #720 read carve#886, which said a padding slot took `whitespace`, and
    // narrowed this from `char::is_whitespace` to `[' ', '\t']`. carve#901
    // landed as carve#905 and reverted that clause; the Unicode half of the
    // narrowing was right and is kept by going to `' '`, which admits neither
    // a tab nor U+00A0 nor a form feed (#722, corpus 255 and 255-2).
    if !after_kind.is_empty() && !after_kind.starts_with(' ') {
        return None;
    }
    let mut after = after_kind.trim_start_matches(' ');
    let mut title_col = None;
    let title = if after.starts_with('"') {
        // Where the text after the quote sits in the ORIGINAL line. These are
        // all subslices of `line`, so the byte offset is a pointer difference;
        // columns are codepoints (PART 12 section 4).
        let quote_at = (after.as_ptr() as usize) - (line.as_ptr() as usize);
        let text_at = quote_at + 1;
        let (title, remainder) = parse_quoted_metadata(after)?;
        // The title is a subslice of this line, so its columns always map.
        title_col = Some(line[..text_at].chars().count());
        // THE LABEL SLOT IS SPACES, for the same reason as the title slot
        // above. This one was missed entirely by #720, which narrowed only the
        // slot before the title: `trim_start` is `char::is_whitespace`, so this
        // admitted a tab AND every Unicode space. `::: note "T"` followed by
        // U+00A0 or U+2003 and a `[label]` opened an admonition here (#722,
        // corpus 255-3 and 255-4).
        after = remainder.trim_start_matches(' ');
        Some(title)
    } else {
        None
    };
    let label = if after.starts_with('[') {
        // Balanced, escape-aware and opaque inside a verbatim span, for the
        // reason `parse_bare_label` gives: this is the same `label` production,
        // and the two spellings of it have to close in the same place.
        let (label, end) = read_bracketed(after.as_bytes(), 0)?;
        if !trim_ascii(&after[end..]).is_empty() {
            return None;
        }
        Some(label)
    } else {
        if !after.is_empty() {
            return None;
        }
        None
    };
    Some(ContainerOpen {
        fence_len,
        kind: Some(kind),
        title,
        title_col,
        label,
        attrs: None,
        invalid_metadata: false,
    })
}

/// The glued `:::[label]` form's label, closed where the READER's own bracket
/// scan closes it rather than at the first `]`.
///
/// `label` takes a BALANCED run (markup-carve/carve#2576), so a nested bracket,
/// an escaped `]` and a `]` inside a verbatim span all belong to the label - and
/// an UNCLOSED backtick run swallows the closer and leaves the line as prose,
/// which is what a link's text does with the same bytes. A first-`]` scan is a
/// second spelling of that production and disagreed with it on all four.
fn parse_bare_label(s: &str) -> Option<String> {
    let (label, after) = read_bracketed(s.as_bytes(), 0)?;
    if !s[after..].trim().is_empty() {
        return None;
    }
    Some(label)
}

/// The slot is the source VERBATIM and the first `"` closes it, per
/// `quoted_title = '"', {character - '"'}, '"'` and its normative note that
/// there is no escape mechanism inside a quoted title. So a backslash is a
/// literal backslash and a `"` has no spelling in the slot at all: a
/// backslash-skipping scan read an escaped quote as content and opened a
/// container on a line the grammar leaves a paragraph (carve-rs#1946).
fn parse_quoted_metadata(s: &str) -> Option<(String, &str)> {
    if !s.starts_with('"') {
        return None;
    }
    let close = 1 + s[1..].find('"')?;
    Some((s[1..close].to_string(), &s[close + 1..]))
}

#[inline(never)]
fn boxed_figure_group(
    children: Vec<BlockNode>,
    caption: Option<Vec<InlineNode>>,
    pos: Option<Pos>,
) -> Box<BlockNode> {
    Box::new(BlockNode::FigureGroup(FigureGroup {
        attrs: None,
        children,
        caption,
        pos,
    }))
}

#[inline(never)]
fn boxed_container_node(
    open: ContainerOpen,
    children: Vec<BlockNode>,
    pos: Option<Pos>,
    title_anchor: Option<(usize, isize)>,
    options: &Options<'_>,
) -> Box<BlockNode> {
    if let Some(kind) = open.kind {
        if crate::ast::is_generated_content_kind(&kind) {
            return Box::new(BlockNode::Directive(Directive {
                attrs: open.attrs,
                kind,
                title: open.title.map(|title| {
                    parse_inline_lines_with_anchor(&title, options, vec![title_anchor])
                }),
                label: container_open_label(open.label, options),
                children,
                pos,
            }));
        }
        Box::new(BlockNode::Admonition(Admonition {
            attrs: open.attrs,
            kind,
            title: open
                .title
                .map(|title| parse_inline_lines_with_anchor(&title, options, vec![title_anchor])),
            label: container_open_label(open.label, options),
            children,
            pos,
        }))
    } else {
        Box::new(BlockNode::Div(Div {
            attrs: open.attrs,
            label: container_open_label(open.label, options),
            children,
            pos,
        }))
    }
}

/// A container for the SINGLE-block parser, which returns one node and so has
/// nowhere to record a pending body.
///
/// It still goes through the worklist rather than around it: only this one
/// opener costs a frame, and the whole nest under it is flat.
pub(super) fn parse_container(cur: &mut LineCursor, options: &Options<'_>) -> Box<BlockNode> {
    let (node, body) = open_container(cur, options);
    let mut out = vec![*node];
    resolve_pending_bodies(
        &mut out,
        vec![PendingBody {
            index: 0,
            body: Body::Colon(body),
        }],
        options,
    );
    Box::new(out.pop().expect("the node the worklist just filled in"))
}

/// Open a colon container and COLLECT its body, without parsing it.
///
/// Splitting the two halves is the point. The opener, the extent of the body
/// and - for a composite figure - the caption hanging on the closing fence are
/// all decided from the ENCLOSING cursor, and none of them needs the body's
/// blocks. So the node can be emitted here, hollow, and filled in by the
/// worklist; the recursive form parsed the body BETWEEN these two halves and
/// paid a stack frame per nesting level for it
/// (markup-carve/carve-rs#1165). `collect_shared_colon_body` is purely
/// lexical, so nothing here reads the state the body will parse under - what
/// that state is gets recorded in the [`ContainerBody`] instead.
#[inline(never)]
pub(super) fn open_container(
    cur: &mut LineCursor,
    options: &Options<'_>,
) -> (Box<BlockNode>, ContainerBody) {
    let open = Box::new(detect_container_open(cur.peek().unwrap()).unwrap());
    let span_start = cur.pos;
    cur.consume();
    let depth = NESTING_DEPTH.with(Cell::get);
    // PART 9 §4c: a BARE `::: figure` opener opens a COMPOSITE FIGURE, unless
    // it sits inside an open group's body (groups do not nest - the inner one
    // stays the generic container the Admonition arm below builds). The body
    // and closer follow the unchanged container rules; what §4c adds is the
    // caption slot hanging on the CLOSING fence, this kind only.
    if is_bare_figure_open(&open) && !IN_FIGURE_GROUP.with(Cell::get) {
        let (inner, closed) = collect_shared_colon_body(cur, open.fence_len);
        if span_start == 0 && closed {
            cur.closed_initial_container_end = Some(cur.pos);
        }
        // The slot hangs on the closer. A group left open at end of input
        // closed there without one, so there is no line for a caption to
        // attach to (§4c).
        let caption = if closed {
            consume_caption(cur, options)
        } else {
            None
        };
        // Through the caption the cursor just consumed, like a figure's span.
        let pos = container_span(cur, span_start, closed, options);
        return (
            boxed_figure_group(Vec::new(), caption, pos),
            ContainerBody {
                inner,
                depth,
                // The body IS the group's body, which is what the recursive
                // form said by parsing it inside a `FigureGroupGuard`.
                in_group: true,
            },
        );
    }
    let (inner, closed) = collect_shared_colon_body(cur, open.fence_len);
    if span_start == 0 && closed {
        cur.closed_initial_container_end = Some(cur.pos);
    }
    // The span covers the opening fence through the closing one.
    let pos = container_span(cur, span_start, closed, options);
    let title_anchor = open.title_col.and_then(|col| {
        Some((
            cur.source_line(span_start)?,
            cur.source_col(span_start)? + col as isize,
        ))
    });
    let in_group = IN_FIGURE_GROUP.with(Cell::get);
    (
        boxed_container_node(*open, Vec::new(), pos, title_anchor, options),
        ContainerBody {
            inner,
            depth,
            in_group,
        },
    )
}

fn container_span(
    cur: &LineCursor<'_>,
    start: usize,
    closed: bool,
    options: &Options<'_>,
) -> Option<Pos> {
    let mut end = cur.pos;
    if options.positions && !closed {
        while end > start + 1 && is_blank_line(cur.lines.at(end - 1)) {
            end -= 1;
        }
    }
    // A verbatim child may own these blanks. Filling the children extends
    // this span over that payload after the body has been parsed.
    span_of(cur, start, end, options)
}

/// `quote_block_open = colon_fence, space, ">"` -- a colon fence whose type
/// token is a bare `>`. A SPACE before the token is REQUIRED, exactly as the
/// line block and the hard-break block require one, so `:::>` is not an opener
/// (markup-carve/carve#1718).
pub(super) fn detect_quote_block_open(line: &str) -> Option<usize> {
    let trimmed = trim_ascii(line);
    let fence_len = trimmed.bytes().take_while(|b| *b == b':').count();
    if fence_len < 3 {
        return None;
    }
    let after = &trimmed[fence_len..];
    let trimmed_after = after.trim_start_matches(' ');
    if trimmed_after.len() == after.len() {
        return None; // no space before the token
    }
    if trimmed_after == ">" {
        Some(fence_len)
    } else {
        None
    }
}

/// A `::: \` local hard-break block opener: a colon fence (3+) then a bare
/// backslash and nothing else (grammar PART 9 §23). Returns the fence length.
/// Deliberately smaller than a line block: it converts soft breaks in DIRECT
/// paragraph children to hard breaks, but does NOT preserve leading whitespace,
/// keeps the stanza/block structure of its body, and does not affect nested
/// blocks. Mirrors carve-js `RE_HARDBREAKS_BLOCK_OPEN` / `parseHardBreaksBlock`.
pub(super) fn detect_hardbreaks_block_open(line: &str) -> Option<usize> {
    let trimmed = trim_ascii(line);
    let fence_len = trimmed.bytes().take_while(|b| *b == b':').count();
    if fence_len < 3 {
        return None;
    }
    // `hardbreaks_block_open = colon_fence, space, "\"` -- a SPACE between the
    // fence and the backslash is REQUIRED, so a glued opener is not one and
    // neither is a tabbed one. The production always said `space`; this read it
    // as "space or tab", which PART 7's MARKER SEPARATORS AND PADDING SLOTS now
    // rules out explicitly - the backslash is the token that selects this
    // block, so the slot before it is a marker separator (#712, spec
    // carve#886).
    let after = &trimmed[fence_len..];
    let trimmed_after = after.trim_start_matches(' ');
    if trimmed_after.len() == after.len() {
        return None; // no space before the backslash
    }
    // Same as the line block above: `trim_ascii` has already taken the trailing
    // space-or-tab run, so a `trim_end` could only strip content (PART 7,
    // carve#977).
    if trimmed_after == "\\" {
        Some(fence_len)
    } else {
        None
    }
}

/// Parse a `::: \` local hard-break block into a `<div class="hardbreaks">`:
/// the body is parsed as ordinary blocks, then every soft break in a DIRECT
/// paragraph child becomes a hard break. Unlike a line block, leading
/// whitespace is not preserved and nested blocks keep ordinary soft breaks.
/// Parse a `::: >` fenced block quote. The body is ordinary block content and
/// the node is the one the `>`-prefixed form produces; `fenced` records which
/// spelling was authored so the writer can write it back
/// (markup-carve/carve#1718).
pub(super) fn parse_quote_block(cur: &mut LineCursor, options: &Options<'_>) -> BlockNode {
    let opener = cur.peek().unwrap();
    let fence_len = detect_quote_block_open(opener).unwrap();
    let span_start = cur.pos;
    cur.consume();
    let (inner, _closed) = collect_shared_colon_body(cur, fence_len);
    let children = drained(options, |pending| parse_colon_view(inner, options, pending));
    // The quote's own span ends at its CLOSING fence, before the caption slot
    // that hangs on it, so it is taken before `consume_caption` moves the
    // cursor past a caption line.
    let quote_pos = span_of(cur, span_start, cur.pos, options);
    let caption = consume_caption(cur, options);
    // Section 4's seventh host. What it produces is what the PREFIXED spelling
    // produces: a captioned quote is a figure either way, because the two
    // spellings are one node and the caption rule reads the node (carve#1742).
    *boxed_blockquote_node(
        children,
        quote_pos,
        caption,
        span_of(cur, span_start, cur.pos, options),
        true,
    )
}

pub(super) fn parse_hardbreaks_block(cur: &mut LineCursor, options: &Options<'_>) -> BlockNode {
    let opener = cur.peek().unwrap();
    let fence_len = detect_hardbreaks_block_open(opener).unwrap();
    let span_start = cur.pos;
    cur.consume();
    let (inner, _closed) = collect_shared_colon_body(cur, fence_len);
    // The span covers the opening fence through the closing one, like any other
    // colon fence.
    let pos = span_of(cur, span_start, cur.pos, options);
    let mut children = drained(options, |pending| parse_colon_view(inner, options, pending));
    for child in &mut children {
        if let BlockNode::Paragraph(para) = child {
            for node in &mut para.children {
                if let InlineNode::SoftBreak(brk) = node {
                    // Carry the break's span across. Building a fresh
                    // `hard_break()` here threw it away, and the loss was
                    // invisible: the two render identically in this block, so
                    // only the tree showed it.
                    *node = InlineNode::HardBreak(Break {
                        pos: brk.pos.clone(),
                    });
                }
            }
        }
    }
    // No inline opener attributes (strict djot); a preceding block-attribute
    // line merges onto this div in parse_blocks.
    BlockNode::Div(Div {
        attrs: Some(Attrs {
            id: None,
            classes: vec!["hardbreaks".to_string()],
            key_values: BTreeMap::new(),
            order: vec![AttrSlot::Class],
        }),
        label: None,
        children,
        // The author DID write this block: the opener is a colon fence carrying
        // a lone \, and a matching fence closes it. Refusing a position on the
        // grounds that it is a synthesized wrapper had it backwards - the
        // `.hardbreaks` class is synthesized, the fence is not.
        pos,
    })
}
