use super::*;

pub(super) fn emphasis_mask(source: &str) -> String {
    let _paragraph_cache = ParagraphCacheScope::new(source);
    let mut mask = mask_djot_attributes(source).into_bytes();
    let autolink = cached_regex!(r"<[^<>\s]+>").unwrap();
    let scheme = cached_regex!(r"[^:]@|[A-Za-z]:").unwrap();
    for value in autolink.find_iter(source) {
        if !is_escaped(source.as_bytes(), value.start()) && scheme.is_match(value.as_str()) {
            mask[value.range()].fill(b' ');
        }
    }
    for label in cached_regex!(r"\[\^[^\]\n]*\]").unwrap().find_iter(source) {
        mask[label.range()].fill(b' ');
    }
    let refs = cached_regex!(
        r"(?m)^[ \t]*(?:>[ \t]*)*(?:(?:[-*+]|[0-9]+[.)])[ \t]+)?\[[^\^\]\n][^\]\n]*\]:[^\n]*"
    )
    .unwrap();
    let reference_boundary =
        cached_regex!(r"^(?:#{1,6} |`{3,}|~{3,}|:{3,}|\{[ \t.#A-Za-z}%]|\[[^\]]*\]:)").unwrap();
    for value in refs.find_iter(source) {
        let text = value.as_str().trim_start_matches([' ', '\t', '>']);
        if text.starts_with("[^") {
            continue;
        }
        let previous = source[..value.start()]
            .split('\n')
            .rev()
            .nth(1)
            .unwrap_or("")
            .trim_start_matches([' ', '\t', '>'])
            .trim();
        if previous.is_empty() || reference_boundary.is_match(previous) {
            mask[value.range()].fill(b' ');
        }
    }
    let images = cached_regex!(r"!\[[^\[\]\n]*\][\[(]").unwrap();
    for value in images.find_iter(source) {
        if !is_escaped(source.as_bytes(), value.start())
            && !is_escaped(source.as_bytes(), value.end() - 1)
        {
            mask[value.range()].fill(b' ');
        }
    }
    let bytes = source.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'[' && i > 0 && bytes[i - 1] == b']' {
            if let Some(end) = source[i..].split('\n').next().unwrap().find(']') {
                mask[i..=i + end].fill(b' ');
            }
        }
        i += 1;
    }
    let mut at = 0;
    while at < bytes.len() {
        if mask[at] == b'{' && !is_escaped(bytes, at) {
            if let Some((end, _)) = read_djot_word_attributes(source, at) {
                if source[at..end].starts_with("{#") && source[at..end].ends_with("#}") {
                    at = end;
                    continue;
                }
                for byte in &mut mask[at..end] {
                    if *byte != b'\n' {
                        *byte = 0;
                    }
                }
                at = end;
                continue;
            }
        }
        at += 1;
    }
    String::from_utf8(mask).expect("inline masks preserve UTF-8 boundaries")
}

pub(super) fn rewrite_djot_body(djot: &str) -> String {
    rewrite_djot_body_with(djot, str::to_string)
}

pub(super) fn rewrite_djot_body_with(djot: &str, transform: impl FnMut(&str) -> String) -> String {
    rewrite_djot_body_with_quotes(djot, transform, false)
}

pub(super) fn rewrite_djot_body_with_quotes(
    djot: &str,
    transform: impl FnMut(&str) -> String,
    omit_quote_delimiters: bool,
) -> String {
    let source = convert_djot_block_markers(&djot.replace("\r\n", "\n").replace('\r', "\n"));
    // Before anything else, and deliberately as a same-length rewrite: `+` and
    // `-` are one byte each, so every offset the mask and the rules below
    // compute stays valid. Doing it afterwards would mean re-masking.
    let source = normalize_plus_bullets(&source);
    // Layout, not a delimiter, and it runs before the escape pass because it
    // works on whole lines: a blank-line run Djot reads as nothing is a list
    // boundary in Carve.
    let source = collapse_false_list_boundaries(&source);
    let (source, orphan_spans) = consume_orphan_djot_attributes(&source);
    let mask = emphasis_mask(&source);
    orphan_spans.restore_with(
        &if omit_quote_delimiters {
            emphasis::convert_image_child(&source, &mask, rewrite_djot_inline)
        } else {
            emphasis::convert(&source, &mask, rewrite_djot_inline)
        },
        transform,
    )
}

pub(super) fn rewrite_djot_inline(source: &str) -> String {
    let _paragraph_cache = ParagraphCacheScope::new(source);
    let masked = emphasis_mask(source);
    let source = escape_plain_carve_syntax_masked(source, HandledDelimiters::DJOT, &masked);
    let masked = emphasis_mask(&source);

    let mut edits: Vec<(usize, usize, String)> = Vec::new();
    let mut taken: std::collections::HashMap<char, std::collections::BTreeMap<usize, usize>> =
        std::collections::HashMap::new();

    for rule in RULES {
        for (start, end, inner_start, inner_end) in find_pairs(&masked, rule, &source) {
            // One delimiter run belongs to one rule. A `~~x~~` claimed by the
            // strikethrough rule must not be re-read as two subscripts.
            let ranges = taken.entry(rule.family).or_default();
            if ranges
                .range(..=start)
                .next_back()
                .is_some_and(|(_, &previous_end)| previous_end > start)
                || ranges
                    .range(start..)
                    .next()
                    .is_some_and(|(&next_start, _)| next_start < end)
            {
                continue;
            }
            ranges.insert(start, end);
            // Only the DELIMITERS are replaced, never the inner text, so a
            // construct of another family nested inside this one is still
            // rewritten by its own rule rather than swallowed whole.
            edits.push((start, inner_start, rule.open.to_string()));
            edits.push((inner_end, end, rule.close.to_string()));
        }
    }

    if edits.is_empty() {
        return source;
    }

    edits.sort_by_key(|(start, _, _)| *start);

    let mut out = String::with_capacity(source.len());
    let mut cursor = 0;
    for (start, end, replacement) in edits {
        if start < cursor {
            continue;
        }
        out.push_str(&source[cursor..start]);
        out.push_str(&replacement);
        cursor = end;
    }
    out.push_str(&source[cursor..]);

    out
}

