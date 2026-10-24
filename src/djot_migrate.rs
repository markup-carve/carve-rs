//! Migrate Djot source to Carve source.
//!
//! Port of carve-php `DjotToCarve`, which is itself the converter form of the
//! carve-js `djot-migrate` linter - the canonical list of Djot/Carve delimiter
//! collisions.
//!
//! Several inline delimiters mean different things in the two languages, so a
//! Djot document fed to a Carve processor renders WRONG WITH NO ERROR. This
//! rewrites exactly those:
//!
//! | Djot | Carve | why |
//! |---|---|---|
//! | `_x_` | `/x/` | Djot emphasis is underline in Carve |
//! | `~x~` | `{,x,}` | Djot subscript is strikethrough in Carve |
//! | `^x^` | `{^x^}` | Carve has no bare superscript; the braced form is the only one |
//! | `**x**` | `*x*` | Markdown bold; Carve bold is a single `*` |
//! | `~~x~~` | `~x~` | Markdown strikethrough; Carve strike is a single `~` |
//! | `{=x=}` | `{=x=}` | already the same braced form |
//!
//! Constructs that mean the same in both languages - `$math$`, `{+ins+}`,
//! `{-del-}`, reference links - are left alone. Delimiters inside code, fenced
//! or inline, and inside link destinations are never rewritten.
//!
//! ```
//! assert_eq!(carve::djot_to_carve("_em_ and ~sub~"), "/em/ and {,sub,}");
//! ```

macro_rules! cached_regex {
    ($pattern:literal) => {{
        static COMPILED: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
        Ok::<&'static regex::Regex, regex::Error>($crate::djot_migrate::cached_djot_regex(
            &COMPILED, $pattern,
        ))
    }};
}

#[inline]
pub(crate) fn cached_djot_regex(
    cache: &'static std::sync::OnceLock<regex::Regex>,
    pattern: &'static str,
) -> &'static regex::Regex {
    cache.get_or_init(|| regex::Regex::new(pattern).unwrap())
}

#[path = "djot_migrate/djot_emphasis.rs"]
mod emphasis;

#[path = "djot_migrate/djot_import_normalize.rs"]
mod normalize;

#[path = "djot_migrate/djot_div_classes.rs"]
mod div_classes;
#[path = "djot_migrate/djot_heading_ids.rs"]
mod heading_ids;
#[path = "djot_migrate/djot_heading_targets.rs"]
mod heading_targets;
#[path = "djot_migrate/djot_tight_definition_descriptions.rs"]
mod tight_definition_descriptions;

use std::collections::{HashMap, HashSet};

const DJOT_LINE_SPACING: [char; 3] = [' ', '\t', '\r'];

use crate::ast::{BlockNode, FigureTarget};

fn djot_placeholder_prefix(source: &str, base: &str) -> String {
    let _paragraph_cache = ParagraphCacheScope::new(source);
    if !source.contains(base) {
        return format!("{base}0\0");
    }
    let pattern = cached_regex!(r"(\x00DJOT[A-Z]+\x00?)([0-9]+)").unwrap();
    let mut reserved = HashSet::new();
    for caps in pattern.captures_iter(source) {
        let matched = caps.get(0).unwrap();
        if &caps[1] == base && source.as_bytes().get(matched.end()) == Some(&0) {
            reserved.insert(caps[2].to_string());
        }
    }
    let mut serial = 0usize;
    while reserved.contains(&serial.to_string()) {
        serial += 1;
    }
    format!("{base}{serial}\0")
}

/// Convert Djot source to Carve source.
pub fn djot_to_carve(djot: &str) -> String {
    djot_to_carve_prepared(&strip_footnote_definition_attributes(djot))
}

pub(crate) fn djot_to_carve_prepared(stripped_definitions: &DjotFootnoteAttributeStrip) -> String {
    djot_to_carve_prepared_with_quotes(stripped_definitions, false, false, false)
}

fn djot_to_carve_prepared_with_quotes(
    stripped_definitions: &DjotFootnoteAttributeStrip,
    omit_quote_delimiters: bool,
    nul_shielded: bool,
    skip_heading_targets: bool,
) -> String {
    if nul_shielded || !stripped_definitions.source.contains('\0') {
        return convert_djot_document(
            stripped_definitions,
            omit_quote_delimiters,
            skip_heading_targets,
        );
    }
    let token = "\0U\0";
    let pattern = cached_regex!(r"\x00DJOTNOTEATTR\x00([0-9]+)\x00").unwrap();
    let mut source = String::with_capacity(stripped_definitions.source.len());
    let mut end = 0;
    for caps in pattern.captures_iter(&stripped_definitions.source) {
        if !caps[1]
            .parse::<usize>()
            .ok()
            .is_some_and(|id| stripped_definitions.comments.contains(&id))
        {
            continue;
        }
        let marker = caps.get(0).unwrap();
        source.push_str(&stripped_definitions.source[end..marker.start()].replace('\0', token));
        source.push_str(marker.as_str());
        end = marker.end();
    }
    source.push_str(&stripped_definitions.source[end..].replace('\0', token));
    let protected = DjotFootnoteAttributeStrip {
        source,
        losses: stripped_definitions.losses.clone(),
        comments: stripped_definitions.comments.clone(),
    };
    convert_djot_document(&protected, omit_quote_delimiters, skip_heading_targets)
        .replace(token, "\0")
}

fn mask_djot_reference_definitions<'a>(source: &str, mask: &'a str) -> std::borrow::Cow<'a, str> {
    use std::borrow::Cow;
    if !source.contains("]:") {
        return Cow::Borrowed(mask);
    }
    let mut accepted = HashMap::new();
    fold_djot_references_with_decisions(source, Some(&mut accepted));
    if accepted.is_empty() {
        return Cow::Borrowed(mask);
    }
    let mut result = mask.as_bytes().to_vec();
    let mut covered = None;
    let mut offset = 0;
    for (line, value) in source.split('\n').enumerate() {
        if let Some(decision) = accepted.get(&line) {
            covered = Some(covered.map_or(decision.end, |end: usize| end.max(decision.end)));
        }
        if covered.is_some_and(|end| line <= end) {
            result[offset..offset + value.len()].fill(b' ');
        }
        offset += value.len() + 1;
    }
    Cow::Owned(String::from_utf8(result).expect("Reference masks preserve UTF-8"))
}

fn plain_image_character(ch: char) -> bool {
    ch.is_ascii_alphanumeric()
        || " ,;?/=&@\t\n".contains(ch)
        || !ch.is_ascii() && {
            let mut buffer = [0; 4];
            cached_regex!(r"^[\p{L}\p{N}\p{M}]$")
                .unwrap()
                .is_match(ch.encode_utf8(&mut buffer))
        }
}

fn collapse_plain_nested_image_chains(source: &str) -> std::borrow::Cow<'_, str> {
    use std::borrow::Cow;
    if !source.contains("![") || !source.contains('\n') {
        return Cow::Borrowed(source);
    }
    let mask_source = mask_code_and_destinations(source);
    let mask = mask_djot_reference_definitions(source, &mask_source);
    let mask = mask.as_bytes();
    let quote_pattern = cached_regex!(r"^(?:[ \t]*>[ \t])+").unwrap();
    let mut line_offset = 0;
    let quote_lines: Vec<_> = source
        .split('\n')
        .map(|line| {
            let start = line_offset;
            line_offset += line.len() + 1;
            (
                start,
                quote_pattern
                    .find(line)
                    .map_or("", |prefix| prefix.as_str()),
            )
        })
        .collect();
    let mut source_line = 0;
    let bytes = source.as_bytes();
    let mut result = String::new();
    let mut copied = 0;
    let mut start = 0;
    while start < bytes.len() {
        if bytes.get(start..start + 2) != Some(b"![")
            || mask[start] != b'!'
            || is_escaped(bytes, start)
        {
            start += 1;
            continue;
        }
        while quote_lines
            .get(source_line + 1)
            .is_some_and(|(at, _)| *at <= start)
        {
            source_line += 1;
        }
        let quote_prefix = quote_lines[source_line].1;
        let quote_break = format!("\n{quote_prefix}");
        let mut at = start;
        let mut first_safe = 0;
        let mut previous = '\0';
        let mut non_space = '\0';
        let mut labels = Vec::new();
        let mut positions = Vec::new();
        while bytes.get(at..at + 2) == Some(b"![") && mask[at] == b'!' {
            positions.push(at);
            at += 2;
            let begin = at;
            while at < bytes.len() {
                let ch = source[at..].chars().next().unwrap();
                if !plain_image_character(ch) {
                    break;
                }
                if ch == '\n'
                    && !quote_prefix.is_empty()
                    && !source[at + ch.len_utf8()..].starts_with(quote_prefix)
                {
                    break;
                }
                at += ch.len_utf8();
                if ch == '\n' && !quote_prefix.is_empty() {
                    at += quote_prefix.len();
                }
                if ch == '\n' && (non_space == '\n' || matches!(previous, ' ' | '\t')) {
                    first_safe = labels.len() + 1;
                }
                if !matches!(ch, ' ' | '\t') {
                    non_space = ch;
                }
                previous = ch;
            }
            labels.push(if quote_prefix.is_empty() {
                Cow::Borrowed(&source[begin..at])
            } else {
                Cow::Owned(source[begin..at].replace(&quote_break, "\n"))
            });
            if bytes.get(at..at + 2) != Some(b"![") {
                break;
            }
        }
        if labels.len() < 2 || bytes.get(at) != Some(&b']') {
            start = start.max(at.saturating_sub(1)) + 1;
            continue;
        }
        let mut targets = Vec::new();
        let mut ends = Vec::new();
        for _ in 0..labels.len() {
            if bytes.get(at..at + 2) != Some(b"](") {
                break;
            }
            at += 2;
            let begin = at;
            while at < bytes.len()
                && !bytes[at].is_ascii_whitespace()
                && !b"()[]<>\\\0".contains(&bytes[at])
            {
                at += 1;
            }
            if at == begin || bytes.get(at) != Some(&b')') {
                break;
            }
            targets.push(&source[begin..at]);
            at += 1;
            ends.push(at);
        }
        let first = first_safe.max(labels.len() - targets.len());
        let count = labels.len() - first;
        if count < 2 {
            start = start.max(at.saturating_sub(1)) + 1;
            continue;
        }
        result.push_str(&source[copied..positions[first]]);
        result.push_str("![");
        let label: String = labels[first..].iter().map(|label| label.as_ref()).collect();
        result.push_str(&label.replace('\n', &quote_break));
        result.push_str("](");
        result.push_str(targets[count - 1]);
        result.push(')');
        copied = ends[count - 1];
        start = copied;
    }
    if copied == 0 {
        return Cow::Borrowed(source);
    }
    result.push_str(&source[copied..]);
    Cow::Owned(result)
}

fn convert_djot_document(
    stripped_definitions: &DjotFootnoteAttributeStrip,
    omit_quote_delimiters: bool,
    skip_heading_targets: bool,
) -> String {
    let normalized = &stripped_definitions.source;
    let (frontmatter, separator, body) = split_frontmatter(normalized);
    let collapsed = collapse_plain_nested_image_chains(body);
    let div_openers = normalize_djot_div_openers(&collapsed);
    let table_attributes = escape_djot_table_attribute_boundaries(&div_openers);
    let body = table_attributes.as_ref();
    let has_source_nuls = body.contains("\0U\0");
    let repaired_headings = if !skip_heading_targets
        && has_source_nuls
        && body.contains("][")
        && !cached_regex!(
            r"(?m)^[ \t]*(?:>[ \t]*)*(?:(?:[-*+]|[0-9A-Za-z]+[.)])[ \t]+)?\[\^[^\]\n]+\]:"
        )
        .unwrap()
        .is_match(body)
        && cached_regex!(r"(?m)^[ \t]*(?:>[ \t]*)*(?:(?:[-*+]|[0-9A-Za-z]+[.)])[ \t]+)?#{1,6}[ \t]")
            .unwrap()
            .is_match(body)
    {
        let baseline = convert_djot_document(stripped_definitions, omit_quote_delimiters, true);
        let targets = heading_targets::from_carve(&baseline);
        std::borrow::Cow::Owned(normalize::heading_reference_keys(body, &targets))
    } else {
        std::borrow::Cow::Borrowed(body)
    };
    let body = repaired_headings.as_ref();

    let early_references = if body.lines().any(|line| {
        cached_regex!(r"^\[([^\]\n]*)\]:")
            .unwrap()
            .captures(&line[djot_content_start(line)..])
            .is_some_and(|caps| {
                !caps[1].starts_with('^')
                    && (normalize::reference_key_needs_alias(&caps[1])
                        || has_source_nuls && caps[1].contains('�'))
            })
    }) {
        std::borrow::Cow::Owned(normalize::literal_reference_keys(body))
    } else {
        std::borrow::Cow::Borrowed(body)
    };
    let body = early_references.as_ref();
    let caret_references = if body.contains("[^")
        && body.contains("]:")
        && body.lines().any(|line| {
            cached_regex!(r"^\[\^[^\]\n]*\]:")
                .unwrap()
                .is_match(&line[djot_content_start(line)..])
        }) {
        let mut accepted = HashMap::new();
        fold_djot_references_with_decisions(body, Some(&mut accepted));
        std::borrow::Cow::Owned(normalize::caret_references(body, &accepted))
    } else {
        std::borrow::Cow::Borrowed(body)
    };
    let image_attributes = if caret_references.contains("![")
        && caret_references.contains("][]")
        && caret_references.contains('{')
        && caret_references.contains('\n')
    {
        let mask = mask_code_and_destinations(&caret_references);
        let owners = image_line_owners(&caret_references, &mask).1;
        let mut line = 0;
        std::borrow::Cow::Owned(replace_image_labels(
            &caret_references,
            &mask,
            |image, label, at| {
                if !caret_references[at + image.len()..].starts_with("[]")
                    || !label.contains('{')
                    || !label.contains('\n')
                {
                    return image.to_owned();
                }
                while owners[line].0 <= at {
                    line += 1;
                }
                let owner = owners[line];
                let (normalized, continuations, valid) =
                    image_alt_layout(label, owner.1, owner.2, owner.3, owner.4);
                if !valid {
                    return image.to_owned();
                }
                let text = normalize::image_label_attributes_for_losses(&normalized);
                let mut restored = String::new();
                for (index, value) in text.split('\n').enumerate() {
                    if index > 0 {
                        restored.push('\n');
                        restored.push_str(
                            continuations
                                .get(index - 1)
                                .map(String::as_str)
                                .unwrap_or(""),
                        );
                    }
                    restored.push_str(value);
                }
                format!("![{restored}]")
            },
        ))
    } else {
        std::borrow::Cow::Borrowed(caret_references.as_ref())
    };
    let carried = carry_djot_attributes_across_quote_exits(
        &carry_djot_attributes_across_div_closers(&image_attributes),
    );
    let (literal_attributes, inherited) = escape_invalid_djot_attributes(&carried);
    let attributes =
        definition_term_block_boundaries(&normalize_djot_attribute_lines(&literal_attributes));
    let owned_references = fold_djot_references(&attributes);
    let fences = normalize_djot_fences(&owned_references);
    let empty_note = djot_placeholder_prefix(body, "\0DJOTEMPTYNOTE\0");
    let item_markers = convert_djot_block_markers_with(&fences, true);
    let item_attribute_prefix = djot_placeholder_prefix(&item_markers, "\0DJOTITEMATTR\0");
    let mut item_attributes = Vec::new();
    let blocks = normalize::note_blocks(
        &item_markers,
        stripped_definitions,
        &empty_note,
        &item_attribute_prefix,
        &mut item_attributes,
    );
    let fence_closers = normalize::table_edges(&blocks);
    let folded_references = fold_djot_references(&fence_closers);
    let inline = image_quote_layout(&folded_references, omit_quote_delimiters);
    // Carve has no empty footnote body: `[^b]:` alone is a paragraph, not a
    // definition, so an empty Djot body needs a body that renders nothing - a
    // comment. It travels as a NUL token so the text escaper cannot reach it and
    // turn the comment into a literal `%%%%` the reader was never shown.
    let footnotes =
        normalize_djot_footnotes(&inline, Some(stripped_definitions), &inherited, &empty_note);
    let references = normalize::references(&footnotes);

    let links = normalize_djot_links(&references, &inherited);
    let autolinks = normalize_djot_autolinks(&links);
    let table_pipes = escape_djot_non_table_rows(
        &normalize_djot_table_pipes(&autolinks, &empty_note),
        &empty_note,
    );
    let folded = fold_heading_continuations(&table_pipes);
    let collapsed = cached_regex!(r"(!?\[([^\[\]]*)\])\[\]").unwrap();
    let mut collapsed_mask = mask_code_and_destinations(&folded).into_bytes();
    let mut at = 0;
    while at < folded.len() {
        if collapsed_mask[at] == b'{' {
            if let Some((end, _)) = read_djot_word_attributes(&folded, at) {
                for byte in &mut collapsed_mask[at..end] {
                    if *byte != b'\n' {
                        *byte = b' ';
                    }
                }
                at = end;
                continue;
            }
        }
        at += 1;
    }
    let collapsed_mask =
        String::from_utf8(collapsed_mask).expect("attribute masks preserve UTF-8 boundaries");
    let autolink = cached_regex!(r"<[^<>\s]+>").unwrap();
    let collapsed_mask = autolink.replace_all(&collapsed_mask, |caps: &regex::Captures<'_>| {
        if caps[0].contains(':') || caps[0].contains('@') {
            " ".repeat(caps[0].len())
        } else {
            caps[0].to_string()
        }
    });
    let definitions_regex =
        cached_regex!(r"(?m)^[ \t]*(?:> ?)*(?:(?:[-*+]|[0-9]+[.)])[ \t]+)?\[([^\[\]\n]*)\]:[ \t]")
            .unwrap();
    let definition_boundary =
        cached_regex!(r"^(?:#{1,6} |`{3,}|~{3,}|:{3,}|\{[ \t.#A-Za-z}%]|\[[^\]]*\]:)").unwrap();
    let collapsed_space = cached_regex!(r"[ \t\r\n]+").unwrap();
    let definitions: HashMap<String, String> = definitions_regex
        .captures_iter(&folded)
        .filter(|caps| {
            let start = caps.get(0).unwrap().start();
            let previous = folded[..start]
                .split('\n')
                .rev()
                .nth(1)
                .unwrap_or("")
                .trim_start_matches([' ', '\t', '>'])
                .trim();
            collapsed_mask.as_bytes()[start + caps[0].find('[').unwrap()] == b'['
                && (previous.is_empty() || definition_boundary.is_match(previous))
        })
        .map(|caps| {
            (
                collapsed_space
                    .replace_all(caps[1].trim_matches([' ', '\t', '\r', '\n']), " ")
                    .into_owned(),
                caps[1].to_owned(),
            )
        })
        .collect();
    let mut child_text_cache = HashMap::<String, String>::new();
    let mut collapsed_image_lines: Option<Vec<ImageLineOwner>> = None;
    let mut collapsed_image_line = 0;
    let collapsed_blank = cached_regex!(r"\n[ \t]*\n").unwrap();
    let folded = collapsed.replace_all(&folded, |caps: &regex::Captures<'_>| {
        let at = caps.get(0).unwrap().start();
        let bracket_at = at + usize::from(caps[1].starts_with('!'));
        if definitions.is_empty()
            || is_escaped(folded.as_bytes(), bracket_at)
            || collapsed_mask.as_bytes()[at] != caps[0].as_bytes()[0]
            || collapsed_mask.as_bytes()[at + caps[0].len() - 3] != b']'
            || caps[2].starts_with('^') && caps[2].len() > 1
        {
            return caps[0].to_string();
        }
        let mut key = caps[2].to_owned();
        if key.contains('\n') {
            if collapsed_blank.is_match(&key) || is_escaped(folded.as_bytes(), bracket_at) {
                return caps[0].to_string();
            }
            let lines = collapsed_image_lines
                .get_or_insert_with(|| image_line_owners(&folded, &collapsed_mask).1);
            while lines[collapsed_image_line].0 <= at {
                collapsed_image_line += 1;
            }
            let owner = lines[collapsed_image_line];
            let (normalized, _, valid) = image_alt_layout(&key, owner.1, owner.2, owner.3, owner.4);
            if !valid {
                return caps[0].to_string();
            }
            key = collapsed_space
                .replace_all(
                    child_text_cache
                        .entry(format!("0:{normalized}"))
                        .or_insert_with(|| {
                            djot_image_label_child_text(&normalized, &inherited, true, true, true)
                        })
                        .trim_matches([' ', '\t', '\r', '\n']),
                    " ",
                )
                .into_owned();
        }
        let key = collapsed_space.replace_all(key.trim_matches([' ', '\t', '\r', '\n']), " ");
        if let Some(definition) = definitions.get(key.as_ref()) {
            format!("{}[{definition}]", &caps[1])
        } else {
            caps[0].to_string()
        }
    });
    let mut reference_edits = Vec::new();
    if !definitions.is_empty() && folded.contains("][]") {
        let mask = mask_code_and_destinations(&folded);
        let owners = image_line_owners(&folded, &mask).1;
        let mut line = 0;
        replace_image_labels(&folded, &mask, |image, label, at| {
            let end = at + image.len();
            if !folded[end..].starts_with("[]") || label.starts_with('^') && label.len() > 1 {
                return image.to_owned();
            }
            while owners[line].0 <= at {
                line += 1;
            }
            let owner = owners[line];
            let (normalized, _, valid) =
                image_alt_layout(label, owner.1, owner.2, owner.3, owner.4);
            if !valid {
                return image.to_owned();
            }
            let text = if !normalized
                .bytes()
                .any(|byte| b"_*`{~^\\[]=!/<:\"'\0".contains(&byte))
            {
                normalized.clone()
            } else if normalized.len() <= 256 {
                child_text_cache
                    .entry(format!("REF:{normalized}"))
                    .or_insert_with(|| {
                        djot_image_label_child_text(&normalized, &inherited, true, true, true)
                    })
                    .clone()
            } else {
                djot_image_label_child_text(&normalized, &inherited, true, true, true)
            };
            let key = collapsed_space.replace_all(text.trim_matches([' ', '\t', '\r', '\n']), " ");
            if let Some(definition) = definitions.get(key.as_ref()) {
                reference_edits.push((end + 1, definition.clone()));
            }
            image.to_owned()
        });
    }
    reference_edits.sort_by_key(|entry| entry.0);
    let mut resolved = String::new();
    let mut cursor = 0;
    for (at, name) in reference_edits {
        resolved.push_str(&folded[cursor..at]);
        resolved.push_str(&name);
        cursor = at;
    }
    resolved.push_str(&folded[cursor..]);
    let folded = resolved;
    let alt_prefix = djot_placeholder_prefix(&folded, "\0DJOTALT\0");
    let image_paragraph = djot_placeholder_prefix(&folded, "\0DJOTIMAGEPARAGRAPH\0");
    let mut alts = Vec::new();
    let mut alt_cache = std::collections::HashMap::<(String, usize, bool), String>::new();
    let mask = mask_code_and_destinations(&folded);
    let (image_starts, image_lines) = image_line_owners(&folded, &mask);
    let mut image_line = 0;
    let folded = replace_image_labels(&folded, &mask, |image, label, at| {
        if mask.as_bytes()[at] != b'!'
            || mask.as_bytes()[at + image.len() - 1] != b']'
            || cached_regex!(r"\n[ \t]*\n").unwrap().is_match(label)
            || is_escaped(folded.as_bytes(), at)
        {
            return image.to_string();
        }
        let paragraph = if image_starts.contains(&at) {
            image_paragraph.as_str()
        } else {
            ""
        };
        if label.starts_with('^') {
            return image.to_string();
        }
        if !label.contains('\n')
            && !label.bytes().any(|byte| b"_*`{^~[]<:".contains(&byte))
            && !b"\"'"
                .iter()
                .any(|quote| label.bytes().filter(|byte| byte == quote).take(2).count() >= 2)
            && !label.contains("\"}")
            && !label.contains("'}")
            && !label.as_bytes().windows(2).enumerate().any(|(at, pair)| {
                pair[0] == b'\\'
                    && matches!(pair[1], b' ' | b'\t')
                    && !is_escaped(label.as_bytes(), at)
            })
        {
            alts.push(label.to_string());
            return format!("{paragraph}![{alt_prefix}{}\0]", alts.len() - 1);
        }
        while image_lines[image_line].0 <= at {
            image_line += 1;
        }
        let quotes = image_lines[image_line].1;
        let (_, _, item_column, item_quote, div, in_table) = image_lines[image_line];
        let (normalized_label, continuations, valid) =
            image_alt_layout(label, quotes, item_column, item_quote, div);
        if !valid {
            return image.to_string();
        }
        let cache_key = (label.to_string(), quotes, in_table);
        if let Some(alt) = alt_cache.get(&cache_key) {
            alts.push(alt.clone());
            return format!("{paragraph}![{alt_prefix}{}\0]", alts.len() - 1);
        }
        let plain = child_text_cache
            .entry(format!("1:{normalized_label}"))
            .or_insert_with(|| {
                djot_image_label_child_text(&normalized_label, &inherited, true, true, false)
            });
        let plain = cached_regex!(r"\n(?:[ \t]*\n)+")
            .unwrap()
            .replace_all(plain, "\n");
        let plain = crate::render_carve::escape_image_alt(&plain, in_table);
        let mut alt = String::new();
        for (index, line) in plain.split('\n').enumerate() {
            if index > 0 {
                alt.push('\n');
                alt.push_str(
                    continuations
                        .get(index - 1)
                        .map(String::as_str)
                        .unwrap_or(""),
                );
            }
            alt.push_str(line);
        }
        alt_cache.insert(cache_key, alt.clone());
        alts.push(alt);
        format!("{paragraph}![{alt_prefix}{}\0]", alts.len() - 1)
    });
    let prefix = djot_placeholder_prefix(&folded, "\0DJOTSTRONG");
    let mut spans = Vec::new();
    let words = protect_attributed_words(&folded, &prefix, &mut spans, &inherited);
    let empty_term = djot_placeholder_prefix(&words, "\0DJOTEMPTYTERM\0");
    let restore = regex::Regex::new(&format!(
        r"(?:{}([0-9]+)|{}([0-9]+))\x00",
        regex::escape(&prefix),
        regex::escape(&alt_prefix),
    ))
    .unwrap();
    let converted = rewrite_djot_body_with_quotes(
        &convert_definition_lists(&words, &empty_term),
        |chunk| {
            restore
                .replace_all(
                    &chunk.replace(&empty_term, "%%"),
                    |caps: &regex::Captures<'_>| {
                        let (index, values) = if let Some(index) = caps.get(1) {
                            (index.as_str(), &spans)
                        } else {
                            (caps.get(2).unwrap().as_str(), &alts)
                        };
                        index
                            .parse::<usize>()
                            .ok()
                            .and_then(|index| values.get(index))
                            .cloned()
                            .unwrap_or_else(|| caps[0].to_string())
                    },
                )
                .into_owned()
        },
        omit_quote_delimiters,
    );
    let converted = converted
        .replace(&empty_note, "%%%%")
        .replace(&image_paragraph, "{%%}");
    let converted = if item_attributes.is_empty() {
        converted
    } else {
        let fence_pattern = regex::Regex::new(&format!(
            r"(?m)^([ \t>]*)(\[\^[^\]\n]+\]:[ \t]*)?{}(B?)([0-9]+)\x00\n(?:[ \t>]*\n)*([ \t>]*)([-*+]|[^ \t\n]+)([ \t]+)",
            regex::escape(&item_attribute_prefix)
        ))
        .unwrap();
        let converted = fence_pattern.replace_all(&converted, |caps: &regex::Captures<'_>| {
            caps[4]
                .parse::<usize>()
                .ok()
                .and_then(|index| item_attributes.get(index))
                .map_or_else(
                    || caps[0].to_string(),
                    |attrs| {
                        let header = caps.get(2).map_or("", |header| header.as_str());
                        if &caps[3] == "B" {
                            let prefix = if header.is_empty() {
                                &caps[5]
                            } else {
                                &caps[1]
                            };
                            format!(
                                "{prefix}{header}{attrs}\n{}{}{}",
                                &caps[5], &caps[6], &caps[7]
                            )
                        } else {
                            format!("{}{header}{}{attrs}{}", &caps[5], &caps[6], &caps[7])
                        }
                    },
                )
        });
        let pattern = regex::Regex::new(&format!(
            r"([ \t]+)((?:\[[ xX]\][ \t]+)?){}([0-9]+)\x00",
            regex::escape(&item_attribute_prefix)
        ))
        .unwrap();
        pattern
            .replace_all(&converted, |caps: &regex::Captures<'_>| {
                caps[3]
                    .parse::<usize>()
                    .ok()
                    .and_then(|index| item_attributes.get(index))
                    .map_or_else(
                        || caps[0].to_string(),
                        |attrs| format!("{attrs}{}{}", &caps[1], &caps[2]),
                    )
            })
            .into_owned()
    };
    let converted = cached_regex!(r"\x00DJOTINVALIDATTR\x00[0-9]+\x00")
        .unwrap()
        .replace_all(&converted, |caps: &regex::Captures<'_>| {
            if inherited.contains(&caps[0]) {
                String::new()
            } else {
                caps[0].to_string()
            }
        })
        .into_owned();
    let converted = tight_definition_descriptions::normalize(&div_classes::normalize(&converted));
    heading_ids::preserve(stripped_definitions.restore(&if frontmatter.is_empty() {
        converted
    } else if converted.is_empty() {
        format!("{frontmatter}{separator}")
    } else {
        format!("{}{}{}", frontmatter, separator, converted)
    }))
}

