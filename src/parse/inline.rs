use super::*;

// ============================================================================
// Inline parsing
// ============================================================================

/// Number of distinct `X}` pair closers tracked in `InlineBounds::delim_brace`.
/// Covers the critic markers (`+ - ~ #`) and the forced-emphasis delimiters
/// (`/ * _ ^ , = ~`); `~` is shared between critic substitution/strike.
pub(in crate::parse) const DELIM_BRACE_SLOTS: usize = 10;

/// Slot in `InlineBounds::delim_brace` for the leading byte of an `X}` pair, or
/// `None` for a byte that never opens a tracked pair.
#[inline]
pub(in crate::parse) fn delim_brace_slot(b: u8) -> Option<usize> {
    Some(match b {
        b'+' => 0,
        b'-' => 1,
        b'~' => 2,
        b'#' => 3,
        b'/' => 4,
        b'*' => 5,
        b'_' => 6,
        b'^' => 7,
        b',' => 8,
        b'=' => 9,
        _ => return None,
    })
}

/// Precomputed, per-inline-text closer positions used to short-circuit the
/// per-position construct scanners in `parse_inline_context`. Each scanner needs
/// a specific closing delimiter somewhere ahead; if the last occurrence of that
/// closer lies before the candidate opener, the scan could only walk to
/// end-of-text and fail, so it is skipped in O(1). Without these bounds, a run
/// of unclosed openers (`{+`×n, `[^`×n, `[x]{`×n, `<`×n, …) forces every opener
/// to re-scan to EOF -- classic O(n^2). Skipping only ever elides a scan that
/// would have returned `None`, so output stays byte-identical.
pub(in crate::parse) struct InlineBounds<'a> {
    /// Matching `]` index for every `[` (see `compute_bracket_matches`); empty
    /// when the text contains no bracket construct trigger.
    pub(in crate::parse) matches: &'a [usize],
    /// For each position, the `[` of the innermost bracket run holding it (see
    /// `compute_bracket_matches`). Empty alongside `matches`.
    pub(in crate::parse) run_at: &'a [usize],
    /// The `[` that each `]` closes, for the scan that hides a link
    /// destination (E2a). Empty alongside `matches`.
    pub(in crate::parse) openers: &'a [usize],
    /// Index of the last `)` (inline link/image destination closer).
    pub(in crate::parse) last_close_paren: Option<usize>,
    /// Index of the last `}` (attribute-block closer).
    pub(in crate::parse) last_close_brace: Option<usize>,
    /// Index of the last `]` (footnote-ref / inline-footnote / extension closer).
    pub(in crate::parse) last_close_bracket: Option<usize>,
    /// Index of the last `>` (crossref / autolink closer).
    pub(in crate::parse) last_gt: Option<usize>,
    /// Starts in this range already failed at the same whitespace byte.
    pub(in crate::parse) crossref_invalid_range: Cell<Option<(usize, usize)>>,
    /// For each tracked `X}` pair, the index of the leading `X` of its LAST
    /// occurrence (see `delim_brace_slot`). Used by critic markup and forced
    /// emphasis, whose closers are two-byte `X}` pairs.
    pub(in crate::parse) delim_brace: [Option<usize>; DELIM_BRACE_SLOTS],
    pub(in crate::parse) braced_closers: RefCell<Option<BracedClosers>>,
    pub(in crate::parse) native_pairs: Option<NativeBracedView>,
}

pub(in crate::parse) fn inline_pos(
    map: Option<&InlinePositionMap<'_>>,
    start: usize,
    end: usize,
) -> Option<Pos> {
    map.and_then(|m| m.pos(start, end))
}

pub(in crate::parse) fn set_inline_node_pos(node: &mut InlineNode, pos: Option<Pos>) {
    match node {
        InlineNode::Text(n) => n.pos = pos,
        InlineNode::EscapedText(n) => n.pos = pos,
        InlineNode::SmartPunctuation(n) => n.pos = pos,
        InlineNode::Emphasis(n) => n.pos = pos,
        InlineNode::Code(n) => n.pos = pos,
        InlineNode::Link(n) => n.pos = pos,
        InlineNode::Image(n) => n.pos = pos,
        InlineNode::Span(n) => n.pos = pos,
        InlineNode::Ruby(n) => n.pos = pos,
        InlineNode::Math(n) => n.pos = pos,
        InlineNode::RawInline(n) => n.pos = pos,
        InlineNode::LiteralInline(n) => n.pos = pos,
        InlineNode::Symbol(n) => n.pos = pos,
        InlineNode::AutoLink(n) => n.pos = pos,
        InlineNode::CrossRef(n) => n.pos = pos,
        InlineNode::CaptionNumber(n) => n.pos = pos,
        InlineNode::Mention(n) => n.pos = pos,
        InlineNode::Tag(n) => n.pos = pos,
        InlineNode::CitationGroup(n) => n.pos = pos,
        InlineNode::Extension(n) => n.pos = pos,
        InlineNode::Abbreviation(n) => n.pos = pos,
        InlineNode::Footnote(n) => n.pos = pos,
        InlineNode::NonBreakingSpace(n) => n.pos = pos,
        InlineNode::SoftBreak(n) | InlineNode::HardBreak(n) => n.pos = pos,
        InlineNode::CellWrap(n) => n.pos = pos,
        InlineNode::CriticInsert(n) => n.pos = pos,
        InlineNode::CriticDelete(n) => n.pos = pos,
        InlineNode::CriticSubstitute(n) => n.pos = pos,
        InlineNode::Comment(n) => n.pos = pos,
        InlineNode::CriticComment(n) => n.pos = pos,
    }
}

pub(in crate::parse) fn position_citation_items_from_map(
    group: &mut CitationGroup,
    positions: Option<&InlinePositionMap<'_>>,
    base: usize,
) {
    let inner_start = if group.integral() { 2 } else { 1 };
    if group.raw.len() <= inner_start || !group.raw.ends_with(']') {
        return;
    }
    let inner = &group.raw[inner_start..group.raw.len() - 1];
    let mut cursor = 0usize;
    for (item, part) in group.items.iter_mut().zip(inner.split(';')) {
        let leading = part.len() - part.trim_start().len();
        let trailing = part.len() - part.trim_end().len();
        let start = base + inner_start + cursor + leading;
        let end = base + inner_start + cursor + part.len() - trailing;
        item.pos = inline_pos(positions, start, end);
        cursor += part.len() + 1;
    }
}

impl InlineBounds<'_> {
    pub(in crate::parse) fn braced_closers(
        &self,
        bytes: &[u8],
    ) -> std::cell::RefMut<'_, BracedClosers> {
        std::cell::RefMut::map(self.braced_closers.borrow_mut(), |memo| {
            memo.get_or_insert_with(|| {
                let mut memo = BracedClosers::with_hosts(self.braced_hosts(bytes));
                memo.inherit(self.native_pairs.clone());
                memo
            })
        })
    }

    fn braced_hosts(&self, bytes: &[u8]) -> HashMap<usize, usize> {
        let mut hosts = HashMap::new();
        for (open, &close) in self.matches.iter().enumerate() {
            if bytes.get(open) == Some(&b'[') && close != usize::MAX && close > open {
                hosts.insert(open, close + 1);
            }
        }
        // A cross-reference resolves before braced spans (PART 8, E2a).
        let mut from = 0;
        while let Some(at) = find_seq(bytes, from, b"</#") {
            match crossref_close(bytes, at, self) {
                Some(close) => {
                    hosts.insert(at, close + 1);
                    from = close + 1;
                }
                None => from = at + 1,
            }
        }
        // An editorial or delimited comment is opaque (E2a, ruled 2026-10-15), so
        // a closer inside it is comment text.
        let last_brace = bytes.iter().rposition(|&byte| byte == b'}');
        for marker in *b"#%" {
            let closer = [marker, b'}'];
            let mut from = 0;
            while let Some(at) = find_seq(bytes, from, &[b'{', marker]) {
                // `{#id}` glued to what it decorates is an attribute block, not a comment.
                if decorated_braced_opener(bytes, at)
                    || (marker == b'#'
                        && at > 0
                        && !bytes[at - 1].is_ascii_whitespace()
                        && read_attrs_at(bytes, at, last_brace).is_some())
                {
                    from = at + 1;
                    continue;
                }
                let Some(close) = find_seq(bytes, at + 2, &closer) else {
                    break;
                };
                if marker == b'#' && close == at + 2 {
                    from = at + 1;
                    continue;
                }
                hosts.entry(at).or_insert(close + 2);
                from = close + 2;
            }
        }
        if !bytes.windows(2).any(|pair| pair == b"](") {
            return hosts;
        }
        let mut whitespace = vec![false; bytes.len()];
        if let Ok(text) = std::str::from_utf8(bytes) {
            for (at, ch) in text.char_indices() {
                whitespace[at] = ch.is_whitespace();
            }
        }
        let mut stops = vec![bytes.len(); bytes.len() + 2];
        let mut quotes = [vec![None; bytes.len() + 2], vec![None; bytes.len() + 2]];
        for at in (0..bytes.len()).rev() {
            stops[at] = match bytes[at] {
                b'\\' if matches!(bytes.get(at + 1), Some(b'\\' | b'(' | b')')) => stops[at + 2],
                b'(' => {
                    let close = stops[at + 1];
                    if bytes.get(close) == Some(&b')') {
                        stops[close + 1]
                    } else {
                        usize::MAX
                    }
                }
                b')' => at,
                _ if whitespace[at] => at,
                _ => stops[at + 1],
            };
            for (slot, quote) in (*b"\"'").into_iter().enumerate() {
                quotes[slot][at] = if bytes[at] == b'\\' && at + 1 < bytes.len() {
                    quotes[slot][at + 2]
                } else if bytes[at] == quote {
                    Some(at)
                } else {
                    quotes[slot][at + 1]
                };
            }
        }
        let mut covered = 0;
        for at in 1..bytes.len() {
            if at < covered
                || bytes[at - 1] != b']'
                || bytes[at] != b'('
                || !opens_a_link(bytes, at - 1, self)
            {
                continue;
            }
            let stop = stops[at + 1];
            if stop <= at + 1 || stop >= bytes.len() {
                continue;
            }
            let title_at = stop + usize::from(bytes[stop] == b' ');
            let complete = match bytes.get(title_at) {
                Some(b'"') => {
                    quotes[0][title_at + 1].is_some_and(|end| bytes.get(end + 1) == Some(&b')'))
                }
                Some(b'\'') => {
                    quotes[1][title_at + 1].is_some_and(|end| bytes.get(end + 1) == Some(&b')'))
                }
                _ => bytes[stop] == b')',
            };
            if complete {
                if let Some((_, _, end)) = read_link_target(bytes, at + 1, self.last_close_paren) {
                    hosts.insert(at, end);
                    covered = end;
                }
            }
        }
        hosts
    }

    /// True when a `]` occurs at or after `pos`.
    #[inline]
    fn has_bracket_from(&self, pos: usize) -> bool {
        self.last_close_bracket.is_some_and(|p| p >= pos)
    }

    /// True when a `>` occurs at or after `pos`.
    #[inline]
    /// The `[` of the innermost bracket run holding `pos`, or `None` outside every
    /// run. `None` too when no bracket construct can fire in this text at all.
    ///
    /// PART 8 resolves a bracket run before an emphasis marker, so a delimiter
    /// pair has to open and close in ONE run (markup-carve/carve#2577). This is
    /// the question every scan asks, and it is asked by the run's own opener
    /// rather than by a depth: two sibling runs sit at the same depth, so a depth
    /// comparison pairs `[a /b] [c d/]` across the gap between them.
    pub(in crate::parse) fn bracket_run_at(&self, pos: usize) -> Option<usize> {
        let &open = self.run_at.get(pos)?;
        // An UNCLOSED `[` delimits nothing, so it opens no run. The pairing pass
        // records it while it is on the stack, and only a `]` that pops it gives
        // it a match. No ENCLOSING bracket can have matched after an unmatched
        // one either: the stack pops the most recent `[`, so an outer `]` would
        // have matched this one first. Which is why this answers `None` rather
        // than looking outward.
        if open == NO_BRACKET_MATCH || self.matches.get(open) == Some(&NO_BRACKET_MATCH) {
            return None;
        }
        Some(open)
    }

    pub(in crate::parse) fn has_gt_from(&self, pos: usize) -> bool {
        self.last_gt.is_some_and(|p| p >= pos)
    }

    /// True when an `X}` pair with leading byte `delim` occurs at or after
    /// `pos`.
    #[inline]
    pub(in crate::parse) fn has_delim_brace_from(&self, delim: u8, pos: usize) -> bool {
        delim_brace_slot(delim).is_some_and(|s| self.delim_brace[s].is_some_and(|p| p >= pos))
    }
}

