use super::*;

pub(in crate::parse) fn parse_standalone_attrs(line: &str) -> Option<Attrs> {
    let trimmed = trim_ascii(line);
    if !trimmed.starts_with('{') {
        return None;
    }
    let bytes = trimmed.as_bytes();
    let last_close_brace = bytes.iter().rposition(|&b| b == b'}');
    let mut pos = 0usize;
    let mut attrs: Option<Attrs> = None;
    let mut slots = None;
    while pos < bytes.len() {
        // The block-attribute LINE, so `whitespace` at all three of its slots.
        let (incoming, next) = read_attrs_at_with(bytes, pos, last_close_brace, false)?;
        merge_attrs_indexed(&mut attrs, incoming, &mut slots);
        pos = next;
        if pos < bytes.len() && bytes[pos] != b'{' {
            return None;
        }
    }
    attrs
}

#[cfg(test)]
thread_local! {
    pub(in crate::parse) static ATTRS_LOOKAHEAD_BYTES: Cell<usize> = const { Cell::new(0) };
}

pub(in crate::parse) fn valid_wrapped_attr_continuation(line: &str) -> bool {
    trim_ascii_end(line).ends_with('}') || parse_attrs_with(trim_ascii(line), false).is_some()
}

/// A standalone block-attribute block, possibly spanning several contiguous
/// (non-blank) lines: it opens with `{` and closes with `}` on a later line
/// (`{#id` / ` .foo}`). Consumes the lines and returns the parsed attributes,
/// or leaves the cursor untouched if it is not a valid attribute block.
pub(in crate::parse) fn parse_standalone_attrs_block(cur: &mut LineCursor) -> Option<Attrs> {
    // The OPENER is read RAW: a framed first line is a lazily-folded indented
    // `{...}`, which is paragraph text under the strict column-0 rule and must
    // NOT be pulled flush-left into a block. Only the CONTINUATION lines below
    // are unframed, so a wrapped block whose CLOSER folded down from a shallower
    // quote depth still joins (carve-rs#1538); the opener there arrives unframed
    // through the quote's own marker.
    let first = cur.peek()?;
    if !trim_ascii_start(first).starts_with('{') {
        return None;
    }
    if let Some(attrs) = parse_standalone_attrs(first) {
        cur.consume();
        return Some(attrs);
    }
    // See `standalone_attrs_block_len` for the cursor-free twin of everything
    // below; the two must keep answering the same way.
    // A COMPLETE single line (already closes with `}`) that parse_standalone_attrs
    // rejected is not a valid attribute block -- do NOT rescue it via the
    // multi-line strip-outer path below, which would parse an interior `}{` as an
    // unquoted value (`{k=v}{+i+}` -> k="v}{+i+", swallowing the whole line). The
    // multi-line join is only for a block that genuinely continues onto later
    // lines (`{#id` then `.foo}`), i.e. whose first line does not itself close.
    // Matches carve-js, which keeps such a line literal.
    if trim_ascii_end(first).ends_with('}') {
        return None;
    }
    // Multi-line: join contiguous lines until one closes with `}`.
    let mut joined = String::new();
    let mut count = 0usize;
    let mut quote: Option<char> = None;
    while let Some(raw) = cur.lines.line(cur.pos + count) {
        let line = strip_lazy(raw);
        if is_blank_line(line) {
            return None;
        }
        if count > 0 && !valid_wrapped_attr_continuation(line) {
            return None;
        }
        if !joined.is_empty() {
            if quote.is_some() {
                return None;
            }
            joined.push(' ');
        }
        #[cfg(test)]
        ATTRS_LOOKAHEAD_BYTES.with(|work| work.set(work.get() + line.len()));
        joined.push_str(trim_ascii(line));
        quote = open_quote_at_end(trim_ascii(line));
        count += 1;
        if trim_ascii_end(line).ends_with('}') {
            let inner = trim_ascii(&joined);
            if inner.starts_with('{') && inner.ends_with('}') {
                if let Some(attrs) = parse_attrs_with(&inner[1..inner.len() - 1], false) {
                    for _ in 0..count {
                        cur.consume();
                    }
                    return Some(attrs);
                }
            }
            return None;
        }
    }
    None
}

/// How many LINES a standalone attribute block occupies when one starts at
/// `lines[0]`, or `None` when these lines do not spell one.
///
/// The cursor-free twin of `parse_standalone_attrs_block`, for the predicates
/// that must ask "does an attribute block sit here" without a cursor to consume
/// from. Both follow the same rules: a single complete line, or a run joined
/// until one closes with `}`, refused at a blank line and refused when a quoted
/// value is left open across a break (PART 4, markup-carve/carve#888).
///
/// FLUSH-LEFT ONLY, like every caller's own guard: an indented `{...}` is lazy
/// paragraph text under the strict column-0 rule (PART 9 section 24 C3).
pub(in crate::parse) fn standalone_attrs_block_len(lines: &[&str]) -> Option<usize> {
    standalone_attrs_block_len_iter(lines.iter().copied())
}