pub(crate) fn djot_to_carve_with_losses(source: &str) -> (String, Vec<crate::MigrationDiagnostic>) {
    let _paragraph_cache = ParagraphCacheScope::new(source);
    (djot_to_carve(source), normalize::losses(source))
}

fn quote_djot_attribute(value: &str, carve: bool) -> String {
    let mut output = String::from("\"");
    for ch in value.chars() {
        if ch == '\\' || ch == '"' || (carve && ch.is_ascii_punctuation()) {
            output.push('\\');
        }
        output.push(ch);
    }
    output.push('"');
    output
}

fn decode_djot_attribute(value: &str) -> String {
    let collapsed = cached_regex!(r"[ \r\n]+").unwrap().replace_all(value, " ");
    let mut chars = collapsed.chars().peekable();
    let mut output = String::new();
    while let Some(ch) = chars.next() {
        if ch == '\\'
            && chars
                .peek()
                .is_some_and(|next| ".,\\/#!$%^&*;:{}=-_`~+[]()'\"?|".contains(*next))
        {
            output.push(chars.next().unwrap());
        } else {
            output.push(ch);
        }
    }
    output
}

mod image_labels;
use image_labels::*;

type ParagraphContext = (usize, Option<usize>, usize);

struct ParagraphCache {
    source: (usize, usize),
    starts: HashMap<usize, (ParagraphContext, bool, usize)>,
}

thread_local! {
    static PARAGRAPH_CACHE: std::cell::RefCell<Option<ParagraphCache>> = const { std::cell::RefCell::new(None) };
}

struct ParagraphCacheScope<'a> {
    previous: Option<ParagraphCache>,
    installed: bool,
    _source: &'a str,
}

impl<'a> ParagraphCacheScope<'a> {
    fn new(source: &'a str) -> Self {
        let (previous, installed) = PARAGRAPH_CACHE.with(|slot| {
            let identity = (source.as_ptr() as usize, source.len());
            if slot
                .borrow()
                .as_ref()
                .is_some_and(|cache| cache.source == identity)
            {
                return (None, false);
            }
            (
                slot.replace(Some(ParagraphCache {
                    source: identity,
                    starts: HashMap::new(),
                })),
                true,
            )
        });
        Self {
            previous,
            installed,
            _source: source,
        }
    }
}

impl Drop for ParagraphCacheScope<'_> {
    fn drop(&mut self) {
        if !self.installed {
            return;
        }
        PARAGRAPH_CACHE.with(|slot| {
            slot.replace(self.previous.take());
        });
    }
}

fn cached_paragraph_start(
    source: &str,
    from: usize,
    context: (usize, Option<usize>, usize),
) -> Option<bool> {
    PARAGRAPH_CACHE.with(|slot| {
        slot.borrow().as_ref().and_then(|cache| {
            if cache.source != (source.as_ptr() as usize, source.len()) {
                return None;
            }
            cache
                .starts
                .get(&from)
                .filter(|(held, _, _)| *held == context)
                .map(|(_, result, _)| *result)
        })
    })
}

fn comment_continues_paragraph(source: &str, start: usize) -> bool {
    comment_continues_paragraph_with_context(source, start, djot_attribute_context(source, start))
}

fn comment_continues_paragraph_with_context(
    source: &str,
    start: usize,
    context: ParagraphContext,
) -> bool {
    if context.1.is_none() {
        return true;
    }
    let from = source[..start].rfind('\n').map_or(0, |at| at + 1);
    let cached = PARAGRAPH_CACHE.with(|slot| {
        slot.borrow()
            .as_ref()
            .filter(|cache| cache.source == (source.as_ptr() as usize, source.len()))
            .and_then(|cache| cache.starts.get(&from))
            .filter(|(held, _, at)| *held == context && *at == start)
            .map(|(_, result, _)| *result)
    });
    if let Some(result) = cached {
        return result;
    }
    let result = scan_comment_paragraph(source, start, context);
    PARAGRAPH_CACHE.with(|slot| {
        if let Some(cache) = slot.borrow_mut().as_mut() {
            if cache.source == (source.as_ptr() as usize, source.len()) {
                cache.starts.insert(from, (context, result, start));
            }
        }
    });
    result
}

fn scan_comment_paragraph(source: &str, start: usize, context: ParagraphContext) -> bool {
    if context.1.is_none() {
        return true;
    }
    let item_pattern = cached_regex!(r"^(?:[ \t]*>[ \t]*)*[ \t]*(?:[-*+]|(?:[0-9]+|[A-Za-z]|(?:[ivxlcdm]+|[IVXLCDM]+))[.)]|\((?:[0-9]+|[A-Za-z]|(?:[ivxlcdm]+|[IVXLCDM]+))\))[ \t]+").unwrap();
    let quote_pattern = cached_regex!(r"^[ \t]*(?:>[ \t]*)+").unwrap();
    let leading_pattern = cached_regex!(r"^(?:[ \t]*>[ \t]*)*[ \t]*").unwrap();
    let heading_pattern = cached_regex!(r"^#{1,6}(?:[ \t]|$)").unwrap();
    let fence_pattern = cached_regex!(r"^(`{3,}|~{3,})").unwrap();
    let definition_pattern = cached_regex!(r"^:[ \t]").unwrap();
    let footnote_pattern = cached_regex!(r"^\[\^[^\]]+\]:[ \t]*(.*)$").unwrap();
    let block_pattern =
        cached_regex!(r"^(?::::|\||\[[^\]]*\]:|:[ \t]|(?:\*[ \t]*){3,}$|(?:-[ \t]*){3,}$)")
            .unwrap();
    let marker = item_pattern;
    let Some(mut end) = source[..start].rfind('\n') else {
        return false;
    };
    let prefix = &source[end + 1..start];
    if marker.is_match(prefix) {
        return false;
    }
    let mut lines: Vec<(&str, usize)> = Vec::new();
    loop {
        let from = source[..end].rfind('\n').map_or(0, |at| at + 1);
        let raw = &source[from..end];
        let quotes = quote_pattern.find(raw).map_or("", |m| m.as_str());
        if quotes.bytes().filter(|b| *b == b'>').count() != context.0 {
            return false;
        }
        if raw[emphasis::structural_prefix_end(raw)..]
            .trim_matches(DJOT_LINE_SPACING)
            .is_empty()
        {
            if lines.is_empty() {
                break;
            }
            let mut parent_end = from.checked_sub(1);
            while let Some(end) = parent_end {
                let parent_from = source[..end].rfind('\n').map_or(0, |at| at + 1);
                let parent = &source[parent_from..end];
                if marker.is_match(parent) {
                    let column = leading_pattern.find(parent).unwrap().end();
                    if lines.last().is_some_and(|(raw, _)| {
                        prefix.len() <= column && emphasis::structural_prefix_end(raw) > column
                    }) {
                        return false;
                    }
                    break;
                }
                if parent.trim_matches(DJOT_LINE_SPACING).is_empty() {
                    break;
                }
                parent_end = parent_from.checked_sub(1);
            }
            break;
        }
        if let Some(cached) = cached_paragraph_start(source, from, context) {
            if cached {
                return true;
            }
            if let Some((end, _, _)) = read_djot_attributes_with_context(
                source,
                from + emphasis::structural_prefix_end(raw),
                false,
                false,
                false,
            ) {
                if end <= start {
                    while lines.last().is_some_and(|(_, from)| *from < end) {
                        lines.pop();
                    }
                    break;
                }
            }
        }
        lines.push((raw, from));
        if from == 0 {
            break;
        }
        end = from - 1;
    }
    let mut kind = None;
    let mut fence = "";
    let mut attribute_end = 0;
    for (raw, from) in lines.into_iter().rev() {
        if from < attribute_end || kind == Some("paragraph") {
            continue;
        }
        if marker.is_match(raw) && prefix.len() <= leading_pattern.find(raw).unwrap().end() {
            return false;
        }
        let body_at = emphasis::structural_prefix_end(raw);
        let body = &raw[body_at..];
        if kind == Some("fence") {
            if body.starts_with(fence) {
                kind = None;
            }
            continue;
        }
        if body.starts_with('{') {
            if let Some((end, _, _)) =
                read_djot_attributes_with_context(source, from + body_at, false, false, false)
            {
                attribute_end = end;
                kind = None;
                continue;
            }
        }
        if heading_pattern.is_match(body) {
            kind = Some("heading");
            continue;
        }
        if let Some(opened) = fence_pattern.find(body) {
            kind = Some("fence");
            fence = opened.as_str();
            continue;
        }
        if body.starts_with("{%")
            && read_djot_attributes_with_context(&format!("{body}}}"), 0, false, false, false)
                .is_some()
        {
            kind = Some("paragraph");
            continue;
        }
        if kind == Some("heading") {
            continue;
        }
        if definition_pattern.is_match(body) {
            if prefix.len() <= body_at {
                return false;
            }
            kind = if body[1..].trim_matches(DJOT_LINE_SPACING).is_empty() {
                None
            } else {
                Some("paragraph")
            };
            continue;
        }
        if let Some(note) = footnote_pattern.captures(body) {
            if prefix.len() < body_at + 2 {
                return false;
            }
            kind = if note[1].trim_matches(DJOT_LINE_SPACING).is_empty() {
                None
            } else {
                Some("paragraph")
            };
            continue;
        }
        if block_pattern.is_match(body) {
            kind = None;
            continue;
        }
        kind = Some("paragraph");
    }
    kind == Some("paragraph")
}

mod attributes;
use attributes::*;

fn fold_heading_continuations(source: &str) -> String {
    let _paragraph_cache = ParagraphCacheScope::new(source);
    let block = cached_regex!(r"^(?:[#>|{]|[-*+][ \t]|(?:[0-9]+|[A-Za-z]|[ivxlcdm]+|[IVXLCDM]+)[.)][ \t]|:[ \t]|:{2,}|\([0-9a-zA-Z]+\)[ \t]|[`~]{3,}|\^[ \t]|\[[^\]\n]*\]:|(?:\*[ \t]*){3,}$|(?:-[ \t]*){3,}$)").unwrap();
    let container = cached_regex!(r"^([ \t]*)(>(?:[ \t]|$)|\[\^[^\]\n]+\]:[ \t]*|[-*+][ \t]+|(?:[0-9]+|[A-Za-z]|[ivxlcdm]+|[IVXLCDM]+)[.)][ \t]+|\((?:[0-9]+|[A-Za-z]|[ivxlcdm]+|[IVXLCDM]+)\)[ \t]+)").unwrap();
    let task = cached_regex!(r"^\[[ xX]\][ \t]+").unwrap();
    let heading_re = cached_regex!(r"^([ \t]*)(#{1,6})(?:[ \t]+\S|[ \t]*$)").unwrap();
    let quote_prefix = cached_regex!(r"^(?:[ \t]*>(?:[ \t]|$))*").unwrap();
    let masked = mask_code_and_destinations(source);
    let masks: Vec<_> = masked.split('\n').collect();
    let lines: Vec<_> = source.split('\n').collect();
    let mut result = Vec::new();
    let mut definition_hosts = HashMap::new();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        let (mut at, mut depth, mut column) = (0, 0, 0);
        while let Some(token) = container.captures(&line[at..]) {
            at += token.get(0).unwrap().len();
            let marker = &token[2];
            if marker.starts_with('>') {
                depth += 1;
                column = 0;
            } else if marker.starts_with("[^") {
                column = token[1].len() + 2;
            } else {
                column += token.get(0).unwrap().len();
                if marker.starts_with(['-', '*', '+']) {
                    if let Some(check) = task.find(&line[at..]) {
                        at += check.len();
                    }
                }
            }
        }
        definition_hosts.retain(|held, _| *held <= depth);
        if depth > 0
            && column > 0
            && cached_regex!(r"^:(?:[ \t]|$)")
                .unwrap()
                .is_match(&line[at..])
            && masks[i].as_bytes().get(at) == Some(&b':')
        {
            definition_hosts.insert(depth, column);
        } else if column > 0 {
            definition_hosts.remove(&depth);
        }
        if !line.contains('#') {
            result.push(line.to_owned());
            i += 1;
            continue;
        }
        let Some(heading) = heading_re.captures(&line[at..]) else {
            result.push(line.to_owned());
            i += 1;
            continue;
        };
        if masks[i].as_bytes().get(at + heading[1].len()) != Some(&b'#') {
            result.push(line.to_owned());
            i += 1;
            continue;
        }
        let first_line = if depth > 0
            && column == 0
            && !definition_hosts
                .get(&depth)
                .is_some_and(|held| heading[1].len() >= *held)
        {
            format!("{}{}", &line[..at], &line[at + heading[1].len()..])
        } else {
            column += heading[1].len();
            line.to_owned()
        };
        let marker = &heading[2];
        let mut parts = vec![first_line.trim_end_matches([' ', '\t'])];
        let mut tail = line;
        while i + 1 < lines.len() {
            if tail.bytes().rev().take_while(|b| *b == b'\\').count() % 2 == 1 {
                break;
            }
            let candidate = lines[i + 1];
            let prefix = quote_prefix.find(candidate).unwrap().as_str();
            if prefix.bytes().filter(|b| *b == b'>').count() != depth {
                break;
            }
            let remainder = &candidate[prefix.len()..];
            let indent = remainder
                .bytes()
                .take_while(|b| matches!(b, b' ' | b'\t'))
                .count();
            if indent < column {
                break;
            }
            let next = &remainder[indent..];
            if rejected_comment_starts_block(next) {
                break;
            }
            let part = if let Some(rest) = next
                .strip_prefix(marker)
                .filter(|rest| rest.starts_with([' ', '\t']))
            {
                let rest = rest.trim_start_matches([' ', '\t']);
                if rest.trim_matches([' ', '\t', '\r', '\n']).is_empty() {
                    break;
                }
                rest
            } else {
                if next.trim_matches([' ', '\t', '\r', '\n']).is_empty() || block.is_match(next) {
                    break;
                }
                next
            };
            parts.push(part.trim_end_matches([' ', '\t']));
            tail = part;
            i += 1;
        }
        result.push(if parts.len() == 1 {
            first_line
        } else {
            parts.join(" ")
        });
        i += 1;
    }
    result.join("\n")
}

/// Site generators conventionally remove a leading YAML envelope before Djot
/// sees the document. Migration has to do the same: feeding those bytes to a
/// Djot parser turns the opening line into a rule and `_x_` in a YAML scalar
/// into emphasis. The envelope is already valid Carve frontmatter, so keep it
/// byte-for-byte and import only the body.
fn split_frontmatter(source: &str) -> (&str, &str, &str) {
    let _paragraph_cache = ParagraphCacheScope::new(source);
    if !source.starts_with("---\n") {
        return ("", "", source);
    }
    let mut end = 4;
    for line in source[4..].split_inclusive('\n') {
        end += line.len();
        if line.trim_end_matches('\n') == "---" {
            let frontmatter_end = if source.as_bytes().get(end.saturating_sub(1)) == Some(&b'\n') {
                end - 1
            } else {
                end
            };
            let rest = &source[frontmatter_end..];
            let separator_len = if rest.starts_with("\n\n") {
                2
            } else if rest.starts_with('\n') {
                1
            } else {
                0
            };
            return (
                &source[..frontmatter_end],
                &rest[..separator_len],
                &rest[separator_len..],
            );
        }
    }
    ("", "", source)
}

fn leading_indent(line: &str) -> (usize, usize) {
    let mut columns = 0;
    let mut bytes = 0;
    for byte in line.bytes() {
        match byte {
            b' ' => columns += 1,
            b'\t' => columns += 4 - columns % 4,
            _ => break,
        }
        bytes += 1;
    }
    (columns, bytes)
}

fn bytes_through_columns(line: &str, wanted: usize) -> usize {
    let mut columns = 0;
    let mut bytes = 0;
    for byte in line.bytes() {
        if columns >= wanted {
            break;
        }
        match byte {
            b' ' => columns += 1,
            b'\t' => columns += 4 - columns % 4,
            _ => break,
        }
        bytes += 1;
    }
    bytes
}

fn definition_term(line: &str) -> Option<(usize, usize)> {
    let (columns, mut byte) = leading_indent(line);
    let bytes = line.as_bytes();
    if bytes.get(byte) != Some(&b':') {
        return None;
    }
    byte += 1;
    let whitespace = byte;
    while matches!(bytes.get(byte), Some(b' ' | b'\t')) {
        byte += 1;
    }
    (byte > whitespace || byte == bytes.len()).then_some((columns, byte))
}

mod definition_lists;
use definition_lists::*;

fn quote_prefix_len(line: &str) -> usize {
    let bytes = line.as_bytes();
    let mut accepted = 0;
    let mut cursor = 0;
    loop {
        let checkpoint = cursor;
        while matches!(bytes.get(cursor), Some(b' ' | b'\t')) {
            cursor += 1;
        }
        if bytes.get(cursor) != Some(&b'>') {
            return accepted;
        }
        cursor += 1;
        while matches!(bytes.get(cursor), Some(b' ' | b'\t')) {
            cursor += 1;
        }
        accepted = cursor;
        if cursor == checkpoint {
            return accepted;
        }
    }
}

fn list_marker(line: &str) -> bool {
    let trimmed = line.trim_start_matches([' ', '\t']);
    let thematic: Vec<_> = trimmed
        .bytes()
        .filter(|byte| !matches!(byte, b' ' | b'\t'))
        .collect();
    if thematic.len() >= 3
        && thematic
            .iter()
            .all(|byte| *byte == thematic[0] && matches!(*byte, b'*' | b'-'))
    {
        return false;
    }
    let Some((marker, rest)) = trimmed.split_once([' ', '\t']) else {
        return false;
    };
    if rest.trim().is_empty() {
        return false;
    }
    matches!(marker, "-" | "*" | "+" | ":")
        || marker.strip_suffix(['.', ')']).is_some_and(|value| {
            !value.is_empty() && value.chars().all(|ch| ch.is_ascii_alphanumeric())
        })
        || (marker.starts_with('(')
            && marker.ends_with(')')
            && marker[1..marker.len() - 1]
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric()))
}

fn normalize_djot_div_openers(source: &str) -> std::borrow::Cow<'_, str> {
    if !source.contains(":::") {
        return std::borrow::Cow::Borrowed(source);
    }
    let mut mask: Option<String> = None;
    let mut boundaries: Option<HashSet<usize>> = None;
    let opener = cached_regex!(r"^(:{3,})([ \t]*)([A-Za-z0-9_-]+)([ \t]*)(\r?)$").unwrap();
    let mut changed = false;
    let mut offset = 0;
    let lines: Vec<_> = source
        .split('\n')
        .map(|line| {
            let start = offset;
            offset += line.len() + 1;
            let at = djot_content_start(line);
            let prefix = &line[..at];
            if (line[at..].contains('{') || line[at..].contains('\\'))
                && cached_regex!(r"^:{3,}[ \t]*(?:[A-Za-z0-9_-]|\\[_-])+")
                    .unwrap()
                    .is_match(&line[at..])
                && !opener.is_match(&line[at..])
            {
                let mask = mask.get_or_insert_with(|| mask_code_and_destinations(source));
                if mask.as_bytes().get(start + at) == Some(&b':') {
                    changed = true;
                    return std::borrow::Cow::Owned(format!("{prefix}\\{}", &line[at..]));
                }
                return std::borrow::Cow::Borrowed(line);
            }
            if let Some(caps) = opener.captures(&line[at..]) {
                if &caps[2] != " " {
                    let mask = mask.get_or_insert_with(|| mask_code_and_destinations(source));
                    let boundaries = boundaries.get_or_insert_with(|| {
                        djot_inline_boundaries_with_code_scopes(source, mask, true)
                            .into_iter()
                            .collect()
                    });
                    if mask.as_bytes().get(start + at) == Some(&b':') && boundaries.contains(&start)
                    {
                        changed = true;
                        return std::borrow::Cow::Owned(format!(
                            "{}{} {}{}{}",
                            prefix, &caps[1], &caps[3], &caps[4], &caps[5]
                        ));
                    }
                }
            }
            std::borrow::Cow::Borrowed(line)
        })
        .collect();
    if changed {
        std::borrow::Cow::Owned(lines.join("\n"))
    } else {
        std::borrow::Cow::Borrowed(source)
    }
}

fn empty_djot_footnote_task_attributes(line: &str) -> bool {
    let Some(at) = line.find("[^") else {
        return false;
    };
    let Some(head) = cached_regex!(r"^\[\^[^\]\n]+\]:[ \t]+(.*)$")
        .unwrap()
        .captures(&line[at..])
    else {
        return false;
    };
    head[1].contains('{') && empty_djot_task_tail_end(&head[1], true).is_some()
}

fn empty_djot_task_tail_end(text: &str, allow_attributes: bool) -> Option<usize> {
    let marker = cached_regex!(r"^(?:[-*+]|(?:[0-9]+|[A-Za-z]|[ivxlcdm]+|[IVXLCDM]+)[.)]|\((?:[0-9]+|[A-Za-z]|[ivxlcdm]+|[IVXLCDM]+)\))[ \t]+").unwrap();
    let task = cached_regex!(r"^\[[ xX]\](?:[ \t]+|$)").unwrap();
    let mut at = 0;
    while at < text.len() {
        let prefix = marker.find(&text[at..])?;
        at += prefix.end();
        if matches!(prefix.as_str(), "- " | "* " | "+ ") {
            if let Some(checkbox) = task.find(&text[at..]) {
                at += checkbox.end();
                if at == text.len() {
                    return Some(at);
                }
                if allow_attributes
                    && read_djot_word_attributes(text, at)
                        .is_some_and(|(end, _)| text[end..].trim().is_empty())
                {
                    return Some(at);
                }
            }
        }
    }
    None
}

fn convert_djot_block_markers(source: &str) -> String {
    convert_djot_block_markers_with(source, false)
}

fn convert_djot_block_markers_with(source: &str, only_empty: bool) -> String {
    let _paragraph_cache = ParagraphCacheScope::new(source);
    let mut lines: Vec<String> = source.split('\n').map(str::to_owned).collect();
    let masked_source = mask_code_and_destinations(source);
    let masked: Vec<&str> = masked_source.split('\n').collect();
    let nested_lines = djot_nested_list_lines(&masked);
    let fenced_mask = mask_djot_fences(source, None, &[], true);
    let fenced_lines: Vec<_> = fenced_mask.split('\n').collect();
    let mut scan = BlockMarkerScan {
        masked: &masked,
        fenced_lines: &fenced_lines,
        only_empty,
        div_widths: Vec::new(),
        item_columns: Vec::new(),
        boundary: true,
        thematic_lines: HashSet::new(),
        quote_depth: 0,
        quote_exit_items: HashMap::new(),
        empty_task_breaks: HashMap::new(),
    };
    for index in 0..lines.len() {
        scan.scan_line(&mut lines, index);
    }
    if only_empty {
        return join_djot_marker_lines(lines, &scan.quote_exit_items, &scan.empty_task_breaks);
    }
    let rewritten = lines.join("\n");
    let masked_source = mask_code_and_destinations(&rewritten);
    let masked: Vec<&str> = masked_source.split('\n').collect();
    let mut containers: Vec<(usize, bool)> = Vec::new();
    for index in 0..lines.len() {
        rewrite_djot_block_line(
            &mut lines,
            &masked,
            index,
            &mut containers,
            &nested_lines,
            &scan.thematic_lines,
        );
    }
    join_djot_marker_lines(lines, &scan.quote_exit_items, &scan.empty_task_breaks)
}

fn djot_nested_list_lines(masked: &[&str]) -> Vec<bool> {
    let mut nested_lines = Vec::with_capacity(masked.len());
    let mut ancestors: Vec<(usize, bool)> = Vec::new();
    for line in masked {
        if line.trim().is_empty() {
            nested_lines.push(false);
            continue;
        }
        let (columns, _) = leading_indent(line);
        while ancestors.last().is_some_and(|top| top.0 >= columns) {
            ancestors.pop();
        }
        nested_lines.push(ancestors.last().is_some_and(|top| top.1));
        ancestors.push((columns, list_marker(line)));
    }
    nested_lines
}

fn djot_empty_marker_re() -> &'static regex::Regex {
    cached_regex!(
        r"^(?:[-*+](?: \[[ xX]\])?|(?:[0-9]+|[ivxlcdm]+|[IVXLCDM]+|[a-zA-Z])[.)]|\((?:[0-9]+|[A-Za-z]|[ivxlcdm]+|[IVXLCDM]+)\))[ \t]*$"
    )
    .unwrap()
}

fn djot_full_marker_re() -> &'static regex::Regex {
    cached_regex!(
        r"^(?:[-*+]|(?:[0-9]+|[ivxlcdm]+|[IVXLCDM]+|[a-zA-Z])[.)]|\((?:[0-9]+|[A-Za-z]|[ivxlcdm]+|[IVXLCDM]+)\))[ \t]+"
    )
    .unwrap()
}

fn djot_heading_re() -> &'static regex::Regex {
    cached_regex!(r"^#{1,6}(?:[ \t]|$)").unwrap()
}

fn quote_marker_depth(line: &str, prefix: usize) -> usize {
    line[..prefix].bytes().filter(|byte| *byte == b'>').count()
}

fn retain_items_before(item_columns: &mut Vec<(usize, usize)>, depth: usize, column: usize) {
    item_columns.retain(|&(held_depth, held_column)| {
        held_depth < depth || held_depth == depth && held_column < column
    });
}

struct BlockMarkerScan<'a> {
    masked: &'a [&'a str],
    fenced_lines: &'a [&'a str],
    only_empty: bool,
    div_widths: Vec<(usize, usize)>,
    item_columns: Vec<(usize, usize)>,
    boundary: bool,
    thematic_lines: HashSet<usize>,
    quote_depth: usize,
    quote_exit_items: HashMap<usize, String>,
    empty_task_breaks: HashMap<usize, String>,
}