pub(super) struct Rule {
    /// The delimiter run that opens the construct.
    delimiter: &'static str,
    /// The delimiter run that closes it. Equal to `delimiter` for a symmetric
    /// construct such as `~x~`; different for a braced one, where `{~` opens
    /// and `~}` closes. Searching for the OPENER as the closer is why the
    /// `{=x=}` rule below could never fire - it looked correct only because
    /// its conversion is the identity, so a rule that never matched and a rule
    /// that matched and changed nothing produce the same output.
    closer: &'static str,
    /// Which delimiter character owns the range, so two rules over the same
    /// character cannot both claim it.
    family: char,
    open: &'static str,
    close: &'static str,
    /// `_` only opens and closes at a word boundary. See INTRAWORD below.
    word_bounded: bool,
    /// The inverse: this rule matches ONLY between word characters, which is
    /// the case the word-bounded rule above deliberately declines.
    intraword: bool,
}

/// Longest run first within a family: `~~x~~` is strikethrough, and only what
/// is left over is read as a subscript.
const RULES: &[Rule] = &[
    Rule {
        delimiter: "**",
        closer: "**",
        family: '*',
        open: "*",
        close: "*",
        word_bounded: false,
        intraword: false,
    },
    Rule {
        delimiter: "~~",
        closer: "~~",
        family: '~',
        open: "~",
        close: "~",
        word_bounded: false,
        intraword: false,
    },
    // Djot spells subscript braced as well as bare and means the same by each.
    // The braced form is listed BEFORE the bare one and shares its family, so
    // it claims the range first and the bare rule's match inside the braces is
    // rejected by the overlap check. Converting it as one edit is what keeps
    // the source's own braces from being left behind around a replacement that
    // supplies its own.
    Rule {
        delimiter: "{~",
        closer: "~}",
        family: '~',
        open: "{,",
        close: ",}",
        word_bounded: false,
        intraword: false,
    },
    Rule {
        delimiter: "~",
        closer: "~",
        family: '~',
        open: "{,",
        close: ",}",
        word_bounded: false,
        intraword: false,
    },
    // Braced superscript is spelled identically in both languages, so this is
    // the identity. It still needs a rule: claiming the range is what stops the
    // bare rule below from matching the `^x^` inside the braces and wrapping it
    // a second time into `{{^x^}}`.
    Rule {
        delimiter: "{^",
        closer: "^}",
        family: '^',
        open: "{^",
        close: "^}",
        word_bounded: false,
        intraword: false,
    },
    Rule {
        delimiter: "^",
        closer: "^",
        family: '^',
        open: "{^",
        close: "^}",
        word_bounded: false,
        intraword: false,
    },
    Rule {
        delimiter: "_",
        closer: "_",
        family: '_',
        open: "/",
        close: "/",
        word_bounded: true,
        intraword: false,
    },
    // The complement of the rule above, and it CONVERTS rather than leaving the
    // run literal. The input is a DJOT document: Djot emphasizes an intraword
    // `_`, and an author who wanted the literal characters had to escape them.
    // `snake\_case\_name` renders as `snake_case_name` in Djot and arrives here
    // already escaped, so an UNESCAPED `snake_case_name` is emphasis the author
    // saw in their own renderer and kept.
    //
    // The braced form is required, not stylistic: a bare `/` is literal
    // intraword in Carve, so only `snake{/case/}name` renders as
    // `snake<em>case</em>name`.
    Rule {
        delimiter: "_",
        closer: "_",
        family: '_',
        open: "{/",
        close: "/}",
        word_bounded: false,
        intraword: true,
    },
    Rule {
        delimiter: "{=",
        closer: "=}",
        family: '{',
        open: "{=",
        close: "=}",
        word_bounded: false,
        intraword: false,
    },
];

/// INTRAWORD UNDERSCORES ARE DELIBERATELY LEFT ALONE, and this is a choice
/// about what the author MEANT rather than about what Djot says.
///
/// A strict Djot reader emphasizes them: pandoc's Djot reader turns
/// `snake_case_name` into `snake<em>case</em>name`. This converter does not,
/// because the documents it exists for - notes, READMEs, generated docs - are
/// full of `snake_case` identifiers that no author intended as emphasis, and
/// Carve itself leaves an intraword `_` literal for exactly that reason.
///
/// So the migration is faithful to intent, not to a strict reading, and the
/// cost is real: a Djot document that DID mean emphasis there loses it
/// silently. Documented rather than left as a surprise in the pattern.
pub(super) fn is_word_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// Is the byte at `index` escaped by a backslash? An odd run of backslashes
/// before it escapes it; an even run is literal backslashes and the character
/// still counts. `\_not em\_` is text, not emphasis.
pub(super) fn is_escaped(bytes: &[u8], index: usize) -> bool {
    let mut backslashes = 0;
    let mut k = index;
    while k > 0 && bytes[k - 1] == b'\\' {
        backslashes += 1;
        k -= 1;
    }

    backslashes % 2 == 1
}