pub(in crate::parse) fn standalone_attrs_block_len_iter<'a>(
    mut lines: impl Iterator<Item = &'a str>,
) -> Option<usize> {
    // OPENER read RAW (see `parse_standalone_attrs_block`): a framed first line
    // is lazily-folded indented text, not a block opener. Only the CONTINUATION
    // lines in the join loop below are unframed.
    let first = lines.next()?;
    if first.starts_with([' ', '\t']) {
        return None;
    }
    // AN ATTRIBUTE BLOCK OPENS WITH A BRACE, and refusing everything else here
    // is what keeps this cheap. `interrupts_paragraph` asks this question of
    // EVERY continuation line, and without the guard a paragraph whose lines
    // never end in `}` sends the join loop below over the whole remainder once
    // per line - quadratic time and allocation on ordinary prose that has no
    // attributes in it at all.
    if !trim_ascii(first).starts_with('{') {
        return None;
    }
    if parse_standalone_attrs(first).is_some() {
        return Some(1);
    }
    // Same refusal as the cursor form: a line that already closes and was
    // rejected is not rescued by the multi-line join.
    if trim_ascii_end(first).ends_with('}') {
        return None;
    }
    let mut joined = String::new();
    let mut quote: Option<char> = None;
    for (count, raw) in std::iter::once(first).chain(lines).enumerate() {
        let line = strip_lazy(raw);
        if is_blank_line(line) {
            return None;
        }
        if count > 0 && !valid_wrapped_attr_continuation(line) {
            return None;
        }
        if !joined.is_empty() {
            if quote.is_some() {
                return None;
            }
            joined.push(' ');
        }
        #[cfg(test)]
        ATTRS_LOOKAHEAD_BYTES.with(|work| work.set(work.get() + line.len()));
        joined.push_str(trim_ascii(line));
        quote = open_quote_at_end(trim_ascii(line));
        if trim_ascii_end(line).ends_with('}') {
            let inner = trim_ascii(&joined);
            if inner.starts_with('{')
                && inner.ends_with('}')
                && parse_attrs_with(&inner[1..inner.len() - 1], false).is_some()
            {
                return Some(count + 1);
            }
            return None;
        }
    }
    None
}

/// What a quoted line says about a standalone attribute block starting on it.
pub(in crate::parse) enum QuotedAttrsBlock {
    /// One starts here and spans this many QUOTED lines.
    Block(usize),
    /// None starts here, and none can start on any of the next `0..n` lines
    /// either: the scan walked that far without meeting a line that could CLOSE
    /// one. See [`quoted_attrs_block_len`] for why that generalizes.
    NoneWithin(usize),
    /// None starts here. A later line may still open one.
    No,
}

/// Whether a standalone attribute block starts on the quoted line `stripped`,
/// and how many QUOTED lines it takes.
///
/// `stripped` is the current line with ONE quote marker taken off, and `rest`
/// is the raw document from the next line on. Both are walked down to their
/// INNERMOST quoted content, exactly as [`ParaOpen::resolve`] does: a lazy line
/// continues the innermost open paragraph, so what an attribute block ends is
/// the innermost paragraph too, and a block written at depth 2 has to answer
/// like one written at depth 1 (markup-carve/carve#506).
///
/// THE BRACE GUARD COMES FIRST AND IS TESTED ON THE UNWALKED LINE. Every quoted
/// line reaches this, and the walk costs one strip per remaining quote marker -
/// which is precisely the per-line cost `ParaOpen` was built to defer
/// (markup-carve/carve-rs#731). A line with no `{` anywhere on it cannot open an
/// attribute block at any depth, so a quote ladder pays one `memchr` and stops.
///
/// AND THE SCAN REPORTS WHAT IT PROVED, so a run of brace-shaped lines that
/// close nothing is walked ONCE rather than once per line. A block ends at the
/// first line with a trailing `}`; if the walk reaches a blank line or the end
/// of the quoted run without meeting one, no line it passed can open a block
/// either, and `NoneWithin` hands the caller that window to skip. Without it a
/// quoted `{a` repeated n times pays an O(n) scan per line. Invalid continuation
/// fragments also end the window before a later opener is skipped.
pub(in crate::parse) fn quoted_attrs_block_len<'a>(
    stripped: &'a str,
    rest: impl Iterator<Item = &'a str>,
) -> QuotedAttrsBlock {
    if !stripped.contains('{') {
        return QuotedAttrsBlock::No;
    }
    fn innermost(mut line: &str) -> (&str, usize) {
        let mut depth = 0;
        while let Some(next) = strip_blockquote_prefix(line) {
            line = next;
            depth += 1;
        }
        (line, depth)
    }
    let (first, nested_depth) = innermost(stripped);
    if first.starts_with([' ', '\t']) || !first.starts_with('{') {
        return QuotedAttrsBlock::No;
    }
    // The single-line spelling is already answered by
    // `interrupts_paragraph_with_rest`, which `ParaOpen` consults. Only the
    // WRAPPED one needs the lookahead, and refusing a complete line here keeps
    // this from being a second answer to a question that already has one.
    if trim_ascii_end(first).ends_with('}') {
        return QuotedAttrsBlock::No;
    }
    let mut block: Vec<&str> = vec![first];
    let mut met_a_closer = false;
    for line in rest {
        // The block lives INSIDE the quote: an unprefixed line is the lazy
        // continuation the caller is deciding about, not part of the block.
        let Some(next) = strip_blockquote_prefix(line) else {
            break;
        };
        let mut inner = next;
        let mut matched_depth = true;
        for _ in 0..nested_depth {
            if let Some(rest) = strip_blockquote_prefix(inner) {
                inner = rest;
            } else {
                matched_depth = false;
                break;
            }
        }
        if !matched_depth || strip_blockquote_prefix(inner).is_some() {
            break;
        }
        #[cfg(test)]
        ATTRS_LOOKAHEAD_BYTES.with(|work| work.set(work.get() + inner.len()));
        if !is_blank_line(inner) && !valid_wrapped_attr_continuation(inner) {
            return QuotedAttrsBlock::NoneWithin(block.len());
        }
        block.push(inner);
        if is_blank_line(inner) {
            break;
        }
        if trim_ascii_end(inner).ends_with('}') {
            met_a_closer = true;
            break;
        }
    }
    match standalone_attrs_block_len(&block) {
        Some(len) => QuotedAttrsBlock::Block(len),
        // A closer was there and the join was still refused - a later line may
        // open a block that ends on that same closer, so nothing is proved.
        None if met_a_closer => QuotedAttrsBlock::No,
        None => QuotedAttrsBlock::NoneWithin(block.len()),
    }
}