impl BlockMarkerScan<'_> {
    fn scan_line(&mut self, lines: &mut [String], index: usize) {
        let masked = self.masked;
        let fenced_lines = self.fenced_lines;
        let (_, content_at) = leading_indent(&lines[index]);
        if masked[index].as_bytes().get(content_at) != lines[index].as_bytes().get(content_at) {
            self.boundary = fenced_lines[index].trim().is_empty();
            return;
        }
        let prefix = quote_prefix_len(&lines[index]);
        if prefix > 0
            && !fenced_lines[index].trim().is_empty()
            && !masked[index][..prefix].contains('>')
        {
            self.boundary = false;
            return;
        }
        let depth = quote_marker_depth(&lines[index], prefix);
        if !self.enter_quote(&lines[index], depth) {
            return;
        }
        let exits_quote = depth < self.quote_depth;
        if depth != self.quote_depth {
            self.boundary = true;
        }
        self.quote_depth = depth;
        let rest = &masked[index][prefix..];
        let (column, indent) = leading_indent(&lines[index][prefix..]);
        let content = &rest[indent..];
        let authored = &lines[index][prefix + indent..];
        if content.trim().is_empty() {
            self.boundary = authored.trim().is_empty()
                || (fenced_lines[index] != lines[index] && fenced_lines[index].trim().is_empty());
            return;
        }
        if self.closes_block(index, authored, content, depth, column) {
            return;
        }
        let sibling = self
            .item_columns
            .iter()
            .any(|&(held_depth, held_column)| held_depth == depth && held_column == column);
        let allow_task_attributes = lines
            .get(index + 1)
            .map_or(true, |line| line.trim().is_empty());
        let empty_task_end = empty_djot_task_tail_end(authored, allow_task_attributes);
        let empty_task = empty_task_end.is_some()
            && empty_djot_task_tail_end(content, allow_task_attributes).is_some();
        let empty_marker = djot_empty_marker_re();
        if (empty_marker.is_match(content) && empty_marker.is_match(authored) || empty_task)
            && (self.boundary || sibling)
        {
            if exits_quote {
                self.quote_exit_items
                    .insert(index, lines[index][..prefix].trim_end().to_owned());
            }
            let marker = empty_task_end
                .map_or(content, |end| &authored[..end])
                .trim_end();
            let marker = if marker == "+" && !self.only_empty {
                "-"
            } else {
                marker
            };
            if empty_task
                && lines
                    .get(index + 1)
                    .is_some_and(|next| empty_task_ends_before_text(next, depth, column))
            {
                self.empty_task_breaks
                    .insert(index, lines[index][..prefix].trim_end().to_owned());
            }
            lines[index] = format!("{}{}{} +", &lines[index][..prefix], &rest[..indent], marker);
            retain_items_before(&mut self.item_columns, depth, column);
            self.item_columns.push((depth, column));
            self.boundary = true;
        } else {
            if djot_full_marker_re().is_match(content) && (self.boundary || sibling) {
                retain_items_before(&mut self.item_columns, depth, column);
                self.item_columns.push((depth, column));
            } else if self.boundary {
                retain_items_before(&mut self.item_columns, depth, column);
            }
            self.boundary = false;
        }
    }

    /// Returns false when the line opens a quote that cannot start here.
    fn enter_quote(&mut self, line: &str, depth: usize) -> bool {
        let quote_owner_prefix = cached_regex!(r"^[ \t]*>(?:[ \t]|$)").unwrap();
        let mut owner_quote_columns = Vec::new();
        if depth > self.quote_depth {
            let mut rest = line;
            while let Some(prefix) = quote_owner_prefix.find(rest) {
                owner_quote_columns.push(leading_indent(rest).0);
                rest = &rest[prefix.end()..];
            }
        }
        let outside_item_quote = depth > self.quote_depth
            && self.item_columns.iter().any(|&(held_depth, column)| {
                owner_quote_columns
                    .get(held_depth)
                    .is_some_and(|&indent| indent <= column)
            });
        if outside_item_quote {
            self.item_columns.retain(|&(held_depth, column)| {
                !owner_quote_columns
                    .get(held_depth)
                    .is_some_and(|&indent| indent <= column)
            });
            self.boundary = true;
        }
        !(depth > self.quote_depth && !self.boundary)
    }

    /// Handles div fences, block attributes, headings and thematic breaks.
    fn closes_block(
        &mut self,
        index: usize,
        authored: &str,
        content: &str,
        depth: usize,
        column: usize,
    ) -> bool {
        let thematic = cached_regex!(r"^(?:(?:\*[ \t]*){3,}|(?:-[ \t]*){3,})$").unwrap();
        let colon_width = authored.bytes().take_while(|byte| *byte == b':').count();
        let div_close = colon_width >= 3
            && authored.trim_end().len() == colon_width
            && self.div_widths.last() == Some(&(depth, colon_width));
        if div_close || (self.boundary && colon_width >= 3) {
            if div_close {
                self.div_widths.pop();
            } else {
                self.div_widths.push((depth, colon_width));
            }
            self.boundary = true;
            retain_items_before(&mut self.item_columns, depth, column);
            return true;
        }
        if self.boundary
            && authored.starts_with('{')
            && read_djot_attributes(authored, 0, false, false)
                .is_some_and(|(end, _)| authored[end..].trim().is_empty())
        {
            return true;
        }
        let thematic_boundary = self.boundary
            || self
                .item_columns
                .iter()
                .any(|&(held_depth, held_column)| held_depth == depth && held_column == column);
        if djot_heading_re().is_match(content) || (thematic_boundary && thematic.is_match(content))
        {
            if thematic_boundary && thematic.is_match(content) {
                self.thematic_lines.insert(index);
            }
            self.boundary = true;
            retain_items_before(&mut self.item_columns, depth, column);
            return true;
        }
        false
    }
}

fn empty_task_ends_before_text(next: &str, depth: usize, column: usize) -> bool {
    let next_prefix = quote_prefix_len(next);
    let next_depth = quote_marker_depth(next, next_prefix);
    let (next_column, next_indent) = leading_indent(&next[next_prefix..]);
    let next_content = &next[next_prefix + next_indent..];
    next_depth == depth
        && next_column <= column
        && !next_content.trim().is_empty()
        && !djot_full_marker_re().is_match(next_content)
        && !djot_empty_marker_re().is_match(next_content)
        && !djot_heading_re().is_match(next_content)
        && !next_content.starts_with(['>', '{', '|'])
        && !next_content.starts_with(":::")
        && !cached_regex!(r"^\[[^\]\n]+\]:")
            .unwrap()
            .is_match(next_content)
}

fn rewrite_djot_block_line(
    lines: &mut [String],
    masked: &[&str],
    index: usize,
    containers: &mut Vec<(usize, bool)>,
    nested_lines: &[bool],
    thematic_lines: &HashSet<usize>,
) {
    let host_prefix = cached_regex!(r"^(?:[ \t]*> ?|[ \t]*(?:(?:[-*+]|(?:[0-9]+|[ivxlcdm]+|[IVXLCDM]+|[a-zA-Z])[.)]|\([0-9A-Za-z]+\)) +(?:\[[ xX]\] +)?|: |\[\^[^\]\r\n]+\]: +))").unwrap();
    let (_, content_at) = leading_indent(&lines[index]);
    if masked[index].as_bytes().get(content_at) != lines[index].as_bytes().get(content_at) {
        return;
    }

    let mut view = masked[index];
    while let Some(prefix) = host_prefix.find(view) {
        view = &view[prefix.end()..];
    }
    view = view.trim_start_matches([' ', '\t']);
    if view.starts_with('.')
        && view
            .as_bytes()
            .get(1)
            .map_or(true, |byte| matches!(byte, b' ' | b'\t' | b'{'))
    {
        let offset = masked[index].len() - view.len();
        lines[index].insert(offset, '\\');
        return;
    }
    let prefix = quote_prefix_len(masked[index]);
    let quote = &masked[index][..prefix];
    let rest = &masked[index][prefix..];
    let (_, indent_bytes) = leading_indent(rest);
    let content = &rest[indent_bytes..];
    if content.trim().is_empty() {
        return;
    }
    let mut container_view = lines[index].as_str();
    while let Some(host) = host_prefix.find(container_view) {
        container_view = &container_view[host.end()..];
    }
    container_view = container_view.trim_start_matches([' ', '\t']);
    let container_offset = lines[index].len() - container_view.len();
    let authored = &lines[index][container_offset..];
    let unmasked = masked[index][container_offset..].starts_with(":::");
    let width = if unmasked {
        crate::parse::lint_container_fence_width(authored)
    } else {
        None
    };
    let close = width.is_some()
        && authored
            .trim_end_matches([' ', '\t'])
            .bytes()
            .all(|b| b == b':')
        && containers.last().is_some_and(|top| Some(top.0) == width);
    let invalid = if close {
        containers.pop().unwrap().1
    } else {
        let invalid = unmasked && crate::parse::lint_invalid_container_metadata(authored);
        if let Some(width) = width {
            containers.push((width, invalid));
        }
        invalid
    };
    if invalid {
        let at = container_offset;
        lines[index].insert(at, '\\');
    }
    if let Some(close) = content.strip_prefix('(').and_then(|value| value.find(')')) {
        let token = &content[1..close + 1];
        let tail = &content[close + 2..];
        if !token.is_empty()
            && djot_full_marker_re().is_match(content)
            && tail.starts_with([' ', '\t'])
            && !tail.trim().is_empty()
        {
            let nested = quote.is_empty() && nested_lines[index];
            let authored = &lines[index];
            let authored_rest = &authored[prefix + indent_bytes..];
            lines[index] = format!(
                "{}{}{}.{}",
                quote,
                if nested { &rest[..indent_bytes] } else { "" },
                token,
                &authored_rest[close + 2..]
            );
            return;
        }
    }
    let mut marker = None;
    let mut count = 0;
    let mut valid = true;
    for byte in content.bytes() {
        if matches!(byte, b' ' | b'\t') {
            continue;
        }
        if !matches!(byte, b'*' | b'-') || marker.is_some_and(|seen| seen != byte) {
            valid = false;
            break;
        }
        marker = Some(byte);
        count += 1;
    }
    let authored_rule = cached_regex!(
        r"^((?:(?:[ \t]*>)+[ \t]*)?)([ \t]*)(?:\*(?:[ \t]*\*){2,}|-(?:[ \t]*-){2,})[ \t]*$"
    )
    .unwrap()
    .is_match(&lines[index]);
    if valid && count >= 3 && authored_rule && !thematic_lines.contains(&index) {
        lines[index].insert(prefix + indent_bytes, '\\');
    } else if valid && count >= 3 && authored_rule {
        let nested = quote.is_empty() && nested_lines[index];
        lines[index] = format!(
            "{}{}***",
            quote,
            if nested { &rest[..indent_bytes] } else { "" }
        );
    }
}

