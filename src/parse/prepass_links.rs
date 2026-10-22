use super::*;

pub(in crate::parse) fn parse_link_def_line(line: &str) -> Option<(&str, &str)> {
    parse_link_def_line_with_value(line).map(|(label, target, _)| (label, target))
}

pub(in crate::parse) fn parse_link_def_line_with_value(
    line: &str,
) -> Option<(&str, &str, LinkDef)> {
    let (label, tail) = line.strip_prefix('[').and_then(|s| s.split_once("]:"))?;
    let first = label.chars().next()?;
    if first == '@'
        || (first == '^' && (label.len() > 1 || tail.bytes().all(|b| matches!(b, b' ' | b'\t'))))
        || label.contains(']')
    {
        return None;
    }
    if tail.bytes().all(|b| matches!(b, b' ' | b'\t')) {
        return Some((
            label,
            tail,
            LinkDef {
                raw_label: None,
                href: String::new(),
                title: None,
                attrs: None,
                line: None,
            },
        ));
    }
    let target = tail.strip_prefix(' ')?;
    // THE PRODUCTION IS ANCHORED AT END OF LINE (PART 7, carve#911):
    //
    //     reference_definition = '[', reference_label, ']', ':', space,
    //                            link_destination, [link_title],
    //                            [space, attributes], newline ;
    //
    // What follows the destination and the optional title makes the production
    // FAIL, and the line is then an ordinary paragraph. This engine read the
    // tail as junk and ignored it, which nothing in the grammar authorized.
    //
    // The anchor lives HERE, in the shape test, rather than at the two places
    // that build the node - so paragraph interruption, the container scan and
    // the footnote-body scan all get the same answer from the same code. That
    // is the sweep carve#922 asks for: while the pattern ended in a
    // swallow-everything tail, a caller could test the RAW line and be right by
    // accident.
    let (href, title, attrs) = parse_link_def_target_parts(target)?;
    Some((
        label,
        target,
        LinkDef {
            raw_label: None,
            href,
            title,
            attrs,
            line: None,
        },
    ))
}

/// The closing quote of the title run `after_pad` opens, as a byte index.
///
/// A backslash escapes the next character, which is the rule `unescape_title`
/// reads, so `[y]: /u "a\"b"` keeps its escaped quote.
pub(in crate::parse) fn closing_title_quote(after_pad: &str) -> Option<usize> {
    let mut chars = after_pad.char_indices();
    let quote = match chars.next() {
        Some((_, q @ ('"' | '\''))) => q,
        _ => return None,
    };
    let mut escaped = false;
    for (idx, c) in chars {
        if escaped {
            escaped = false;
            continue;
        }
        if c == '\\' {
            escaped = true;
            continue;
        }
        if c == quote {
            return Some(idx);
        }
    }
    None
}

/// The parts of a definition's target, read in the production's own order:
/// `link_destination`, an optional `link_title`, an optional trailing
/// `attributes` block, and then the line ending.
///
/// The block is looked for AFTER the destination and the title rather than
/// split off the target first (carve#604, reverted by
/// markup-carve/carve-rs#1791). A scan walking the target from its start cannot
/// tell a brace or a quote in the DESTINATION from one opening the block, so
/// `[a]: /u{x} {.c}` and `[a]: it's {.c}` both fell through to prose.
///
pub(in crate::parse) fn parse_link_def_target_parts(
    target: &str,
) -> Option<(String, Option<String>, Option<Attrs>)> {
    // THE LINE ENDING IS `whitespace` - a space or a tab, the same terminal
    // `blank_line` takes (PART 1, carve#890). So `[a]: /u<SP>` is a definition
    // and `[a]: /u<NBSP>` is not: a no-break space, an en quad, a byte order
    // mark and a form feed are CONTENT under that ruling, and content after the
    // destination is what the anchor rejects.
    let target = trim_ascii_end(target.trim_start_matches(' '));
    // `link_destination` ends at the first whitespace, and that scan IS the
    // Unicode property (`unicode_url_char` is "non-whitespace, non-ASCII").
    let end = target
        .char_indices()
        .find(|(_, c)| c.is_whitespace())
        .map_or(target.len(), |(idx, _)| idx);
    // And the run still has to BE a destination: a parenthesis reaches one only
    // through `balanced_parens` or `destination_escape`, and the three escapes
    // resolve. A bare `[r]:` was never a definition and still is not.
    let href = link_destination_value(&target[..end])?;
    let mut rest = &target[end..];
    // `link_title = space, ('"' … '"' | "'" … "'")`, ONE space (carve#912).
    let mut title = None;
    if let Some(after_pad) = rest.strip_prefix(' ') {
        if let Some(close) = closing_title_quote(after_pad) {
            title = Some(unescape_title(&after_pad[1..close]));
            rest = &after_pad[close + 1..];
        }
    }
    // `[space, attributes]`, one space again. An invalid block is not
    // `attributes`, so it is leftover content and the anchor below rejects the
    // line (CARVE-P3-006).
    let mut attrs = None;
    if let Some(after_pad) = rest.strip_prefix(' ') {
        let block = trim_ascii_end(after_pad);
        if block.starts_with('{') && block.ends_with('}') && block.len() >= 2 {
            attrs = Some(parse_attrs(&block[1..block.len() - 1])?);
            rest = &after_pad[block.len()..];
        }
    }
    if !rest.chars().all(|c| c == ' ' || c == '\t') {
        return None;
    }
    Some((href, title, attrs))
}

