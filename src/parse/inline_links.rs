use super::*;

pub(in crate::parse) fn skip_link_title_padding(bytes: &[u8], mut at: usize) -> usize {
    while bytes.get(at) == Some(&b' ') {
        at += 1;
    }
    at
}

pub(in crate::parse) fn read_empty_attrs_at(bytes: &[u8], start: usize) -> Option<(Attrs, usize)> {
    if bytes.get(start) != Some(&b'{') {
        return None;
    }
    let mut i = start + 1;
    // ONLY SPACES between the braces. The BLESSED EMPTY BLOCK is a separate
    // position rather than a use of the separator, and it is the one most
    // likely to be missed: narrowing the separator alone leaves `[x]{<TAB>}` a
    // valid empty block, and the corpus document that pins it stays green
    // (PART 4 THE INLINE INTERIOR IS SPACE-ONLY, carve#906; the executable spec
    // needed two edits for exactly this reason).
    //
    // A newline was already excluded here, for the neighbouring reason: it is
    // not a single-line inline attribute block, so `[x]{` + newline + `}` stays
    // literal.
    while bytes.get(i) == Some(&b' ') {
        i += 1;
    }
    if bytes.get(i) == Some(&b'}') {
        Some((Attrs::default(), i + 1))
    } else {
        None
    }
}

/// Length of a mention/tag name = name_word ('.' name_word)*, where
/// name_word = (letter | digit | '_' | '-')+ (grammar PART 9 §7). A `.` is
/// INTERIOR only -- it must sit between two name_words, so `a..b` yields `a`
/// (the run stops before the doubled dot) and `markus.` yields `markus`.
pub(crate) fn name_run_len(s: &str) -> usize {
    let b = s.as_bytes();
    let is_word = |c: u8| c.is_ascii_alphanumeric() || c == b'_' || c == b'-';
    let mut i = 0;
    while i < b.len() && is_word(b[i]) {
        i += 1;
    }
    if i == 0 {
        return 0;
    }
    while b.get(i) == Some(&b'.') && b.get(i + 1).is_some_and(|&c| is_word(c)) {
        i += 1; // the interior dot
        while i < b.len() && is_word(b[i]) {
            i += 1;
        }
    }
    i
}

pub(in crate::parse) fn parse_mention(text: &str, pos: usize) -> Option<(Mention, usize)> {
    if pos > 0 {
        let prev = text.as_bytes()[pos - 1];
        if prev.is_ascii_alphanumeric() || prev == b'_' {
            return None;
        }
    }
    let rest = text.get(pos + 1..)?;
    let len = name_run_len(rest);
    if len == 0 {
        return None;
    }
    Some((
        Mention {
            attrs: None,
            user: rest[..len].to_string(),
            pos: None,
        },
        len + 1,
    ))
}

pub(in crate::parse) fn parse_tag(text: &str, pos: usize) -> Option<(Tag, usize)> {
    if pos > 0 {
        let prev = text.as_bytes()[pos - 1];
        if prev.is_ascii_alphanumeric() || prev == b'_' {
            return None;
        }
    }
    let rest = text.get(pos + 1..)?;
    let len = name_run_len(rest);
    if len == 0 {
        return None;
    }
    Some((
        Tag {
            attrs: None,
            name: rest[..len].to_string(),
            pos: None,
        },
        len + 1,
    ))
}

pub(in crate::parse) fn parse_symbol(
    text: &str,
    pos: usize,
    bounds: &InlineBounds<'_>,
) -> Option<(Symbol, usize)> {
    let bytes = text.as_bytes();
    if bytes.get(pos) != Some(&b':') {
        return None;
    }
    if pos > 0 {
        let prev = bytes[pos - 1];
        if prev.is_ascii_alphanumeric() || prev == b'_' {
            return None;
        }
    }
    // The first name char is a letter, digit, `+` or `-` (so the reaction
    // shortcodes `:+1:` / `:-1:` parse), but never `_`: `:_x_:` would steal
    // from underline. Scanning the symbol at the opening `:` also gives it
    // precedence over smart typography, so `:+-:` is the symbol `+-`, not a
    // `±` between colons (grammar PART 9 §7).
    let first = *bytes.get(pos + 1)?;
    if !first.is_ascii_alphanumeric() && first != b'+' && first != b'-' {
        return None;
    }
    let mut len = 1;
    while let Some(&b) = bytes.get(pos + 1 + len) {
        if b.is_ascii_alphanumeric() || b == b'_' || b == b'+' || b == b'-' {
            len += 1;
        } else {
            break;
        }
    }
    let close_pos = pos + 1 + len;
    if bytes.get(close_pos) != Some(&b':') {
        return None;
    }
    let (attrs, consumed) =
        if let Some((attrs, next)) = read_attrs_at(bytes, close_pos + 1, bounds.last_close_brace) {
            (Some(attrs), next - pos)
        } else {
            (None, len + 2)
        };
    Some((
        Symbol {
            name: text[pos + 1..close_pos].to_string(),
            attrs,
            pos: None,
        },
        consumed,
    ))
}

pub(in crate::parse) fn parse_autolink(
    text: &str,
    pos: usize,
    bounds: &InlineBounds<'_>,
) -> Option<(AutoLink, usize)> {
    // The target runs to the closing `>`; with no `>` ahead the scan could only
    // walk to end-of-text and fail, so bail in O(1) (keeps `<`×n linear).
    if !bounds.has_gt_from(pos) {
        return None;
    }
    let rest = text.get(pos..)?;
    let close = autolink_closer(rest.as_bytes(), 0)?;
    let target = &rest[1..close];
    let href = autolink_href(target)?;
    let mut attrs = None;
    let mut consumed = close + 1;
    let bytes = text.as_bytes();
    if bytes.get(pos + consumed) == Some(&b'{') {
        if let Some((parsed_attrs, next)) =
            read_attrs_at(bytes, pos + consumed, bounds.last_close_brace)
        {
            attrs = Some(parsed_attrs);
            consumed = next - pos;
        }
    }
    Some((
        AutoLink {
            attrs,
            href,
            text: target.to_string(),
            pos: None,
        },
        consumed,
    ))
}

/// `email_char = letter | digit | '.' | '-' | '_' | '+'` (grammar.ebnf).
/// Note `:` is deliberately NOT an email char.
pub(in crate::parse) fn is_email_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_' | b'+')
}

pub(crate) fn autolink_href(target: &str) -> Option<String> {
    if is_url_autolink_target(target) {
        Some(target.to_string())
    } else if is_email_autolink_target(target) {
        Some(format!("mailto:{target}"))
    } else {
        None
    }
}

/// `email_autolink = {email_char}+ '@' {email_char}+ '.' {letter}+`.
/// The local part and domain are both non-empty runs of `email_char`, and the
/// domain MUST end in `.` followed by a TLD of one or more ASCII letters. So
/// `<a@b>` (no TLD) and `<x@y:z>` (`:` is not an email_char) stay literal,
/// while `<a@b.com>` is a `mailto:` link.
pub(in crate::parse) fn is_email_autolink_target(target: &str) -> bool {
    let bytes = target.as_bytes();
    let Some(at) = bytes.iter().position(|&b| b == b'@') else {
        return false;
    };
    let local = &bytes[..at];
    let domain = &bytes[at + 1..];
    if local.is_empty() || domain.is_empty() {
        return false;
    }
    if !local.iter().all(|&b| is_email_char(b)) || !domain.iter().all(|&b| is_email_char(b)) {
        return false;
    }
    // A single `@` only: `@` is not an email_char, so any later `@` already
    // failed the `is_email_char` check above.
    // Domain must end in `.` + TLD ({letter}+).
    let Some(dot) = domain.iter().rposition(|&b| b == b'.') else {
        return false;
    };
    let tld = &domain[dot + 1..];
    if dot == 0 {
        // No host label before the final dot.
        return false;
    }
    !tld.is_empty() && tld.iter().all(|&b| b.is_ascii_alphabetic())
}