fn join_djot_marker_lines(
    lines: Vec<String>,
    quote_exit_items: &HashMap<usize, String>,
    empty_task_breaks: &HashMap<usize, String>,
) -> String {
    lines
        .into_iter()
        .enumerate()
        .map(|(index, line)| {
            let line = if let Some(prefix) = quote_exit_items.get(&index) {
                format!("{prefix}\n{line}")
            } else {
                line
            };
            if let Some(prefix) = empty_task_breaks.get(&index) {
                format!("{line}\n{prefix}")
            } else {
                line
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn mask_djot_attributes(source: &str) -> String {
    let _paragraph_cache = ParagraphCacheScope::new(source);
    static PATTERN: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let pattern = PATTERN.get_or_init(|| {
        let item = r#"(?:[.#][A-Za-z0-9_][A-Za-z0-9_-]*|[A-Za-z_][A-Za-z0-9_-]*=(?:"(?:\\.|[^"\\\n])*"|[A-Za-z0-9_:-]+))"#;
        regex::Regex::new(&format!(r"^\s*{item}(?:\s+{item})*\s*$")).unwrap()
    });
    let mut masked = mask_code_and_destinations(source).into_bytes();
    let bytes = source.as_bytes();
    let mut i = 0;
    let mut checked = 0;
    let mut block_start = true;
    let mut last_block_end = None;
    while i < bytes.len() {
        if bytes[i] != b'{' || masked[i] != b'{' || is_escaped(bytes, i) {
            i += 1;
            continue;
        }
        let mut end = i + 1;
        let mut quoted = false;
        while end < bytes.len() {
            match bytes[end] {
                b'\n' if quoted || blank_line_follows(bytes, end) => break,
                b'\\' => end += 1,
                b'"' => quoted = !quoted,
                b'{' | b'}' if !quoted => break,
                _ => {}
            }
            end += 1;
        }
        for byte in &bytes[checked..i] {
            if *byte == b'\n' {
                block_start = true;
            } else if !matches!(*byte, b' ' | b'\t') {
                block_start = false;
            }
        }
        checked = i;
        let attached_span = i > 0
            && matches!(bytes[i - 1], b']' | b')')
            && !bytes[i..end.min(bytes.len())].contains(&b'\n');
        let attached_block = if block_start {
            let after = source.get(end + 1..).unwrap_or("");
            let (line_tail, following) = after.split_once('\n').unwrap_or((after, ""));
            let boundary = match source[..i].rfind('\n') {
                None => true,
                Some(previous_end) => {
                    let previous_start = source[..previous_end].rfind('\n').map_or(0, |at| at + 1);
                    source[previous_start..previous_end].trim().is_empty()
                        || last_block_end
                            .is_some_and(|end| end >= previous_start && end < previous_end)
                }
            };
            boundary
                && line_tail.trim().is_empty()
                && following
                    .split('\n')
                    .next()
                    .is_some_and(|line| !line.trim().is_empty())
        } else {
            false
        };
        if end < bytes.len()
            && bytes[end] == b'}'
            && (attached_span || attached_block)
            && pattern.is_match(&source[i + 1..end])
        {
            for byte in &mut masked[i..=end] {
                if *byte != b'\n' {
                    *byte = 0;
                }
            }
            if attached_block {
                last_block_end = Some(end);
            }
            i = end + 1;
        } else {
            i = end.max(i + 1);
        }
    }
    String::from_utf8(masked).expect("attribute masks preserve UTF-8 boundaries")
}

struct OrphanAttributeSpans {
    prefix: String,
    values: Vec<String>,
}

impl OrphanAttributeSpans {
    fn restore_with(&self, source: &str, mut transform: impl FnMut(&str) -> String) -> String {
        if self.values.is_empty() {
            return transform(source);
        }
        let pattern =
            regex::Regex::new(&format!(r"{}([0-9]+)\x00", regex::escape(&self.prefix))).unwrap();
        let mut result = String::with_capacity(source.len());
        let mut end = 0;
        for caps in pattern.captures_iter(source) {
            let token = caps.get(0).unwrap();
            result.push_str(&transform(&source[end..token.start()]));
            result.push_str(
                caps[1]
                    .parse::<usize>()
                    .ok()
                    .and_then(|index| self.values.get(index))
                    .map_or(token.as_str(), String::as_str),
            );
            end = token.end();
        }
        result.push_str(&transform(&source[end..]));
        result
    }
}

fn consume_orphan_djot_attributes(source: &str) -> (String, OrphanAttributeSpans) {
    if !source.contains('{') {
        return (
            source.to_owned(),
            OrphanAttributeSpans {
                prefix: String::new(),
                values: Vec::new(),
            },
        );
    }
    let _paragraph_cache = ParagraphCacheScope::new(source);
    static PATTERN: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let pattern = PATTERN.get_or_init(|| {
        let item = r#"(?:[.#][A-Za-z0-9_:-]+|[A-Za-z][A-Za-z0-9_-]*=(?:"(?:\\.|[^"\\\n])*"|[A-Za-z0-9_:-]+))"#;
        regex::Regex::new(&format!(r"\{{[ \t]*{item}(?:[ \t]+{item})*[ \t]*\}}")).unwrap()
    });
    let mut masked = mask_djot_attributes(source).into_bytes();
    for note in cached_regex!(r"\[\^[^\]\n]*\]").unwrap().find_iter(source) {
        if !is_escaped(source.as_bytes(), note.start()) {
            masked[note.range()].fill(b' ');
        }
    }
    let masked = String::from_utf8(masked).expect("footnote masks preserve UTF-8");
    let uri = cached_regex!(r"<[A-Za-z][A-Za-z0-9+.-]*:[^<>\s]*>").unwrap();
    let masked = uri.replace_all(&masked, |caps: &regex::Captures<'_>| {
        " ".repeat(caps[0].len())
    });
    let masked_lines: Vec<&str> = masked.split('\n').collect();
    let table_rows = if source.contains('|') {
        djot_table_rows(source, &masked)
    } else {
        Vec::new()
    };
    let mut spans = OrphanAttributeSpans {
        prefix: djot_placeholder_prefix(source, "\0DJOTORPHAN\0"),
        values: Vec::new(),
    };
    let block_end = cached_regex!(
        r"^(?:`{3,}|~{3,}|:{3,}|#{1,6} |:{1,2} |(?:\*[ \t]*){3,}|(?:-[ \t]*){3,}|\|.*\||\[[^\]^][^\]]*\]:|\[\^[^\]]+\]:[ \t]*$)")
    .unwrap();
    let quote_prefix = cached_regex!(r"^(?:[ \t]*>[ \t]*)*[ \t]*").unwrap();
    let marker = cached_regex!(
        r"^(?:[ \t]*>[ \t]*)*[ \t]*(?:[-*+]|[0-9]+[.)]|#{1,6}|:{1,2}|\[\^[^\]]+\]:)[ \t]+(?:\[[ xX-]\][ \t]+)?$")
    .unwrap();
    let source_lines: Vec<&str> = source.split('\n').collect();
    let mut dangling = vec![false; source_lines.len()];
    let mut follows_blank = true;
    let mut next_depth = None;
    for index in (0..source_lines.len()).rev() {
        let line = source_lines[index];
        let first = quote_prefix.find(line).unwrap().end();
        let depth = line[..first].bytes().filter(|&byte| byte == b'>').count();
        if line[first..].trim_matches(DJOT_LINE_SPACING).is_empty() {
            if depth != 0 {
                follows_blank = true;
                next_depth = Some(depth);
            }
            continue;
        }
        let visible = masked_lines[index].as_bytes().get(first);
        let attribute = matches!(visible, Some(b'{' | 0))
            && pattern.find(line).is_some_and(|attrs| {
                attrs.start() == first
                    && attrs.end() == line.trim_end_matches(DJOT_LINE_SPACING).len()
            });
        if attribute {
            if next_depth.is_some_and(|next| next != depth) {
                follows_blank = false;
            }
            dangling[index] = follows_blank;
            next_depth = Some(depth);
        } else {
            follows_blank = false;
            next_depth = None;
        }
    }
    let list_marker = cached_regex!(r"(?:[-*+] |[0-9A-Za-z]+[.)] |\([0-9A-Za-z]+\) )").unwrap();
    let mut block_attributes = vec![false; source_lines.len()];
    let mut quote_scope_changes = vec![false; source_lines.len()];
    let mut target_prefixes = vec![""; source_lines.len()];
    let mut previous_depth = 0;
    let mut previous_nonblank_depth = 0;
    let mut block_owners: Vec<(usize, usize)> = Vec::new();
    let owner_marker =
        cached_regex!(r"(?:\[\^[^\]\n]+\]:|[-*+]|[0-9A-Za-z]+[.)]|\([0-9A-Za-z]+\))[ \t]+")
            .unwrap();
    for (index, line) in source_lines.iter().enumerate() {
        let first = quote_prefix.find(line).unwrap().end();
        let body = line[first..].trim_end_matches(DJOT_LINE_SPACING);
        let depth = line[..first].bytes().filter(|&byte| byte == b'>').count();
        if body.trim_matches(DJOT_LINE_SPACING).is_empty() && depth <= previous_depth {
            previous_depth = previous_depth.min(depth);
            continue;
        }
        let opens = depth > previous_depth;
        quote_scope_changes[index] = opens || depth != previous_nonblank_depth;
        previous_nonblank_depth = depth;
        let owner_end = if opens {
            line[..first]
                .match_indices('>')
                .nth(previous_depth)
                .map_or(first, |(at, _)| at)
        } else {
            first
        };
        target_prefixes[index] = &line[..owner_end];
        previous_depth = depth;
        let previous_line = source_lines
            .get(index.wrapping_sub(1))
            .copied()
            .unwrap_or("");
        let previous_body = quote_prefix.replace(previous_line, "");
        let block_owner = block_owners.last().copied();
        let owner_outdent = block_owner.is_some_and(|(start, owner_depth)| {
            depth < owner_depth || depth == owner_depth && first < start
        });
        let can_open_owner = index == 0
            || previous_body.trim_matches(DJOT_LINE_SPACING).is_empty()
            || owner_outdent
            || index > 0 && block_attributes[index - 1];
        let row_end = line.trim_end().len().saturating_sub(1);
        let exits_at_row = owner_outdent
            && body.starts_with('|')
            && row_end > first
            && masked_lines[index].as_bytes().get(first) == Some(&b'|')
            && masked_lines[index].as_bytes().get(row_end) == Some(&b'|')
            && line.as_bytes().get(row_end - 1) != Some(&b'\\');
        let closes_owner = exits_at_row
            || cached_regex!(r"^(?:#{1,6}(?:[ \t]|$)|:{3,}|[`~]{3,}|(?:[-*][ \t]*){3,}$)")
                .unwrap()
                .is_match(body)
                && masked_lines[index].as_bytes().get(first) != Some(&b' ')
            || table_rows.get(index) == Some(&true)
            || cached_regex!(r"^(?:`{3,}|~{3,})[ \t]*=?[A-Za-z0-9_+#.-]*[ \t]*$")
                .unwrap()
                .is_match(body);
        if previous_body.trim_matches(DJOT_LINE_SPACING).is_empty()
            || pattern.find(line).is_some_and(|attrs| {
                attrs.start() == first
                    && attrs.end() == line.trim_end_matches(DJOT_LINE_SPACING).len()
            })
            || closes_owner
            || owner_outdent && table_rows.get(index.wrapping_sub(1)) == Some(&true)
        {
            while block_owners.last().is_some_and(|&(start, owner_depth)| {
                depth < owner_depth || depth == owner_depth && first < start
            }) {
                block_owners.pop();
            }
        }
        let content_start = djot_content_start(line);
        if can_open_owner {
            if let Some(marker) = owner_marker.find(&line[..content_start]) {
                if masked_lines[index].as_bytes().get(marker.start()) != Some(&b' ') {
                    let owner = (marker.start() + 1, depth);
                    if block_owners.last() != Some(&owner) {
                        block_owners.push(owner);
                    }
                }
            }
        }
        if !matches!(masked_lines[index].as_bytes().get(first), Some(b'{' | 0))
            || !pattern.find(line).is_some_and(|attrs| {
                attrs.start() == first
                    && attrs.end() == line.trim_end_matches(DJOT_LINE_SPACING).len()
            })
        {
            continue;
        }
        let previous = source_lines
            .get(index.wrapping_sub(1))
            .copied()
            .unwrap_or("");
        let previous_start = djot_content_start(previous);
        let note_at = previous[..previous_start].find("[^");
        let owner_start = note_at.map_or(previous_start, |at| at + 1);
        let previous_quotes = quote_prefix.find(previous).unwrap().as_str();
        let outdented = depth < previous_quotes.bytes().filter(|&byte| byte == b'>').count()
            || first < owner_start
                && (note_at.is_some() || list_marker.is_match(&previous[..previous_start]));
        let previous = quote_prefix.replace(previous, "");
        block_attributes[index] = index == 0
            || previous.trim_matches(DJOT_LINE_SPACING).is_empty()
            || outdented
            || owner_outdent
            || (index > 0 && block_attributes[index - 1])
            || block_end.is_match(previous.trim());
    }
    let mut next_block = vec![None; source_lines.len()];
    let mut following = None;
    for (index, line) in source_lines.iter().enumerate().rev() {
        next_block[index] = following;
        let first = quote_prefix.find(line).unwrap().end();
        let attribute = matches!(masked_lines[index].as_bytes().get(first), Some(b'{' | 0))
            && pattern.find(line).is_some_and(|attrs| {
                attrs.start() == first
                    && attrs.end() == line.trim_end_matches(DJOT_LINE_SPACING).len()
            });
        if quote_scope_changes[index]
            || !line[first..].trim_matches(DJOT_LINE_SPACING).is_empty() && !attribute
        {
            following = Some(index);
        }
    }
    let mut moved: Vec<Vec<String>> = vec![Vec::new(); source_lines.len()];
    let mut lines = Vec::new();
    for (index, line) in source.split('\n').enumerate() {
        let mask = masked_lines[index].as_bytes();
        let bytes = line.as_bytes();
        let first = quote_prefix.find(line).unwrap().end();
        let mut output = String::new();
        let mut cursor = 0;
        let mut drop_line = false;
        let stripped_line = pattern.replace_all(line, "");
        let marker_only = marker.is_match(&stripped_line);
        for attrs in pattern.find_iter(line) {
            let at = attrs.start();
            if at < cursor
                || (mask[at] != b'{'
                    && !(dangling[index]
                        && at == first
                        && attrs.end() == line.trim_end_matches(DJOT_LINE_SPACING).len()))
                || is_escaped(bytes, at)
            {
                continue;
            }
            if at > 0
                && at != cursor
                && (b"]*_}^~".contains(&bytes[at - 1])
                    || (mask[at - 1] == b' ' && bytes[at - 1] != b' '))
            {
                continue;
            }
            if marker_only {
                continue;
            }
            let alone =
                at == first && attrs.end() == line.trim_end_matches(DJOT_LINE_SPACING).len();
            let block = block_attributes[index];
            if alone && block {
                if !dangling[index] {
                    if let Some(target) = next_block[index] {
                        let target_prefix = target_prefixes[target];
                        if line[..first].bytes().filter(|&byte| byte == b'>').count()
                            > target_prefix.bytes().filter(|&byte| byte == b'>').count()
                        {
                            if target_prefixes[index] == &line[..first] {
                                let pending = std::mem::take(&mut moved[index]);
                                moved[target].extend(pending);
                            }
                            moved[target].push(attrs.as_str().to_owned());
                            drop_line = true;
                            cursor = attrs.end();
                        }
                    }
                    continue;
                }
                drop_line = true;
                if target_prefixes[index] == &line[..first] {
                    moved[index].clear();
                }
            }
            output.push_str(&line[cursor..at]);
            cursor = attrs.end();
            let end = cursor + line[cursor..].len()
                - line[cursor..].trim_start_matches([' ', '\t']).len();
            let space = &line[cursor..end];
            let value = if at != first {
                space.to_string()
            } else if space.is_empty() {
                "{%%}".to_owned()
            } else {
                format!("!`{space}`")
            };
            output.push_str(&format!("{}{}\0", spans.prefix, spans.values.len()));
            spans.values.push(value);
            cursor = end;
        }
        output.push_str(&line[cursor..]);
        for pending in moved[index].drain(..) {
            lines.push(format!("{}{pending}", target_prefixes[index]));
        }
        if !drop_line {
            lines.push(output);
        } else if line[..first].contains('>') {
            lines.push(line[..first].trim_end_matches(DJOT_LINE_SPACING).to_owned());
        }
    }
    (lines.join("\n"), spans)
}

mod inline_masks;
use inline_masks::*;

/// Collapse a blank-line run that Carve alone would read as a list boundary.
///
/// Djot reads ANY run of blank lines between two compatible sibling markers as
/// one list. Carve reads a run of THREE OR MORE as a hard boundary (PART 9 §11
/// N1a) and opens a second list after it. So passing the author's run through
/// splits a list the source never split, and it does it silently: the halves
/// render as `</ul><ul>`, which shows nothing at all for a bullet list, and on
/// an ordered list restarts the numbering.
///
/// A run of three or more blank lines before a list-marker line therefore
/// collapses to ONE blank line, which is how Carve spells what the Djot source
/// said - one loose list. Other runs are left alone: between two paragraphs the
/// count means nothing in either language, and rewriting it would edit layout
/// the author chose for no gain.
///
/// The run must also FOLLOW a list, so the rewrite fires only where the two
/// languages disagree. Blank lines inside a fenced block are that block's
/// content and are skipped.
fn collapse_false_list_boundaries(source: &str) -> String {
    let _paragraph_cache = ParagraphCacheScope::new(source);
    let lines: Vec<&str> = source.split('\n').collect();
    let fenced = fenced_lines(&lines);

    let mut out: Vec<&str> = Vec::with_capacity(lines.len());
    let mut i = 0;
    while i < lines.len() {
        let run_start = i;
        let depth = quoted(lines[i]).0;
        while i < lines.len() && !fenced[i] && is_blank(lines[i]) && quoted(lines[i]).0 == depth {
            i += 1;
        }
        let run = i - run_start;
        let above = out.iter().rev().find(|line| !is_blank(line));
        if run >= 3
            && i < lines.len()
            && at_the_same_depth(lines[i], depth, opens_a_list_item)
            && above.is_some_and(|line| at_the_same_depth(line, depth, continues_a_list))
        {
            // The run's own first line, so a quoted run keeps its `>` prefix
            // and an unquoted one stays the empty line it already was.
            out.push(lines[run_start]);
            continue;
        }
        if run > 0 {
            out.extend_from_slice(&lines[run_start..i]);
            continue;
        }
        out.push(lines[i]);
        i += 1;
    }

    out.join("\n")
}

/// A line's block-quote depth and what it holds inside the quote markers. The
/// boundary applies inside a quote as much as outside it, and there a "blank"
/// line is written `>` -- so both the run and the markers around it are read
/// through the prefix rather than off the raw line.
fn quoted(line: &str) -> (usize, &str) {
    let mut rest = line;
    let mut depth = 0;
    loop {
        let trimmed = rest.trim_start_matches([' ', '\t']);
        match trimmed.strip_prefix('>') {
            Some(after) if after.is_empty() || after.starts_with([' ', '\t']) => {
                depth += 1;
                rest = after.strip_prefix([' ', '\t']).unwrap_or(after);
            }
            // The remainder is NOT trimmed: an item's own indentation is what
            // says the line continues the item, and inside a quote it sits
            // right after the marker.
            _ => return (depth, rest),
        }
    }
}

/// Whether a line carries no content, inside whatever quote holds it.
fn is_blank(line: &str) -> bool {
    quoted(line).1.trim().is_empty()
}

/// Whether a line sits at `depth` and its content answers `test`. A marker one
/// quote level away from the run separates nothing the run could join.
fn at_the_same_depth(line: &str, depth: usize, test: fn(&str) -> bool) -> bool {
    let (line_depth, content) = quoted(line);

    line_depth == depth && test(content)
}

/// Which lines sit inside a fenced block, whose blank lines are content.
fn fenced_lines(lines: &[&str]) -> Vec<bool> {
    let mut inside: Vec<bool> = Vec::with_capacity(lines.len());
    let mut open: Option<(char, usize)> = None;
    for line in lines {
        // Through the quote prefix: a fence opened inside a quote holds its
        // blank lines as content exactly like an unquoted one.
        let trimmed = quoted(line).1.trim_start();
        let first = trimmed.chars().next();
        let run = match first {
            Some(c @ ('`' | '~')) => trimmed.chars().take_while(|x| *x == c).count(),
            _ => 0,
        };
        match open {
            Some((fence_char, open_run)) => {
                inside.push(true);
                if run >= open_run && first == Some(fence_char) {
                    open = None;
                }
            }
            None => {
                if run >= 3 {
                    open = Some((first.expect("a run implies a character"), run));
                    inside.push(true);
                } else {
                    inside.push(false);
                }
            }
        }
    }

    inside
}

/// Whether a line is a list-item marker line: a bullet or an ordered marker
/// followed by content. A marker with nothing after it is not an item.
fn opens_a_list_item(line: &str) -> bool {
    let trimmed = line.trim_start();
    let mut chars = trimmed.chars();
    match chars.next() {
        Some('-' | '*' | '+') => {
            matches!(chars.next(), Some(' ' | '\t')) && !chars.as_str().trim().is_empty()
        }
        Some(c) if c.is_ascii_alphanumeric() => {
            let head: String = trimmed
                .chars()
                .take_while(|x| x.is_ascii_alphanumeric())
                .collect();
            let rest = &trimmed[head.len()..];
            let mut rest = rest.chars();
            matches!(rest.next(), Some('.' | ')'))
                && matches!(rest.next(), Some(' ' | '\t'))
                && !rest.as_str().trim().is_empty()
        }
        _ => false,
    }
}

/// Whether a list is open above the run: the nearest line with content is
/// either a marker line or an item's indented content. Djot has no indented code blocks,
/// so an indented line under a list is that list's content and nothing else.
fn continues_a_list(line: &str) -> bool {
    opens_a_list_item(line) || line.starts_with(' ') || line.starts_with('\t')
}

/// Rewrite a Djot `+` bullet marker to `-`.
///
/// Djot allows `-`, `*` and `+` as bullets. Carve has no `+` bullet: `+` is the
/// list-continuation marker, so a Djot `+` list degrades to a paragraph - the
/// line stops being a list item at all, which is a structural change and not a
/// delimiter one.
///
/// A LONE `+` is left alone. That is the Carve continuation marker itself, and
/// a marker with no content is exactly the form that means it, so rewriting it
/// would break the construct this rule exists to protect.
///
/// Lines inside a fenced block are skipped via the code mask. The rewrite is
/// one byte for one byte, so it runs before the mask is taken for the inline
/// rules and leaves every later offset valid.
fn normalize_plus_bullets(source: &str) -> String {
    let _paragraph_cache = ParagraphCacheScope::new(source);
    let masked = mask_code_and_destinations(source);
    let mask = masked.as_bytes();
    let mut out = source.as_bytes().to_vec();
    let continuation_lines = if masked
        .lines()
        .any(|line| line.trim_start_matches([' ', '\t']).starts_with('+') && line.contains('|'))
    {
        table_continuation_lines(source)
    } else {
        HashSet::new()
    };

    let mut line_start = 0;
    let mut line_number = 1;
    while line_start <= out.len() {
        let line_end = mask[line_start..]
            .iter()
            .position(|b| *b == b'\n')
            .map(|n| line_start + n)
            .unwrap_or(out.len());

        // A masked line is code: its `+` is content, not a marker.
        let mut i = line_start + quote_prefix_len(&masked[line_start..line_end]);
        while i < line_end && (mask[i] == b' ' || mask[i] == b'\t') {
            i += 1;
        }

        if i < line_end && mask[i] == b'+' && !continuation_lines.contains(&line_number) {
            // `+ content` is a bullet; a bare `+` (or `+` then only spaces) is
            // the continuation marker and stays.
            let mut j = i + 1;
            let mut spaced = false;
            while j < line_end && (mask[j] == b' ' || mask[j] == b'\t') {
                spaced = true;
                j += 1;
            }
            if spaced && j < line_end {
                out[i] = b'-';
            }
        }

        if line_end >= out.len() {
            break;
        }
        line_start = line_end + 1;
        line_number += 1;
    }

    String::from_utf8(out).expect("one-byte substitution preserves UTF-8")
}

fn table_continuation_lines(source: &str) -> HashSet<usize> {
    if !source.contains('|') {
        return HashSet::new();
    }
    let _paragraph_cache = ParagraphCacheScope::new(source);
    fn visit(blocks: &[BlockNode], lines: &mut HashSet<usize>) {
        for block in blocks {
            match block {
                BlockNode::Table(table) => visit_table(table, lines),
                BlockNode::List(list) => {
                    for item in &list.items {
                        visit(&item.children, lines);
                    }
                }
                BlockNode::BlockQuote(node) => visit(&node.children, lines),
                BlockNode::Admonition(node) => visit(&node.children, lines),
                BlockNode::Directive(node) => visit(&node.children, lines),
                BlockNode::Div(node) => visit(&node.children, lines),
                BlockNode::Section(node) => visit(&node.children, lines),
                BlockNode::LineBlock(node) => visit(&node.children, lines),
                BlockNode::DefinitionList(list) => {
                    for item in &list.items {
                        for definition in &item.definitions {
                            visit(&definition.children, lines);
                        }
                    }
                }
                BlockNode::Figure(figure) => match &*figure.target {
                    FigureTarget::Table(table) => visit_table(table, lines),
                    FigureTarget::BlockQuote(node) => visit(&node.children, lines),
                    _ => {}
                },
                BlockNode::FigureGroup(group) => visit(&group.children, lines),
                BlockNode::ExtensionCarrier(extension) => visit(&extension.children, lines),
                _ => {}
            }
        }
    }

    fn visit_table(table: &crate::ast::Table, lines: &mut HashSet<usize>) {
        for row in &table.rows {
            if let Some(pos) = row.pos.clone() {
                lines.extend((pos.start_line + 1)..=pos.end_line);
            }
            for cell in &row.cells {
                if let Some(blocks) = &cell.blocks {
                    visit(blocks, lines);
                }
            }
        }
    }

    let mut lines = HashSet::new();
    let options = crate::Options::default().with_positions(true);
    visit(
        &crate::parse_with_options(source, &options).children,
        &mut lines,
    );
    lines
}

mod escaping;
use escaping::*;
pub(crate) use escaping::{escape_plain_carve_syntax, HandledDelimiters};
#[cfg(test)]
#[path = "djot_migrate/tests.rs"]
mod tests;

/// The shared escaper corpus (`tests/spec/tests/corpus-escape/`), read directly
/// against the escaper rather than end to end through a converter.
///
/// Its module declaration stays beside the function because the function is
/// crate-internal: it is one rule with one implementation, not public API.
/// Before markup-carve/carve-rs#995 the only way to reach it from outside was
/// `carve migrate --from djot`, which can only probe inputs that are INERT in
/// Djot - 46 of the corpus's 55, since the other 9 are Djot markup and what
/// came back was the converter doing its job rather than the escaper.
#[cfg(test)]
#[path = "djot_migrate/escape_corpus.rs"]
mod escape_corpus;

#[cfg(test)]
#[path = "djot_migrate/attributed_strong_tests.rs"]
mod attributed_strong_tests;

#[cfg(test)]
#[path = "djot_migrate/attribute_list_tests.rs"]
mod attribute_list_tests;

#[cfg(test)]
#[path = "djot_migrate/heading_continuation_tests.rs"]
mod heading_continuation_tests;

fn djot_inline_boundaries(source: &str, mask: &str) -> Vec<usize> {
    let _paragraph_cache = ParagraphCacheScope::new(source);
    djot_inline_boundaries_with_code_scopes(source, mask, false)
}

fn djot_inline_boundaries_with_code_scopes(
    source: &str,
    mask: &str,
    code_scopes: bool,
) -> Vec<usize> {
    djot_inline_boundaries_with_div_closers(source, mask, code_scopes, None, None)
}

fn djot_inline_boundaries_with_div_closers(
    source: &str,
    mask: &str,
    code_scopes: bool,
    mut div_closers: Option<&mut HashMap<usize, Vec<usize>>>,
    mut div_owners: Option<&mut HashMap<usize, (Option<usize>, usize)>>,
) -> Vec<usize> {
    let rows = djot_table_rows(source, mask);
    let quote = cached_regex!(r"^(?:[ \t]*>(?:[ \t]|$))*").unwrap();
    let marker = cached_regex!(r"^(?:\[\^[^\]\n]+\]:[ \t]*|(?:[-*+]|(?:[0-9]+|[A-Za-z]|[ivxlcdm]+|[IVXLCDM]+)[.)]|\((?:[0-9]+|[A-Za-z]|[ivxlcdm]+|[IVXLCDM]+)\)|:)[ \t]+)").unwrap();
    let scope_block =
        cached_regex!(r"^(?:#{1,6}(?: |$)|:{3,}|[`~]{3,}|(?:[*-][ \t]*){3,}$)").unwrap();
    let scope_quote = cached_regex!(r"^(?:>[ \t]+)*").unwrap();
    let scope_ticks = cached_regex!(r"^(`{3,}|~{3,})(.*)$").unwrap();
    let scope_div = cached_regex!(r"^(:{3,})(?:[ \t]*[A-Za-z0-9_-]+)?[ \t]*$").unwrap();
    let scope_heading =
        cached_regex!(r"^(?:#{1,6}(?: |$)|(?:[*-][ \t]*){3,}$|\[[^\]]+\]:)").unwrap();
    let mut boundaries = Vec::new();
    let mut items: Vec<(usize, usize)> = Vec::new();
    #[derive(Clone, Copy)]
    struct DivOwner {
        minimum_width: usize,
        scope_start: usize,
        start: usize,
        depth: usize,
        item_column: Option<usize>,
        item_depth: usize,
    }
    let mut divs: Vec<DivOwner> = Vec::new();
    let mut previous_block = true;
    let mut previous_depth = 0;
    let mut code_fence: Option<(u8, usize, usize, Option<usize>)> = None;
    let mut offset = 0;
    let mut previous_blank = true;
    for (row, raw_line) in source.split('\n').enumerate() {
        let line = if code_scopes {
            raw_line.strip_suffix('\r').unwrap_or(raw_line)
        } else {
            raw_line
        };
        let prefix = quote.find(line).unwrap().as_str();
        let depth = prefix.bytes().filter(|ch| *ch == b'>').count();
        let content = &line[prefix.len()..];
        let trimmed = content.trim_start_matches([' ', '\t']);
        let indent = content.len() - trimmed.len();
        let item = marker.find(trimmed);
        let starts_item = item.is_some()
            && (previous_blank
                || code_scopes && previous_block
                || items
                    .last()
                    .is_some_and(|&(column, item_depth)| depth <= item_depth && indent < column));
        let blank = trimmed.is_empty() || trimmed == "\r";
        if code_scopes {
            if previous_block && !blank && !starts_item {
                while items
                    .last()
                    .is_some_and(|&(column, item_depth)| depth == item_depth && indent < column)
                {
                    items.pop();
                }
            }
            let block_start = scope_block.is_match(trimmed) || item.is_some();
            while let Some(owner) = divs.last() {
                let outside_quote =
                    depth < owner.depth && (blank || previous_blank || starts_item || block_start);
                let outside_item = owner.item_column.is_some_and(|column| {
                    depth == owner.item_depth
                        && indent < column
                        && !blank
                        && (previous_blank || starts_item || block_start)
                });
                if !outside_quote && !outside_item {
                    break;
                }
                divs.pop();
            }
            let marker_width = if starts_item {
                item.as_ref().unwrap().as_str().len()
            } else {
                0
            };
            let mut body = &trimmed[marker_width..];
            if starts_item
                && marker_width == 2
                && trimmed.as_bytes()[1] == b' '
                && matches!(trimmed.as_bytes()[0], b'-' | b'*' | b'+')
            {
                if let Some(task) = cached_regex!(r"^\[[ xX]\][ \t]+").unwrap().find(body) {
                    body = &body[task.end()..];
                }
            }

            let padding = body.len() - body.trim_start_matches([' ', '\t']).len();
            body = &body[padding..];
            let nested_quote = scope_quote.find(body).unwrap().as_str();
            let body_depth = depth + nested_quote.bytes().filter(|&ch| ch == b'>').count();
            body = &body[nested_quote.len()..];
            let can_open = previous_block
                || previous_blank
                || starts_item
                || (block_start
                    && (depth < previous_depth
                        || items.last().is_some_and(|&(column, item_depth)| {
                            depth == item_depth && indent < column
                        })));
            if code_fence.is_some_and(|(_, _, fence_depth, item_column)| {
                depth < fence_depth
                    || item_column
                        .is_some_and(|column| depth == fence_depth && indent < column && !blank)
            }) {
                code_fence = None;
                while items
                    .last()
                    .is_some_and(|&(column, item_depth)| depth == item_depth && indent < column)
                {
                    items.pop();
                }
            }
            let ticks = scope_ticks.captures(body);
            if let Some((ch, width, fence_depth, _)) = code_fence {
                if ticks.as_ref().is_some_and(|t| {
                    t[1].as_bytes()[0] == ch
                        && t[1].len() >= width
                        && t[2].trim_matches([' ', '\t']).is_empty()
                        && body_depth == fence_depth
                }) {
                    code_fence = None;
                    previous_block = true;
                } else {
                    previous_block = false;
                }
            } else if ticks
                .as_ref()
                .is_some_and(|t| can_open && (t[1].as_bytes()[0] != b'`' || !t[2].contains('`')))
            {
                let ticks = ticks.unwrap();
                let item_column = if starts_item {
                    Some(indent + marker_width)
                } else {
                    items
                        .last()
                        .filter(|&&(column, item_depth)| depth == item_depth && indent >= column)
                        .map(|&(column, _)| column)
                };
                code_fence = Some((
                    ticks[1].as_bytes()[0],
                    ticks[1].len(),
                    body_depth,
                    item_column,
                ));
                previous_block = false;
            } else {
                let fence = scope_div.captures(body);
                let owner = divs.last().copied();
                let visible = mask.as_bytes().get(offset + line.len() - body.len()) == Some(&b':');
                if fence.as_ref().zip(owner).is_some_and(|(f, owner)| {
                    visible
                        && body[f[1].len()..].trim_matches([' ', '\t']).is_empty()
                        && body_depth == owner.depth
                        && f[1].len() >= owner.minimum_width
                }) {
                    boundaries.push(offset);
                    let width = fence.unwrap()[1].len();
                    let mut low = owner.unwrap().scope_start;
                    let mut high = divs.len() - 1;
                    while low < high {
                        let mid = (low + high) / 2;
                        if divs[mid].minimum_width <= width {
                            high = mid;
                        } else {
                            low = mid + 1;
                        }
                    }
                    if let Some(closers) = div_closers.as_mut() {
                        closers.insert(
                            offset,
                            divs[low..].iter().rev().map(|owner| owner.start).collect(),
                        );
                    }
                    if let Some(owners) = div_owners.as_mut() {
                        owners.insert(offset, (divs[low].item_column, divs[low].item_depth));
                    }
                    divs.truncate(low);
                    previous_block = true;
                } else if fence.is_some() && visible && can_open {
                    boundaries.push(offset);
                    let width = fence.unwrap()[1].len();
                    divs.push(DivOwner {
                        minimum_width: owner
                            .filter(|owner| owner.depth == body_depth)
                            .map_or(width, |owner| owner.minimum_width.min(width)),
                        scope_start: owner
                            .filter(|owner| owner.depth == body_depth)
                            .map_or(divs.len(), |owner| owner.scope_start),
                        depth: body_depth,
                        start: offset,
                        item_column: if starts_item {
                            Some(
                                indent
                                    + if item.as_ref().unwrap().as_str().starts_with("[^") {
                                        2
                                    } else {
                                        marker_width
                                    },
                            )
                        } else {
                            items
                                .last()
                                .filter(|&&(column, item_depth)| {
                                    depth == item_depth && indent >= column
                                })
                                .map(|&(column, _)| column)
                        },
                        item_depth: depth,
                    });
                    previous_block = true;
                } else {
                    let attrs = if body.starts_with('{') {
                        read_djot_word_attributes(body, 0)
                    } else {
                        None
                    };
                    previous_block = blank
                        || rows[row]
                        || (can_open
                            && (scope_heading.is_match(body)
                                || attrs.is_some_and(|(end, _)| {
                                    body[end..].trim_matches([' ', '\t']).is_empty()
                                })));
                }
            }
        }
        if previous_blank || starts_item {
            boundaries.push(offset);
            if !blank {
                while let Some(&(column, item_depth)) = items.last() {
                    if depth > item_depth || (depth == item_depth && indent >= column) {
                        break;
                    }
                    items.pop();
                }
                if starts_item {
                    let value = item.unwrap().as_str();
                    items.push((
                        indent
                            + if value.starts_with("[^") {
                                2
                            } else {
                                value.len()
                            },
                        depth,
                    ));
                }
            }
        }
        if rows.get(row).copied().unwrap_or(false) {
            for (at, ch) in line.bytes().enumerate() {
                if ch == b'|'
                    && mask.as_bytes()[offset + at] == b'|'
                    && (at == 0 || line.as_bytes()[at - 1] != b'\\')
                {
                    boundaries.push(offset + at);
                }
            }
        }
        previous_blank = blank;
        previous_depth = depth;
        offset += raw_line.len() + 1;
    }
    boundaries
}

fn djot_simple_destination_ranges(source: &str) -> Option<HashMap<usize, usize>> {
    let _paragraph_cache = ParagraphCacheScope::new(source);
    let mut ranges = HashMap::new();
    if !source.contains("](") {
        return Some(ranges);
    }
    if source.contains('\\') || source.contains('`') || source.contains("][") {
        return None;
    }
    let pattern = cached_regex!(r"\[[^\[\]\n]*\]\([^()\n]*\)").unwrap();
    for value in pattern.find_iter(source) {
        let target = value.as_str().find("](")? + 1;
        if value.as_str().as_bytes().get(1) == Some(&b'^')
            || value.as_str().contains('|')
            || value.as_str()[target + 1..value.len() - 1]
                .bytes()
                .any(|ch| matches!(ch, b'{' | b'<' | b'"'))
        {
            return None;
        }
        ranges.insert(value.start() + target, value.end());
    }
    (ranges.len() == source.matches("](").count()).then_some(ranges)
}

fn djot_destination_ranges(source: &str, mask: &str) -> HashMap<usize, usize> {
    let _paragraph_cache = ParagraphCacheScope::new(source);
    let mut ranges = HashMap::new();
    if !source.contains("](") {
        return ranges;
    }
    let bytes = source.as_bytes();
    let boundaries = djot_inline_boundaries(source, mask);
    let mut next_bracket = HashMap::new();
    let mut close = usize::MAX;
    for at in (0..bytes.len()).rev() {
        if bytes[at] == b'\n' {
            close = usize::MAX;
        } else if bytes[at] == b']' && !is_escaped(bytes, at) {
            close = at;
        }
        if at >= 2 && bytes[at - 1] == b'[' && bytes[at - 2] == b']' {
            next_bracket.insert(at, close);
        }
    }
    let angles: HashMap<_, _> = cached_regex!(r"<[^<>\s]+>")
        .unwrap()
        .find_iter(source)
        .filter(|value| {
            cached_regex!(r"[^:]@|[A-Za-z]:")
                .unwrap()
                .is_match(value.as_str())
        })
        .map(|value| (value.start(), value.end()))
        .collect();
    let table_prefix =
        cached_regex!(r"^[ \t]*(?:>[ \t]*)*(?:(?:[-*+]|[0-9A-Za-z]+[.)])[ \t]+)?\|").unwrap();
    let mut table_code = HashSet::new();
    let mut line_offset = 0;
    for line in source.split('\n') {
        if table_prefix.is_match(line) {
            for (offset, _) in line.match_indices('`') {
                if offset == 0 || line.as_bytes()[offset - 1] != b'`' {
                    table_code.insert(line_offset + offset);
                }
            }
        }
        line_offset += line.len() + 1;
    }
    struct Label {
        at: usize,
        target: usize,
        parens: usize,
    }
    let mut labels: Vec<Label> = Vec::new();
    let mut owner: Option<usize> = None;
    let mut boundary = 0;
    let mut at = 0;
    while at < bytes.len() {
        while boundaries.get(boundary).is_some_and(|value| *value <= at) {
            labels.clear();
            owner = None;
            boundary += 1;
        }
        if owner.is_some() && table_code.contains(&at) {
            owner = None;
        }
        if mask.as_bytes()[at] != bytes[at] {
            at += 1;
            continue;
        }
        if bytes[at] == b'\\' && bytes.get(at + 1).is_some_and(u8::is_ascii_punctuation) {
            at += 2;
            continue;
        }
        if let Some(end) = angles.get(&at) {
            at = *end;
            continue;
        }
        if bytes[at] == b'{' {
            if let Some((end, _)) = read_djot_word_attributes(source, at) {
                at = end;
                continue;
            }
        }
        if bytes[at] == b'[' {
            labels.push(Label {
                at,
                target: 0,
                parens: 0,
            });
            at += 1;
            continue;
        }
        let Some(tip) = labels.len().checked_sub(1) else {
            at += 1;
            continue;
        };
        if bytes[at] == b']' {
            if bytes.get(labels[tip].at + 1) == Some(&b'^') {
                labels.pop();
                if owner == Some(tip) {
                    owner = None;
                }
                at += 1;
                continue;
            }
            if bytes.get(at + 1) == Some(&b'[') {
                let end = next_bracket.get(&(at + 2)).copied().unwrap_or(usize::MAX);
                if end != usize::MAX {
                    labels.pop();
                    if owner == Some(tip) {
                        owner = None;
                    }
                    at = end;
                }
                at += 1;
                continue;
            }
            if bytes.get(at + 1) == Some(&b'{')
                && read_djot_word_attributes(source, at + 1).is_some()
            {
                labels.pop();
                if owner == Some(tip) {
                    owner = None;
                }
                at += 1;
                continue;
            }
            if bytes.get(at + 1) == Some(&b'(') {
                labels[tip].target = at + 1;
                labels[tip].parens = 0;
                owner = Some(tip);
                at += 2;
                continue;
            }
        }
        if let Some(index) = owner {
            if bytes[at] == b'(' {
                labels[index].parens += 1;
            } else if bytes[at] == b')' {
                if labels[index].parens > 0 {
                    labels[index].parens -= 1;
                } else {
                    ranges.insert(labels[index].target, at + 1);
                    labels.truncate(index);
                    owner = None;
                }
            }
        }
        at += 1;
    }
    ranges
}

fn djot_destination_lines(source: &str, depth: usize) -> String {
    let _paragraph_cache = ParagraphCacheScope::new(source);
    if !source.contains('\n') {
        return source.to_owned();
    }
    let folds = cached_regex!(r"\n([ \t]*[^\n]*)").unwrap();
    let escaped_char = cached_regex!(r"\\(?:\r?\n|[^\r\n])").unwrap();
    let raw_destination = escaped_char
        .replace_all(source, |c: &regex::Captures<'_>| {
            if c[0].ends_with('\n') {
                "\n".to_owned()
            } else {
                c[0].to_owned()
            }
        })
        .into_owned();
    folds
        .replace_all(&raw_destination, |c: &regex::Captures<'_>| {
            let mut rest = c[1].trim_start_matches([' ', '\t']);
            for _ in 0..depth {
                if !rest.starts_with('>')
                    || rest
                        .as_bytes()
                        .get(1)
                        .is_some_and(|b| !matches!(b, b' ' | b'\t'))
                {
                    break;
                }
                let Some(tail) = rest.strip_prefix('>') else {
                    break;
                };
                rest = tail.trim_start_matches([' ', '\t']);
            }
            rest.to_owned()
        })
        .into_owned()
}

fn normalize_djot_links(source: &str, inherited: &HashSet<String>) -> String {
    let _paragraph_cache = ParagraphCacheScope::new(source);
    if !source.contains("](") {
        return source.to_owned();
    }
    let mask = mask_djot_forms_with_options(
        source,
        false,
        true,
        None,
        &[],
        OpaqueOptions {
            destinations: false,
            ..OpaqueOptions::default()
        },
    );
    let mask = mask_djot_reference_definitions(source, &mask);
    let rows = djot_table_rows(source, &mask);
    let bytes = source.as_bytes();
    let angles: HashMap<_, _> = cached_regex!(r"<[^<>\s]+>")
        .unwrap()
        .find_iter(source)
        .filter(|m| {
            cached_regex!(r"[^:]@|[A-Za-z]:")
                .unwrap()
                .is_match(m.as_str())
        })
        .map(|m| (m.start(), m.end()))
        .collect();
    #[derive(Clone)]
    struct Link {
        at: usize,
        image: bool,
        depth: usize,
        label_end: usize,
        target: Option<usize>,
        parens: usize,
    }
    fn label_of(
        source: &str,
        mask: &str,
        angles: &HashMap<usize, usize>,
        edits: &HashMap<usize, (usize, String)>,
        start: usize,
        limit: usize,
    ) -> Vec<u8> {
        let bytes = source.as_bytes();
        let mut label = Vec::new();
        let mut brackets = 0;
        let mut at = start;
        while at < limit {
            if let Some((end, text)) = edits.get(&at) {
                if *end <= limit {
                    label.extend_from_slice(text.as_bytes());
                    at = *end;
                    continue;
                }
            }
            if let Some(&end) = angles.get(&at) {
                label.extend_from_slice(&bytes[at..end]);
                at = end;
                continue;
            }
            if mask.as_bytes()[at] == b' ' {
                label.push(bytes[at]);
                at += 1;
                continue;
            }
            if bytes[at] == b'\\' {
                let end = (at + 2).min(bytes.len());
                label.extend_from_slice(&bytes[at..end]);
                at = end;
                continue;
            }
            if bytes[at] == b'[' {
                brackets += 1;
            }
            if bytes[at] == b']' {
                if brackets > 0 {
                    brackets -= 1;
                } else {
                    label.push(b'\\');
                }
            }
            label.push(bytes[at]);
            at += 1;
        }
        label
    }
    let mut image_lines: Option<Vec<ImageLineOwner>> = None;
    let quote_depths: Vec<_> = source
        .split('\n')
        .map(|line| {
            let prefix = cached_regex!(r"^(?:[ \t]*>(?:[ \t]|$))*")
                .unwrap()
                .find(line)
                .unwrap()
                .as_str();
            prefix.bytes().filter(|&b| b == b'>').count()
        })
        .collect();
    let mut stack: Vec<Link> = Vec::new();
    let mut pending_notes = HashSet::new();
    let mut literal_notes = HashMap::new();
    let mut edits = HashMap::<usize, (usize, String)>::new();
    let mut pipe_targets = HashMap::<String, String>::new();
    let mut pipe_definitions = Vec::new();
    let mut alias_definitions = HashMap::<String, String>::new();
    let mut occupied_labels: Option<HashSet<String>> = None;
    let mut pipe_serial = 0;
    let mut line = 0;
    let mut boundaries: HashSet<_> = djot_inline_boundaries(source, &mask).into_iter().collect();
    if source.contains(":::") {
        let mut div_closers = HashMap::new();
        djot_inline_boundaries_with_div_closers(source, &mask, true, Some(&mut div_closers), None);
        boundaries.extend(div_closers.into_keys());
    }
    let mut destination_owner: Option<usize> = None;
    let mut i = 0;
    let blank = cached_regex!(r"^\n[ \t]*(?:>[ \t]*)*\n").unwrap();
    let note_newline = cached_regex!(r"\n[ \t]*").unwrap();
    let alt_end = cached_regex!(r" DJOTEND\n?$").unwrap();
    let rejected_attribute = cached_regex!(r"\A\x00DJOTINVALIDATTR\x00[0-9]+\x00").unwrap();
    while i < source.len() {
        if boundaries.contains(&i) {
            if let Some(owner) = destination_owner.and_then(|index| stack.get(index)) {
                let at = owner.target.unwrap() - 1;
                edits.insert(at, (at + 1, "\\(".to_owned()));
            }
            stack.clear();
            pending_notes.clear();
            destination_owner = None;
        }
        if bytes[i] == b'\n' {
            line += 1;
            if blank.is_match(&source[i..]) {
                if let Some(owner) = destination_owner.and_then(|index| stack.get(index)) {
                    let at = owner.target.unwrap() - 1;
                    edits.insert(at, (at + 1, "\\(".to_owned()));
                }
                stack.clear();
                pending_notes.clear();
                destination_owner = None;
                i += 1;
                continue;
            }
        }
        if mask.as_bytes()[i] == b' ' {
            i += 1;
            continue;
        }
        if bytes[i] == b'\\' {
            i += 2;
            continue;
        }
        if let Some(&end) = angles.get(&i) {
            i = end;
            continue;
        }
        if bytes[i] == b'{' {
            if let Some((end, _)) = read_djot_word_attributes(source, i) {
                i = end;
                continue;
            }
        }
        if rows.get(line) == Some(&true)
            && bytes[i] == b'|'
            && i.checked_sub(1).and_then(|p| bytes.get(p)) != Some(&b'\\')
        {
            if let Some(owner) = destination_owner.and_then(|index| stack.get(index)) {
                let at = owner.target.unwrap() - 1;
                edits.insert(at, (at + 1, "\\(".to_owned()));
            }
            stack.clear();
            pending_notes.clear();
            destination_owner = None;
            i += 1;
            continue;
        }
        if bytes[i] == b'[' {
            if stack.len() >= 200 {
                return source.to_owned();
            }
            if bytes.get(i + 1) == Some(&b'^') {
                pending_notes.insert(i);
            }
            stack.push(Link {
                at: i,
                image: i > 0 && bytes[i - 1] == b'!' && !is_escaped(bytes, i - 1),
                depth: quote_depths[line],
                label_end: 0,
                target: None,
                parens: 0,
            });
            i += 1;
            continue;
        }
        let Some(tip) = stack.len().checked_sub(1) else {
            i += 1;
            continue;
        };
        if bytes[i] == b']' {
            if bytes.get(stack[tip].at + 1) == Some(&b'^') {
                literal_notes.insert(stack[tip].at, i + 1);
                pending_notes.remove(&stack[tip].at);
                stack.pop();
                i += 1;
                continue;
            }
            if bytes.get(i + 1) == Some(&b'(') {
                if let Some(owner) = destination_owner {
                    if owner != tip {
                        let at = stack[owner].target.unwrap() - 1;
                        edits.insert(at, (at + 1, "\\(".to_owned()));
                    }
                }
                stack[tip].label_end = i;
                stack[tip].target = Some(i + 2);
                stack[tip].parens = 0;
                destination_owner = Some(tip);
                i += 2;
                continue;
            }
            if bytes.get(i + 1) == Some(&b'[') {
                let mut end = i + 2;
                while end < bytes.len() && bytes[end] != b']' && bytes[end] != b'[' {
                    if bytes[end] == b'\\' {
                        end += 1;
                    }
                    end += 1;
                }
                if bytes.get(end) == Some(&b']') && mask.as_bytes().get(end) == Some(&b']') {
                    if stack[tip].target.is_some() {
                        let at = stack[tip].at;
                        let label = label_of(source, &mask, &angles, &edits, at + 1, i);
                        edits.insert(
                            at,
                            (
                                i + 1,
                                format!(
                                    "[{}]",
                                    String::from_utf8(label).expect("link labels preserve UTF-8")
                                ),
                            ),
                        );
                    }
                    if destination_owner == Some(tip) {
                        destination_owner = None;
                    }
                    stack.pop();
                    i = end;
                }
                i += 1;
                continue;
            }
            let rejected = bytes.get(i + 1) == Some(&b'\\')
                && bytes.get(i + 2) == Some(&b'{')
                && rejected_attribute
                    .find(&source[i + 3..])
                    .is_some_and(|marker| inherited.contains(marker.as_str()));
            if bytes.get(i + 1) == Some(&b'{') || rejected {
                if stack[tip].target.is_some() {
                    let at = stack[tip].at;
                    let text = if bytes.get(i + 1) == Some(&b'{')
                        && read_djot_word_attributes(source, i + 1).is_some()
                    {
                        let label = label_of(source, &mask, &angles, &edits, at + 1, i);
                        format!(
                            "[{}]",
                            String::from_utf8(label).expect("link labels preserve UTF-8")
                        )
                    } else {
                        format!("\\[{}", &source[at + 1..i + 1])
                    };
                    edits.insert(at, (i + 1, text));
                }
                if destination_owner == Some(tip) {
                    destination_owner = None;
                }
                stack.pop();
            }
        }
        if bytes[i] == b'(' {
            if let Some(owner) = destination_owner {
                stack[owner].parens += 1;
            }
        }
        if bytes[i] != b')' || destination_owner.is_none() {
            i += 1;
            continue;
        }
        let owner_index = destination_owner.unwrap();
        let owner = stack[owner_index].clone();
        if owner.parens > 0 {
            stack[owner_index].parens -= 1;
            i += 1;
            continue;
        }
        if tip != owner_index {
            let at = owner.target.unwrap() - 1;
            edits.insert(at, (at + 1, "\\(".to_owned()));
            if let Some(owner) = destination_owner.and_then(|index| stack.get(index)) {
                let at = owner.target.unwrap() - 1;
                edits.insert(at, (at + 1, "\\(".to_owned()));
            }
            stack.clear();
            pending_notes.clear();
            destination_owner = None;
            i += 1;
            continue;
        }
        if owner.image {
            let authored_label = &source[owner.at + 1..owner.label_end];
            if authored_label.contains('\n') {
                let lines = image_lines.get_or_insert_with(|| image_line_owners(source, &mask).1);
                let image_owner = lines[lines.partition_point(|line| line.0 <= owner.at)];
                if !image_alt_layout(
                    authored_label,
                    image_owner.1,
                    image_owner.2,
                    image_owner.3,
                    None,
                )
                .2
                {
                    stack.pop();
                    destination_owner = None;
                    i += 1;
                    continue;
                }
            }
        }
        let mut label = label_of(
            source,
            &mask,
            &angles,
            &edits,
            owner.at + 1,
            owner.label_end,
        );
        let mut raw_destination = String::new();
        let mut at = owner.target.unwrap();
        while at < i {
            let end = literal_notes.get(&at).copied();
            if let Some(end) = end.filter(|&end| end <= i) {
                raw_destination.push_str(&note_newline.replace_all(
                    &source[at..end].replace(
                        '\\',
                        if rows.get(line) == Some(&true) {
                            "\\\\"
                        } else {
                            "%5C"
                        },
                    ),
                    " ",
                ));
                at = end;
            } else {
                let ch = source[at..].chars().next().unwrap();
                raw_destination.push(ch);
                at += ch.len_utf8();
            }
        }
        let destination = djot_destination_lines(&raw_destination, owner.depth);
        let pipe_target = rows.get(line) == Some(&true) && destination.contains('|');
        if owner.image
            && !source[owner.at + 1..owner.label_end].contains('\n')
            && label.contains(&b'[')
        {
            let raw = String::from_utf8(label).expect("image labels preserve UTF-8");
            let raw = remove_inherited_attribute_markers(&raw, inherited);
            let definitions = cached_regex!(r"\[(carve-djot-pipe-target-[0-9]+)\]")
                .unwrap()
                .captures_iter(&raw)
                .filter_map(|caps| alias_definitions.get(&caps[1]).cloned())
                .collect::<Vec<_>>()
                .join("\n");
            let definitions = if definitions.is_empty() {
                definitions
            } else {
                format!("{definitions}\n\n")
            };
            let converted = djot_to_carve_prepared_with_quotes(
                &strip_footnote_definition_attributes(&format!(
                    "{definitions}DJOTALT {raw} DJOTEND"
                )),
                true,
                true,
                true,
            );
            let options = crate::Options {
                smart_typography: crate::SmartTypographyMode::Source,
                ..Default::default()
            };
            let mut document =
                crate::parse_with_options(&converted.replace("\0U\0", "\0"), &options);
            use crate::include_walk::SubtreeVisitor;
            ImageAltChildText {
                preserve_breaks: raw.contains('\n'),
            }
            .blocks(&mut document.children);
            let plain =
                crate::render_plain_text_with_options(&document, &options).unwrap_or_default();
            let plain = alt_end.replace(&plain, "");
            label = plain[8..]
                .chars()
                .fold(String::new(), |mut escaped, ch| {
                    if ch.is_ascii_punctuation() {
                        escaped.push('\\');
                    }
                    escaped.push(ch);
                    escaped
                })
                .into_bytes();
        }
        let mut target = String::new();
        let mut chars = destination.chars().peekable();
        while let Some(ch) = chars.next() {
            if ch == '\\'
                && chars
                    .peek()
                    .is_some_and(|next| next.is_ascii_punctuation() || matches!(next, ' ' | '\t'))
            {
                let next = chars.next().unwrap();
                if "()\\".contains(next) {
                    target.push(ch);
                    target.push(next);
                } else if " \t\"<>`".contains(next) {
                    target.push_str(&format!("%{:02X}", next as u8));
                } else {
                    target.push(next);
                }
            } else if ch == '\\' {
                target.push_str("\\\\");
            } else if ch.is_whitespace()
                || "\"<>`".contains(ch)
                || (!pipe_target && "()".contains(ch))
            {
                for byte in ch.to_string().bytes() {
                    target.push_str(&format!("%{byte:02X}"));
                }
            } else {
                target.push(ch);
            }
        }
        let mut protected_target = String::with_capacity(target.len());
        let mut chars = target.chars().peekable();
        while let Some(ch) = chars.next() {
            if ch == ']' && chars.peek().is_some_and(|next| matches!(next, '[' | '{')) {
                protected_target.push_str("%5D");
            } else {
                protected_target.push(ch);
            }
        }
        target = protected_target;
        let label = String::from_utf8(label).expect("link labels preserve UTF-8");
        let replacement = if rows.get(line) == Some(&true) && target.contains('|') {
            let alias = if let Some(alias) = pipe_targets.get(&target) {
                alias.clone()
            } else {
                let occupied_labels = occupied_labels.get_or_insert_with(|| {
                    cached_regex!(r"\[([^\[\]\n]*)\]")
                        .unwrap()
                        .captures_iter(source)
                        .map(|caps| {
                            let label = cached_regex!(r"\\([ \t!-\/:-@\[-`{-~])")
                                .unwrap()
                                .replace_all(&caps[1], "$1");
                            cached_regex!(r"[ \t\r\n]+")
                                .unwrap()
                                .replace_all(label.trim(), " ")
                                .into_owned()
                        })
                        .collect()
                });
                let alias = loop {
                    pipe_serial += 1;
                    let alias = format!("carve-djot-pipe-target-{pipe_serial}");
                    if occupied_labels.insert(alias.clone()) {
                        break alias;
                    }
                };
                pipe_definitions.push(format!("[{alias}]: {target}"));
                alias_definitions.insert(alias.clone(), format!("[{alias}]: {target}"));
                pipe_targets.insert(target, alias.clone());
                alias
            };
            format!("[{label}][{alias}]")
        } else {
            format!("[{label}]({target})")
        };
        edits.insert(owner.at, (i + 1, replacement));
        stack.pop();
        destination_owner = None;
        for at in pending_notes.drain() {
            edits.insert(at, (at + 1, "\\[".to_owned()));
        }
        i += 1;
    }
    if let Some(owner) = destination_owner.and_then(|index| stack.get(index)) {
        let at = owner.target.unwrap() - 1;
        edits.insert(at, (at + 1, "\\(".to_owned()));
    }
    let mut output = Vec::new();
    i = 0;
    while i < bytes.len() {
        if let Some((end, text)) = edits.get(&i) {
            output.extend_from_slice(text.as_bytes());
            i = *end;
        } else {
            output.push(bytes[i]);
            i += 1;
        }
    }
    let output = String::from_utf8(output).expect("link replacements preserve UTF-8");
    let used: HashSet<_> = cached_regex!(r"\[(carve-djot-pipe-target-[0-9]+)\]")
        .unwrap()
        .captures_iter(&output)
        .map(|capture| capture.get(1).unwrap().as_str())
        .collect();
    pipe_definitions.retain(|definition| {
        definition
            .find(']')
            .is_some_and(|end| used.contains(&definition[1..end]))
    });
    if pipe_definitions.is_empty() {
        output
    } else {
        format!("{}\n\n{output}", pipe_definitions.join("\n"))
    }
}

fn carry_djot_attributes_across_div_closers(source: &str) -> std::borrow::Cow<'_, str> {
    if !source.contains('{') || !source.contains(":::") {
        return std::borrow::Cow::Borrowed(source);
    }
    let mask = mask_djot_inline(source, true);
    let mut closers = HashMap::new();
    let mut owners = HashMap::new();
    djot_inline_boundaries_with_div_closers(
        source,
        &mask,
        true,
        Some(&mut closers),
        Some(&mut owners),
    );
    if closers.is_empty() {
        return std::borrow::Cow::Borrowed(source);
    }
    let lines: Vec<_> = source.split('\n').collect();
    let mut removed = HashSet::new();
    let mut moved: HashMap<usize, Vec<String>> = HashMap::new();
    let mut pending: Vec<(usize, &str)> = Vec::new();
    let mut residues = HashMap::new();
    let mut crossed = false;
    let mut offset = 0;
    let mut previous_depth = 0;
    let mut previous = "";
    let mut closed_owner: Option<(Option<usize>, usize)> = None;
    let physical = cached_regex!(r"^(?:[ \t]*>(?:[ \t]|$))*[ \t]*").unwrap();
    let block =
        cached_regex!(r"^(?:#{1,6} |:{3,}|[`~]{3,}|\[[^\]]+\]:|(?:[*-][ \t]*){3,}$)").unwrap();
    for (index, line) in lines.iter().enumerate() {
        let prefix = physical.find(line).unwrap().as_str();
        let start = prefix.len();
        let content = &line[start..];
        let depth = prefix.bytes().filter(|&b| b == b'>').count();
        let follows_block = !pending.is_empty() || previous.is_empty() || block.is_match(previous);
        let visible = mask.as_bytes().get(offset + start) == Some(&b'{');
        let candidate = if content.starts_with('{') && (follows_block || visible) {
            read_djot_word_attributes(content, 0)
        } else {
            None
        };
        if follows_block
            && depth == previous_depth
            && candidate.as_ref().is_some_and(|(end, value)| {
                *end == content.trim_end_matches(DJOT_LINE_SPACING).len() && value == "{}"
            })
        {
            offset += line.len() + 1;
            continue;
        }
        let whole = visible
            && candidate
                .is_some_and(|(end, _)| end == content.trim_end_matches(DJOT_LINE_SPACING).len());
        if crossed
            && !closers.contains_key(&offset)
            && !content.trim_matches(DJOT_LINE_SPACING).is_empty()
            && (!whole || depth != previous_depth)
        {
            let quote_prefix = cached_regex!(r"^(?:[ \t]*>(?:[ \t]|$))*")
                .unwrap()
                .find(line)
                .unwrap()
                .as_str();
            let indent = line[quote_prefix.len()..]
                .bytes()
                .take_while(|b| matches!(b, b' ' | b'\t'))
                .count();
            let peer_item =
                closed_owner.is_some_and(|(column, owner_depth)| {
                    column.is_some_and(|column| depth == owner_depth && indent < column)
                }) && cached_regex!(r"^(?:[-*+]|(?:[0-9]+|[A-Za-z]|[ivxlcdm]+|[IVXLCDM]+)[.)]|\((?:[0-9]+|[A-Za-z]|[ivxlcdm]+|[IVXLCDM]+)\))(?:[ \t]+|$)")
                    .unwrap()
                    .is_match(&line[prefix.len()..]);
            if !peer_item
                || cached_regex!(r"^(?:[-*][ \t]*){3,}$")
                    .unwrap()
                    .is_match(content)
            {
                let target_prefix = if depth > previous_depth {
                    let marker = prefix.match_indices('>').nth(previous_depth).unwrap().0;
                    &prefix[..marker]
                } else {
                    prefix
                };
                for (line, value) in pending.drain(..) {
                    removed.insert(line);
                    moved
                        .entry(index)
                        .or_default()
                        .push(format!("{target_prefix}{value}"));
                }
            }
            pending.clear();
            crossed = false;
        }
        if whole && follows_block {
            if !crossed && depth > previous_depth {
                pending.clear();
            }
            pending.push((index, content.trim_end_matches(DJOT_LINE_SPACING)));
            if depth > previous_depth {
                residues.insert(index, prefix.trim_end());
            }
        } else if closers.contains_key(&offset) {
            crossed |= !pending.is_empty();
            closed_owner = owners.get(&offset).copied();
        } else if !content.trim_matches(DJOT_LINE_SPACING).is_empty() {
            pending.clear();
        }
        if !whole {
            previous = content.trim_matches(DJOT_LINE_SPACING);
        }
        previous_depth = if content.trim_matches(DJOT_LINE_SPACING).is_empty() {
            previous_depth.min(depth)
        } else {
            depth
        };
        offset += line.len() + 1;
    }
    if crossed {
        for (line, _) in pending {
            removed.insert(line);
        }
    }
    if removed.is_empty() {
        return std::borrow::Cow::Borrowed(source);
    }
    let mut out = Vec::new();
    for index in 0..=lines.len() {
        for attr in moved.remove(&index).unwrap_or_default() {
            out.push(attr);
        }
        if let Some(line) = lines.get(index) {
            if !removed.contains(&index) {
                out.push((*line).to_owned());
            } else if let Some(residue) = residues.get(&index) {
                out.push((*residue).to_owned());
            }
        }
    }
    std::borrow::Cow::Owned(out.join("\n"))
}