/// Every `(start, end, inner_start, inner_end)` this rule matches, scanning
/// left to right and never overlapping itself.
pub(super) fn find_pairs(
    masked: &str,
    rule: &Rule,
    source: &str,
) -> Vec<(usize, usize, usize, usize)> {
    let bytes = masked.as_bytes();
    let delimiter = rule.delimiter.as_bytes();
    let width = delimiter.len();
    let closer_width = rule.closer.len();
    let mut found = Vec::new();
    let mut i = 0;
    while i + width + closer_width <= bytes.len() {
        if !bytes[i..].starts_with(delimiter) {
            i += 1;
            continue;
        }

        // An escaped delimiter is literal text and opens nothing.
        if is_escaped(bytes, i) {
            i += width;
            continue;
        }
        if matches!(rule.delimiter, "~" | "^") && bytes.get(i + width) == Some(&b'}') {
            i += width;
            continue;
        }
        if matches!(rule.delimiter, "~" | "^")
            && i > 0
            && source.as_bytes()[i - 1] == b'{'
            && !is_escaped(source.as_bytes(), i - 1)
        {
            i += width;
            continue;
        }

        // A `~~` opener must not be read as the `~` rule's opener plus content,
        // so a longer run of the same character is not an opener for the
        // shorter rule.
        if width == 1 && bytes.get(i + 1) == Some(&delimiter[0]) {
            i += 2;
            continue;
        }

        if rule.word_bounded && i > 0 {
            let before = masked[..i].chars().next_back();
            if before.is_some_and(is_word_char) {
                i += width;
                continue;
            }
        }

        if rule.intraword {
            let before = if i > 0 {
                masked[..i].chars().next_back()
            } else {
                None
            };
            if !before.is_some_and(is_word_char) {
                i += width;
                continue;
            }
        }

        let inner_start = i + width;
        // An opener is not one when whitespace follows it.
        if !rule.delimiter.starts_with('{')
            && bytes
                .get(inner_start)
                .is_some_and(|b| b.is_ascii_whitespace())
        {
            i += width;
            continue;
        }

        match find_closer(masked, inner_start, rule, source) {
            Ok(inner_end) => {
                found.push((i, inner_end + closer_width, inner_start, inner_end));
                i = inner_end + closer_width;
            }
            Err(paragraph_end) => i = paragraph_end,
        }
    }

    found
}

pub(super) fn find_closer(
    masked: &str,
    from: usize,
    rule: &Rule,
    source: &str,
) -> Result<usize, usize> {
    let bytes = masked.as_bytes();
    let delimiter = rule.closer.as_bytes();
    let width = delimiter.len();
    // Only a symmetric construct can mistake a longer run of its own delimiter
    // for a closer; `~}` cannot be part of a `~~` run.
    let symmetric = rule.closer == rule.delimiter;
    let mut j = from;

    while j + width <= bytes.len() {
        if rule.delimiter.starts_with('{')
            && bytes[j..].starts_with(rule.delimiter.as_bytes())
            && !is_escaped(source.as_bytes(), j)
        {
            return Err(j);
        }
        // A construct never spans a blank line: that is a paragraph break, and
        // a delimiter on the far side of one closes nothing.
        if bytes[j] == b'\n' && quoted_blank_line_follows(source.as_bytes(), j, true) {
            return Err(j + 1);
        }

        if bytes[j..].starts_with(delimiter) {
            if is_escaped(bytes, j) {
                j += width;
                continue;
            }
            if matches!(rule.delimiter, "~" | "^") && bytes.get(j + width) == Some(&b'}') {
                j += width;
                continue;
            }

            // A closer is not one when whitespace precedes it, and an empty
            // pair is not a construct.
            let preceded_by_space = j > from && bytes[j - 1].is_ascii_whitespace();
            if j == from && rule.delimiter.starts_with('{') {
                return Err(j + width);
            }
            if j == from || (preceded_by_space && !rule.delimiter.starts_with('{')) {
                j += 1;
                continue;
            }

            if rule.word_bounded {
                let after = masked[j + width..].chars().next();
                if after.is_some_and(is_word_char) {
                    j += width;
                    continue;
                }
            }

            if rule.intraword {
                let after = masked[j + width..].chars().next();
                if !after.is_some_and(is_word_char) {
                    j += width;
                    continue;
                }
            }

            // Adjacent subscript or superscript spans can share a delimiter run.
            if symmetric
                && width == 1
                && !matches!(rule.delimiter, "~" | "^")
                && bytes.get(j + 1) == Some(&delimiter[0])
            {
                j += 2;
                continue;
            }

            return Ok(j);
        }

        j += 1;
    }

    Err(bytes.len())
}

/// Is the newline at `index` followed by a line holding nothing but spaces and
/// tabs, i.e. a paragraph break?
pub(super) fn blank_line_follows(bytes: &[u8], index: usize) -> bool {
    quoted_blank_line_follows(bytes, index, false)
}

pub(super) fn quoted_blank_line_follows(bytes: &[u8], index: usize, quoted: bool) -> bool {
    let mut k = index + 1;
    while k < bytes.len() && (bytes[k] == b' ' || bytes[k] == b'\t') {
        k += 1;
    }
    while quoted && bytes.get(k) == Some(&b'>') {
        k += 1;
        while matches!(bytes.get(k), Some(b' ' | b'\t')) {
            k += 1;
        }
    }

    k >= bytes.len() || bytes[k] == b'\n' || (bytes[k] == b'\r' && bytes.get(k + 1) == Some(&b'\n'))
}

/// Replace every byte of code and every link destination with a space, so the
/// scan above cannot see a delimiter that is not one. Offsets are preserved, so
/// a match in the mask splices into the original unchanged.
pub(super) fn mask_code_and_destinations(source: &str) -> String {
    let _paragraph_cache = ParagraphCacheScope::new(source);
    mask_djot_inline(source, true)
}

pub(super) fn mask_djot_inline(source: &str, inline_forms: bool) -> String {
    let _paragraph_cache = ParagraphCacheScope::new(source);
    mask_djot_forms(source, inline_forms, true, None, &[])
}

type FenceLineCallback<'a> = &'a mut dyn FnMut(usize, &str);