/// The quote a run leaves OPEN at its end.
///
/// A backslash escapes the next character, so `\"` neither opens a value nor
/// closes one. Used to decide whether a line break falls inside a
/// `quoted_value`, which PART 4 forbids in both of its alternatives
/// (carve#888).
///
/// The scan starts from NO open quote every time, and deliberately takes no
/// incoming state: a line that ends inside a value refuses the block at the
/// very next break, so no later line is ever scanned from inside one. A
/// carried-in state would be a parameter that cannot take a second value -
/// dead by construction and untestable, which is the shape this repository has
/// had to remove repeatedly.
pub(in crate::parse) fn open_quote_at_end(s: &str) -> Option<char> {
    let mut quote = None;
    let mut escaped = false;
    for ch in s.chars() {
        if escaped {
            escaped = false;
            continue;
        }
        match quote {
            Some(q) => {
                if ch == '\\' {
                    escaped = true;
                } else if ch == q {
                    quote = None;
                }
            }
            None => match ch {
                '\\' => escaped = true,
                '"' | '\'' => quote = Some(ch),
                _ => {}
            },
        }
    }
    quote
}

/// Split a TRAILING standalone block-attribute block off a collected chunk,
/// returning its attributes and shortening the chunk to the lines before it.
pub(in crate::parse) fn split_trailing_attrs(
    nested: &mut MappedSource,
) -> Option<(Attrs, Option<Pos>)> {
    let lines: Vec<&str> = nested.source.lines().collect();
    if lines.is_empty() {
        return None;
    }
    // A trailing attribute block ends on the last line, so that line must close
    // one. Anything else is ordinary content and there is nothing to split.
    if !trim_ascii_end(lines[lines.len() - 1]).ends_with('}') {
        return None;
    }
    // Walk back to the line that OPENS the run. A blank ends the search: a
    // block-attribute block is contiguous.
    let mut start = lines.len() - 1;
    loop {
        let line = lines[start];
        if is_blank_line(line) {
            return None;
        }
        // FLUSH LEFT, the same guard `parse_blocks` puts on this call
        // (`line_flush`). The chunk is already dedented by the item's content
        // column, so flush here means AT that column - and a line PAST it is
        // ordinary text, which is what corpus 87-compact-list-blocks-10 pins:
        // `- a` / blank / three spaces / `{.c}` is a second paragraph reading
        // `{.c}`, not an attribute block. Trimming the indent away instead
        // deleted that paragraph and re-tightened the item.
        if line.starts_with('{') {
            break;
        }
        if line.starts_with([' ', '\t']) {
            return None;
        }
        if start == 0 {
            return None;
        }
        start -= 1;
    }
    let tail: Vec<&str> = lines[start..].to_vec();
    let mut probe = LineCursor::new_with_cols(&tail, None, None);
    let attrs = parse_standalone_attrs_block(&mut probe)?;
    // Only a run the reader consumed WHOLE is the chunk's trailing attribute
    // block. A partial consume means the lines after it are content, and
    // splitting there would silently delete them.
    if !probe.eof() {
        return None;
    }
    // WHERE THE RUN WAS WRITTEN, for §15 A4's diagnostic - taken BEFORE the
    // maps are truncated below, since after that the split-off lines have no
    // entries left to read. A set lifted off a chunk and never placed is
    // dropped when the list ends or when a sibling marker opens
    // (markup-carve/carve#1281), and the drop is a finding wherever it happens.
    let pos = mapped_span_of(nested, start, lines.len(), &lines);
    let line_count = lines.len();
    let kept: Vec<&str> = lines[..start].to_vec();
    let source = kept.join("\n");
    nested.source = source;
    // Both maps are read by LINE INDEX, and the kept lines keep the indices
    // they had, so trailing entries are simply never reached - trimming them is
    // tidiness, not correctness. It is only safe on a map that is index-aligned
    // with the chunk: `push_newline_at` skips a LEADING run of unmapped lines
    // rather than storing `None` for them, so a shorter `line_map` is offset
    // from the lines and cutting it at `start` would drop an entry belonging to
    // a line being KEPT. Leave such a map alone; its extra entries cost nothing.
    if nested.col_map.len() == line_count {
        nested.col_map.truncate(start);
    }
    if nested.line_map.len() == line_count {
        nested.line_map.truncate(start);
    }
    Some((attrs, pos))
}

/// The document span of `lines[start..end]` inside a collected chunk.
///
/// [`span_of`] answers this for a [`LineCursor`], which carries the same two
/// maps under different names; a chunk that has already been split off has no
/// cursor over it, so the same arithmetic is done here. `None` when the chunk
/// carries no line map - positions are opt-in, and a span nobody asked for is
/// not worth inventing.
pub(in crate::parse) fn mapped_span_of(
    nested: &MappedSource,
    start: usize,
    end: usize,
    lines: &[&str],
) -> Option<Pos> {
    let last = end.saturating_sub(1).max(start);
    let start_line = *nested.line_map.get(start)?;
    let start_line = start_line?;
    let end_line = nested
        .line_map
        .get(last)
        .copied()
        .flatten()
        .unwrap_or(start_line);
    let stripped = nested.col_map.get(start).copied().flatten().unwrap_or(0);
    let end_stripped = nested
        .col_map
        .get(last)
        .copied()
        .flatten()
        .unwrap_or(stripped);
    let indent = lines
        .get(start)
        .map(|l| l.chars().count() - trim_ascii_start(l).chars().count())
        .unwrap_or(0);
    let width = lines.get(last).map(|l| l.chars().count()).unwrap_or(0);
    Some(Pos {
        start_line,
        end_line,
        start_column: (stripped.max(0) as usize) + indent + 1,
        end_column: (end_stripped.max(0) as usize) + width + 1,
        start_offset: 0,
        end_offset: 0,
        file: None,
    })
}

pub(in crate::parse) fn merge_attrs(target: &mut Option<Attrs>, incoming: Attrs) {
    if target.is_none() {
        *target = Some(incoming);
        return;
    }
    merge_attrs_indexed(target, incoming, &mut None);
}

pub(in crate::parse) fn merge_attrs_indexed(
    target: &mut Option<Attrs>,
    incoming: Attrs,
    slots: &mut Option<HashSet<AttrSlot>>,
) {
    if target.is_none() {
        *slots = None;
        *target = Some(incoming);
        return;
    }
    let target = target.as_mut().unwrap();
    let slots = slots.get_or_insert_with(|| target.order.iter().cloned().collect());
    if incoming.id.is_some() {
        target.id = incoming.id;
    }
    target.classes.extend(incoming.classes);
    target.key_values.extend(incoming.key_values);
    for slot in incoming.order {
        if slots.insert(slot.clone()) {
            target.order.push(slot);
        }
    }
}