/// A line with its blockquote prefixes removed, for CONTENT-COLUMN purposes.
///
/// Columns are measured inside the quote: `> - a` puts the item's content
/// column at 2 of the quoted content, not of the raw line - which carries the
/// `> ` and matches no marker, so the tracker saw no item at all and a
/// definition written at that column registered nothing while the item consumed
/// the line (carve#658).
pub(in crate::parse) fn without_blockquote_prefixes(line: &str) -> &str {
    let mut rest = line;
    while let Some(inner) = strip_blockquote_prefix(rest) {
        rest = inner;
    }
    rest
}

/// Does a `:` line here open a definition list's DESCRIPTION?
///
/// Only when a term opened the entry above it. A description line with no term
/// above it is not a description at all - it is paragraph text, and a
/// definition in it defines nothing (corpus
/// `216-a-description-line-needs-a-term-above-it`). Stripping the marker from
/// every `: ` line collects that one too, which is the opposite of what 216
/// pins.
///
/// The previous line decides it: a `::` term opens an entry and a further
/// description continues one.
/// Does the PREVIOUS line open a definition entry, read through its container?
///
/// The prefixes come off first, the way they do for the line being tested one
/// line down. Asking the raw line meant `> :: term` did not read as a term, so
/// the `:  ` marker below it was never stripped and a definition written there
/// was neither collected nor hoisted - PART 12 §10 puts it on the DOCUMENT
/// (markup-carve/carve#840). A div was the one container that worked, because
/// it adds no per-line prefix for this to hide behind.
pub(in crate::parse) fn opens_definition_entry(previous: &str) -> bool {
    opens_definition_entry_bare(strip_container_prefixes(previous, false).bare)
}

pub(in crate::parse) fn opens_definition_entry_bare(previous: &str) -> bool {
    let t = previous.trim_start_matches([' ', '\t']);
    let rest = match t.strip_prefix("::") {
        Some(after) if !after.starts_with(':') => after,
        Some(_) => return false,
        None => match t.strip_prefix(':') {
            Some(after) => after,
            None => return false,
        },
    };
    rest.starts_with(' ') || rest.starts_with('\t')
}

/// The description marker itself: `:` then whitespace, at the start of a line.
///
/// `::` is the TERM marker and a `:::` fence opener is a fence; both need
/// whitespace after a SINGLE colon and neither has it, so neither matches here.
pub(in crate::parse) fn strip_description_prefix(line: &str) -> Option<&str> {
    let trimmed = line.trim_start_matches([' ', '\t']);
    let rest = trimmed.strip_prefix(':')?;
    if !(rest.starts_with(' ') || rest.starts_with('\t')) {
        return None;
    }
    let content = rest.trim_start_matches([' ', '\t']);
    if content.is_empty() {
        return None;
    }
    Some(content)
}