pub(super) fn mask_djot_fences(
    source: &str,
    on_fence_line: Option<FenceLineCallback<'_>>,
    row_boundaries: &[bool],
    strict: bool,
) -> String {
    mask_djot_fences_with_open(source, on_fence_line, row_boundaries, strict, None)
}

pub(super) fn mask_djot_fences_with_open(
    source: &str,
    mut on_fence_line: Option<FenceLineCallback<'_>>,
    row_boundaries: &[bool],
    strict: bool,
    on_open: Option<&dyn Fn(usize)>,
) -> String {
    let mut mask = source.as_bytes().to_vec();
    let opener = cached_regex!(r"^([ \t]*)(?:(\[\^[^\]\n]+\]:[ \t]*|[-*+][ \t]+(?:\[[ xX]\][ \t]+)?|(?:(?:[0-9]+|[A-Za-z]|[ivxlcdm]+|[IVXLCDM]+)[.)]|\((?:[0-9]+|[A-Za-z]|[ivxlcdm]+|[IVXLCDM]+)\)|:)[ \t]+))?(`{3,}|~{3,})[ \t]*=?[A-Za-z0-9_+#.-]*[ \t]*$").unwrap();
    let boundary = cached_regex!(r"^[ \t]*(?:[-*+] |[0-9]+[.)] |:{1,2} |#{1,6} )").unwrap();
    let nested_note_header = cached_regex!(r"\[\^[^\]\n]+\]:[ \t]*").unwrap();
    let mut fence: Option<(u8, usize, usize, Option<usize>, usize)> = None;
    let mut previous_block = true;
    let mut fence_target = String::new();
    let mut fence_dedent = 0;
    let mut fence_normalize = false;
    let mut normalize_boundary = true;
    let normalize_boundary_pattern = cached_regex!(r"^[ \t]*(?:#{1,6} |:{3,})").unwrap();
    let mut offset = 0;
    let marker_pattern = cached_regex!(r"^[ \t]*(?:[-*+]|(?:[0-9]+|[A-Za-z]|[ivxlcdm]+|[IVXLCDM]+)[.)]|\((?:[0-9]+|[A-Za-z]|[ivxlcdm]+|[IVXLCDM]+)\)|:)[ \t]+\S").unwrap();
    let thematic_pattern = cached_regex!(r"^(?:([*-])[ \t]*){3,}$").unwrap();
    let native_marker = cached_regex!(r"\(([0-9A-Za-z]+)\)([ \t]+)$").unwrap();
    let marker_prefix = cached_regex!(r"^[ \t]*(?:[-*+]|(?:[0-9]+|[A-Za-z]|[ivxlcdm]+|[IVXLCDM]+)[.)]|\((?:[0-9]+|[A-Za-z]|[ivxlcdm]+|[IVXLCDM]+)\)|:)[ \t]+").unwrap();
    let footnote_prefix = cached_regex!(r"^([ \t]*(?:(?:[-*+]|(?:[0-9]+|[A-Za-z]|[ivxlcdm]+|[IVXLCDM]+)[.)]|\((?:[0-9]+|[A-Za-z]|[ivxlcdm]+|[IVXLCDM]+)\)|:)[ \t]+)*)\[\^[^\]\n]+\]:").unwrap();
    let empty_footnote = cached_regex!(r"^[ \t]*\[\^[^\]\n]+\]:[ \t]*$").unwrap();
    let mut previous_depth = 0;
    let block_definition = cached_regex!(r"^[ \t]*\[[^\^\]][^\]]*\]:").unwrap();
    let mut ancestors: Vec<Vec<(usize, usize, usize)>> = Vec::new();
    let quote_prefix = cached_regex!(r"\A[ \t]*>(?:[ \t]|$)").unwrap();
    for (line_index, raw) in source.split_inclusive('\n').enumerate() {
        let line = raw.trim_end_matches('\n');
        let (depth, content) = if let Some((_, _, _, _, host)) = fence {
            let mut depth = 0;
            let mut at = 0;
            while depth < host {
                let Some(prefix) = quote_prefix.find(&line[at..]) else {
                    break;
                };
                depth += 1;
                at += prefix.end();
            }
            (depth, &line[at..])
        } else {
            quoted(line)
        };
        let mut can_normalize = normalize_boundary
            || row_boundaries.get(line_index.wrapping_sub(1)) == Some(&true)
            || depth < previous_depth;
        previous_depth = depth;
        let prefix_len = line.len() - content.len();
        if fence.is_some_and(|(_, _, _, _, owner_depth)| {
            depth < owner_depth && !content.trim().is_empty()
        }) {
            fence = None;
        }
        let indent = content.len() - content.trim_start_matches([' ', '\t']).len();
        if fence.is_some_and(|(_, _, _, minimum, owner_depth)| {
            minimum.is_some_and(|minimum| indent < minimum)
                && !content.trim().is_empty()
                && depth == owner_depth
        }) {
            fence = None;
        }
        let mut nested = false;
        let mut owner_column = 0;
        let mut owner_indent = 0;
        ancestors.truncate(depth + 1);
        if fence.is_none() && !content.trim().is_empty() {
            ancestors.resize_with(depth + 1, Vec::new);
            let mut view = line;
            for (level, stack) in ancestors.iter_mut().enumerate() {
                let columns = view.len() - view.trim_start_matches([' ', '\t']).len();
                while stack.last().is_some_and(|(prior, _, _)| *prior >= columns) {
                    if level == depth
                        && stack.last().unwrap().1 > 0
                        && columns <= stack.last().unwrap().2
                    {
                        can_normalize = true;
                    }
                    stack.pop();
                }
                if level == depth {
                    owner_column = stack.last().map_or(0, |top| top.1);
                    owner_indent = stack.last().map_or(0, |top| top.2);
                    nested = owner_column > 0;
                }
                let body = view.trim_start_matches([' ', '\t']).as_bytes();
                let first = body.first().copied().unwrap_or(0);
                let next = body.get(1).copied().unwrap_or(0);
                let roman = |byte: u8| b"ivxlcdmIVXLCDM".contains(&byte);
                let marker_lead = matches!(first, b'-' | b'*' | b'+' | b':' | b'[' | b'(')
                    || first.is_ascii_digit()
                    || (first.is_ascii_alphabetic()
                        && (matches!(next, b'.' | b')') || (roman(first) && roman(next))));
                if stack.is_empty() && !marker_lead {
                    if level < depth {
                        let after = view
                            .trim_start_matches([' ', '\t'])
                            .strip_prefix('>')
                            .unwrap();
                        view = after.strip_prefix(' ').unwrap_or(after);
                    }
                    continue;
                }
                let marker =
                    !thematic_pattern.is_match(view.trim()) && marker_pattern.is_match(view);
                let prefix = marker_prefix.find(view).map_or("", |value| value.as_str());
                let note = footnote_prefix.captures(view);
                let column = if let Some(note) = &note {
                    native_marker.replace_all(&note[1], "$1.$2").len() + 2
                } else if marker {
                    native_marker.replace(prefix, "$1.$2").len()
                } else {
                    stack.last().map_or(0, |top| top.1)
                };
                let owning_indent = if let Some(note) = &note {
                    note[1].len()
                } else if marker {
                    columns
                } else {
                    stack.last().map_or(0, |top| top.2)
                };
                if note.is_some() && marker {
                    stack.push((
                        columns,
                        native_marker.replace(prefix, "$1.$2").len(),
                        columns,
                    ));
                }
                if column > 0 {
                    stack.push((
                        note.as_ref().map_or(columns, |note| note[1].len()),
                        column,
                        owning_indent,
                    ));
                }
                if let Some(prefix) = cached_regex!(r"^[ \t]*(?:(?:[-*+]|(?:[0-9]+|[A-Za-z]|[ivxlcdm]{2,}|[IVXLCDM]{2,})[.)]|\((?:[0-9]+|[A-Za-z]|[ivxlcdm]{2,}|[IVXLCDM]{2,})\))[ \t]+){2,}:").unwrap().find(view) {
                    let head = &view[..prefix.end() - 1];
                    let last = cached_regex!(r"(?:[-*+]|(?:[0-9]+|[A-Za-z]|[ivxlcdm]{2,}|[IVXLCDM]{2,})[.)]|\((?:[0-9]+|[A-Za-z]|[ivxlcdm]{2,}|[IVXLCDM]{2,})\))[ \t]+").unwrap().find_iter(head).last().unwrap();
                    stack.push((last.start(), native_marker.replace_all(head, "$1.$2").len(), last.start()));
                }
                if level < depth {
                    let after = view
                        .trim_start_matches([' ', '\t'])
                        .strip_prefix('>')
                        .unwrap();
                    view = after.strip_prefix(' ').unwrap_or(after);
                }
            }
        }
        let ordinary_open = fence.is_none().then(|| opener.captures(content)).flatten();
        let opening_at = if fence.is_none()
            && ordinary_open.is_none()
            && content.contains("[^")
            && (content.contains("```") || content.contains("~~~"))
        {
            djot_content_start(content)
        } else {
            0
        };
        let opening_prefix = &content[..opening_at];
        let nested_headers: Vec<_> = nested_note_header.find_iter(opening_prefix).collect();
        let nested_note = nested_headers.len() > 1
            && !nested_note_header
                .replace_all(opening_prefix, "")
                .contains('>');
        let opening_content = if nested_note {
            &content[opening_at..]
        } else {
            content
        };
        if let Some((ch, run, _, _, owner_depth)) = fence {
            blank_out(&mut mask, offset, offset + line.len());
            let candidate = &content[indent..];
            let length = candidate.bytes().take_while(|byte| *byte == ch).count();
            if depth == owner_depth
                && length >= run
                && candidate[length..].trim_matches([' ', '\t']).is_empty()
            {
                if let Some(callback) = on_fence_line.as_mut().filter(|_| fence_normalize) {
                    callback(
                        line_index,
                        &format!("{}{}{}", &line[..prefix_len], fence_target, candidate),
                    );
                }
                normalize_boundary = fence_normalize;
                fence = None;
                previous_block = true;
            } else if fence_normalize && depth == owner_depth {
                if let Some(callback) = on_fence_line.as_mut() {
                    callback(
                        line_index,
                        &format!(
                            "{}{}{}",
                            &line[..prefix_len],
                            fence_target,
                            &content[fence_dedent.min(indent)..]
                        ),
                    );
                }
            }
        } else if let Some(open) = ordinary_open.or_else(|| {
            nested_note
                .then(|| opener.captures(opening_content))
                .flatten()
        }) {
            let opening_indent = if nested_note {
                &content[..content.len() - content.trim_start_matches([' ', '\t']).len()]
            } else {
                &open[1]
            };
            let marker = if nested_note {
                &opening_prefix[opening_indent.len()..]
            } else {
                open.get(2).map_or("", |value| value.as_str())
            };
            let nested_note_column = nested_note.then(|| {
                opening_prefix[..nested_headers.last().unwrap().start()]
                    .encode_utf16()
                    .count()
            });
            let task_start = cached_regex!(r"\[[ xX]\][ \t]+$")
                .unwrap()
                .find(marker)
                .map_or(marker.len(), |task| task.start());
            let marker_base = &marker[..task_start];
            let native_prefix = format!(
                "{}{}",
                native_marker.replace(marker_base, "$1.$2"),
                &marker[task_start..]
            );
            if (!marker.starts_with(':') || previous_block)
                && (!strict || can_normalize || !marker.is_empty())
            {
                let container = if let Some(column) = nested_note_column {
                    Some(column + 1)
                } else if !marker.is_empty() {
                    Some(opening_indent.len() + 1)
                } else if nested {
                    Some(owner_indent + 1)
                } else {
                    None
                };
                let target_column = if let Some(column) = nested_note_column {
                    column + 2
                } else if marker.starts_with("[^") {
                    opening_indent.len() + 2
                } else if !marker.is_empty() {
                    opening_indent.len() + native_marker.replace(marker_base, "$1.$2").len()
                } else if nested {
                    owner_column
                } else {
                    0
                };
                fence_normalize = can_normalize || !marker.is_empty();
                fence_dedent = if nested_note {
                    opening_prefix.encode_utf16().count()
                } else {
                    opening_indent.len() + marker.len()
                };
                fence_target = " ".repeat(target_column);
                if fence_normalize && (marker.is_empty() || native_prefix != marker) {
                    if let Some(callback) = on_fence_line.as_mut() {
                        callback(
                            line_index,
                            &format!(
                                "{}{}{}",
                                &line[..prefix_len],
                                if marker.is_empty() {
                                    fence_target.clone()
                                } else {
                                    format!("{}{}", opening_indent, native_prefix)
                                },
                                &content[if nested_note {
                                    opening_at
                                } else {
                                    fence_dedent
                                }..]
                            ),
                        );
                    }
                }
                let start = open.get(3).unwrap().start()
                    + prefix_len
                    + if nested_note { opening_at } else { 0 };
                if let Some(on_open) = on_open {
                    on_open(line_index);
                }
                fence = Some((open[3].as_bytes()[0], open[3].len(), 0, container, depth));
                blank_out(&mut mask, offset + start, offset + line.len());
            }
        } else {
            let attribute_start = content.len() - content.trim_start().len();
            let line_attributes = if content.as_bytes().get(attribute_start) == Some(&b'{') {
                read_djot_word_attributes(content, attribute_start)
            } else {
                None
            };
            let attribute_boundary = can_normalize
                && line_attributes.is_some_and(|(end, _)| end == content.trim_end().len());
            let definition_div_boundary = can_normalize
                && cached_regex!(r"^[ \t]*:[ \t]+:{3,}(?:[ \t]*[A-Za-z0-9_-]+)?[ \t]*$")
                    .unwrap()
                    .is_match(content);
            normalize_boundary = thematic_pattern.is_match(content.trim())
                || block_definition.is_match(content)
                || content.trim().is_empty()
                || empty_footnote.is_match(content)
                || normalize_boundary_pattern.is_match(content)
                || definition_div_boundary
                || attribute_boundary;
            previous_block = content.trim().is_empty()
                || boundary.is_match(content)
                || definition_div_boundary
                || attribute_boundary;
        }
        offset += raw.len();
    }

    String::from_utf8(mask).expect("fence masks preserve UTF-8")
}