/// Merge a leading block-attribute line onto a node that may already carry
/// its own opener attributes. Leading attrs are earlier in source, so their
/// classes precede the opener's and the opener wins on id/key conflict (§15).
pub(crate) fn merge_leading_attrs(target: &mut Option<Attrs>, leading: Attrs) {
    match target.take() {
        None => *target = Some(leading),
        Some(own) => {
            *target = Some(leading);
            merge_attrs(target, own);
        }
    }
}

/// Add a `data-source-line` attribute to a block node, preserving any existing
/// attributes. No-op for blocks that carry no attributes (comment,
/// abbreviation definition).
pub(in crate::parse) fn stamp_source_line(node: &mut BlockNode, line: usize) {
    let slot: Option<&mut Option<Attrs>> = match node {
        // §15 A2b puts a definition's attributes on its own line, not from a
        // preceding block-attribute line, so there is no slot to fill here.
        BlockNode::LinkReferenceDefinition(_) => None,
        BlockNode::Heading(n) => Some(&mut n.attrs),
        BlockNode::Paragraph(n) => Some(&mut n.attrs),
        BlockNode::ThematicBreak(n) => Some(&mut n.attrs),
        BlockNode::CodeBlock(n) => Some(&mut n.attrs),
        BlockNode::RawBlock(n) => Some(&mut n.attrs),
        BlockNode::List(n) => Some(&mut n.attrs),
        BlockNode::BlockQuote(n) => Some(&mut n.attrs),
        BlockNode::Table(n) => Some(&mut n.attrs),
        BlockNode::Admonition(n) => Some(&mut n.attrs),
        BlockNode::Directive(n) => Some(&mut n.attrs),
        BlockNode::Div(n) => Some(&mut n.attrs),
        BlockNode::Section(n) => Some(&mut n.attrs),
        BlockNode::LineBlock(n) => Some(&mut n.attrs),
        BlockNode::DefinitionList(n) => Some(&mut n.attrs),
        BlockNode::Figure(n) => Some(&mut n.attrs),
        BlockNode::FigureGroup(n) => Some(&mut n.attrs),
        BlockNode::BlockExtension(n) => Some(&mut n.attrs),
        BlockNode::ExtensionCarrier(n) => Some(&mut n.attrs),
        BlockNode::BlockImage(n) => Some(&mut n.attrs),
        // A citation definition's attributes are the metadata block on its own
        // line, not a preceding block-attribute line - same as the link
        // reference definition above.
        BlockNode::AbbreviationDef(_)
        | BlockNode::CitationDefinition(_)
        | BlockNode::Comment(_) => None,
    };
    let Some(opt) = slot else {
        return;
    };
    let attrs = opt.get_or_insert_with(Attrs::default);
    stamp_source_line_attr(attrs, line);
}

pub(in crate::parse) fn source_line_attrs(
    mut attrs: Option<Attrs>,
    line: Option<usize>,
    options: &Options<'_>,
) -> Option<Attrs> {
    if options.source_lines {
        if let Some(line) = line {
            let attrs = attrs.get_or_insert_with(Attrs::default);
            stamp_source_line_attr(attrs, line);
        }
    }
    attrs
}

pub(in crate::parse) fn stamp_source_line_attr(attrs: &mut Attrs, line: usize) {
    let key = "data-source-line";
    if !attrs.key_values.contains_key(key) {
        attrs.key_values.insert(key.to_string(), line.to_string());
        attrs.order.push(AttrSlot::Key(key.to_string()));
    }
}

/// PART 9 §17 L7: the consumed `loose` boolean, taken off the attribute line.
///
/// Returns whether the key was SPELLED, and removes it, so it never reaches the
/// output as an HTML attribute. The precedent is PART 12 §15's `header-rows`:
/// a structural fact riding the block-attribute line, consumed rather than
/// emitted.
///
/// A BOOLEAN AND AN EMPTY VALUE ARE ONE KEY. PART 4 makes `{loose}` and
/// `{loose=""}` the same attribute, and both arrive here with an empty value,
/// so both are consumed. `loose=x` names a value this key does not take, so it
/// is NOT this key: it is left alone and renders `loose="x"` like any other
/// attribute. There is no error state and no spelling that half-applies.
pub(in crate::parse) fn consume_loose(attrs: &mut Attrs) -> bool {
    if attrs.key_values.get("loose").map(String::as_str) != Some("") {
        return false;
    }
    attrs.key_values.remove("loose");
    attrs
        .order
        .retain(|slot| !matches!(slot, AttrSlot::Key(key) if key == "loose"));
    true
}

pub(in crate::parse) fn attrs_are_empty(attrs: &Attrs) -> bool {
    attrs.id.is_none()
        && attrs.classes.is_empty()
        && attrs.key_values.is_empty()
        && attrs.order.is_empty()
}