fn carry_djot_attributes_across_quote_exits(source: &str) -> String {
    if !source.contains('>') || !source.contains('{') {
        return source.to_owned();
    }
    let _paragraph_cache = ParagraphCacheScope::new(source);
    let mask = mask_djot_attribute_source(source);
    let masks: Vec<_> = mask.split('\n').collect();
    let mut lines: Vec<_> = source.split('\n').map(str::to_owned).collect();
    let quote = cached_regex!(r"^(?:[ \t]*>(?:[ \t]|$))*").unwrap();
    let marker = cached_regex!(r"^(?:[-*+]|(?:[0-9]+|[A-Za-z]|[ivxlcdm]+|[IVXLCDM]+)[.)]|\((?:[0-9]+|[A-Za-z]|[ivxlcdm]+|[IVXLCDM]+)\))[ \t]+").unwrap();
    let mut columns: HashMap<usize, Vec<usize>> = HashMap::new();
    let mut contexts = Vec::new();
    let mut offset = 0;
    let mut previous_depth = 0;
    for (n, line) in lines.iter().enumerate() {
        let prefix = quote.find(line).unwrap().end();
        let depth = line[..prefix].bytes().filter(|byte| *byte == b'>').count();
        let body = &line[prefix..];
        let indent = body.len() - body.trim_start_matches([' ', '\t']).len();
        let attributes = masks[n].as_bytes().get(prefix + indent) == Some(&b'{')
            && read_djot_word_attributes(body, indent)
                .is_some_and(|(end, _)| end == body.trim_end_matches(DJOT_LINE_SPACING).len());
        let mut continued = false;
        let mut list_owned = false;
        if !body.trim_matches(DJOT_LINE_SPACING).is_empty() {
            let held = columns.entry(depth).or_default();
            if marker.is_match(&body[indent..]) {
                while held.last().is_some_and(|column| *column >= indent) {
                    held.pop();
                }
                held.push(indent);
            }
            if attributes {
                continued = comment_continues_paragraph(source, offset + prefix + indent);
                list_owned = held.iter().any(|column| *column < indent);
            } else if held.last().is_some_and(|column| indent <= *column)
                && !marker.is_match(&body[indent..])
            {
                held.clear();
            }
        }
        contexts.push((
            prefix,
            depth,
            indent,
            attributes,
            continued,
            list_owned,
            body.trim_matches(DJOT_LINE_SPACING).is_empty(),
            depth > previous_depth,
        ));
        previous_depth = depth;
        offset += line.len() + 1;
    }
    let mut moved: HashMap<usize, Vec<String>> = HashMap::new();
    let mut following = None;
    let mut next_content: Option<usize> = None;
    let mut destination_depth: Option<usize> = None;
    for n in (0..lines.len()).rev() {
        let (prefix, depth, indent, attributes, continued, list_owned, empty, opens) = contexts[n];
        if attributes {
            if let (Some(target), Some(destination)) = (following, destination_depth) {
                if depth > destination
                    && !next_content.is_some_and(|next| contexts[next].7)
                    && !continued
                    && !list_owned
                {
                    let target_line: &String = &lines[target];
                    let mut end = 0;
                    for _ in 0..destination {
                        let rest = &target_line[end..];
                        end += rest.len() - rest.trim_start_matches([' ', '\t']).len() + 1;
                        if target_line
                            .as_bytes()
                            .get(end)
                            .is_some_and(|byte| matches!(byte, b' ' | b'\t'))
                        {
                            end += 1;
                        }
                    }
                    end += target_line[end..].len()
                        - target_line[end..].trim_start_matches([' ', '\t']).len();
                    let mut owner_end = 0;
                    for _ in 0..destination {
                        let rest = &lines[n][owner_end..prefix];
                        owner_end += rest.len() - rest.trim_start_matches([' ', '\t']).len() + 1;
                        if lines[n]
                            .as_bytes()
                            .get(owner_end)
                            .is_some_and(|byte| matches!(byte, b' ' | b'\t'))
                        {
                            owner_end += 1;
                        }
                    }
                    let owner_rest = &lines[n][owner_end..prefix];
                    let owner_indent =
                        owner_rest.len() - owner_rest.trim_start_matches([' ', '\t']).len();
                    let target_prefix = if owner_indent > 0 {
                        format!("{}{}", &lines[n][..owner_end], " ".repeat(owner_indent))
                    } else {
                        target_line[..end].to_owned()
                    };
                    moved
                        .entry(target)
                        .or_default()
                        .push(format!("{target_prefix}{}", &lines[n][prefix + indent..]));
                    lines[n] = lines[n][..prefix]
                        .trim_end_matches(DJOT_LINE_SPACING)
                        .to_owned();
                } else if depth <= destination {
                    following = Some(n);
                }
            }
        } else if empty {
            destination_depth = destination_depth.map(|destination| destination.min(depth));
        } else {
            following = Some(n);
            destination_depth = Some(depth);
        }
        if !empty || opens {
            next_content = Some(n);
        }
    }
    let mut output = Vec::new();
    for (n, line) in lines.into_iter().enumerate() {
        if let Some(attributes) = moved.remove(&n) {
            output.extend(attributes.into_iter().rev());
        }
        output.push(line);
    }
    output.join("\n")
}

fn normalize_djot_attribute_lines(source: &str) -> String {
    let _paragraph_cache = ParagraphCacheScope::new(source);
    if !source.contains('{') {
        return source.to_owned();
    }
    let mask = mask_djot_attribute_source(source);
    let mut closes = std::collections::HashMap::new();
    let mut close = None;
    for (at, byte) in source.bytes().enumerate().rev() {
        match byte {
            b'\n' => close = None,
            b'}' => close = Some(at + 1),
            b'{' => {
                if let Some(end) = close {
                    closes.insert(at, end);
                }
            }
            _ => {}
        }
    }
    let mut output = String::new();
    let mut copied = 0;
    let mut i = 0;
    let attribute_key = cached_regex!(r"^[A-Za-z][A-Za-z0-9_-]*=").unwrap();
    while i < source.len() {
        if mask.as_bytes()[i] == b'{' && !is_escaped(source.as_bytes(), i) {
            if let Some((end, attrs, _)) =
                read_djot_attributes_with_source_form(source, i, false, false, true, true)
            {
                if source[i..end].contains('\n') {
                    output.push_str(&source[copied..i]);
                    output.push_str(&attrs);
                    copied = end;
                }
                i = end;
                continue;
            }
            if let Some(&end) = closes.get(&i) {
                if attribute_key.is_match(&source[i + 1..]) {
                    output.push_str(&source[copied..i]);
                    for ch in source[i..end].chars() {
                        if "{}[]".contains(ch) {
                            output.push('\\');
                        }
                        output.push(ch);
                    }
                    copied = end;
                    i = end;
                    continue;
                }
            }
        }
        i += 1;
    }
    output.push_str(&source[copied..]);
    output
}