pub(in crate::parse) fn is_url_autolink_target(target: &str) -> bool {
    let Some((scheme, url)) = target.split_once(':') else {
        return false;
    };
    let Some(first) = scheme.bytes().next() else {
        return false;
    };
    if url.is_empty() || !first.is_ascii_alphabetic() {
        return false;
    }
    scheme
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'-' | b'.'))
        && url.chars().all(is_url_char)
}

/// PART 3 `url_char`, the autolink body's character class.
///
/// Inside ASCII it is the enumerated set and nothing else, so `"`, `\`, `` ` ``,
/// `{`, `}`, `|`, `^`, `<` and `>` still break an autolink. Outside ASCII the
/// clause AN AUTOLINK BODY ADMITS NON-ASCII AND EXCLUDES FORMAT CHARACTERS
/// reads `unicode_url_char - format_char - control_char` (carve#844,
/// carve#860): an internationalized domain, an accented host and a non-ASCII
/// path are autolinks, because the same destination written
/// `[t](https://<IDN>/)` already links through `link_destination` and one
/// destination cannot answer differently on the character set depending on the
/// spelling.
///
/// The CONTROL term is not redundant with the ASCII enumeration, which is the
/// trap it is written around: `unicode_url_char` is "non-whitespace, non-ASCII",
/// so the C1 block U+0080-U+009F satisfies it - those are Cc, are not Cf, and
/// only U+0085 is whitespace. Without the term, fourteen control characters
/// would be `url_char`s while every C0 control is excluded.
///
/// `link_destination` is a DIFFERENT production and is unchanged: a format
/// character in an inline destination or a reference definition is still an
/// ordinary destination character.
pub(in crate::parse) fn is_url_char(c: char) -> bool {
    if c.is_ascii() {
        return is_url_autolink_char(c as u8);
    }
    // `char::is_whitespace` is the White_Space property and `char::is_control`
    // is General_Category=Cc - the properties themselves, not a host language's
    // `\s` shorthand. JavaScript's `\s` matches U+FEFF and misses U+0085, and
    // PCRE's Unicode `\s` matches U+180E, which Unicode 6.3.0 removed from
    // White_Space; spelling either half that way decides part of this rule by
    // accident (carve-php#957).
    !c.is_whitespace() && !c.is_control() && !is_format_char(c)
}

/// General_Category=Cf: 170 codepoints in 21 ranges (Unicode 15.0.0).
///
/// A format character is invisible by definition, so a host carrying one
/// renders as the host without it and links somewhere else. That is a spoofing
/// surface rather than an authoring convenience, which is why the class is
/// excluded rather than the individual zero-width characters anyone has thought
/// to name. The whole property is pinned by codepoint in
/// `tests/autolink_url_char_classes.rs`, since 170 corpus documents stating one
/// rule is not a corpus.
pub(in crate::parse) fn is_format_char(c: char) -> bool {
    matches!(
        c,
        '\u{00AD}'
            | '\u{0600}'..='\u{0605}'
            | '\u{061C}'
            | '\u{06DD}'
            | '\u{070F}'
            | '\u{0890}'..='\u{0891}'
            | '\u{08E2}'
            | '\u{180E}'
            | '\u{200B}'..='\u{200F}'
            | '\u{202A}'..='\u{202E}'
            | '\u{2060}'..='\u{2064}'
            | '\u{2066}'..='\u{206F}'
            | '\u{FEFF}'
            | '\u{FFF9}'..='\u{FFFB}'
            | '\u{110BD}'
            | '\u{110CD}'
            | '\u{13430}'..='\u{1343F}'
            | '\u{1BCA0}'..='\u{1BCA3}'
            | '\u{1D173}'..='\u{1D17A}'
            | '\u{E0001}'
            | '\u{E0020}'..='\u{E007F}'
    )
}

pub(in crate::parse) fn is_url_autolink_char(b: u8) -> bool {
    b.is_ascii_alphanumeric()
        || matches!(
            b,
            b'-' | b'.'
                | b'_'
                | b'~'
                | b':'
                | b'/'
                | b'?'
                | b'#'
                | b'['
                | b']'
                | b'@'
                | b'!'
                | b'$'
                | b'&'
                | b'\''
                | b'('
                | b')'
                | b'*'
                | b'+'
                | b','
                | b';'
                | b'='
                | b'%'
        )
}

pub(in crate::parse) fn parse_crossref(
    text: &str,
    pos: usize,
    bounds: &InlineBounds<'_>,
) -> Option<(CrossRef, usize)> {
    let close = crossref_close(text.as_bytes(), pos, bounds)?;
    let target = text.get(pos + 3..close)?;
    Some((
        CrossRef {
            target: target.to_string(),
            // Filled by `fill_crossref_hrefs` once the whole document exists:
            // resolution needs the heading table, which is not built yet here.
            href: None,
            pos: None,
        },
        close + 1 - pos,
    ))
}

/// The `>` closing the cross-reference `</#id>` at `pos`, or `None`.
pub(in crate::parse) fn crossref_close(
    bytes: &[u8],
    pos: usize,
    bounds: &InlineBounds<'_>,
) -> Option<usize> {
    // The target runs to the closing `>`; with no `>` ahead the scan could only
    // walk to end-of-text and fail, so bail in O(1) (keeps `</#`×n linear).
    if !bounds.has_gt_from(pos) || !bytes.get(pos..)?.starts_with(b"</#") {
        return None;
    }
    let start = pos + 3;
    if bounds
        .crossref_invalid_range
        .get()
        .is_some_and(|(from, to)| start >= from && start <= to)
    {
        return None;
    }
    let close = bytes[start..]
        .iter()
        .position(|&b| matches!(b, b'>' | b' ' | b'\r' | b'\n'))?;
    if bytes[start + close] != b'>' {
        bounds
            .crossref_invalid_range
            .set(Some((start, start + close)));
        return None;
    }
    (close > 0).then_some(start + close)
}

/// O(1) lookup of the matching `]` for the `[` at `start` using a precomputed
/// match table (see `compute_bracket_matches`). `start` must index a `[`.
/// Returns the byte index of the closing `]` (a borrow position, no allocation).
///
/// Callers materialize the bracket label only after the follow (target `(`,
/// reference `[`, span `{`) validates, so a `[` whose construct never completes
/// stays O(1) instead of eagerly copying its label at every position -- the
/// difference between linear and quadratic parsing on pathological input like
/// `[[[...x]()]()...]()`.
pub(in crate::parse) fn bracketed_close(
    bytes: &[u8],
    start: usize,
    matches: &[usize],
) -> Option<usize> {
    if bytes.get(start) != Some(&b'[') {
        return None;
    }
    let close = *matches.get(start)?;
    if close == NO_BRACKET_MATCH {
        return None;
    }
    Some(close)
}