pub(crate) fn parse_matcher_inlines(text: &str, options: &Options<'_>) -> Vec<InlineNode> {
    let _recording = definition_regions::RecordingPause::enter();
    parse_inline_with_options(text, options)
}

pub(crate) fn parse_inline_with_options(text: &str, options: &Options<'_>) -> Vec<InlineNode> {
    parse_inline_context(text, options, false, false, None, 0)
}

pub(in crate::parse) fn parse_inline_with_anchor(
    text: &str,
    options: &Options<'_>,
    anchor: InlineAnchor<'_>,
) -> Vec<InlineNode> {
    if !options.positions {
        return parse_inline_with_options(text, options);
    }
    let map = InlinePositionMap::new(text, anchor);
    parse_inline_context(text, options, false, false, Some(&map), 0)
}

pub(in crate::parse) fn parse_caption_inline_with_options(
    text: &str,
    options: &Options<'_>,
    caption_context: bool,
) -> Vec<InlineNode> {
    parse_inline_context(text, options, caption_context, false, None, 0)
}

/// A caption's inline content, anchored to the source lines it was folded from.
///
/// Separate from `parse_inline_with_anchor` only because a caption may contain
/// a caption NUMBER placeholder and ordinary inline content may not, so the
/// two cannot share the flag.
pub(in crate::parse) fn parse_caption_inline_with_anchor(
    text: &str,
    options: &Options<'_>,
    lines: Vec<Option<(usize, isize)>>,
    caption_context: bool,
) -> Vec<InlineNode> {
    if !options.positions {
        return parse_caption_inline_with_options(text, options, caption_context);
    }
    let map = InlinePositionMap::new(text, InlineAnchor::lines(&lines));
    parse_inline_context(text, options, true, false, Some(&map), 0)
}

pub(in crate::parse) fn parse_inline_context(
    text: &str,
    options: &Options<'_>,
    caption_number_allowed: bool,
    in_footnote: bool,
    positions: Option<&InlinePositionMap<'_>>,
    base: usize,
) -> Vec<InlineNode> {
    parse_inline_context_with_pairs(
        text,
        options,
        caption_number_allowed,
        in_footnote,
        positions,
        base,
        None,
    )
}

pub(in crate::parse) fn parse_inline_context_with_pairs(
    text: &str,
    options: &Options<'_>,
    caption_number_allowed: bool,
    in_footnote: bool,
    positions: Option<&InlinePositionMap<'_>>,
    base: usize,
    pairs: Option<NativeBracedView>,
) -> Vec<InlineNode> {
    // Recursion cap (see MAX_NESTING_DEPTH). Nested links/spans/emphasis recurse
    // through here one frame per level; over the cap, keep the remaining text
    // literal rather than recursing further (prevents a stack-overflow abort on
    // input like `[[[[[…x]]]]]`). Shares the depth counter with block parsing.
    let quotes = quote_state::Frame::enter();
    let kinds = OpenKinds::enter();
    let Some(_depth) = DepthGuard::enter() else {
        return vec![InlineNode::text(text.to_string())];
    };
    let _kinds = kinds;
    if let Some(plain) = plain_inline_text(text, options, positions, base) {
        return plain;
    }
    let bytes = text.as_bytes();
    // A `[` only opens an inline link, reference link, or span when a `](`,
    // `][`, or `]{` follows (there is no bare shortcut-reference form). If none
    // occur, those attempts -- each an O(n) bracket scan -- can be skipped, so a
    // deeply nested run like `[[[[x]]]]` stays O(n) instead of O(n^2). Footnotes
    // (`[^...]`) are handled separately and cheaply gated on `[^`.
    let has_link_trigger = text.contains("](") || text.contains("][") || text.contains("]{");
    // Precompute every `[`-to-`]` match once (O(n)) so the per-`[` link /
    // reference / span / image parsers locate their closing bracket in O(1)
    // instead of re-scanning O(n) each. Without this, deeply nested balanced
    // links (`[[[...x]()]()...]`) are O(n^2). Only needed when bracket
    // constructs can actually fire.
    let has_brackets = has_link_trigger || text.contains("![");
    // Two readers need the matches for a run that opens NO construct, so the
    // precompute answers for every `[`: a `%%` marker inside a bracket run ends
    // at the run's closer (CARVE-P9-041, markup-carve/carve#2576), and an
    // emphasis delimiter does not pair across one (PART 8, carve#2577).
    let (bracket_matches, bracket_run_at) = if text.contains('[') {
        compute_bracket_matches(bytes)
    } else {
        (Vec::new(), Vec::new())
    };
    let bracket_openers = compute_bracket_openers(&bracket_matches);
    let mut run = InlineRun {
        text,
        bytes,
        options,
        in_footnote,
        caption_number_allowed,
        has_link_trigger,
        wraps_live: CELL_WRAPS.with(|wraps| !wraps.borrow().is_empty()),
        last_delimited_comment_close: bytes.windows(2).rposition(|w| w == b"%}"),
        bounds: inline_bounds(
            text,
            has_brackets,
            &bracket_matches,
            &bracket_openers,
            &bracket_run_at,
            pairs,
        ),
        quotes: &quotes,
        citation_brackets: std::cell::OnceCell::new(),
        // Keeps `_a](`×n / `*a](`×n linear. See cached_find_emphasis_close.
        emphasis_no_close: EmphasisMemo::default(),
        attribute_target: None,
        attribute_slots: None,
        sink: InlineSink {
            out: Vec::with_capacity(4),
            buf: String::with_capacity(text.len()),
            buf_start: None,
            buf_placeable: true,
            buf_src_delta: 0,
            positions,
            base,
        },
    };
    let mut i = 0;
    while i < bytes.len() {
        i = run.step(i);
    }
    run.sink.flush();
    quotes.finish(&mut run.sink.out);
    run.sink.out
}

/// The whole result for a short run of letters, digits and spaces that no
/// construct can claim, or `None` when the text needs the full scan.
fn plain_inline_text(
    text: &str,
    options: &Options<'_>,
    positions: Option<&InlinePositionMap<'_>>,
    base: usize,
) -> Option<Vec<InlineNode>> {
    let bytes = text.as_bytes();
    if !(options.extensions.is_empty()
        && !bytes.is_empty()
        && bytes.len() <= 128
        && {
            // Keep the ASCII check cheap. Only a Unicode byte needs the
            // character scan for letters and combining marks.
            match bytes
                .iter()
                .find(|&&b| !(b.is_ascii_alphanumeric() || b == b' '))
            {
                None => true,
                Some(&b) if !b.is_ascii() => text.chars().all(|c| {
                    c.is_alphanumeric()
                        || c == ' '
                        || unicode_normalization::char::is_combining_mark(c)
                }),
                _ => false,
            }
        }
        && (!bytes.contains(&b' ') || CELL_WRAPS.with(|wraps| wraps.borrow().is_empty())))
    {
        return None;
    }
    let mapped = positions
        .and_then(|map| map.unmapped_prefix.as_ref())
        .map_or(true, |prefix| {
            prefix
                .get(base)
                .zip(prefix.get(base + bytes.len()))
                .is_some_and(|(start, end)| start == end)
        });
    Some(vec![InlineNode::Text(Text {
        value: text.to_string(),
        pos: mapped
            .then(|| inline_pos(positions, base, base + bytes.len()))
            .flatten(),
    })])
}

/// Closer bounds for one inline run; see `InlineBounds`.
fn inline_bounds<'a>(
    text: &str,
    has_brackets: bool,
    matches: &'a [usize],
    openers: &'a [usize],
    run_at: &'a [usize],
    pairs: Option<NativeBracedView>,
) -> InlineBounds<'a> {
    let bytes = text.as_bytes();
    // Last-occurrence positions of each mandatory closer, precomputed once so the
    // per-position scanners short-circuit in O(1) when their closer cannot lie
    // ahead (see InlineBounds). Each is gated on a cheap presence check; a
    // `rposition`/pair scan runs only when that byte actually appears. This is
    // what keeps runs of unclosed openers linear instead of O(n^2).
    let last_close_paren = if has_brackets {
        bytes.iter().rposition(|&b| b == b')')
    } else {
        None
    };
    let has_close_brace = text.contains('}');
    let last_close_brace = if has_close_brace {
        bytes.iter().rposition(|&b| b == b'}')
    } else {
        None
    };
    let last_close_bracket = if text.contains(']') {
        bytes.iter().rposition(|&b| b == b']')
    } else {
        None
    };
    let last_gt = if text.contains('>') {
        bytes.iter().rposition(|&b| b == b'>')
    } else {
        None
    };
    // For each tracked `X}` pair (`+} -} ~} #}` for critic, plus the forced-
    // emphasis delimiters), record the leading byte's LAST position. Built only
    // when a `}` exists at all, since every such pair ends in `}`.
    let mut delim_brace: [Option<usize>; DELIM_BRACE_SLOTS] = [None; DELIM_BRACE_SLOTS];
    if has_close_brace {
        for p in 0..bytes.len().saturating_sub(1) {
            if bytes[p + 1] == b'}' {
                if let Some(slot) = delim_brace_slot(bytes[p]) {
                    delim_brace[slot] = Some(p);
                }
            }
        }
    }
    // The widened gate cannot reach a construct parser: every reader of `matches`
    // and `openers` needs a `](`, `][`, `]{` or `![` at the position it examines,
    // and `comment_needs_run_end` only fires where `has_brackets` found none of
    // those anywhere in the text.
    InlineBounds {
        matches,
        openers,
        run_at,
        last_close_paren,
        last_close_brace,
        last_close_bracket,
        last_gt,
        crossref_invalid_range: Cell::new(None),
        delim_brace,
        braced_closers: RefCell::new(None),
        native_pairs: pairs,
    }
}

/// The nodes emitted so far, plus the literal text buffered since the last one.
struct InlineSink<'a> {
    out: Vec<InlineNode>,
    buf: String,
    buf_start: Option<usize>,
    buf_placeable: bool,
    buf_src_delta: isize,
    positions: Option<&'a InlinePositionMap<'a>>,
    base: usize,
}