fn fold_djot_references(source: &str) -> String {
    fold_djot_references_with_decisions(source, None)
}

pub(super) struct ReferenceDecision {
    pub label: String,
    pub target: String,
    pub end: usize,
}

pub(super) fn fold_djot_references_with_decisions(
    source: &str,
    mut accepted: Option<&mut HashMap<usize, ReferenceDecision>>,
) -> String {
    let _paragraph_cache = ParagraphCacheScope::new(source);
    if !source.contains("]:") {
        return source.to_owned();
    }
    let mut lines: Vec<String> = source.split('\n').map(str::to_owned).collect();
    let mask = mask_djot_inline(source, false);
    let mut rows = djot_table_rows(source, &mask);
    let definition = cached_regex!(r"^\[([^\]\n]*)\]:[ \t]*([^ \t\r\n]*)\r?$").unwrap();
    let boundary = cached_regex!(r"^(?:#{1,6} |:{3,}|\[[^\]]*\]:|(?:[-*][ \t]*){3,}$)").unwrap();
    let fence_end = cached_regex!(r"^(?:`{3,}|~{3,})[ \t]*$").unwrap();
    let marker = cached_regex!(r"(?:[-*+]|[0-9A-Za-z]+[.)]|\([0-9A-Za-z]+\)|:)[ \t]+").unwrap();
    let token = cached_regex!(r"^[^ \t\r\n]+$").unwrap();
    let thematic = cached_regex!(r"^(?:[ \t]*>[ \t]*)*[ \t]*(?:[-*][ \t]*){3,}$").unwrap();
    let mut removed = HashSet::new();
    let (mut offset, mut previous_depth, mut n) = (0usize, 0, 0);
    let mut previous = String::new();
    let mut previous_attribute_block = false;
    let mut definition_owners: Vec<(usize, usize)> = Vec::new();
    let mut fence_mask: Option<String> = None;
    let owner_marker = cached_regex!(r"(?:[-*+]|[0-9A-Za-z]+[.)]|\([0-9A-Za-z]+\))[ \t]+").unwrap();
    while n < lines.len() {
        let line = lines[n].clone();
        let depth = quoted(&line).0;
        let at = djot_content_start(&line);
        let content = &line[at..];
        let previous_line = n.checked_sub(1).map_or("", |n| lines[n].as_str());
        let previous_at = djot_content_start(previous_line);
        let previous_note = previous_line[..previous_at].find("[^");
        let previous_owner = previous_note.map_or(previous_at, |at| at + 1);
        let prefix = &line[..at];
        let note_at = prefix.find("[^");
        let header_marker = owner_marker.find(prefix);
        let physical_prefix = cached_regex!(r"^(?:[ \t]*>(?:[ \t]|$))*[ \t]*")
            .unwrap()
            .find(&line)
            .unwrap()
            .as_str();
        let definition_owner = definition_owners.last().copied();
        let owner_exit = definition_owner.is_some_and(|(start, owner_depth)| {
            depth < owner_depth || depth == owner_depth && physical_prefix.len() < start
        });
        let definition_caps = definition.captures(content).filter(|caps| {
            !caps[1].starts_with('^')
                || caps[1].len() == 1
                || !matches!(
                    content.as_bytes().get(caps[1].len() + 3),
                    None | Some(b' ' | b'\t' | b'\r')
                )
        });
        let metadata = definition_caps.is_some()
            || content.starts_with('{')
                && read_djot_word_attributes(content, 0)
                    .is_some_and(|(end, _)| end == content.trim_end().len());
        if owner_exit && metadata && mask.as_bytes().get(offset + at) == Some(&b' ') {
            fence_mask.get_or_insert_with(|| mask_djot_fences(source, None, &rows, true));
        }
        let metadata_exit = owner_exit
            && metadata
            && (mask.as_bytes().get(offset + at) != Some(&b' ')
                || fence_mask
                    .as_ref()
                    .is_some_and(|mask| mask.as_bytes().get(offset + at) != Some(&b' ')));
        let outdented = metadata_exit
            || at < previous_owner
                && marker.is_match(&previous_line[..previous_note.unwrap_or(previous_at)]);
        let block_allowed = previous.is_empty()
            || previous_attribute_block
            || rows.get(n.wrapping_sub(1)) == Some(&true)
            || depth != previous_depth
            || thematic.is_match(previous_line)
            || outdented
            || (fence_end.is_match(&previous)
                && mask
                    .as_bytes()
                    .get(offset.saturating_sub(previous_line.len() + 1) + previous_at)
                    == Some(&b' '))
            || boundary.is_match(&previous);
        let has_item = marker.is_match(&prefix[..note_at.unwrap_or(prefix.len())]);
        let allowed = block_allowed || has_item;
        let owner_body = &line[physical_prefix.len()..];
        let row_end = line.trim_end().len().saturating_sub(1);
        let boundary_row = (owner_exit || rows.get(n.wrapping_sub(1)) == Some(&true))
            && owner_body.starts_with('|')
            && row_end > physical_prefix.len()
            && mask.as_bytes().get(offset + physical_prefix.len()) == Some(&b'|')
            && mask.as_bytes().get(offset + row_end) == Some(&b'|')
            && line.as_bytes().get(row_end - 1) != Some(&b'\\');
        let row_entry = boundary_row && owner_exit && rows.get(n) != Some(&true);
        if boundary_row {
            rows[n] = true;
        }
        let closes_owner = boundary_row
            || cached_regex!(r"^(?:#{1,6}(?:[ \t]|$)|:{3,}|[`~]{3,}|(?:[-*][ \t]*){3,}$)")
                .unwrap()
                .is_match(owner_body)
                && mask.as_bytes().get(offset + physical_prefix.len()) != Some(&b' ')
            || rows.get(n) == Some(&true)
            || cached_regex!(r"^(?:`{3,}|~{3,})[ \t]*=?[A-Za-z0-9_+#.-]*[ \t]*$")
                .unwrap()
                .is_match(owner_body);
        if owner_exit
            && (metadata_exit
                || previous.is_empty()
                || closes_owner
                || rows.get(n.wrapping_sub(1)) == Some(&true))
        {
            while definition_owners
                .last()
                .is_some_and(|&(start, owner_depth)| {
                    depth < owner_depth || depth == owner_depth && physical_prefix.len() < start
                })
            {
                definition_owners.pop();
            }
        }
        if block_allowed {
            if let Some(start) = note_at.or_else(|| header_marker.map(|marker| marker.start())) {
                if mask.as_bytes().get(offset + start) != Some(&b' ') {
                    let owner = (start + 1, depth);
                    if definition_owners.last() != Some(&owner) {
                        definition_owners.push(owner);
                    }
                }
            }
        }
        let empty_note_task_attributes = !definition.is_match(content)
            && content.starts_with('{')
            && previous_note.is_some()
            && empty_djot_footnote_task_attributes(previous_line);
        let separator = if (metadata_exit && !has_item
            || row_entry
            || definition.is_match(content)
                && rows.get(n.wrapping_sub(1)) == Some(&true)
                && !has_item
                && (owner_exit || definition_owner.is_none()))
            && !previous.is_empty()
            && !empty_note_task_attributes
        {
            format!("{physical_prefix}\n")
        } else {
            String::new()
        };
        if !separator.is_empty() {
            lines[n] = format!("{separator}{line}");
        }
        if let Some(caps) = definition_caps {
            if mask.as_bytes().get(offset + at) == Some(&b'[') && !allowed {
                lines[n] = format!("{}{}", &line[..at], line[at..].replacen("]:", "]\\:", 1));
            }
            if (mask.as_bytes().get(offset + at) == Some(&b'[')
                || metadata_exit
                    && fence_mask
                        .as_ref()
                        .is_some_and(|mask| mask.as_bytes().get(offset + at) == Some(&b'[')))
                && allowed
            {
                let mut target = caps.get(2).map_or("", |m| m.as_str()).to_owned();
                let mut end = n;
                while end + 1 < lines.len() {
                    let next = &lines[end + 1];
                    let next_at = djot_content_start(next);
                    let next_token = next[next_at..]
                        .strip_suffix('\r')
                        .unwrap_or(&next[next_at..]);
                    if quoted(next).0 != depth
                        || next_at <= at
                        || next[..next_at].contains("[^")
                        || marker.is_match(&next[..next_at])
                        || !token.is_match(next_token)
                    {
                        break;
                    }
                    target.push_str(next_token);
                    end += 1;
                }
                if let Some(decisions) = accepted.as_deref_mut() {
                    decisions.insert(
                        n,
                        ReferenceDecision {
                            label: caps[1].to_owned(),
                            target: target.clone(),
                            end,
                        },
                    );
                }
                let next_byte = content.as_bytes().get(caps[1].len() + 3);
                if end > n || !target.is_empty() && !matches!(next_byte, Some(b' ')) {
                    let padding = if target.is_empty() { "" } else { " " };
                    lines[n] =
                        format!("{separator}{}[{}]:{padding}{target}", &line[..at], &caps[1]);
                }
                if end > n {
                    for (k, value) in lines.iter_mut().enumerate().take(end + 1).skip(n + 1) {
                        offset += value.len() + 1;
                        removed.insert(k);
                    }
                    n = end;
                }
            }
        }
        if !definition.is_match(content)
            && cached_regex!(r"^\[([^\^\]\n][^\]\n]*|)\]:")
                .unwrap()
                .is_match(content)
            && mask.as_bytes().get(offset + at) == Some(&b'[')
        {
            lines[n] = format!("{}{}", &line[..at], line[at..].replacen("]:", "]\\:", 1));
        }
        previous_attribute_block = allowed
            && content.starts_with('{')
            && read_djot_word_attributes(content, 0)
                .is_some_and(|(end, _)| end == content.trim_end().len());
        previous = content.trim().to_owned();
        previous_depth = depth;
        offset += line.len() + 1;
        n += 1;
    }
    lines
        .into_iter()
        .enumerate()
        .filter_map(|(n, line)| (!removed.contains(&n)).then_some(line))
        .collect::<Vec<_>>()
        .join("\n")
}

fn normalize_djot_paragraph_fences(source: &str) -> String {
    let _paragraph_cache = ParagraphCacheScope::new(source);
    let rows = djot_table_rows(source, &mask_djot_inline(source, false));
    let mask = mask_djot_fences(source, None, &rows, true);
    if !mask.contains("```") {
        return source.to_owned();
    }
    let bytes = source.as_bytes();
    let tick_pattern = cached_regex!(r"`+").unwrap();
    let matches: Vec<_> = tick_pattern.find_iter(&mask).collect();
    let mut runs = HashMap::new();
    let mut next = HashMap::new();
    for m in matches.into_iter().rev() {
        let width = m.len();
        runs.insert(
            m.start(),
            (width, *next.get(&width).unwrap_or(&source.len())),
        );
        next.insert(width, m.start());
    }
    let breaks: Vec<_> = cached_regex!(r"\n[ \t]*(?:>[ \t]*)*\n")
        .unwrap()
        .find_iter(&mask)
        .map(|m| m.start())
        .collect();
    let angles: HashMap<_, _> = cached_regex!(r"<[^<>\s]+>")
        .unwrap()
        .find_iter(&mask)
        .filter(|m| {
            cached_regex!(r"[^:]@|[A-Za-z]:")
                .unwrap()
                .is_match(m.as_str())
        })
        .map(|m| (m.start(), m.end()))
        .collect();
    let head_pattern = cached_regex!(r"^[ \t]*(?:> ?[ \t]*)*").unwrap();
    let mut heads = HashSet::new();
    let mut line_offset = 0;
    for line in source.split('\n') {
        heads.insert(line_offset + head_pattern.find(line).unwrap().len());
        line_offset += line.len() + 1;
    }
    let mut output = String::new();
    let (mut copied, mut boundary, mut i) = (0, 0, 0);
    let payload_end = cached_regex!(r"\n[ \t]*(?:>[ \t]*)*$").unwrap();
    while i < source.len() {
        if mask.as_bytes()[i] == b' ' {
            i += 1;
            continue;
        }
        if bytes[i] == b'\\' {
            i += 2;
            continue;
        }
        if let Some(&end) = angles.get(&i) {
            i = end;
            continue;
        }
        while breaks.get(boundary).is_some_and(|&end| end <= i) {
            boundary += 1;
        }
        if let Some(&(width, closer)) = runs.get(&i) {
            let end = closer.min(*breaks.get(boundary).unwrap_or(&source.len()));
            let closed = closer < *breaks.get(boundary).unwrap_or(&source.len());
            let raw_payload = &source[i + width..end];
            let payload = if !closed && end == source.len() {
                payload_end.replace(raw_payload, "")
            } else {
                std::borrow::Cow::Borrowed(raw_payload)
            };
            if width >= 3 && heads.contains(&i) {
                let widths: HashSet<_> =
                    tick_pattern.find_iter(&payload).map(|m| m.len()).collect();
                let mut native_width = 1;
                while widths.contains(&native_width) {
                    native_width += 1;
                }
                let ticks = "`".repeat(native_width);
                output.push_str(&source[copied..i]);
                if native_width >= 3 {
                    output.push_str("{%%}");
                }
                if payload.is_empty() {
                    output.push_str("`<code></code>`{=html}");
                } else {
                    output.push_str(&ticks);
                    output.push_str(&payload);
                    output.push_str(&ticks);
                }
                copied = end + if closed { width } else { 0 };
            }
            i = end + if closed { width } else { 0 };
            continue;
        }
        i += 1;
    }
    output.push_str(&source[copied..]);
    output
}

mod footnotes;
use footnotes::*;
pub(crate) use footnotes::{strip_footnote_definition_attributes, DjotFootnoteAttributeStrip};
fn has_djot_code_layout_candidates(source: &str) -> bool {
    (source.contains('`') && cached_regex!(r"(?:^|\n)[ \t]*:(?:[ \t]|$)").unwrap().is_match(source))
        || source.contains("\\|")
        || (source.contains('`') && cached_regex!(r"(?:^|\n)(?:[ \t]*>(?:[ \t]|$))*[ \t]*\[\^[^\]\n]+\]:").unwrap().is_match(source))
        || (source.contains('`') && cached_regex!(r"(?:^|\n)[ \t]*(?:(?:[-*+]|(?:[0-9]+|[A-Za-z]|[ivxlcdm]+|[IVXLCDM]+)[.)]|\((?:[0-9]+|[A-Za-z]|[ivxlcdm]+|[IVXLCDM]+)\))[ \t]+(?:\[[ xX]\][ \t]+)?|\[\^[^\]\n]+\]:[ \t]*)*>").unwrap().is_match(source))
        || (source.contains("```")
            && cached_regex!(r"(?:^|\n)(?:[ \t]*>(?:[ \t]|$))*[ \t]+`{3,}").unwrap().is_match(source))
}

fn normalize_djot_fences(source: &str) -> String {
    let _paragraph_cache = ParagraphCacheScope::new(source);
    let quote_code = has_djot_code_layout_candidates(source);
    if !source.contains("```") && !source.contains("~~~") {
        return if source.contains("\\|") || quote_code {
            close_djot_table_code(source, !source.contains("\\|"), None)
        } else {
            source.to_owned()
        };
    }
    let mut lines: Vec<String> = source.split('\n').map(str::to_owned).collect();
    let rows = djot_table_rows(source, &mask_djot_inline(source, false));
    mask_djot_forms(
        source,
        false,
        false,
        Some(&mut |line, replacement| {
            lines[line] = replacement.to_owned();
        }),
        &rows,
    );
    let normalized = normalize_djot_paragraph_fences(&lines.join("\n"));
    let indented_ticks = cached_regex!(r"^[ \t]+`{3,}").unwrap();
    if source.contains("\\|")
        || quote_code
        || source
            .lines()
            .any(|line| indented_ticks.is_match(quoted(line).1))
    {
        close_djot_table_code(
            &normalized,
            !source.contains("\\|")
                && !source
                    .lines()
                    .any(|line| indented_ticks.is_match(quoted(line).1)),
            None,
        )
    } else {
        normalized
    }
}

fn normalize_djot_autolinks(source: &str) -> String {
    let _paragraph_cache = ParagraphCacheScope::new(source);
    if !source.contains('<') {
        return source.to_owned();
    }
    let bytes = source.as_bytes();
    let code_mask = mask_djot_forms_with_options(
        source,
        false,
        true,
        None,
        &[],
        OpaqueOptions {
            autolinks: false,
            ..OpaqueOptions::default()
        },
    );
    let mut mask = mask_djot_forms_with_options(
        source,
        true,
        true,
        None,
        &[],
        OpaqueOptions {
            autolinks: false,
            ..OpaqueOptions::default()
        },
    )
    .into_bytes();
    let rows = djot_table_rows(source, &code_mask);
    let definition_pattern = cached_regex!(r"^\[[^\^\]\n][^\]\n]*\]:|^\[\]:").unwrap();
    let boundary_pattern =
        cached_regex!(r"^(?:#{1,6} |`{3,}|~{3,}|:{3,}|\{[ \t.#A-Za-z}%]|\[[^\^\]][^\]]*\]:|\[\]:)")
            .unwrap();
    let continuation_pattern = cached_regex!(r"^\S+$").unwrap();
    let mut definition_indent = None;
    let mut definition_offset = 0;
    let mut previous_content = "";
    for (definition_line, line) in source.split('\n').enumerate() {
        let at = djot_content_start(line);
        let content = &line[at..];
        let boundary = previous_content.is_empty()
            || rows.get(definition_line.wrapping_sub(1)) == Some(&true)
            || boundary_pattern.is_match(previous_content);
        let definition = code_mask.as_bytes().get(definition_offset + at) == Some(&b'[')
            && definition_pattern.is_match(content)
            && boundary;
        let continuation = definition_indent.is_some_and(|indent| at > indent)
            && continuation_pattern.is_match(content);
        if definition || continuation {
            blank_out(&mut mask, definition_offset, definition_offset + line.len());
            if definition {
                definition_indent = Some(at);
            }
        } else {
            definition_indent = None;
        }
        previous_content = content.trim();
        definition_offset += line.len() + 1;
    }

    let valid_angle = cached_regex!(r"[^:]@|[A-Za-z]:").unwrap();
    let email = cached_regex!(r"[^:]@").unwrap();
    let authority_pattern = cached_regex!(r"^[A-Za-z][A-Za-z0-9+.-]*://[^/?#\\]*").unwrap();
    let scheme_pattern = cached_regex!(r"^[A-Za-z][A-Za-z0-9+.-]*:").unwrap();
    let angle_ends: std::collections::HashMap<usize, usize> = cached_regex!(r"<[^<>\s]+>")
        .unwrap()
        .find_iter(source)
        .filter(|value| valid_angle.is_match(value.as_str()))
        .map(|value| (value.start(), value.end()))
        .collect();
    let mut image_autolinks = HashSet::new();
    let mut paren_ends = std::collections::HashMap::new();
    let mut parens = Vec::new();
    let mut quote = None;
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' {
            i += 2;
            continue;
        }
        if let Some(ch) = quote {
            if bytes[i] == ch || bytes[i] == b'\n' {
                quote = None;
            }
        } else if !parens.is_empty()
            && i > 0
            && matches!(bytes[i - 1], b' ' | b'\t')
            && matches!(bytes[i], b'"' | b'\'')
        {
            quote = Some(bytes[i]);
        } else if bytes[i] == b'(' {
            parens.push(i);
        } else if bytes[i] == b')' {
            if let Some(start) = parens.pop() {
                paren_ends.insert(start, i);
            }
        }
        i += 1;
    }
    let mut bracket_ends = std::collections::HashMap::new();
    let mut nested_brackets = HashSet::new();
    let mut stack = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' {
            i += 2;
            continue;
        }
        if let Some(end) = angle_ends.get(&i) {
            i = *end;
            continue;
        }
        if bytes[i] == b'[' {
            if let Some(parent) = stack.last() {
                nested_brackets.insert(*parent);
            }
            stack.push(i);
        }
        if bytes[i] == b']' {
            if let Some(start) = stack.pop() {
                bracket_ends.insert(start, i);
            }
        }
        i += 1;
    }
    i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' {
            i += 2;
            continue;
        }
        let mut end = None;
        if bytes[i] == b'{' && mask[i] == b'{' {
            end = read_djot_word_attributes(source, i).map(|value| value.0);
        }
        if bytes[i] == b'!' && mask[i] == b'!' && bytes.get(i + 1) == Some(&b'[') {
            if let Some(close) = bracket_ends.get(&(i + 1)) {
                if bytes
                    .get(close + 1)
                    .is_some_and(|byte| matches!(byte, b'(' | b'['))
                {
                    let mut angles = Vec::new();
                    let mut plain = true;
                    let mut at = i + 2;
                    while at < *close {
                        if let Some(end) = angle_ends.get(&at).filter(|end| **end <= *close) {
                            angles.push(at);
                            at = *end;
                            continue;
                        }
                        if b"`{_*~^\\[".contains(&bytes[at]) {
                            plain = false;
                            break;
                        }
                        at += 1;
                    }
                    if plain {
                        image_autolinks.extend(angles);
                    }
                    end = Some(close + 1);
                }
            }
        }
        if bytes[i] == b']' && bytes.get(i + 1) == Some(&b'[') {
            if let Some(close) = bracket_ends.get(&(i + 1)) {
                end = Some(close + 1);
            }
        }
        if bytes[i] == b']' && bytes.get(i + 1) == Some(&b'(') {
            if let Some(close) = paren_ends.get(&(i + 1)) {
                end = Some(close + 1);
            }
        }
        if bytes[i] == b'[' && bytes.get(i + 1) == Some(&b'^') {
            if let Some(close) = bracket_ends.get(&i) {
                if !nested_brackets.contains(&i) {
                    end = Some(close + 1);
                }
            }
        }
        if let Some(end) = end {
            blank_out(&mut mask, i, end);
            i = end;
        } else {
            i += 1;
        }
    }
    let mut out = String::with_capacity(source.len());
    let mut copied = 0;
    let mut line = 0;
    let mut offset = 0;
    for capture in cached_regex!(r"<([^<>\s]+)>")
        .unwrap()
        .captures_iter(source)
    {
        let matched = capture.get(0).unwrap();
        let at = matched.start();
        while offset < at {
            if bytes[offset] == b'\n' {
                line += 1;
            }
            offset += 1;
        }
        let image = image_autolinks.contains(&at) && code_mask.as_bytes()[at] == b'<';
        if (!image && mask[at] != b'<') || is_escaped(bytes, at) {
            continue;
        }
        let body = capture[1].to_owned();
        // A body that already carries a scheme is a URL, not a bare address: djot.js
        // prefixes it with a second `mailto:` (jgm/djot.js#162), which we do not copy.
        let address = email.is_match(&body) && !scheme_pattern.is_match(&body);
        // Such an address keeps its autolink, but only where Carve reads one back: a dash
        // run or an ellipsis inside it becomes punctuation instead.
        let written = body.contains(['[', ']', '{', '}', '`', '|', '\\'])
            || (address && body.contains(':'))
            || (!address && email.is_match(&body) && (body.contains("--") || body.contains("...")));
        if !valid_angle.is_match(&body) || (!image && !written) {
            continue;
        }
        if rows[line] && body.contains(['|', '`']) {
            continue;
        }
        let mut label = String::new();
        for ch in body.chars() {
            if ch.is_ascii_punctuation() {
                label.push('\\');
            }
            label.push(ch);
        }
        let destination = if address {
            format!("mailto:{body}")
        } else {
            body.clone()
        };
        let authority = authority_pattern
            .find(&destination)
            .map_or(0, |value| value.end());
        let mut target = String::new();
        for (at, ch) in destination.char_indices() {
            match ch {
                '`' => target.push_str("%60"),
                '|' => target.push_str("%7C"),
                '\\' => target.push_str("\\\\"),
                '[' if at >= authority => target.push_str("%5B"),
                ']' if at >= authority => target.push_str("%5D"),
                '(' => target.push_str("%28"),
                ')' => target.push_str("%29"),
                _ => target.push(ch),
            }
        }
        out.push_str(&source[copied..at]);
        if image {
            out.push_str(&label);
        } else {
            out.push_str(&format!("[{label}]({target})"));
        }
        copied = matched.end();
    }
    out.push_str(&source[copied..]);
    out
}

fn djot_content_start(line: &str) -> usize {
    let bytes = line.as_bytes();
    let mut at = 0;
    let marker_re = cached_regex!(
        r"^(?:\[\^[^\]\n]+\]:(?:[ \t]+|[\r\n]*$)|(?:[-*+]|(?:[0-9]+|[A-Za-z]|[ivxlcdm]{2,}|[IVXLCDM]{2,})[.)]|\((?:[0-9]+|[A-Za-z]|[ivxlcdm]{2,}|[IVXLCDM]{2,})\)|:)[ \t]+)"
    )
    .unwrap();
    while at < bytes.len() {
        while matches!(bytes.get(at), Some(b' ' | b'\t')) {
            at += 1;
        }
        if bytes.get(at) == Some(&b'>')
            && bytes
                .get(at + 1)
                .map_or(true, |b| matches!(b, b' ' | b'\t'))
        {
            at += 1;
            continue;
        }
        let marker = marker_re.find(&line[at..]);
        match marker {
            Some(marker) => {
                at += marker.end();
                let task = &bytes[at..];
                if matches!(marker.as_str(), "- " | "* " | "+ ")
                    && task.len() >= 4
                    && task[0] == b'['
                    && matches!(task[1], b' ' | b'x' | b'X')
                    && task[2] == b']'
                    && matches!(task[3], b' ' | b'\t')
                {
                    at += 3;
                    while matches!(bytes.get(at), Some(b' ' | b'\t')) {
                        at += 1;
                    }
                }
            }
            None => break,
        }
    }
    at
}

fn djot_table_rows(source: &str, mask: &str) -> Vec<bool> {
    djot_table_rows_with_empty_note(source, mask, None)
}