/// Find the first structural `]` in a reference label. An opening `[` is label
/// content rather than a nested delimiter, while escapes and opaque spans keep
/// their closing brackets literal.
pub(in crate::parse) fn reference_label_close(bytes: &[u8], start: usize) -> Option<usize> {
    if bytes.get(start) != Some(&b'[') {
        return None;
    }
    let mut i = start + 1;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' if i + 1 < bytes.len() => i += 2,
            b'`' => i = skip_code_span(bytes, i)?,
            b'{' if skip_editorial_comment(bytes, i).is_some() => {
                i = skip_editorial_comment(bytes, i)?;
            }
            b']' => {
                return (!bytes[start + 1..i]
                    .iter()
                    .any(|byte| matches!(byte, b'\n' | b'\r')))
                .then_some(i);
            }
            _ => i += 1,
        }
    }
    None
}

pub(crate) fn reference_label_has_definition_spelling(label: &str) -> bool {
    let parsed = crate::parse(&format!("[{label}]: /carrier-validation\n"));
    matches!(parsed.children.as_slice(), [BlockNode::LinkReferenceDefinition(def)] if def.label == label)
}

pub(crate) fn reference_label_has_source_spelling(label: &str) -> bool {
    let run = format!("[{label}]");
    reference_label_has_definition_spelling(label)
        && !label.contains("{#")
        && !label.contains('[')
        && !label.contains('`')
        && reference_label_close(run.as_bytes(), 0) == Some(run.len() - 1)
}

/// Read `[...]` content for an inline extension: the content runs to the
/// FIRST `]` and does NOT balance nested brackets or honor escapes
/// (`extension_content = {character - ']'}`, carve-js regex `\[([^\]]*)\]`).
/// Returns the content and the index just past the closing `]`.
pub(in crate::parse) fn read_to_first_bracket(
    bytes: &[u8],
    start: usize,
) -> Option<(String, usize)> {
    if bytes.get(start) != Some(&b'[') {
        return None;
    }
    let content_start = start + 1;
    let mut i = content_start;
    while i < bytes.len() {
        if bytes[i] == b']' {
            let text = std::str::from_utf8(&bytes[content_start..i])
                .ok()?
                .to_string();
            return Some((text, i + 1));
        }
        i += 1;
    }
    None
}

/// The body of the bracketed run `text` opens with, if the run closes inside it.
///
/// The canonical writer asks this before deciding whether a `^` it is about to
/// emit opens an inline note, so the question has to be answered by the READER
/// rather than by a second scan that promises to agree with it: the close is
/// bracket-balanced, escape-aware and opaque inside a verbatim span or an
/// editorial comment, and [`read_bracketed`] is where all of that already lives.
///
/// `None` when the run does not close in `text` - which for the writer means the
/// closer may still arrive from a later node, so it must not drop the escape.
pub(crate) fn bracketed_run_body(text: &str) -> Option<String> {
    read_bracketed(text.as_bytes(), 0).map(|(body, _)| body)
}

/// Whether the reader closes a bracketed run at its final delimiter.
/// This checks syntax; callers separately check escape decoding.
pub(crate) fn raw_bracket_run_closes(text: &str) -> bool {
    let wrapped = format!("[{text}]");
    read_bracketed(wrapped.as_bytes(), 0).is_some_and(|(_, after)| after == wrapped.len())
}

/// The end (exclusive) of a complete destination slot opening at `open`, read
/// after a `]` that closes a NESTED run: the grammar's `nested` and
/// `altNested` take `"[" ... "]" destSlot?`, so the slot is content of the run
/// around it and a `]` inside it closes nothing (CARVE-P3-001). The scan is
/// charged to `budget`; once it runs out no slot is skipped, which keeps a
/// chain of nested slots linear.
pub(in crate::parse) fn nested_destination_slot_end(
    bytes: &[u8],
    open: usize,
    budget: &mut usize,
) -> Option<usize> {
    if bytes.get(open) != Some(&b'(') || *budget == 0 {
        return None;
    }
    let limit = bytes
        .len()
        .min(open.saturating_add(*budget).saturating_add(1));
    match link_destination_end(&bytes[..limit], open) {
        Some(close) => {
            *budget = budget.saturating_sub(close + 1 - open);
            Some(close + 1)
        }
        None => {
            *budget = if limit < bytes.len() {
                0
            } else {
                budget.saturating_sub(limit - open)
            };
            None
        }
    }
}

pub(in crate::parse) fn read_bracketed(bytes: &[u8], start: usize) -> Option<(String, usize)> {
    if bytes.get(start) != Some(&b'[') {
        return None;
    }
    let mut i = start + 1;
    let content_start = i;
    let mut depth = 0usize;
    // An image alt is a literal slot, so a `{%` there opens no comment.
    let in_alt = start > 0 && bytes[start - 1] == b'!';
    let last_percent_close = bytes.windows(2).rposition(|w| w == b"%}");
    let mut slot_budget = BRACKET_RESCAN_BUDGET * bytes.len() + 64;
    while i < bytes.len() {
        match bytes[i] {
            b'{' if !in_alt
                && bytes.get(i + 1) == Some(&b'%')
                && !decorated_braced_opener(bytes, i)
                && last_percent_close.is_some_and(|close| close >= i + 2) =>
            {
                i = find_seq(bytes, i + 2, b"%}")? + 2;
            }
            b'\\' if i + 1 < bytes.len() => i += 2,
            b'`' => {
                // An unclosed verbatim span is opaque to end of text, so no `]`
                // after it can close the bracket: the construct is not balanced.
                i = skip_code_span(bytes, i)?;
            }
            b'{' if skip_editorial_comment(bytes, i).is_some() => {
                i = skip_editorial_comment(bytes, i)?;
            }
            b'<' if scanned_crossref_end(bytes, i).is_some() => {
                i = scanned_crossref_end(bytes, i)?;
            }
            b'<' if scanned_autolink_end(bytes, i).is_some() => {
                i = scanned_autolink_end(bytes, i)? + 1;
            }
            b'[' => {
                depth += 1;
                i += 1;
            }
            b']' if depth > 0 => {
                depth -= 1;
                i = nested_destination_slot_end(bytes, i + 1, &mut slot_budget).unwrap_or(i + 1);
            }
            b']' => {
                let text = std::str::from_utf8(&bytes[content_start..i])
                    .ok()?
                    .to_string();
                return Some((text, i + 1));
            }
            _ => i += 1,
        }
    }
    None
}

/// Skip an editorial comment opening at `start` (`{#`), returning the index just
/// past its `#}`.
///
/// Its content is LITERAL (PART 9 `editorial_comment`), so a `]` inside it is
/// text and cannot be the close of a link label - and no escape can say so
/// either, because `{# ... #}` resolves none. Returns None when there is no
/// closer, in which case it is not a comment and the scan continues normally
/// (carve#403).
pub(in crate::parse) fn skip_editorial_comment(bytes: &[u8], start: usize) -> Option<usize> {
    if bytes.get(start) != Some(&b'{')
        || bytes.get(start + 1) != Some(&b'#')
        || decorated_braced_opener(bytes, start)
    {
        return None;
    }
    let mut i = start + 2;
    while i + 1 < bytes.len() {
        if bytes[i] == b'#' && bytes[i + 1] == b'}' {
            return Some(i + 2);
        }
        i += 1;
    }
    None
}

/// The byte after the `>` of the cross-reference `</#id>` at `open`, for the
/// bracket scans, which skip its body (CARVE-P3-001).
pub(in crate::parse) fn scanned_crossref_end(bytes: &[u8], open: usize) -> Option<usize> {
    if !bytes.get(open..)?.starts_with(b"</#") || bytes.get(open + 3) == Some(&b'>') {
        return None;
    }
    let start = open + 3;
    let close = bytes[start..]
        .iter()
        .position(|&b| matches!(b, b'>' | b' ' | b'\r' | b'\n'))?;
    (bytes[start + close] == b'>').then_some(start + close + 1)
}

