use super::*;

/// What an escape decision needs that one text node cannot say.
///
/// Both facts here are about what SURROUNDS the node: whether it sits inside a
/// note's content, and what the node after it writes. A text node holds neither,
/// and `boundary_text` cannot supply the second - it reports a code span's
/// CONTENT while the span writes a backtick ahead of it.
#[derive(Clone, Copy)]
pub(super) struct NeighbourEscape {
    /// Inside an inline note's content, where PART 9 §16 disables note
    /// recognition at every depth.
    pub(super) in_note_content: bool,
    /// The `[` arrived as the NEXT node's boundary character, and that node
    /// opens a note with it.
    pub(super) next_node_opens_a_note: bool,
    /// The next node writes [`render_code`]'s backtick fence at byte zero, so a
    /// `$`, `$$` or `!` this node ends on binds to it.
    pub(super) next_node_opens_a_verbatim_span: bool,
    /// The next node writes a `[` at byte zero, so a trailing `:name` would
    /// open an inline extension with it.
    pub(super) next_node_opens_a_bracket: bool,
}

/// Whether the `^` before the `[` at `bracket` needs its escape.
///
/// PART 11 §2 escapes a character IF AND ONLY IF omitting the escape would
/// change the re-parsed AST, and it takes the decision per OPENER OCCURRENCE.
/// `^[` is only an occurrence where the note can FORM, and PART 9 §16 gives two
/// shapes where it cannot: an empty or whitespace-only body is literal, and note
/// recognition is DISABLED inside a note's own content, at every depth. Escaping
/// either one is the over-escaping §2 calls a defect rather than a safe default.
///
/// Three of the four answers are certain and are taken here. The fourth is not:
/// the run does not close in THIS text node, and a later node may still supply
/// the `]` - or may not, which is the `x ^[a` that needs nothing. That one is
/// handed to the minimal/conservative vote (§4) rather than guessed, which is
/// what the `mode` argument is for: the two passes then differ, W3 parses both,
/// and the bare form is emitted exactly when it re-parses the same.
pub(super) fn caret_needs_its_escape(
    text: &str,
    bracket: usize,
    note: NeighbourEscape,
    mode: EscapeMode,
) -> bool {
    if note.in_note_content {
        return false;
    }
    let run = match text.get(bracket..) {
        Some(rest) if rest.starts_with('[') => rest,
        // The `[` is the next NODE's, so the run is not this node's to weigh -
        // and it is not even certain to follow the caret in the output, since a
        // node reporting a boundary character emits its own opener ahead of it.
        // `next_node_opens_a_note` is that question, answered where the node was
        // in hand.
        _ => return note.next_node_opens_a_note,
    };
    match crate::parse::bracketed_run_body(run) {
        Some(body) => !body.trim().is_empty(),
        None => mode == EscapeMode::Conservative,
    }
}

/// The characters a node writes VERBATIM at the very front of its output.
///
/// A different question from [`boundary_text`], which answers "what character is
/// adjacent" for the emphasis and comment-spacing decisions and is happy to name
/// one the node does not write first: a code span reports its content while
/// writing a backtick ahead of it, escaped text reports the character while
/// writing a backslash, and a mention, a tag and a symbol all write their sigil
/// first. Only these three put their own value at byte zero.
///
/// The nodes that open with a bare `[` are [`leading_bracket_run`]'s.
pub(super) fn leading_verbatim_text(node: &InlineNode) -> Option<&str> {
    match node {
        InlineNode::Text(text) => Some(&text.value),
        InlineNode::SmartPunctuation(punctuation) => Some(&punctuation.value),
        InlineNode::Abbreviation(abbr) => Some(&abbr.abbr),
        _ => None,
    }
}

