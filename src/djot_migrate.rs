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

#[path = "djot_emphasis.rs"]
mod emphasis;

#[path = "djot_import_normalize.rs"]
mod normalize;

#[path = "djot_div_classes.rs"]
mod div_classes;
#[path = "djot_heading_targets.rs"]
mod heading_targets;

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

fn convert_djot_document(
    stripped_definitions: &DjotFootnoteAttributeStrip,
    omit_quote_delimiters: bool,
    skip_heading_targets: bool,
) -> String {
    let normalized = &stripped_definitions.source;
    let (frontmatter, separator, body) = split_frontmatter(normalized);
    let div_openers = normalize_djot_div_openers(body);
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
    let attributes = normalize_djot_attribute_lines(&literal_attributes);
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
    let converted = div_classes::normalize(&converted);
    stripped_definitions.restore(&if frontmatter.is_empty() {
        converted
    } else if converted.is_empty() {
        format!("{frontmatter}{separator}")
    } else {
        format!("{}{}{}", frontmatter, separator, converted)
    })
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

struct ImageBoundaryIndex {
    offsets: Vec<usize>,
    tree: Vec<usize>,
    size: usize,
    item_tree: Vec<isize>,
    stride: isize,
    div_closers: std::collections::HashMap<usize, usize>,
}

impl ImageBoundaryIndex {
    fn new(source: &str, mask: &[u8]) -> Self {
        let mut offsets = Vec::new();
        let mut thresholds = Vec::new();
        let mut item_thresholds = Vec::new();
        let stride = source.len() as isize + 1;
        let mut div_closers = std::collections::HashMap::new();
        let mut divs: Vec<(usize, usize, usize)> = Vec::new();
        let mut offset = 0;
        let block = cached_regex!(r"^(?:[-*+](?:[ \t]|$)|(?:[0-9]+|[A-Za-z]|[ivxlcdm]+|[IVXLCDM]+)[.)] |\((?:[0-9]+|[A-Za-z]|[ivxlcdm]+|[IVXLCDM]+)\) |: |#{1,6} |`{3,}|~{3,}|:{3,}(?:[ \t]*[A-Za-z0-9_-]+)?[ \t]*$|\||\^ |\[[^\]]+\]:)").unwrap();
        for line in source.split('\n') {
            let (depth, body) = quoted(line);
            let blank = body.trim_matches([' ', '\t']).is_empty();
            let text = body.trim_start_matches([' ', '\t']);
            let indent = body.len() - text.len();
            let attributes = text.starts_with('{')
                && read_djot_word_attributes(text, 0)
                    .is_some_and(|(end, _)| text[end..].trim_matches([' ', '\t']).is_empty());
            let extra_block = cached_regex!(r"^(?:[-*][ \t]*){3,}$")
                .unwrap()
                .is_match(text)
                || attributes;
            if blank || block.is_match(text) || extra_block {
                offsets.push(offset);
                thresholds.push(depth + usize::from(!blank));
                item_thresholds
                    .push(depth as isize * stride + if blank { -1 } else { indent as isize });
            }
            let content_at = djot_content_start(line);
            if let Some(delimiter) = cached_regex!(r"^(:{3,})(?:[ \t]+\S.*)?[ \t]*$")
                .unwrap()
                .captures(&line[content_at..])
            {
                if mask[offset + content_at] != b' ' {
                    if divs.last().is_some_and(|&(width, owner_depth, _)| {
                        depth == owner_depth
                            && cached_regex!(r"^:{3,}[ \t]*$")
                                .unwrap()
                                .is_match(&line[content_at..])
                            && delimiter[1].len() >= width
                    }) {
                        let (_, _, owner_at) = divs.pop().unwrap();
                        div_closers.insert(owner_at, offset);
                    } else {
                        divs.push((delimiter[1].len(), depth, offset));
                    }
                }
            }
            offset += line.len() + 1;
        }
        let size = offsets.len().max(1).next_power_of_two();
        let mut tree = vec![usize::MAX; size * 2];
        let mut item_tree = vec![isize::MAX; size * 2];
        for (at, threshold) in thresholds.into_iter().enumerate() {
            tree[size + at] = threshold;
            item_tree[size + at] = item_thresholds[at];
        }
        for at in (1..size).rev() {
            tree[at] = tree[at * 2].min(tree[at * 2 + 1]);
            item_tree[at] = item_tree[at * 2].min(item_tree[at * 2 + 1]);
        }
        Self {
            offsets,
            tree,
            size,
            item_tree,
            stride,
            div_closers,
        }
    }

    fn valid(
        &self,
        start: usize,
        end: usize,
        depth: usize,
        item_column: usize,
        item_quote: usize,
        div_at: Option<usize>,
    ) -> bool {
        if div_at.is_some_and(|at| self.div_closers.get(&at).is_some_and(|&close| close <= end)) {
            return false;
        }
        let boundary = item_quote as isize * self.stride + item_column as isize;
        let mut left = self.offsets.partition_point(|&offset| offset <= start) + self.size;
        let mut right = self.offsets.partition_point(|&offset| offset <= end) + self.size;
        while left < right {
            if left % 2 != 0 {
                if self.tree[left] <= depth || item_column > 0 && self.item_tree[left] < boundary {
                    return false;
                }
                left += 1;
            }
            if right % 2 != 0 {
                right -= 1;
                if self.tree[right] <= depth || item_column > 0 && self.item_tree[right] < boundary
                {
                    return false;
                }
            }
            left /= 2;
            right /= 2;
        }
        true
    }
}

fn djot_image_empty_destinations(source: &str) -> String {
    if !source.contains("](") {
        return source.to_owned();
    }
    let mask = mask_code_and_destinations(source);
    let ranges = djot_simple_destination_ranges(source).unwrap_or_else(|| {
        let code_mask = mask_djot_forms_with_options(
            source,
            true,
            true,
            None,
            &[],
            OpaqueOptions {
                destinations: false,
                ..OpaqueOptions::default()
            },
        );
        djot_destination_ranges(source, &code_mask)
    });
    let empty: HashMap<usize, usize> = ranges
        .into_iter()
        .filter(|&(start, end)| {
            source[start + 1..end - 1]
                .trim_matches([' ', '\t', '\r', '\n'])
                .is_empty()
        })
        .collect();
    if empty.is_empty() {
        return source.to_owned();
    }
    let bytes = source.as_bytes();
    let mut brackets = Vec::new();
    let mut removed = vec![false; bytes.len()];
    for at in 0..bytes.len() {
        if mask.as_bytes()[at] != bytes[at] || is_escaped(bytes, at) {
            continue;
        }
        if bytes[at] == b'[' {
            brackets.push(at);
        } else if bytes[at] == b']' {
            let open = brackets.pop();
            if let (Some(open), Some(&end)) = (open, empty.get(&(at + 1))) {
                if bytes.get(open + 1) == Some(&b'^') {
                    continue;
                }
                removed[open] = true;
                removed[at..end].fill(true);
                if open > 0 && bytes[open - 1] == b'!' && !is_escaped(bytes, open - 1) {
                    removed[open - 1] = true;
                }
            }
        }
    }
    let mut output = String::new();
    let mut cursor = 0;
    let mut at = 0;
    while at < removed.len() {
        if removed[at] {
            output.push_str(&source[cursor..at]);
            while at < removed.len() && removed[at] {
                at += 1;
            }
            cursor = at;
        } else {
            at += 1;
        }
    }
    output.push_str(&source[cursor..]);
    output
}

fn djot_image_label_child_text(
    label: &str,
    inherited: &HashSet<String>,
    omit_quote_delimiters: bool,
    nul_shielded: bool,
    preserve_source_nuls: bool,
) -> String {
    if let Some(inner) = simple_nested_image_label(label) {
        return djot_image_label_child_text(
            inner,
            inherited,
            omit_quote_delimiters,
            nul_shielded,
            preserve_source_nuls,
        );
    }
    #[cfg(test)]
    IMAGE_LABEL_FULL_PARSES.with(|count| count.set(count.get() + 1));
    let options = crate::Options {
        smart_typography: crate::SmartTypographyMode::Source,
        ..crate::Options::default()
    };
    let empty_destinations = djot_image_empty_destinations(label);
    let normalized = normalize::image_label_attributes(&empty_destinations);
    let label = normalized.as_str();
    let (protected_label, scalars, scalar_prefix) = protect_image_alt_scalars(
        label,
        preserve_source_nuls.then_some(if nul_shielded { "\0U\0" } else { "\0" }),
    );
    let native = djot_to_carve_prepared_with_quotes(
        &strip_footnote_definition_attributes(&format!("DJOTALT {} DJOTEND", protected_label)),
        omit_quote_delimiters,
        nul_shielded,
        true,
    );
    let native = if nul_shielded {
        native.replace("\0U\0", "\0")
    } else {
        native
    };
    let native = remove_inherited_attribute_markers(&native, inherited);
    let mut document = crate::parse_with_options(&native, &options);
    use crate::include_walk::SubtreeVisitor;
    ImageAltChildText {
        preserve_breaks: label.contains('\n'),
    }
    .blocks(&mut document.children);
    let plain = crate::render_plain_text_with_options(&document, &options).unwrap_or_default();
    let plain = plain
        .trim_end_matches('\n')
        .strip_prefix("DJOTALT")
        .unwrap_or("");
    let plain = plain.strip_prefix(' ').unwrap_or(plain);
    let plain = plain.strip_suffix("DJOTEND").unwrap_or("");
    let plain = plain.strip_suffix(' ').unwrap_or(plain);
    let plain = if scalars.is_empty() {
        std::borrow::Cow::Borrowed(plain)
    } else {
        cached_regex!(r"DJOTALTSCALAR([0-9]+)Z([0-9]+)Z")
            .unwrap()
            .replace_all(plain, |caps: &regex::Captures<'_>| {
                if caps[0].starts_with(&scalar_prefix) {
                    caps[2]
                        .parse::<usize>()
                        .ok()
                        .and_then(|at| scalars.get(at))
                        .cloned()
                        .unwrap_or_else(|| caps[0].to_string())
                } else {
                    caps[0].to_string()
                }
            })
    };
    plain.into_owned()
}

// A chain of images whose entire label is another image has the innermost
// label as its plain text. Reading each enclosing label independently would
// reparse the same suffix at every depth.
fn simple_nested_image_label(label: &str) -> Option<&str> {
    let bytes = label.as_bytes();
    let mut start = 0;
    while bytes.get(start..start + 2) == Some(b"![") {
        start += 2;
    }
    if start == 0 {
        return None;
    }
    let mut end = bytes.len();
    let mut count = 0;
    while end > start && bytes[end - 1] == b')' {
        let open = label[..end].rfind("](")?;
        if !bytes[open + 2..end - 1]
            .iter()
            .all(|b| b.is_ascii_alphanumeric() || b"/._-".contains(b))
        {
            return None;
        }
        end = open;
        count += 1;
    }
    if count != start / 2 || start >= end {
        return None;
    }
    let mut at = start;
    while at < end {
        match bytes[at] {
            b'\\' if at + 1 < end && !matches!(bytes[at + 1], b'\n' | b'\r') => at += 2,
            b'[' | b']' | b'\\' => return None,
            _ => at += 1,
        }
    }
    Some(&label[start..end])
}

#[cfg(test)]
thread_local! {
    #[allow(clippy::missing_const_for_thread_local)]
    static IMAGE_LABEL_FULL_PARSES: std::cell::Cell<usize> = std::cell::Cell::new(0);
}

fn image_quote_layout(source: &str, omit_quote_delimiters: bool) -> String {
    let has_forced_quotes = source.contains("{\"")
        || source.contains("{'")
        || source.contains("\"}")
        || source.contains("'}");
    if !omit_quote_delimiters && (!source.contains("![") || !has_forced_quotes) {
        return normalize::inline(source);
    }
    let source = if !omit_quote_delimiters {
        replace_image_labels_with_targets(
            source,
            &mask_code_and_destinations(source),
            |image, label, _, complete| {
                if complete
                    || label.contains("![")
                    || !(label.contains("{\"")
                        || label.contains("{'")
                        || label.contains("\"}")
                        || label.contains("'}"))
                {
                    image.to_owned()
                } else {
                    format!("![{}]", normalize::quote_spellings(label))
                }
            },
            true,
        )
    } else {
        source.to_owned()
    };
    let mut positions = HashSet::new();
    if !omit_quote_delimiters {
        let mask = mask_code_and_destinations(&source);
        let (_, image_lines) = image_line_owners(&source, &mask);
        let mut line = 0;
        replace_image_labels(
            &source,
            &mask_code_and_destinations(&source),
            |image, label, at| {
                if label.starts_with('^') || cached_regex!(r"\n[ \t]*\n").unwrap().is_match(label) {
                    return image.to_owned();
                }
                while image_lines[line].0 <= at {
                    line += 1;
                }
                let (_, quotes, item_column, item_quote, div, _) = image_lines[line];
                if !image_alt_layout(label, quotes, item_column, item_quote, div).2 {
                    return image.to_owned();
                }
                for (i, byte) in label.bytes().enumerate() {
                    if matches!(byte, b'"' | b'\'') {
                        positions.insert(at + 2 + i);
                        positions.insert(at + 1 + i);
                    }
                }
                image.to_owned()
            },
        );
    }
    normalize::inline_with_quote_preservation(&source, |at| {
        omit_quote_delimiters || positions.contains(&at)
    })
}

fn replace_image_labels(
    source: &str,
    mask: &str,
    mut replace: impl FnMut(&str, &str, usize) -> String,
) -> String {
    replace_image_labels_with_targets(
        source,
        mask,
        |image, label, at, _| replace(image, label, at),
        false,
    )
}

fn replace_image_labels_with_targets(
    source: &str,
    mask: &str,
    mut replace: impl FnMut(&str, &str, usize, bool) -> String,
    allow_incomplete: bool,
) -> String {
    if !source.contains("![") {
        return source.to_owned();
    }
    let bytes = source.as_bytes();
    let visible = mask.as_bytes();
    let mut opens: Vec<usize> = Vec::new();
    let mut closes = HashMap::new();
    let mut boundaries: HashSet<_> = djot_inline_boundaries(source, mask).into_iter().collect();
    for blank in cached_regex!(r"\n[ \t]*\n").unwrap().find_iter(source) {
        boundaries.insert(blank.start());
    }
    let mut reference_ends = vec![None; bytes.len()];
    let mut reference_end = None;
    for at in (0..bytes.len()).rev() {
        if boundaries.contains(&at) {
            reference_end = None;
        }
        if bytes[at] == b']' && !is_escaped(bytes, at) {
            reference_end = Some(at);
        }
        reference_ends[at] = reference_end;
    }
    for at in 0..bytes.len() {
        if boundaries.contains(&at) {
            opens.clear();
        }
        if visible[at] == b'[' && !is_escaped(bytes, at) {
            opens.push(at);
        } else if visible[at] == b']' && !is_escaped(bytes, at) {
            if let Some(open) = opens.pop() {
                let complete_target = match bytes.get(at + 1) {
                    Some(b'(') => visible.get(at + 1) == Some(&b' '),
                    Some(b'[') => reference_ends
                        .get(at + 2)
                        .copied()
                        .flatten()
                        .is_some_and(|end| end >= at + 2),
                    _ => false,
                };
                if open > 0
                    && bytes[open - 1] == b'!'
                    && visible[open - 1] == b'!'
                    && !is_escaped(bytes, open - 1)
                    && (complete_target
                        || allow_incomplete && matches!(bytes.get(at + 1), Some(b'(' | b'[')))
                {
                    closes.insert(open - 1, (at, complete_target));
                }
            }
        }
    }
    if closes.is_empty() {
        return source.to_owned();
    }
    let mut output = String::with_capacity(source.len());
    let mut cursor = 0;
    let mut at = 0;
    while at < bytes.len() {
        if let Some(&(end, complete_target)) = closes.get(&at) {
            let image = &source[at..end + 1];
            let replaced = replace(image, &source[at + 2..end], at, complete_target);
            if replaced == image {
                at += 1;
                continue;
            }
            output.push_str(&source[cursor..at]);
            output.push_str(&replaced);
            cursor = end + 1;
            at = end;
        }
        at += 1;
    }
    output.push_str(&source[cursor..]);
    output
}

type ImageLineOwner = (
    usize,
    usize,
    usize,
    usize,
    Option<(usize, usize, usize)>,
    bool,
);

fn image_line_owners(source: &str, mask: &str) -> (HashSet<usize>, Vec<ImageLineOwner>) {
    let mut image_starts = HashSet::new();
    let mut image_lines = Vec::new();
    let mut image_offset = 0;
    let mut image_item_column = 0;
    let mut image_item_quote = 0;
    let mut image_blank = false;
    let mut image_divs: Vec<(usize, usize, usize)> = Vec::new();
    let image_owner_marker = cached_regex!(r"^(?:[-*+]|(?:[0-9]+|[A-Za-z]|[ivxlcdm]+|[IVXLCDM]+)[.)]|\((?:[0-9]+|[A-Za-z]|[ivxlcdm]+|[IVXLCDM]+)\)|:)[ \t]+").unwrap();
    let owner_quote = cached_regex!(r"^[ \t]*>(?:[ \t]|$)").unwrap();
    for line in source.split('\n') {
        let content_start = djot_content_start(line);
        image_starts.insert(image_offset + content_start);
        let (depth, body) = quoted(line);
        let text = body.trim_start_matches([' ', '\t']);
        let indent = body.len() - text.len();
        let mut item_body = line;
        for _ in 0..image_item_quote {
            let Some(prefix) = owner_quote.find(item_body) else {
                break;
            };
            item_body = &item_body[prefix.end()..];
        }
        let owner_indent = item_body.len() - item_body.trim_start_matches([' ', '\t']).len();
        if image_owner_marker.is_match(text) {
            image_item_column = indent + 1;
            image_item_quote = depth;
        } else if !text.is_empty()
            && (depth < image_item_quote
                || image_blank && owner_indent < image_item_column
                || depth == image_item_quote
                    && indent < image_item_column
                    && cached_regex!(r"^(?:#{1,6} |:{3,}|[-*+] |\{|\|)")
                        .unwrap()
                        .is_match(text))
        {
            image_item_column = 0;
        }
        image_blank = text.is_empty();
        if let Some(delimiter) = cached_regex!(r"^(:{3,})(?:[ \t]+\S.*)?[ \t]*$")
            .unwrap()
            .captures(&line[content_start..])
        {
            if mask.as_bytes()[image_offset + content_start] != b' ' {
                if image_divs.last().is_some_and(|&(width, div_depth, _)| {
                    line[..content_start]
                        .bytes()
                        .filter(|&ch| ch == b'>')
                        .count()
                        == div_depth
                        && cached_regex!(r"^:{3,}[ \t]*$")
                            .unwrap()
                            .is_match(&line[content_start..])
                        && delimiter[1].len() >= width
                }) {
                    image_divs.pop();
                } else {
                    image_divs.push((
                        delimiter[1].len(),
                        line[..content_start]
                            .bytes()
                            .filter(|&ch| ch == b'>')
                            .count(),
                        content_start,
                    ));
                }
            }
        }
        image_lines.push((
            image_offset + line.len() + 1,
            line[..content_start].bytes().filter(|&b| b == b'>').count(),
            image_item_column,
            image_item_quote,
            image_divs.last().copied(),
            line.as_bytes().get(content_start) == Some(&b'|'),
        ));
        image_offset += line.len() + 1;
    }
    (image_starts, image_lines)
}

fn image_alt_layout(
    label: &str,
    quotes: usize,
    item_column: usize,
    item_quote: usize,
    div: Option<(usize, usize, usize)>,
) -> (String, Vec<String>, bool) {
    let mut normalized = String::new();
    let mut continuations = Vec::new();
    let mut valid = true;
    for (index, line) in label.split('\n').enumerate() {
        if index == 0 {
            normalized.push_str(line);
            continue;
        }
        let (depth, body) = quoted(line);
        let mut owner_body = line;
        for _ in 0..item_quote {
            let trimmed = owner_body.trim_start_matches([' ', '\t']);
            if !trimmed.starts_with('>')
                || trimmed.len() > 1 && !matches!(trimmed.as_bytes()[1], b' ' | b'\t')
            {
                break;
            }
            owner_body = &trimmed[if trimmed.len() > 1 { 2 } else { 1 }..];
        }
        let owner_indent = owner_body.len() - owner_body.trim_start_matches([' ', '\t']).len();
        let text = body.trim_start_matches([' ', '\t']);
        let attributes = text.starts_with('{')
            && read_djot_word_attributes(text, 0)
                .is_some_and(|(end, _)| text[end..].trim_matches([' ', '\t']).is_empty());
        let extra_block = cached_regex!(r"^(?:[-*][ \t]*){3,}$")
            .unwrap()
            .is_match(text)
            || attributes;
        if (extra_block
            || cached_regex!(r"^[ \t]*>(?:[ \t]|$)")
                .unwrap()
                .is_match(owner_body))
            && (depth < quotes || item_column > 0 && owner_indent < item_column)
        {
            valid = false;
        }
        if let Some((width, div_depth, column)) = div {
            if depth == div_depth
                && cached_regex!(r"^[ \t]*:{3,}[ \t]*$")
                    .unwrap()
                    .is_match(body)
                && body.trim().len() >= width
                && body.len() - text.len() <= column + 3
            {
                valid = false;
            }
        }
        if (depth < quotes || item_column > 0 && owner_indent < item_column)
            && cached_regex!(
                r"^(?:[-*+](?:[ \t]|$)|(?:[0-9]+|[A-Za-z]|[ivxlcdm]+|[IVXLCDM]+)[.)] |\((?:[0-9]+|[A-Za-z]|[ivxlcdm]+|[IVXLCDM]+)\) |: |#{1,6} |`{3,}|~{3,}|:{3,}(?:[ \t]*[A-Za-z0-9_-]+)?[ \t]*$|\||\^ |\[[^\]]+\]:)"
            )
            .unwrap()
            .is_match(body.trim_start_matches([' ', '\t']))
        {
            valid = false;
        }
        let bytes = line.as_bytes();
        let mut start = 0;
        for _ in 0..quotes {
            while matches!(bytes.get(start), Some(b' ' | b'\t')) {
                start += 1;
            }
            if bytes.get(start) != Some(&b'>')
                || start + 1 < bytes.len() && !matches!(bytes[start + 1], b' ' | b'\t')
            {
                break;
            }
            start += 1;
        }
        while matches!(bytes.get(start), Some(b' ' | b'\t')) {
            start += 1;
        }
        continuations.push(line[..start].to_string());
        normalized.push('\n');
        normalized.push_str(&line[start..]);
    }
    valid &= !cached_regex!(r"\n[ \t]*\n").unwrap().is_match(&normalized);
    (normalized, continuations, valid)
}