pub(super) fn mask_djot_forms(
    source: &str,
    inline_forms: bool,
    footnotes: bool,
    on_fence_line: Option<FenceLineCallback<'_>>,
    row_boundaries: &[bool],
) -> String {
    mask_djot_forms_with_options(
        source,
        inline_forms,
        footnotes,
        on_fence_line,
        row_boundaries,
        OpaqueOptions {
            destinations: inline_forms,
            ..OpaqueOptions::default()
        },
    )
}

pub(super) fn mask_djot_forms_with_options(
    source: &str,
    inline_forms: bool,
    footnotes: bool,
    on_fence_line: Option<FenceLineCallback<'_>>,
    row_boundaries: &[bool],
    options: OpaqueOptions<'_>,
) -> String {
    let bytes = source.as_bytes();
    let footnote_ends = if inline_forms && footnotes && source.contains("[^") {
        let mut ends = vec![usize::MAX; bytes.len()];
        let mut close = usize::MAX;
        for i in (0..bytes.len()).rev() {
            if bytes[i] == b'\n' {
                close = usize::MAX;
            } else if bytes[i] == b']' {
                close = i;
            }
            ends[i] = close;
        }
        Some(ends)
    } else {
        None
    };

    let mut mask = mask_djot_fences_with_open(
        source,
        on_fence_line,
        row_boundaries,
        true,
        options.on_fence_open,
    )
    .into_bytes();

    mask = mask_djot_opaque(&String::from_utf8(mask).unwrap(), options);
    if inline_forms && footnotes {
        if let Some(ends) = footnote_ends {
            for (at, _) in source.match_indices("[^") {
                if mask[at] != b'[' || is_escaped(bytes, at) {
                    continue;
                }
                let close = ends.get(at + 2).copied().unwrap_or(usize::MAX);
                if close != usize::MAX {
                    blank_out(&mut mask, at, close + 1);
                }
            }
        }
    }
    let metadata = cached_regex!(r"(?m)^(?:[ \t]*>)*[ \t]*(?:(?:[-*+]|[0-9]+[.)]|:)[ \t]+)?:{3,}[ \t]+([A-Za-z_][A-Za-z0-9_.-]*)").unwrap();
    let item_metadata = cached_regex!(r"(?:[-*+]|[0-9]+[.)])[ \t]+:{3,}").unwrap();
    for caps in metadata.captures_iter(source) {
        let value = caps.get(0).unwrap();
        let previous = source[..value.start()]
            .split('\n')
            .rev()
            .nth(1)
            .unwrap_or("");
        if cached_regex!(r"^(?:[ \t]*>)*[ \t]*:[ \t]+:{3,}")
            .unwrap()
            .is_match(value.as_str())
        {
            let line_end = source[value.start()..]
                .find('\n')
                .map_or(source.len(), |end| value.start() + end);
            if !cached_regex!(r"^(?:[ \t]*>)*[ \t]*:[ \t]+:{3,}(?:[ \t]*[A-Za-z0-9_-]+)?[ \t]*$")
                .unwrap()
                .is_match(&source[value.start()..line_end])
            {
                continue;
            }
        }
        if previous.trim().is_empty()
            || cached_regex!(r"^[ \t]*:{1,2}[ \t]+\x00DJOTEMPTYTERM\x00")
                .unwrap()
                .is_match(previous)
            || item_metadata.is_match(value.as_str())
        {
            mask[caps.get(1).unwrap().range()].fill(b' ');
        }
    }
    String::from_utf8(mask).unwrap_or_else(|_| source.to_string())
}