fn djot_table_rows_with_empty_note(
    source: &str,
    mask: &str,
    empty_note: Option<&str>,
) -> Vec<bool> {
    let _paragraph_cache = ParagraphCacheScope::new(source);
    let item_re = cached_regex!(r"(?:[-*+]|[0-9A-Za-z]+[.)]|\([0-9A-Za-z]+\)|:)[ \t]+").unwrap();
    let boundary_re = cached_regex!(r"^(?:#{1,6} |`{3,}|~{3,}|:{3,}|\{|\[[^\]]+\]:)").unwrap();
    let nested_note_label = cached_regex!(r"\[\^[^\]\n]+\]:[ \t]*").unwrap();
    let structural_quote = cached_regex!(r">[ \t]").unwrap();
    let mut inline_note: Option<(usize, usize)> = None;
    let mut offset = 0;
    let mut previous_row = false;
    let mut previous_block = true;
    let mut footnote_column = None;
    let mut note_container: Option<(usize, usize)> = None;
    let mut note_item: Option<(usize, usize, usize)> = None;
    let physical_quote = cached_regex!(r"^(?:[ \t]*>(?:[ \t]|$))*").unwrap();
    let actual_item = cached_regex!(r"^(?:[-*+]|(?:[0-9]+|[A-Za-z]|[ivxlcdm]+|[IVXLCDM]+)[.)]|\((?:[0-9]+|[A-Za-z]|[ivxlcdm]+|[IVXLCDM]+)\))[ \t]+").unwrap();
    let mut item_owners: Vec<(usize, usize)> = Vec::new();
    let mut held_quote = 0;
    let note = cached_regex!(r"^\[\^[^\]\n]+\]:").unwrap();
    source
        .split('\n')
        .map(|line| {
            let at = djot_content_start(line);
            let bytes = line.as_bytes();
            let end = line.trim_end().len().saturating_sub(1);
            let content = &line[at..];
            let prefix = physical_quote.find(line).unwrap().as_str();
            let depth = prefix.bytes().filter(|&b| b == b'>').count();
            let body = &line[prefix.len()..];
            let indent = body
                .bytes()
                .take_while(|b| matches!(b, b' ' | b'\t'))
                .count();
            let text = &body[indent..];
            let visible_quote = prefix
                .find('>')
                .is_some_and(|at| mask.as_bytes().get(offset + at) == Some(&b'>'));
            let closes_note = footnote_column.is_some_and(|column| at < column);
            let literal_quote_blank =
                depth > held_quote && !previous_block && !closes_note && content.trim().is_empty();
            let closes_quote = depth < held_quote && content.starts_with('|');
            if previous_row && !content.trim().is_empty() {
                while item_owners.last().is_some_and(|&(owner_depth, column)| {
                    depth < owner_depth || depth == owner_depth && indent <= column
                }) {
                    item_owners.pop();
                }
            }
            let closes_item = item_owners.last().is_some_and(|&(owner_depth, column)| {
                depth < owner_depth || depth == owner_depth && indent <= column
            }) && content.starts_with('|');
            let opens_item = actual_item.is_match(text)
                && (previous_block
                    || previous_row
                    || item_owners.last().is_some_and(|&(owner_depth, column)| {
                        depth == owner_depth && indent <= column
                    }));
            if opens_item || closes_item {
                while item_owners.last().is_some_and(|&(owner_depth, column)| {
                    depth < owner_depth || depth == owner_depth && indent <= column
                }) {
                    item_owners.pop();
                }
                if opens_item {
                    item_owners.push((depth, indent));
                }
            }
            if visible_quote && (previous_block || closes_note || held_quote > 0) {
                held_quote = depth;
            } else if depth == 0 && previous_block && !content.trim().is_empty() {
                held_quote = 0;
            }
            if opens_item {
                held_quote = depth
                    + nested_note_label
                        .replace_all(&text[..djot_content_start(text)], "")
                        .bytes()
                        .filter(|&byte| byte == b'>')
                        .count();
            }
            let closes_container = note_container.is_some_and(|(column, owner_depth)| {
                !content.trim().is_empty()
                    && if owner_depth == 0 {
                        at <= column
                    } else {
                        depth < owner_depth
                    }
            });
            if closes_container {
                note_container = None;
            }
            if closes_note {
                footnote_column = None;
                note_container = None;
            }
            let closes_note_item = note_item.is_some_and(|(column, owner_depth, owner_prefix)| {
                let current = if depth > owner_depth {
                    prefix
                        .match_indices('>')
                        .nth(owner_depth)
                        .map_or(0, |(at, _)| at)
                        .saturating_sub(owner_prefix)
                } else {
                    (indent + prefix.len()).saturating_sub(owner_prefix)
                };
                content.starts_with('|') && (depth < owner_depth || current <= column)
            });
            if closes_note_item || closes_note {
                note_item = None;
            }
            let closes_inline_note = inline_note.is_some_and(|(owner_depth, column)| {
                !content.trim().is_empty()
                    && (depth < owner_depth || depth == owner_depth && indent <= column)
            });
            if closes_inline_note {
                inline_note = None;
            }
            let allowed = closes_inline_note
                || previous_block
                || previous_row
                || item_re.is_match(&line[..at])
                || closes_note
                || closes_container
                || closes_quote
                || closes_item
                || closes_note_item;
            if allowed {
                let note_at = line[..at].find("[^");
                if let Some((note_at, m)) = note_at.and_then(|note_at| {
                    note.find(&line[note_at..])
                        .filter(|m| {
                            mask.as_bytes().get(offset + note_at + m.end() - 1) == Some(&b':')
                        })
                        .map(|m| (note_at, m))
                }) {
                    let note_end = note_at + m.end();
                    if let Some(nested) = nested_note_label.find_iter(&line[note_end..at]).last() {
                        let nested_at = note_end + nested.start();
                        let structural = nested_note_label
                            .replace_all(&line[..nested_at], |m: &regex::Captures<'_>| {
                                " ".repeat(m[0].len())
                            });
                        let quotes: Vec<_> = structural_quote.find_iter(&structural).collect();
                        let quote_end = quotes.last().map_or(0, |q| q.end());
                        inline_note = Some((
                            quotes.len(),
                            line[quote_end..nested_at].encode_utf16().count(),
                        ));
                    }
                    let note_depth = line[..note_at].bytes().filter(|&byte| byte == b'>').count();
                    footnote_column = Some(note_at + 2);
                    let after = &line[note_end..];
                    let padding = after
                        .bytes()
                        .take_while(|b| matches!(b, b' ' | b'\t'))
                        .count();
                    let body = &after[padding..];
                    note_item = actual_item.is_match(body).then_some((
                        line[note_at..note_end].encode_utf16().count() + padding,
                        note_depth,
                        note_at,
                    ));
                    held_quote = note_depth
                        + nested_note_label
                            .replace_all(&body[..djot_content_start(body)], "")
                            .bytes()
                            .filter(|&byte| byte == b'>')
                            .count();
                    if actual_item.is_match(body)
                        || body.starts_with('>')
                            && body
                                .as_bytes()
                                .get(1)
                                .map_or(true, |b| matches!(b, b' ' | b'\t'))
                    {
                        let inline_depth = nested_note_label
                            .replace_all(&body[..djot_content_start(body)], "")
                            .bytes()
                            .filter(|&byte| byte == b'>')
                            .count();
                        note_container = Some((
                            line[..note_end].encode_utf16().count() + padding,
                            if inline_depth > 0 {
                                note_depth + inline_depth
                            } else {
                                0
                            },
                        ));
                    }
                }
            }
            let row = allowed
                && bytes.get(at) == Some(&b'|')
                && mask.as_bytes().get(offset + at) == Some(&b'|')
                && bytes.get(end) == Some(&b'|')
                && mask.as_bytes().get(offset + end) == Some(&b'|')
                && end.checked_sub(1).and_then(|p| bytes.get(p)) != Some(&b'\\');
            previous_block = !literal_quote_blank
                && (content.trim().is_empty()
                    || empty_note.is_some_and(|marker| content.trim() == marker)
                    || boundary_re.is_match(content)
                    || cached_regex!(r"^(?:[-*][ \t]*){3,}$")
                        .unwrap()
                        .is_match(content));
            previous_row = row;
            offset += line.len() + 1;
            row
        })
        .collect()
}

fn rename_djot_pipe_footnotes(source: &str) -> String {
    let _paragraph_cache = ParagraphCacheScope::new(source);
    let mask = mask_djot_inline(source, false);
    let prefix = "carve-djot-footnote-";
    let reserved: std::collections::HashSet<usize> =
        cached_regex!(r"(?i)carve-djot-footnote-(\d+)")
            .unwrap()
            .captures_iter(source)
            .filter_map(|caps| caps[1].parse().ok())
            .collect();
    let mut serial = 0usize;
    let spaces = cached_regex!(r"\s+").unwrap();
    let mut labels = std::collections::HashMap::new();
    let mut definition_ends = std::collections::HashMap::new();
    let definitions = cached_regex!(
        r"(?m)^[ \t]*(?:>[ \t]*)*(?:(?:[-*+]|[0-9A-Za-z]+[.)])[ \t]+)?\[\^([^\]\n]+)\]:"
    )
    .unwrap();
    for caps in definitions.captures_iter(source) {
        let value = caps.get(0).unwrap();
        let at = value.start() + value.as_str().find('[').unwrap();
        let key = spaces.replace_all(&caps[1], " ").trim().to_owned();
        if mask.as_bytes()[at] == b'[' {
            definition_ends.insert(at, at + 2 + caps[1].len());
        }
        if mask.as_bytes()[at] == b'['
            && !key.contains('[')
            && key.contains('|')
            && !labels.contains_key(&key)
        {
            while reserved.contains(&serial) {
                serial += 1;
            }
            labels.insert(key, format!("{prefix}{serial}"));
            serial += 1;
        }
    }
    if labels.is_empty() && !source.contains("[^") {
        return source.to_owned();
    }
    let item_re = cached_regex!(r"(?:[-*+]|[0-9A-Za-z]+[.)]|\([0-9A-Za-z]+\)|:)[ \t]+").unwrap();
    let boundary_re = cached_regex!(r"^(?:#{1,6} |`{3,}|~{3,}|:{3,}|\{|\[[^\]]+\]:)").unwrap();
    let mut parens = std::collections::HashMap::new();
    let mut stack = Vec::new();
    let mut row_ranges = Vec::new();
    let mut line_offset = 0;
    let mut previous_row = false;
    let mut previous_block = true;
    for line in source.split('\n') {
        let at = djot_content_start(line);
        let bytes = line.as_bytes();
        let end = line.trim_end().len().saturating_sub(1);
        let content = &line[at..];
        let row = (previous_block || previous_row || item_re.is_match(&line[..at]))
            && bytes.get(at) == Some(&b'|')
            && mask.as_bytes().get(line_offset + at) == Some(&b'|')
            && bytes.get(end) == Some(&b'|')
            && mask.as_bytes().get(line_offset + end) == Some(&b'|')
            && end.checked_sub(1).and_then(|p| bytes.get(p)) != Some(&b'\\');
        previous_block = content.trim().is_empty() || boundary_re.is_match(content);
        previous_row = row;
        if row {
            row_ranges.push((line_offset + at, line_offset + bytes.len()));
        }
        if row || content.trim().is_empty() {
            stack.clear();
        }
        let mut i = 0;
        while i < bytes.len() {
            if mask.as_bytes()[line_offset + i] == b' ' {
                i += 1;
                continue;
            }
            if bytes[i] == b'{' {
                if let Some((end, _)) = read_djot_word_attributes(line, i) {
                    i = end;
                    continue;
                }
            }
            if bytes[i] == b'\\' && bytes.get(i + 1).is_some_and(u8::is_ascii_punctuation) {
                i += 2;
                continue;
            }
            if row
                && bytes[i] == b'|'
                && i.checked_sub(1).and_then(|p| bytes.get(p)) != Some(&b'\\')
            {
                stack.clear();
            } else if bytes[i] == b'(' {
                stack.push(line_offset + i);
            } else if bytes[i] == b')' {
                if let Some(open) = stack.pop() {
                    parens.insert(open, line_offset + i);
                }
            }
            i += 1;
        }
        if row {
            stack.clear();
        }
        line_offset += line.len() + 1;
    }
    let mut row_index = 0;
    let bytes = source.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut images = Vec::new();
    let mut image_depth = 0;
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\n' {
            let mut next = i + 1;
            while matches!(bytes.get(next), Some(b' ' | b'\t')) {
                next += 1;
            }
            if bytes.get(next) == Some(&b'\n') {
                images.clear();
                image_depth = 0;
            }
        }
        if mask.as_bytes()[i] == b'{' {
            if let Some((end, _)) = read_djot_word_attributes(source, i) {
                output.extend_from_slice(&bytes[i..end]);
                i = end;
                continue;
            }
        }
        if mask.as_bytes()[i] == b' ' {
            output.push(bytes[i]);
            i += 1;
            continue;
        }
        if bytes[i] == b'\\' {
            output.push(bytes[i]);
            i += 1;
            if bytes.get(i).is_some_and(u8::is_ascii_punctuation) {
                output.push(bytes[i]);
                i += 1;
            }
            continue;
        }
        while row_ranges.get(row_index).is_some_and(|(_, end)| *end <= i) {
            row_index += 1;
        }
        let in_row = row_ranges
            .get(row_index)
            .is_some_and(|(start, _)| *start <= i);
        if in_row && bytes[i] == b'|' && i.checked_sub(1).and_then(|p| bytes.get(p)) != Some(&b'\\')
        {
            images.clear();
            image_depth = 0;
        }
        if bytes[i..].starts_with(b"[^") {
            let mut end = i + 2;
            while end < bytes.len() && !matches!(bytes[end], b'[' | b']' | b'\n') {
                end += 1;
            }
            if bytes.get(end) == Some(&b'[') {
                if let Some(&definition_end) = definition_ends.get(&i) {
                    output.extend_from_slice(&bytes[i..=definition_end]);
                    i = definition_end + 1;
                    continue;
                }
                output.extend_from_slice(b"\\[^");
                i += 2;
                continue;
            }
            if bytes.get(end) == Some(&b']') {
                let label = &source[i + 2..end];
                let raw_pipe = in_row
                    && label.as_bytes().iter().enumerate().any(|(at, &ch)| {
                        ch == b'|' && (at == 0 || label.as_bytes()[at - 1] != b'\\')
                    });
                if raw_pipe {
                    images.clear();
                    image_depth = 0;
                }
                let renamed = if image_depth == 0 && !raw_pipe {
                    labels.get(spaces.replace_all(&source[i + 2..end], " ").trim())
                } else {
                    None
                };
                match renamed {
                    Some(label) => output.extend_from_slice(format!("[^{label}]").as_bytes()),
                    None => output.extend_from_slice(&bytes[i..=end]),
                }
                i = end + 1;
                continue;
            }
        }
        if bytes[i] == b'[' {
            let image = i > 0 && bytes[i - 1] == b'!';
            images.push(image);
            if image {
                image_depth += 1;
            }
        } else if bytes[i] == b']' && !images.is_empty() {
            if images.pop() == Some(true) {
                image_depth -= 1;
            }
            if bytes.get(i + 1) == Some(&b'(') {
                if let Some(&end) = parens.get(&(i + 1)) {
                    output.extend_from_slice(&bytes[i..=end]);
                    i = end + 1;
                    continue;
                }
            }
        }
        output.push(bytes[i]);
        i += 1;
    }
    String::from_utf8(output).expect("only ASCII syntax changed")
}

fn close_djot_table_code(
    source: &str,
    only_closed: bool,
    on_raw_code: Option<&mut dyn FnMut(usize)>,
) -> String {
    let _paragraph_cache = ParagraphCacheScope::new(source);
    if !source.contains('`') {
        return source.to_owned();
    }
    let mask = mask_djot_opaque(
        &mask_djot_fences(source, None, &[], true),
        OpaqueOptions {
            code: false,
            unclosed_code: false,
            ..OpaqueOptions::default()
        },
    );
    let syntax = TableCodeSyntax::get();
    let ticks = TableCodeTicks::new(source, syntax.ticks);
    let (parens, labels) = djot_bracket_pairs(source.as_bytes());
    let image_paragraph_valid = source
        .contains("![")
        .then(|| ImageBoundaryIndex::new(source, &mask));
    let mut closer = TableCodeCloser {
        source,
        bytes: source.as_bytes(),
        mask,
        only_closed,
        on_raw_code,
        syntax,
        ticks,
        parens,
        labels,
        image_paragraph_valid,
        offset: 0,
        cursor: 0,
        consumed: 0,
        item_column: 0,
        item_marker_column: 0,
        item_quote: 0,
        previous_quote: 0,
        item_kind: String::new(),
        code_host_depth: 0,
        previous_block: true,
        fenced: None,
        divs: Vec::new(),
        output: String::new(),
    };
    for line in source.split('\n') {
        closer.close_line(line);
    }
    closer.output.push_str(&source[closer.cursor..]);
    closer.output
}

fn djot_block_attributes(value: &str) -> bool {
    value.starts_with('{')
        && read_djot_word_attributes(value, 0)
            .is_some_and(|(end, _)| value[end..].trim_matches([' ', '\t']).is_empty())
}

fn djot_item_kind(item: &str) -> String {
    let digits = cached_regex!(r"^[0-9]+").unwrap();
    let letters = cached_regex!(r"^[A-Za-z]+").unwrap();
    letters
        .replace(&digits.replace(item, "1"), "a")
        .trim()
        .to_owned()
}

fn djot_item_width(item: &regex::Match<'_>) -> usize {
    if item.as_str().starts_with("[^") {
        2
    } else {
        item.len()
    }
}

fn djot_bracket_pairs(bytes: &[u8]) -> (HashMap<usize, usize>, HashMap<usize, usize>) {
    let mut parens = std::collections::HashMap::new();
    let mut labels = std::collections::HashMap::new();
    let mut parenthesis_stack = Vec::new();
    let mut bracket_stack = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] == b'\\' && bytes.get(at + 1).is_some_and(u8::is_ascii_punctuation) {
            at += 2;
            continue;
        }
        match bytes[at] {
            b'(' => parenthesis_stack.push(at),
            b')' => {
                if let Some(open) = parenthesis_stack.pop() {
                    parens.insert(open, at);
                }
            }
            b'[' => bracket_stack.push(at),
            b']' => {
                if let Some(open) = bracket_stack.pop() {
                    labels.insert(open, at);
                }
            }
            _ => {}
        }
        at += 1;
    }
    (parens, labels)
}

/// Returns the quote depth, continuation prefix and tab flag of a line's nested hosts.
fn djot_code_prefix(line: &str, marker: &regex::Regex) -> (usize, String, bool) {
    let mut code_depth = 0;
    let mut code_prefix = String::new();
    let mut marker_has_tab = false;
    let mut prefix_view = line;
    loop {
        let (quotes, content) = quoted(prefix_view);
        code_depth += quotes;
        if quotes > 0 {
            code_prefix.push_str(&prefix_view[..prefix_view.len() - content.len()]);
        }
        let rest = content.trim_start_matches([' ', '\t']);
        let Some(item_prefix) = marker.find(rest) else {
            break;
        };
        code_prefix.push_str(&content[..content.len() - rest.len()]);
        code_prefix.push_str(&" ".repeat(djot_item_width(&item_prefix)));
        marker_has_tab |= item_prefix.as_str().contains('\t');
        prefix_view = &rest[item_prefix.end()..];
        if matches!(
            item_prefix.as_str().as_bytes().first(),
            Some(b'-' | b'*' | b'+')
        ) && (prefix_view.starts_with("[ ] ")
            || prefix_view.starts_with("[x] ")
            || prefix_view.starts_with("[X] "))
        {
            prefix_view = &prefix_view[4..];
        }
    }
    (code_depth, code_prefix, marker_has_tab)
}

fn normalize_djot_code_lines(
    payload: &str,
    code_depth: usize,
    quote_re: &regex::Regex,
) -> Vec<String> {
    payload
        .split('\n')
        .enumerate()
        .map(|(n, value)| {
            if n == 0 {
                return value.to_owned();
            }
            let mut rest = value;
            for _ in 0..code_depth {
                let Some(prefix) = quote_re.find(rest) else {
                    break;
                };
                rest = &rest[prefix.end()..];
            }
            rest.trim_start_matches([' ', '\t']).to_owned()
        })
        .collect()
}

/// Reads attributes after a raw code span and returns their end and the rendered code tag.
fn raw_djot_code_tag(source: &str, mut code_end: usize) -> (usize, String) {
    let bytes = source.as_bytes();
    let mut code_tag = "<code>".to_owned();
    let mut parts = Vec::new();
    while bytes.get(code_end) == Some(&b'{') {
        let Some((attr_end, attrs)) = read_djot_attributes(source, code_end, false, false) else {
            break;
        };
        parts.push(attrs[1..attrs.len() - 1].to_owned());
        code_end = attr_end;
    }
    if !parts.is_empty() {
        let html = crate::to_html(&format!("`X`{{{}}}", parts.join(" ")));
        if let Some(tag) = cached_regex!(r"<code\b[^>]*>").unwrap().find(&html) {
            code_tag = tag.as_str().to_owned();
        }
    }
    (code_end, code_tag)
}

fn raw_djot_code_html(code_tag: String, normalized: &[String]) -> String {
    let mut html = code_tag;
    for ch in normalized.join("\n").chars() {
        if ch.is_ascii_punctuation() || ch == '\n' {
            html.push_str(&format!("&#{};", ch as u32));
        } else {
            html.push(ch);
        }
    }
    html.push_str("</code>");
    html
}

struct TableCodeSyntax {
    ticks: &'static regex::Regex,
    block: &'static regex::Regex,
    marker: &'static regex::Regex,
    autolink: &'static regex::Regex,
    div: &'static regex::Regex,
    close_div: &'static regex::Regex,
    quote: &'static regex::Regex,
    separator: &'static regex::Regex,
    fence_open: &'static regex::Regex,
    boundary: &'static regex::Regex,
}

impl TableCodeSyntax {
    fn get() -> Self {
        Self {
            ticks: cached_regex!(r"`+").unwrap(),
            block: cached_regex!(r"^(?:[-*+] |(?:[0-9]+|[A-Za-z]|[ivxlcdm]+|[IVXLCDM]+)[.)] |\((?:[0-9]+|[A-Za-z]|[ivxlcdm]+|[IVXLCDM]+)\) |: |#{1,6} |`{3,}|~{3,}|:{3,}(?:[ \t]*[A-Za-z0-9_-]+)?[ \t]*$|>(?:[ \t]|$)|\||\^ |\[[^\]]+\]:)").unwrap(),
            marker: cached_regex!(
                r"^(?:\[\^[^\]\n]+\]:[ \t]*|(?:[-*+]|(?:[0-9]+|[A-Za-z]|[ivxlcdm]+|[IVXLCDM]+)[.)]|\((?:[0-9]+|[A-Za-z]|[ivxlcdm]+|[IVXLCDM]+)\)|:)[ \t]+)"
            )
            .unwrap(),
            autolink: cached_regex!(r"^<(?:[A-Za-z][A-Za-z0-9+.-]*:[^<>\s]*|[^<>\s@]+@[^<>\s]+)>")
                .unwrap(),
            div: cached_regex!(r"^(:{3,})(?:[ \t]+\S.*)?[ \t]*$").unwrap(),
            close_div: cached_regex!(r"^:{3,}[ \t]*$").unwrap(),
            quote: cached_regex!(r"^[ \t]*>(?:[ \t]|$)").unwrap(),
            separator: cached_regex!(r"^\|[ \t:|-]*\|$").unwrap(),
            fence_open: cached_regex!(r"^(`{3,}|~{3,})[ \t]*=?[a-zA-Z0-9_+#.-]*$").unwrap(),
            boundary: cached_regex!(
                r"^(?:#{1,6} |:{3,}(?:[ \t]*[A-Za-z0-9_-]+)?[ \t]*$|\[[^\]]+\]:)"
            )
            .unwrap(),
        }
    }
}

/// Backtick runs of the source and the closing span of every tick before the last link end.
struct TableCodeTicks {
    runs: HashMap<usize, Vec<usize>>,
    len: usize,
    starts: Vec<usize>,
    next: Vec<usize>,
    ends: Vec<usize>,
    jumps: Vec<usize>,
}

impl TableCodeTicks {
    fn new(source: &str, ticks: &regex::Regex) -> Self {
        let bytes = source.as_bytes();
        let mut runs: std::collections::HashMap<usize, Vec<usize>> =
            std::collections::HashMap::new();
        for run in ticks.find_iter(source) {
            runs.entry(run.len()).or_default().push(run.start());
        }
        let mut tick_starts = Vec::new();
        let mut tick_widths = Vec::new();
        let query_end = if source.contains("![") || source.contains("[^") || source.contains("](") {
            source
                .rfind(']')
                .unwrap_or(0)
                .max(source.rfind(')').unwrap_or(0))
        } else {
            0
        };
        let mut at = 0;
        while at < query_end {
            if bytes[at] == b'\\' && bytes.get(at + 1).is_some_and(u8::is_ascii_punctuation) {
                at += 2;
                continue;
            }
            if bytes[at] != b'`' {
                at += 1;
                continue;
            }
            let width = bytes[at..].iter().take_while(|&&b| b == b'`').count();
            tick_starts.push(at);
            tick_widths.push(width);
            at += width;
        }
        let mut spans = Self {
            runs,
            len: bytes.len(),
            starts: tick_starts,
            next: Vec::new(),
            ends: Vec::new(),
            jumps: Vec::new(),
        };
        let count = spans.starts.len();
        let mut next = vec![count; count + 1];
        let mut ends = vec![bytes.len() + 1; count + 1];
        let mut jumps = vec![count; count + 1];
        let mut depths = vec![0; count + 1];
        // Merge equal-length ancestor jumps so each tick stores one pointer.
        for at in (0..count).rev() {
            let finish = spans.close_run(spans.starts[at], tick_widths[at]) + tick_widths[at];
            let parent = spans.index(finish);
            let jump = jumps[parent];
            let farther = jumps[jump];
            next[at] = parent;
            ends[at] = finish;
            depths[at] = depths[parent] + 1;
            jumps[at] = if depths[parent] - depths[jump] == depths[jump] - depths[farther] {
                farther
            } else {
                parent
            };
        }
        spans.next = next;
        spans.ends = ends;
        spans.jumps = jumps;
        spans
    }

    fn close_run(&self, open: usize, width: usize) -> usize {
        self.runs
            .get(&width)
            .and_then(|positions| positions.get(positions.partition_point(|&at| at < open + width)))
            .copied()
            .unwrap_or(self.len)
    }

    fn index(&self, position: usize) -> usize {
        self.starts.partition_point(|&at| at < position)
    }

    fn balanced(&self, from: usize, end: usize) -> bool {
        let count = self.starts.len();
        let mut at = self.index(from);
        while at < count && self.starts[at] < end {
            let jump = self.jumps[at];
            if jump < count && self.ends[jump] <= end {
                at = jump;
            } else if self.ends[at] <= end {
                at = self.next[at];
            } else {
                return false;
            }
        }
        true
    }
}

struct TableCodeLine<'l> {
    line: &'l str,
    at: usize,
    line_end: usize,
    depth: usize,
    code_depth: usize,
    code_prefix: String,
    marker_has_tab: bool,
    trimmed: &'l str,
    indent: usize,
}

struct TableCodeCloser<'a, 'f> {
    source: &'a str,
    bytes: &'a [u8],
    mask: Vec<u8>,
    only_closed: bool,
    on_raw_code: Option<&'f mut dyn FnMut(usize)>,
    syntax: TableCodeSyntax,
    ticks: TableCodeTicks,
    parens: HashMap<usize, usize>,
    labels: HashMap<usize, usize>,
    image_paragraph_valid: Option<ImageBoundaryIndex>,
    offset: usize,
    cursor: usize,
    consumed: usize,
    item_column: usize,
    item_marker_column: usize,
    item_quote: usize,
    previous_quote: usize,
    item_kind: String,
    code_host_depth: usize,
    previous_block: bool,
    fenced: Option<(usize, u8, usize, usize, usize)>,
    divs: Vec<(usize, usize, usize, usize)>,
    output: String,
}