impl InlineSink<'_> {
    fn flush(&mut self) {
        flush_text(
            &mut self.out,
            &mut self.buf,
            self.positions,
            self.base,
            &mut self.buf_start,
            &mut self.buf_placeable,
            &mut self.buf_src_delta,
        );
    }

    fn pos(&self, start: usize, end: usize) -> Option<Pos> {
        inline_pos(self.positions, self.base + start, self.base + end)
    }

    /// Appends source text to the buffer, starting it at `start` if empty.
    fn buffer(&mut self, start: usize, source: &str) {
        if self.buf_start.is_none() {
            self.buf_start = Some(start);
        }
        self.buf.push_str(source);
    }

    /// Ends the text run and appends `node` as is.
    fn push(&mut self, node: InlineNode) {
        self.flush();
        self.out.push(node);
    }

    /// Ends the text run and appends `node` spanning `start..end`, through the
    /// quote frame.
    fn emit(
        &mut self,
        quotes: &quote_state::Frame,
        mut node: InlineNode,
        start: usize,
        end: usize,
    ) -> usize {
        self.flush();
        set_inline_node_pos(&mut node, self.pos(start, end));
        quotes.push(&mut self.out, node);
        end
    }
}

/// Scan state for one call of `parse_inline_context_with_pairs`.
struct InlineRun<'a, 'o> {
    text: &'a str,
    bytes: &'a [u8],
    options: &'a Options<'o>,
    in_footnote: bool,
    caption_number_allowed: bool,
    has_link_trigger: bool,
    wraps_live: bool,
    last_delimited_comment_close: Option<usize>,
    bounds: InlineBounds<'a>,
    quotes: &'a quote_state::Frame,
    citation_brackets: std::cell::OnceCell<HashMap<usize, usize>>,
    emphasis_no_close: EmphasisMemo,
    attribute_target: Option<usize>,
    attribute_slots: Option<HashSet<AttrSlot>>,
    sink: InlineSink<'a>,
}

impl InlineRun<'_, '_> {
    /// Consumes whatever starts at `i` and returns the next scan position.
    fn step(&mut self, i: usize) -> usize {
        let c = self.bytes[i];
        if c == 0 {
            let pos = self.sink.pos(i, i + 1);
            self.sink
                .push(InlineNode::NonBreakingSpace(NonBreakingSpace {
                    attrs: None,
                    pos,
                }));
            return i + 1;
        }

        // A live cell wrap can claim an ordinary space before any text run.
        if c == b' ' && self.wraps_live {
            if let Some(wrap) = cell_wrap_at(self.sink.base + i) {
                self.sink.push(InlineNode::CellWrap(wrap));
                return i + 1;
            }
        }
        // Literal punctuation and Unicode can join the ASCII prose run when
        // neither extensions nor cell wraps can claim its bytes.
        if self.options.extensions.is_empty() && !self.wraps_live {
            let end = self.literal_run_end(i);
            if end > i {
                self.sink.buffer(i, &self.text[i..end]);
                return end;
            }
        }
        // Keep the older ASCII run for contexts with live cell wraps.
        if self.options.extensions.is_empty()
            && (c.is_ascii_alphanumeric() || c == b' ' || c == b'\t')
        {
            return self.prose_run(i);
        }
        if let Some(next) = self.delimited_construct(c, i) {
            return next;
        }
        if let Some(next) = self.open_construct(c, i) {
            return next;
        }
        let ch = self.text[i..].chars().next().unwrap();
        let end = i + ch.len_utf8();
        self.sink.buffer(i, &self.text[i..end]);
        end
    }

    /// Consume literal prose in one slice, including punctuation and Unicode
    /// bytes that cannot open a core construct. Keep the older dispatch for
    /// cell wraps and extension matchers, which can claim ordinary bytes.
    fn literal_run_end(&self, start: usize) -> usize {
        let bytes = self.bytes;
        let mut end = start;
        while end < bytes.len() {
            let literal = match bytes[end] {
                b' ' | b'\t' | b',' | b';' | b'?' | b'&' | b')' => true,
                b'.' => !bytes[end..].starts_with(b"..."),
                b'-' => !matches!(bytes.get(end + 1), Some(b'-' | b'>')),
                b':' => matches!(bytes.get(end + 1), Some(b' ' | b'\t' | b'\n')),
                b'#' => {
                    !self.caption_number_allowed
                        && end > 0
                        && (bytes[end - 1].is_ascii_alphanumeric() || bytes[end - 1] == b'_')
                }
                b'@' => {
                    end > 0 && (bytes[end - 1].is_ascii_alphanumeric() || bytes[end - 1] == b'_')
                }
                b'!' => !matches!(bytes.get(end + 1), Some(b'=' | b'[' | b'`')),
                b'*' => {
                    end > 0
                        && bytes[end - 1] != b'*'
                        && bytes.get(end + 1) == Some(&b'*')
                        && bytes.get(end + 2) != Some(&b'*')
                }
                b'(' => ![b"(c)".as_slice(), b"(r)", b"(tm)"]
                    .iter()
                    .any(|form| bytes[end..].starts_with(form)),
                byte => byte.is_ascii_alphanumeric() || !byte.is_ascii(),
            };
            if !literal {
                break;
            }
            if bytes[end] == b'*' {
                end += 2;
                continue;
            }
            end += 1;
        }
        end
    }

    fn prose_run(&mut self, start: usize) -> usize {
        let bytes = self.bytes;
        let mut i = start + 1;
        while i < bytes.len()
            && (bytes[i].is_ascii_alphanumeric()
                || bytes[i] == b'\t'
                || (bytes[i] == b' '
                    && !(self.wraps_live && cell_wrap_at(self.sink.base + i).is_some())))
        {
            i += 1;
        }
        self.sink.buffer(start, &self.text[start..i]);
        i
    }

    /// Constructs that only a specific opening byte can start.
    fn delimited_construct(&mut self, c: u8, i: usize) -> Option<usize> {
        match c {
            b'\\' => self.escape(i),
            b'%' => self.trailing_comment(i),
            b'`' => self.code_span(i),
            b'$' => self.math(i),
            b'{' => self.brace(i),
            b'!' => self.bang(i),
            b'[' => self.bracket(i),
            b'<' => self.angle(i),
            b'@' => {
                let (mention, consumed) = parse_mention(self.text, i)?;
                Some(self.emit(InlineNode::Mention(mention), i, consumed))
            }
            b'#' => self.hash(i),
            b':' => self.colon(i),
            _ => None,
        }
    }

    fn emit(&mut self, node: InlineNode, i: usize, consumed: usize) -> usize {
        self.sink.emit(self.quotes, node, i, i + consumed)
    }

    fn escape(&mut self, i: usize) -> Option<usize> {
        let bytes = self.bytes;
        // Backslash escapes
        // A backslash at the very end of the content (no following byte) is a
        // hard break, mirroring the `\`-before-newline rule at end of input
        // (`para\` at EOF -> `<br>`), matching djot and the cheatsheet.
        if i + 1 >= bytes.len() {
            let pos = self.sink.pos(i, i + 1);
            self.sink.push(InlineNode::HardBreak(Break { pos }));
            return Some(i + 1);
        }
        let nxt = bytes[i + 1];
        if nxt == b' ' {
            let pos = self.sink.pos(i, i + 2);
            self.sink
                .push(InlineNode::NonBreakingSpace(NonBreakingSpace {
                    attrs: None,
                    pos,
                }));
            return Some(i + 2);
        }
        if !is_escapable(nxt) {
            return None;
        }
        // The escape is its own node: the backslash carries intent the
        // literal character does not (carve issue 350). The caret keeps
        // its placeholder inside the node's value, so the checks that
        // stop `\^` being read as a caption marker still see it.
        let pos = self.sink.pos(i, i + 2);
        self.sink.push(InlineNode::EscapedText(EscapedText {
            // The CHARACTER, not a marker standing in for it. PART 12
            // section 1 requires mapping internals on the way out, and
            // the node type already carries the only thing the marker
            // was distinguishing: an `escaped_text` node IS an escape,
            // so the writer emits a backslash plus this value without
            // needing the value to say so again (carve-rs#408).
            value: (nxt as char).to_string(),
            pos,
        }));
        if nxt == b'<' {
            if let Some(end) = crossref_close(bytes, i + 1, &self.bounds).map(|close| close + 1) {
                let pos = self.sink.pos(i + 2, end);
                self.sink.out.push(InlineNode::Text(Text {
                    value: self.text[i + 2..end].to_string(),
                    pos,
                }));
                return Some(end);
            }
        }
        Some(i + 2)
    }

    /// Explicitly delimited inline comment. The first closer wins; without
    /// one the opener remains ordinary visible text. Backslash escapes have
    /// already been consumed, and code/raw spans consume their whole run.
    fn delimited_comment(&mut self, i: usize) -> Option<usize> {
        let bytes = self.bytes;
        if bytes.get(i + 1) != Some(&b'%')
            || decorated_braced_opener(bytes, i)
            || !self
                .last_delimited_comment_close
                .is_some_and(|close| close >= i + 2)
        {
            return None;
        }
        let close_rel = bytes[i + 2..].windows(2).position(|w| w == b"%}")?;
        let close = i + 2 + close_rel;
        self.sink.flush();
        let raw = String::from_utf8_lossy(&bytes[i + 2..close]);
        let without_leading = raw.strip_prefix(' ').unwrap_or(&raw);
        let content = without_leading
            .strip_suffix(' ')
            .unwrap_or(without_leading)
            .to_string();
        let pos = self.sink.pos(i, close + 2);
        self.sink.out.push(InlineNode::Comment(Comment {
            block: false,
            delimited: true,
            content,
            pos,
        }));
        Some(close + 2)
    }

    /// Trailing line comment: `%%` at the start of the inline run or after
    /// whitespace runs to the end of the line and is dropped
    /// (`text %% comment`). A bracket run open here is the host: the marker
    /// needs no separator at its start, and takes nothing past its closer.
    fn trailing_comment(&mut self, i: usize) -> Option<usize> {
        let bytes = self.bytes;
        let comment_run = self.bounds.bracket_run_at(i);
        let run_start = comment_run.map_or(0, |open| open + 1);
        if bytes.get(i + 1) != Some(&b'%')
            || !(i == run_start || bytes[i - 1] == b' ' || bytes[i - 1] == b'\n')
        {
            return None;
        }
        // Popping from the END keeps the buffer equal to the source it
        // started at - `flush_text` measures the span as
        // `start .. start + buf.len()`, so a shorter buffer is a shorter
        // span, not a wrong one. Clearing `buf_placeable` here refused a
        // position for every line ending in a `%%` comment.
        while self.sink.buf.ends_with(' ') {
            self.sink.buf.pop();
        }
        let end = trailing_comment_end(
            bytes,
            i + 2,
            comment_run.map(|open| self.bounds.matches[open]),
        )
        .unwrap_or(bytes.len());
        // The comment renders to nothing, but it is PUBLISHED: PART 12 has a
        // `comment` node and carve-js and carve-php both emit one here, so a
        // tree that records what the author wrote cannot drop it
        // (carve-rs#513). Flush the text before it so the node lands in
        // source order.
        self.sink.flush();
        let raw = String::from_utf8_lossy(&bytes[i + 2..end]);
        let content = trailing_comment_content(&raw).to_string();
        let pos = self.sink.pos(i, end);
        self.sink.out.push(InlineNode::Comment(Comment {
            block: false,
            delimited: false,
            content,
            pos,
        }));
        Some(end)
    }