fn protect_image_alt_scalars(label: &str, nul_key: Option<&str>) -> (String, Vec<String>, String) {
    let names = cached_regex!(r"DJOTALTSCALAR([0-9]+)Z").unwrap();
    let projected: String = label.chars().filter(char::is_ascii_alphanumeric).collect();
    let reserved: HashSet<String> = names
        .captures_iter(&projected)
        .map(|caps| caps[1].to_string())
        .collect();
    let mut serial = 0;
    while reserved.contains(&serial.to_string()) {
        serial += 1;
    }
    let prefix = format!("DJOTALTSCALAR{serial}Z");
    let code_mask = mask_djot_opaque(
        label,
        OpaqueOptions {
            code: false,
            raw_code: false,
            unclosed_code: false,
            ..OpaqueOptions::default()
        },
    );
    let mut runs = Vec::new();
    let mut at = 0;
    while at < code_mask.len() {
        if code_mask[at] != b'`' {
            at += 1;
            continue;
        }
        let start = at;
        while at < code_mask.len() && code_mask[at] == b'`' {
            at += 1;
        }
        runs.push((start, at - start));
    }
    let mut next = HashMap::new();
    let mut ends = HashMap::new();
    for &(at, width) in runs.iter().rev() {
        for offset in 0..width {
            if let Some(&end) = next.get(&(width - offset)) {
                ends.insert(at + offset, end);
            }
        }
        next.insert(width, at + width);
    }
    let raw_formats: HashMap<usize, usize> = cached_regex!(r"\{=[^\x09-\x0d\x20\x{00a0}\x{1680}\x{2000}-\x{200a}\x{2028}\x{2029}\x{202f}\x{205f}\x{3000}\x{feff}{}`]+\}")
        .unwrap()
        .find_iter(label)
        .map(|raw| (raw.start(), raw.end()))
        .collect();
    let mut values = Vec::new();
    let mut code_protected = String::new();
    let mut copied = 0;
    for (mut at, mut width) in runs {
        if is_escaped(label.as_bytes(), at) {
            at += 1;
            width -= 1;
        }
        if width == 0 || at < copied {
            continue;
        }
        let Some(&end) = ends.get(&at) else {
            continue;
        };
        let mut value = &label[at + width..end - width];
        let raw = raw_formats.get(&end).copied();
        if raw.is_some() {
            if value.starts_with(" `") {
                value = &value[1..];
            }
            if value.ends_with("` ") {
                value = &value[..value.len() - 1];
            }
        } else if value.starts_with(' ')
            && value.ends_with(' ')
            && !value.trim_matches([' ', '\t', '\n', '\r']).is_empty()
        {
            value = &value[1..value.len() - 1];
        }
        code_protected.push_str(&label[copied..at]);
        code_protected.push_str(&format!("{prefix}{}Z", values.len()));
        values.push(value.to_string());
        copied = raw.unwrap_or(end);
    }
    code_protected.push_str(&label[copied..]);
    let label = code_protected.as_str();
    let mask = mask_code_and_destinations(label);
    let marker = cached_regex!(
        r"^(#{1,6}|>|[-*+]|:+|\^|(?:[0-9]+|[ivxlcdm]+|[IVXLCDM]+|[a-z]|[A-Z])[.)])(?:[ \t]|$)"
    )
    .unwrap();
    let thematic = cached_regex!(r"^(?:[-*][ \t]*){3,}$").unwrap();
    let mut output = String::new();
    let mut offset = 0;
    for (index, line) in label.split('\n').enumerate() {
        if index > 0 {
            output.push('\n');
        }
        let unicode_blank = !line.is_empty()
            && line
                .chars()
                .all(|ch| ch.is_whitespace() || ch == '\u{feff}')
            && line.chars().any(|ch| !matches!(ch, ' ' | '\t'));
        if unicode_blank {
            output.push_str(&format!("{prefix}{}Z", values.len()));
            values.push(line.to_string());
        } else if index > 0 {
            let text = line.trim_start_matches([' ', '\t']);
            let indent = line.len() - text.len();
            let width = if thematic.is_match(text) {
                text.len()
            } else if text.starts_with('|') {
                1
            } else if text.starts_with("%%") {
                2
            } else {
                marker.captures(text).map_or(0, |caps| caps[1].len())
            };
            if width > 0 && mask.as_bytes()[offset + indent] == text.as_bytes()[0] {
                output.push_str(&line[..indent]);
                output.push_str(&format!("{prefix}{}Z", values.len()));
                values.push(text[..width].to_string());
                output.push_str(&text[width..]);
            } else {
                output.push_str(line);
            }
        } else {
            output.push_str(line);
        }
        offset += line.len() + 1;
    }
    // Reference keys retain NUL identity even though image text renders it as U+FFFD.
    if let Some(value) = nul_key {
        if output.contains(value) {
            let token = format!("{prefix}{}Z", values.len());
            values.push(value.to_string());
            output = output.replace(value, &token);
        }
    }
    (output, values, prefix)
}

struct ImageAltChildText {
    preserve_breaks: bool,
}

impl crate::include_walk::SubtreeVisitor for ImageAltChildText {
    fn attributes(&mut self, attrs: &mut Option<crate::ast::Attrs>, _inline: bool) {
        *attrs = None;
    }

    fn image(&mut self, image: &mut crate::ast::Image) {
        if image.src.is_empty() {
            image.ref_label = None;
            image.raw_ref = None;
        }
    }

    fn blocks(&mut self, blocks: &mut Vec<crate::ast::BlockNode>) {
        for block in blocks {
            crate::include_walk::visit_block_children(block, self);
        }
    }

    fn inlines(&mut self, inlines: &mut Vec<crate::ast::InlineNode>) {
        for inline in inlines {
            if matches!(
                inline,
                crate::ast::InlineNode::NonBreakingSpace(_) | crate::ast::InlineNode::Footnote(_)
            ) {
                *inline = crate::ast::InlineNode::Text("".into());
                continue;
            }
            if let crate::ast::InlineNode::Link(link) = inline {
                if link.href.is_empty() {
                    link.ref_label = None;
                    link.raw_ref = None;
                }
            }
            if self.preserve_breaks {
                if let crate::ast::InlineNode::SoftBreak(value) = inline {
                    *inline = crate::ast::InlineNode::HardBreak(value.clone());
                    continue;
                }
            }
            crate::include_walk::visit_inline_children(inline, self);
        }
    }
}

fn djot_attribute_context(source: &str, start: usize) -> (usize, Option<usize>, usize) {
    let mut prefix = &source[source[..start].rfind('\n').map_or(0, |p| p + 1)..start];
    if prefix.is_empty() {
        return (0, Some(0), 0);
    }
    let enclosing =
        cached_regex!(r"^[ \t]*(?:[-*+]|(?:[0-9]+|[A-Za-z]|[ivxlcdm]+|[IVXLCDM]+)[.)]|\((?:[0-9]+|[A-Za-z]|[ivxlcdm]+|[IVXLCDM]+)\))[ \t]+").unwrap();
    if let Some(item) = enclosing.find(prefix) {
        if prefix[item.end()..].starts_with('>') {
            prefix = &prefix[item.end()..];
        }
    }
    let quote = cached_regex!(r"^[ \t]*>(?:[ \t]|$)").unwrap();
    let mut depth = 0;
    while let Some(m) = quote.find(prefix) {
        prefix = &prefix[m.len()..];
        depth += 1;
    }
    let marker =
        cached_regex!(r"^[ \t]*(?:[-*+]|(?:[0-9]+|[A-Za-z]|[ivxlcdm]+|[IVXLCDM]+)[.)]|\((?:[0-9]+|[A-Za-z]|[ivxlcdm]+|[IVXLCDM]+)\))[ \t]+").unwrap();
    let block =
        cached_regex!(r"^(?:[ \t]*(?:[-*+]|(?:[0-9]+|[A-Za-z]|[ivxlcdm]+|[IVXLCDM]+)[.)]|\((?:[0-9]+|[A-Za-z]|[ivxlcdm]+|[IVXLCDM]+)\))[ \t]+)?[ \t]*$")
            .unwrap();
    (
        depth,
        block.is_match(prefix).then_some(prefix.len()),
        marker.find(prefix).map_or(0, |m| m.len()),
    )
}
fn djot_attribute_line(mut line: &str, depth: usize) -> Option<&str> {
    let quote = cached_regex!(r"^[ \t]*>(?:[ \t]|$)").unwrap();
    for _ in 0..depth {
        line = &line[quote.find(line)?.len()..];
    }
    Some(line)
}

fn mask_djot_attribute_source(source: &str) -> String {
    let _paragraph_cache = ParagraphCacheScope::new(source);
    let code_mask = mask_djot_inline(source, false);
    let mut mask = mask_code_and_destinations(source).into_bytes();
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

    let autolink = cached_regex!(r"<[^<>\s]+>").unwrap();
    for value in autolink.find_iter(source) {
        if value.as_str().contains(':') || value.as_str().contains('@') {
            blank_out(&mut mask, value.start(), value.end());
        }
    }
    String::from_utf8(mask).expect("attribute masks preserve UTF-8")
}

fn remove_inherited_attribute_markers(source: &str, inherited: &HashSet<String>) -> String {
    let _paragraph_cache = ParagraphCacheScope::new(source);
    cached_regex!(r"\x00DJOTINVALIDATTR\x00[0-9]+\x00")
        .unwrap()
        .replace_all(source, |value: &regex::Captures<'_>| {
            if inherited.contains(&value[0]) {
                String::new()
            } else {
                value[0].to_owned()
            }
        })
        .into_owned()
}

fn protect_rejected_comment_quotes(source: &str) -> String {
    let _paragraph_cache = ParagraphCacheScope::new(source);
    let mask = mask_djot_attribute_source(source);
    let quote_pattern = cached_regex!(r"^(?:[ \t]*>(?:[ \t]|$))+").unwrap();
    let enclosing =
        cached_regex!(r"^[ \t]*(?:[-*+]|(?:[0-9]+|[A-Za-z]|[ivxlcdm]+|[IVXLCDM]+)[.)]|\((?:[0-9]+|[A-Za-z]|[ivxlcdm]+|[IVXLCDM]+)\))[ \t]+").unwrap();
    let mut edits = std::collections::BTreeMap::new();
    for (at, _) in source.match_indices("{%") {
        if mask.as_bytes()[at] != b'{' || read_djot_word_attributes(source, at).is_some() {
            continue;
        }
        let context = djot_attribute_context(source, at);
        let Some(indent) = context.1 else {
            continue;
        };
        if context.0 == 0 || comment_continues_paragraph(source, at) {
            continue;
        }
        let Some(first_end) = source[at..].find('\n').map(|n| at + n) else {
            continue;
        };
        if read_djot_word_attributes(&format!("{}}}", &source[at..first_end]), 0).is_none() {
            continue;
        }
        let from = source[..at].rfind('\n').map_or(0, |n| n + 1);
        let opening_prefix = &source[from..at];
        let enclosing_indent = opening_prefix.len() - opening_prefix.trim_start().len();
        let enclosing_end = enclosing
            .find(opening_prefix)
            .filter(|m| opening_prefix[m.end()..].starts_with('>'))
            .map_or(0, |m| m.end());
        let opening_prefix = &opening_prefix[enclosing_end..];
        let opening_quote = quote_pattern.find(opening_prefix).map(|m| m.as_str());
        let mut line = Some(first_end);
        while let Some(pos) = line {
            let Some(quote) = quote_pattern.find(&source[pos + 1..]) else {
                break;
            };
            let end = source[pos + 1..]
                .find('\n')
                .map_or(source.len(), |n| pos + 1 + n);
            let body_at = pos + 1 + quote.end();
            let body = &source[body_at..end];
            if quote.as_str().len() - quote.as_str().trim_start().len() < enclosing_end {
                if quote.as_str().len() - quote.as_str().trim_start().len() > enclosing_indent
                    && Some(quote.as_str().trim_start()) == opening_quote.map(str::trim_start)
                    && quote.as_str().bytes().filter(|b| *b == b'>').count() == context.0
                {
                    edits.insert(
                        pos + 1 + quote.end(),
                        format!("{} ", quote.as_str().trim().replace('>', "\\>")),
                    );
                }
                break;
            }
            if quote.as_str().bytes().filter(|&b| b == b'>').count() < context.0 {
                edits.insert(pos + 1, format!("{}\n", quote.as_str().trim_end()));
                break;
            }
            if Some(quote.as_str().trim_start()) != opening_quote.map(str::trim_start)
                || quote.as_str().bytes().filter(|b| *b == b'>').count() != context.0
            {
                break;
            }
            if body.trim().is_empty() {
                break;
            }
            let held = body
                .bytes()
                .take_while(|b| matches!(b, b' ' | b'\t'))
                .count();
            if held < context.2 {
                if quote.as_str().len() - quote.as_str().trim_start().len()
                    >= opening_quote.map_or(0, |quote| quote.len() - quote.trim_start().len())
                        + context.2
                {
                    edits.insert(
                        body_at,
                        format!("{} ", quote.as_str().trim().replace('>', "\\>")),
                    );
                }
                break;
            }
            if held <= indent {
                edits.insert(
                    body_at,
                    format!("{} ", quote.as_str().trim().replace('>', "\\>")),
                );
                break;
            }
            if body.contains(['}', '%']) {
                break;
            }
            line = (end < source.len()).then_some(end);
        }
    }
    let mut output = String::new();
    let mut cursor = 0;
    for (at, text) in edits {
        output.push_str(&source[cursor..at]);
        output.push_str(&text);
        cursor = at;
    }
    output.push_str(&source[cursor..]);
    output
}

fn escape_djot_table_attribute_boundaries(source: &str) -> std::borrow::Cow<'_, str> {
    if !source.contains('|') || !source.contains('{') && !source.contains('<') {
        return std::borrow::Cow::Borrowed(source);
    }
    let _paragraph_cache = ParagraphCacheScope::new(source);
    let mask = mask_djot_forms_with_options(
        source,
        false,
        true,
        None,
        &[],
        OpaqueOptions {
            autolinks: false,
            destinations: false,
            attribute_values: false,
            ..OpaqueOptions::default()
        },
    );
    let rows = djot_table_rows(source, &mask);
    let bytes = source.as_bytes();
    let code_mask = if source.contains('`') {
        let mut offset = 0;
        source
            .split('\n')
            .enumerate()
            .map(|(index, physical)| {
                let masked = if rows.get(index) == Some(&true) && physical.contains('`') {
                    mask_djot_forms_with_options(
                        &physical.replace('{', " "),
                        false,
                        true,
                        None,
                        &[],
                        OpaqueOptions {
                            autolinks: false,
                            destinations: false,
                            attribute_values: false,
                            unclosed_code: false,
                            ..OpaqueOptions::default()
                        },
                    )
                } else {
                    mask[offset..offset + physical.len()].to_owned()
                };
                offset += physical.len() + 1;
                masked
            })
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        mask.clone()
    };
    let mut rejected_rows = HashMap::new();
    let mut row_offsets = Vec::new();
    let mut edits = HashMap::<usize, (usize, String, usize)>::new();
    let mut code_ends = HashMap::new();
    let mut line = 0;
    let mut line_start = 0;
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] == b'\n' {
            line += 1;
            line_start = at + 1;
            at += 1;
            continue;
        }
        if let Some(&end) = code_ends.get(&at) {
            at = end;
            continue;
        }
        if rows.get(line) == Some(&true) && bytes[at] == b'<' && !is_escaped(bytes, at) {
            if let Some(angle) = cached_regex!(r"\A<[^<>\s]+>").unwrap().find(&source[at..]) {
                if (at + 1..at + angle.end())
                    .any(|inner| bytes[inner] == b'|' && bytes[inner - 1] != b'\\')
                {
                    edits.insert(at, (at, "\\".to_owned(), line));
                }
            }
        }
        if rows.get(line) != Some(&true)
            || bytes[at] != b'{'
            || mask.as_bytes()[at] != b'{'
            || is_escaped(bytes, at)
        {
            at += 1;
            continue;
        }
        let Some((end, _)) = read_djot_word_attributes(source, at) else {
            at += 1;
            continue;
        };
        let mut first_pipe = None;
        let mut crosses_cell = false;
        for inner in at + 1..end {
            if bytes[inner] == b'|' && bytes.get(inner.wrapping_sub(1)) != Some(&b'\\') {
                first_pipe.get_or_insert(inner);
                if mask.as_bytes()[inner] == b'|'
                    && (bytes[at + 1] == b'%' || code_mask.as_bytes()[inner] == b'|')
                {
                    crosses_cell = true;
                }
            }
        }
        if first_pipe
            .is_some_and(|inner| bytes[at + 1] != b'%' && code_mask.as_bytes()[inner] != b'|')
        {
            let physical_end = source[line_start..]
                .find('\n')
                .map_or(source.len(), |offset| line_start + offset);
            if let std::collections::hash_map::Entry::Vacant(entry) = rejected_rows.entry(line) {
                let offset = line_start + djot_content_start(&source[line_start..physical_end]);
                entry.insert(offset);
                row_offsets.push(offset);
            }
        }
        if crosses_cell {
            for inner in at + 1..first_pipe.unwrap_or(at + 1) {
                if bytes[inner] == b'"'
                    && bytes.get(inner + 1) == Some(&b'}')
                    && !is_escaped(bytes, inner)
                {
                    edits.insert(inner, (inner + 2, "“\\}".to_owned(), line));
                }
            }
        }
        if let Some(first_pipe) =
            first_pipe.filter(|_| crosses_cell && !rejected_rows.contains_key(&line))
        {
            let mut open = None;
            let mut width = 0;
            let mut inner = at + 1;
            while inner < first_pipe {
                if bytes[inner] == b'\\' {
                    inner += 2;
                    continue;
                }
                if bytes[inner] != b'`' {
                    inner += 1;
                    continue;
                }
                let mut run = 1;
                while bytes.get(inner + run) == Some(&b'`') {
                    run += 1;
                }
                if open.is_none() {
                    open = Some(inner);
                    width = run;
                } else if width == run {
                    open = None;
                }
                inner += run;
            }
            if let Some(open) = open {
                if open + width == first_pipe {
                    edits.insert(
                        open,
                        (first_pipe, "`<code></code>`{=html}".to_owned(), line),
                    );
                } else {
                    edits.insert(first_pipe, (first_pipe, "`".repeat(width), line));
                }
                code_ends.insert(open, first_pipe);
            }
        }
        if crosses_cell {
            edits.insert(
                at,
                (
                    at + 1,
                    if at > 0 && matches!(bytes[at - 1], b']' | b')') {
                        "{}\\{"
                    } else {
                        "\\{"
                    }
                    .to_owned(),
                    line,
                ),
            );
            at += 1;
            continue;
        }
        if let Some(offset) = bytes[at..end].iter().rposition(|&byte| byte == b'\n') {
            line_start = at + offset + 1;
        }
        line += bytes[at..end].iter().filter(|&&byte| byte == b'\n').count();
        at = end;
    }
    let owned_rows = if !rejected_rows.is_empty() {
        let mut paragraph = bytes.to_vec();
        for &offset in &row_offsets {
            paragraph[offset] = b'x';
        }
        let paragraph = String::from_utf8(paragraph).unwrap();
        let mut analysis = Vec::new();
        let mut positions = Vec::new();
        let mut owner: Option<(usize, usize, Option<usize>)> = None;
        for (index, physical) in paragraph.split('\n').enumerate() {
            let prefix = &physical[..djot_content_start(physical)];
            let structural = cached_regex!(r"\[\^[^\]\n]+\]:[ \t]*")
                .unwrap()
                .replace_all(prefix, "");
            let depth = structural.bytes().filter(|&byte| byte == b'>').count();
            let leading = cached_regex!(r"\A(?:[ \t]*>(?:[ \t]|$))*[ \t]*")
                .unwrap()
                .find(physical)
                .unwrap()
                .as_str();
            let quotes: Vec<_> = leading.match_indices('>').map(|(at, _)| at).collect();
            let column = owner
                .and_then(|(_, item_depth, _)| quotes.get(item_depth).copied())
                .unwrap_or(leading.len());
            if rows.get(index) == Some(&true)
                && owner.is_some_and(|(owner_depth, _, owner_column)| {
                    depth < owner_depth
                        || owner_column.is_some_and(|owner_column| column <= owner_column)
                })
            {
                analysis.push("");
                owner = None;
            }
            positions.push(analysis.len());
            analysis.push(physical);
            if rejected_rows.contains_key(&index) {
                let mut at = 0;
                let mut column = 0;
                let mut quote_depth = 0;
                let mut item_depth = 0;
                let mut item_column = None;
                while at < prefix.len() {
                    if matches!(prefix.as_bytes()[at], b' ' | b'\t') {
                        at += 1;
                        column += 1;
                        continue;
                    }
                    if prefix.as_bytes()[at] == b'>' {
                        at += 1;
                        column += 1;
                        quote_depth += 1;
                        continue;
                    }
                    let Some(marker) = cached_regex!(
                        r"\A(?:[-*+]|[0-9A-Za-z]+[.)]|\([0-9A-Za-z]+\)|\[\^[^\]\n]+\]:)[ \t]+"
                    )
                    .unwrap()
                    .find(&prefix[at..]) else {
                        break;
                    };
                    item_column = Some(column + usize::from(marker.as_str().starts_with("[^")));
                    item_depth = quote_depth;
                    at += marker.end();
                    column += marker.as_str().encode_utf16().count();
                }
                owner = Some((depth, item_depth, item_column));
            }
        }
        let analysis_source = analysis.join("\n");
        let paragraph_mask = mask_djot_forms_with_options(
            &analysis_source,
            false,
            true,
            None,
            &[],
            OpaqueOptions {
                autolinks: false,
                destinations: false,
                attribute_values: false,
                ..OpaqueOptions::default()
            },
        );
        let analysis_rows = djot_table_rows(&analysis_source, &paragraph_mask);
        positions
            .into_iter()
            .map(|position| analysis_rows.get(position).copied().unwrap_or(false))
            .collect()
    } else {
        rows
    };
    let row_starts: HashSet<_> = row_offsets.into_iter().collect();
    let mut output = String::new();
    let mut copied = 0;
    let mut at = 0;
    while at < bytes.len() {
        if row_starts.contains(&at) {
            output.push_str(&source[copied..at]);
            output.push('\\');
            copied = at;
        }
        if let Some((end, text, line)) = edits.get(&at).filter(|(_, _, line)| {
            !rejected_rows.contains_key(line) && owned_rows.get(*line) == Some(&true)
        }) {
            let _ = line;
            output.push_str(&source[copied..at]);
            output.push_str(text);
            copied = *end;
            at = (*end).max(at + 1);
        } else {
            at += 1;
        }
    }
    if output.is_empty() {
        std::borrow::Cow::Borrowed(source)
    } else {
        output.push_str(&source[copied..]);
        std::borrow::Cow::Owned(output)
    }
}