pub(in crate::parse) fn strip_container_prefixes(
    mut line: &str,
    after_term: bool,
) -> StrippedContainerLine<'_> {
    let original = line;
    let mut needs_empty_list_content = false;
    let mut description = false;
    loop {
        let before = line;
        while let Some(rest) = strip_blockquote_prefix(line) {
            line = rest;
            needs_empty_list_content = false;
        }
        // Every list spelling carries definitions at its content column. The
        // marker dialect changes numbering only; it cannot make an otherwise
        // identical reference definition visible.
        if let Some(marker) = detect_list_marker_full(line) {
            line = marker.content;
            needs_empty_list_content = true;
        }
        // A definition list's DESCRIPTION marker opens entry content exactly as
        // a bullet does, so a definition written on that line is collected from
        // it (carve-rs#668, spec markup-carve/carve#801).
        if after_term {
            if let Some(content) = strip_description_prefix(line) {
                line = content;
                description = true;
                // The entry must survive the line's removal as an EMPTY
                // description, the same way an item does when its only content
                // was a definition - otherwise the marker is left behind as a
                // stray paragraph and the `dd` disappears entirely.
                needs_empty_list_content = true;
            }
        }
        if line.len() == before.len() {
            break;
        }
    }
    // Compute the structural prefix length from the BYTE OFFSET of `line`
    // within `original`, not by length subtraction. `marker_tail` trims trailing
    // ASCII whitespace off the END of the collected content, so `line` can be
    // shorter than its true offset; a length-difference cut would then land
    // inside a leading multibyte content char and panic (`- ́ ` repro). `line`
    // is always a subslice of `original` whose START pointer is preserved by an
    // end-trim, so pointer subtraction yields the correct, char-boundary length.
    let structural_len = line.as_ptr() as usize - original.as_ptr() as usize;
    StrippedContainerLine {
        structural: &original[..structural_len],
        bare: line,
        needs_empty_list_content,
        description,
    }
}

/// `strip_container_prefixes`, but a quote marker AT an item's content column
/// counts as a container prefix.
///
/// A block quote nested in a list item is indented to that column (`- a` /
/// `  > [r]: /u`), and the plain stripper only reads a marker at position 0 - so
/// the definition inside such a quote was never collected, while at TOP level
/// the same `> [r]: /u` is collected by every engine (carve-rs#588).
///
/// EXACTLY that column, never arbitrary indentation: a top-level
/// `    > [r]: /u` is indented text, not a quote, and stays uncollected.
pub(in crate::parse) fn strip_container_prefixes_at<'a>(
    line: &'a str,
    columns: &ContentColumns,
    after_term: bool,
) -> StrippedContainerLine<'a> {
    let mut cut = 0usize;
    // WHICH LEVEL the question is asked at, not just how deep the walk has got.
    // Each hop enters one more container, and the column a `>` must sit at is
    // that container's, measured in its own coordinates - so the frame the
    // walk consults advances with it. Asking the innermost frame every time
    // answered for the wrong depth as soon as a nested quote opened a list
    // (carve-rs#1096).
    let mut level = 0usize;
    loop {
        let mut inside = &line[cut..];
        while let Some(rest) = strip_blockquote_prefix(inside) {
            inside = rest;
            level += 1;
        }
        if let Some(marker) = detect_list_marker_full(inside) {
            inside = marker.content;
        }
        let content_col = columns.reached_by_at(level, leading_ws(inside));
        if content_col == 0
            || inside.len() <= content_col
            || !inside.as_bytes()[..content_col].iter().all(|b| *b == b' ')
            || inside.as_bytes()[content_col] != b'>'
        {
            break;
        }
        let next = (inside.as_ptr() as usize - line.as_ptr() as usize) + content_col;
        if next <= cut {
            break;
        }
        cut = next;
    }
    if cut > 0 {
        let inner = strip_container_prefixes(&line[cut..], after_term);
        let structural_len = inner.bare.as_ptr() as usize - line.as_ptr() as usize;
        return StrippedContainerLine {
            structural: &line[..structural_len],
            bare: inner.bare,
            needs_empty_list_content: inner.needs_empty_list_content,
            description: inner.description,
        };
    }
    strip_container_prefixes(line, after_term)
}

// A quote-only prefix leaves the definition at its quote document root.
pub(in crate::parse) fn definition_at_quote_document_column(mut prefix: &str, bare: &str) -> bool {
    if leading_ws(bare) != 0 {
        return false;
    }
    if prefix.is_empty() {
        return true;
    }
    let mut quoted = false;
    while let Some(rest) = strip_blockquote_prefix(trim_ascii_start(prefix)) {
        prefix = rest;
        quoted = true;
    }
    quoted && prefix.is_empty()
}

// A blank permits a below-column line to leave the item. A continuation at
// the authored column remains opaque CODE, including its definitions.
pub(in crate::parse) fn description_fence_leaves_after_blank(
    line: &str,
    open: FenceOpen,
    quote_columns: &[usize],
) -> bool {
    strip_term_quote_prefixes(line, quote_columns).is_some_and(|content| {
        !is_blank_line(content) && indent_columns(content) < open.content_col
    })
}