    /// Inline code spans
    fn code_span(&mut self, i: usize) -> Option<usize> {
        let bytes = self.bytes;
        let (value, consumed) = parse_inline_code(bytes, i)?;
        let cell_wraps = if self.wraps_live {
            verbatim_cell_wraps(bytes, self.sink.base, i, &value)
        } else {
            None
        };
        if let Some((mut raw, raw_consumed)) =
            parse_raw_inline_after_code(bytes, i, &value, consumed)
        {
            raw.cell_wraps = cell_wraps;
            return Some(self.emit(InlineNode::RawInline(raw), i, raw_consumed));
        }
        // Push the bare code span. A trailing inline attribute block
        // (`` `code`{.cls} ``) is attached by the general attr-merge in
        // the main loop, which runs AFTER the forced-emphasis / critic
        // checks -- so `` `c`{_u_} `` is a code span + forced underline,
        // not a bogus `_u_` attribute. Matches carve-js / carve-php.
        let pos = self.sink.pos(i, i + consumed);
        self.sink.push(InlineNode::Code(Code {
            cell_wraps,
            value,
            attrs: None,
            pos,
        }));
        Some(i + consumed)
    }

    fn math(&mut self, i: usize) -> Option<usize> {
        let (mut math, consumed) = parse_math(self.bytes, i, &self.bounds)?;
        if self.wraps_live {
            let tick = if math.display { i + 2 } else { i + 1 };
            math.cell_wraps = verbatim_cell_wraps(self.bytes, self.sink.base, tick, &math.content);
        }
        Some(self.emit(InlineNode::Math(math), i, consumed))
    }

    fn brace(&mut self, i: usize) -> Option<usize> {
        if let Some(next) = self.delimited_comment(i) {
            return Some(next);
        }
        let (bytes, options, in_footnote) = (self.bytes, self.options, self.in_footnote);
        let (positions, base) = (self.sink.positions, self.sink.base);
        if let Some((critic, consumed)) = parse_critic_markup(
            bytes,
            i,
            options,
            in_footnote,
            &self.bounds,
            positions,
            base,
        ) {
            return Some(self.emit(critic, i, consumed));
        }
        // Forced intraword emphasis `{X…X}` — tried before inline attribute
        // blocks, matching the reference scan order.
        if let Some((node, consumed)) = parse_forced_emphasis(
            bytes,
            i,
            options,
            in_footnote,
            &self.bounds,
            positions,
            base,
        ) {
            // A trailing `{...}` attribute block attaches to the forced span,
            // exactly like a bare span (`{*x*}{.c}` -> <strong class="c">x</strong>).
            return Some(self.emit_with_trailing_attrs(node, i, consumed));
        }
        self.attach_attributes(i)
    }

    /// Emits `node` and the attribute block glued to its end, if one parses.
    fn emit_with_trailing_attrs(
        &mut self,
        mut node: InlineNode,
        i: usize,
        consumed: usize,
    ) -> usize {
        let mut consumed = consumed;
        if self.bytes.get(i + consumed) == Some(&b'{') {
            if let Some((attrs, next)) =
                read_attrs_at(self.bytes, i + consumed, self.bounds.last_close_brace)
            {
                apply_attrs_to_inline(&mut node, attrs);
                consumed = next - i;
            }
        }
        self.emit(node, i, consumed)
    }

    /// A standalone attribute block merges into the immediately preceding
    /// inline node, so adjacent blocks chain (`[x]{.a}{.b}`,
    /// `*x*{.a}{.b}` -> merged classes, §15). It must be GLUED: a
    /// non-empty `buf` means text (e.g. a space) sits between the node
    /// and the `{`, so the block stays literal. An empty/invalid `{...}`
    /// also stays literal. Matches carve-php / carve-js.
    fn attach_attributes(&mut self, i: usize) -> Option<usize> {
        let bytes = self.bytes;
        if !self.sink.buf.is_empty() || !self.sink.out.last().is_some_and(inline_is_attributable) {
            return None;
        }
        let (attrs, mut next) = read_attrs_at(bytes, i, self.bounds.last_close_brace)?;
        let mut attrs = Some(attrs);
        let mut slots = None;
        while bytes.get(next) == Some(&b'{')
            && (bytes.get(next + 1) != Some(&b'_') || !self.bounds.has_delim_brace_from(b'_', next))
        {
            let Some((incoming, end)) = read_attrs_at(bytes, next, self.bounds.last_close_brace)
            else {
                break;
            };
            merge_attrs_indexed(&mut attrs, incoming, &mut slots);
            next = end;
        }
        if self.attribute_target != Some(self.sink.out.len()) {
            self.attribute_target = Some(self.sink.out.len());
            self.attribute_slots = None;
        }
        let attr_extent = self.sink.pos(i, next);
        let last = self.sink.out.last_mut().unwrap();
        // A reference link carries `raw_ref` (its literal source) in
        // case it stays unresolved. Merge the block into the link's
        // attrs (used when it resolves) AND append the block's
        // literal text to `raw_ref` (used when it reverts), so a
        // resolved `[t][r]{.a}{.b}` gets class="a b" while an
        // unresolved `[t][missing]{.a}{.b}` keeps both blocks literal.
        if let InlineNode::Link(l) = last {
            if let Some(raw) = l.raw_ref.as_mut() {
                if let Ok(lit) = std::str::from_utf8(&bytes[i..next]) {
                    raw.push_str(lit);
                }
            }
        }
        merge_attrs_into_inline(last, attrs.unwrap(), &mut self.attribute_slots);
        // An attached attribute block is owned by the construct it
        // decorates, so extend the construct through the closing
        // brace while retaining its original opening delimiter.
        if let Some(attr_extent) = attr_extent {
            if let Some(pos) = inline_pos_mut(last) {
                pos.end_line = attr_extent.end_line;
                pos.end_column = attr_extent.end_column;
                pos.end_offset = attr_extent.end_offset;
            }
        }
        Some(next)
    }

    fn bang(&mut self, i: usize) -> Option<usize> {
        let bytes = self.bytes;
        // Inline literal (§27): a `!` prefix on a verbatim code span, mirroring
        // the `$`-math prefix above. The span content is captured verbatim,
        // later HTML-escaped and emitted by every renderer with the `<code>`
        // wrapper dropped; a trailing `{…}` attaches below via the general
        // attr-merge as an ordinary inline attribute block (no special
        // first-token sigil). Like math it requires a CLOSED span — a bare `!`
        // before an unclosed run stays literal and the run becomes an ordinary
        // (unclosed) code span. Tried before the image case, which needs `[`.
        if bytes.get(i + 1) == Some(&b'`') {
            if let Some((lit, consumed)) = parse_literal_inline(bytes, i) {
                return Some(self.emit(InlineNode::LiteralInline(lit), i, consumed));
            }
        }
        // Image: ![alt](src), then reference image ![alt][ref] / ![alt][].
        if bytes.get(i + 1) != Some(&b'[') {
            return None;
        }
        let (img, consumed) = parse_image_at(bytes, i, &self.bounds)
            .or_else(|| parse_reference_image(bytes, i, &self.bounds))?;
        Some(self.emit(InlineNode::Image(img), i, consumed))
    }

    /// Footnote reference, then inline link `[text](href)`, reference link and
    /// span.
    fn bracket(&mut self, i: usize) -> Option<usize> {
        let (bytes, options, in_footnote) = (self.bytes, self.options, self.in_footnote);
        let (positions, base) = (self.sink.positions, self.sink.base);
        if !in_footnote {
            if let Some((footnote, consumed)) = parse_footnote_ref(bytes, i, &self.bounds) {
                return Some(self.emit(InlineNode::Footnote(footnote), i, consumed));
            }
        }
        if !self.has_link_trigger {
            return None;
        }
        let bounds = &self.bounds;
        let (node, consumed) =
            parse_inline_link_with_options(bytes, i, options, in_footnote, bounds, positions, base)
                .or_else(|| {
                    parse_reference_link(bytes, i, options, in_footnote, bounds, positions, base)
                })
                .map(|(link, consumed)| (InlineNode::Link(link), consumed))
                .or_else(|| {
                    parse_span(bytes, i, options, in_footnote, bounds, positions, base)
                        .map(|(span, consumed)| (InlineNode::Span(span), consumed))
                })?;
        Some(self.emit(node, i, consumed))
    }

    /// Cross-reference, then autolink.
    fn angle(&mut self, i: usize) -> Option<usize> {
        if let Some((crossref, consumed)) = parse_crossref(self.text, i, &self.bounds) {
            return Some(self.emit(InlineNode::CrossRef(crossref), i, consumed));
        }
        let (autolink, consumed) = parse_autolink(self.text, i, &self.bounds)?;
        Some(self.emit(InlineNode::AutoLink(autolink), i, consumed))
    }

    fn hash(&mut self, i: usize) -> Option<usize> {
        if self.caption_number_allowed
            && !self
                .bytes
                .get(i + 1)
                .is_some_and(|b| is_attr_ident_part(*b))
        {
            let pos = self.sink.pos(i, i + 1);
            self.sink.push(InlineNode::CaptionNumber(CaptionNumber {
                number: None,
                pos,
            }));
            self.caption_number_allowed = false;
            return Some(i + 1);
        }
        let (tag, consumed) = parse_tag(self.text, i)?;
        Some(self.emit(InlineNode::Tag(tag), i, consumed))
    }

    /// Inline extension `:name[content]`, then symbol.
    fn colon(&mut self, i: usize) -> Option<usize> {
        let (positions, base) = (self.sink.positions, self.sink.base);
        if let Some((node, consumed)) = parse_inline_extension(
            self.bytes,
            i,
            self.options,
            self.in_footnote,
            &self.bounds,
            positions,
            base,
        ) {
            return Some(self.emit(InlineNode::Extension(node), i, consumed));
        }
        let (symbol, consumed) = parse_symbol(self.text, i, &self.bounds)?;
        Some(self.emit(InlineNode::Symbol(symbol), i, consumed))
    }

    /// Constructs tried at any byte once the delimited ones declined, in
    /// precedence order.
    fn open_construct(&mut self, c: u8, i: usize) -> Option<usize> {
        let text = self.text;
        // PART 9 §8 (markup-carve/carve#1443): a hyphen run PRECEDED by
        // whitespace (or the start of the content) and FOLLOWED by a
        // non-whitespace character is a long CLI flag, not a dash, and stays
        // literal. `git log --oneline` rendered an en dash before `oneline` -
        // silently, and in the rendered output only.
        //
        // It joins the text buffer WHOLE, which is two decisions rather than
        // one: handing back a Text node would split `a --foo` into two text
        // nodes where the other engines keep one, and consuming the run a byte
        // at a time would leave `-->` as a stray `-` plus a live `->` symbol.
        if let Some(run) = flag_shaped_hyphen_run(text, i) {
            self.sink.buffer(i, &text[i..i + run]);
            return Some(i + run);
        }
        if let Some(next) = self.smart_punctuation(i) {
            return Some(next);
        }
        // Inline footnote `^[content]`. A `^` anywhere else is literal text
        // (there is no bare superscript), so `^^[x]` is a literal `^` + a note.
        if !self.in_footnote && c == b'^' && self.bytes.get(i + 1) == Some(&b'[') {
            let (positions, base) = (self.sink.positions, self.sink.base);
            if let Some((footnote, consumed)) =
                parse_inline_footnote(self.bytes, i, self.options, &self.bounds, positions, base)
            {
                return Some(self.emit(InlineNode::Footnote(footnote), i, consumed));
            }
        }
        // Bold-italic, sub, highlight, then single-char emphasis
        if let Some((node, consumed)) = match_emphasis(
            self.bytes,
            i,
            self.options,
            self.in_footnote,
            &mut self.emphasis_no_close,
            &self.bounds,
            self.sink.positions,
            self.sink.base,
        ) {
            return Some(self.emit_with_trailing_attrs(node, i, consumed));
        }
        if c == b'\n' {
            return Some(self.line_break(i));
        }
        self.extension_inline(i)
    }