fn escape_invalid_djot_attributes(source: &str) -> (String, HashSet<String>) {
    let protected = (source.contains("{%") && source.contains('>'))
        .then(|| protect_rejected_comment_quotes(source));
    let source = protected.as_deref().unwrap_or(source);
    let _paragraph_cache = ParagraphCacheScope::new(source);
    let mask = mask_djot_attribute_source(source);
    let reserved: HashSet<String> = cached_regex!(r"\x00DJOTINVALIDATTR\x00[0-9]+\x00")
        .unwrap()
        .find_iter(source)
        .map(|value| value.as_str().to_owned())
        .collect();
    let mut inherited = HashSet::new();
    let mut serial = 0usize;
    let mut marker = || loop {
        let value = format!("\0DJOTINVALIDATTR\x00{serial}\0");
        serial += 1;
        if !reserved.contains(&value) {
            inherited.insert(value.clone());
            return value;
        }
    };
    let mut output = String::new();
    let mut cursor = 0;
    let mut at = 0;
    while at < source.len() {
        if mask.as_bytes()[at] == b'\\' {
            at += 2;
            continue;
        }
        if mask.as_bytes()[at] == b'{' {
            if let Some((end, _)) = read_djot_word_attributes(source, at) {
                at = end;
                continue;
            }
            if source
                .as_bytes()
                .get(at + 1)
                .is_some_and(|byte| byte.is_ascii_alphabetic() || b".#% \t".contains(byte))
            {
                output.push_str(&source[cursor..at]);
                output.push_str("\\{");
                output.push_str(&marker());
                cursor = at + 1;
                // Every brace inside the same rejected run is literal Djot text too, so a
                // later pass must not read one as a forced quote and swallow it.
                let bytes = source.as_bytes();
                let mut inner = at + 1;
                while inner < source.len() && bytes[inner] != b'\n' && bytes[inner] != b'}' {
                    if bytes[inner] == b'{'
                        && mask.as_bytes()[inner] == b'{'
                        && read_djot_word_attributes(source, inner).is_none()
                    {
                        output.push_str(&source[cursor..inner]);
                        output.push_str("\\{");
                        cursor = inner + 1;
                    }
                    inner += 1;
                }
                if source.as_bytes().get(at + 1) == Some(&b'%')
                    && comment_continues_paragraph(source, at)
                {
                    let mut inner = (at + 2).max(cursor);
                    while inner < source.len() && !matches!(source.as_bytes()[inner], b'%' | b'}') {
                        if source.as_bytes()[inner] == b'\n' {
                            let end = source[inner + 1..]
                                .find('\n')
                                .map_or(source.len(), |at| inner + 1 + at);
                            let line = &source[inner + 1..end];
                            if line[emphasis::structural_prefix_end(line)..]
                                .trim()
                                .is_empty()
                            {
                                break;
                            }
                        }
                        if source.as_bytes()[inner] == b'{' && mask.as_bytes()[inner] == b'{' {
                            output.push_str(&source[cursor..inner]);
                            output.push_str("\\{");
                            output.push_str(&marker());
                            cursor = inner + 1;
                        }
                        inner += 1;
                    }
                }
                at = cursor;
                continue;
            }
        }
        at += 1;
    }
    output.push_str(&source[cursor..]);
    (output, inherited)
}

struct DjotAttributeToken {
    key: String,
    source: String,
    append: bool,
}

fn read_djot_attribute_tokens(
    source: &str,
    start: usize,
) -> Option<(usize, String, Vec<DjotAttributeToken>)> {
    read_djot_attributes_with_tokens(source, start, false, false)
}

fn rejected_comment_starts_block(line: &str) -> bool {
    let Some(capture) = cached_regex!(r"^\\\{\x00DJOTINVALIDATTR\x00[0-9]+\x00(%[^\n]*)")
        .unwrap()
        .captures(line)
    else {
        return false;
    };
    read_djot_word_attributes(&format!("{{{}}}", &capture[1]), 0).is_some()
}

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

fn read_djot_word_attributes(source: &str, start: usize) -> Option<(usize, String)> {
    read_djot_attributes(source, start, false, false)
}

fn read_djot_attributes(
    source: &str,
    start: usize,
    carve: bool,
    table: bool,
) -> Option<(usize, String)> {
    read_djot_attributes_with_tokens(source, start, carve, table).map(|(end, wire, _)| (end, wire))
}

fn read_djot_attributes_with_tokens(
    source: &str,
    start: usize,
    carve: bool,
    table: bool,
) -> Option<(usize, String, Vec<DjotAttributeToken>)> {
    read_djot_attributes_with_context(source, start, carve, table, true)
}

fn read_djot_attributes_with_context(
    source: &str,
    start: usize,
    carve: bool,
    table: bool,
    paragraph_context: bool,
) -> Option<(usize, String, Vec<DjotAttributeToken>)> {
    read_djot_attributes_with_source_form(source, start, carve, table, paragraph_context, false)
}

fn read_djot_attributes_with_source_form(
    source: &str,
    start: usize,
    carve: bool,
    table: bool,
    paragraph_context: bool,
    preserve_shorthand: bool,
) -> Option<(usize, String, Vec<DjotAttributeToken>)> {
    let bytes = source.as_bytes();
    let quote_prefix = cached_regex!(r"^[ \t]*>(?:[ \t]|$)").unwrap();
    let mut parts = Vec::new();
    let mut tokens: Vec<DjotAttributeToken> = Vec::new();
    let mut last_class_assignment = None;
    let mut i = start + 1;
    let mut context = None;
    while i < bytes.len() {
        while i < bytes.len() && matches!(bytes[i], b' ' | b'\t' | b'\n' | b'\r') {
            if bytes[i] == b'\n' && blank_line_follows(bytes, i) {
                return None;
            }
            if bytes[i] == b'\n' {
                let (depth, _, _) =
                    *context.get_or_insert_with(|| djot_attribute_context(source, start));
                i += 1;
                for _ in 0..depth {
                    let Some(quote) = quote_prefix.find(&source[i..]) else {
                        break;
                    };
                    i += quote.end();
                }
            } else {
                i += 1;
            }
        }
        if bytes.get(i) == Some(&b'}') && source[start..=i].contains('\n') {
            let (depth, block_indent, minimum) =
                *context.get_or_insert_with(|| djot_attribute_context(source, start));
            let invalid = source[start..=i].split('\n').skip(1).any(|raw| {
                let stripped = djot_attribute_line(raw, depth);
                if block_indent.is_some() && stripped.is_none() {
                    return true;
                }
                let line = stripped.unwrap_or(raw);
                let indent = line
                    .bytes()
                    .take_while(|b| matches!(b, b' ' | b'\t'))
                    .count();
                block_indent.is_some_and(|owner| indent < minimum || indent <= owner)
            });
            if invalid
                && (!parts.is_empty()
                    || !paragraph_context
                    || !comment_continues_paragraph_with_context(
                        source,
                        start,
                        (depth, block_indent, minimum),
                    ))
            {
                return None;
            }
        }
        if bytes.get(i) == Some(&b'}') {
            if carve && table {
                let mut at = start;
                while at <= i {
                    if bytes[at] == b'\\' {
                        at += 2;
                    } else if bytes[at] == b'|' {
                        return None;
                    } else {
                        at += 1;
                    }
                }
            }
            if carve {
                if let Some(last_class_assignment) = last_class_assignment {
                    parts = parts
                        .into_iter()
                        .enumerate()
                        .filter_map(|(index, part)| {
                            (index >= last_class_assignment || tokens[index].key != "class")
                                .then_some(part)
                        })
                        .collect();
                }
            }
            return Some((i + 1, format!("{{{}}}", parts.join(" ")), tokens));
        }
        if bytes.get(i) == Some(&b'%') {
            let mut end = i + 1;
            while end < bytes.len() && !matches!(bytes[end], b'%' | b'}') {
                end += 1;
            }
            if end == bytes.len()
                || source[i..end]
                    .match_indices('\n')
                    .any(|(at, _)| blank_line_follows(bytes, i + at))
            {
                return None;
            }
            let comment = &source[i..end];
            if comment.contains('>') && comment.contains('\n') {
                let (depth, _, _) =
                    *context.get_or_insert_with(|| djot_attribute_context(source, start));
                if depth > 0
                    && comment[..comment.rfind('\n').unwrap_or(0)]
                        .split('\n')
                        .skip(1)
                        .any(|raw| {
                            djot_attribute_line(raw, depth)
                                .is_some_and(|line| line.trim().is_empty())
                        })
                {
                    return None;
                }
            }
            i = if bytes[end] == b'%' { end + 1 } else { end };
            continue;
        }
        let bare_end = |from: usize| {
            let mut end = from;
            for ch in source[from..].chars() {
                if !(ch.is_ascii_alphanumeric() || "_:-".contains(ch)) {
                    break;
                }
                end += ch.len_utf8();
            }
            end
        };
        if matches!(bytes.get(i), Some(b'#' | b'.')) {
            let kind = bytes[i] as char;
            i += 1;
            let from = i;
            for ch in source[from..].chars() {
                let valid = if kind == '#' {
                    !(ch.is_whitespace() && ch != '\u{85}')
                        && ch != '\u{feff}'
                        && !r#"][~!@#$%^&*(){}`,.<>\|=+/?"#.contains(ch)
                } else {
                    ch.is_ascii_alphanumeric() || "_:-".contains(ch)
                };
                if !valid {
                    break;
                }
                i += ch.len_utf8();
            }
            if i == from {
                return None;
            }
            let value = &source[from..i];
            let invalid = if kind == '#' {
                value.chars().any(|ch| {
                    (ch.is_whitespace() && ch != '\u{85}')
                        || ch == '\u{feff}'
                        || r#"][~!@#$%^&*(){}`,.<>\|=+/?"#.contains(ch)
                })
            } else {
                !value
                    .bytes()
                    .all(|ch| ch.is_ascii_alphanumeric() || b"_:-".contains(&ch))
            };
            if invalid {
                return None;
            }
            let identifier = value
                .bytes()
                .all(|ch| ch.is_ascii_alphanumeric() || ch == b'_' || ch == b'-')
                && value.as_bytes()[0] != b'-';
            parts.push(
                if identifier || (preserve_shorthand && !carve && kind == '.') {
                    format!("{kind}{value}")
                } else {
                    format!(
                        "{}={}",
                        if kind == '#' { "id" } else { "class" },
                        quote_djot_attribute(value, carve)
                    )
                },
            );
            tokens.push(DjotAttributeToken {
                key: if kind == '#' { "id" } else { "class" }.to_owned(),
                source: if kind == '#' {
                    format!("id={}", quote_djot_attribute(value, carve))
                } else {
                    parts.last().unwrap().clone()
                },
                append: kind == '.',
            });
        } else {
            let key_start = i;
            if !bytes.get(i).is_some_and(|ch| ch.is_ascii_alphabetic()) {
                return None;
            }
            i += 1;
            while bytes
                .get(i)
                .is_some_and(|ch| ch.is_ascii_alphanumeric() || matches!(ch, b'_' | b'-'))
            {
                i += 1;
            }
            if bytes.get(i) != Some(&b'=') {
                return None;
            }
            i += 1;
            let key = &source[key_start..i];
            let from = i;
            if bytes.get(i) == Some(&b'"') {
                i += 1;
                while i < bytes.len() && bytes[i] != b'"' {
                    if bytes[i] == b'\n' && blank_line_follows(bytes, i) {
                        return None;
                    }
                    if bytes[i] == b'\\' {
                        i += 1;
                    }
                    i += 1;
                }
                if bytes.get(i) != Some(&b'"') {
                    return None;
                }
                i += 1;
                let value = &source[from + 1..i - 1];
                let folded = if value.contains('\n') {
                    let (depth, _, _) = djot_attribute_context(source, start);
                    let mut lines = value.split('\n');
                    let mut folded = lines.next().unwrap().to_owned();
                    for raw in lines {
                        folded.push(' ');
                        folded.push_str(
                            djot_attribute_line(raw, depth)
                                .unwrap_or(raw)
                                .trim_start_matches([' ', '\t']),
                        );
                    }
                    folded
                } else {
                    value.to_owned()
                };
                parts.push(format!(
                    "{key}{}",
                    quote_djot_attribute(&decode_djot_attribute(&folded), carve)
                ));
            } else {
                i = bare_end(i);
                if i == from {
                    return None;
                }
                parts.push(format!(
                    "{key}{}",
                    quote_djot_attribute(&source[from..i], carve)
                ));
            }
            if key == "class=" {
                last_class_assignment = Some(parts.len() - 1);
            }
            tokens.push(DjotAttributeToken {
                key: key.trim_end_matches('=').to_owned(),
                source: if bytes[from] == b'"' || source[key_start..i].contains('\n') {
                    parts.last().unwrap().clone()
                } else {
                    source[key_start..i].to_owned()
                },
                append: false,
            });
        }
        if i < bytes.len() {
            let next = source[i..].chars().next()?;
            if !next.is_whitespace() && next != '}' && next != '%' {
                return None;
            }
        }
    }
    None
}

struct NativeAttributeReader<'a> {
    source: &'a str,
    line_end: usize,
    table: bool,
}

impl<'a> NativeAttributeReader<'a> {
    fn new(source: &'a str) -> Self {
        let line_end = source.find('\n').unwrap_or(source.len());
        let line = &source[..line_end];
        Self {
            source,
            line_end,
            table: line.as_bytes().get(emphasis::structural_prefix_end(line)) == Some(&b'|'),
        }
    }

    fn read(&mut self, start: usize) -> Option<(usize, String)> {
        while self.line_end < start {
            let line_start = self.line_end + 1;
            self.line_end = self.source[line_start..]
                .find('\n')
                .map_or(self.source.len(), |at| line_start + at);
            let line = &self.source[line_start..self.line_end];
            self.table = line.as_bytes().get(emphasis::structural_prefix_end(line)) == Some(&b'|');
        }
        read_djot_attributes(self.source, start, true, self.table)
    }
}

fn djot_word_whitespace(ch: char) -> bool {
    (ch.is_whitespace() && ch != '\u{85}') || ch == '\u{feff}'
}