pub(in crate::parse) fn description_fence_content_column(
    raw: &str,
    prefix: &StrippedContainerLine<'_>,
    columns: &ContentColumns,
) -> usize {
    let quote_columns = term_quote_columns_for(prefix.structural, columns);
    let owned = strip_term_quote_prefixes(raw, &quote_columns).unwrap_or(raw);
    let before = prefix.bare.as_ptr() as usize - owned.as_ptr() as usize;
    owned[..before].chars().count() + leading_ws(prefix.bare)
}

pub(in crate::parse) fn strip_container_prefixes_keep_indent(mut line: &str) -> String {
    let mut out = String::new();
    loop {
        let before = line;
        while let Some(rest) = strip_blockquote_prefix(line) {
            line = rest;
        }
        if let Some(marker) = detect_list_marker_full(line) {
            let marker_width = marker.content.as_ptr() as usize - line.as_ptr() as usize;
            out.extend(std::iter::repeat(' ').take(marker_width));
            line = marker.content;
        }
        if line.len() == before.len() {
            break;
        }
    }
    out.push_str(line);
    out
}

// Counts every `strip_blockquote_prefix` call on the current thread.
//
// The blockquote prefix is the one operation this parser can be made to
// repeat superlinearly (markup-carve/carve-rs#731), so the regression guard
// counts calls rather than reading a clock: a counted curve is independent of
// machine load, and this repo's own perf tests record that a ratio bound
// "flaked on nearly every run". Test-only, so a release build carries nothing.
#[cfg(test)]
thread_local! {
    pub(crate) static QUOTE_PREFIX_CALLS: std::cell::Cell<u64> =
        const { std::cell::Cell::new(0) };
}

/// `strip_quote_levels` for quotes each written after a fixed indentation.
pub(in crate::parse) fn strip_indented_quote_levels<'a>(
    mut line: &'a str,
    indents: &[usize],
) -> Option<&'a str> {
    for &indent in indents {
        if leading_ws(line) < indent {
            return None;
        }
        line = strip_blockquote_prefix(trim_ascii_start(&line[indent..]))?;
    }
    Some(line)
}

pub(in crate::parse) fn strip_quote_levels(mut line: &str, depth: usize) -> Option<&str> {
    for _ in 0..depth {
        line = strip_blockquote_prefix(line)?;
    }
    Some(line)
}

pub(in crate::parse) fn strip_blockquote_prefix(line: &str) -> Option<&str> {
    #[cfg(test)]
    QUOTE_PREFIX_CALLS.with(|c| c.set(c.get() + 1));
    let rest = line.strip_prefix('>')?;
    if rest.is_empty() {
        return Some(rest);
    }
    rest.strip_prefix(' ')
}

/// Hoist every authored `[label]: /url` definition into the document as a
/// `LinkReferenceDefinition` node (PART 12 §10, NORMATIVE).
pub(in crate::parse) fn append_link_reference_definitions(
    doc: &mut Document,
    link_defs: &BTreeMap<String, LinkDef>,
    source: &str,
    line_starts: Option<&CodepointLineStarts>,
) {
    // Skip the source-line table when there are no definitions or positions.
    if link_defs.is_empty() {
        return;
    }
    let lines = line_starts.map(|_| source.split('\n').collect::<Vec<_>>());
    let mut authored: Vec<(Option<usize>, LinkReferenceDefinition)> = Vec::new();
    for (label, def) in link_defs {
        let pos = match (line_starts, lines.as_ref(), def.line) {
            (Some(line_starts), Some(lines), Some(line)) => {
                let text = lines.get(line).copied().unwrap_or("");
                let start = line_starts.get(line).copied().unwrap_or(0);
                let bom_column = usize::from(line == 0 && line_starts.has_leading_bom());
                Some(Pos {
                    start_line: line + 1,
                    end_line: line + 1,
                    start_column: 1 + bom_column,
                    end_column: text.chars().count() + 1 + bom_column,
                    start_offset: start,
                    end_offset: start + text.chars().count(),
                    file: None,
                })
            }
            _ => None,
        };
        authored.push((
            def.line,
            LinkReferenceDefinition {
                label: def.raw_label.clone().unwrap_or_else(|| label.clone()),
                href: def.href.clone(),
                title: def.title.clone(),
                attrs: def.attrs.clone(),
                pos,
            },
        ));
    }
    // A definition with no line sorts last, and keeps the map's label order among
    // its peers - there is nothing better to order it by.
    authored.sort_by_key(|(line, _)| line.unwrap_or(usize::MAX));
    doc.children.extend(
        authored
            .into_iter()
            .map(|(_, node)| BlockNode::LinkReferenceDefinition(node)),
    );
}