/// [`scanned_crossref_end`] for a one-pass scan: every `</#` inside one id run
/// ends at the same byte, so a run of unclosed openers is read once.
#[derive(Default)]
pub(crate) struct CrossrefScan {
    run: Cell<Option<(usize, usize)>>,
}

impl CrossrefScan {
    pub(crate) fn end(&self, bytes: &[u8], open: usize) -> Option<usize> {
        if !bytes.get(open..)?.starts_with(b"</#") || bytes.get(open + 3) == Some(&b'>') {
            return None;
        }
        let start = open + 3;
        let stop = match self.run.get() {
            Some((from, stop)) if start >= from && start <= stop => stop,
            _ => {
                let stop = bytes[start..]
                    .iter()
                    .position(|&b| matches!(b, b'>' | b' ' | b'\r' | b'\n'))
                    .map_or(bytes.len(), |at| start + at);
                self.run.set(Some((start, stop)));
                stop
            }
        };
        (bytes.get(stop) == Some(&b'>')).then_some(stop + 1)
    }
}

/// Sentinel in a bracket-match table meaning "this `[` has no matching `]`".
pub(in crate::parse) const NO_BRACKET_MATCH: usize = usize::MAX;

/// How many bytes the per-bracket rescans in [`compute_bracket_matches`] may
/// walk in one text, as a multiple of its length. A DoS guard: past it the
/// bracket stays unmatched, as in carve-js.
pub(in crate::parse) const BRACKET_RESCAN_BUDGET: usize = 8;

/// The start of the last maximal backtick run of each length.
pub(in crate::parse) fn last_backtick_run_starts(bytes: &[u8]) -> HashMap<usize, usize> {
    let mut last = HashMap::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'`' {
            i += 1;
            continue;
        }
        let start = i;
        while i < bytes.len() && bytes[i] == b'`' {
            i += 1;
        }
        last.insert(i - start, start);
    }
    last
}

/// Locate structural brackets in a self-contained verbatim run.
pub(crate) fn structural_bracket_offsets(source: &str) -> Option<Vec<usize>> {
    if source
        .lines()
        .any(|line| line.trim_start_matches([' ', '\t']).starts_with("%%"))
    {
        return None;
    }
    let bytes = source.as_bytes();
    let mut offsets = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' if i + 1 < bytes.len() => i += 2,
            b'`' => {
                i = skip_code_span(bytes, i)?;
            }
            b'{' if skip_editorial_comment(bytes, i).is_some() => {
                i = skip_editorial_comment(bytes, i).unwrap();
            }
            b'<' if scanned_crossref_end(bytes, i).is_some() => {
                i = scanned_crossref_end(bytes, i).unwrap();
            }
            b'<' if scanned_autolink_end(bytes, i).is_some() => {
                i = scanned_autolink_end(bytes, i).unwrap() + 1;
            }
            b'{' if matches!(bytes.get(i + 1), Some(b'#' | b'%'))
                && !decorated_braced_opener(bytes, i) =>
            {
                return None
            }
            b'[' | b']' => {
                offsets.push(i);
                i += 1;
            }
            _ => i += 1,
        }
    }
    Some(offsets)
}

/// Whether `at` opens an image's `(dest)` or `[ref]` tail (`destSlot`, `refTail`).
fn image_tail(bytes: &[u8], at: usize) -> bool {
    match bytes.get(at) {
        Some(b'(') => {
            let body = bytes[at + 1..].iter().take_while(|&&b| b == b' ').count();
            bytes.get(at + 1 + body) == Some(&b')') || link_destination_end(bytes, at).is_some()
        }
        Some(b'[') => {
            bytes[at + 1..]
                .iter()
                .find(|&&b| matches!(b, b']' | b'\n' | b'\r'))
                == Some(&b']')
        }
        _ => false,
    }
}

/// Each `[^` whose label (up to the first `]` on its line) holds a `[`, as
/// (open, close).
pub(in crate::parse) fn footnote_labels_holding_bracket(bytes: &[u8]) -> Vec<(usize, usize)> {
    let mut found = Vec::new();
    // Shared forward cursors keep the scan linear; `bytes.len()` means none ahead.
    let (mut stop, mut bracket) = (0, 0);
    let mut at = 0;
    while let Some(open) = find_seq(bytes, at, b"[^") {
        at = open + 1;
        let escapes = bytes[..open]
            .iter()
            .rev()
            .take_while(|&&b| b == b'\\')
            .count();
        if escapes % 2 == 1 {
            continue;
        }
        if stop < open + 2 {
            stop = bytes[open + 2..]
                .iter()
                .position(|&b| matches!(b, b']' | b'\n' | b'\r'))
                .map_or(bytes.len(), |len| open + 2 + len);
        }
        if stop == bytes.len() {
            break;
        }
        if bracket < open + 2 {
            bracket = bytes[open + 2..]
                .iter()
                .position(|&b| b == b'[')
                .map_or(bytes.len(), |len| open + 2 + len);
        }
        if stop > open + 2 && bytes[stop] == b']' && bracket < stop {
            found.push((open, stop));
        }
    }
    found
}

/// Precompute the matching `]` index for every `[` in `bytes`, giving the
/// answer `read_bracketed` gives scanning from that `[` (CARVE-P3-001): backslash
/// escapes skip two bytes, and a verbatim span, editorial comment or autolink
/// hides its brackets.
///
/// One pass answers every `[` outside a closed verbatim span. A backtick run
/// with no closer ends only the labels opened before it, so a `[` after a run
/// an earlier construct used up still closes (carve-rs#1733). A `[` inside a
/// closed span is reached only when something else used the span's opening run
/// up, so it is rescanned from itself under a shared budget. Linear either way,
/// which keeps nested links (`[[[...x]()]()...]`) off the O(n^2) path.
///
/// Entry `i` is meaningful only when `bytes[i] == b'['`; it holds the matching
/// `]` index, or `NO_BRACKET_MATCH` when that `[` never closes.
pub(in crate::parse) fn compute_bracket_matches(bytes: &[u8]) -> (Vec<usize>, Vec<usize>) {
    compute_bracket_runs(bytes, true)
}