impl TableCodeCloser<'_, '_> {
    fn close_line(&mut self, line: &str) {
        let marker = self.syntax.marker;
        let mut at = djot_content_start(line);
        if cached_regex!(r"[-*+][ \t]+$")
            .unwrap()
            .is_match(&line[..at])
        {
            if let Some(task) = cached_regex!(r"^\[[ xX]\][ \t]+")
                .unwrap()
                .find(&line[at..])
            {
                at += task.end();
            }
        }
        let line_end = self.offset + line.len();
        let (parsed_depth, content) = quoted(line);
        let (mut code_depth, code_prefix, marker_has_tab) = djot_code_prefix(line, marker);
        let trimmed = content.trim_start_matches([' ', '\t']);
        let indent = content.len() - trimmed.len();
        let item = marker.find(trimmed).filter(|item| {
            !item.as_str().starts_with("[^")
                || self.previous_block
                || parsed_depth < self.previous_quote
                || (self.item_column > 0
                    && parsed_depth == self.item_quote
                    && indent < self.item_column)
        });
        let old_column = self.item_column;
        let old_quote = self.item_quote;
        let began_inside = self.offset < self.consumed;
        let depth = if began_inside {
            self.code_host_depth
        } else {
            parsed_depth
        };
        if began_inside {
            code_depth = self.code_host_depth;
        } else if !self.previous_block
            && parsed_depth < self.previous_quote
            && !trimmed.is_empty()
            && !self.syntax.block.is_match(trimmed)
        {
            code_depth = code_depth.max(self.previous_quote);
        }
        let ctx = TableCodeLine {
            line,
            at,
            line_end,
            depth,
            code_depth,
            code_prefix,
            marker_has_tab,
            trimmed,
            indent,
        };
        if !began_inside && self.track_blocks(&ctx, item, old_column, old_quote) {
            self.offset = line_end + 1;
            return;
        }
        self.scan_inline(&ctx);
        self.previous_block = !began_inside
            && self.consumed <= line_end
            && (trimmed.is_empty()
                || self.syntax.boundary.is_match(trimmed)
                || djot_block_attributes(trimmed)
                || (line.as_bytes().get(at) == Some(&b'|')
                    && line.trim_end().ends_with('|')
                    && !line.trim_end().ends_with("\\|")));
        self.offset = line_end + 1;
    }

    /// Tracks fences, list items, divs and table rows; returns true when the line is skipped.
    fn track_blocks(
        &mut self,
        ctx: &TableCodeLine<'_>,
        item: Option<regex::Match<'_>>,
        old_column: usize,
        old_quote: usize,
    ) -> bool {
        let TableCodeLine {
            line,
            at,
            depth,
            indent,
            ..
        } = *ctx;
        if self.track_fence(ctx, item) {
            return true;
        }
        self.track_item(ctx, item);
        if let Some(div) = self.syntax.div.captures(&line[at..]) {
            if self.divs.last().is_some_and(|&(width, owner_depth, _, _)| {
                depth == owner_depth
                    && self.syntax.close_div.is_match(&line[at..])
                    && div[1].len() >= width
            }) {
                self.divs.pop();
            } else if self.mask[self.offset + at] != b' ' {
                self.divs.push((div[1].len(), depth, at, self.offset));
            }
        }
        if line.as_bytes().get(at) == Some(&b'|')
            && ((old_column > 0 && depth <= old_quote && indent < old_column)
                || depth < self.previous_quote)
        {
            self.output.push_str(&self.source[self.cursor..self.offset]);
            let mut quote_end = 0;
            for _ in 0..depth {
                if let Some(prefix) = self.syntax.quote.find(&line[quote_end..]) {
                    quote_end += prefix.end();
                }
            }
            self.output
                .push_str(line[..quote_end].trim_end_matches([' ', '\t']));
            self.output.push('\n');
            self.cursor = self.offset;
        }
        self.previous_quote = depth.max(ctx.code_depth);
        false
    }

    fn track_fence(&mut self, ctx: &TableCodeLine<'_>, item: Option<regex::Match<'_>>) -> bool {
        let TableCodeLine {
            line,
            at,
            depth,
            trimmed,
            indent,
            ..
        } = *ctx;
        if self
            .fenced
            .is_some_and(|(_, _, owner_depth, _, owner_item)| {
                !trimmed.is_empty()
                    && (depth < owner_depth || (depth == owner_depth && indent < owner_item))
            })
        {
            self.fenced = None;
        }
        if let Some((width, ch, owner_depth, column, _)) = self.fenced {
            let run = trimmed.bytes().take_while(|&b| b == ch).count();
            if depth == owner_depth
                && indent <= column
                && run >= width
                && trimmed[run..].trim_matches([' ', '\t']).is_empty()
            {
                self.fenced = None;
                self.previous_block = true;
            }
            return true;
        }
        if let Some(opening) = self
            .syntax
            .fence_open
            .captures(&line[at..])
            .filter(|_| self.previous_block || item.is_some())
        {
            if item.is_none()
                && (depth < self.item_quote
                    || (depth == self.item_quote && indent < self.item_column))
            {
                self.item_column = indent;
                self.item_quote = depth;
                self.item_kind.clear();
            }
            self.fenced = Some((
                opening[1].len(),
                opening[1].as_bytes()[0],
                depth,
                at,
                item.map_or(self.item_column.min(indent), |item| {
                    indent + djot_item_width(&item)
                }),
            ));
            return true;
        }
        false
    }

    fn track_item(&mut self, ctx: &TableCodeLine<'_>, item: Option<regex::Match<'_>>) {
        let TableCodeLine {
            line,
            depth,
            trimmed,
            indent,
            ..
        } = *ctx;
        if let Some(item) = item {
            self.item_marker_column = indent;
            self.item_column = indent + djot_item_width(&item);
            self.item_quote = depth;
            self.item_kind = djot_item_kind(item.as_str());
        } else if !trimmed.is_empty() && self.item_column > 0 {
            let mut owner_content = line;
            for _ in 0..self.item_quote {
                let Some(prefix) = self.syntax.quote.find(owner_content) else {
                    break;
                };
                owner_content = &owner_content[prefix.end()..];
            }
            let owner_text = owner_content.trim_start_matches([' ', '\t']);
            let owner_indent = owner_content.len() - owner_text.len();
            if depth < self.item_quote
                || (owner_indent < self.item_column
                    && (self.previous_block
                        || self.syntax.block.is_match(owner_text)
                        || djot_block_attributes(owner_text)))
            {
                self.item_column = 0;
                self.item_kind.clear();
            }
        }
    }

    fn scan_inline(&mut self, ctx: &TableCodeLine<'_>) {
        let bytes = self.bytes;
        let line_end = ctx.line_end;
        let mut i = (self.offset + ctx.at).max(self.consumed);
        let mut pending = Vec::new();
        let mut footnotes = Vec::new();
        while i < line_end {
            if let Some(next) = self.skip_inline(i, line_end, ctx.code_depth, &mut footnotes) {
                i = next;
                continue;
            }
            if bytes[i] == b']' {
                footnotes.clear();
            }
            if bytes[i] == b'(' {
                pending.push(i);
            } else if bytes[i] == b')' {
                pending.pop();
            }
            if bytes[i] != b'`' {
                i += self.source[i..].chars().next().unwrap().len_utf8();
                continue;
            }
            let width = bytes[i..].iter().take_while(|&&b| b == b'`').count();
            self.code_host_depth = ctx.code_depth;
            let candidate = self.ticks.close_run(i, width);
            if self.only_closed && candidate >= bytes.len() {
                i += width;
                continue;
            }
            if self.only_closed
                && (ctx.marker_has_tab
                    || (i > 0 && bytes[i - 1] == b'$' && !is_escaped(bytes, i - 1))
                    || bytes.get(candidate + width) == Some(&b'{') && self.item_kind != ":")
            {
                i = candidate + width;
                self.consumed = i;
                continue;
            }
            if candidate + width <= line_end {
                i = candidate + width;
                self.consumed = i;
                continue;
            }
            self.rewrite_code_span(ctx, i, width, candidate, &pending, &footnotes);
            i = self.consumed;
        }
    }

    /// Returns the position after inline syntax that cannot hold a code span opener.
    fn skip_inline(
        &mut self,
        i: usize,
        line_end: usize,
        code_depth: usize,
        footnotes: &mut Vec<usize>,
    ) -> Option<usize> {
        let bytes = self.bytes;
        let source = self.source;
        if self.mask[i] == b' ' && bytes[i] != b' ' {
            return Some(i + 1);
        }
        if bytes[i] == b'\\' && bytes.get(i + 1).is_some_and(u8::is_ascii_punctuation) {
            return Some(i + 2);
        }
        if bytes[i] == b'{' {
            if let Some((end, _)) = read_djot_word_attributes(source, i) {
                return Some(end);
            }
        }
        if bytes[i] == b'<' {
            if let Some(auto) = self.syntax.autolink.find(&source[i..]) {
                return Some(i + auto.len());
            }
        }
        if bytes[i..].starts_with(b"![") {
            if let Some(&end) = self
                .labels
                .get(&(i + 1))
                .filter(|&&end| self.ticks.balanced(i + 2, end))
            {
                if end >= line_end
                    && matches!(bytes.get(end + 1), Some(b'(' | b'['))
                    && self.image_paragraph_valid.as_ref().is_some_and(|index| {
                        index.valid(
                            i,
                            end,
                            code_depth,
                            self.item_column,
                            self.item_quote,
                            self.divs.last().map(|owner| owner.3),
                        )
                    })
                {
                    self.consumed = end + 1;
                    self.code_host_depth = code_depth;
                }
                return Some(end + 1);
            }
        }
        if bytes[i..].starts_with(b"[^") {
            let mut end = i + 2;
            while end < line_end && !matches!(bytes[end], b'[' | b']') {
                end += 1;
            }
            if bytes.get(end) == Some(&b']') && self.ticks.balanced(i + 2, end) {
                return Some(end + 1);
            }
            footnotes.push(i);
        }
        if bytes[i..].starts_with(b"](") {
            if let Some(&end) = self
                .parens
                .get(&(i + 1))
                .filter(|&&end| end <= line_end && self.ticks.balanced(i + 2, end))
            {
                return Some(end + 1);
            }
        }
        None
    }

    /// Finds where an unclosed code span stops and whether a quote separator must follow.
    fn code_span_limit(&self, ctx: &TableCodeLine<'_>, candidate: usize) -> (usize, bool, usize) {
        let source = self.source;
        let bytes = self.bytes;
        let quote_re = self.syntax.quote;
        let block = self.syntax.block;
        let TableCodeLine {
            depth, code_depth, ..
        } = *ctx;
        let item_column = self.item_column;
        let item_kind = self.item_kind.as_str();
        let mut limit = candidate.min(bytes.len());
        let mut separate = false;
        let mut separate_depth = 0;
        let mut next = ctx.line_end + 1;
        while next < limit {
            let newline = source[next..].find('\n').map(|at| next + at);
            let end = newline.unwrap_or(bytes.len());
            let following = &source[next..end];
            let (next_depth, next_content) = quoted(following);
            let next_trimmed = next_content.trim_start_matches([' ', '\t']);
            let mut owner_content = following;
            for _ in 0..self.item_quote {
                let prefix = owner_content.trim_start_matches([' ', '\t']);
                if !prefix.starts_with('>')
                    || !prefix
                        .as_bytes()
                        .get(1)
                        .map_or(true, |b| matches!(b, b' ' | b'\t'))
                {
                    break;
                }
                owner_content = &prefix[if prefix.len() > 1 { 2 } else { 1 }..];
            }
            let owner_indent =
                owner_content.len() - owner_content.trim_start_matches([' ', '\t']).len();
            let outside = (code_depth > 0 && next_depth < code_depth)
                || (item_column > 0 && owner_indent < item_column);
            if (next_depth <= code_depth || outside)
                && next_content.trim_matches([' ', '\t']).is_empty()
            {
                limit = next - 1;
                break;
            }
            let next_indent = next_content.len() - next_trimmed.len();
            let closes_div = self
                .divs
                .last()
                .is_some_and(|&(width, owner_depth, column, _)| {
                    next_depth == owner_depth
                        && next_indent <= column + 3
                        && self.syntax.close_div.is_match(next_trimmed)
                        && next_trimmed.trim().len() >= width
                });
            let mut owner_content = &source[next..end];
            for _ in 0..self.item_quote {
                let Some(prefix) = quote_re.find(owner_content) else {
                    break;
                };
                owner_content = &owner_content[prefix.end()..];
            }
            let owner_indent =
                owner_content.len() - owner_content.trim_start_matches([' ', '\t']).len();
            let scope_edge =
                if item_kind.starts_with("[^") || item_kind == ":" || item_kind.is_empty() {
                    item_column
                } else {
                    self.item_marker_column + 1
                };
            let outside_item = item_column > 0 && owner_indent < scope_edge;
            let outside = (code_depth > 0 && next_depth < code_depth) || outside_item;
            let attributes = djot_block_attributes(next_trimmed);
            if closes_div
                || (outside
                    && (block.is_match(next_trimmed)
                        || attributes
                        || (outside_item
                            && block.is_match(owner_content.trim_start_matches([' ', '\t'])))))
            {
                limit = next - 1;
                let next_item = self
                    .syntax
                    .marker
                    .find(next_trimmed)
                    .map(|item| djot_item_kind(item.as_str()));
                separate =
                    outside && (next_depth < depth || next_item.as_deref() != Some(item_kind));
                separate_depth = depth.min(next_depth);
                break;
            }
            let Some(newline) = newline else {
                break;
            };
            next = newline + 1;
        }
        (limit, separate, separate_depth)
    }

    fn is_raw_code(&self, ctx: &TableCodeLine<'_>, normalized: &[String]) -> bool {
        let item_kind = self.item_kind.as_str();
        normalized.iter().skip(1).any(|value| {
            (self.syntax.block.is_match(value)
                || item_kind == ":"
                    && cached_regex!(r"^[ivxlcdmIVXLCDM]+[.)][ \t]")
                        .unwrap()
                        .is_match(value)
                || value.starts_with(['{', '>'])
                || value.starts_with(":::"))
                && (!self.syntax.separator.is_match(value)
                    || ctx.line.as_bytes().get(ctx.at) != Some(&b'|'))
        }) || (item_kind.starts_with("[^") && normalized.len() > 1)
    }

    fn rewrite_code_span(
        &mut self,
        ctx: &TableCodeLine<'_>,
        i: usize,
        width: usize,
        candidate: usize,
        pending: &[usize],
        footnotes: &[usize],
    ) {
        let source = self.source;
        let bytes = self.bytes;
        let code_depth = ctx.code_depth;
        let (limit, separate, separate_depth) = self.code_span_limit(ctx, candidate);
        let closed = candidate <= limit && candidate < bytes.len();
        let end = if closed { candidate } else { limit };
        let mut payload = &source[i + width..end];
        if !closed {
            payload = payload.strip_suffix('\n').unwrap_or(payload);
        }
        if payload.starts_with(" `") {
            payload = &payload[1..];
        }
        if payload.ends_with("` ") {
            payload = &payload[..payload.len() - 1];
        }
        let normalized = normalize_djot_code_lines(payload, code_depth, self.syntax.quote);
        let raw_code = self.is_raw_code(ctx, &normalized);
        let quoted_payload = (code_depth > 0 && !raw_code).then(|| {
            normalized
                .iter()
                .enumerate()
                .map(|(line, value)| {
                    if line == 0 {
                        value.clone()
                    } else {
                        format!("{}{value}", ctx.code_prefix)
                    }
                })
                .collect::<Vec<_>>()
                .join("\n")
        });
        if let Some(quoted) = quoted_payload.as_deref() {
            payload = quoted;
        }
        let mut code_end = if closed { end + width } else { end };
        let mut code_tag = "<code>".to_owned();
        if raw_code && closed && bytes.get(code_end) == Some(&b'{') {
            (code_end, code_tag) = raw_djot_code_tag(source, code_end);
        }
        let raw_payload = if raw_code {
            if let Some(callback) = self.on_raw_code.as_mut() {
                callback(i);
            }
            raw_djot_code_html(code_tag, &normalized)
        } else if code_depth > 0 {
            normalized.join("\n")
        } else {
            String::new()
        };
        if raw_code || code_depth > 0 {
            payload = &raw_payload;
        }
        let fence_width = self
            .syntax
            .ticks
            .find_iter(payload)
            .map(|run| run.len() + 1)
            .max()
            .unwrap_or(1);
        let fence = "`".repeat(fence_width);
        let pad = if payload.starts_with('`') || payload.ends_with('`') {
            " "
        } else {
            ""
        };
        let mut escapes: Vec<usize> = pending
            .iter()
            .copied()
            .filter(|&k| k > 0 && bytes[k - 1] == b']')
            .chain(footnotes.iter().copied())
            .collect();
        escapes.sort_unstable();
        for escape in escapes {
            self.output.push_str(&source[self.cursor..escape]);
            self.output.push('\\');
            self.cursor = escape;
        }
        self.output.push_str(&source[self.cursor..i]);
        self.output.push_str(&format!(
            "{fence}{pad}{payload}{pad}{fence}{}",
            if raw_code { "{=html}" } else { "" }
        ));
        self.cursor = code_end;
        self.consumed = self.cursor;
        if separate && !closed {
            self.output.push('\n');
            self.output.push_str("> ".repeat(separate_depth).trim_end());
        }
    }
}

fn escape_djot_non_table_rows(source: &str, empty_note: &str) -> String {
    let _paragraph_cache = ParagraphCacheScope::new(source);
    if !source.contains('|') {
        return source.to_owned();
    }
    let mask = mask_djot_forms_with_options(
        source,
        false,
        true,
        None,
        &[],
        OpaqueOptions {
            destinations: false,
            autolinks: false,
            attribute_values: false,
            ..OpaqueOptions::default()
        },
    );
    let rows = djot_table_rows_with_empty_note(source, &mask, Some(empty_note));
    let mut out = String::with_capacity(source.len());
    let mut offset = 0;
    for (n, line) in source.split('\n').enumerate() {
        if n > 0 {
            out.push('\n');
        }
        let start = djot_content_start(line);
        if !rows[n]
            && line.as_bytes().get(start) == Some(&b'|')
            && line.trim_end().ends_with('|')
            && mask.as_bytes().get(offset + start) == Some(&b'|')
        {
            out.push_str(&line[..start]);
            out.push('\\');
            out.push_str(&line[start..]);
        } else {
            out.push_str(line);
        }
        offset += line.len() + 1;
    }
    out
}

fn normalize_djot_table_pipes(source: &str, empty_note: &str) -> String {
    let _paragraph_cache = ParagraphCacheScope::new(source);
    if !source.contains("\\|") {
        return source.to_owned();
    }
    fn punctuation(byte: Option<&u8>) -> bool {
        byte.is_some_and(u8::is_ascii_punctuation)
    }
    let renamed = rename_djot_pipe_footnotes(source);
    let source = renamed.as_str();
    let mask = mask_djot_forms_with_options(
        source,
        false,
        true,
        None,
        &[],
        OpaqueOptions {
            autolinks: false,
            attribute_values: false,
            destinations: false,
            ..OpaqueOptions::default()
        },
    );
    let mut lines: Vec<String> = source.split('\n').map(str::to_owned).collect();
    let mut offsets = Vec::with_capacity(lines.len());
    let mut offset = 0;
    for line in &lines {
        offsets.push(offset);
        offset += line.len() + 1;
    }
    let definition_re =
        cached_regex!(r"^\[([^\[\]\n^][^\[\]\n]*)\]:[ \t]*([^ \t\r\n]*)\r?$").unwrap();
    let spaces = cached_regex!(r"\s+").unwrap();
    let escapes = cached_regex!(r"\\([!-/:-@\[-`{-~])").unwrap();
    let item_re = cached_regex!(r"(?:[-*+]|[0-9A-Za-z]+[.)]|\([0-9A-Za-z]+\)|:)[ \t]+").unwrap();
    let boundary_re = cached_regex!(r"^(?:#{1,6} |`{3,}|~{3,}|:{3,}|\[[^\]]+\]:)").unwrap();
    let attribute_pipes_re = cached_regex!(r"\\+\|?|\|").unwrap();
    let mut definitions: HashMap<String, (String, String)> = HashMap::new();
    let row_flags = djot_table_rows_with_empty_note(source, &mask, Some(empty_note));
    let thematic_re = cached_regex!(r"^(?:\*[ \t]*){3,}$|^(?:-[ \t]*){3,}$").unwrap();
    for n in 0..lines.len() {
        let line = lines[n].clone();
        let at = djot_content_start(&line);
        if mask.as_bytes().get(offsets[n] + at) != Some(&b'[') {
            continue;
        }
        let Some(definition) = definition_re.captures(&line[at..]) else {
            continue;
        };
        if !definition[1].contains('|') {
            continue;
        }
        let previous_line = if n > 0 { lines[n - 1].as_str() } else { "" };
        let previous_at = djot_content_start(previous_line);
        let previous = previous_line[previous_at..].trim();
        let opens_item = item_re.is_match(&line[..at]);
        if !previous.is_empty()
            && !row_flags.get(n.wrapping_sub(1)).copied().unwrap_or(false)
            && at >= previous_at
            && !thematic_re.is_match(previous)
            && !boundary_re.is_match(previous)
            && !(previous.starts_with('{')
                && read_djot_word_attributes(previous, 0)
                    .is_some_and(|(end, _)| end == previous.len()))
            && !opens_item
            && line[..at].bytes().filter(|&b| b == b'>').count()
                <= previous_line[..previous_at]
                    .bytes()
                    .filter(|&b| b == b'>')
                    .count()
        {
            lines[n] = format!("{}\\{}", &line[..at], &line[at..]);
            continue;
        }
        let mut target = definition[2].to_owned();
        let mut end = n;
        while end + 1 < lines.len() {
            let next = &lines[end + 1];
            let next_at = djot_content_start(next);
            if next_at <= at
                || next[next_at..].is_empty()
                || next[next_at..].chars().any(char::is_whitespace)
                || mask.as_bytes().get(offsets[end + 1] + next_at) == Some(&b' ')
            {
                break;
            }
            target.push_str(&next[next_at..]);
            end += 1;
        }
        if target.is_empty() {
            continue;
        }
        let mut attribute_parts = Vec::new();
        for k in (0..n).rev() {
            let value = lines[k][djot_content_start(&lines[k])..].trim();
            let parsed = if value.starts_with('{') {
                read_djot_word_attributes(value, 0)
            } else {
                None
            };
            if !parsed.is_some_and(|(end, _)| end == value.len()) {
                break;
            }
            attribute_parts.push(value);
        }
        definitions.insert(
            spaces.replace_all(&definition[1], " ").trim().to_owned(),
            (target.clone(), attribute_parts.into_iter().rev().collect()),
        );
        lines[n] = format!("{}[{}]: {}", &line[..at], &definition[1], target);
        for continuation in &mut lines[n + 1..=end] {
            continuation.clear();
        }
    }
    let mut result = Vec::with_capacity(lines.len());
    for (n, line) in lines.iter().enumerate() {
        let at = djot_content_start(line);
        let bytes = line.as_bytes();
        let line_mask = mask
            .as_bytes()
            .get(offsets[n]..offsets[n] + line.len())
            .unwrap_or(&[]);
        let row = bytes.get(at) == Some(&b'|') && line_mask.get(at) == Some(&b'|');
        if !row {
            result.push(line.clone());
            continue;
        }
        if row_flags.get(n) != Some(&true) {
            result.push(format!("{}\\{}", &line[..at], &line[at..]));
            continue;
        }
        let mut brackets = HashMap::new();
        let mut bracket_pairs = Vec::new();
        let mut parens = HashMap::new();
        let mut bracket_stack = Vec::new();
        let mut paren_stack = Vec::new();
        let mut i = at;
        while i < bytes.len() {
            if line_mask.get(i) == Some(&b' ') {
                i += 1;
                continue;
            }
            if bytes[i] == b'{' {
                if let Some((end, _)) = read_djot_word_attributes(line, i) {
                    i = end;
                    continue;
                }
            }
            if bytes[i] == b'|' && i.checked_sub(1).and_then(|p| bytes.get(p)) != Some(&b'\\') {
                bracket_stack.clear();
                paren_stack.clear();
                i += 1;
                continue;
            }
            if bytes[i] == b'\\' {
                let begin = i;
                while bytes.get(i) == Some(&b'\\') {
                    i += 1;
                }
                if (i - begin) % 2 != 0 && punctuation(bytes.get(i)) {
                    i += 1;
                }
                continue;
            }
            match bytes[i] {
                b'[' => bracket_stack.push(i),
                b']' => {
                    if let Some(open) = bracket_stack.pop() {
                        brackets.insert(open, i);
                        bracket_pairs.push((open, i));
                    }
                }
                b'(' => paren_stack.push(i),
                b')' => {
                    if let Some(open) = paren_stack.pop() {
                        parens.insert(open, i);
                    }
                }
                _ => {}
            }
            i += 1;
        }
        let mut destinations = vec![0i32; bytes.len() + 1];
        let mut references: HashMap<usize, (usize, String)> = HashMap::new();
        for (open, close) in bracket_pairs {
            if bytes.get(open + 1) == Some(&b'^') {
                continue;
            }
            let next = close + 1;
            if bytes.get(next) == Some(&b'(') {
                if let Some(&last) = parens.get(&next) {
                    destinations[next + 1] += 1;
                    destinations[last] -= 1;
                }
            } else if bytes.get(next) == Some(&b'[') && !references.contains_key(&open) {
                let Some(&last) = brackets.get(&next) else {
                    continue;
                };
                let label = if last == next + 1 {
                    &line[open + 1..close]
                } else {
                    &line[next + 1..last]
                };
                let decoded = escapes.replace_all(label, "$1");
                let key = spaces.replace_all(&decoded, " ");
                let Some((target, attrs)) = definitions.get(key.trim()) else {
                    continue;
                };
                let target = target
                    .replace('\\', "%5C")
                    .replace('|', "%7C")
                    .replace('(', "%28")
                    .replace(')', "%29")
                    .replace('<', "%3C")
                    .replace('>', "%3E")
                    .replace(' ', "%20");
                let attrs = attribute_pipes_re.replace_all(attrs, |caps: &regex::Captures<'_>| {
                    let value = &caps[0];
                    if value.ends_with('|') && (value.len() - 1) % 2 == 0 {
                        format!("{}\\|", &value[..value.len() - 1])
                    } else {
                        value.to_owned()
                    }
                });
                references.insert(next, (last + 1, format!("({target}){attrs}")));
            }
        }
        let mut inside = 0;
        for count in &mut destinations {
            inside += *count;
            *count = inside;
        }
        let mut output = Vec::with_capacity(bytes.len());
        i = 0;
        while i < bytes.len() {
            if let Some((end, text)) = references.get(&i) {
                output.extend_from_slice(text.as_bytes());
                i = *end;
                continue;
            }
            if line_mask.get(i) == Some(&b' ') || bytes[i] != b'\\' {
                output.push(bytes[i]);
                i += 1;
                continue;
            }
            let begin = i;
            while bytes.get(i) == Some(&b'\\') {
                i += 1;
            }
            let run = i - begin;
            if bytes.get(i) != Some(&b'|') {
                output.extend_from_slice(&bytes[begin..i]);
                if run % 2 != 0 && punctuation(bytes.get(i)) {
                    output.push(bytes[i]);
                    i += 1;
                }
                continue;
            }
            if destinations[begin] > 0 {
                output.extend_from_slice("%5C".repeat(run / 2).as_bytes());
                output.extend_from_slice(b"%7C");
            } else {
                output.extend_from_slice("\\".repeat(run + usize::from(run % 2 == 0)).as_bytes());
                output.push(b'|');
            }
            i += 1;
        }
        result.push(String::from_utf8(output).expect("only ASCII syntax changed"));
    }
    result.join("\n")
}

#[cfg(test)]
#[path = "djot_migrate/destination_mask_tests.rs"]
mod destination_mask_tests;

#[cfg(test)]
#[path = "djot_migrate/placeholder_namespace_tests.rs"]
mod placeholder_namespace_tests;

#[cfg(test)]
#[path = "djot_migrate/plain_image_growth_tests.rs"]
mod plain_image_growth_tests;