fn protect_attributed_words(
    source: &str,
    prefix: &str,
    spans: &mut Vec<String>,
    inherited: &HashSet<String>,
) -> String {
    if !source.contains('{') {
        return source.to_owned();
    }
    let paired = emphasis::paired_openers(source, &emphasis_mask(source));
    let mut read_native = NativeAttributeReader::new(source);
    let mut masked = mask_code_and_destinations(source).into_bytes();
    for note in cached_regex!(r"\[\^[^\]\n]*\]").unwrap().find_iter(source) {
        if !is_escaped(source.as_bytes(), note.start()) {
            masked[note.range()].fill(b' ');
        }
    }
    let masked = String::from_utf8(masked).expect("footnote masks preserve UTF-8");
    let mut literal_braces = std::collections::HashMap::new();
    let paired_closes: HashSet<usize> = paired.values().copied().collect();
    let mut escaped_brace_closes = HashSet::new();
    let mut word_atoms = HashMap::new();
    for note in cached_regex!(r"\[\^[^\]\n]*\]").unwrap().find_iter(source) {
        if is_escaped(source.as_bytes(), note.start())
            && !is_escaped(source.as_bytes(), note.end() - 1)
            && !note
                .as_str()
                .chars()
                .any(|ch| ch.is_whitespace() || "{}*_~`\\".contains(ch))
        {
            literal_braces.insert(note.end() - 1, note.start() - 1);
            escaped_brace_closes.insert(note.end() - 1);
        }
    }
    let mut brace_stack: Vec<(usize, bool, Option<usize>)> = Vec::new();
    let mut read_brace_attributes = NativeAttributeReader::new(source);
    let mut attribute_end = 0;
    let mut last_space = None;
    let mut last_atom_escape = None;
    let mut last_inline_end = None;
    let mut escaped = None;
    let mut last_escaped = None;
    let inherited_marker = cached_regex!(r"\A\x00DJOTINVALIDATTR\x00[0-9]+\x00").unwrap();
    for (at, ch) in source.char_indices() {
        if djot_word_whitespace(ch) {
            last_space = Some(at);
        }
        if paired_closes.contains(&at) {
            last_inline_end = Some(at);
        }
        if let Some(begin) = escaped.take() {
            if masked.as_bytes()[begin] != source.as_bytes()[begin]
                || masked.as_bytes()[at] != source.as_bytes()[at]
            {
                last_inline_end = Some(at + ch.len_utf8());
                continue;
            }
            if !djot_word_whitespace(ch) {
                word_atoms.insert(
                    if source[at..].starts_with("\0U\0") {
                        at + 3
                    } else {
                        at + ch.len_utf8()
                    },
                    begin,
                );
            }
            last_escaped = Some(at);
            let marker_start = at + ch.len_utf8();
            let generated = inherited_marker
                .find(&source[marker_start..])
                .filter(|value| inherited.contains(value.as_str()));
            if let Some(marker) = generated {
                literal_braces.insert(marker_start + marker.len() - 1, marker_start);
            } else if ch.is_ascii_punctuation() {
                last_atom_escape = Some(begin);
            }
            if ch == '{' && masked.as_bytes()[at] == b'{' {
                brace_stack.push((begin, true, last_space));
            } else if ch == '}' || ch == ']' {
                if ch == '}' && brace_stack.last().is_some_and(|(_, literal, _)| *literal) {
                    brace_stack.pop();
                }
                literal_braces.insert(at, begin);
                escaped_brace_closes.insert(at);
            }
            continue;
        }
        if ch == '\\' {
            escaped = Some(at);
            continue;
        }
        if masked.as_bytes()[at] != source.as_bytes()[at] {
            last_inline_end = Some(at + ch.len_utf8());
            continue;
        }
        if source[at..].starts_with("\0U\0") {
            word_atoms.insert(at + 3, at);
        }
        if ch == '{' && at >= attribute_end {
            attribute_end = read_brace_attributes.read(at).map_or(at, |(end, _)| end);
        }
        if ch == '}'
            && at >= attribute_end
            && at > 0
            && b"+-=~^*_".contains(&source.as_bytes()[at - 1])
            && !paired_closes.contains(&(at + 1))
        {
            let escaped_marker = last_escaped == Some(at - 1);
            literal_braces.insert(at, if escaped_marker { at - 2 } else { at - 1 });
            if escaped_marker {
                escaped_brace_closes.insert(at);
            }
        }
        if ch == '{' {
            brace_stack.push((
                at,
                at >= attribute_end
                    && source
                        .as_bytes()
                        .get(at + 1)
                        .is_some_and(|ch| ch.is_ascii_alphabetic() || b".#% \t".contains(ch)),
                last_space,
            ));
        } else if ch == '}' {
            if let Some((begin, true, opening_space)) = brace_stack.pop() {
                if !escaped_brace_closes.contains(&at) && !paired_closes.contains(&(at + 1)) {
                    if let Some(from) = last_atom_escape.max(last_inline_end).filter(|from| {
                        *from >= begin && last_space.map_or(true, |space| *from > space)
                    }) {
                        literal_braces.insert(at, from);
                        escaped_brace_closes.insert(at);
                    } else {
                        literal_braces.insert(
                            at,
                            if last_space == opening_space {
                                begin
                            } else {
                                at
                            },
                        );
                    }
                }
            }
        }
    }
    let bytes = source.as_bytes();
    let mut output = String::new();
    let mut cursor = 0;
    let mut i = 0;
    let last_close = source.rfind('}');
    while last_close.is_some_and(|end| i <= end) {
        if bytes[i] != b'{' || masked.as_bytes()[i] != b'{' || is_escaped(bytes, i) {
            i += 1;
            continue;
        }
        let Some((mut end, mut attrs)) = read_native.read(i) else {
            i += 1;
            continue;
        };
        while bytes.get(end) == Some(&b'{') {
            let Some((next_end, next_attrs)) = read_native.read(end) else {
                break;
            };
            end = next_end;
            if attrs == "{}" {
                attrs.clear();
            }
            if next_attrs != "{}" {
                attrs.push_str(&next_attrs);
            }
        }
        if attrs.is_empty() || attrs == "{}" {
            i = end;
            continue;
        }
        let mut word = i;
        if i > 0
            && masked.as_bytes()[i - 1] == bytes[i - 1]
            && (!b"`*_~^]}>".contains(&bytes[i - 1])
                || literal_braces.contains_key(&(i - 1))
                || word_atoms.contains_key(&i))
        {
            while word > cursor {
                if let Some(&literal) = literal_braces
                    .get(&(word - 1))
                    .filter(|&&literal| literal >= cursor)
                {
                    word = literal;
                    continue;
                }
                if let Some(&begin) = word_atoms.get(&word) {
                    if begin >= cursor {
                        word = begin;
                        continue;
                    }
                }
                let ch = source[..word].chars().next_back().unwrap();
                let at = word - ch.len_utf8();
                if djot_word_whitespace(ch)
                    || "\"'{}[]`\0>|".contains(ch)
                    || masked.as_bytes()[at] != bytes[at]
                {
                    break;
                }
                word = at;
            }
        }
        let paired_word = (word..i)
            .rev()
            .find(|at| paired.get(at).is_some_and(|close| *close > end));
        if let Some(at) = paired_word {
            word = at + 1;
        }
        if word > 0
            && bytes[word - 1] == b'{'
            && bytes.get(word).is_some_and(|ch| b"+-=".contains(ch))
        {
            word += 1;
        }
        if word < i {
            output.push_str(&source[cursor..word]);
            output.push_str(&format!("{prefix}{}\0", spans.len()));
            let converted = rewrite_djot_body(&format!("x {}", &source[word..i]));
            let mut body = converted[2..].to_owned();
            if source.as_bytes()[i - 1] == b']'
                && escaped_brace_closes.contains(&(i - 1))
                && literal_braces.get(&(i - 1)) != Some(&(i - 2))
            {
                body.pop();
                body.push_str("\\]");
            }
            spans.push(format!(
                "{}[{}{body}]{attrs}",
                if word > 0 && b"]^!".contains(&source.as_bytes()[word - 1]) {
                    "{%%}"
                } else {
                    ""
                },
                if body.starts_with('^') { "\\" } else { "" }
            ));
            cursor = end;
        }
        i = end;
    }
    output.push_str(&source[cursor..]);
    output
}

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
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        if !line.contains('#') {
            result.push(line.to_owned());
            i += 1;
            continue;
        }
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
        let first_line = if depth > 0 && column == 0 {
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

#[derive(Clone, Copy)]
struct DefinitionFrame {
    source: usize,
    target: usize,
    body: bool,
    ready: bool,
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
    (byte > whitespace && byte < bytes.len()).then_some((columns, byte))
}

/// Translate Djot's definition-item shape without canonicalizing unrelated
/// links, raw blocks, attributes, or author spelling elsewhere in the file.
fn convert_definition_lists(source: &str, empty_term: &str) -> String {
    let _paragraph_cache = ParagraphCacheScope::new(source);
    let mut lines: Vec<String> = source.split('\n').map(str::to_owned).collect();
    let masked_source = mask_code_and_destinations(source);
    let masked: Vec<&str> = masked_source.split('\n').collect();
    let mut stack: Vec<DefinitionFrame> = Vec::new();
    for index in 0..lines.len() {
        let raw_term = definition_term(&lines[index]);
        let raw_fence = raw_term.is_some_and(|(_, content)| {
            let payload = &lines[index][content..];
            (payload.starts_with("~~~")
                || (payload.starts_with("```") && !payload.trim_start_matches('`').contains('`')))
                && masked[index].trim_start().starts_with(':')
        });
        let container_body = raw_term.is_some_and(|(_, content)| {
            cached_regex!(r"^:{3,}(?:[ \t]*[A-Za-z0-9_-]+)?[ \t]*$")
                .unwrap()
                .is_match(&lines[index][content..])
        });
        let starts_definition_body = raw_fence || container_body;
        if let (Some((indent, _)), Some((raw_indent, content))) = (
            if raw_fence
                || raw_term.is_some_and(|(_, content)| lines[index][content..].starts_with('`'))
            {
                raw_term
            } else {
                definition_term(masked[index])
            },
            raw_term,
        ) {
            if indent != raw_indent {
                continue;
            }
            while stack.last().is_some_and(|frame| indent < frame.source) {
                stack.pop();
            }
            let top = stack.last().copied();
            let may_start = top.is_some() || index == 0 || lines[index - 1].trim().is_empty();
            if may_start {
                let term = lines[index][content..].to_owned();
                if top.is_none() || top.is_some_and(|frame| indent == frame.source) {
                    let target = top.map_or(indent, |frame| frame.target);
                    let starts = top.is_none();
                    if let Some(frame) = stack.last_mut() {
                        frame.body = false;
                        frame.ready = false;
                    } else {
                        stack.push(DefinitionFrame {
                            source: indent,
                            target,
                            body: false,
                            ready: false,
                        });
                    }
                    lines[index] = format!(
                        "{}{}:: {}",
                        if starts {
                            format!("{}{{loose}}\n", " ".repeat(target))
                        } else {
                            String::new()
                        },
                        " ".repeat(target),
                        if starts_definition_body {
                            empty_term
                        } else {
                            &term
                        }
                    );
                    if starts_definition_body {
                        if container_body {
                            lines[index].push_str(&format!(
                                "\n{}:  {empty_term}\n{}{term}",
                                " ".repeat(target),
                                " ".repeat(target + 3)
                            ));
                        } else {
                            lines[index].push_str(&format!("\n{}:  {term}", " ".repeat(target)));
                        }
                        let frame = stack.last_mut().unwrap();
                        frame.body = true;
                        frame.ready = true;
                    }
                    continue;
                }
                if top.is_some_and(|frame| frame.ready && indent >= frame.source + 2) {
                    let frame = top.expect("checked above");
                    let target = frame.target + 3;
                    let lead = if frame.body {
                        " ".repeat(target)
                    } else {
                        format!("{}:  ", " ".repeat(frame.target))
                    };
                    if let Some(parent) = stack.last_mut() {
                        parent.body = true;
                    }
                    lines[index] = format!(
                        "{lead}{{loose}}\n{}:: {}",
                        " ".repeat(target),
                        if starts_definition_body {
                            empty_term
                        } else {
                            &term
                        }
                    );
                    if starts_definition_body {
                        if container_body {
                            lines[index].push_str(&format!(
                                "\n{}:  {empty_term}\n{}{term}",
                                " ".repeat(target),
                                " ".repeat(target + 3)
                            ));
                        } else {
                            lines[index].push_str(&format!("\n{}:  {term}", " ".repeat(target)));
                        }
                    }
                    stack.push(DefinitionFrame {
                        source: indent,
                        target,
                        body: starts_definition_body,
                        ready: starts_definition_body,
                    });
                    continue;
                }
            }
        }
        if stack.is_empty() {
            continue;
        }
        if lines[index].trim().is_empty() {
            if let Some(frame) = stack.last_mut() {
                if !frame.body
                    && lines
                        .get(index + 1)
                        .is_some_and(|line| rejected_comment_starts_block(line.trim_start()))
                {
                    lines[index] = format!("{}:  {empty_term}\n", " ".repeat(frame.target));
                    frame.body = true;
                }
                frame.ready = true;
            }
            continue;
        }
        if !stack.last().is_some_and(|frame| frame.ready) {
            lines[index] = format!(
                "{}{}",
                " ".repeat(stack.last().unwrap().target),
                lines[index].trim_start()
            );
            continue;
        }
        let (indent, _) = leading_indent(&lines[index]);
        while stack.last().is_some_and(|frame| indent < frame.source + 2) {
            stack.pop();
        }
        let Some(frame) = stack.last_mut() else {
            continue;
        };
        let payload =
            lines[index][bytes_through_columns(&lines[index], frame.source + 2)..].to_owned();
        let extra = indent.saturating_sub(frame.source + 2);
        lines[index] = if frame.body {
            format!("{}{}", " ".repeat(frame.target + 3 + extra), payload)
        } else {
            format!(
                "{}:  {}{}",
                " ".repeat(frame.target),
                " ".repeat(extra),
                payload
            )
        };
        frame.body = true;
    }
    lines.join("\n")
}

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
    let mut nested_lines = Vec::with_capacity(masked.len());
    let mut ancestors: Vec<(usize, bool)> = Vec::new();
    for line in &masked {
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
    let fenced_mask = mask_djot_fences(source, None, &[], true);
    let fenced_lines: Vec<_> = fenced_mask.split('\n').collect();
    let mut div_widths = Vec::new();
    let mut item_columns: Vec<(usize, usize)> = Vec::new();
    let mut boundary = true;
    let mut thematic_lines = HashSet::new();
    let mut quote_depth = 0;
    let mut quote_exit_items = HashMap::new();
    let mut empty_task_breaks = HashMap::new();
    let empty_marker = cached_regex!(
        r"^(?:[-*+](?: \[[ xX]\])?|(?:[0-9]+|[ivxlcdm]+|[IVXLCDM]+|[a-zA-Z])[.)]|\((?:[0-9]+|[A-Za-z]|[ivxlcdm]+|[IVXLCDM]+)\))[ \t]*$"
    )
    .unwrap();
    let full_marker = cached_regex!(
        r"^(?:[-*+]|(?:[0-9]+|[ivxlcdm]+|[IVXLCDM]+|[a-zA-Z])[.)]|\((?:[0-9]+|[A-Za-z]|[ivxlcdm]+|[IVXLCDM]+)\))[ \t]+"
    )
    .unwrap();
    let heading = cached_regex!(r"^#{1,6}(?:[ \t]|$)").unwrap();
    let thematic = cached_regex!(r"^(?:(?:\*[ \t]*){3,}|(?:-[ \t]*){3,})$").unwrap();
    let quote_owner_prefix = cached_regex!(r"^[ \t]*>(?:[ \t]|$)").unwrap();
    for index in 0..lines.len() {
        let (_, content_at) = leading_indent(&lines[index]);
        if masked[index].as_bytes().get(content_at) != lines[index].as_bytes().get(content_at) {
            boundary = fenced_lines[index].trim().is_empty();
            continue;
        }
        let prefix = quote_prefix_len(&lines[index]);
        if prefix > 0
            && !fenced_lines[index].trim().is_empty()
            && !masked[index][..prefix].contains('>')
        {
            boundary = false;
            continue;
        }
        let depth = lines[index][..prefix]
            .bytes()
            .filter(|byte| *byte == b'>')
            .count();
        let mut owner_quote_columns = Vec::new();
        if depth > quote_depth {
            let mut rest = lines[index].as_str();
            while let Some(prefix) = quote_owner_prefix.find(rest) {
                owner_quote_columns.push(leading_indent(rest).0);
                rest = &rest[prefix.end()..];
            }
        }
        let outside_item_quote = depth > quote_depth
            && item_columns.iter().any(|&(held_depth, column)| {
                owner_quote_columns
                    .get(held_depth)
                    .is_some_and(|&indent| indent <= column)
            });
        if outside_item_quote {
            item_columns.retain(|&(held_depth, column)| {
                !owner_quote_columns
                    .get(held_depth)
                    .is_some_and(|&indent| indent <= column)
            });
            boundary = true;
        }
        if depth > quote_depth && !boundary {
            continue;
        }
        let exits_quote = depth < quote_depth;
        if depth != quote_depth {
            boundary = true;
        }
        quote_depth = depth;
        let rest = &masked[index][prefix..];
        let (column, indent) = leading_indent(&lines[index][prefix..]);
        let content = &rest[indent..];
        let authored = &lines[index][prefix + indent..];
        if content.trim().is_empty() {
            boundary = authored.trim().is_empty()
                || (fenced_lines[index] != lines[index] && fenced_lines[index].trim().is_empty());
            continue;
        }
        let colon_width = authored.bytes().take_while(|byte| *byte == b':').count();
        let div_close = colon_width >= 3
            && authored.trim_end().len() == colon_width
            && div_widths.last() == Some(&(depth, colon_width));
        if div_close || (boundary && colon_width >= 3) {
            if div_close {
                div_widths.pop();
            } else {
                div_widths.push((depth, colon_width));
            }
            boundary = true;
            item_columns.retain(|&(held_depth, held_column)| {
                held_depth < depth || held_depth == depth && held_column < column
            });
            continue;
        }
        if boundary
            && authored.starts_with('{')
            && read_djot_attributes(authored, 0, false, false)
                .is_some_and(|(end, _)| authored[end..].trim().is_empty())
        {
            continue;
        }
        let thematic_boundary = boundary
            || item_columns
                .iter()
                .any(|&(held_depth, held_column)| held_depth == depth && held_column == column);
        if heading.is_match(content) || (thematic_boundary && thematic.is_match(content)) {
            if thematic_boundary && thematic.is_match(content) {
                thematic_lines.insert(index);
            }
            boundary = true;
            item_columns.retain(|&(held_depth, held_column)| {
                held_depth < depth || held_depth == depth && held_column < column
            });
            continue;
        }
        let sibling = item_columns
            .iter()
            .any(|&(held_depth, held_column)| held_depth == depth && held_column == column);
        let allow_task_attributes = lines
            .get(index + 1)
            .map_or(true, |line| line.trim().is_empty());
        let empty_task_end = empty_djot_task_tail_end(authored, allow_task_attributes);
        let empty_task = empty_task_end.is_some()
            && empty_djot_task_tail_end(content, allow_task_attributes).is_some();
        if (empty_marker.is_match(content) && empty_marker.is_match(authored) || empty_task)
            && (boundary || sibling)
        {
            if exits_quote {
                quote_exit_items.insert(index, lines[index][..prefix].trim_end().to_owned());
            }
            let marker = empty_task_end
                .map_or(content, |end| &authored[..end])
                .trim_end();
            let marker = if marker == "+" && !only_empty {
                "-"
            } else {
                marker
            };
            if empty_task {
                let ends_before_text = lines.get(index + 1).is_some_and(|next| {
                    let next_prefix = quote_prefix_len(next);
                    let next_depth = next[..next_prefix]
                        .bytes()
                        .filter(|byte| *byte == b'>')
                        .count();
                    let (next_column, next_indent) = leading_indent(&next[next_prefix..]);
                    let next_content = &next[next_prefix + next_indent..];
                    next_depth == depth
                        && next_column <= column
                        && !next_content.trim().is_empty()
                        && !full_marker.is_match(next_content)
                        && !empty_marker.is_match(next_content)
                        && !heading.is_match(next_content)
                        && !next_content.starts_with(['>', '{', '|'])
                        && !next_content.starts_with(":::")
                        && !cached_regex!(r"^\[[^\]\n]+\]:")
                            .unwrap()
                            .is_match(next_content)
                });
                if ends_before_text {
                    empty_task_breaks.insert(index, lines[index][..prefix].trim_end().to_owned());
                }
            }
            lines[index] = format!("{}{}{} +", &lines[index][..prefix], &rest[..indent], marker);
            item_columns.retain(|&(held_depth, held_column)| {
                held_depth < depth || held_depth == depth && held_column < column
            });
            item_columns.push((depth, column));
            boundary = true;
        } else {
            if full_marker.is_match(content) && (boundary || sibling) {
                item_columns.retain(|&(held_depth, held_column)| {
                    held_depth < depth || held_depth == depth && held_column < column
                });
                item_columns.push((depth, column));
            } else if boundary {
                item_columns.retain(|&(held_depth, held_column)| {
                    held_depth < depth || held_depth == depth && held_column < column
                });
            }
            boundary = false;
        }
    }
    if only_empty {
        return lines
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
            .join("\n");
    }
    let rewritten = lines.join("\n");
    let masked_source = mask_code_and_destinations(&rewritten);
    let masked: Vec<&str> = masked_source.split('\n').collect();
    let mut containers: Vec<(usize, bool)> = Vec::new();
    let host_prefix = cached_regex!(r"^(?:[ \t]*> ?|[ \t]*(?:(?:[-*+]|(?:[0-9]+|[ivxlcdm]+|[IVXLCDM]+|[a-zA-Z])[.)]|\([0-9A-Za-z]+\)) +(?:\[[ xX]\] +)?|: |\[\^[^\]\r\n]+\]: +))").unwrap();
    for index in 0..lines.len() {
        let (_, content_at) = leading_indent(&lines[index]);
        if masked[index].as_bytes().get(content_at) != lines[index].as_bytes().get(content_at) {
            continue;
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
            continue;
        }
        let prefix = quote_prefix_len(masked[index]);
        let quote = &masked[index][..prefix];
        let rest = &masked[index][prefix..];
        let (_, indent_bytes) = leading_indent(rest);
        let content = &rest[indent_bytes..];
        if content.trim().is_empty() {
            continue;
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
                && full_marker.is_match(content)
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
                continue;
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

fn emphasis_mask(source: &str) -> String {
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

fn rewrite_djot_body(djot: &str) -> String {
    rewrite_djot_body_with(djot, str::to_string)
}

fn rewrite_djot_body_with(djot: &str, transform: impl FnMut(&str) -> String) -> String {
    rewrite_djot_body_with_quotes(djot, transform, false)
}

fn rewrite_djot_body_with_quotes(
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

fn rewrite_djot_inline(source: &str) -> String {
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

struct Rule {
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
fn is_word_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// Is the byte at `index` escaped by a backslash? An odd run of backslashes
/// before it escapes it; an even run is literal backslashes and the character
/// still counts. `\_not em\_` is text, not emphasis.
fn is_escaped(bytes: &[u8], index: usize) -> bool {
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
fn find_pairs(masked: &str, rule: &Rule, source: &str) -> Vec<(usize, usize, usize, usize)> {
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

fn find_closer(masked: &str, from: usize, rule: &Rule, source: &str) -> Result<usize, usize> {
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
fn blank_line_follows(bytes: &[u8], index: usize) -> bool {
    quoted_blank_line_follows(bytes, index, false)
}

fn quoted_blank_line_follows(bytes: &[u8], index: usize, quoted: bool) -> bool {
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
fn mask_code_and_destinations(source: &str) -> String {
    let _paragraph_cache = ParagraphCacheScope::new(source);
    mask_djot_inline(source, true)
}

fn mask_djot_inline(source: &str, inline_forms: bool) -> String {
    let _paragraph_cache = ParagraphCacheScope::new(source);
    mask_djot_forms(source, inline_forms, true, None, &[])
}

type FenceLineCallback<'a> = &'a mut dyn FnMut(usize, &str);

fn mask_djot_fences(
    source: &str,
    on_fence_line: Option<FenceLineCallback<'_>>,
    row_boundaries: &[bool],
    strict: bool,
) -> String {
    mask_djot_fences_with_open(source, on_fence_line, row_boundaries, strict, None)
}

fn mask_djot_fences_with_open(
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

fn mask_djot_forms(
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

fn mask_djot_forms_with_options(
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
struct OpaqueOptions<'a> {
    code: bool,
    raw_code: bool,
    unclosed_code: bool,
    autolinks: bool,
    destinations: bool,
    attribute_values: bool,
    on_raw_format: Option<RawFormatCallback<'a>>,
    on_fence_open: Option<&'a dyn Fn(usize)>,
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

fn mask_djot_opaque(source: &str, options: OpaqueOptions<'_>) -> Vec<u8> {
    let _paragraph_cache = ParagraphCacheScope::new(source);
    mask_djot_opaque_with_comments(source, options).0
}

fn mask_djot_opaque_with_comments(
    source: &str,
    options: OpaqueOptions<'_>,
) -> (Vec<u8>, Vec<(usize, usize)>) {
    mask_djot_opaque_with_comments_and_code(source, options, None)
}

struct DjotCodeSpan {
    start: usize,
    end: usize,
    closed: bool,
}

fn mask_djot_opaque_with_comments_and_code(
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

fn blank_out(mask: &mut [u8], from: usize, to: usize) {
    let end = to.min(mask.len());
    for byte in &mut mask[from..end] {
        if *byte != b'\n' {
            *byte = b' ';
        }
    }
}

fn djot_backtick_closers(bytes: &[u8]) -> HashMap<usize, usize> {
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

fn find_backtick_close(bytes: &[u8], from: usize, run: usize) -> Option<usize> {
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

/// Escape Carve inline syntax that is ORDINARY TEXT in Djot.
#[derive(Clone, Copy, Debug)]
pub(crate) struct HandledDelimiters<'a> {
    /// Braced runs (`{X…X}`) the caller's language spells too.
    pub braced: &'a str,
    /// Bare runs (`X…X`) the caller's language spells too.
    pub bare: &'a str,
}

impl HandledDelimiters<'_> {
    /// Djot: the language of `djot_to_carve`.
    pub(crate) const DJOT: HandledDelimiters<'static> = HandledDelimiters {
        braced: "=+-*_^~",
        bare: "~*_",
    };

    /// A language that owns none of these delimiters: HTML and BBCode text.
    ///
    /// The BBCode importer passes this after protecting its tag and literal
    /// spans, because BBCode owns none of Carve's inline delimiters.
    pub(crate) const PLAIN: HandledDelimiters<'static> = HandledDelimiters {
        braced: "",
        bare: "",
    };

    /// Markdown: the `markdown` profile of the shared escaper corpus.
    #[cfg(test)]
    pub(crate) const MARKDOWN: HandledDelimiters<'static> = HandledDelimiters {
        braced: "*_",
        bare: "*_~",
    };

    fn owns_braced(&self, delim: u8) -> bool {
        self.braced.as_bytes().contains(&delim)
    }

    fn owns_bare(&self, delim: u8) -> bool {
        self.bare.as_bytes().contains(&delim)
    }
}

/// Every braced run Carve spells. A caller's handled set is subtracted from
/// this; what is left is what gets frozen.
const BRACED_DELIMITERS: &[u8] = b",/#=+-*_^~";

/// Every bare run Carve spells (PART 4: `/ * _ ~ =`).
const BARE_DELIMITERS: &[u8] = b"/=~*_";

/// Freeze the Carve constructs in `source` that the caller's language leaves as
/// literal text, given the delimiters that language HANDLES itself.
pub(crate) fn escape_plain_carve_syntax(source: &str, handled: HandledDelimiters<'_>) -> String {
    let _paragraph_cache = ParagraphCacheScope::new(source);
    let masked = mask_code_and_destinations(source);
    escape_plain_carve_syntax_masked(source, handled, &masked)
}

fn escape_plain_carve_syntax_masked(
    source: &str,
    handled: HandledDelimiters<'_>,
    masked: &str,
) -> String {
    let mask = masked.as_bytes();
    let mut at: Vec<usize> = Vec::new();

    // `%%` opens a comment at the start of a line or after whitespace, and a
    // longer run opens on its FIRST two: a third sign neither closes the
    // comment nor cancels it, so `a %%%c b` loses everything after the run.
    // One escape on the first sign makes the whole run literal, because the
    // rest then sit inside a word (carve-rs#2451).
    let mut i = 0;
    while i + 1 < mask.len() {
        if mask[i] == b'%'
            && mask[i + 1] == b'%'
            && !is_escaped(mask, i)
            && (i == 0 || matches!(mask[i - 1], b' ' | b'\t' | b'\n'))
        {
            at.push(i);
            i += 2;
            continue;
        }
        i += 1;
    }

    // Braced forms whose delimiter this converter does not own.
    for &delim in BRACED_DELIMITERS {
        if handled.owns_braced(delim) {
            continue;
        }
        let mut i = 0;
        while i + 3 < mask.len() {
            if mask[i] != b'{' || mask[i + 1] != delim || is_escaped(mask, i) {
                i += 1;
                continue;
            }
            match find_braced_close(mask, i + 2, delim) {
                Some(end) => {
                    at.push(i);
                    i = end + 2;
                }
                None => {
                    // AN UNCLOSED BRACED OPENER STILL FREEZES. The escaper's
                    // unit is a LINE, but a braced run is not: `a {^x` here and
                    // `y^} b` on the next line render one `<sup>x\ny</sup>`. An
                    // opener left bare therefore lets the NEXT line close it and
                    // turns two lines of literal text into markup, which is the
                    // one failure a line-oriented escaper cannot see from inside
                    // its own line (corpus case `braced-unclosed`). A bare pair
                    // is deliberately NOT treated this way - `bare-unclosed`
                    // pins it unchanged under every profile.
                    if braced_run_opens(mask, i + 2) {
                        at.push(i);
                    }
                    i += 1;
                }
            }
        }
    }

    // Bare pairs: the ones the caller's language does not spell are the author's
    // literal text. Under the Djot profile that is `/` (Carve emphasis) and `=`
    // (Carve highlight), neither of which is Djot syntax.
    for &delim in BARE_DELIMITERS {
        if handled.owns_bare(delim) {
            continue;
        }
        let mut closer = None;
        let mut line_is_blank = false;
        for i in (0..mask.len()).rev() {
            let byte = mask[i];
            if byte == b'\n' {
                if line_is_blank {
                    closer = None;
                }
                line_is_blank = true;
                continue;
            }
            if !matches!(byte, b' ' | b'\t' | b'\r') {
                line_is_blank = false;
            }
            if byte != delim || is_escaped(mask, i) {
                continue;
            }
            let before = i.checked_sub(1).map(|at| mask[at]);
            let after = mask.get(i + 1).copied();
            let whitespace = |byte| matches!(byte, b' ' | b'\t' | b'\r' | b'\n');
            let brace_protects =
                handled.owns_braced(delim) && before == Some(b'{') && !is_escaped(mask, i - 1);
            let opens = before != Some(delim)
                && !before.is_some_and(|byte| byte.is_ascii_alphanumeric())
                && !brace_protects
                && after.is_some_and(|byte| byte != delim && !whitespace(byte));
            if opens && closer.is_some_and(|at| at > i + 1) {
                at.push(i);
            }
            if before.is_some_and(|byte| !whitespace(byte))
                && !after.is_some_and(|byte| byte.is_ascii_alphanumeric())
            {
                closer = Some(i);
            }
        }
    }

    let mut i = 0;
    while i < mask.len() {
        if mask[i] != b'#' || is_escaped(mask, i) {
            i += 1;
            continue;
        }
        let before_ok = i == 0 || !mask[i - 1].is_ascii_alphanumeric();
        let opens_tag = mask
            .get(i + 1)
            .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'-');
        if before_ok && opens_tag {
            at.push(i);
        }
        i += 1;
    }

    // A MENTION is the tag's sibling and needs the same rule for the same
    // reason: it opens on its own, so nothing downstream neutralizes it.
    // Ported from carve-php#1381, which fixed the same gap there. Djot has no
    // mention either, so prose quoting a framework directive came back as a
    // span that existed nowhere in the source.
    //
    // Mirrors `parse_mention` rather than approximating it: a mention opens on
    // an `@` NOT preceded by an alphanumeric or `_` and followed by a name
    // character. The preceding-character test is what leaves an email address
    // alone, since `foo@bar` has a letter before the `@`.
    let mut i = 0;
    while i < mask.len() {
        if mask[i] != b'@' || is_escaped(mask, i) {
            i += 1;
            continue;
        }
        let before_ok = i == 0 || !(mask[i - 1].is_ascii_alphanumeric() || mask[i - 1] == b'_');
        let opens_mention = mask
            .get(i + 1)
            .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_' || *b == b'-');
        if before_ok && opens_mention {
            at.push(i);
        }
        i += 1;
    }

    let mut i = 0;
    while i < mask.len() {
        if mask[i] != b':' || is_escaped(mask, i) {
            i += 1;
            continue;
        }
        if i > 0 && (mask[i - 1].is_ascii_alphanumeric() || mask[i - 1] == b'_') {
            i += 1;
            continue;
        }
        let Some(&first) = mask.get(i + 1) else {
            break;
        };
        if !first.is_ascii_alphanumeric() && first != b'+' && first != b'-' {
            i += 1;
            continue;
        }
        let mut len = 1;
        while let Some(&b) = mask.get(i + 1 + len) {
            if b.is_ascii_alphanumeric() || b == b'_' || b == b'+' || b == b'-' {
                len += 1;
            } else {
                break;
            }
        }
        if mask.get(i + 1 + len) == Some(&b':') {
            at.push(i);
            // The whole shortcode is consumed, the way the parser consumes it.
            i += len + 2;
        } else {
            i += 1;
        }
    }

    for (i, byte) in source.bytes().enumerate() {
        if byte == b'!'
            && source.as_bytes().get(i + 1) == Some(&b'`')
            && mask[i] == b'!'
            && !is_escaped(source.as_bytes(), i)
        {
            at.push(i);
        }
    }
    if at.is_empty() {
        return source.to_string();
    }

    at.sort_unstable();
    at.dedup();
    let bytes = source.as_bytes();
    let mut out = Vec::with_capacity(bytes.len() + at.len());
    let mut cursor = 0;
    for point in at {
        out.extend_from_slice(&bytes[cursor..point]);
        out.push(b'\\');
        cursor = point;
    }
    out.extend_from_slice(&bytes[cursor..]);

    String::from_utf8(out).expect("inserting an ASCII backslash preserves UTF-8")
}

/// Whether a `{X` whose content starts at `from` opens a braced run at all.
///
/// The parser needs non-space content against the delimiter, so `{ ^x^ }` opens
/// nothing and is ordinary text. This is what separates "does not open" from
/// "opens and is never closed": only the second freezes.
fn braced_run_opens(mask: &[u8], from: usize) -> bool {
    mask.get(from).is_some_and(|b| !b.is_ascii_whitespace())
}

/// The offset of the `X}` that closes a `{X` opened before `from`, on the same
/// line, with non-space content between.
fn find_braced_close(mask: &[u8], from: usize, delim: u8) -> Option<usize> {
    if !braced_run_opens(mask, from) {
        return None;
    }
    let mut j = from;
    while j + 1 < mask.len() {
        if mask[j] == b'\n' {
            return None;
        }
        if mask[j] == delim && mask[j + 1] == b'}' && j > from && !mask[j - 1].is_ascii_whitespace()
        {
            return Some(j);
        }
        j += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    #[test]
    fn empty_task_items_keep_their_checkboxes() {
        for (source, expected) in [
            ("- [ ]\n", "- [ ] +\n"),
            ("- [ ] a\n- [x]\n", "- [ ] a\n- [x] +\n"),
            ("- [ ] a\n- [x] b\n", "- [ ] a\n- [x] b\n"),
            ("1. [ ]\n", "1. [ ]\n"),
        ] {
            assert_eq!(super::djot_to_carve(source), expected, "{source:?}");
        }
    }

    #[test]
    fn nested_image_label_import_has_bounded_work() {
        let depth = 1600;
        let source = format!("{}x{}", "![".repeat(depth), "](u)".repeat(depth));
        super::IMAGE_LABEL_FULL_PARSES.with(|count| count.set(0));
        assert_eq!(super::djot_to_carve(&source), "{%%}![x](u)");
        super::IMAGE_LABEL_FULL_PARSES.with(|count| assert!(count.get() <= 2));
        assert!(super::simple_nested_image_label("![a](u) ![b](v)").is_none());
    }
    use super::*;

    #[test]
    fn image_reference_child_text() {
        let alt = cached_regex!(r#"<img\b[^>]*\balt="([^"]*)""#).unwrap();
        let href = cached_regex!(r#"<a\b[^>]*\bhref="([^"]*)""#).unwrap();
        let code = cached_regex!(r"(?s)<code\b[^>]*>(.*?)</code>").unwrap();
        let tags = cached_regex!(r"<[^>]*>").unwrap();
        let space = cached_regex!(r"\s+").unwrap();
        for fixture in [
            include_str!("../tests/fixtures/djot-image-reference-child-text.json"),
            include_str!("../tests/fixtures/djot-image-reference-hard-breaks.json"),
        ] {
            let cases: serde_json::Value = serde_json::from_str(fixture).unwrap();
            for case in cases.as_array().unwrap() {
                let source = case["source"].as_str().unwrap();
                let imported = djot_to_carve(source);
                let html = crate::to_html(&imported);
                let actual = alt
                    .captures(&html)
                    .map(|capture| capture[1].replace("&apos;", "'"));
                if let Some(expected) = case.get("text") {
                    let text = tags.replace_all(&html, "");
                    let text = space.replace_all(&text, " ");
                    assert_eq!(Some(text.trim()), expected.as_str(), "{source}\n{imported}");
                }
                if let Some(expected) = case.get("code") {
                    let actual_code = code.captures(&html).map(|capture| capture[1].to_owned());
                    assert_eq!(
                        actual_code.as_deref(),
                        expected.as_str(),
                        "{source}\n{imported}"
                    );
                }
                if let Some(expected) = case.get("href") {
                    let actual_href = href.captures(&html).map(|capture| capture[1].to_owned());
                    assert_eq!(
                        actual_href.as_deref(),
                        expected.as_str(),
                        "{}\n{source}\n{imported}",
                        case["name"]
                    );
                }
                assert_eq!(
                    actual.as_deref(),
                    case["alt"]
                        .as_str()
                        .map(|text| text.replace("&apos;", "'"))
                        .as_deref(),
                    "{}\n{source}\n{imported}",
                    case["name"]
                );
            }
        }
    }

    #[test]
    fn site_frontmatter_is_preserved_outside_the_djot_reader() {
        let source = "---\nkey: my_long_value\n---\n\nBody.\n";
        assert_eq!(djot_to_carve(source), source);
    }

    #[test]
    fn multiple_footnotes_are_opaque_to_delimiter_conversion() {
        let source =
            "Here's one.[^foo] Here's two.[^bar]\n\n[^foo]: First note.\n\n[^bar]: Second note.\n";
        let carve = djot_to_carve(source);
        assert!(carve.contains("[^foo]"), "{carve}");
        assert!(carve.contains("[^bar]"), "{carve}");
        assert!(carve.contains("First note."), "{carve}");
        assert!(carve.contains("Second note."), "{carve}");
        assert!(!carve.contains("[{^"), "{carve}");
        assert!(!carve.contains("[^}"), "{carve}");
    }

    #[test]
    fn an_unclosed_footnote_token_does_not_mask_later_lines() {
        assert_eq!(
            djot_to_carve("Unclosed [^ then _one_.\n\nLater _two_."),
            "Unclosed [\\^ then /one/.\n\nLater /two/."
        );
    }

    #[test]
    fn a_definition_document_does_not_rewrite_code_that_looks_like_a_term() {
        let source = ": term\n\n  Body.\n\n```\n:: code\n```\n";
        let carve = djot_to_carve(source);
        assert!(carve.contains("```\n:: code\n```"), "{carve}");
        assert!(!carve.contains("```\n{loose}"), "{carve}");
    }

    #[test]
    fn a_djot_definition_item_becomes_a_carve_definition_item() {
        let source = ": orange\n\n  A citrus fruit.\n";
        let carve = djot_to_carve(source);
        assert!(carve.contains("{loose}"), "{carve}");
        assert!(carve.contains(":: orange"), "{carve}");
        assert!(carve.contains(":  A citrus fruit."), "{carve}");
    }

    #[test]
    fn definition_conversion_is_local_to_documents_with_links_and_raw_blocks() {
        let source = ": term\n\n  Body with [a link](/url).\n\n``` =latex\n\\alpha\n```\n";
        let carve = djot_to_carve(source);
        assert!(carve.contains(":: term"), "{carve}");
        assert!(carve.contains("[a link](/url)"), "{carve}");
        assert!(carve.contains("``` =latex\n\\alpha\n```"), "{carve}");
    }

    #[test]
    fn djot_only_block_markers_take_carve_spellings() {
        assert_eq!(djot_to_carve("* * *"), "***");
        assert_eq!(djot_to_carve("      - - -"), "***");
        assert_eq!(djot_to_carve("(1) one\n(2) two"), "1. one\n2. two");
        assert_eq!(djot_to_carve("(a) one\n(b) two"), "a. one\nb. two");
        assert_eq!(djot_to_carve("- item\n\n  * * *"), "- item\n\n  ***");
    }

    #[test]
    fn emphasis_becomes_the_carve_spelling() {
        assert_eq!(djot_to_carve("_em_ text"), "/em/ text");
    }

    #[test]
    fn subscript_and_superscript_take_the_braced_form() {
        assert_eq!(djot_to_carve("H~2~O"), "H{,2,}O");
        assert_eq!(djot_to_carve("E=mc^2^"), "E=mc{^2^}");
    }

    #[test]
    fn repeated_djot_delimiters_preserve_nested_nodes() {
        assert_eq!(djot_to_carve("**strong**"), "{*{*strong*}*}");
        assert_eq!(djot_to_carve("~~gone~~"), "{,{,gone,},}");
    }

    #[test]
    fn highlight_is_already_the_same_form() {
        assert_eq!(djot_to_carve("{=marked=}"), "{=marked=}");
    }

    /// Djot reads any blank run between two markers as one list; Carve reads
    /// three or more as a hard boundary. Passing the run through therefore
    /// split a list the source never split.
    #[test]
    fn a_blank_run_before_a_sibling_marker_does_not_split_the_list() {
        assert_eq!(
            djot_to_carve("- apples\n\n\n\n\n- oranges\n"),
            "- apples\n\n- oranges\n"
        );
    }

    /// The boundary is not a top-level rule, so neither is the collapse.
    #[test]
    fn the_run_collapses_inside_an_item_too() {
        assert_eq!(
            djot_to_carve("- outer\n  - inner\n\n\n\n\n  - inner2\n"),
            "- outer\n  \\- inner\n\n  - inner2\n"
        );
    }

    /// Two blank lines are the loose separator in both languages, so there is
    /// nothing to correct.
    #[test]
    fn a_shorter_run_is_left_alone() {
        assert_eq!(
            djot_to_carve("- apples\n\n\n- oranges\n"),
            "- apples\n\n\n- oranges\n"
        );
    }

    /// Djot's own way of writing two lists is a different marker, and it means
    /// the same in Carve. Nothing about it is a false boundary.
    #[test]
    fn a_marker_change_still_separates_two_lists() {
        assert_eq!(
            djot_to_carve("- apples\n\n* oranges\n"),
            "- apples\n\n* oranges\n"
        );
    }

    /// The rewrite fires only where the two languages disagree, which needs a
    /// list open above the run. After a paragraph the run says nothing in
    /// either language and the author's layout is left as written.
    #[test]
    fn a_run_that_follows_no_list_keeps_its_lines() {
        assert_eq!(
            djot_to_carve("paragraph\n\n\n\n\n- apples\n"),
            "paragraph\n\n\n\n\n- apples\n"
        );
    }

    /// A run followed by an item's own indented content continues that item at
    /// any length -- N1a closes nothing -- so both languages already agree.
    #[test]
    fn a_run_before_an_items_content_keeps_its_lines() {
        assert_eq!(
            djot_to_carve("- apples\n\n\n\n\n  still apples\n"),
            "- apples\n\n\n\n\n  still apples\n"
        );
    }

    /// N1a applies inside a container too, and there a blank line is written
    /// `>`, so the run and the markers around it are read through the prefix.
    #[test]
    fn a_quoted_run_collapses_and_keeps_its_prefix() {
        assert_eq!(
            djot_to_carve("> 1. one\n>\n>\n>\n> 2. two\n"),
            "> 1. one\n>\n> 2. two\n"
        );
    }

    /// An item's own indentation is what says the line continues the item, and
    /// inside a quote it sits right after the marker -- so the quote prefix is
    /// stripped and the indentation is not.
    #[test]
    fn a_quoted_run_after_an_items_indented_content_still_collapses() {
        assert_eq!(
            djot_to_carve("> - apples\n>   more apples\n>\n>\n>\n> - oranges\n"),
            "> - apples\n>   more apples\n>\n> - oranges\n"
        );
    }

    /// A fence opened inside a quote holds its blank lines as content exactly
    /// like an unquoted one.
    #[test]
    fn a_run_inside_a_quoted_fence_is_content() {
        let source = "> ```\n> - one\n>\n>\n>\n> - two\n> ```\n";
        assert_eq!(djot_to_carve(source), source);
    }

    /// A marker one quote level away separates nothing the run could join.
    #[test]
    fn a_run_and_a_marker_at_different_depths_are_left_alone() {
        let source = "> - apples\n>\n>\n>\n- oranges\n";
        assert_eq!(djot_to_carve(source), source);
    }

    /// Blank lines inside a fenced block are that block's content.
    #[test]
    fn a_run_inside_a_fence_is_content() {
        let source = "```\ncode\n\n\n\n- not a marker\n```\n";
        assert_eq!(djot_to_carve(source), source);
    }

    /// An intraword `_x_` CONVERTS, to the braced form. Djot emphasizes it, and
    /// an author who wanted the literal characters had to escape them, so an
    /// unescaped run is emphasis the source states rather than an identifier
    /// the converter should protect.
    #[test]
    fn an_intraword_underscore_converts_to_the_braced_form() {
        assert_eq!(
            djot_to_carve("snake_case_name stays"),
            "snake{/case/}name stays"
        );
        assert_eq!(djot_to_carve("MAX_BUFFER_SIZE"), "MAX{/BUFFER/}SIZE");
        assert_eq!(djot_to_carve("a _x_ and y_z_w"), "a /x/ and y{/z/}w");
    }

    /// The other side, and what makes it safe: the escape survives, so an
    /// author who did mean the identifier keeps it.
    #[test]
    fn an_escaped_intraword_underscore_is_left_alone() {
        assert_eq!(djot_to_carve("snake\\_case\\_name"), "snake\\_case\\_name");
    }

    /// BOUND: the word-bounded rule still emits the BARE form, and shapes that
    /// are not an intraword pair at all are untouched. Removing the intraword
    /// rule leaves every row here passing.
    #[test]
    fn the_surrounding_underscore_shapes_are_unchanged() {
        assert_eq!(djot_to_carve("a _x_ b"), "a /x/ b");
        assert_eq!(djot_to_carve("__init__"), "{/{/init/}/}");
        assert_eq!(djot_to_carve("_leading"), "\\_leading");
        assert_eq!(djot_to_carve("[t](/a_b_c)"), "[t](/a_b_c)");
    }

    #[test]
    fn code_is_never_rewritten() {
        assert_eq!(
            djot_to_carve("`_no_ **no**` yes _yes_"),
            "`_no_ **no**` yes /yes/"
        );
        assert_eq!(
            djot_to_carve("``` js\n_no_\n```\n\n_yes_"),
            "``` js\n_no_\n```\n\n/yes/"
        );
    }

    #[test]
    fn a_destination_is_never_rewritten() {
        assert_eq!(
            djot_to_carve("[t](http://e.com/_a_/b) and _yes_"),
            "[t](http://e.com/_a_/b) and /yes/"
        );
    }

    #[test]
    fn a_pair_never_spans_a_paragraph_break() {
        assert_eq!(djot_to_carve("_a\n\nb_"), "\\_a\n\nb\\_");
        assert_eq!(djot_to_carve("_a\nb_"), "/a\nb/");
    }

    #[test]
    fn an_escaped_delimiter_is_literal_text() {
        assert_eq!(djot_to_carve("\\_not em\\_"), "\\_not em\\_");
    }

    /// A tag is the one construct that is not a pair, so escaping an enclosing
    /// brace cannot neutralize it. Djot has no hashtag - pandoc renders
    /// `a #y b` as `<p>a #y b</p>` - so every `#word` became a Carve tag span
    /// (carve-php#1191).
    #[test]
    fn a_hash_does_not_become_a_tag() {
        assert_eq!(djot_to_carve("a #y b"), "a \\#y b");
        assert_eq!(djot_to_carve("a #1 b"), "a \\#1 b");
        assert_eq!(djot_to_carve("{#y#} x"), "\\{\\#y#} x");
    }

    /// A heading is `#` plus a space and is shared with Djot; `a#y` is not a
    /// tag either. Djot character-reference text freezes before Carve reads it.
    #[test]
    fn the_hash_negatives_stay_bare() {
        assert_eq!(djot_to_carve("# Heading"), "# Heading");
        assert_eq!(djot_to_carve("a#y b"), "a#y b");
        assert_eq!(djot_to_carve("a &#8212; b"), "a &\\#8212; b");
        assert_eq!(djot_to_carve("a &#x2014; b"), "a &\\#x2014; b");
    }

    #[test]
    fn constructs_that_mean_the_same_are_untouched() {
        assert_eq!(djot_to_carve("{+add+} and {-cut-}"), "{+add+} and {-cut-}");
    }

    #[test]
    fn nesting_of_different_families_composes() {
        assert_eq!(djot_to_carve("_a **b** c_"), "/a {*{*b*}*} c/");
    }

    /// Djot spells subscript braced as well as bare and means the same by each,
    /// so the braced form converts too. It previously fell through untouched
    /// and stayed a Carve STRIKETHROUGH, which is a different word.
    #[test]
    fn the_braced_subscript_converts_like_the_bare_one() {
        assert_eq!(djot_to_carve("{~y~} a"), "{,y,} a");
        assert_eq!(djot_to_carve("a{~b~}c"), "a{,b,}c");
        assert_eq!(djot_to_carve("{~y~}"), djot_to_carve("~y~"));
    }

    /// The braced superscript is spelled identically in both languages, so the
    /// conversion is the identity. Without a rule claiming the range the bare
    /// rule matched the `^x^` INSIDE the braces and wrapped it again, into
    /// `{{^x^}}`.
    #[test]
    fn the_braced_superscript_is_not_wrapped_twice() {
        assert_eq!(djot_to_carve("{^x^} a"), "{^x^} a");
        // The inner `{,b,}` is literal text in Djot and a SUBSCRIPT in Carve,
        // so it is escaped rather than passed through. Rendering the result
        // gives Djot's `<sup>a{,b,}c</sup>` back.
        assert_eq!(djot_to_carve("{^a{,b,}c^} x"), "{^a\\{,b,}c^} x");
    }

    /// A `+` bullet is a list item in Djot and the CONTINUATION MARKER in
    /// Carve, so leaving it turns the list into a paragraph.
    #[test]
    fn the_plus_bullet_becomes_a_dash() {
        assert_eq!(djot_to_carve("+ one\n+ two\n"), "- one\n- two\n");
        assert_eq!(djot_to_carve("+ a\n  + b\n"), "- a\n  \\+ b\n");
    }

    /// An empty Djot plus bullet needs an explicit empty Carve item.
    /// A plus inside ordinary text stays literal.
    #[test]
    fn an_empty_plus_item_and_inline_plus_keep_their_roles() {
        assert_eq!(djot_to_carve("+\n"), "- +\n");
        assert_eq!(djot_to_carve("a + b\n"), "a + b\n");
    }

    #[test]
    fn a_table_continuation_row_is_left_alone() {
        let source = "| a | b |\n|---|---|\n| one | x |\n+ continues here | y |\n";
        assert_eq!(table_continuation_lines(source), HashSet::from([4]));
        assert_eq!(djot_to_carve(source), source);
    }

    #[test]
    fn a_plus_bullet_containing_a_pipe_still_becomes_a_dash() {
        assert_eq!(
            djot_to_carve("A paragraph.\n\n+ a bullet with a | pipe\n"),
            "A paragraph.\n\n- a bullet with a | pipe\n"
        );
    }

    /// Carve syntax that is ordinary text in Djot has to be escaped or the
    /// conversion renders something the source never said. `%%` is the sharpest
    /// case: Carve reads it as a line comment and the line DISAPPEARS.
    #[test]
    fn carve_syntax_that_is_plain_djot_text_is_escaped() {
        assert_eq!(djot_to_carve("%% not a comment\n"), "\\%% not a comment\n");
        assert_eq!(djot_to_carve("/slashes/ x\n"), "\\/slashes/ x\n");
        assert_eq!(djot_to_carve("=marked= x\n"), "\\=marked= x\n");
        assert_eq!(djot_to_carve("{,sub,} x\n"), "\\{,sub,} x\n");
    }

    /// Escaping the brace ALONE is not enough when the delimiter inside it has
    /// a bare Carve form: `\\{/y/}` still renders `{<em>y</em>}`.
    #[test]
    fn a_braced_delimiter_with_a_bare_form_escapes_both() {
        assert_eq!(djot_to_carve("{/y/} x\n"), "\\{\\/y/} x\n");
    }

    /// BOUND, and the reason the rule above is delimiter-specific: `{=x=}` is a
    /// highlight in BOTH languages, so its inner `=` is markup that must
    /// survive. Escaping it would break the one braced form that already works.
    #[test]
    fn the_braced_highlight_keeps_its_inner_delimiter() {
        assert_eq!(djot_to_carve("{=marked=} x"), "{=marked=} x");
    }

    /// BOUND: escaping never reaches code, a destination, or an unpaired
    /// delimiter. None of these move under any of the rules above.
    #[test]
    fn code_destinations_and_unpaired_delimiters_are_untouched() {
        assert_eq!(djot_to_carve("a `_x_` b\n"), "a `_x_` b\n");
        assert_eq!(djot_to_carve("ftp://x/ y\n"), "ftp://x/ y\n");
        assert_eq!(djot_to_carve("a/b/c\n"), "a/b/c\n");
        assert_eq!(djot_to_carve("```\n/code/\n```\n"), "```\n/code/\n```\n");
    }

    /// A run longer than two is NOT exempt: the comment opens on the first two
    /// signs and the rest of the line disappears, so the first sign takes the
    /// one escape that makes the whole run literal (carve-rs#2451). Every
    /// spelling here, at both run lengths and on a line that is not the first.
    #[test]
    fn a_percent_run_longer_than_two_takes_one_escape_on_its_first_sign() {
        assert_eq!(djot_to_carve("a %%%c b\n"), "a \\%%%c b\n");
        assert_eq!(djot_to_carve("a %%%%c b\n"), "a \\%%%%c b\n");
        assert_eq!(djot_to_carve("%%%c b\n"), "\\%%%c b\n");
        assert_eq!(djot_to_carve("x\n%%%c y\n"), "x\n\\%%%c y\n");
        // BOUND: already escaped in the source, and never a second escape.
        assert_eq!(djot_to_carve("a \\%% c\n"), "a \\%% c\n");
        // BOUND: a lone sign and an intraword pair are not openers.
        assert_eq!(djot_to_carve("a % c\n"), "a % c\n");
        assert_eq!(djot_to_carve("100%%x\n"), "100%%x\n");
        // CONTROL: the two-sign case already escaped and does not move.
        assert_eq!(djot_to_carve("a %% c\n"), "a \\%% c\n");
        assert_eq!(djot_to_carve("a\n%% c\n"), "a\n\\%% c\n");
    }
}

/// The shared escaper corpus (`tests/spec/tests/corpus-escape/`), read directly
/// against the escaper rather than end to end through a converter.
///
/// This lives beside the function rather than in `tests/` because the function
/// is crate-internal: it is one rule with one implementation, not public API.
/// Before markup-carve/carve-rs#995 the only way to reach it from outside was
/// `carve migrate --from djot`, which can only probe inputs that are INERT in
/// Djot - 46 of the corpus's 55, since the other 9 are Djot markup and what
/// came back was the converter doing its job rather than the escaper.
#[cfg(test)]
mod escape_corpus {
    use super::{escape_plain_carve_syntax, HandledDelimiters};
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    /// The profiles THIS crate can produce, by the corpus's names.
    ///
    /// The corpus tells an engine to run "every profile its converters can
    /// produce" and skip the rest, the way the render corpus skips a target an
    /// engine does not implement. All three are listed here deliberately: the
    /// handled set is a parameter now, so a profile with no caller is still a
    /// statement this implementation can be held to, and `markdown` and `plain`
    /// are exactly what a future text-level converter would pass. Which of them
    /// has a caller today is recorded in `a_profile_with_no_caller_is_named`.
    fn profiles() -> BTreeMap<&'static str, HandledDelimiters<'static>> {
        BTreeMap::from([
            ("plain", HandledDelimiters::PLAIN),
            ("markdown", HandledDelimiters::MARKDOWN),
            ("djot", HandledDelimiters::DJOT),
        ])
    }

    fn corpus() -> serde_json::Value {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/spec/tests/corpus-escape/cases.json");
        let raw = std::fs::read_to_string(&path).unwrap_or_else(|e| {
            panic!(
                "escaper corpus not found at {}: {e}\n\
                 Did you initialize the submodule?\n  git submodule update --init",
                path.display()
            )
        });
        serde_json::from_str(&raw).expect("escaper corpus is JSON")
    }

    #[test]
    fn the_handled_sets_match_the_corpus_profiles() {
        // The sets are spelled in two places - here and in the corpus - and a
        // silent drift between them would make every case below pass while
        // measuring the wrong question.
        let corpus = corpus();
        let declared = corpus["profiles"]
            .as_object()
            .expect("profiles is an object");
        for (name, handled) in profiles() {
            let entry = declared
                .get(name)
                .unwrap_or_else(|| panic!("corpus declares no profile {name}"));
            assert_eq!(
                entry.get("braced").and_then(|v| v.as_str()).unwrap_or(""),
                handled.braced,
                "{name}: braced handled set"
            );
            assert_eq!(
                entry.get("bare").and_then(|v| v.as_str()).unwrap_or(""),
                handled.bare,
                "{name}: bare handled set"
            );
        }
    }

    #[test]
    fn a_case_is_read() {
        // Guards the sweep below against a glob that quietly matches nothing -
        // a checker reporting success having compared nothing is the state this
        // rule was already in.
        let corpus = corpus();
        let cases = corpus["cases"].as_array().expect("cases is an array");
        assert!(cases.len() >= 50, "found {} cases", cases.len());
    }

    #[test]
    fn every_case_matches_under_every_profile() {
        let corpus = corpus();
        let profiles = profiles();
        let mut checked = 0usize;
        let mut failures: Vec<String> = Vec::new();
        for case in corpus["cases"].as_array().expect("cases is an array") {
            let name = case["name"].as_str().expect("a case has a name");
            let input = case["input"].as_str().expect("a case has an input");
            for (profile, expected) in case["expected"]
                .as_object()
                .expect("a case has expectations")
            {
                let Some(handled) = profiles.get(profile.as_str()) else {
                    continue;
                };
                let expected = expected.as_str().expect("an expectation is a string");
                let got = escape_plain_carve_syntax(input, *handled);
                if got != expected {
                    failures.push(format!(
                        "{name} [{profile}]: {input:?} -> {got:?}, expected {expected:?}"
                    ));
                }
                checked += 1;
            }
        }
        assert!(
            failures.is_empty(),
            "{} of {checked} escaper cases diverge:\n{}",
            failures.len(),
            failures.join("\n")
        );
        assert!(checked >= 150, "checked only {checked} case-profile pairs");
    }

    #[test]
    fn escaping_only_ever_inserts_backslashes() {
        // The corpus's own invariant, restated against THIS implementation: an
        // expectation is a fabrication if removing its backslashes does not give
        // the input back. Asserting it on the output rather than on the fixture
        // catches an escaper that rewrites text instead of freezing it.
        let corpus = corpus();
        for case in corpus["cases"].as_array().expect("cases is an array") {
            let input = case["input"].as_str().expect("a case has an input");
            for (profile, handled) in profiles() {
                let got = escape_plain_carve_syntax(input, handled);
                assert_eq!(
                    got.replace('\\', ""),
                    *input,
                    "{} [{profile}] rewrote its input",
                    case["name"]
                );
            }
        }
    }

    #[test]
    fn a_delimiter_with_a_space_against_it_is_where_the_opener_test_bites() {
        // `braced_run_opens` is the test that separates "does not open" from
        // "opens and is never closed", and the corpus does NOT reach it: its
        // only inner-space case is `a { ^x^ } b`, whose space sits between the
        // `{` and the delimiter, so the loop never matches the opener at all and
        // every answer this test could give passes. The case that reaches it has
        // the space AFTER the delimiter, and it is pinned here instead.
        assert_eq!(
            escape_plain_carve_syntax("a { ^x^ } b", HandledDelimiters::PLAIN),
            "a { ^x^ } b"
        );
        assert_eq!(
            escape_plain_carve_syntax("a {^ x^} b", HandledDelimiters::PLAIN),
            "a {^ x^} b"
        );
        assert_eq!(
            escape_plain_carve_syntax("a {^ x b", HandledDelimiters::PLAIN),
            "a {^ x b"
        );

        // STATED, NOT ASSERTED AS CORRECT. `a {^ x^} b` renders `a <sup> x</sup>
        // b` in Carve, so the second line above is a LEAK: text that is literal
        // in the calling language reaches Carve as markup. The same holds on the
        // Djot path for the delimiters Djot does not own - `a {, x,} b` is
        // literal Djot and renders `a <sub> x</sub> b` as Carve. Widening the
        // opener to accept a space against the delimiter fixes it, and is left
        // alone here on purpose: the shared corpus has no case for it, carve-php
        // passes the same corpus with the same boundary, and moving one engine
        // off an unpinned shape is how the four spellings drifted apart in the
        // first place. It wants a corpus case and all three engines, not a
        // unilateral change under a ticket about exposing the function.
    }

    #[test]
    fn the_profiles_remain_distinct() {
        // Djot and BBCode are text-level converters. Markdown and HTML build an
        // AST and let the canonical writer emit source, so they escape no text
        // and Markdown passes no handled set.
        // The handled set is what separates the profiles: `*` is Djot markup and
        // is left for the converter, and is literal text under PLAIN. Asserting
        // the two differ is what keeps the parameter load-bearing - a hardwired
        // set would make every profile give the same answer and every case above
        // would still pass.
        assert_eq!(
            escape_plain_carve_syntax("a *x* b", HandledDelimiters::DJOT),
            "a *x* b"
        );
        assert_eq!(
            escape_plain_carve_syntax("a *x* b", HandledDelimiters::PLAIN),
            "a \\*x* b"
        );
        assert_eq!(
            escape_plain_carve_syntax("a ~x~ b", HandledDelimiters::MARKDOWN),
            "a ~x~ b"
        );
        assert_eq!(
            escape_plain_carve_syntax("a ~x~ b", HandledDelimiters::PLAIN),
            "a \\~x~ b"
        );
    }

    /// An at-sign that opens a Carve mention is escaped when it arrives as
    /// text. The sibling of the tag rule, ported from carve-php#1381.
    #[test]
    fn an_at_sign_in_source_text_is_not_a_mention() {
        for (input, want) in [
            ("hi @user ok", "hi \\@user ok"),
            ("@click toggles it", "\\@click toggles it"),
            ("use @keydown.window here", "use \\@keydown.window here"),
            ("see (@can) there", "see (\\@can) there"),
            ("the @-form", "the \\@-form"),
            ("@can and @click", "\\@can and \\@click"),
        ] {
            assert_eq!(
                escape_plain_carve_syntax(input, HandledDelimiters::DJOT),
                want,
                "input {input:?}"
            );
        }
    }

    /// BOUND: the escape mirrors the parser's opener, so an at-sign the parser
    /// never opens on gains no backslash.
    #[test]
    fn an_at_sign_that_opens_nothing_is_left_bare() {
        for input in [
            "mail me at foo@bar.de",
            "a@b",
            "name @ handle",
            "ping @, later",
            "ends with @",
        ] {
            assert_eq!(
                escape_plain_carve_syntax(input, HandledDelimiters::DJOT),
                input,
                "input {input:?}"
            );
        }
    }

    /// BOUND: an at-sign the source already escaped is not escaped twice.
    #[test]
    fn an_already_escaped_at_sign_is_left_alone() {
        assert_eq!(
            escape_plain_carve_syntax("hi \\@user ok", HandledDelimiters::DJOT),
            "hi \\@user ok"
        );
    }

    /// ONLY THE OPENING COLON. Escaping the opener is what makes the whole
    /// shortcode text - the closing colon then has a letter against it and
    /// opens nothing - so a second escape would be bytes PART 11 §4 asks the
    /// writer not to spend. The corpus case `a-symbol-shortcode` pins the
    /// one-escape form; this names WHICH colon under every profile.
    #[test]
    fn a_symbol_shortcode_is_frozen_at_its_opener_only() {
        for (input, expected) in [
            ("a :rocket: b", "a \\:rocket: b"),
            (":rocket:", "\\:rocket:"),
            // The reaction shortcodes: `+` and `-` open a name, `_` does not.
            ("a :+1: b", "a \\:+1: b"),
            ("a :-1: b", "a \\:-1: b"),
            // Two shortcodes against each other are two openers.
            (":a::b:", "\\:a:\\:b:"),
        ] {
            for (profile, handled) in profiles() {
                assert_eq!(
                    escape_plain_carve_syntax(input, handled),
                    expected,
                    "{input:?} under {profile}"
                );
            }
        }
    }

    /// BOUND: THE NEAR NEIGHBOUR THAT MUST NOT MOVE. `a : b : c` is the shape a
    /// rule over every colon would break, and the corpus pins it unchanged
    /// (`a-colon-that-closes-no-shortcode`). The parser opens no symbol at
    /// either colon - a space is not a name character - so neither is offered
    /// an escape here.
    #[test]
    fn a_colon_that_opens_no_shortcode_is_left_bare() {
        for input in [
            // The corpus's own bound.
            "a : b : c",
            // A colon with a letter against it is not an opener.
            "https://example.com",
            "note: see below",
            "12:30:45",
            // A name that never closes.
            "a :rocket b",
            // The fence and the definition marker, which are line-initial
            // constructs the converters already own.
            ":::",
            ": definition",
            // Nothing after the colon at all.
            "ends with :",
        ] {
            for (profile, handled) in profiles() {
                assert_eq!(
                    escape_plain_carve_syntax(input, handled),
                    input,
                    "{input:?} under {profile}"
                );
            }
        }
    }

    /// BOUND: `_` CANNOT OPEN A NAME, because `:_x_:` would steal from
    /// underline - `parse_symbol` excludes it from the first name character's
    /// class and this mirrors that. Asserted on the colon alone: the `_x_` in
    /// it is a bare underline run, which the profiles that do not spell `_`
    /// freeze for their own unrelated reason.
    #[test]
    fn an_underscore_does_not_open_a_symbol_name() {
        for (profile, handled) in profiles() {
            let got = escape_plain_carve_syntax("a :_x_: b", handled);
            assert!(!got.contains("\\:"), "{profile} escaped a colon in {got:?}");
        }
    }

    /// BOUND: a colon the source already escaped is not escaped twice.
    #[test]
    fn an_already_escaped_symbol_opener_is_left_alone() {
        assert_eq!(
            escape_plain_carve_syntax("a \\:rocket: b", HandledDelimiters::DJOT),
            "a \\:rocket: b"
        );
    }
}

#[cfg(test)]
mod attributed_strong_tests {
    #[test]
    fn attributes_do_not_close_strong_delimiters() {
        for (source, expected) in [
            (
                "a *word{#id key=\"*\"}*",
                "<p>a <strong><span id=\"id\" key=\"*\">word</span></strong></p>",
            ),
            (
                "*more words{#id key=\"*\"} here*",
                "<p><strong>more <span id=\"id\" key=\"*\">words</span> here</strong></p>",
            ),
            (
                "`*word{#id key=\"*\"}*`",
                "<p><code>*word{#id key=\"*\"}*</code></p>",
            ),
        ] {
            assert_eq!(crate::to_html(&super::djot_to_carve(source)), expected);
        }
    }
}

#[cfg(test)]
mod attribute_list_tests {
    use super::djot_to_carve;

    #[test]
    fn explicit_class_assignment_resets_classes_within_its_group() {
        for (source, expected) in [
            (
                "word{.a class=b .c}",
                "<p><span class=\"b c\">word</span></p>",
            ),
            (
                "word{.a class=b .c class=d .e}",
                "<p><span class=\"d e\">word</span></p>",
            ),
            (
                "word{.a title=x class=b .c}",
                "<p><span title=\"x\" class=\"b c\">word</span></p>",
            ),
            (
                "word{.a\n.md:flex}",
                "<p><span class=\"a md:flex\">word</span></p>",
            ),
            (
                "word{.a\n.-dash}",
                "<p><span class=\"a -dash\">word</span></p>",
            ),
            (
                "word{.a\n.md:flex class=b .c}",
                "<p><span class=\"b c\">word</span></p>",
            ),
            (
                "word{.a\nclass=b .md:flex}",
                "<p><span class=\"b md:flex\">word</span></p>",
            ),
            (
                "word{class=a}{class=b}",
                "<p><span class=\"a b\">word</span></p>",
            ),
        ] {
            assert_eq!(crate::to_html(&djot_to_carve(source)), expected);
        }
    }

    #[test]
    fn opening_brackets_in_reference_definition_labels_resolve() {
        for (source, expected) in [
            (
                "[a[b]: /u\n\n[`a[b`][]",
                "<p><a href=\"/u\"><code>a[b</code></a></p>",
            ),
            ("[a[b]: /u\n\n[x][a\\[b]", "<p><a href=\"/u\">x</a></p>"),
            (
                "[a[b]: /u\n[b]: /v\n\n[a\\[b][]",
                "<p><a href=\"/u\">a[b</a></p>",
            ),
        ] {
            let result = crate::migration::migrate_djot(source);
            assert_eq!(crate::to_html(&result.value), expected);
            assert!(!result
                .report
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.message.contains("unresolved Djot reference")));
        }
    }

    #[test]
    fn opening_bracket_reference_aliases_keep_contexts_and_avoid_collisions() {
        for source in [
            "[a[b]: /u\n\n| [x][a\\[b] |",
            "[a[b|c]: /u\n\n| [x][a\\[b\\|c] |",
            "[a[b]: /u\n\n> [x][a\\[b]",
            "[a[b]: /u\n\n- [x][a\\[b]",
            "[a[b]: /old\n[a[b]: /u\n\n[x][a\\[b]",
            "[carve-djot-reference-1]: /other\n[a[b]: /u\n\n[x][a\\[b] [y][carve-djot-reference-1]",
        ] {
            let native = djot_to_carve(source);
            let html = crate::to_html(&native);
            assert!(html.contains("<a href=\"/u\">x</a>"), "{source}: {html}");
            if source.contains("/other") {
                assert!(html.contains("<a href=\"/other\">y</a>"));
            }
        }
    }

    #[test]
    fn shorthand_reference_classes_keep_their_append_semantics() {
        for (source, classes) in [
            ("{.a .md:flex}\n[r]: /u\n\n[x][r]", "a md:flex"),
            ("{.a\n  .md:flex}\n[r]: /u\n\n[x][r]", "a md:flex"),
            ("[r]: /u\n\n[x][r]{.a .md:flex}", "a md:flex"),
            ("{.a class=b .md:flex}\n[r]: /u\n\n[x][r]", "b md:flex"),
        ] {
            assert_eq!(
                crate::to_html(&djot_to_carve(source)),
                format!("<p><a href=\"/u\" class=\"{classes}\">x</a></p>"),
                "{source}"
            );
        }
    }

    #[test]
    fn valid_attribute_lists_keep_their_delimiters() {
        for source in [
            "{#id .class}\nA paragraph\n",
            "{#id}\n> Block quote\n",
            "{#id}\n# Heading\n",
            "[nested [span]{.blue}]{#ident}\n",
        ] {
            assert_eq!(djot_to_carve(source), source, "{source}");
        }
        assert_eq!(
            crate::to_html(&djot_to_carve("[span]{title=\"_*#literal*\"}\n")),
            "<p><span title=\"_*#literal*\">span</span></p>"
        );
    }

    #[test]
    fn multiline_attribute_lists_are_canonicalized() {
        assert_eq!(
            crate::to_html(&djot_to_carve(
                "{#id .class\n  style=\"color:red\"}\nA paragraph\n"
            )),
            "<p id=\"id\" class=\"class\" style=\"color:red\">A paragraph</p>"
        );
    }

    #[test]
    fn rejected_attributes_do_not_hide_paragraph_boundaries_or_later_spans() {
        let source = "_a [x]{.a\n\n.b} b_";
        assert!(!djot_to_carve(source).contains("/a"));
        assert!(!crate::to_html(&djot_to_carve("para\n{#id}\nnext")).contains("id=\"id\""));
        assert!(
            crate::to_html(&djot_to_carve("a { \"q\n[x]{title=\"_a_\"}"))
                .contains("<span title=\"_a_\">x</span>")
        );
        assert_eq!(
            crate::to_html(&djot_to_carve("[x]{#a\n.b}")),
            "<p><span id=\"a\" class=\"b\">x</span></p>"
        );
        let invalid = "[x]{k=a*b*}";
        assert_eq!(super::mask_djot_attributes(invalid), invalid);
    }

    #[test]
    fn attributes_in_code_and_invalid_lists_remain_literal() {
        for source in ["`[x]{#id}`", "```\n[x]{#id}\n```"] {
            let written = djot_to_carve(source);
            assert_eq!(crate::to_html(source), crate::to_html(&written));
        }
        assert_eq!(djot_to_carve("[span]{#a<b}"), "[span]\\{\\#a<b}");
        assert_eq!(djot_to_carve("a #tag"), "a \\#tag");
        assert_eq!(crate::to_html(&djot_to_carve("\\{#id}")), "<p>{#id}</p>");
    }
}

#[cfg(test)]
mod heading_continuation_tests {
    use super::djot_to_carve;

    #[test]
    fn headings_fold_matching_markers_and_lazy_lines() {
        for (source, expected) in [
            ("# Heading\n# continued\n", "# Heading continued\n"),
            ("# Heading\nlazy\n", "# Heading lazy\n"),
            (
                "# Heading\nlazy\n# more\nlazy\n\ntext\n",
                "# Heading lazy more lazy\n\ntext\n",
            ),
            ("## A\n## B\nC", "## A B C"),
            ("# A\n  # B\n  C", "# A B C"),
            ("# A\n    B", "# A B"),
        ] {
            assert_eq!(djot_to_carve(source), expected);
        }
    }

    /// A percent run is not a block starter in Djot, so it folds onto the
    /// heading like any other lazy continuation line (carve-rs#2453). The
    /// two-sign run always folded; the three-sign one did not, because the
    /// folder's block-starter list carried the Carve comment fence, and
    /// Carve's two-sign line comment is not in the same list.
    #[test]
    fn a_percent_run_folds_onto_a_heading_at_every_run_length() {
        for (djot, carve, html) in [
            (
                "# A\n%\n",
                "# A %\n",
                "<section id=\"A\">\n  <h1>A %</h1>\n</section>",
            ),
            (
                "# A\n%%\n",
                "# A \\%%\n",
                "<section id=\"A\">\n  <h1>A %%</h1>\n</section>",
            ),
            (
                "# A\n%%%\n",
                "# A \\%%%\n",
                "<section id=\"A\">\n  <h1>A %%%</h1>\n</section>",
            ),
            (
                "# A\n%%%%\n",
                "# A \\%%%%\n",
                "<section id=\"A\">\n  <h1>A %%%%</h1>\n</section>",
            ),
            (
                "## A\n%%%\n",
                "## A \\%%%\n",
                "<section id=\"A\">\n  <h2>A %%%</h2>\n</section>",
            ),
            (
                "###### A\n%%%\n",
                "###### A \\%%%\n",
                "<section id=\"A\">\n  <h6>A %%%</h6>\n</section>",
            ),
            (
                "# A\nB\n",
                "# A B\n",
                "<section id=\"A-B\">\n  <h1>A B</h1>\n</section>",
            ),
            ("A\n%%%\n", "A\n\\%%%\n", "<p>A\n%%%</p>"),
            ("%%%\nX\n", "\\%%%\nX\n", "<p>%%%\nX</p>"),
        ] {
            assert_eq!(djot_to_carve(djot), carve);
            assert_eq!(crate::to_html(carve), html);
        }
    }

    #[test]
    fn headings_stop_at_blocks_and_leave_code_alone() {
        for next in [
            "",
            "## B",
            "#",
            "- item",
            "1. item",
            "> quote",
            "```",
            "~~~",
            "[r]: /url",
            "::: div",
            "  - item",
            "***",
            "---",
            "* * *",
            "^ x",
        ] {
            let source = format!("# A\n{next}\n");
            assert_eq!(
                djot_to_carve(&source),
                format!("# A\n{}", djot_to_carve(&format!("{next}\n")))
            );
        }
        assert_eq!(djot_to_carve("# A\n{.class}\n"), "# A\n");
        assert_eq!(djot_to_carve("para\n# A\nB\n"), "para\n\\# A\nB\n");
        for source in ["```\n# A\n# B\n```\n", "`x\n# A\n# B\ny`\n", "# A\\\nB\n"] {
            assert_eq!(djot_to_carve(source), source);
        }
    }
}

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

fn djot_note_alias(
    key: &str,
    labels: &mut std::collections::HashMap<String, String>,
    serial: &mut usize,
    reserved: &HashSet<usize>,
) -> String {
    labels
        .entry(key.to_owned())
        .or_insert_with(|| {
            while reserved.contains(serial) {
                *serial += 1;
            }
            let value = format!("carve-djot-note-{serial}");
            *serial += 1;
            value
        })
        .clone()
}

fn normalize_djot_footnotes(
    source: &str,
    boundaries: Option<&DjotFootnoteAttributeStrip>,
    inherited: &HashSet<String>,
    empty_note: &str,
) -> String {
    if !source.contains("[^") {
        return source.to_owned();
    }
    let mask = mask_djot_inline(source, false);
    let rows = djot_table_rows_with_empty_note(source, &mask, Some(empty_note));
    let bytes = source.as_bytes();
    let mut non_rows = HashSet::new();
    let mut row_offset = 0;
    for (n, line) in source.split('\n').enumerate() {
        let at = djot_content_start(line);
        if rows.get(n) != Some(&true)
            && line.as_bytes().get(at) == Some(&b'|')
            && mask.as_bytes().get(row_offset + at) == Some(&b'|')
        {
            non_rows.insert(row_offset + at);
        }
        row_offset += line.len() + 1;
    }
    let mut labels = HashMap::new();
    let mut definitions = HashMap::new();
    let mut defined = HashSet::new();
    let mut used = Vec::new();
    let mut used_keys = HashSet::new();
    let reserved: HashSet<usize> = cached_regex!(r"(?i)carve-djot-note-(\d+)")
        .unwrap()
        .captures_iter(source)
        .filter_map(|c| c[1].parse().ok())
        .collect();
    let mut serial = 0usize;
    let mut offset = 0;
    let mut line_heads = HashSet::new();
    let mut empty_definitions = HashSet::new();
    let key_of = |key: &str| {
        let key = remove_inherited_attribute_markers(key, inherited);
        cached_regex!(r"[ \t\r\n]+")
            .unwrap()
            .replace_all(
                key.trim_matches(|ch: char| {
                    (ch.is_whitespace() && ch != '\u{85}') || ch == '\u{feff}'
                }),
                " ",
            )
            .into_owned()
    };
    let unsupported = |key: &str| key.contains(['[', ']', '`', '<', '>', '|', '\\', '\u{c}', '\0']);
    let prefix = cached_regex!(
        r"^[ \t]*(?:>[ \t]+|>$)*(?:(?:[-*+]|(?:[0-9]+|[A-Za-z]|[ivxlcdm]{2,}|[IVXLCDM]{2,})[.)]|\((?:[0-9]+|[A-Za-z]|[ivxlcdm]{2,}|[IVXLCDM]{2,})\)|:)[ \t]+)*"
    )
    .unwrap();
    let head = cached_regex!(r"^\[\^([^\]\n]+)\]:(?:[ \t]|$)").unwrap();
    let thematic = cached_regex!(r"^(?:[ \t]*>[ \t]*)*[ \t]*(?:[-*][ \t]*){3,}$").unwrap();
    let note_lines: Vec<_> = source.split('\n').collect();
    let item = cached_regex!(r"(?:[-*+]|(?:[0-9]+|[A-Za-z]|[ivxlcdm]{2,}|[IVXLCDM]{2,})[.)]|\((?:[0-9]+|[A-Za-z]|[ivxlcdm]{2,}|[IVXLCDM]{2,})\)|:)[ \t]+").unwrap();
    let block = cached_regex!(r"^(?:#{1,6} |`{3,}|~{3,}|:{3,}|\{|\[[^\]]*\]:|(?:[-*][ \t]*){3,}$)")
        .unwrap();
    let reference_head =
        cached_regex!(r"^\[(?:[^\^\]\r\n][^\]\r\n]*)?\]:([ \t]+[^ \t\r\n]*|)\r?$").unwrap();
    let mut reference_payloads = Vec::new();
    let mut literal_reference_colons = HashSet::new();
    let reference_candidate = cached_regex!(r"^\[(?:[^\^\]\r\n][^\]\r\n]*)?\]:").unwrap();
    let mut reference_allowed = true;
    let mut reference_indent = None;
    let mut reference_containers: Vec<(usize, usize)> = Vec::new();
    let mut reference_divs: Vec<(usize, usize)> = Vec::new();
    let reference_div = cached_regex!(r"^(:{3,})(?:[ \t]+\S.*)?[ \t]*$").unwrap();
    let reference_fence_end = cached_regex!(r"^(?:`{3,}|~{3,})[ \t]*$").unwrap();
    let reference_continuation = cached_regex!(r"^[^ \t\r\n]+\r?$").unwrap();
    let note_paragraph_head = cached_regex!(r"^\[\^[^\]\n]+\]:[ \t]*(.+)$").unwrap();
    let note_paragraph_block = cached_regex!(r"^(?:>(?:[ \t]|$)|[-*+][ \t]|#{1,6}(?:[ \t]|$)|[`~]{3,}|:{3,}|[|]|\{|(?:[0-9]+|[A-Za-z]|[ivxlcdm]+|[IVXLCDM]+)[.)][ \t]|\((?:[0-9]+|[A-Za-z]|[ivxlcdm]+|[IVXLCDM]+)\)[ \t])").unwrap();
    let nested_head = cached_regex!(r"\[\^([^\]\n]+)\]:(?:[ \t]|$)").unwrap();
    for (n, line) in note_lines.iter().enumerate() {
        let at = prefix.find(line).unwrap().len();
        line_heads.insert(offset + at);
        let previous_line = n.checked_sub(1).map_or("", |n| note_lines[n]);
        let previous_prefix = prefix.find(previous_line).unwrap().as_str();
        let previous = previous_line[previous_prefix.len()..].trim();
        let mut boundary = boundaries.is_some_and(|strip| strip.is_boundary(previous_line))
            || previous.is_empty()
            || thematic.is_match(previous_line)
            || at < previous_prefix.len() && item.is_match(previous_prefix)
            || n.checked_sub(1).is_some_and(|n| rows.get(n) == Some(&true))
            || item.is_match(&line[..at]) && item.is_match(previous_prefix)
            || line[..at].matches('>').count() < previous_prefix.matches('>').count()
            || block.is_match(previous);
        if at > previous_prefix.len() {
            if let Some(previous_note) = note_paragraph_head.captures(previous) {
                if !note_paragraph_block.is_match(&previous_note[1]) {
                    boundary = false;
                }
            }
        }
        let content = &line[at..];
        let item_prefix = item.is_match(&line[..at]);
        let (quote_depth, quote_content) = quoted(line);
        let quote_prefix = line.len() - quote_content.len();
        let content_column = at.saturating_sub(quote_prefix);
        let mut exited_container = false;
        while reference_containers.last().is_some_and(|&(column, depth)| {
            quote_depth < depth || (quote_depth == depth && content_column < column)
        }) {
            reference_containers.pop();
            exited_container = true;
        }
        if item_prefix {
            while reference_containers
                .last()
                .is_some_and(|&(column, depth)| quote_depth == depth && content_column <= column)
            {
                reference_containers.pop();
            }
            reference_containers.push((content_column, quote_depth));
        }
        let allowed = reference_allowed
            || item_prefix
            || exited_container
            || quote_depth < quoted(previous_line).0
            || n.checked_sub(1).is_some_and(|n| rows.get(n) == Some(&true))
            || boundaries.is_some_and(|strip| strip.is_boundary(previous_line));
        let mut closed_div = false;
        if let Some(caps) = reference_div.captures(content) {
            let width = caps[1].len();
            if content.trim() == &caps[1]
                && reference_divs
                    .last()
                    .is_some_and(|&(open_width, depth)| width >= open_width && quote_depth == depth)
            {
                reference_divs.pop();
                closed_div = true;
            } else if allowed {
                reference_divs.push((width, quote_depth));
            }
        }
        let reference = allowed && mask.as_bytes().get(offset + at) == Some(&b'[');
        if let Some(caps) = reference
            .then(|| reference_head.captures(content))
            .flatten()
        {
            let target = caps.get(1).unwrap();
            reference_payloads.push((offset + at + target.start(), offset + at + target.end()));
            reference_indent = Some((content_column, quote_depth));
            reference_allowed = true;
        } else if reference_indent
            .is_some_and(|(indent, depth)| content_column > indent && quote_depth == depth)
            && !item_prefix
            && reference_continuation.is_match(content)
        {
            reference_payloads.push((offset + at, offset + line.len()));
        } else {
            if mask.as_bytes().get(offset + at) == Some(&b'[') {
                if let Some(candidate) = reference_candidate.find(content) {
                    literal_reference_colons.insert(offset + at + candidate.end() - 1);
                }
            }
            reference_indent = None;
            reference_allowed = content.trim().is_empty()
                || (allowed && !content.starts_with('[') && block.is_match(content))
                || (allowed && thematic.is_match(line))
                || rows.get(n) == Some(&true)
                || boundaries.is_some_and(|strip| strip.is_boundary(line))
                || closed_div
                || (reference_fence_end.is_match(content)
                    && mask.as_bytes().get(offset + at) == Some(&b' '));
        }
        if let Some(caps) = head.captures(&line[at..]) {
            if boundary && mask.as_bytes().get(offset + at) == Some(&b'[') {
                reference_containers.push((content_column + 2, quote_depth));
                let key = key_of(&caps[1]);
                defined.insert(key.clone());
                definitions.insert(offset + at, (key.clone(), offset + at + 2 + caps[1].len()));
                if line[at + caps.get(0).unwrap().len()..].trim().is_empty() {
                    empty_definitions.insert(offset + at);
                    reference_allowed = true;
                }
                if unsupported(&key) {
                    djot_note_alias(&key, &mut labels, &mut serial, &reserved);
                }
                let body_at = at + caps.get(0).unwrap().end();
                for nested in
                    nested_head.captures_iter(&line[body_at..at + djot_content_start(&line[at..])])
                {
                    let nested_at = body_at + nested.get(0).unwrap().start();
                    if mask.as_bytes().get(offset + nested_at) != Some(&b'[') {
                        continue;
                    }
                    let nested_key = key_of(&nested[1]);
                    defined.insert(nested_key.clone());
                    definitions.insert(
                        offset + nested_at,
                        (nested_key.clone(), offset + nested_at + 2 + nested[1].len()),
                    );
                    line_heads.insert(offset + nested_at);
                    if unsupported(&nested_key) {
                        djot_note_alias(&nested_key, &mut labels, &mut serial, &reserved);
                    }
                }
            }
        }
        offset += line.len() + 1;
    }
    let mut mask = mask.into_bytes();
    for (start, end) in reference_payloads {
        blank_out(&mut mask, start, end);
    }
    let mask = String::from_utf8(mask).expect("reference masks preserve UTF-8");
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
    let mut i = 0;
    let blank = cached_regex!(r"^\n[ \t]*(?:>[ \t]+)*>?\n").unwrap();
    let mut destinations = HashMap::new();
    let mut accepted_destinations = HashSet::new();
    let mut destination_line = 0;
    let mut parens = Vec::new();
    while i < bytes.len() {
        if bytes[i] == b'\\' {
            i += 2;
            continue;
        }
        if bytes[i] == b'\n' {
            destination_line += 1;
        }
        if rows.get(destination_line) == Some(&true)
            && bytes[i] == b'|'
            && (i == 0 || bytes[i - 1] != b'\\')
        {
            parens.clear();
        }
        if bytes[i] == b'(' {
            parens.push(i);
        } else if bytes[i] == b')' {
            if let Some(at) = parens.pop() {
                if at > 0 && at < bytes.len() && bytes[at - 1] == b']' {
                    destinations.insert(at, i + 1);
                }
            }
        }
        i += 1;
    }
    let mut malformed_ends = HashMap::new();
    let malformed_prefix =
        cached_regex!(r"^\{(\x00DJOTINVALIDATTR\x00[0-9]+\x00)?[A-Za-z][\w-]*=").unwrap();
    let mut next_brace = None;
    for at in (0..bytes.len()).rev() {
        if bytes[at] == b'\n' {
            next_brace = None;
        } else if bytes[at] == b'}' {
            next_brace = Some(at + 1);
        } else if bytes[at] == b'{'
            && malformed_prefix
                .captures(&source[at..])
                .is_some_and(|value| {
                    value
                        .get(1)
                        .map_or(true, |marker| inherited.contains(marker.as_str()))
                })
        {
            if let Some(end) = next_brace {
                malformed_ends.insert(at, end);
            }
        }
    }
    let mut brackets = HashMap::new();
    let mut stack = Vec::new();
    let mut line_index = 0;
    i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\n' {
            line_index += 1;
            if blank.is_match(&source[i..]) {
                stack.clear();
            }
        }
        if bytes[i] == b'\\' {
            i += 2;
            continue;
        }
        if bytes[i] == b'{' {
            if let Some((end, _)) = read_djot_word_attributes(source, i) {
                i = end;
                continue;
            }
            if let Some(&end) = malformed_ends.get(&i) {
                i = end;
                continue;
            }
        }
        if let Some(&end) = angles.get(&i) {
            i = end;
            continue;
        }
        if mask.as_bytes()[i] == b' ' {
            i += 1;
            continue;
        }
        if rows.get(line_index) == Some(&true)
            && bytes[i] == b'|'
            && i.checked_sub(1).and_then(|p| bytes.get(p)) != Some(&b'\\')
        {
            stack.clear();
            i += 1;
            continue;
        }
        if let Some(definition) = definitions.get(&i) {
            i = definition.1 + 1;
            continue;
        }
        if bytes[i] == b'[' {
            stack.push(i);
        } else if bytes[i] == b']' {
            if let Some(&open) = stack.last() {
                if bytes.get(open + 1) == Some(&b'^') {
                    brackets.insert(stack.pop().unwrap(), i);
                } else if bytes.get(i + 1) == Some(&b'(') && destinations.contains_key(&(i + 1)) {
                    brackets.insert(stack.pop().unwrap(), i);
                    accepted_destinations.insert(i + 1);
                    i = destinations[&(i + 1)];
                    continue;
                } else if bytes.get(i + 1) == Some(&b'[') {
                    let mut end = i + 2;
                    while end < bytes.len() && bytes[end] != b']' {
                        if bytes[end] == b'\\' {
                            end += 1;
                        }
                        end += 1;
                    }
                    if bytes.get(end) == Some(&b']') {
                        brackets.insert(stack.pop().unwrap(), i);
                        i = end;
                    }
                } else if bytes.get(i + 1) == Some(&b'{')
                    && read_djot_word_attributes(source, i + 1).is_some()
                {
                    brackets.insert(stack.pop().unwrap(), i);
                }
            }
        }
        i += 1;
    }
    let image_ends: HashMap<_, _> = brackets
        .iter()
        .filter(|(&at, &close)| {
            at > 0
                && at < bytes.len()
                && bytes[at - 1] == b'!'
                && (bytes.get(at + 1) != Some(&b'^') || close == at + 2)
                && matches!(bytes.get(close + 1), Some(b'[' | b'('))
        })
        .map(|(&at, &close)| (at, close))
        .collect();
    let mut output = Vec::new();
    i = 0;
    while i < source.len() {
        if let Some(&end) = destinations
            .get(&i)
            .filter(|_| accepted_destinations.contains(&i))
        {
            output.extend_from_slice(&bytes[i..end]);
            i = end;
            continue;
        }
        if literal_reference_colons.contains(&i) {
            output.push(b'\\');
        }
        if non_rows.contains(&i) {
            output.push(b'\\');
        }
        if let Some(&end) = image_ends.get(&i) {
            let mut cursor = i;
            let mut at = i + 1;
            while at < end {
                if bytes[at..].starts_with(b"[^") {
                    if let Some(&close) = brackets.get(&at) {
                        if close < end {
                            output.extend_from_slice(&bytes[cursor..at]);
                            cursor = close + 1;
                            at = close;
                        }
                    }
                }
                at += 1;
            }
            output.extend_from_slice(&bytes[cursor..=end]);
            i = end + 1;
            continue;
        }
        if bytes[i] == b'{' && read_djot_word_attributes(source, i).is_none() {
            if let Some(&end) = malformed_ends.get(&i) {
                output.extend_from_slice(&bytes[i..end]);
                i = end;
                continue;
            }
        }
        if bytes[i] == b'{' {
            if let Some((end, _)) = read_djot_word_attributes(source, i) {
                output.extend_from_slice(&bytes[i..end]);
                i = end;
                continue;
            }
        }
        let definition = definitions.get(&i);
        let end = definition
            .map(|d| d.1)
            .or_else(|| brackets.get(&i).copied());
        if bytes[i..].starts_with(b"[^") && (definition.is_some() || mask.as_bytes()[i] == b'[') {
            if let Some(end) = end.filter(|&end| end > i + 2) {
                let key = definition.map_or_else(|| key_of(&source[i + 2..end]), |d| d.0.clone());
                {
                    let rename =
                        labels.contains_key(&key) || unsupported(&key) || !defined.contains(&key);
                    let name = if rename {
                        djot_note_alias(&key, &mut labels, &mut serial, &reserved)
                    } else {
                        key.clone()
                    };
                    let at = i;
                    if definition.is_none() && used_keys.insert(key.clone()) {
                        used.push(key.clone());
                    }
                    if rename || key != source[i + 2..end] {
                        output.extend_from_slice(format!("[^{name}]").as_bytes());
                    } else {
                        output.extend_from_slice(&bytes[i..=end]);
                    }
                    i = end + 1;
                    if definition.is_some() && empty_definitions.contains(&at) {
                        output.extend_from_slice(b": ");
                        output.extend_from_slice(empty_note.as_bytes());
                        i += 1;
                    }
                    if definition.is_some()
                        && bytes.get(i) == Some(&b':')
                        && bytes.get(i + 1) == Some(&b'\t')
                    {
                        output.extend_from_slice(b": ");
                        i += 2;
                    }
                    if definition.is_none()
                        && bytes.get(i) == Some(&b':')
                        && line_heads.contains(&at)
                    {
                        output.extend_from_slice(b"\\:");
                        i += 1;
                    }
                    continue;
                }
            }
        }
        if bytes[i] == b'!'
            && !is_escaped(bytes, i)
            && bytes.get(i + 1..i + 3) == Some(b"[^")
            && brackets.get(&(i + 1)).is_some_and(|&end| end > i + 3)
            && mask.as_bytes().get(i + 1) == Some(&b'[')
        {
            output.push(b'\\');
        }
        output.push(bytes[i]);
        i += 1;
    }
    let mut stubs = String::new();
    for key in used {
        if !defined.contains(&key) {
            let alias = djot_note_alias(&key, &mut labels, &mut serial, &reserved);
            stubs.push_str(&format!("[^{alias}]: {empty_note}\n\n"));
        }
    }
    stubs.push_str(&String::from_utf8(output).expect("footnote replacements preserve UTF-8"));
    stubs
}

fn has_djot_code_layout_candidates(source: &str) -> bool {
    source.contains("\\|")
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
    mut on_raw_code: Option<&mut dyn FnMut(usize)>,
) -> String {
    let _paragraph_cache = ParagraphCacheScope::new(source);
    if !source.contains('`') {
        return source.to_owned();
    }
    let block_attributes = |value: &str| {
        value.starts_with('{')
            && read_djot_word_attributes(value, 0)
                .is_some_and(|(end, _)| value[end..].trim_matches([' ', '\t']).is_empty())
    };
    let bytes = source.as_bytes();
    let mask = mask_djot_opaque(
        &mask_djot_fences(source, None, &[], true),
        OpaqueOptions {
            code: false,
            unclosed_code: false,
            ..OpaqueOptions::default()
        },
    );
    let ticks = cached_regex!(r"`+").unwrap();
    let mut runs: std::collections::HashMap<usize, Vec<usize>> = std::collections::HashMap::new();
    for run in ticks.find_iter(source) {
        runs.entry(run.len()).or_default().push(run.start());
    }
    let close_run = |open: usize, width: usize| {
        runs.get(&width)
            .and_then(|positions| positions.get(positions.partition_point(|&at| at < open + width)))
            .copied()
            .unwrap_or(bytes.len())
    };
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
    let count = tick_starts.len();
    let tick_index = |position: usize| tick_starts.partition_point(|&at| at < position);
    let mut next = vec![count; count + 1];
    let mut ends = vec![bytes.len() + 1; count + 1];
    let mut jumps = vec![count; count + 1];
    let mut depths = vec![0; count + 1];
    // Merge equal-length ancestor jumps so each tick stores one pointer.
    for at in (0..count).rev() {
        let finish = close_run(tick_starts[at], tick_widths[at]) + tick_widths[at];
        let parent = tick_index(finish);
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
    let balanced_ticks = |from: usize, end: usize| {
        let mut at = tick_index(from);
        while at < count && tick_starts[at] < end {
            let jump = jumps[at];
            if jump < count && ends[jump] <= end {
                at = jump;
            } else if ends[at] <= end {
                at = next[at];
            } else {
                return false;
            }
        }
        true
    };
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
    let block = cached_regex!(r"^(?:[-*+] |(?:[0-9]+|[A-Za-z]|[ivxlcdm]+|[IVXLCDM]+)[.)] |\((?:[0-9]+|[A-Za-z]|[ivxlcdm]+|[IVXLCDM]+)\) |: |#{1,6} |`{3,}|~{3,}|:{3,}(?:[ \t]*[A-Za-z0-9_-]+)?[ \t]*$|>(?:[ \t]|$)|\||\^ |\[[^\]]+\]:)").unwrap();
    let marker = cached_regex!(
        r"^(?:\[\^[^\]\n]+\]:[ \t]*|(?:[-*+]|(?:[0-9]+|[A-Za-z]|[ivxlcdm]+|[IVXLCDM]+)[.)]|\((?:[0-9]+|[A-Za-z]|[ivxlcdm]+|[IVXLCDM]+)\)|:)[ \t]+)"
    )
    .unwrap();
    let digits = cached_regex!(r"^[0-9]+").unwrap();
    let letters = cached_regex!(r"^[A-Za-z]+").unwrap();
    let item_kind_of = |item: &str| {
        letters
            .replace(&digits.replace(item, "1"), "a")
            .trim()
            .to_owned()
    };
    let autolink =
        cached_regex!(r"^<(?:[A-Za-z][A-Za-z0-9+.-]*:[^<>\s]*|[^<>\s@]+@[^<>\s]+)>").unwrap();
    let div_re = cached_regex!(r"^(:{3,})(?:[ \t]+\S.*)?[ \t]*$").unwrap();
    let close_div = cached_regex!(r"^:{3,}[ \t]*$").unwrap();
    let quote_re = cached_regex!(r"^[ \t]*>(?:[ \t]|$)").unwrap();
    let separator = cached_regex!(r"^\|[ \t:|-]*\|$").unwrap();
    let fence_open = cached_regex!(r"^(`{3,}|~{3,})[ \t]*=?[a-zA-Z0-9_+#.-]*$").unwrap();
    let boundary =
        cached_regex!(r"^(?:#{1,6} |:{3,}(?:[ \t]*[A-Za-z0-9_-]+)?[ \t]*$|\[[^\]]+\]:)").unwrap();
    let image_paragraph_valid = source
        .contains("![")
        .then(|| ImageBoundaryIndex::new(source, &mask));
    let mut offset = 0;
    let mut cursor = 0;
    let mut consumed = 0;
    let mut item_column = 0;
    let mut item_marker_column = 0;
    let mut item_quote = 0;
    let mut previous_quote = 0;
    let mut item_kind = String::new();
    let mut code_host_depth = 0;
    let mut previous_block = true;
    let mut fenced: Option<(usize, u8, usize, usize, usize)> = None;
    let mut divs: Vec<(usize, usize, usize, usize)> = Vec::new();
    let mut output = String::new();
    for line in source.split('\n') {
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
        let line_end = offset + line.len();
        let (parsed_depth, content) = quoted(line);
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
            code_prefix.push_str(&" ".repeat(if item_prefix.as_str().starts_with("[^") {
                2
            } else {
                item_prefix.len()
            }));
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
        let trimmed = content.trim_start_matches([' ', '\t']);
        let indent = content.len() - trimmed.len();
        let item = marker.find(trimmed).filter(|item| {
            !item.as_str().starts_with("[^")
                || previous_block
                || parsed_depth < previous_quote
                || (item_column > 0 && parsed_depth == item_quote && indent < item_column)
        });
        let old_column = item_column;
        let old_quote = item_quote;
        let began_inside = offset < consumed;
        let depth = if began_inside {
            code_host_depth
        } else {
            parsed_depth
        };
        if began_inside {
            code_depth = code_host_depth;
        } else if !previous_block
            && parsed_depth < previous_quote
            && !trimmed.is_empty()
            && !block.is_match(trimmed)
        {
            code_depth = code_depth.max(previous_quote);
        }
        if !began_inside {
            if fenced.is_some_and(|(_, _, owner_depth, _, owner_item)| {
                !trimmed.is_empty()
                    && (depth < owner_depth || (depth == owner_depth && indent < owner_item))
            }) {
                fenced = None;
            }
            if let Some((width, ch, owner_depth, column, _)) = fenced {
                let run = trimmed.bytes().take_while(|&b| b == ch).count();
                if depth == owner_depth
                    && indent <= column
                    && run >= width
                    && trimmed[run..].trim_matches([' ', '\t']).is_empty()
                {
                    fenced = None;
                    previous_block = true;
                }
                offset = line_end + 1;
                continue;
            }
            if let Some(opening) = fence_open
                .captures(&line[at..])
                .filter(|_| previous_block || item.is_some())
            {
                if item.is_none()
                    && (depth < item_quote || (depth == item_quote && indent < item_column))
                {
                    item_column = indent;
                    item_quote = depth;
                    item_kind.clear();
                }
                fenced = Some((
                    opening[1].len(),
                    opening[1].as_bytes()[0],
                    depth,
                    at,
                    item.map_or(item_column.min(indent), |item| {
                        indent
                            + if item.as_str().starts_with("[^") {
                                2
                            } else {
                                item.len()
                            }
                    }),
                ));
                offset = line_end + 1;
                continue;
            }
            if let Some(item) = item {
                item_marker_column = indent;
                item_column = indent
                    + if item.as_str().starts_with("[^") {
                        2
                    } else {
                        item.len()
                    };
                item_quote = depth;
                item_kind = item_kind_of(item.as_str());
            } else if !trimmed.is_empty() && item_column > 0 {
                let mut owner_content = line;
                for _ in 0..item_quote {
                    let Some(prefix) = quote_re.find(owner_content) else {
                        break;
                    };
                    owner_content = &owner_content[prefix.end()..];
                }
                let owner_text = owner_content.trim_start_matches([' ', '\t']);
                let owner_indent = owner_content.len() - owner_text.len();
                if depth < item_quote
                    || (owner_indent < item_column
                        && (previous_block
                            || block.is_match(owner_text)
                            || block_attributes(owner_text)))
                {
                    item_column = 0;
                    item_kind.clear();
                }
            }
            if let Some(div) = div_re.captures(&line[at..]) {
                if divs.last().is_some_and(|&(width, owner_depth, _, _)| {
                    depth == owner_depth && close_div.is_match(&line[at..]) && div[1].len() >= width
                }) {
                    divs.pop();
                } else if mask[offset + at] != b' ' {
                    divs.push((div[1].len(), depth, at, offset));
                }
            }
            if line.as_bytes().get(at) == Some(&b'|')
                && ((old_column > 0 && depth <= old_quote && indent < old_column)
                    || depth < previous_quote)
            {
                output.push_str(&source[cursor..offset]);
                let mut quote_end = 0;
                for _ in 0..depth {
                    if let Some(prefix) = quote_re.find(&line[quote_end..]) {
                        quote_end += prefix.end();
                    }
                }
                output.push_str(line[..quote_end].trim_end_matches([' ', '\t']));
                output.push('\n');
                cursor = offset;
            }
            previous_quote = depth.max(code_depth);
        }
        let mut i = (offset + at).max(consumed);
        let mut pending = Vec::new();
        let mut footnotes = Vec::new();
        while i < line_end {
            if mask[i] == b' ' && bytes[i] != b' ' {
                i += 1;
                continue;
            }
            if bytes[i] == b'\\' && bytes.get(i + 1).is_some_and(u8::is_ascii_punctuation) {
                i += 2;
                continue;
            }
            if bytes[i] == b'{' {
                if let Some((end, _)) = read_djot_word_attributes(source, i) {
                    i = end;
                    continue;
                }
            }
            if bytes[i] == b'<' {
                if let Some(auto) = autolink.find(&source[i..]) {
                    i += auto.len();
                    continue;
                }
            }
            if bytes[i..].starts_with(b"![") {
                if let Some(&end) = labels
                    .get(&(i + 1))
                    .filter(|&&end| balanced_ticks(i + 2, end))
                {
                    if end >= line_end
                        && matches!(bytes.get(end + 1), Some(b'(' | b'['))
                        && image_paragraph_valid.as_ref().is_some_and(|index| {
                            index.valid(
                                i,
                                end,
                                code_depth,
                                item_column,
                                item_quote,
                                divs.last().map(|owner| owner.3),
                            )
                        })
                    {
                        consumed = end + 1;
                        code_host_depth = code_depth;
                    }
                    i = end + 1;
                    continue;
                }
            }
            if bytes[i..].starts_with(b"[^") {
                let mut end = i + 2;
                while end < line_end && !matches!(bytes[end], b'[' | b']') {
                    end += 1;
                }
                if bytes.get(end) == Some(&b']') && balanced_ticks(i + 2, end) {
                    i = end + 1;
                    continue;
                }
                footnotes.push(i);
            }
            if bytes[i..].starts_with(b"](") {
                if let Some(&end) = parens
                    .get(&(i + 1))
                    .filter(|&&end| end <= line_end && balanced_ticks(i + 2, end))
                {
                    i = end + 1;
                    continue;
                }
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
                i += source[i..].chars().next().unwrap().len_utf8();
                continue;
            }
            let width = bytes[i..].iter().take_while(|&&b| b == b'`').count();
            code_host_depth = code_depth;
            let candidate = close_run(i, width);
            if only_closed && candidate >= bytes.len() {
                i += width;
                continue;
            }
            if only_closed
                && (marker_has_tab
                    || (i > 0 && bytes[i - 1] == b'$' && !is_escaped(bytes, i - 1))
                    || bytes.get(candidate + width) == Some(&b'{'))
            {
                i = candidate + width;
                consumed = i;
                continue;
            }
            if candidate + width <= line_end {
                i = candidate + width;
                consumed = i;
                continue;
            }
            let mut limit = candidate.min(bytes.len());
            let mut separate = false;
            let mut separate_depth = 0;
            let mut next = line_end + 1;
            while next < limit {
                let newline = source[next..].find('\n').map(|at| next + at);
                let end = newline.unwrap_or(bytes.len());
                let following = &source[next..end];
                let (next_depth, next_content) = quoted(following);
                let next_trimmed = next_content.trim_start_matches([' ', '\t']);
                let mut owner_content = following;
                for _ in 0..item_quote {
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
                let closes_div = divs.last().is_some_and(|&(width, owner_depth, column, _)| {
                    next_depth == owner_depth
                        && next_indent <= column + 3
                        && close_div.is_match(next_trimmed)
                        && next_trimmed.trim().len() >= width
                });
                let mut owner_content = &source[next..end];
                for _ in 0..item_quote {
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
                        item_marker_column + 1
                    };
                let outside_item = item_column > 0 && owner_indent < scope_edge;
                let outside = (code_depth > 0 && next_depth < code_depth) || outside_item;
                let attributes = block_attributes(next_trimmed);
                if closes_div
                    || (outside
                        && (block.is_match(next_trimmed)
                            || attributes
                            || (outside_item
                                && block.is_match(owner_content.trim_start_matches([' ', '\t'])))))
                {
                    limit = next - 1;
                    let next_item = marker
                        .find(next_trimmed)
                        .map(|item| item_kind_of(item.as_str()));
                    separate = outside
                        && (next_depth < depth || next_item.as_deref() != Some(item_kind.as_str()));
                    separate_depth = depth.min(next_depth);
                    break;
                }
                let Some(newline) = newline else {
                    break;
                };
                next = newline + 1;
            }
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
            let normalized: Vec<String> = payload
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
                .collect();
            let raw_code = normalized.iter().skip(1).any(|value| {
                (block.is_match(value) || value.starts_with(['{', '>']) || value.starts_with(":::"))
                    && (!separator.is_match(value) || line.as_bytes().get(at) != Some(&b'|'))
            }) || (item_kind.starts_with("[^") && normalized.len() > 1);
            let quoted_payload = (code_depth > 0 && !raw_code).then(|| {
                normalized
                    .iter()
                    .enumerate()
                    .map(|(line, value)| {
                        if line == 0 {
                            value.clone()
                        } else {
                            format!("{code_prefix}{value}")
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            });
            if let Some(quoted) = quoted_payload.as_deref() {
                payload = quoted;
            }
            let raw_payload = if raw_code {
                if let Some(callback) = on_raw_code.as_mut() {
                    callback(i);
                }
                let mut html = String::from("<code>");
                for ch in normalized.join("\n").chars() {
                    if ch.is_ascii_punctuation() || ch == '\n' {
                        html.push_str(&format!("&#{};", ch as u32));
                    } else {
                        html.push(ch);
                    }
                }
                html.push_str("</code>");
                html
            } else if code_depth > 0 {
                normalized.join("\n")
            } else {
                String::new()
            };
            if raw_code || code_depth > 0 {
                payload = &raw_payload;
            }
            let fence_width = ticks
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
                output.push_str(&source[cursor..escape]);
                output.push('\\');
                cursor = escape;
            }
            output.push_str(&source[cursor..i]);
            output.push_str(&format!(
                "{fence}{pad}{payload}{pad}{fence}{}",
                if raw_code { "{=html}" } else { "" }
            ));
            cursor = if closed { end + width } else { end };
            consumed = cursor;
            if separate && !closed {
                output.push('\n');
                output.push_str("> ".repeat(separate_depth).trim_end());
            }
            i = consumed;
        }
        previous_block = !began_inside
            && consumed <= line_end
            && (trimmed.is_empty()
                || boundary.is_match(trimmed)
                || block_attributes(trimmed)
                || (line.as_bytes().get(at) == Some(&b'|')
                    && line.trim_end().ends_with('|')
                    && !line.trim_end().ends_with("\\|")));
        offset = line_end + 1;
    }
    output.push_str(&source[cursor..]);
    output
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

pub(crate) struct DjotFootnoteAttributeStrip {
    pub source: String,
    pub losses: Vec<usize>,
    comments: HashSet<usize>,
}

impl DjotFootnoteAttributeStrip {
    fn is_boundary(&self, line: &str) -> bool {
        cached_regex!(r"\x00DJOTNOTEATTR\x00(\d+)\x00$")
            .unwrap()
            .captures(line.trim_end())
            .and_then(|caps| caps[1].parse::<usize>().ok())
            .is_some_and(|id| self.comments.contains(&id))
    }

    pub fn restore(&self, text: &str) -> String {
        cached_regex!(r"\x00DJOTNOTEATTR\x00(\d+)\x00")
            .unwrap()
            .replace_all(text, |caps: &regex::Captures<'_>| {
                if caps[1]
                    .parse::<usize>()
                    .ok()
                    .is_some_and(|id| self.comments.contains(&id))
                {
                    "%%".to_owned()
                } else {
                    caps[0].to_owned()
                }
            })
            .into_owned()
    }
}

pub(crate) fn strip_footnote_definition_attributes(input: &str) -> DjotFootnoteAttributeStrip {
    let source = input.replace("\r\n", "\n").replace('\r', "\n");
    if !source.contains('{') || !source.contains("[^") {
        return DjotFootnoteAttributeStrip {
            source,
            losses: Vec::new(),
            comments: HashSet::new(),
        };
    }
    let (frontmatter, separator, body) = split_frontmatter(&source);
    let header = if frontmatter.is_empty() {
        String::new()
    } else {
        format!("{frontmatter}{separator}")
    };
    let header_lines = header.bytes().filter(|ch| *ch == b'\n').count();
    let mut fence_lines = HashSet::new();
    mask_djot_fences(
        body,
        Some(&mut |line, _| {
            fence_lines.insert(line);
        }),
        &[],
        true,
    );
    let mask = mask_djot_inline(body, false);
    let original: Vec<&str> = body.split('\n').collect();
    let mut lines: Vec<String> = original.iter().map(|line| (*line).to_owned()).collect();
    let mut losses = Vec::new();
    let reserved: HashSet<usize> = cached_regex!(r"\x00DJOTNOTEATTR\x00(\d+)\x00")
        .unwrap()
        .captures_iter(&source)
        .filter_map(|caps| caps[1].parse().ok())
        .collect();
    let mut comments = HashSet::new();
    let mut serial = 0usize;
    let mut offset = 0;
    let mut boundary = true;
    struct Group {
        line: usize,
        end: usize,
        start: usize,
        wire: String,
    }
    let mut pending: Vec<Group> = Vec::new();
    let mut quote_depth = 0;
    let mut list_column = None;
    let mut list_quote_depth = 0;
    let mut consumed_until = 0;
    let mut heading = false;
    let mut table = false;
    let mut dedent: Option<(usize, usize, usize)> = None;
    let mut note_parents = Vec::new();
    let mut metadata_note = false;
    let mut note_column = None;
    let mut reference_column = None;
    let mut div_widths = Vec::new();
    let prefix_pattern = cached_regex!(r"^(?:[ \t]*>[ \t]?|[ \t]*(?:[-*+]|(?:[0-9]+|[A-Za-z]|[ivxlcdm]+|[IVXLCDM]+)[.)]|\((?:[0-9]+|[A-Za-z]|[ivxlcdm]+|[IVXLCDM]+)\))[ \t]+)*[ \t]*").unwrap();
    let quote_pattern = cached_regex!(r"^(?:[ \t]*>[ \t]?)*").unwrap();
    let block_pattern_0 = cached_regex!(r"^\S+$").unwrap();
    let block_pattern_1 = cached_regex!(r"^\[\^[^\]\n]+\]:(?:[ \t]|$)").unwrap();
    let block_pattern_2 = cached_regex!(r"^(?:[ \t]*>[ \t]?)*[ \t]*").unwrap();
    let block_pattern_3 =
        cached_regex!(r"^\[(?:[^\^\]\n][^\]\n]*)?\]:(?:[ \t]+\S*[ \t]*|)$").unwrap();
    let block_pattern_4 = cached_regex!(r"^#{1,6}(?:[ \t]|$)").unwrap();
    let block_pattern_5 = cached_regex!(r"^(:{3,})(?:[ \t].*)?$").unwrap();
    let block_pattern_6 = cached_regex!(r"^(?:[-*][ \t]*){3,}$").unwrap();
    for (n, &line) in original.iter().enumerate() {
        if offset < consumed_until {
            offset += line.len() + 1;
            continue;
        }
        let prefix = prefix_pattern.find(line).unwrap().as_str();
        let content = line[prefix.len()..].trim_end_matches([' ', '\t']);
        let depth = prefix.bytes().filter(|ch| *ch == b'>').count();
        let marker = prefix.bytes().any(|ch| b"-*+.)".contains(&ch));
        let mut column = quote_pattern.replace(prefix, "").len();
        if mask.as_bytes().get(offset + prefix.len()) != Some(&b' ')
            && prefix.contains('\t')
            && (boundary || !pending.is_empty() || list_column.is_some())
        {
            if let Some(col) =
                list_column.filter(|col| !marker && depth == list_quote_depth && column + 1 == *col)
            {
                column = col;
                let quotes = quote_pattern.find(prefix).unwrap().as_str();
                lines[n] = format!(
                    "{}{}{}",
                    quotes.replace('\t', " "),
                    " ".repeat(column),
                    &line[prefix.len()..]
                );
            } else {
                lines[n] = format!("{}{}", prefix.replace('\t', " "), &line[prefix.len()..]);
            }
        }
        if let Some((dedent_column, delta, dedent_depth)) = dedent {
            let quotes = quote_pattern.find(line).unwrap().as_str();
            let indent = line[quotes.len()..]
                .bytes()
                .take_while(|ch| *ch == b' ' || *ch == b'\t')
                .count();
            if !content.is_empty() && (depth != dedent_depth || indent < dedent_column) {
                dedent = None;
            } else if !content.is_empty() {
                lines[n] = format!("{}{}", quotes, &line[quotes.len() + delta..]);
            }
        }
        let mut exited_note = false;
        while note_column.is_some_and(|col| !content.is_empty() && column + 1 < col) {
            exited_note = true;
            if metadata_note
                && list_column.is_some_and(|col| column >= col)
                && n > 0
                && lines[n - 1].bytes().all(|ch| ch == b' ' || ch == b'\t')
            {
                while reserved.contains(&serial) {
                    serial += 1;
                }
                comments.insert(serial);
                lines[n - 1] = format!(
                    "{}\0DJOTNOTEATTR\x00{serial}\0",
                    " ".repeat(list_column.unwrap())
                );
                serial += 1;
            }
            note_column = note_parents.pop();
            metadata_note = false;
            boundary = true;
        }
        if note_column.is_some_and(|col| !content.is_empty() && column + 1 == col) {
            let raw = lines[n].clone();
            let quotes = quote_pattern.find(&raw).unwrap().as_str();
            lines[n] = format!("{} {}", quotes, &raw[quotes.len()..]);
            column += 1;
        }
        let parent_note_column = note_column;
        if depth < quote_depth {
            boundary = true;
            heading = false;
        }
        if list_column.is_some_and(|col| {
            depth != list_quote_depth || !content.is_empty() && !marker && column < col
        }) {
            list_column = None;
            boundary = true;
            heading = false;
        }
        let opens_item = marker && (boundary || !pending.is_empty() || list_column.is_some());
        let opens_quote = depth > quote_depth && (boundary || !pending.is_empty() || opens_item);
        if opens_item || opens_quote {
            pending.clear();
            heading = false;
        }
        if opens_item {
            list_column = Some(column);
            list_quote_depth = depth;
        }
        if opens_quote || depth < quote_depth {
            quote_depth = depth;
        }
        if let Some(reference) = reference_column {
            if !content.is_empty()
                && column > reference
                && !marker
                && block_pattern_0.is_match(content)
            {
                offset += line.len() + 1;
                boundary = false;
                pending.clear();
                continue;
            }
            reference_column = None;
            if !content.is_empty() {
                boundary = true;
            }
        }
        let block_allowed = boundary || !pending.is_empty() || opens_item || opens_quote;
        let mut handled_note = false;
        let attrs = if content.starts_with('{')
            && mask.as_bytes().get(offset + prefix.len()) == Some(&b'{')
        {
            read_djot_word_attributes(body, offset + prefix.len())
        } else {
            None
        };
        let standalone = attrs.as_ref().is_some_and(|(end, _)| {
            let trailing = body[*end..].find('\n').map_or(body.len(), |at| end + at);
            body[*end..trailing]
                .bytes()
                .all(|ch| ch == b' ' || ch == b'\t')
        });
        if standalone && block_allowed {
            let (end, wire) = attrs.unwrap();
            pending.push(Group {
                line: n,
                end,
                start: offset,
                wire,
            });
            consumed_until = end;
        } else if !content.is_empty() {
            if !pending.is_empty()
                && block_pattern_1.is_match(content)
                && mask.as_bytes().get(offset + prefix.len()) == Some(&b'[')
            {
                handled_note = true;
                let quote_prefix = quote_pattern.find(prefix).unwrap().as_str();
                let target_column = list_column
                    .unwrap_or(0)
                    .max(parent_note_column.unwrap_or(0));
                let note_prefix = format!("{}{}", quote_prefix, " ".repeat(target_column));
                if column > target_column {
                    dedent = Some((column, column - target_column, depth));
                }
                lines[n] = format!("{}{}", note_prefix, &line[prefix.len()..]);
                for group in &pending {
                    if group.wire != "{}" {
                        losses.push(header_lines + group.line + 1);
                    }
                    let mut at = group.line;
                    let mut position = group.start;
                    while at < n && position < group.end {
                        let raw = lines[at].clone();
                        let lead = if at == group.line {
                            &raw[..raw.find('{').unwrap()]
                        } else {
                            block_pattern_2.find(&raw).unwrap().as_str()
                        };
                        while reserved.contains(&serial) {
                            serial += 1;
                        }
                        comments.insert(serial);
                        lines[at] = format!("{lead}\0DJOTNOTEATTR\x00{serial}\0");
                        serial += 1;
                        position += raw.len() + 1;
                        at += 1;
                    }
                }
            }
            if !content.is_empty() {
                pending.clear();
            }
        }
        let reference = block_allowed
            && mask.as_bytes().get(offset + prefix.len()) == Some(&b'[')
            && block_pattern_3.is_match(content);
        if reference {
            reference_column = Some(column);
            heading = false;
        }
        let note = block_pattern_1.is_match(content);
        if note && block_allowed && mask.as_bytes().get(offset + prefix.len()) == Some(&b'[') {
            if let Some(parent) = note_column {
                note_parents.push(parent);
            }
            note_column = Some(column + 2);
            metadata_note = handled_note;
            heading = false;
        }
        if standalone || fence_lines.contains(&n) || content.is_empty() {
            heading = false;
        } else if block_allowed && block_pattern_4.is_match(content) {
            heading = true;
        }
        let row = (block_allowed || table)
            && content.starts_with('|')
            && content.ends_with('|')
            && mask.as_bytes().get(offset + prefix.len()) == Some(&b'|')
            && !is_escaped(
                body.as_bytes(),
                offset + line.trim_end_matches([' ', '\t']).len() - 1,
            );
        table = row;
        let colon = block_pattern_5.captures(content);
        let mut div = false;
        if let Some(colon) = colon {
            let bare = content.bytes().all(|ch| ch == b':');
            if block_allowed || !div_widths.is_empty() && bare {
                div = true;
                let width = colon[1].len();
                if div_widths.last().is_some_and(|old| width >= *old) && bare {
                    div_widths.pop();
                } else {
                    div_widths.push(width);
                }
                heading = false;
            }
        }
        boundary = content.is_empty()
            || reference
            || fence_lines.contains(&n)
            || row
            || div
            || heading
            || block_allowed && block_pattern_6.is_match(content);
        if exited_note
            && !marker
            && content.starts_with('{')
            && read_djot_word_attributes(content, 0).is_some_and(|(end, _)| end == content.len())
            && n > 0
            && !lines[n - 1].trim().is_empty()
            && !empty_djot_footnote_task_attributes(&lines[n - 1])
        {
            lines[n] = format!("{prefix}\n{}", lines[n]);
        }
        offset += line.len() + 1;
    }
    DjotFootnoteAttributeStrip {
        source: header + &lines.join("\n"),
        losses,
        comments,
    }
}

#[cfg(test)]
mod destination_mask_tests {
    use super::mask_djot_forms;

    #[test]
    fn opaque_parentheses_keep_following_text_visible() {
        for target in ["u`)`z", "u{a=\")\"}x"] {
            let source = format!("[t]({target}) and ~x~ later");
            assert_eq!(
                mask_djot_forms(&source, true, false, None, &[]),
                format!("[t]{} and ~x~ later", " ".repeat(target.len() + 2))
            );
        }
    }

    #[test]
    fn quote_boundary_requires_space_or_line_ending() {
        for (heads, boundary) in [
            (&["># heading", ">***", ">{.c}", ">:::"][..], false),
            (&["> # heading", "> ***", "> {.c}", "> :::", ">"][..], true),
        ] {
            for head in heads {
                let source = format!("{head}\n[r]: /a`b");
                let mask = super::mask_djot_opaque(
                    &source,
                    super::OpaqueOptions {
                        unclosed_code: false,
                        ..super::OpaqueOptions::default()
                    },
                );
                let expected = if boundary {
                    source.replace("/a`b", "    ")
                } else {
                    source
                };
                assert_eq!(mask, expected.into_bytes(), "{head}");
            }
        }
    }

    #[test]
    fn list_hosted_block_boundaries_remain_opaque() {
        for head in [
            "- ",
            "1. ",
            ": ",
            "- [ ] ",
            "- # H",
            "- [a]: /x",
            "- {.c}",
            "- :::",
        ] {
            let source = format!("{head}\n[r]: /a`b\n[s]: /c`d");
            let mask = super::mask_djot_opaque(
                &source,
                super::OpaqueOptions {
                    unclosed_code: false,
                    ..super::OpaqueOptions::default()
                },
            );
            let expected = source
                .replace("/a`b", "    ")
                .replace("/c`d", "    ")
                .replace("/x", "  ");
            assert_eq!(mask, expected.into_bytes(), "{head}");
        }
    }

    #[test]
    fn definitions_do_not_join_code_across_paragraphs() {
        let source = "[r]: /x\n\na `b\n\nc` d";
        let mask = super::mask_djot_opaque(
            source,
            super::OpaqueOptions {
                unclosed_code: false,
                ..super::OpaqueOptions::default()
            },
        );
        assert_eq!(mask, source.replace("/x", "  ").into_bytes());
    }

    #[test]
    fn disabling_inline_forms_keeps_destination_visible() {
        let source = "[t](u~x~)";
        assert_eq!(mask_djot_forms(source, false, false, None, &[]), source);
    }
}

#[cfg(test)]
mod placeholder_namespace_tests {
    #[test]
    fn reserves_exact_namespaces_without_growing_the_prefix() {
        for base in [
            "\0DJOTSTRONG",
            "\0DJOTLITERAL\0",
            "\0DJOTORPHAN\0",
            "\0DJOTEMPTYTERM\0",
            "\0DJOTALT\0",
        ] {
            let cases = [
                (format!("{base}0\0{}1\0", &base[1..]), 2),
                (format!("{base}00\0"), 0),
                (format!("{base}{}", "\0".repeat(32768)), 0),
                (
                    (0..128).map(|n| format!("{base}{n}\0")).collect::<String>(),
                    128,
                ),
            ];
            for (source, namespace) in cases {
                assert_eq!(
                    super::djot_placeholder_prefix(&source, base),
                    format!("{base}{namespace}\0")
                );
            }
        }
    }
}