type RawFormatCallback<'a> = &'a dyn Fn(usize, usize, &str);

// All import scans share the same mask; code padding leaves ordinary code visible.
#[derive(Clone, Copy)]
pub(super) struct OpaqueOptions<'a> {
    pub(super) code: bool,
    pub(super) raw_code: bool,
    pub(super) unclosed_code: bool,
    pub(super) autolinks: bool,
    pub(super) destinations: bool,
    pub(super) attribute_values: bool,
    pub(super) on_raw_format: Option<RawFormatCallback<'a>>,
    pub(super) on_fence_open: Option<&'a dyn Fn(usize)>,
}

impl Default for OpaqueOptions<'_> {
    fn default() -> Self {
        Self {
            code: true,
            raw_code: true,
            unclosed_code: true,
            autolinks: true,
            destinations: true,
            attribute_values: true,
            on_raw_format: None,
            on_fence_open: None,
        }
    }
}

pub(super) fn mask_djot_opaque(source: &str, options: OpaqueOptions<'_>) -> Vec<u8> {
    let _paragraph_cache = ParagraphCacheScope::new(source);
    mask_djot_opaque_with_comments(source, options).0
}

pub(super) fn mask_djot_opaque_with_comments(
    source: &str,
    options: OpaqueOptions<'_>,
) -> (Vec<u8>, Vec<(usize, usize)>) {
    mask_djot_opaque_with_comments_and_code(source, options, None)
}