pub(super) fn note_braced_span(
    node: &InlineNode,
    rendered: &mut String,
    braced_before: usize,
    emphasis_before: [usize; 7],
    ctx: &mut CarveContext,
) {
    let (delimiters, pos): (&[char], _) = match node {
        InlineNode::Emphasis(emphasis) => {
            (emphasis_delimiters(emphasis.kind), emphasis.pos.as_ref())
        }
        InlineNode::CriticInsert(insert) if rendered.starts_with("{+") => {
            (&['+'], insert.pos.as_ref())
        }
        InlineNode::CriticDelete(delete) if rendered.starts_with("{-") => {
            (&['-'], delete.pos.as_ref())
        }
        _ => return,
    };
    if let InlineNode::Emphasis(emphasis) = node {
        let repeated = delimiters.iter().any(|marker| {
            let index = "/*_~^,=".find(*marker).unwrap();
            ctx.open_kinds.contains(marker) || ctx.emphasis_counts[index] > emphasis_before[index]
        });
        if repeated {
            if let Some(delim) = bare_delimiter(emphasis.kind) {
                if rendered.starts_with(delim) {
                    let attrs = render_attrs_with_markers(
                        &emphasis.attrs,
                        &ctx.open_kinds,
                        ctx.attribute_bracket_depth > 0,
                    );
                    let body = &rendered[..rendered.len() - attrs.len()];
                    *rendered = format!("{{{body}}}{attrs}");
                }
            }
        }
        for marker in delimiters {
            let index = "/*_~^,=".find(*marker).unwrap();
            ctx.emphasis_counts[index] += 1;
        }
    } else {
        let nested: Vec<Option<usize>> = ctx.braced_spans[braced_before..]
            .iter()
            .filter(|(inner, _)| delimiters.contains(inner))
            .map(|(_, mark)| *mark)
            .collect();
        if !nested.is_empty() {
            crate::render_carve_error::record_unspellable(
                "emphasis",
                "a span inside a span of the same kind has no Carve source spelling",
            );
            crate::render_carve_error::record_nested_same_kind(
                nested.into_iter().flatten().collect(),
            );
        }
    }
    if rendered.starts_with('{') {
        ctx.braced_spans.truncate(braced_before);
    }
    for delimiter in delimiters {
        ctx.braced_spans
            .push((*delimiter, pos.map(|pos| pos.start_offset)));
    }
}

pub(super) fn emphasis_delimiters(kind: EmphasisKind) -> &'static [char] {
    match kind {
        EmphasisKind::Italic => &['/'],
        EmphasisKind::Strong => &['*'],
        EmphasisKind::Underline => &['_'],
        EmphasisKind::Strike => &['~'],
        EmphasisKind::Highlight => &['='],
        EmphasisKind::Super => &['^'],
        EmphasisKind::Sub => &[','],
        EmphasisKind::BoldItalic => &['/', '*'],
        // No delimiter, because no source spelling: the writer flattens it.
        EmphasisKind::SmallCaps => &[],
    }
}

/// The offset of a `:` that, with the name after it, ends `text` as an inline
/// extension opener short only of the next node's `[` (markup-carve/carve#2068).
pub(super) fn trailing_extension_colon(text: &str) -> Option<usize> {
    let name_start = text
        .trim_end_matches(|c: char| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        .len();
    let colon = name_start.checked_sub(1)?;
    let first = text[name_start..].chars().next()?;
    (text.as_bytes()[colon] == b':' && (first.is_ascii_alphabetic() || first == '_'))
        .then_some(colon)
}

/// The bracket run a node writes at byte zero, with a stand-in label that is
/// blank exactly when the written label is: `^[](u)` opens no note, `^[n](u)`
/// does (markup-carve/carve-rs#1710).
pub(super) fn leading_bracket_run(node: &InlineNode) -> Option<Cow<'_, str>> {
    fn label(children: &[InlineNode]) -> Cow<'static, str> {
        let blank = children
            .iter()
            .all(|child| matches!(child, InlineNode::Text(t) if t.value.trim().is_empty()));
        Cow::Borrowed(if blank { "[]" } else { "[x]" })
    }
    match node {
        InlineNode::Link(link) if link.ref_label.is_some() && link.raw_ref.is_some() => {
            link.raw_ref.as_deref().map(Cow::Borrowed)
        }
        InlineNode::Link(link) if link.from_crossref && link.href.starts_with('#') => None,
        InlineNode::Link(link) => Some(label(&link.children)),
        InlineNode::Span(span) => Some(label(&span.children)),
        InlineNode::Footnote(note) if note.inline.is_none() => Some(Cow::Borrowed("[^x]")),
        InlineNode::CitationGroup(group) => Some(Cow::Borrowed(&group.raw)),
        _ => None,
    }
}