pub(in crate::parse) fn apply_attrs_to_block(node: &mut BlockNode, attrs: Attrs) {
    let mut attrs = attrs;
    match node {
        BlockNode::Heading(n) => n.attrs = Some(attrs),
        BlockNode::Paragraph(n) => n.attrs = Some(attrs),
        BlockNode::ThematicBreak(n) => n.attrs = Some(attrs),
        BlockNode::CodeBlock(n) => n.attrs = Some(attrs),
        BlockNode::RawBlock(n) => n.attrs = Some(attrs),
        BlockNode::List(n) => {
            // §17 L7. The axis is TOTAL for a list, so the spelled fact lands in
            // the field that already states it and nothing else is published.
            // Redundant use is a legal no-op: a list the blank lines already
            // loosened is already `tight: false`.
            if consume_loose(&mut attrs) {
                n.tight = false;
                // A LINE THAT CARRIED ONLY THE KEY LEAVES NO ATTRIBUTES BEHIND.
                // Attaching the emptied set published `attrs: {}` on a node the
                // author gave no attributes, which is a tree the same document
                // written without the key does not produce - so `fmt` of a
                // one-item loose list stopped round-tripping to its own parse.
                if attrs_are_empty(&attrs) {
                    n.attrs = None;
                    return;
                }
            }
            n.attrs = Some(attrs);
        }
        BlockNode::BlockQuote(n) => n.attrs = Some(attrs),
        BlockNode::Table(n) => {
            n.columns = table_columns_from_attrs(&attrs);
            n.columns
                .truncate(n.rows.iter().map(|row| row.cells.len()).max().unwrap_or(0));
            n.row_groups =
                crate::table_source_metadata::row_groups(&attrs, n.rows.len()).map(Box::new);
            n.attrs = Some(attrs);
        }
        // A typed colon-fence opener may already carry its own attribute
        // block (`::: note {.x}`); a leading block-attribute line is earlier
        // in source, so its classes come first and the opener's win on
        // id/key conflict (§15) -- merge instead of clobbering.
        BlockNode::Admonition(n) => merge_leading_attrs(&mut n.attrs, attrs),
        BlockNode::Directive(n) => merge_leading_attrs(&mut n.attrs, attrs),
        BlockNode::Div(n) => merge_leading_attrs(&mut n.attrs, attrs),
        BlockNode::LineBlock(n) => merge_leading_attrs(&mut n.attrs, attrs),
        BlockNode::DefinitionList(n) => {
            // §17 L7. The axis is NOT total here: a `<dl>` derives a loose
            // description from its block count and has no blank-line spelling
            // for a one-block one at any entry count, so the spelled fact needs
            // a field of its own rather than landing in one that already states
            // it. Redundant use stays a legal no-op - a description already
            // holding two blocks renders the same either way.
            if consume_loose(&mut attrs) {
                n.loose = true;
                // The same emptied-set rule the list arm applies: a line that
                // carried only the key leaves no attributes behind, so `fmt`
                // round-trips to its own parse.
                if attrs_are_empty(&attrs) {
                    n.attrs = None;
                    return;
                }
            }
            n.attrs = Some(attrs);
        }
        BlockNode::Figure(n) => n.attrs = Some(attrs),
        // The bare opener carries nothing of its own (§4c), so the preceding
        // block-attribute line is the group's only attribute source.
        BlockNode::FigureGroup(n) => n.attrs = Some(attrs),
        BlockNode::ExtensionCarrier(n) => n.attrs = Some(attrs),
        // A direct block image (`{#id}\n![…](…)`) carries the leading attrs on
        // the `<img>` itself; the image's own inline attrs win on conflict (§15).
        BlockNode::BlockImage(img) => merge_leading_attrs(&mut img.attrs, attrs),
        _ => {}
    }
}

pub(in crate::parse) fn table_columns_from_attrs(attrs: &Attrs) -> Vec<TableColumn> {
    let split = |key: &str| {
        attrs
            .key_values
            .get(key)
            .map(|v| v.split(',').collect::<Vec<_>>())
            .unwrap_or_default()
    };
    let aligns = split("aligns");
    let valigns = split("valigns");
    let widths = split("widths");
    let len = aligns.len().max(valigns.len()).max(widths.len());
    (0..len)
        .map(|i| TableColumn {
            align: aligns.get(i).and_then(|v| match *v {
                "left" => Some(TableAlign::Left),
                "right" => Some(TableAlign::Right),
                "center" => Some(TableAlign::Center),
                _ => None,
            }),
            valign: valigns.get(i).and_then(|v| match *v {
                "top" => Some(TableVerticalAlign::Top),
                "middle" => Some(TableVerticalAlign::Middle),
                "bottom" => Some(TableVerticalAlign::Bottom),
                _ => None,
            }),
            width: widths.get(i).and_then(|v| crate::table_width::fraction(v)),
        })
        .collect()
}

pub(in crate::parse) fn apply_attrs_to_inline(node: &mut InlineNode, attrs: Attrs) {
    match node {
        InlineNode::Emphasis(n) => n.attrs = Some(attrs),
        InlineNode::Link(n) => n.attrs = Some(attrs),
        InlineNode::Image(n) => n.attrs = Some(attrs),
        InlineNode::Span(n) => n.attrs = Some(attrs),
        InlineNode::Math(n) => n.attrs = Some(attrs),
        InlineNode::AutoLink(n) => n.attrs = Some(attrs),
        InlineNode::Extension(n) => n.attrs = Some(attrs),
        _ => {}
    }
}

/// Merge an attribute block onto an inline node, accumulating classes (§15)
/// instead of overwriting -- used for chained blocks (`[x]{.a}{.b}`).
pub(in crate::parse) fn merge_attrs_into_inline(
    node: &mut InlineNode,
    attrs: Attrs,
    slots: &mut Option<HashSet<AttrSlot>>,
) {
    match node {
        InlineNode::Emphasis(n) => merge_attrs_indexed(&mut n.attrs, attrs, slots),
        InlineNode::Link(n) => merge_attrs_indexed(&mut n.attrs, attrs, slots),
        InlineNode::Image(n) => merge_attrs_indexed(&mut n.attrs, attrs, slots),
        InlineNode::Span(n) => merge_attrs_indexed(&mut n.attrs, attrs, slots),
        InlineNode::Math(n) => merge_attrs_indexed(&mut n.attrs, attrs, slots),
        InlineNode::AutoLink(n) => merge_attrs_indexed(&mut n.attrs, attrs, slots),
        InlineNode::Extension(n) => merge_attrs_indexed(&mut n.attrs, attrs, slots),
        InlineNode::Code(n) => merge_attrs_indexed(&mut n.attrs, attrs, slots),
        // A trailing standalone block chains onto an inline literal, promoting a
        // bare literal to a `<span>` (`` !`x`{.a}{.b} `` -> class="a b"). Matches
        // carve-js, whose merge attaches to any non-text node.
        InlineNode::LiteralInline(n) => merge_attrs_indexed(&mut n.attrs, attrs, slots),
        InlineNode::Footnote(n) => merge_attrs_indexed(&mut n.attrs, attrs, slots),
        InlineNode::CriticInsert(n) => merge_attrs_indexed(&mut n.attrs, attrs, slots),
        InlineNode::CriticDelete(n) => merge_attrs_indexed(&mut n.attrs, attrs, slots),
        _ => {}
    }
}