/// [`compute_bracket_matches`] for a slot that may or may not read `[^`
/// references; an inline note's body does not (SS16).
pub(in crate::parse) fn compute_bracket_runs(
    bytes: &[u8],
    footnote_refs: bool,
) -> (Vec<usize>, Vec<usize>) {
    let mut matches = vec![NO_BRACKET_MATCH; bytes.len()];
    // For each position, the `[` of the INNERMOST bracket run holding it, or
    // `NO_BRACKET_MATCH` outside every run. Filled by the same pass that pairs
    // the brackets, so it hides exactly what that pass hides: a `[` inside a
    // closed verbatim span or an editorial comment opens no run, and neither does
    // an escaped one. Four scans need the answer - the inline loop's comment
    // extent, the emphasis closing scan, the combined `/*` scan and the braced
    // pair - and a stack per scan is four chances to hide a different construct.
    let mut run_at = vec![NO_BRACKET_MATCH; bytes.len() + 1];
    let last_run_start = if bytes.contains(&b'`') {
        last_backtick_run_starts(bytes)
    } else {
        HashMap::new()
    };
    let last_comment_close = bytes.windows(2).rposition(|w| w == b"#}");
    let closed_span_end = |at: usize| -> (usize, Option<usize>) {
        let len = bytes[at..].iter().take_while(|&&b| b == b'`').count();
        let closes = last_run_start
            .get(&len)
            .is_some_and(|&last| last >= at + len);
        (
            len,
            if closes {
                skip_code_span(bytes, at)
            } else {
                None
            },
        )
    };
    let comment_end = |at: usize| -> Option<usize> {
        if last_comment_close.is_some_and(|close| close >= at + 2) {
            skip_editorial_comment(bytes, at)
        } else {
            None
        }
    };
    // A delimited comment hides its brackets too, except in an image alt, a
    // literal slot where `{%` opens nothing (CARVE-P3-001).
    let last_percent_close = bytes.windows(2).rposition(|w| w == b"%}");
    let delimited_end = |at: usize, in_alt: bool| -> Option<usize> {
        if in_alt
            || bytes.get(at + 1) != Some(&b'%')
            || decorated_braced_opener(bytes, at)
            || !last_percent_close.is_some_and(|close| close >= at + 2)
        {
            return None;
        }
        find_seq(bytes, at + 2, b"%}").map(|close| close + 2)
    };
    let crossref_end = CrossrefScan::default();
    // A `[^` reference is read before bracket scanning (PART 8), so a `[` in its
    // label opens no run. Every run's content is read again as inline text, so
    // that holds inside a run as well, while each run keeps the extent plain
    // nesting gives it (`nested`). The second pass reads the references the
    // plain pass reached as atomic, and a `]` that plain nesting paired with a
    // `[` in a label closes nothing.
    let footnote_candidates: HashMap<usize, usize> = if footnote_refs {
        footnote_labels_holding_bracket(bytes).into_iter().collect()
    } else {
        HashMap::new()
    };
    let mut reached: Vec<(usize, usize)> = Vec::new();
    let mut atomic: HashMap<usize, usize> = HashMap::new();
    let mut freed: HashSet<usize> = HashSet::new();
    let mut spans: Vec<(usize, usize)>;
    let mut passes = 0;
    loop {
        // Closed verbatim spans the pass skipped, as (content start, end).
        spans = Vec::new();
        let mut stack: Vec<usize> = Vec::new();
        // Per open bracket, whether it is (inside) an image alt.
        let mut alt: Vec<bool> = Vec::new();
        let mut last_bang = usize::MAX;
        let mut slot_budget = BRACKET_RESCAN_BUDGET * bytes.len() + 64;
        let mut i = 0;
        while i < bytes.len() {
            let in_alt = alt.last().copied().unwrap_or(false);
            match bytes[i] {
                b'\\' if i + 1 < bytes.len() => {
                    fill_run(&mut run_at, &stack, i, i + 2);
                    i += 2;
                }
                b'`' => match closed_span_end(i) {
                    (len, Some(end)) => {
                        spans.push((i + len, end));
                        fill_run(&mut run_at, &stack, i, end);
                        i = end;
                    }
                    (len, None) => {
                        fill_run(&mut run_at, &stack, i, i + len);
                        stack.clear();
                        alt.clear();
                        i += len;
                    }
                },
                b'{' if comment_end(i).is_some() => {
                    let end = comment_end(i).unwrap_or(i + 1);
                    fill_run(&mut run_at, &stack, i, end);
                    i = end;
                }
                b'{' if delimited_end(i, in_alt).is_some() => {
                    let end = delimited_end(i, in_alt).unwrap_or(i + 1);
                    fill_run(&mut run_at, &stack, i, end);
                    i = end;
                }
                b'<' if crossref_end.end(bytes, i).is_some() => {
                    let end = crossref_end.end(bytes, i).unwrap_or(i + 1);
                    fill_run(&mut run_at, &stack, i, end);
                    i = end;
                }
                b'<' if scanned_autolink_end(bytes, i).is_some() => {
                    let end = scanned_autolink_end(bytes, i).unwrap_or(i);
                    fill_run(&mut run_at, &stack, i, end + 1);
                    i = end + 1;
                }
                b'[' if atomic.contains_key(&i) => {
                    let close = atomic[&i];
                    run_at[i] = stack.last().copied().unwrap_or(NO_BRACKET_MATCH);
                    run_at[close] = run_at[i];
                    fill_run(&mut run_at[..close], &[i], i + 1, close);
                    matches[i] = close;
                    i = close + 1;
                }
                b']' if freed.contains(&i) => {
                    run_at[i] = stack.last().copied().unwrap_or(NO_BRACKET_MATCH);
                    i += 1;
                }
                b'[' => {
                    if let Some(&close) = footnote_candidates.get(&i) {
                        reached.push((i, close));
                    }
                    run_at[i] = stack.last().copied().unwrap_or(NO_BRACKET_MATCH);
                    stack.push(i);
                    alt.push(in_alt || (i > 0 && last_bang == i - 1));
                    i += 1;
                }
                b']' => {
                    if let Some(open) = stack.pop() {
                        matches[open] = i;
                    }
                    alt.pop();
                    run_at[i] = stack.last().copied().unwrap_or(NO_BRACKET_MATCH);
                    i += 1;
                    if !stack.is_empty() {
                        if let Some(end) = nested_destination_slot_end(bytes, i, &mut slot_budget) {
                            fill_run(&mut run_at, &stack, i, end);
                            i = end;
                        }
                    }
                }
                byte => {
                    if byte == b'!' {
                        last_bang = i;
                    }
                    run_at[i] = stack.last().copied().unwrap_or(NO_BRACKET_MATCH);
                    i += 1;
                }
            }
        }
        passes += 1;
        if passes == 2 || reached.is_empty() {
            break;
        }
        let mut label_end = None;
        for &(open, close) in &reached {
            // A `[^` inside an earlier label is label text, and `![` with a
            // tail reads as an image first.
            let image = open > 0
                && bytes[open - 1] == b'!'
                && bytes[..open - 1]
                    .iter()
                    .rev()
                    .take_while(|&&b| b == b'\\')
                    .count()
                    % 2
                    == 0
                && matches[open] != NO_BRACKET_MATCH
                && image_tail(bytes, matches[open] + 1);
            if image || label_end.is_some_and(|end| open <= end) {
                continue;
            }
            label_end = Some(close);
            atomic.insert(open, close);
            for at in open..close {
                if bytes[at] == b'[' && matches[at] != NO_BRACKET_MATCH && matches[at] > close {
                    freed.insert(matches[at]);
                }
            }
        }
        matches.fill(NO_BRACKET_MATCH);
        run_at.fill(NO_BRACKET_MATCH);
    }
    let mut budget = BRACKET_RESCAN_BUDGET * bytes.len() + 64;
    let mut scan_from = |open: usize| -> Option<usize> {
        let escapes = bytes[..open.saturating_sub(1)]
            .iter()
            .rev()
            .take_while(|&&b| b == b'\\')
            .count();
        let mut alt = vec![open > 0 && bytes[open - 1] == b'!' && escapes % 2 == 0];
        let mut bang = usize::MAX;
        let mut i = open + 1;
        while i < bytes.len() {
            budget = budget.checked_sub(1)?;
            let in_alt = alt.last().copied().unwrap_or(false);
            match bytes[i] {
                b'\\' if i + 1 < bytes.len() => i += 2,
                b'`' => {
                    let end = closed_span_end(i).1?;
                    budget = budget.checked_sub(end - i)?;
                    i = end;
                }
                b'{' if comment_end(i).is_some() => {
                    let end = comment_end(i)?;
                    budget = budget.checked_sub(end - i)?;
                    i = end;
                }
                b'{' if delimited_end(i, in_alt).is_some() => {
                    let end = delimited_end(i, in_alt)?;
                    budget = budget.checked_sub(end - i)?;
                    i = end;
                }
                b'<' if crossref_end.end(bytes, i).is_some() => {
                    let end = crossref_end.end(bytes, i)?;
                    budget = budget.checked_sub(end - i)?;
                    i = end;
                }
                b'<' if scanned_autolink_end(bytes, i).is_some() => {
                    let end = scanned_autolink_end(bytes, i)? + 1;
                    budget = budget.checked_sub(end - i)?;
                    i = end;
                }
                b'[' => {
                    alt.push(in_alt || (i > 0 && bang == i - 1));
                    i += 1;
                }
                b']' => {
                    alt.pop();
                    if alt.is_empty() {
                        return Some(i);
                    }
                    i += 1;
                    let mut slot_budget = budget;
                    if let Some(end) = nested_destination_slot_end(bytes, i, &mut slot_budget) {
                        i = end;
                    }
                    budget = slot_budget;
                }
                byte => {
                    if byte == b'!' {
                        bang = i;
                    }
                    i += 1;
                }
            }
        }
        None
    };
    for (content_start, end) in spans {
        for open in content_start..end {
            if bytes[open] == b'[' {
                if let Some(close) = scan_from(open) {
                    matches[open] = close;
                }
            }
        }
    }
    // A position past the last byte is outside every run, which is what the
    // `bytes.len()` slot already holds.
    (matches, run_at)
}