/// Does the node FOLLOWING a text node begin, in the output, with the BACKTICK
/// FENCE a `$`, `$$` or `!` sigil would bind to?
///
/// Asked of the node rather than of [`boundary_text`], which reports a code
/// span's CONTENT and so cannot answer it: `a $` before a code span holding
/// `x+y` sees `x` as its neighbour character while the output puts a backtick
/// there.
///
/// A code span and a raw inline are the two nodes that write [`render_code`]'s
/// fence at byte zero. Math and an inline literal write their own sigil first
/// (`$` and `!`), so the fence is not adjacent to the text at all.
pub(super) fn next_node_opens_a_verbatim_span(next: Option<&InlineNode>) -> bool {
    matches!(
        next,
        Some(InlineNode::Code(_)) | Some(InlineNode::RawInline(_))
    )
}

/// Does the node FOLLOWING a text node begin, in the output, with a bracket run
/// that opens a note?
///
/// This is what stops ``x ^`[t]` `` coming back ``x \^`[t]` ``: the code span
/// reports a `[` as the adjacent character but writes a backtick, so the caret
/// is not in front of a bracket in the output at all.
pub(super) fn next_node_opens_a_note(
    next: Option<&InlineNode>,
    in_note_content: bool,
    mode: EscapeMode,
) -> bool {
    let text = match next {
        Some(node) => leading_verbatim_text(node)
            .map(Cow::Borrowed)
            .or_else(|| leading_bracket_run(node)),
        None => None,
    };
    match text {
        Some(text) => caret_needs_its_escape(
            &text,
            0,
            NeighbourEscape {
                in_note_content,
                next_node_opens_a_note: false,
                next_node_opens_a_verbatim_span: false,
                next_node_opens_a_bracket: false,
            },
            mode,
        ),
        None => false,
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn escape_text(
    session: &RenderSession,
    text: &str,
    bracket_role: &dyn Fn(usize) -> BracketRole,
    paired_closer_carry: &std::cell::Cell<bool>,
    mode: EscapeMode,
    unit: usize,
    opens_block_line: bool,
    caption_can_open: bool,
    in_table_cell: bool,
    previous_boundary: char,
    next_boundary: char,
    note: NeighbourEscape,
) -> String {
    let verbatim_sigil_at = note
        .next_node_opens_a_verbatim_span
        .then(|| {
            let trimmed = text.trim_end_matches('$');
            if trimmed.len() < text.len() {
                return Some(trimmed.len());
            }
            text.strip_suffix('!').map(str::len)
        })
        .flatten();
    let extension_colon_at = note
        .next_node_opens_a_bracket
        .then(|| trailing_extension_colon(text))
        .flatten();
    let mut out = String::new();
    // PART 11 §2's decision is taken per OPENER OCCURRENCE, so every candidate
    // site in this run gets an index the search can address it by
    // (markup-carve/carve#1533).
    let call = next_escape_call_index(session, unit);
    let mut at_line_start = opens_block_line;
    let mut chars = text.char_indices().peekable();
    let mut previous = previous_boundary;
    let mut bracket_ordinal = 0;
    let mut after_paired_closer = paired_closer_carry.get();
    let crossrefs = crate::parse::CrossrefScan::default();
    while let Some((offset, ch)) = chars.next() {
        if ch == '<' {
            if let Some(end) = crossrefs.end(text.as_bytes(), offset).filter(|&end| {
                !session
                    .crossref_targets
                    .borrow()
                    .contains(&text[offset + 3..end - 1])
            }) {
                out.push_str(&text[offset..end]);
                while chars.peek().is_some_and(|&(at, _)| at < end) {
                    if matches!(chars.next(), Some((_, '[' | ']'))) {
                        bracket_ordinal += 1;
                    }
                }
                previous = '>';
                at_line_start = false;
                after_paired_closer = false;
                continue;
            }
        }
        // A CONTROL CHARACTER IS CONTENT, and the writer has to write it back.
        // This dropped 61 codepoints - every C0 control but tab/newline/return,
        // DEL, and the whole C1 block - none of which the parser or the HTML
        // renderer drops, so `to_html(fmt(x)) == to_html(x)` failed on any
        // document holding one. PART 2 keeps a FORM FEED and a VERTICAL TAB
        // explicitly (carve#926), and corpus
        // `261-a-blank-line-holds-spaces-and-tabs-and-nothing-else-3` pins a
        // line holding one as CONTENT rather than as a blank.
        //
        // U+0000 stays dropped, and only it: `normalize_source` removes it
        // before the parser sees it, so keeping it here would write back a byte
        // no re-parse can read. Every other control survives the round trip
        // because it survives the parse.
        //
        // This is not the Trojan-Source hardening, which is a different set in
        // a different place: `escape::is_bidi_control` strips the bidi
        // overrides and isolates (U+202A-E, U+2066-9), none of which are in the
        // range this line held.
        if ch == '\u{0000}' {
            continue;
        }
        // The caption marker is `^` followed by a SPACE. `^sup^` at the start
        // of a line is not one - superscript is braced-only, so it is literal
        // text and needs no escape, which two of this repo's own tests already
        // pinned.
        let next = chars.peek().map(|&(_, c)| c).unwrap_or(next_boundary);
        // SPACE ONLY, which is what the comment above already said and what the
        // code did not do. A tab after the marker leaves the line as prose -
        // corpus
        // `231-a-tab-after-a-heading-quote-or-caption-marker-leaves-the-line-as-prose-2`
        // is that document - so `^<TAB>` re-parses as text either way and PART 11
        // §4 asks for the minimal form when dropping the escape changes nothing.
        let caret_opens_a_caption = ch == '^' && at_line_start && caption_can_open && next == ' ';
        let caret_opens_inline = ch == '^'
            && (next == '[' || (chars.peek().is_none() && note.next_node_opens_a_note))
            && caret_needs_its_escape(text, offset + ch.len_utf8(), note, mode);
        let colon_cannot_open =
            ch == ':' && !at_line_start && !symbol_opens_at(text, offset, previous);
        at_line_start = ch == '\n';
        let opens_a_verbatim_construct = verbatim_sigil_at.is_some_and(|start| offset >= start);
        let role = if matches!(ch, '[' | ']') {
            bracket_ordinal += 1;
            bracket_role(bracket_ordinal - 1)
        } else {
            BracketRole::default()
        };
        let opens_a_destination = ch == '('
            && after_paired_closer
            && crate::parse::opens_inline_link_target(&text[offset..]);
        let unconditional = role.lone
            || role.crossing_opener
            || role.crossing_closer
            || opens_a_destination
            || matches!(ch, '\\' | '`' | '"' | '\'')
            || extension_colon_at == Some(offset)
            || caret_opens_a_caption
            || caret_opens_inline
            || opens_a_verbatim_construct;
        let caret_is_a_span_marker = ch == '^' && in_table_cell;
        // A comment opens on the first TWO unescaped percent signs of a run, so
        // the escape on the first one already makes the whole run literal:
        // `\%%c`, and `\%%%c` for a longer run. A percent repeating the one
        // before it is therefore never a site, which keeps it out of the run
        // rule below that would otherwise hand it its predecessor's escape and
        // write `\%\%c` (carve-rs#2443). The shared escaper corpus pins the
        // one-escape spelling and both peer engines write it.
        let continues_percent_run = ch == '%' && offset > 0 && previous == '%';
        let candidate = !continues_percent_run
            && (caret_is_a_span_marker
                || matches!(
                    ch,
                    '*' | '_'
                        | '{'
                        | '}'
                        | '['
                        | ']'
                        | '('
                        | ')'
                        | '#'
                        | '+'
                        | '-'
                        | '.'
                        | '!'
                        | '~'
                        | '/'
                        | '<'
                        | '>'
                        | '@'
                        | '%'
                        | '|'
                        | '='
                        | ':'
                        | ';'
                        | '^'
                ));
        // In a unit the search has escalated, each candidate site is offered
        // back on its own, so the one occurrence that needed the escape no
        // longer drags the rest of the unit with it (PART 11 §2). A character
        // whose own guard already decided it -- the caret, a sigil binding to a
        // verbatim run, the unconditional set -- is not a candidate and is
        // never offered.
        let escaped = if let (']', Some(opener)) = (ch, role.opener) {
            // A paired closer follows its opener, which may sit in another unit
            // -- unless the second reading already owes this `]` an escape of
            // its own. Bracketed content keeps `closer_openers`, so without
            // this the mirror answered for every closer and the crossing escape
            // never reached one inside a link label or a span
            // (carve-rs#2215).
            let escaped = role.crossing_closer
                || session
                    .escaped_openers
                    .with(|cell| cell.borrow().get(&opener).copied().unwrap_or(false));
            session
                .last_occurrence_relaxed
                .with(|cell| cell.set(!escaped));
            escaped
        } else {
            let offered = mode == EscapeMode::Conservative && candidate && !unconditional;
            let relaxed = offered
                && occurrence_is_relaxed(
                    session,
                    (unit, call, offset),
                    offset > 0 && previous == ch,
                );
            unconditional || (offered && !relaxed && !colon_cannot_open)
        };
        if ch == '[' {
            session
                .escaped_openers
                .with(|cell| cell.borrow_mut().insert(role.key, escaped));
        }
        if escaped {
            out.push('\\');
        }
        out.push(ch);
        previous = ch;
        after_paired_closer = ch == ']' && !escaped && role.paired_closer;
    }
    paired_closer_carry.set(after_paired_closer);
    out
}

/// How PART 11 §5 reads one text bracket.
#[derive(Default, Clone, Copy)]
pub(super) struct BracketRole {
    pub(super) lone: bool,
    /// The `[` of a pair that crosses a formatting boundary.
    pub(super) crossing_opener: bool,
    /// The `]` of such a pair, where an outer opener survives to answer it.
    pub(super) crossing_closer: bool,
    pub(super) paired_closer: bool,
    /// This bracket, as the scope keys it.
    pub(super) key: (usize, usize),
    /// The `[` a paired `]` in bracketed content closes.
    pub(super) opener: Option<(usize, usize)>,
}

/// Whether the `:` at `offset` opens a symbol shortcode.
///
/// MIRRORS [`crate::parse::parse_symbol`] and is deliberately not a second
/// reading of the same production: the opener's preceding-character test, the
/// first name character's narrower class (`_` is excluded so `:_x_:` cannot
/// steal from underline) and the closing colon are the parser's, so a writer
/// that escapes here and a parser that opens there cannot drift apart.
///
/// `previous` is the character before the run, which is what carries the
/// preceding-character test across a node boundary - the text node this is
/// called on may begin mid-line.
pub(super) fn symbol_opens_at(text: &str, offset: usize, previous: char) -> bool {
    let bytes = text.as_bytes();
    if bytes.get(offset) != Some(&b':') {
        return false;
    }
    let prev = if offset == 0 {
        previous
    } else {
        // A multi-byte character before the colon is not ASCII alphanumeric and
        // is not `_`, so the boundary answer is the same either way.
        text[..offset].chars().next_back().unwrap_or(previous)
    };
    if prev.is_ascii_alphanumeric() || prev == '_' {
        return false;
    }
    let Some(&first) = bytes.get(offset + 1) else {
        return false;
    };
    if !first.is_ascii_alphanumeric() && first != b'+' && first != b'-' {
        return false;
    }
    let mut len = 1;
    while let Some(&b) = bytes.get(offset + 1 + len) {
        if b.is_ascii_alphanumeric() || b == b'_' || b == b'+' || b == b'-' {
            len += 1;
        } else {
            break;
        }
    }
    bytes.get(offset + 1 + len) == Some(&b':')
}

pub(super) fn escape_plain_line(text: &str) -> String {
    text.replace('\n', " ")
}

/// An image's ALT TEXT, written between `![` and `]`.
pub(crate) fn escape_image_alt(text: &str, in_table: bool) -> String {
    let escaped = if crate::parse::raw_bracket_run_closes(text)
        && crate::parse::unescape_title(text) == text
        && !text.split('\n').skip(1).any(|line| {
            let text = line.trim_start_matches([' ', '\t']);
            text.starts_with("```")
                || text.strip_prefix('[').is_some_and(|label| {
                    label
                        .find(']')
                        .is_some_and(|end| end > 0 && label[end + 1..].starts_with(':'))
                })
        }) {
        text.to_string()
    } else {
        text.replace('\\', "\\\\")
            .replace('[', "\\[")
            .replace(']', "\\]")
            .replace('`', "\\`")
    };
    let escaped = if in_table {
        escaped.replace('|', "\\|")
    } else {
        escaped
    };
    static LINES: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    static ORDERED: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let lines = LINES.get_or_init(|| regex::Regex::new(r"(?m)(\n[ \t]*)(#{1,6}(?:[ \t]|$)|>(?:[ \t]|$)|[-*](?:[ \t]|[-*]{2})|:{1,3}(?:[ \t]|:|$)|`{3,}|~{3,}|\||\^(?:[ \t]|$)|\{|%%|\[[^\]\n]+\]:)").unwrap());
    let ordered = ORDERED.get_or_init(|| {
        regex::Regex::new(r"(\n[ \t]*(?:[0-9]+|[ivxlcdm]+|[IVXLCDM]+|[a-z]|[A-Z]|))([.)])([ \t])")
            .unwrap()
    });
    let guarded = lines.replace_all(&escaped, "$1\\$2");
    ordered
        .replace_all(&guarded, |caps: &regex::Captures<'_>| {
            if &caps[2] == ")" && caps[1].trim_matches(['\n', ' ', '\t']).is_empty() {
                caps[0].to_string()
            } else {
                format!("{}\\{}{}", &caps[1], &caps[2], &caps[3])
            }
        })
        .into_owned()
}

/// Which characters the destination scan would read differently if emitted
/// bare: a parenthesis with no partner, and a backslash sitting in front of one
/// of the three escapable characters. Balanced parentheses are deliberately
/// absent -- they re-parse as themselves, and escaping them would be churn
/// against the minimal-escaping rule in PART 11 section 4.
pub(super) fn unbalanced_destination_chars(text: &str) -> std::collections::HashSet<usize> {
    let mut openers: Vec<usize> = Vec::new();
    let mut marked = std::collections::HashSet::new();
    for (i, ch) in text.char_indices() {
        if ch == '(' {
            openers.push(i);
        } else if ch == ')' && openers.pop().is_none() {
            marked.insert(i);
        }
    }
    marked.extend(openers);
    marked
}

/// Backslash-escape exactly what the destination scan would otherwise read
/// differently: a parenthesis with no partner, and a backslash sitting in front
/// of one of the three escapable characters. A balanced pair re-parses as
/// itself, so leaving it bare is the minimal escaping PART 11 section 4 asks
/// for.
pub(super) fn escape_destination_escapes(text: &str) -> String {
    let needs_marking = text
        .as_bytes()
        .iter()
        .any(|&b| matches!(b, b'(' | b')' | b'\\'));
    if !needs_marking {
        return text.to_string();
    }
    let marked = unbalanced_destination_chars(text);
    let bytes = text.as_bytes();
    let mut out = String::new();
    for (i, ch) in text.char_indices() {
        let escapable = ch == '\\'
            && (i + 1 == bytes.len()
                || matches!(bytes.get(i + 1), Some(b'(') | Some(b')') | Some(b'\\')));
        if marked.contains(&i) || escapable {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}

pub(crate) fn escape_destination(text: &str) -> String {
    // Almost every destination holds neither a parenthesis nor a backslash, so
    // there is nothing for the scan to misread and nothing to mark. Skipping
    // the walk keeps that case free of the set entirely.
    let needs_marking = text
        .as_bytes()
        .iter()
        .any(|&b| matches!(b, b'(' | b')' | b'\\'));
    let marked = if needs_marking {
        unbalanced_destination_chars(text)
    } else {
        std::collections::HashSet::new()
    };
    let bytes = text.as_bytes();
    let mut out = String::new();
    for (i, ch) in text.char_indices() {
        let escapable = ch == '\\'
            && (i + 1 == bytes.len()
                || matches!(bytes.get(i + 1), Some(b'(') | Some(b')') | Some(b'\\')));
        if marked.contains(&i) || escapable {
            out.push('\\');
        }
        match ch {
            // Whitespace is percent-encoded (it would end the destination
            // otherwise). A backslash before anything the scan does not treat
            // as an escape is emitted verbatim, so URLs carrying backslashes
            // need no doubling.
            ch if ch.is_whitespace() => {
                if ch == ' ' {
                    out.push_str("%20");
                } else {
                    out.push_str(&format!("%{:02X}", ch as u32));
                }
            }
            _ => out.push(ch),
        }
    }
    out
}

/// A link, an image and a `[ref]: url "t"` definition, whose `link_title` reads
/// escapes back.
pub(super) fn escape_quoted(text: &str) -> String {
    escape_quoted_run(text, &['"'])
}

/// Escape inside a quoted slot only what a bare spelling would lose.
///
/// `unescape_title` resolves `\\X` only when X is ASCII punctuation, so a
/// backslash before anything else reads back literally and needs no partner -
/// PART 11 §2 escapes a character only if omitting it would change the
/// re-parse. A TRAILING backslash still doubles: the character after it is the
/// closing delimiter, which is punctuation, so a bare one would swallow it.
pub(super) fn escape_quoted_run(text: &str, escaped: &[char]) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            if chars
                .peek()
                .map_or(true, |next| next.is_ascii_punctuation())
            {
                out.push('\\');
            }
            out.push('\\');
            continue;
        }
        if escaped.contains(&ch) {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}

/// NOT the same rule: `quoted_title` takes `{character - '"'}` verbatim and has
/// no escape mechanism, so escaping here changes the title instead of
/// preserving it (carve-rs#1946). That leaves a `"` unspellable rather than
/// escapable - markup-carve/carve-php#2375's reading of the same slot - and
/// writing it as `\"` would emit source no conforming parser reads back, so the
/// writer refuses instead.
pub(super) fn escape_quoted_title(text: &str, node_type: &'static str) -> String {
    if text.contains('"') {
        crate::render_carve_error::record_unspellable(node_type, QUOTE_IN_QUOTED_TITLE);
    }
    text.replace('"', "")
}

pub(super) const QUOTE_IN_QUOTED_TITLE: &str =
    "a double quote has no spelling inside a quoted title";