    /// Smart typography (§8): parsed into AST nodes so renderers can choose
    /// glyph output or source-preserving Carve output without rescanning.
    fn smart_punctuation(&mut self, i: usize) -> Option<usize> {
        let (nodes, consumed) =
            parse_smart_punctuation_at(self.text, i, &self.sink.buf, &self.sink.out)?;
        self.sink.flush();
        let mut local = 0usize;
        for mut node in nodes {
            let width = smart_punctuation_source_width(&node);
            set_inline_node_pos(&mut node, self.sink.pos(i + local, i + local + width));
            local += width;
            self.quotes.push(&mut self.sink.out, node);
        }
        Some(i + consumed)
    }

    /// Soft break, or a hard break when the buffered text ends in `\`.
    fn line_break(&mut self, i: usize) -> usize {
        if self.sink.buf.ends_with('\\') {
            self.sink.buf.pop();
            // `hard_break = '\', newline`, so the span covers BOTH
            // characters. Placing the newline alone left the backslash in
            // no node at all: a break reporting one character where the
            // construct is two, which nothing rendered differently and no
            // checker could see (carve#549).
            let backslash = if i > 0 && self.bytes[i - 1] == b'\\' {
                i - 1
            } else {
                i
            };
            let pos = self.sink.pos(backslash, i + 1);
            self.sink.push(InlineNode::HardBreak(Break { pos }));
            return i + 1;
        }
        let pos = self.sink.pos(i, i + 1);
        self.sink.push(InlineNode::SoftBreak(Break { pos }));
        i + 1
    }

    fn extension_inline(&mut self, i: usize) -> Option<usize> {
        let text = self.text;
        let InlineMatch { mut node, end } =
            try_extension_inline(text, i, self.options, &self.citation_brackets)?;
        // `end` must land on a char boundary or `text[i..]`/slicing panics;
        // a misbehaving extension matcher must not be able to crash the core.
        if !(end > i && end <= text.len() && text.is_char_boundary(end)) {
            return None;
        }
        self.sink.flush();
        set_inline_node_pos(&mut node, self.sink.pos(i, end));
        if let InlineNode::CitationGroup(group) = &mut node {
            position_citation_items_from_map(group, self.sink.positions, self.sink.base + i);
        }
        self.quotes.push(&mut self.sink.out, node);
        Some(end)
    }
}

pub(in crate::parse) fn parse_critic_markup(
    bytes: &[u8],
    start: usize,
    options: &Options<'_>,
    in_footnote: bool,
    bounds: &InlineBounds<'_>,
    positions: Option<&InlinePositionMap<'_>>,
    base: usize,
) -> Option<(InlineNode, usize)> {
    // Match the two-byte opener on raw bytes -- validating `bytes[start..]` as
    // UTF-8 here would be O(n) at every `{`, i.e. O(n^2) over a run of critic
    // openers. Only the matched inner slice (up to the closing pair) is
    // validated, once a pair is located.
    if bytes.get(start) != Some(&b'{') {
        return None;
    }
    if decorated_braced_opener(bytes, start) {
        return None;
    }
    let content_start = start + 2;
    // Each critic form closes on a two-byte `X}` pair; if that pair does not lie
    // ahead, the `find_seq` could only scan to end-of-text and fail, so bail in
    // O(1). Keeps `{+`×n / `{-`×n / `{~ }`×n (no closing pair) linear.
    match bytes.get(start + 1).copied()? {
        b'+' => {
            if !bounds.has_delim_brace_from(b'+', start) {
                return None;
            }
            let pair = bounds.braced_closers(bytes).close(bytes, start, b'+')?;
            if !braced_closer_shares_the_run(bounds, start, pair) {
                return None;
            }
            // AN EMPTY BRACE PAIR IS NOT A CONSTRUCT (markup-carve/carve#1447).
            // `inline_content` is a one-or-more repetition, so an opener that
            // meets its own closer opened nothing and its characters are text.
            if pair == content_start {
                return None;
            }
            let inner = std::str::from_utf8(&bytes[content_start..pair]).ok()?;
            Some((
                InlineNode::CriticInsert(CriticInsert {
                    children: parse_inline_context(
                        inner,
                        options,
                        false,
                        in_footnote,
                        positions,
                        base + content_start,
                    ),
                    attrs: None,
                    pos: None,
                }),
                pair + 2 - start,
            ))
        }
        b'-' => {
            // A BRACED HYPHEN PAIR IS AN EN DASH (markup-carve/carve#1447). The
            // bare run carries a flanking guard, so `x --verbose y` stays
            // literal and an author who MEANT a dash there had no way to say so.
            // This is that way, and it cost nothing: the string it took was the
            // empty deletion below, which deletes nothing.
            if bytes.get(start..start + 4) == Some(b"{--}") {
                // The SAME node the bare run produces, carrying the authored
                // spelling in `value` - so the AST says "an en dash was written
                // here" rather than holding a glyph in a text run, and `fmt`
                // writes `{--}` back instead of the literal character. PART 12's
                // vocabulary already has the kind; the braced form is a second
                // spelling of it, not a second construct.
                return Some((
                    InlineNode::SmartPunctuation(SmartPunctuation {
                        kind: "en_dash".to_string(),
                        value: "{--}".to_string(),
                        glyph: None,
                        pos: None,
                    }),
                    4,
                ));
            }
            if !bounds.has_delim_brace_from(b'-', start) {
                return None;
            }
            let pair = bounds.braced_closers(bytes).close(bytes, start, b'-')?;
            if !braced_closer_shares_the_run(bounds, start, pair) {
                return None;
            }
            // An empty deletion is not a deletion, same rule as the insertion
            // above; the one string it spelled is the en dash just handled.
            if pair == content_start {
                return None;
            }
            let inner = std::str::from_utf8(&bytes[content_start..pair]).ok()?;
            Some((
                InlineNode::CriticDelete(CriticDelete {
                    children: parse_inline_context(
                        inner,
                        options,
                        false,
                        in_footnote,
                        positions,
                        base + content_start,
                    ),
                    attrs: None,
                    pos: None,
                }),
                pair + 2 - start,
            ))
        }
        b'~' => {
            // A critic substitution is `{~old~>new~}`: the `~>` separator must
            // sit within this `{~ … ~}`. Without it (`{~view~}`), this is not
            // critic markup -- it falls through to forced strike emphasis.
            if !bounds.has_delim_brace_from(b'~', start) {
                return None;
            }
            let (pair, arrow) = substitution_at(bytes, start, bounds)?;
            if !braced_closer_shares_the_run(bounds, start, pair) {
                return None;
            }
            let old = std::str::from_utf8(&bytes[content_start..arrow]).ok()?;
            let new = std::str::from_utf8(&bytes[arrow + 2..pair]).ok()?;
            Some((
                InlineNode::CriticSubstitute(CriticSubstitute {
                    old: parse_inline_context(
                        old,
                        options,
                        false,
                        in_footnote,
                        positions,
                        base + content_start,
                    ),
                    new: parse_inline_context(
                        new,
                        options,
                        false,
                        in_footnote,
                        positions,
                        base + arrow + 2,
                    ),
                    pos: None,
                }),
                pair + 2 - start,
            ))
        }
        b'#' => {
            if !bounds.has_delim_brace_from(b'#', start) {
                return None;
            }
            // No run check here, unlike the three forms above: the bracket-pairing
            // pass SKIPS an editorial comment, so a `[` inside one opens no run and
            // both ends of this pair always answer the same `None`. A guard here
            // could not fire - verified with a probe - and `[{#a]#}` reads as a
            // comment in all three engines, so the crossing closer is its own
            // divergence rather than this one (carve-rs#2212).
            let pair = find_seq(bytes, content_start, b"#}")?;
            // An empty comment says nothing to anybody, and `comment_content`
            // requires a character since markup-carve/carve#1447.
            if pair == content_start {
                return None;
            }
            let inner = std::str::from_utf8(&bytes[content_start..pair]).ok()?;
            Some((
                InlineNode::CriticComment(CriticComment {
                    text: inner.to_string(),
                    pos: None,
                }),
                pair + 2 - start,
            ))
        }
        _ => None,
    }
}

/// Whether a braced inline's closer sits in the bracket run its opener sits in.
///
/// PART 8 resolves a bracket run before a braced inline, so an `X}` inside one is
/// that run's content and closes nothing opened outside it - the rule the bare and
/// forced emphasis forms already read (markup-carve/carve#2577, carve-rs#2173),
/// applied to the critic family as the same rule in another spelling
/// (carve-rs#2212).
// Brackets inside an unclosed verbatim run are content until its raw closer.
pub(in crate::parse) fn braced_closer_shares_the_run(
    bounds: &InlineBounds<'_>,
    open: usize,
    close: usize,
) -> bool {
    bounds.bracket_run_at(open).is_none()
        || bounds.bracket_run_at(close) == bounds.bracket_run_at(open)
        || bounds
            .braced_closers
            .borrow()
            .as_ref()
            .and_then(BracedClosers::last_verbatim_start)
            .is_some_and(|start| bounds.bracket_run_at(start) == bounds.bracket_run_at(open))
}

pub(in crate::parse) fn parse_footnote_ref(
    bytes: &[u8],
    start: usize,
    bounds: &InlineBounds<'_>,
) -> Option<(Footnote, usize)> {
    if bytes.get(start) != Some(&b'[') || bytes.get(start + 1) != Some(&b'^') {
        return None;
    }
    // The id runs to the closing `]`; with no `]` ahead the scan could only walk
    // to end-of-text and fail, so bail in O(1) (keeps `[^`×n linear).
    if !bounds.has_bracket_from(start) {
        return None;
    }
    let mut i = start + 2;
    // A label is a physical-line identifier. Definition markers cannot cross
    // a newline, so such a reference could never bind to a valid definition.
    while i < bytes.len() && bytes[i] != b']' && bytes[i] != b'\n' && bytes[i] != b'\r' {
        i += 1;
    }
    if i >= bytes.len() || bytes[i] != b']' {
        return None;
    }
    let id = std::str::from_utf8(&bytes[start + 2..i]).ok()?.to_string();
    // Same production, reference side: `[^]` is not a `reference_footnote`.
    // Carve has no shortcut reference either, so it is literal text - which is
    // what it already was here until a `[^]: …` line existed for it to bind to.
    if id.is_empty() {
        return None;
    }
    let mut attrs = None;
    let mut after = i + 1;
    if bytes.get(after) == Some(&b'{') {
        if let Some((parsed_attrs, next)) = read_attrs_at(bytes, after, bounds.last_close_brace) {
            attrs = Some(parsed_attrs);
            after = next;
        }
    }
    Some((
        Footnote {
            attrs,
            id: Some(id),
            inline: None,
            number: None,
            ref_id: None,
            pos: None,
        },
        after - start,
    ))
}