#[cfg(test)]
mod autolink_bracket_scan_tests {
    use super::*;

    #[test]
    fn an_autolink_body_is_opaque_to_the_bracket_scan() {
        let source = "[a <https://x]> b](u)";
        let (matches, _) = compute_bracket_matches(source.as_bytes());
        assert_eq!(matches[0], source.rfind(']').unwrap());
        assert_eq!(
            read_bracketed(b"[a <https://x]> b]", 0),
            Some(("a <https://x]> b".to_owned(), 18))
        );
        assert_eq!(
            crate::to_html(source).trim(),
            "<p><a href=\"u\">a https://x] b</a></p>"
        );
    }
}

/// Mark `[from, to)` as held by the run on top of `stack`, for the byte ranges the
/// pairing pass skips over in one step.
pub(in crate::parse) fn fill_run(run_at: &mut [usize], stack: &[usize], from: usize, to: usize) {
    let holder = stack.last().copied().unwrap_or(NO_BRACKET_MATCH);
    let end = to.min(run_at.len());
    for slot in &mut run_at[from.min(end)..end] {
        *slot = holder;
    }
}

/// The `[` each `]` closes, inverted from [`compute_bracket_matches`] once so
/// the closing scan can ask about a `]` in O(1).
pub(in crate::parse) fn compute_bracket_openers(matches: &[usize]) -> Vec<usize> {
    let mut openers = vec![NO_BRACKET_MATCH; matches.len()];
    for (open, &close) in matches.iter().enumerate() {
        if close != NO_BRACKET_MATCH {
            openers[close] = open;
        }
    }
    openers
}

/// Skip a verbatim (code) span opening at `start` (a backtick run). Returns the
/// index just past the equal-length closing run, or `None` when the span is
/// unclosed (opaque to end of text) — mirroring the reference bracket scanner.
pub(in crate::parse) fn skip_code_span(bytes: &[u8], start: usize) -> Option<usize> {
    let mut i = start;
    while i < bytes.len() && bytes[i] == b'`' {
        i += 1;
    }
    let open_len = i - start;
    while i < bytes.len() {
        if bytes[i] != b'`' {
            i += 1;
            continue;
        }
        let close_start = i;
        while i < bytes.len() && bytes[i] == b'`' {
            i += 1;
        }
        if i - close_start == open_len {
            return Some(i);
        }
    }
    None
}

/// Read `href[ "title"])` starting at `start` (just past the opening `(`).
/// Returns (href, optional title, index just past the closing `)`).
/// Resolve backslash escapes in a link/image title: `\X` becomes `X` when X is
/// ASCII punctuation (so `\"` is a literal quote), otherwise the backslash is
/// kept. Mirrors carve-js's unescapeAttrValue.
pub(crate) fn unescape_title(s: &str) -> String {
    let mut out = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(&next) = chars.peek() {
                if next.is_ascii_punctuation() {
                    out.push(next);
                    chars.next();
                    continue;
                }
            }
        }
        out.push(c);
    }
    out
}

/// Walk a destination that contains a parenthesis or a backslash, balancing the
/// parentheses and resolving the three escapes. Returns the destination and the
/// index the scan stopped at.
pub(in crate::parse) fn scan_balanced_destination(
    bytes: &[u8],
    start: usize,
) -> Option<(String, usize)> {
    let mut href_bytes: Vec<u8> = Vec::new();
    let mut depth: usize = 0;
    let mut i = start;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'\\' {
            if let Some(&next) = bytes.get(i + 1) {
                if matches!(next, b'(' | b')' | b'\\') {
                    href_bytes.push(next);
                    i += 2;
                    continue;
                }
            }
        }
        if b == b'(' {
            depth += 1;
        } else if b == b')' {
            if depth == 0 {
                break;
            }
            depth -= 1;
        } else if matches!(b, b' ' | b'\t' | b'\n') {
            break;
        }
        href_bytes.push(b);
        i += 1;
    }
    let href = String::from_utf8(href_bytes).ok()?;
    // Whitespace ends a destination only after every opener has balanced. A
    // title after an unclosed `(` must not rescue the partial prefix as an
    // href: `link_destination` has no bare-parenthesis alternative.
    if depth != 0 {
        return None;
    }
    // The byte loop above breaks on ASCII whitespace only. `unicode_url_char`
    // is "any non-whitespace, non-ASCII Unicode character" with no qualifier,
    // so a destination carrying a narrow no-break space is not a destination -
    // exactly as on the plain path.
    //
    // The plain path got this check and this one did not, which made the rule
    // depend on whether the URL happened to contain a PARENTHESIS: only a
    // destination with one reached here. `[x](<NBSP>https://e.com)` was
    // rejected while `[x](<NBSP>https://e.com/a(b))` linked with the invisible
    // character in the href, and `javascript:alert(1)` - parenthesised - slipped
    // through too, which is what made this look like a scheme-specific
    // divergence rather than a hole (carve#404, carve#407).
    if href.chars().any(char::is_whitespace) {
        return None;
    }
    Some((href, i))
}

/// A whole run read as `link_destination`: its value, or None when the run is
/// not one.
///
/// A reference definition is built from the same production as the inline tail,
/// so `[a]: a(b` is no more a destination than `[t](a(b)` is a link. Read by
/// `scan_balanced_destination` between a synthetic pair, which is the one place
/// the production is spelled: a run the scan does not consume whole left a
/// parenthesis or a whitespace character behind.
pub(in crate::parse) fn link_destination_value(run: &str) -> Option<String> {
    let probe = format!("({run})");
    let (href, end) = scan_balanced_destination(probe.as_bytes(), 1)?;
    if end != run.len() + 1 || href.is_empty() {
        return None;
    }
    Some(href)
}

/// Whether `text`, which starts at a `(`, opens an inline link destination
/// that closes: what a bare `]` directly before it would turn into a link.
///
/// Only the first `LINK_TARGET_PROBE_CAP` bytes are read, so a writer asking
/// once per `(` stays linear. A target that closes past the cap answers `false`,
/// which leaves the `(` to the escape search rather than escaping it.
pub(crate) fn opens_inline_link_target(text: &str) -> bool {
    const LINK_TARGET_PROBE_CAP: usize = 4096;
    let mut end = text.len().min(LINK_TARGET_PROBE_CAP);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    let bytes = &text.as_bytes()[..end];
    bytes.first() == Some(&b'(')
        && read_link_target(bytes, 1, bytes.iter().rposition(|&b| b == b')')).is_some()
}