pub(super) struct DjotCodeSpan {
    pub(super) start: usize,
    pub(super) end: usize,
    pub(super) closed: bool,
}

pub(super) fn mask_djot_opaque_with_comments_and_code(
    source: &str,
    options: OpaqueOptions,
    mut code_spans: Option<&mut Vec<DjotCodeSpan>>,
) -> (Vec<u8>, Vec<(usize, usize)>) {
    let reference_boundary =
        cached_regex!(r"^(?:#{1,6} |:{3,}|[`~]{3,}|\{|\[[^\]]+\]:|(?:[*-][ \t]*){3,}$)").unwrap();
    let paragraph_break = cached_regex!(r"\n[ \t]*(?:>[ \t]*)*\n").unwrap();
    let autolink = cached_regex!(r"^<[^<>\s]+>").unwrap();
    let autolink_scheme = cached_regex!(r"[^:]@|[A-Za-z]:").unwrap();
    let attribute_value = cached_regex!(r#"=\s*("(?:\\.|[^"\\])*"|[^\s{}%]+)"#).unwrap();
    let definition_prefix = cached_regex!(
        r"^[ \t]*(?:>[ \t]*)*(?:(?:[-*+]|[0-9A-Za-z]+[.)])[ \t]+)?\[(?:[^\]\n^][^\]\n]*)\]:"
    )
    .unwrap();
    let destinations = if options.destinations {
        djot_simple_destination_ranges(source).unwrap_or_else(|| {
            let code_mask = mask_djot_forms_with_options(
                source,
                false,
                false,
                None,
                &[],
                OpaqueOptions {
                    destinations: false,
                    unclosed_code: options.unclosed_code,
                    ..OpaqueOptions::default()
                },
            );
            djot_destination_ranges(source, &code_mask)
        })
    } else {
        HashMap::new()
    };
    let quote_prefix = cached_regex!(r"^(?:[ \t]*>(?:[ \t]|$))*[ \t]*").unwrap();
    let mut definition_lines = std::collections::HashSet::new();
    let mut line_offset = 0;
    let mut previous_line: &str = "";
    if source.contains("]:") {
        for line in source.split('\n') {
            if let Some(definition) = definition_prefix.find(line) {
                if line_offset == 0
                    || previous_line[djot_content_start(previous_line)..]
                        .trim()
                        .is_empty()
                    || reference_boundary
                        .is_match(previous_line[djot_content_start(previous_line)..].trim())
                    || reference_boundary.is_match(
                        previous_line[quote_prefix.find(previous_line).unwrap().end()..].trim(),
                    )
                {
                    definition_lines.insert(line_offset + definition.end() - 2);
                }
            }
            line_offset += line.len() + 1;
            previous_line = line;
        }
    }
    let raw_formats: HashMap<usize, usize> = cached_regex!(r"\{=[^\x09-\x0d\x20\x{00a0}\x{1680}\x{2000}-\x{200a}\x{2028}\x{2029}\x{202f}\x{205f}\x{3000}\x{feff}{}`]+\}")
        .unwrap()
        .find_iter(source)
        .map(|raw| (raw.start(), raw.end()))
        .collect();
    let last_brace = source.rfind('}');
    let mut comments = Vec::new();
    let bytes = source.as_bytes();
    let mut mask = bytes.to_vec();
    let mut breaks: Vec<_> = paragraph_break
        .find_iter(source)
        .map(|m| m.start())
        .collect();
    if bytes.contains(&b'`') && bytes.contains(&b'\n') {
        breaks.extend(
            djot_inline_boundaries_with_code_scopes(source, source, true)
                .into_iter()
                .filter(|&at| {
                    at > 0 && at < bytes.len() && bytes[at - 1] == b'\n' && bytes[at] != b'|'
                })
                .map(|at| at - 1),
        );
        breaks.sort_unstable();
    }
    let mut boundary = 0;
    let mut tick_closers: Option<HashMap<usize, usize>> =
        code_spans.as_ref().map(|_| djot_backtick_closers(bytes));
    let mut brackets = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        while breaks.get(boundary).is_some_and(|end| *end <= at) {
            boundary += 1;
            brackets.clear();
        }
        let paragraph_end = breaks.get(boundary).copied().unwrap_or(bytes.len());
        if let Some(end) = destinations.get(&at).copied() {
            blank_out(&mut mask, at, end);
            at = end;
            continue;
        }
        if bytes[at] == b'\\' {
            at = (at + 2).min(bytes.len());
            continue;
        }
        if bytes[at] == b'<' {
            if let Some(angle) = autolink.find(&source[at..]) {
                if autolink_scheme.is_match(angle.as_str()) {
                    if options.autolinks {
                        blank_out(&mut mask, at, at + angle.end());
                    }
                    at += angle.end();
                    continue;
                }
            }
        }
        if bytes[at] == b'{' {
            let attrs = read_djot_word_attributes(source, at);
            if attrs.is_some()
                && bytes.get(at + 1) == Some(&b'%')
                && last_brace.is_some_and(|end| at < end)
            {
                let end = source[at + 2..]
                    .find('}')
                    .map_or(bytes.len(), |offset| at + 2 + offset);
                if bytes.get(end) == Some(&b'}') {
                    comments.push((at, end + 1));
                    if !attrs.as_ref().is_some_and(|(close, _)| *close == end + 1) {
                        blank_out(&mut mask, at, end + 1);
                    }
                    at = end + 1;
                    continue;
                }
            }
            if let Some((end, _)) = attrs {
                for value in attribute_value.captures_iter(&source[at..end]) {
                    let value = value.get(1).unwrap();
                    if options.attribute_values {
                        blank_out(&mut mask, at + value.start(), at + value.end());
                    }
                }
                at = end;
                continue;
            }
        }
        if bytes[at] == b'[' {
            brackets.push(at);
            at += 1;
            continue;
        }
        if bytes[at] == b']'
            && brackets.pop().is_some()
            && bytes.get(at + 1) == Some(&b':')
            && definition_lines.contains(&at)
        {
            let end = source[at..].find('\n').map_or(bytes.len(), |n| at + n);
            if options.destinations {
                blank_out(&mut mask, at + 2, end);
            }
            at = end;
            continue;
        }
        if bytes[at] != b'`' {
            at += 1;
            continue;
        }
        let width = bytes[at..].iter().take_while(|b| **b == b'`').count();
        if let Some(spans) = code_spans.as_mut() {
            let close = tick_closers
                .as_ref()
                .and_then(|closers| closers.get(&at))
                .copied()
                .filter(|close| close + width <= paragraph_end);
            let end = close.map_or(paragraph_end, |close| close + width);
            let math = at > 0 && bytes[at - 1] == b'$' && !is_escaped(bytes, at - 1);
            if !math
                && !raw_formats.contains_key(&end)
                && (close.is_some() || options.unclosed_code)
            {
                spans.push(DjotCodeSpan {
                    start: at,
                    end,
                    closed: close.is_some(),
                });
            }
        }
        let close = if let Some(closers) = &tick_closers {
            closers
                .get(&at)
                .copied()
                .filter(|close| close + width <= paragraph_end)
        } else {
            let close = find_backtick_close(&bytes[..paragraph_end], at + width, width);
            if close.is_none() && (!options.code || !options.unclosed_code) {
                tick_closers = Some(djot_backtick_closers(bytes));
            }
            close
        };
        if let Some(close) = close {
            let end = close + width;
            let raw_end = raw_formats.get(&end).copied();
            if let (Some(raw_end), Some(on_raw_format)) = (raw_end, options.on_raw_format) {
                on_raw_format(at, raw_end, &source[end + 2..raw_end - 1]);
            }
            let math = at > 0 && at < bytes.len() && bytes[at - 1] == b'$';
            if options.code || raw_end.is_some() && options.raw_code || math {
                blank_out(&mut mask, at - usize::from(math), end);
            }
            at = raw_end.unwrap_or(end);
        } else if options.code && options.unclosed_code {
            blank_out(&mut mask, at, paragraph_end);
            at = paragraph_end;
        } else {
            at += width;
        }
    }
    (mask, comments)
}