pub(in crate::parse) fn parse_inline_footnote(
    bytes: &[u8],
    start: usize,
    options: &Options<'_>,
    bounds: &InlineBounds<'_>,
    positions: Option<&InlinePositionMap<'_>>,
    base: usize,
) -> Option<(Footnote, usize)> {
    if bytes.get(start) != Some(&b'^') || bytes.get(start + 1) != Some(&b'[') {
        return None;
    }
    // The body runs to a balancing `]`; with no `]` ahead the bracket scan could
    // only walk to end-of-text and fail, so bail in O(1) (keeps `^[`×n linear).
    if !bounds.has_bracket_from(start) {
        return None;
    }
    let (content, after_bracket) = read_bracketed(bytes, start + 1)?;
    if content.trim().is_empty() {
        return None;
    }
    let mut attrs = None;
    let mut after = after_bracket;
    if bytes.get(after) == Some(&b'{') {
        if let Some((parsed_attrs, next)) = read_attrs_at(bytes, after, bounds.last_close_brace) {
            attrs = Some(parsed_attrs);
            after = next;
        }
    }
    let children = {
        let _scope = quote_state::Scope::enter();
        parse_inline_context(&content, options, false, true, positions, base + start + 2)
    };
    Some((
        Footnote {
            attrs,
            id: None,
            inline: Some(children),
            number: None,
            ref_id: None,
            pos: None,
        },
        after - start,
    ))
}

pub(in crate::parse) fn parse_raw_inline_after_code(
    bytes: &[u8],
    start: usize,
    value: &str,
    code_consumed: usize,
) -> Option<(RawInline, usize)> {
    let attr_start = start + code_consumed;
    if bytes.get(attr_start) != Some(&b'{') || bytes.get(attr_start + 1) != Some(&b'=') {
        return None;
    }
    let mut i = attr_start + 2;
    let format_start = i;
    while i < bytes.len() && bytes[i] != b'}' {
        i += 1;
    }
    if i >= bytes.len() {
        return None;
    }
    if format_start == i {
        return None;
    }
    let format = std::str::from_utf8(&bytes[format_start..i]).ok()?;
    // The format must be a valid `format_name` (an identifier: letter/`_`
    // start, then letter/digit/`_`/`-`), per grammar §20. Anything else
    // (`{=h=}`, `{==h==}`, `{=text/html}`) is NOT a raw inline -- it falls back
    // to a plain code span plus forced-emphasis / literal text, matching
    // carve-js / carve-php. Without this rs greedily consumed the code span and
    // dropped its content for a bogus format.
    let mut fc = format.bytes();
    let valid = matches!(fc.next(), Some(b) if b.is_ascii_alphabetic() || b == b'_')
        && fc.all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-');
    if !valid {
        return None;
    }
    Some((
        RawInline {
            cell_wraps: None,
            format: format.to_string(),
            content: value.to_string(),
            injected: false,
            pos: None,
        },
        i + 1 - start,
    ))
}

/// Inline literal (`` !`…` ``, grammar PART 9 §27, `literal_inline = '!',
/// code_span`). A `!` PREFIX on a verbatim code span, mirroring `parse_math`'s
/// `$` prefix: the maximal backtick run captures the content verbatim, which is
/// later HTML-escaped and emitted by every renderer with the `<code>` wrapper
/// dropped. Like code and math, an unclosed span reaches the end of its block.
///
/// Returns a bare literal; a trailing `{…}` is the ORDINARY inline attribute
/// block and is attached by the general attr-merge in the scanner (same path a
/// bare code span uses), so `` !`x`{.ipa} `` and chained `` !`x`{.a}{.b} ``
/// both work without any special first-token handling here.
pub(in crate::parse) fn parse_literal_inline(
    bytes: &[u8],
    start: usize,
) -> Option<(LiteralInline, usize)> {
    if bytes.get(start) != Some(&b'!') || bytes.get(start + 1) != Some(&b'`') {
        return None;
    }
    let tick = start + 1;
    let (content, code_consumed) = parse_inline_code(bytes, tick)?;
    Some((
        LiteralInline {
            content,
            attrs: None,
            pos: None,
        },
        tick + code_consumed - start,
    ))
}

pub(in crate::parse) fn parse_math(
    bytes: &[u8],
    start: usize,
    bounds: &InlineBounds<'_>,
) -> Option<(Math, usize)> {
    let display = bytes.get(start + 1) == Some(&b'$');
    let tick = if display { start + 2 } else { start + 1 };
    if bytes.get(tick) != Some(&b'`') {
        return None;
    }
    // Math reuses a code span for its verbatim body (grammar `math_inline =
    // '$', code_span`): a MAXIMAL backtick run opens and an equal-length run
    // closes, so `$``a``` and `$`a``b`` behave like the code span `` `a``b` ``.
    let (content, code_consumed) = parse_inline_code(bytes, tick)?;
    // Empty verbatim content is NOT math (`$``` / `$$```): the `$` stays literal
    // and the backtick pair is an empty code span. Matches carve-js / carve-php.
    if content.is_empty() {
        return None;
    }
    let end = tick + code_consumed;
    // A trailing attribute block attaches to the math span (math reuses the
    // code-span attribute slot), EXCEPT `{=format}`, the raw-inline form,
    // which is code-span-only and not inherited by math -- leave it literal.
    let mut attrs = None;
    let mut after = end;
    if bytes.get(end) == Some(&b'{') && bytes.get(end + 1) != Some(&b'=') {
        if let Some((parsed, next)) = read_attrs_at(bytes, end, bounds.last_close_brace) {
            attrs = Some(parsed);
            after = next;
        }
    }
    Some((
        Math {
            cell_wraps: None,
            attrs,
            display,
            content,
            label: None,
            number: None,
            pos: None,
        },
        after - start,
    ))
}

pub(in crate::parse) fn parse_reference_link(
    bytes: &[u8],
    start: usize,
    options: &Options<'_>,
    in_footnote: bool,
    bounds: &InlineBounds<'_>,
    positions: Option<&InlinePositionMap<'_>>,
    base: usize,
) -> Option<(Link, usize)> {
    let text_close = bracketed_close(bytes, start, bounds.matches)?;
    let after_text = text_close + 1;
    if bytes.get(after_text) != Some(&b'[') {
        return None;
    }
    let label_close = reference_label_close(bytes, after_text)?;
    let after_label = label_close + 1;
    // Both brackets are present, so materializing their labels now costs O(1)
    // per accepted reference rather than per candidate `[`.
    let text = std::str::from_utf8(&bytes[start + 1..text_close])
        .ok()?
        .to_string();
    let label = std::str::from_utf8(&bytes[after_text + 1..label_close])
        .ok()?
        .to_string();
    let ref_label = if label.is_empty() {
        text.clone()
    } else {
        label
    };
    // A trailing attribute block attaches to the resolved link, the same
    // slot an inline link uses (`[t][x]{.c}`).
    let mut attrs = None;
    let mut after = after_label;
    if bytes.get(after) == Some(&b'{') {
        if let Some((parsed_attrs, next)) = read_attrs_at(bytes, after, bounds.last_close_brace) {
            attrs = Some(parsed_attrs);
            after = next;
        }
    }
    let mut link = Link {
        attrs,
        href: String::new(),
        title: None,
        children: parse_inline_context(
            &text,
            options,
            false,
            in_footnote,
            positions,
            base + start + 1,
        ),
        ref_label: Some(ref_label),
        // `raw_ref` is the literal source emitted only when the
        // reference does not resolve; it must include the consumed
        // attribute block so an unresolved `[t][x]{.c}` stays fully
        // literal rather than silently dropping the `{.c}`. A resolved
        // reference ignores `raw_ref` and applies `attrs` instead.
        raw_ref: Some(std::str::from_utf8(&bytes[start..after]).ok()?.to_string()),
        from_crossref: false,
        from_heading_reference: false,
        pos: None,
        resolved_empty_reference: false,
    };
    resolve_active_link(&mut link);
    Some((link, after - start))
}

pub(in crate::parse) fn flush_text(
    out: &mut Vec<InlineNode>,
    buf: &mut String,
    positions: Option<&InlinePositionMap<'_>>,
    base: usize,
    buf_start: &mut Option<usize>,
    buf_placeable: &mut bool,
    buf_src_delta: &mut isize,
) {
    if !buf.is_empty() {
        let start = buf_start.take().unwrap_or(0);
        // The span ends where the SOURCE run ends, which is not
        // `start + buf.len()` whenever a substitution made the buffer a
        // different length than the bytes it came from. `buf_src_delta` carries
        // that difference so the end stays anchored to the source; without it a
        // run holding one no-break-space escape reported a span one byte short.
        let end =
            (start as isize + buf.len() as isize + *buf_src_delta).max(start as isize) as usize;
        out.push(InlineNode::Text(Text {
            value: std::mem::take(buf),
            pos: (*buf_placeable
                && positions
                    .and_then(|map| map.unmapped_prefix.as_ref())
                    .map_or(true, |prefix| {
                        prefix
                            .get(base + start)
                            .zip(prefix.get(base + end))
                            .is_some_and(|(start, end)| start == end)
                    }))
            .then(|| inline_pos(positions, base + start, base + end))
            .flatten(),
        }));
    }
    *buf_start = None;
    *buf_placeable = true;
    *buf_src_delta = 0;
}

/// The space class the hyphen-run flanking test reads: PART 7's four whitespace
/// characters plus the NO-BREAK SPACE, in either of its spellings.
///
/// NOT `char::is_whitespace`, which takes a VERTICAL TAB and a FORM FEED. Carve
/// reads both as CONTENT, so `---<VT>` has to answer the way `---!` answers.
pub(in crate::parse) fn is_flank_space(ch: char) -> bool {
    matches!(ch, ' ' | '\t' | '\n' | '\r' | '\u{00a0}') || ch == crate::NBSP_PLACEHOLDER
}

/// The length of the hyphen run at `i` when that run is flag-shaped
/// (markup-carve/carve#1443), else `None`.
///
/// The narrowness is the design: only a run with whitespace BEFORE and a
/// non-whitespace character AFTER is excluded. `pages 1--10`, `the Mon--Fri
/// window` and a trailing `text --` are all left-flanked by a word or
/// right-flanked by a space, and every one of them still converts.
pub(in crate::parse) fn flag_shaped_hyphen_run(text: &str, i: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    if bytes.get(i) != Some(&b'-') || bytes.get(i + 1) != Some(&b'-') {
        return None;
    }
    // `-->` is the canonical rightwards arrow (markup-carve/carve#1442) and is
    // taken before any hyphen run, so the guard must not claim it first - a
    // spaced `x --> y` is flag-SHAPED and is an arrow all the same. Three
    // hyphens are not `-->`, so `x ---> y` still reaches the guard.
    if text[i..].starts_with("-->") {
        return None;
    }
    let n = bytes[i..].iter().take_while(|&&b| b == b'-').count();
    let after = text[i + n..].chars().next()?;
    if is_flank_space(after) {
        return None;
    }
    match text[..i].chars().next_back() {
        None => Some(n),
        Some(before) if is_flank_space(before) => Some(n),
        Some(_) => None,
    }
}