pub(in crate::parse) fn read_link_target(
    bytes: &[u8],
    start: usize,
    last_close_paren: Option<usize>,
) -> Option<(String, Option<String>, usize)> {
    // A valid inline target MUST end with a `)` (checked below). If no `)`
    // occurs at or after `start`, the destination scan can only walk to
    // end-of-text and then fail -- so short-circuit here in O(1). Without this,
    // a run like `[a](`×n (no `)` anywhere) makes every `[` re-scan to EOF,
    // which is O(n^2). `last_close_paren` is the index of the last `)` in the
    // whole text, precomputed once by the caller; skipping only ever elides a
    // call that would have returned `None`, keeping output byte-identical.
    if last_close_paren.map_or(true, |p| start > p) {
        return None;
    }
    // Per the grammar, a destination's parentheses BALANCE: the scan ends at
    // whitespace, which begins a title, or at the first `)` with no opener left
    // to pair with. So a URL carrying a parenthesis -- Wikipedia and MDN
    // produce them constantly -- is written plainly. Djot and CommonMark both
    // balance the same way. The only escapes are an escaped parenthesis and an
    // escaped backslash, for the unbalanced case; a backslash before anything
    // else is an ordinary character, so URLs full of backslashes need no
    // doubling.
    //
    // Almost every destination holds none of those three characters, and that
    // run is a plain slice of the input. Finding it first keeps the common case
    // copy-free; only a run that actually contains one pays for the balancing
    // scan, which has to build its string byte by byte to drop the escapes.
    let mut plain_end = start;
    while plain_end < bytes.len()
        && !matches!(bytes[plain_end], b' ' | b'\t' | b'\n' | b'(' | b')' | b'\\')
    {
        plain_end += 1;
    }
    let (href, mut i) =
        if plain_end == bytes.len() || matches!(bytes[plain_end], b' ' | b'\t' | b'\n' | b')') {
            let plain = std::str::from_utf8(&bytes[start..plain_end]).ok()?;
            // The byte scan above stops at ASCII whitespace only, and
            // `unicode_url_char` is "any non-whitespace, non-ASCII Unicode
            // character" without a qualifier - so a destination carrying a
            // narrow no-break space is not a destination at all, and the link
            // does not form (carve#404).
            if plain.chars().any(char::is_whitespace) {
                return None;
            }
            (plain.to_string(), plain_end)
        } else {
            scan_balanced_destination(bytes, start)?
        };
    // Verse layout expands a tab or aligned gap into this internal marker.
    // It is spacing, so it cannot become a destination character.
    if href.contains(crate::NBSP_PLACEHOLDER) {
        return None;
    }
    let title_at = if bytes.get(i) == Some(&b' ') {
        i + 1
    } else {
        i
    };
    let title_follows = matches!(bytes.get(title_at), Some(&b'"') | Some(&b'\''));
    if title_follows {
        i = title_at;
    }
    let mut title: Option<String> = None;
    if title_follows {
        let quote = bytes[i];
        i += 1;
        let title_start = i;
        // A backslash escapes the next byte, so `\"` is a literal quote inside
        // the title rather than its terminator (matches carve-php / carve-js).
        while i < bytes.len() && bytes[i] != quote {
            if bytes[i] == b'\\' && i + 1 < bytes.len() {
                i += 2;
            } else {
                i += 1;
            }
        }
        if i >= bytes.len() {
            return None;
        }
        title = Some(unescape_title(
            std::str::from_utf8(&bytes[title_start..i]).ok()?,
        ));
        i += 1;
    }
    // ASCII spaces may follow a title with either destination. An empty
    // destination also takes spaces without a title.
    if href.is_empty() || title.is_some() {
        i = skip_link_title_padding(bytes, i);
    }
    if bytes.get(i) != Some(&b')') {
        return None;
    }
    Some((href, title, i + 1))
}

/// Carve's four whitespace characters (PART 7): a tab guards a bare delimiter
/// like a space, and a form feed or vertical tab is content.
pub(in crate::parse) fn is_carve_ws(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | b'\r')
}

pub(crate) fn followed_by_grapheme_extender(bytes: &[u8], at: usize) -> bool {
    if !bytes.get(at).is_some_and(|byte| !byte.is_ascii()) {
        return false;
    }
    let width = match bytes[at] {
        0xC0..=0xDF => 2,
        0xE0..=0xEF => 3,
        0xF0..=0xF7 => 4,
        _ => return false,
    };
    bytes
        .get(at..at + width)
        .and_then(|first| std::str::from_utf8(first).ok())
        .and_then(|first| first.chars().next())
        .is_some_and(|c| {
            let value = c as u32;
            grapheme_extender_ranges::EXTENDER
                .binary_search_by(|&(start, end)| {
                    if end < value {
                        std::cmp::Ordering::Less
                    } else if start > value {
                        std::cmp::Ordering::Greater
                    } else {
                        std::cmp::Ordering::Equal
                    }
                })
                .is_ok()
        })
}

#[cfg(test)]
mod grapheme_extender_tests {
    use super::followed_by_grapheme_extender;

    #[test]
    fn extender_matches_regex_for_every_unicode_scalar() {
        let regex = regex::Regex::new(r"\A[\p{Mn}\p{Mc}\p{Me}\u{FE00}-\u{FE0F}\u{200D}]").unwrap();
        for c in (0..=0x10ffff).filter_map(char::from_u32) {
            let mut buffer = [0; 4];
            let value = c.encode_utf8(&mut buffer);
            assert_eq!(
                followed_by_grapheme_extender(value.as_bytes(), 0),
                regex.is_match(value),
                "U+{:04X}",
                c as u32
            );
        }
    }
}

pub(in crate::parse) fn decorated_braced_opener(bytes: &[u8], open: usize) -> bool {
    bytes.get(open) == Some(&b'{')
        && bytes
            .get(open + 1)
            .is_some_and(|marker| b"/*_^,~=+-#%".contains(marker))
        && followed_by_grapheme_extender(bytes, open + 2)
}