pub(super) fn blank_out(mask: &mut [u8], from: usize, to: usize) {
    let end = to.min(mask.len());
    for byte in &mut mask[from..end] {
        if *byte != b'\n' {
            *byte = b' ';
        }
    }
}

pub(super) fn djot_backtick_closers(bytes: &[u8]) -> HashMap<usize, usize> {
    let mut runs = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] != b'`' {
            at += 1;
            continue;
        }
        let start = at;
        while bytes.get(at) == Some(&b'`') {
            at += 1;
        }
        runs.push((start, at - start));
    }
    let mut next = HashMap::new();
    let mut closers = HashMap::new();
    for (start, width) in runs.into_iter().rev() {
        if let Some(&close) = next.get(&width) {
            closers.insert(start, close);
        }
        // An escaped first tick can leave the rest of a run as an opener.
        if width > 1 {
            if let Some(&close) = next.get(&(width - 1)) {
                closers.insert(start + 1, close);
            }
        }
        next.insert(width, start);
    }
    closers
}

pub(super) fn find_backtick_close(bytes: &[u8], from: usize, run: usize) -> Option<usize> {
    let mut i = from;
    while i < bytes.len() {
        if bytes[i] == b'`' {
            let here = bytes[i..].iter().take_while(|b| **b == b'`').count();
            if here == run {
                return Some(i);
            }
            i += here;
            continue;
        }
        i += 1;
    }

    None
}