pub(in crate::parse) fn parse_smart_punctuation_at(
    text: &str,
    i: usize,
    buf: &str,
    out: &[InlineNode],
) -> Option<(Vec<InlineNode>, usize)> {
    let prev = if buf.is_empty() {
        last_emitted_glyph(out)
    } else {
        buf.chars().last().unwrap_or_default()
    };
    // The hyphen-run pass must not claim `-->` or `<-->`: the FIXED TOKENS below
    // spell the canonical rightwards and bidirectional arrows with a doubled run
    // (markup-carve/carve#1442), and a run decomposed into dashes here can never
    // reach them. `--` with no `>` still falls through to the dash allocation.
    let arrow_run = text[i..].starts_with("-->") || text[i..].starts_with("<-->");
    if !arrow_run
        && text.as_bytes().get(i) == Some(&b'-')
        && text.as_bytes().get(i + 1) == Some(&b'-')
    {
        let n = text.as_bytes()[i..]
            .iter()
            .take_while(|&&b| b == b'-')
            .count();
        let glyphs = crate::render::allocate_dashes(n);
        let mut consumed = 0usize;
        let mut nodes = Vec::new();
        for glyph in glyphs.chars() {
            let (kind, width) = if glyph == '—' {
                ("em_dash", 3)
            } else {
                ("en_dash", 2)
            };
            nodes.push(InlineNode::SmartPunctuation(SmartPunctuation {
                kind: kind.to_string(),
                value: text[i + consumed..i + consumed + width].to_string(),
                glyph: None,
                pos: None,
            }));
            consumed += width;
        }
        return Some((nodes, n));
    }

    // ORDERED LONGEST-FIRST, and the order is the rule (markup-carve/carve#1442):
    // `<-->` beats `<->` and `<--`, `-->` beats the hyphen-run pass above, `<==`
    // beats `<=`, `(tm)` beats `(c)`.
    //
    // The doubled run is the canonical arrow in both families. `<-` `->` `<->`
    // still match and are DEPRECATED rather than removed, so a document written
    // before the rule goes on working. `=>` is GONE: it is ubiquitous in prose
    // about code (`key => value`, `x => x + 1`, `Some(x) => x`) and every one of
    // those silently became an arrow in the rendered output only. `<=` keeps ≤
    // for the mirror-image reason, which is what forces the left double arrow to
    // grow a character in the first place.
    for (source, kind) in [
        ("<-->", "left_right_arrow"),
        ("<->", "left_right_arrow"),
        ("-->", "rightwards_arrow"),
        ("<--", "leftwards_arrow"),
        ("<=>", "left_right_double_arrow"),
        ("==>", "rightwards_double_arrow"),
        ("<==", "leftwards_double_arrow"),
        ("(tm)", "trademark"),
        ("...", "ellipsis"),
        ("->", "rightwards_arrow"),
        ("<-", "leftwards_arrow"),
        ("<=", "less_than_or_equal"),
        (">=", "greater_than_or_equal"),
        ("!=", "not_equal"),
        ("+-", "plus_minus"),
        ("(c)", "copyright"),
        ("(r)", "registered"),
    ] {
        if text[i..].starts_with(source) {
            return Some((
                vec![InlineNode::SmartPunctuation(SmartPunctuation {
                    kind: kind.to_string(),
                    value: source.to_string(),
                    glyph: None,
                    pos: None,
                })],
                source.len(),
            ));
        }
    }

    let c = text[i..].chars().next()?;
    if c == '"' {
        let open =
            quote_open_context(prev) && !closes_after_dash(prev, text[i + 1..].chars().next());
        let glyph = if open { "“" } else { "”" };
        let kind = if open {
            "left_double_quote"
        } else {
            "right_double_quote"
        };
        return Some((
            vec![InlineNode::SmartPunctuation(SmartPunctuation {
                kind: kind.to_string(),
                value: "\"".to_string(),
                glyph: Some(glyph.to_string()),
                pos: None,
            })],
            1,
        ));
    }
    if c == '\'' {
        let next = text[i + 1..].chars().next();
        let open = quote_open_context(prev) && !closes_after_dash(prev, next);
        let apostrophe = quote_state::single(
            text,
            i,
            prev,
            next,
            open,
            out.len() + usize::from(!buf.is_empty()),
        );
        let glyph = if apostrophe { "’" } else { "‘" };
        let kind = if apostrophe {
            "right_single_quote"
        } else {
            "left_single_quote"
        };
        return Some((
            vec![InlineNode::SmartPunctuation(SmartPunctuation {
                kind: kind.to_string(),
                value: "'".to_string(),
                glyph: Some(glyph.to_string()),
                pos: None,
            })],
            1,
        ));
    }

    None
}

pub(in crate::parse) fn smart_punctuation_source_width(node: &InlineNode) -> usize {
    match node {
        InlineNode::SmartPunctuation(s) => s.value.len(),
        _ => 0,
    }
}

pub(in crate::parse) fn last_emitted_glyph(out: &[InlineNode]) -> char {
    match out
        .iter()
        .rev()
        .find(|node| !matches!(node, InlineNode::Comment(_)))
    {
        Some(InlineNode::Text(t)) => t.value.chars().last().unwrap_or('\0'),
        Some(InlineNode::SmartPunctuation(node)) => {
            smart_punctuation_glyph(node).chars().last().unwrap_or('x')
        }
        // An escaped character is its own node but still the character before
        // the quote, and quote flanking reads that character: `\{"quoted"`
        // opens on the brace exactly as an unescaped `{` would (corpus 163).
        Some(InlineNode::NonBreakingSpace(_)) => '\u{00a0}',
        // A line break is whitespace to quote flanking (markup-carve/carve#2822).
        Some(InlineNode::SoftBreak(_) | InlineNode::HardBreak(_) | InlineNode::CellWrap(_)) => '\n',
        Some(InlineNode::EscapedText(t)) => t.value.chars().last().unwrap_or('x'),
        None => '\0',
        Some(_) => 'x',
    }
}

pub(in crate::parse) fn closes_after_dash(prev: char, next: Option<char>) -> bool {
    matches!(prev, '-' | '–' | '—' | '/')
        && next.map_or(true, |c| is_flank_space(c) || "\"'.,;:!?)]".contains(c))
}

pub(in crate::parse) fn quote_open_context(prev: char) -> bool {
    prev == '\0'
        || is_flank_space(prev)
        || matches!(
            prev,
            '(' | '[' | '{' | '=' | ':' | '-' | '/' | '–' | '—' | '“' | '‘'
        )
}

pub(in crate::parse) fn is_escapable(b: u8) -> bool {
    matches!(
        b,
        b'\\'
            | b'`'
            | b'*'
            | b'_'
            | b'{'
            | b'}'
            | b'['
            | b']'
            | b'('
            | b')'
            | b'"'
            | b'\''
            | b'#'
            | b'+'
            | b'-'
            | b'.'
            | b'!'
            | b'~'
            | b'^'
            | b'/'
            | b'<'
            | b'>'
            | b'@'
            | b'%'
            | b'|'
            | b'='
            | b','
            | b':'
            | b';'
            | b'$'
            | b'&'
            | b'?'
    )
}

// The closed-verbatim-span single-space strip: one leading and one trailing
// space are removed when the content BOTH begins and ends with a space -- but
// NOT when it consists entirely of spaces. The all-space guard matches the
// executable spec's `codeText()` and the CommonMark rule it derives from
// ("...but does not consist entirely of space characters"). Without the guard
// `` `  ` `` would strip to the empty string, and an empty verbatim span has no
// representable Carve source (a bare `` `` `` reparses as a two-backtick
// opener), so `carve fmt` could not round-trip it. Shared by the closed and
// unclosed verbatim paths so the two cannot drift apart.
pub(in crate::parse) fn strip_verbatim_padding(raw: &str) -> &str {
    if raw.starts_with(' ') && raw.ends_with(' ') && !raw.chars().all(|c| c == ' ') {
        &raw[1..raw.len() - 1]
    } else {
        raw
    }
}

pub(in crate::parse) fn parse_inline_code(bytes: &[u8], start: usize) -> Option<(String, usize)> {
    // Count opening backticks
    let mut i = start;
    while i < bytes.len() && bytes[i] == b'`' {
        i += 1;
    }
    let open_len = i - start;
    if open_len == 0 {
        return None;
    }
    let content_start = i;
    // Find closing run of exactly `open_len` backticks not followed by another backtick
    while i < bytes.len() {
        if bytes[i] != b'`' {
            i += 1;
            continue;
        }
        let close_start = i;
        while i < bytes.len() && bytes[i] == b'`' {
            i += 1;
        }
        let close_len = i - close_start;
        if close_len == open_len {
            let raw = std::str::from_utf8(&bytes[content_start..close_start]).ok()?;
            return Some((strip_verbatim_padding(raw).to_string(), i - start));
        }
        // Different length closer — keep scanning past it
    }
    // No matching closer: an unclosed verbatim opener is opaque to the end of
    // the text (matches djot / carve-php / carve-js). Whatever ends the run
    // strips its trailing whitespace, a forced-span closer as much as the block
    // end, line breaks included (markup-carve/carve#2051, markup-carve/carve#2089).
    // In a line block a break is content, so only spaces and tabs go there
    // (corpus 380).
    let raw = std::str::from_utf8(&bytes[content_start..]).ok()?;
    let raw = if InLineBlock::inside() {
        raw.trim_end_matches([' ', '\t'])
    } else {
        raw.trim_end_matches([' ', '\t', '\n', '\r'])
    };
    Some((strip_verbatim_padding(raw).to_string(), bytes.len() - start))
}

pub(in crate::parse) fn parse_image_at(
    bytes: &[u8],
    start: usize,
    bounds: &InlineBounds<'_>,
) -> Option<(Image, usize)> {
    if bytes.get(start) != Some(&b'!') || bytes.get(start + 1) != Some(&b'[') {
        return None;
    }
    let alt_close = bracketed_close(bytes, start + 1, bounds.matches)?;
    let after_alt = alt_close + 1;
    if bytes.get(after_alt) != Some(&b'(') {
        return None;
    }
    let (src, title, after_paren) =
        read_link_target(bytes, after_alt + 1, bounds.last_close_paren)?;
    // Only a valid `(target)` reaches here, so the alt copy is deferred off the
    // failing-`![...]()` path that would otherwise be O(n) per position.
    let raw_alt = std::str::from_utf8(&bytes[start + 2..alt_close]).ok()?;
    let alt = unescape_title(raw_alt);
    let mut attrs = None;
    let mut after = after_paren;
    if bytes.get(after) == Some(&b'{') {
        if let Some((parsed_attrs, next)) = read_attrs_at(bytes, after, bounds.last_close_brace) {
            attrs = Some(parsed_attrs);
            after = next;
        }
    }
    Some((
        Image {
            attrs,
            src,
            alt,
            title,
            ref_label: None,
            raw_ref: None,
            resolved_empty_reference: false,
            pos: None,
        },
        after - start,
    ))
}