/// Whether an inline node can carry an attribute block (so a following `{...}`
/// attaches rather than staying literal). Text/raw nodes cannot.
pub(crate) fn inline_is_attributable(node: &InlineNode) -> bool {
    matches!(
        node,
        InlineNode::Emphasis(_)
            | InlineNode::Link(_)
            | InlineNode::Image(_)
            | InlineNode::Span(_)
            | InlineNode::Math(_)
            | InlineNode::AutoLink(_)
            | InlineNode::Extension(_)
            | InlineNode::Code(_)
            | InlineNode::LiteralInline(_)
            | InlineNode::Footnote(_)
            | InlineNode::CriticInsert(_)
            | InlineNode::CriticDelete(_)
    )
}

/// First byte of an attribute identifier (id/class/key): a letter or `_`
/// (matches `is_identifier`'s first-char rule). Non-ASCII bytes are never a
/// start here (`is_identifier` uses `is_ascii_alphabetic`).
#[inline]
fn is_attr_ident_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_'
}

#[inline]
fn is_css_ident_start(b: u8) -> bool {
    is_attr_ident_start(b) || b.is_ascii_digit()
}

/// Continuation byte of an attribute identifier: a letter, digit, `_`, or `-`
/// (matches `is_identifier`'s tail rule). Non-ASCII → false, ending the run.
#[inline]
pub(in crate::parse) fn is_attr_ident_part(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'-'
}

/// Whether the attribute payload following the `{` at `brace` provably cannot
/// parse -- i.e. `read_attrs_at`'s scan+`parse_attrs` would return `None`.
///
/// It walks the SAME token grammar `attr_tokens`/`parse_attrs` accept and bails
/// at the first byte that cannot continue a valid token, so a doomed payload is
/// rejected in O(1) per opener instead of the char walk running to a far `}`
/// (O(n²) on `[x]{`×n + `}`, `[x]{a `×n + `}`, `[x]{.a `×n + `}`, `[x]{k= `×n +
/// `}`, …). It is a pure SKIP filter: it returns `true` ONLY when the payload is
/// provably invalid; on a `}` (a candidate close), a newline, a quote, an
/// escape, a `key=<value>` with a real value, or any non-ASCII byte, it returns
/// `false` and the full scan/`parse_attrs` path decides. Every accepted block and
/// its output, is byte-identical. A nested `{`/`[` (or any other invalid
/// boundary byte) ends the walk, so each byte is visited O(1) times -> O(n)
/// total. Deferring on non-ASCII keeps it correct without decoding chars (only
/// the ASCII pathological shapes need the O(1) bail; a non-ASCII payload is rare
/// and still handled correctly by the full scan). Mirrors carve-js
/// `spanAttrProvablyInvalid`, matched to carve-rs's first-`}` (non-balancing)
/// acceptance.
fn attr_payload_provably_invalid(bytes: &[u8], brace: usize) -> bool {
    let n = bytes.len();
    let mut i = brace + 1;
    while i < n {
        let c = bytes[i];
        // Defer non-ASCII content to the full parser.
        if !c.is_ascii() {
            return false;
        }
        match c {
            // A candidate close at a token boundary: let the real scan decide.
            b'}' => return false,
            // A newline ends an inline block (read_attrs_at bails); defer.
            b'\n' => return false,
            // Skip space, tab, and carriage return; line feed defers above.
            b' ' | b'\t' | b'\r' => i += 1,
            // Quotes and escapes are subtle -- defer.
            b'"' | b'\'' | b'\\' => return false,
            // `#id` / `.class`: an identifier MUST follow, else the token (and
            // the whole payload) is invalid (§14).
            b'#' | b'.' => {
                match bytes.get(i + 1) {
                    Some(&d) if is_css_ident_start(d) => {}
                    _ => return true,
                }
                i += 2;
                while i < n && is_attr_ident_part(bytes[i]) {
                    i += 1;
                }
            }
            // `:TAG` / `:` semantic language attribute. The full parser owns
            // the structural subtag envelope; this fast path only avoids
            // rejecting the newly claimed token before it gets there.
            b':' => return false,
            // A bareword: a boolean attribute, or the name in `key=value`.
            _ if is_attr_ident_start(c) => {
                i += 1;
                while i < n && is_attr_ident_part(bytes[i]) {
                    i += 1;
                }
                if bytes.get(i) == Some(&b'=') {
                    // `key=` with an EMPTY value (EOF, `}`, or ASCII whitespace
                    // next) leaves a dangling `=` -> invalid. A bare value
                    // (>=1 non-space) or a quoted value: defer (a valid bare
                    // value is consumed whole by the scan -> linear). Non-ASCII
                    // after `=` (a value byte) also defers.
                    match bytes.get(i + 1) {
                        None => return true,
                        Some(&b'}') => return true,
                        Some(b' ' | b'\t' | b'\r' | b'\n') => return true,
                        _ => return false,
                    }
                }
                // else continue to the next token.
            }
            // Any other ASCII byte cannot begin a valid token at a boundary
            // (`[`, `{`, `(`, a digit, `-`, `+`, `=`, `,`, …): invalid.
            _ => return true,
        }
    }
    // Ran off the end without a `}`: the scan would fail too.
    true
}