#[allow(clippy::too_many_arguments)]
pub(in crate::parse) fn match_emphasis(
    bytes: &[u8],
    i: usize,
    options: &Options<'_>,
    in_footnote: bool,
    no_close: &mut EmphasisMemo,
    bounds: &InlineBounds<'_>,
    positions: Option<&InlinePositionMap<'_>>,
    base: usize,
    guard_open: bool,
) -> Option<(InlineNode, usize)> {
    let c = bytes[i];
    if i > 0 && bytes[i - 1] == b'{' {
        let unescaped_brace = |mark: usize| {
            let brace = mark - 1;
            let mut before = brace;
            while before > 0 && bytes[before - 1] == b'\\' {
                before -= 1;
            }
            (brace - before) % 2 == 0
        };
        if unescaped_brace(i)
            && cached_find_emphasis_close(bytes, i + 1, c, no_close, bounds).is_some_and(|close| {
                close > 0
                    && bytes[close - 1] == b'{'
                    && bytes.get(close + 1) == Some(&b'}')
                    && unescaped_brace(close)
            })
        {
            return None;
        }
    }

    // /*bold italic*/ -- a combined Strong>Emphasis span. The grammar
    // (`boldItalic = "/*" ~spaceOrEnd biInner+ "*/"`) requires the content to
    // start with a non-space char and be non-empty; carve-php additionally
    // rejects a closer whose content ends in whitespace, scanning on to a later
    // `*/`. Empty / space-bounded content is NOT bold-italic and falls through
    // to ordinary `/` emphasis below. The outer `/` also takes the bare
    // opener guard before the token and the bare closer guard after it.
    if c == b'/'
        && bytes.get(i + 1) == Some(&b'*')
        && !i
            .checked_sub(1)
            .and_then(|before| bytes.get(before))
            .is_some_and(|before| {
                before.is_ascii_alphanumeric() || (!guard_open && matches!(before, b'_' | b'/'))
            })
    {
        let start = i + 2;
        // Opener guard: the first content byte must exist and not be whitespace.
        if bytes.get(start).is_some_and(|b| !is_carve_ws(*b))
            && !followed_by_grapheme_extender(bytes, start)
        {
            if let Some(close) = bold_italic_close(bytes, start, no_close, bounds) {
                if NESTING_DEPTH.with(|depth| depth.get() >= MAX_NESTING_DEPTH) {
                    return Some((
                        InlineNode::text(
                            std::str::from_utf8(&bytes[i..close + 2]).ok()?.to_owned(),
                        ),
                        close + 2 - i,
                    ));
                }
                let inner = std::str::from_utf8(&bytes[start..close]).ok()?;
                OpenKinds::pass_on(
                    OpenKinds::bit(b'/') | OpenKinds::bit(b'*') | OpenKinds::COMBINED,
                );
                return Some((
                    InlineNode::Emphasis(Emphasis {
                        attrs: None,
                        kind: EmphasisKind::BoldItalic,
                        children: parse_inline_context(
                            inner,
                            options,
                            false,
                            in_footnote,
                            positions,
                            base + start,
                        ),
                        pos: None,
                    }),
                    close + 2 - i,
                ));
            }
        }
    }
    // Single-char delimiters. Highlight `=` is single-char like the rest; a
    // doubled `==` is therefore literal by same-delimiter adjacency (checked
    // below), exactly like `**x**`. There is NO bare `^`/`,` delimiter:
    // superscript and subscript exist only in the braced forms `{^x^}`/`{,x,}`
    // (grammar PART 9 §9 rationale note) -- a bare caret or comma is literal.
    let kind = match c {
        b'/' => EmphasisKind::Italic,
        b'*' => EmphasisKind::Strong,
        b'_' => EmphasisKind::Underline,
        b'~' => EmphasisKind::Strike,
        b'=' => EmphasisKind::Highlight,
        _ => return None,
    };
    let delim = c;
    // E3: an opener of a kind already open is content (markup-carve/carve#2078),
    // unless a bracket run holds it: the run bounds emphasis (E2b, carve#3897).
    if OpenKinds::is_open(delim) && !in_bracket_run(bytes, i, no_close, bounds) {
        return None;
    }
    // Opener: next char must exist and not be whitespace (CARVE-P3-013) or delim
    let after = bytes.get(i + 1).copied()?;
    if is_carve_ws(after) || after == delim || followed_by_grapheme_extender(bytes, i + 1) {
        return None;
    }
    // A `=` that is part of a multi-char smart-typography operator is consumed
    // by that operator, not as a highlight opener (grammar PART 8 / §8): it
    // begins `=>` or trails `<=` / `>=` / `!=`. (The spaced forms like `x <= y`
    // already fail the opener test -- their `=` is followed by whitespace -- but
    // compact forms like `a <=b` would otherwise open a stray `<mark>`.)
    if delim == b'=' {
        if after == b'>' {
            return None;
        }
        if i > 0 && matches!(bytes[i - 1], b'<' | b'>' | b'!') {
            // The `>` that closes an autolink or cross-reference is atomic to
            // it, so it forms no `>=` with the `=` after it.
            let closes_atomic = bytes[i - 1] == b'>'
                && no_close
                    .atomic
                    .get_or_insert_with(|| atomic_link_regions(bytes, bounds).into())
                    .binary_search_by_key(&i, |&(_, end)| end)
                    .is_ok();
            if !closes_atomic {
                return None;
            }
        }
    }
    if i > 0 {
        let prev = bytes[i - 1];
        // No same-type nesting: a delimiter adjacent to the same delimiter does
        // not open, so a doubled delimiter (`**x**`, `==x==`, `,,x,,`) is literal.
        if prev == delim {
            return None;
        }
        // Word-boundary opener (spec §9): no bare delimiter opens after an
        // alphanumeric or `_`, keeping paths/identifiers/numbers literal
        // (`a/b/c`, `foo*bar*baz`, `snake_case`, `x = 5`, `key=value`, `1,2,3`).
        // Use the forced `{X…X}` family for deliberate intraword emphasis.
        if prev.is_ascii_alphanumeric() {
            return None;
        }
        // A `_` or `/` guard blocks the opener unless it is an open entry
        // where the opener stands (carve#3973 X1); `/` guards only `/` and `_`.
        if !guard_open && (prev == b'_' || (matches!(delim, b'/' | b'_') && prev == b'/')) {
            return None;
        }
    }
    let close = cached_find_emphasis_close(bytes, i + 1, delim, no_close, bounds)?;
    if NESTING_DEPTH.with(|depth| depth.get() >= MAX_NESTING_DEPTH) {
        return Some((
            InlineNode::text(std::str::from_utf8(&bytes[i..close + 1]).ok()?.to_owned()),
            close + 1 - i,
        ));
    }
    let inner = std::str::from_utf8(&bytes[i + 1..close]).ok()?;
    OpenKinds::pass_on(OpenKinds::bit(delim));
    Some((
        InlineNode::Emphasis(Emphasis {
            attrs: None,
            kind,
            children: parse_inline_context(
                inner,
                options,
                false,
                in_footnote,
                positions,
                base + i + 1,
            ),
            pos: None,
        }),
        close + 1 - i,
    ))
}

/// Whether the bare delimiter at `i` is pushed as an open entry (E4 under E3).
/// `guard_open`: the `_` or `/` before it is itself an open entry there, so it
/// does not guard (carve#3973 X1).
pub(in crate::parse) fn bare_opener_stands(bytes: &[u8], i: usize, guard_open: bool) -> bool {
    let delim = bytes[i];
    let Some(&after) = bytes.get(i + 1) else {
        return false;
    };
    if is_carve_ws(after) || followed_by_grapheme_extender(bytes, i + 1) {
        return false;
    }
    let before = i.checked_sub(1).map(|at| bytes[at]);
    if after == delim || before == Some(delim) || OpenKinds::is_open(delim) {
        return false;
    }
    if before.is_some_and(|b| b.is_ascii_alphanumeric()) || (delim == b'=' && after == b'>') {
        return false;
    }
    guard_open
        || !(before == Some(b'_') || (matches!(delim, b'/' | b'_') && before == Some(b'/')))
}

pub(in crate::parse) fn try_extension_inline(
    text: &str,
    pos: usize,
    options: &Options<'_>,
    citation_brackets: &std::cell::OnceCell<HashMap<usize, usize>>,
) -> Option<InlineMatch> {
    if options.extensions.is_empty() || definition_regions::recording() {
        return None;
    }
    // See `IN_CONTAINER_LABEL`: a label's run is scanned at render time, so no
    // matcher is in scope for it.
    if IN_CONTAINER_LABEL.with(|f| f.get()) {
        return None;
    }
    if !text.is_char_boundary(pos) {
        return None;
    }
    let ctx = MatcherContext::for_inline(options, text, citation_brackets);
    for ext in &options.extensions {
        if let Some(matched) = ext.match_inline(text, pos, &ctx) {
            return Some(matched);
        }
    }
    None
}