/// Parse a reference image `![alt][ref]` / collapsed `![alt][]` — the image
/// form of a reference link, mirroring `parse_reference_link`. `src` is empty
/// until `resolve_reference_links` fills it from the matching `[label]: url`
/// def; the full form allows an empty alt (label = ref), collapsed needs a
/// non-empty alt (label = alt).
pub(in crate::parse) fn parse_reference_image(
    bytes: &[u8],
    start: usize,
    bounds: &InlineBounds<'_>,
) -> Option<(Image, usize)> {
    if bytes.get(start) != Some(&b'!') || bytes.get(start + 1) != Some(&b'[') {
        return None;
    }
    let alt_close = bracketed_close(bytes, start + 1, bounds.matches)?;
    let after_alt = alt_close + 1;
    if bytes.get(after_alt) != Some(&b'[') {
        return None;
    }
    let label_close = reference_label_close(bytes, after_alt)?;
    let after_label = label_close + 1;
    let raw_alt = std::str::from_utf8(&bytes[start + 2..alt_close]).ok()?;
    let alt = unescape_title(raw_alt);
    let label = std::str::from_utf8(&bytes[after_alt + 1..label_close])
        .ok()?
        .to_string();
    // Collapsed `![alt][]` reuses the alt as the label, so it needs a non-empty
    // alt; the full `![alt][ref]` form accepts an empty alt (label = ref).
    if label.is_empty() && alt.is_empty() {
        return None;
    }
    let ref_label = if label.is_empty() {
        raw_alt.to_string()
    } else {
        label
    };
    let mut attrs = None;
    let mut after = after_label;
    if bytes.get(after) == Some(&b'{') {
        if let Some((parsed_attrs, next)) = read_attrs_at(bytes, after, bounds.last_close_brace) {
            attrs = Some(parsed_attrs);
            after = next;
        }
    }
    let mut image = Image {
        attrs,
        src: String::new(),
        alt,
        title: None,
        ref_label: Some(ref_label),
        raw_ref: Some(std::str::from_utf8(&bytes[start..after]).ok()?.to_string()),
        resolved_empty_reference: false,
        pos: None,
    };
    resolve_active_image(&mut image);
    Some((image, after - start))
}

pub(in crate::parse) fn parse_inline_link_with_options(
    bytes: &[u8],
    start: usize,
    options: &Options<'_>,
    in_footnote: bool,
    bounds: &InlineBounds<'_>,
    positions: Option<&InlinePositionMap<'_>>,
    base: usize,
) -> Option<(Link, usize)> {
    if bytes.get(start) != Some(&b'[') {
        return None;
    }
    let text_close = bracketed_close(bytes, start, bounds.matches)?;
    let after_bracket = text_close + 1;
    if bytes.get(after_bracket) != Some(&b'(') {
        return None;
    }
    let (href, title, after_paren) =
        read_link_target(bytes, after_bracket + 1, bounds.last_close_paren)?;
    // The label is copied only once a valid `(target)` follows; the failing
    // `[...]()` path (empty target) returns above without allocating, which is
    // what keeps `[[[...x]()]()...]()` linear instead of quadratic.
    let text = std::str::from_utf8(&bytes[start + 1..text_close])
        .ok()?
        .to_string();
    let mut attrs = None;
    let mut after = after_paren;
    if bytes.get(after) == Some(&b'{') {
        if let Some((parsed_attrs, next)) = read_attrs_at(bytes, after, bounds.last_close_brace) {
            attrs = Some(parsed_attrs);
            after = next;
        }
    }
    Some((
        Link {
            attrs,
            href,
            title,
            children: parse_inline_context(
                &text,
                options,
                false,
                in_footnote,
                positions,
                base + start + 1,
            ),
            ref_label: None,
            raw_ref: None,
            resolved_empty_reference: false,
            from_crossref: false,
            from_heading_reference: false,
            pos: None,
        },
        after - start,
    ))
}

pub(in crate::parse) fn parse_inline_extension(
    bytes: &[u8],
    start: usize,
    options: &Options<'_>,
    in_footnote: bool,
    bounds: &InlineBounds<'_>,
    positions: Option<&InlinePositionMap<'_>>,
    base: usize,
) -> Option<(InlineExtension, usize)> {
    if bytes.get(start) != Some(&b':') {
        return None;
    }
    // The content runs to the first `]`; with no `]` ahead the scan could only
    // walk to end-of-text and fail, so bail in O(1) (keeps `:a[`×n linear).
    if !bounds.has_bracket_from(start) {
        return None;
    }
    let mut i = start + 1;
    let name_start = i;
    // `extension_name = identifier`: must start with a letter or `_` -- a
    // digit-first name (`:1[x]`) is invalid and the whole construct stays
    // literal. (`:a1[x]` is fine; digits are allowed after the first char.)
    match bytes.get(i) {
        Some(b) if b.is_ascii_alphabetic() || *b == b'_' => {}
        _ => return None,
    }
    while i < bytes.len()
        && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'-' || bytes[i] == b'_')
    {
        i += 1;
    }
    if i == name_start || bytes.get(i) != Some(&b'[') {
        return None;
    }
    let name = std::str::from_utf8(&bytes[name_start..i]).ok()?.to_string();
    // `extension_content = {character - ']'}`: the content runs to the FIRST
    // unescaped `]` and does not balance nested brackets (`:foo[a [b] c]` ->
    // `<span class="ext-foo">a [b</span> c]`).
    let (content, after_bracket) = read_to_first_bracket(bytes, i)?;
    let mut attrs = None;
    let mut after = after_bracket;
    if bytes.get(after) == Some(&b'{') {
        if let Some((parsed_attrs, next)) = read_attrs_at(bytes, after, bounds.last_close_brace) {
            attrs = Some(parsed_attrs);
            after = next;
        }
    }
    Some((
        InlineExtension {
            attrs,
            name,
            children: parse_inline_context(
                &content,
                options,
                false,
                in_footnote,
                positions,
                base + i + 1,
            ),
            pos: None,
        },
        after - start,
    ))
}

pub(in crate::parse) fn parse_span(
    bytes: &[u8],
    start: usize,
    options: &Options<'_>,
    in_footnote: bool,
    bounds: &InlineBounds<'_>,
    positions: Option<&InlinePositionMap<'_>>,
    base: usize,
) -> Option<(Span, usize)> {
    let content_close = bracketed_close(bytes, start, bounds.matches)?;
    let after_bracket = content_close + 1;
    if bytes.get(after_bracket) != Some(&b'{') {
        return None;
    }
    let (attrs, after_attrs) = read_attrs_at(bytes, after_bracket, bounds.last_close_brace)
        .or_else(|| read_empty_attrs_at(bytes, after_bracket))?;
    // Absorb a CHAIN of adjacent attribute blocks (`[x]{.a}{.b}` ->
    // class="a b"), accumulating classes (§15). A non-attribute `{...}` (e.g.
    // an empty `{}`) reads as None and is left literal, so `[x]{}{}` keeps the
    // trailing `{}` -- matching carve-php / carve-js.
    let mut attrs = Some(attrs);
    let mut slots = None;
    let mut after_attrs = after_attrs;
    while bytes.get(after_attrs) == Some(&b'{') {
        match read_attrs_at(bytes, after_attrs, bounds.last_close_brace) {
            Some((more, next)) => {
                merge_attrs_indexed(&mut attrs, more, &mut slots);
                after_attrs = next;
            }
            None => break,
        }
    }
    // Only a valid `{attrs}` follow reaches here, so the content copy stays off
    // the failing `[...]` path (e.g. `[...]()` never gets past the `{` check).
    let content = std::str::from_utf8(&bytes[start + 1..content_close])
        .ok()?
        .to_string();
    Some((
        Span {
            attrs,
            injected: false,
            children: parse_inline_context(
                &content,
                options,
                false,
                in_footnote,
                positions,
                base + start + 1,
            ),
            pos: None,
        },
        after_attrs - start,
    ))
}

#[cfg(test)]
mod literal_run_tests {
    use super::*;

    #[test]
    fn right_arrow_stays_a_smart_punctuation_node() {
        let nodes = parse_inline_with_options("Step 1 -> done...", &Options::default());
        assert!(nodes.iter().any(|node| matches!(
            node,
            InlineNode::SmartPunctuation(punctuation) if punctuation.kind == "rightwards_arrow"
        )));
        assert!(nodes.iter().any(|node| matches!(
            node,
            InlineNode::SmartPunctuation(punctuation) if punctuation.kind == "ellipsis"
        )));
    }

    #[test]
    fn punctuation_and_unicode_keep_source_positions() {
        let source = "Café term: text! carve#529 a@b (c) (tm) --flag --> -> ....";
        let document = parse_with_options(source, &Options::default().with_positions(true));
        let BlockNode::Paragraph(paragraph) = &document.children[0] else {
            panic!("expected a paragraph");
        };
        assert!(paragraph.children.iter().any(|node| matches!(
            node,
            InlineNode::Text(text) if text.value.contains("--flag")
        )));
        assert!(paragraph.children.iter().any(|node| matches!(
            node,
            InlineNode::Text(text) if text.value.contains("carve#529 a@b")
        )));
        let kinds: Vec<&str> = paragraph
            .children
            .iter()
            .filter_map(|node| match node {
                InlineNode::SmartPunctuation(punctuation) => {
                    let pos = punctuation
                        .pos
                        .as_ref()
                        .expect("smart punctuation position");
                    let authored: String = source
                        .chars()
                        .skip(pos.start_offset)
                        .take(pos.end_offset - pos.start_offset)
                        .collect();
                    assert_eq!(authored, punctuation.value);
                    Some(punctuation.kind.as_str())
                }
                _ => None,
            })
            .collect();
        assert_eq!(
            kinds,
            [
                "copyright",
                "trademark",
                "rightwards_arrow",
                "rightwards_arrow",
                "ellipsis"
            ]
        );
    }

    #[test]
    fn construct_openers_still_leave_the_literal_run() {
        let options = Options::default();
        assert!(parse_inline_with_options("a != b", &options)
            .iter()
            .any(|node| matches!(node, InlineNode::SmartPunctuation(p) if p.kind == "not_equal")));
        assert!(parse_inline_with_options("x!`lit`", &options)
            .iter()
            .any(|node| matches!(node, InlineNode::LiteralInline(_))));
        assert!(parse_inline_with_options("x ![a](b)", &options)
            .iter()
            .any(|node| matches!(node, InlineNode::Image(_))));
        assert!(parse_inline_with_options("x #tag", &options)
            .iter()
            .any(|node| matches!(node, InlineNode::Tag(_))));
        assert!(parse_inline_with_options("x @user", &options)
            .iter()
            .any(|node| matches!(node, InlineNode::Mention(_))));
        assert!(parse_caption_inline_with_options("Figure#", &options, true)
            .iter()
            .any(|node| matches!(node, InlineNode::CaptionNumber(_))));
    }

    #[test]
    fn paired_asterisks_join_literal_prose_without_hiding_strong_markup() {
        let options = Options::default();
        let nodes = parse_inline_with_options("a **Core syntax** b", &options);
        assert!(
            matches!(nodes.as_slice(), [InlineNode::Text(text)] if text.value == "a **Core syntax** b")
        );

        let nodes = parse_inline_with_options("*x**", &options);
        assert!(
            matches!(nodes.first(), Some(InlineNode::Emphasis(emphasis)) if emphasis.kind == EmphasisKind::Strong)
        );
        assert!(matches!(nodes.last(), Some(InlineNode::Text(text)) if text.value == "*"));
    }
}