/// Read an inline attribute block `{...}` at `start` (which must index a `{`).
///
/// `last_close_brace` is the index of the last `}` in `bytes` (or `None` if
/// there is none). A block can only close on a `}`, so when no `}` lies at or
/// after `start` the scan could only walk to end-of-text and fail -- it is
/// skipped in O(1). This keeps a run of unclosed `{`-attribute openers
/// (`[x]{`×n, `*a*{`×n, `:a:{`×n, …) linear instead of O(n^2). Callers scanning
/// a fresh slice (block attributes, table cells) pass that slice's own last-`}`
/// index. Skipping only elides a call that would return `None`, so output is
/// byte-identical.
/// The INLINE `attributes` production, whose interior is SPACE-ONLY.
///
/// Every whitespace slot of the inline block takes `space` (PART 4 THE INLINE
/// INTERIOR IS SPACE-ONLY, carve#906): the run after `{`, the run between two
/// attributes, the run before `}`, the boundary after an unquoted value, and
/// the blessed empty block `{ }`. All five sit AFTER the first non-whitespace
/// character of their line, which is where PART 7's rule already says a tab is
/// not syntax.
pub(in crate::parse) fn read_attrs_at(
    bytes: &[u8],
    start: usize,
    last_close_brace: Option<usize>,
) -> Option<(Attrs, usize)> {
    read_attrs_at_with(bytes, start, last_close_brace, true)
}

/// `read_attrs_at`, with the interior's whitespace rule as a parameter.
///
/// `space_only` is TRUE for the inline production and FALSE for the
/// block-attribute LINE, which keeps `whitespace` at all three of its slots -
/// and that distinction is the ruling rather than an omission. The block line
/// is the one construct whose interior can hold a leading indentation run:
/// after a `continuation`, the next line's leading whitespace IS indentation,
/// and the rule that narrows the inline block is the same rule that protects
/// this one.
fn read_attrs_at_with(
    bytes: &[u8],
    start: usize,
    last_close_brace: Option<usize>,
    space_only: bool,
) -> Option<(Attrs, usize)> {
    if bytes.get(start) != Some(&b'{') {
        return None;
    }
    if last_close_brace.map_or(true, |p| p < start) {
        return None;
    }
    // Reject, in O(1) per opener, a payload that provably cannot parse before the
    // char walk below runs to a far `}`. Without this, a run of never-validating
    // openers whose only `}` lies far ahead (`[x]{`×n + `}`, `[x]{a `×n + `}`, …)
    // walks to that `}` AND re-parses the whole tail at every opener -- O(n^2).
    // The filter only reports "invalid" where `parse_attrs` would return `None`
    // too, so output is byte-identical.
    if attr_payload_provably_invalid(bytes, start) {
        return None;
    }
    let mut i = start + 1;
    let mut quote: Option<u8> = None;
    let mut escaped = false;
    while i < bytes.len() {
        // An inline attribute block is single-line (grammar): a newline before
        // the closing `}` means this is not an inline attr -- the `{` stays
        // literal (`[x]{.a\n.b}` is text). Matches carve-js. Block-attribute
        // lines, which may span lines, are read by a separate path.
        if bytes[i] == b'\n' {
            return None;
        }
        if escaped {
            escaped = false;
            i += 1;
            continue;
        }
        if bytes[i] == b'\\' {
            escaped = true;
            i += 1;
            continue;
        }
        if let Some(q) = quote {
            if bytes[i] == q {
                quote = None;
            }
            i += 1;
            continue;
        }
        if bytes[i] == b'"' || bytes[i] == b'\'' {
            quote = Some(bytes[i]);
            i += 1;
            continue;
        }
        if bytes[i] == b'}' {
            break;
        }
        i += 1;
    }
    if i >= bytes.len() {
        return None;
    }
    let inner = std::str::from_utf8(&bytes[start + 1..i]).ok()?;
    Some((parse_attrs_with(inner, space_only)?, i + 1))
}

/// A generic attribute name starts with a letter or underscore. Explicit id
/// and class shorthands use the wider `is_css_identifier` preservation rule.
fn is_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

pub(crate) fn is_css_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphanumeric() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// Whether a name can be written as a BOOLEAN attribute -- a bare word with no
/// value. Narrower than [`is_identifier`] by exactly one character: a leading
/// `_` is legal in an id, a class and a key, and refused here.
///
/// `{_x_}` is BOTH a boolean attribute named `_x_` and a forced underline, and
/// alone on a line the attribute reading won: the block below came out as
/// `<p _x_="">`, and with no block below the line rendered NOTHING -- five
/// characters kept in the source and gone from the output. The bare form gives
/// the collision up (markup-carve/carve#1450). HTML has no underscore-first
/// boolean attribute to lose, and every other form ends in something other than
/// `_}`, so none of them collides.
fn is_boolean_attr_name(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

pub(in crate::parse) fn parse_attrs(src: &str) -> Option<Attrs> {
    parse_attrs_with(src, true)
}

fn parse_attrs_with(src: &str, space_only: bool) -> Option<Attrs> {
    // A TAB AT ANY OF THE FIVE INLINE POSITIONS MAKES THE BLOCK UNRECOGNIZED,
    // and its braces show. One test rather than five, because all five are the
    // same question - is this interior character syntax or not - and narrowing
    // them one at a time is how the executable spec left `[x]{<TAB>}` a valid
    // EMPTY block after its separator had already been narrowed. Inside a
    // QUOTED value the character is content and does not move.
    {
        let mut quote: Option<char> = None;
        let mut escaped = false;
        for ch in src.chars() {
            if escaped {
                escaped = false;
                continue;
            }
            match quote {
                Some(q) => {
                    if ch == '\\' {
                        escaped = true;
                    } else if ch == q {
                        quote = None;
                    }
                }
                None => match ch {
                    '\\' => escaped = true,
                    '"' | '\'' => quote = Some(ch),
                    '\t' => return None,
                    '\r' | '\n' if space_only => return None,
                    _ => {}
                },
            }
        }
    }
    if src.trim().is_empty() {
        return None;
    }
    let mut attrs = Attrs::default();
    for token in attr_tokens(src)? {
        if let Some(tag) = token.strip_prefix(':') {
            if !is_language_tag(tag) {
                return None;
            }
            if !attrs.key_values.contains_key("lang") {
                attrs.order.push(AttrSlot::Key("lang".to_string()));
            }
            attrs.key_values.insert("lang".to_string(), tag.to_string());
        } else if let Some(id) = token.strip_prefix('#') {
            if !is_css_identifier(id) {
                return None;
            }
            if attrs.id.is_none() {
                attrs.order.push(AttrSlot::Id);
            }
            attrs.id = Some(id.to_string());
        } else if let Some(class) = token.strip_prefix('.') {
            if !is_css_identifier(class) {
                return None;
            }
            if attrs.classes.is_empty() {
                attrs.order.push(AttrSlot::Class);
            }
            attrs.classes.push(class.to_string());
        } else if let Some((key, value)) = token.split_once('=') {
            if !is_identifier(key) {
                return None;
            }
            if value.is_empty() {
                return None;
            }
            // A quoted value unescapes ANY backslash-escaped ASCII punctuation
            // (grammar: escaped_char = '\' ascii_punctuation), not just \" / \'.
            // Route it through the same scan link/image titles use, matching
            // carve-js / carve-php. A bare value carries no escapes.
            let value = if let Some(inner) = value
                .strip_prefix('"')
                .and_then(|v| v.strip_suffix('"'))
                .or_else(|| value.strip_prefix('\'').and_then(|v| v.strip_suffix('\'')))
            {
                unescape_title(inner)
            } else {
                if value.chars().any(|ch| {
                    matches!(ch, '}' | '|' | '"' | '\'' | '\\' | ' ' | '\t' | '\r' | '\n')
                }) {
                    return None;
                }
                value.to_string()
            };
            if key == "id" {
                // `id=value` is the same attribute as `#id`: it feeds the id
                // slot, last-wins (§15), instead of emitting a second `id="…"`
                // (invalid HTML). `id` never enters key_values, so a bare `id`
                // boolean (below) cannot leave a stale duplicate. Matches
                // carve-php.
                if attrs.id.is_none() {
                    attrs.order.push(AttrSlot::Id);
                }
                attrs.id = Some(value);
            } else if key == "class" {
                // `class=value` is a SPELLING OF THE CLASS SLOT, the same way
                // `id=value` above is a spelling of the id slot (CARVE-P4-007):
                // it appends to `classes` in source order and never enters
                // `key_values`, so `{class=a .b}` is one `class="a b"` instead of
                // two `class` attributes on one element (carve#2439). The two
                // spellings stay distinct in SOURCE, because `.` reads the
                // `explicit_identifier` a fence word does while a value reaches
                // past it - `-col` is a class only the key-value form can spell.
                // ONE `Class` entry for the merged slot, the way the `.` branch
                // above records it: every class renders in a single `class`
                // attribute, so a second entry would make the writer spell an
                // attribute that does not exist.
                if attrs.classes.is_empty() {
                    attrs.order.push(AttrSlot::Class);
                }
                attrs.classes.push(value);
            } else {
                if !attrs.key_values.contains_key(key) {
                    attrs.order.push(AttrSlot::Key(key.to_string()));
                }
                attrs.key_values.insert(key.to_string(), value);
            }
        } else if is_boolean_attr_name(&token) {
            if token == "id" {
                // A bare boolean `id` also feeds the id slot (value ""), last-wins
                // and single -- `{id id=j}` -> `id="j"`, `{id}` -> `id=""`.
                if attrs.id.is_none() {
                    attrs.order.push(AttrSlot::Id);
                }
                attrs.id = Some(String::new());
            } else if token == "class" {
                // And a bare boolean `class` feeds the class slot, because PART 4
                // gives it the SAME empty-string AST value as `class=""` - which
                // the branch above folds - so leaving it here would make two
                // spellings of one documented value build different trees, and
                // would keep the second `class` attribute CARVE-P4-007 removes.
                if attrs.classes.is_empty() {
                    attrs.order.push(AttrSlot::Class);
                }
                attrs.classes.push(String::new());
            } else {
                // Boolean attribute: a bare word with no value, rendered name="".
                // (Matched last so `k=v` is a key/value, not a bare `k`.)
                if !attrs.key_values.contains_key(&token) {
                    attrs.order.push(AttrSlot::Key(token.clone()));
                }
                attrs.key_values.insert(token, String::new());
            }
        } else {
            return None;
        }
    }
    Some(attrs)
}

fn is_language_tag(tag: &str) -> bool {
    tag.is_empty()
        || tag.split('-').all(|subtag| {
            !subtag.is_empty()
                && subtag.len() <= 8
                && subtag.bytes().all(|byte| byte.is_ascii_alphanumeric())
        })
}

fn attr_tokens(src: &str) -> Option<Vec<String>> {
    let mut tokens = Vec::new();
    let mut buf = String::new();
    let mut quote: Option<char> = None;
    let mut escaped = false;
    let mut closed_quote = false;
    for ch in src.chars() {
        if escaped {
            buf.push('\\');
            buf.push(ch);
            escaped = false;
            continue;
        }
        if ch == '\\' && quote.is_some() {
            escaped = true;
            continue;
        }
        if let Some(q) = quote {
            buf.push(ch);
            if ch == q {
                quote = None;
                closed_quote = true;
            }
            continue;
        }
        if ch == '"' || ch == '\'' {
            if closed_quote || !matches!(buf.split_once('='), Some((_, ""))) {
                return None;
            }
            quote = Some(ch);
            buf.push(ch);
            continue;
        }
        // NO SPLIT ON A SIGIL. `attribute_list` is
        // `attribute, {space+, attribute}` (PART 7): a separator is required,
        // so `{.a.b}`, `{#i.c}` and `{.a#i}` are not attribute blocks and stay
        // literal. This branch used to break a class or id token at the next
        // `#` or `.`, manufacturing two attributes where the source has one
        // malformed one - `.a.b` now stays one token and fails `is_identifier`,
        // which is what makes the whole block literal.
        if matches!(ch, ' ' | '\t' | '\r' | '\n') {
            closed_quote = false;
            if !buf.is_empty() {
                tokens.push(std::mem::take(&mut buf));
            }
        } else {
            if closed_quote {
                return None;
            }
            buf.push(ch);
        }
    }
    if quote.is_some() || escaped {
        return None;
    }
    if !buf.is_empty() {
        tokens.push(buf);
    }
    Some(tokens)
}
