use super::*;

/// A copy of `attrs` without one key-value, dropping the slot from `order`.
/// Returns `None` when the removal leaves nothing to render.
pub(super) fn without_key(attrs: &Attrs, key: &str) -> Option<Attrs> {
    let mut next = attrs.clone();
    next.key_values.remove(key);
    next.order
        .retain(|slot| !matches!(slot, AttrSlot::Key(k) if k == key));
    if next.id.is_none() && next.classes.is_empty() && next.key_values.is_empty() {
        return None;
    }
    Some(next)
}

/// PART 9 §17 L7: the writer spells the looseness with `{loose}` ONLY where the
/// blank-line spelling cannot say it.
pub(super) fn with_loose_key(needs_key: bool, attrs: &Option<Attrs>, body: &str) -> String {
    if !needs_key {
        return with_block_attrs(attrs, body);
    }
    let mut attrs = attrs.clone().unwrap_or_default();
    attrs.key_values.insert("loose".to_string(), String::new());
    attrs
        .order
        .retain(|slot| !matches!(slot, AttrSlot::Key(key) if key == "loose"));
    if attrs.order.is_empty() {
        // An EMPTY order means `render_attrs` falls back to id, classes, then
        // keys - and naming one slot switches it to the ordered branch, which
        // would then drop the id and the classes it no longer lists. Spell the
        // fallback out so the key can lead without moving anything else.
        attrs.order = vec![
            AttrSlot::Key("loose".to_string()),
            AttrSlot::Id,
            AttrSlot::Class,
        ];
    } else {
        attrs.order.insert(0, AttrSlot::Key("loose".to_string()));
    }
    // The key LEADS, which is where an author writes it and where the corpus
    // shows it. Its position among the other slots is not observable in the
    // output - it is consumed before any renderer sees it - so leading is a
    // spelling choice rather than a fact being moved.
    format!("{}\n{body}", render_attrs(&Some(attrs)))
}

/// Whether a LIST needs the key: §17 L7's re-parse, with the one shortcut that
/// is sound in a single direction.
pub(super) fn list_needs_loose_key(
    session: &RenderSession,
    list: &List,
    body: &str,
    nested: bool,
) -> bool {
    if list.tight || list.items.is_empty() {
        return false;
    }
    // TWO OR MORE ITEMS ALWAYS RE-PARSE LOOSE. §17 L2 loosens on a blank line
    // between items, and this writer emits one between every pair of a loose
    // list's items, so the re-parse below can only ever answer "already spelled"
    // here. A shortcut in ONE direction: it never suppresses a key the re-parse
    // would have emitted.
    if list.items.len() > 1 {
        return false;
    }
    // ONE ITEM has no "between items" for a blank line to stand in, so the only
    // spelling left is one the item's own CONTENT produces - and whether it does
    // is the parser's question. §17 L1, L2 and L6 decide it together, so a
    // second copy of them here would answer differently the day any of them
    // moves: a lead container holding a blank line re-reads LOOSE, while the
    // same blank line before a fence does not.
    //
    // A body with NO blank line in it cannot re-read loose either way, so the
    // shape the clause exists for - a one-item list holding one paragraph - is
    // answered without a parse.
    if !body_has_blank_line(body) {
        return true;
    }
    let restored = restore_verbatim(session, body);
    let probe = if nested {
        format!("- {}", restored.replace('\n', "\n  "))
    } else {
        restored
    };
    let tree = comparable_tree(&probe);
    let first = tree.as_ref().and_then(|doc| doc.children.first());
    let candidate = if nested {
        match first {
            Some(BlockNode::List(wrapper)) => wrapper.items.first().and_then(|item| {
                item.children
                    .iter()
                    .find(|child| !matches!(child, BlockNode::Comment(_)))
            }),
            _ => None,
        }
    } else {
        tree.as_ref().and_then(|doc| {
            doc.children
                .iter()
                .find(|child| !matches!(child, BlockNode::Comment(_)))
        })
    };
    match candidate {
        Some(BlockNode::List(reparsed)) => reparsed.tight,
        // Anything else means the body did not read back as a list at all, so
        // the looseness certainly did not survive.
        _ => true,
    }
}

/// A blank line INSIDE `body`, which is the only place one can loosen it.
///
/// Interior, so a body's own leading or trailing newline does not count: those
/// are the writer's joins rather than content, and reading one as a blank line
/// would send every single-item list through the re-parse for nothing.
pub(super) fn body_has_blank_line(body: &str) -> bool {
    body.match_indices('\n').any(|(at, _)| {
        body[at + 1..].split('\n').next().is_some_and(|line| {
            line.len() < body.len() - at - 1
                && line
                    .trim_matches(|ch: char| ch == ' ' || ch == '\t')
                    .is_empty()
        })
    })
}

pub(super) fn with_block_attrs(attrs: &Option<Attrs>, body: &str) -> String {
    let rendered = render_attrs(attrs);
    if rendered.is_empty() {
        body.to_string()
    } else {
        format!("{rendered}\n{body}")
    }
}

pub(super) fn nested_tight_item_loses_tightness(item: &ListItem) -> bool {
    item.children.windows(2).any(|pair| {
        matches!(pair[1], BlockNode::Paragraph(_))
            && !matches!(&pair[1], BlockNode::Paragraph(p) if matches!(p.children.as_slice(), [InlineNode::Text(t)] if t.value == "+"))
            && (matches!(pair[0], BlockNode::BlockQuote(_))
                || matches!(&pair[0], BlockNode::List(list) if list.items.last().is_some_and(|item| !item.children.is_empty())))
    })
}

pub(super) fn indent_adjacent_list(text: &str, width: usize) -> String {
    let indent = " ".repeat(width);
    text.lines()
        .map(|line| {
            if line.is_empty() {
                String::new()
            } else {
                format!("{indent}{line}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub(super) fn ends_in_empty_nested_item(list: &List) -> bool {
    list.items.last().and_then(|item| item.children.last()).is_some_and(
        |block| matches!(block, BlockNode::List(child) if child.items.last().is_some_and(|item| item.children.is_empty() || matches!(item.children.last(), Some(BlockNode::List(_)) if ends_in_empty_nested_item(child))))
    )
}

pub(super) fn spell_list_item(
    session: &RenderSession,
    prefix: &str,
    content: &str,
    continuation_width: usize,
) -> String {
    let mut lines = content.split('\n');
    let marker = marker_column(session);
    let first = lines.next().unwrap_or_default();
    let first = first.strip_prefix(marker.as_str()).unwrap_or(first);
    let mut source = format!("{prefix}{first}\n");
    let continuation = " ".repeat(continuation_width);
    let blank = verbatim_blank(session);
    for line in lines {
        if line.is_empty() || line == *blank {
            source.push_str(line);
            source.push('\n');
        } else if let Some(rest) = line.strip_prefix(marker.as_str()) {
            source.push_str(rest);
            source.push('\n');
        } else {
            source.push_str(&continuation);
            source.push_str(line);
            source.push('\n');
        }
    }
    source
}

pub(super) fn relative_item_content(
    session: &RenderSession,
    content: &str,
    boundaries: &[(usize, usize)],
) -> String {
    let mut relative = content.to_string();
    for &(start, end) in boundaries.iter().rev() {
        let block = &content[start + 1..end];
        relative.replace_range(
            start..end,
            &format!(
                "{}\n{}",
                at_marker_column(session, "+"),
                at_marker_column(session, block)
            ),
        );
    }
    relative
}

pub(super) fn list_item_reads_back(
    session: &RenderSession,
    source: &str,
    list: &List,
    item: &ListItem,
    list_depth: usize,
    quote_depth: usize,
) -> bool {
    let mut nested = restore_verbatim(session, source);
    for _ in 1..list_depth {
        nested = format!("- parent\n{}", indent_adjacent_list(&nested, 2));
    }
    for _ in 0..quote_depth {
        nested = nested
            .split('\n')
            .map(|line| {
                if line.is_empty() {
                    ">".to_string()
                } else {
                    format!("> {line}")
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
    }
    let Some(parsed) = comparable_tree(&nested) else {
        return false;
    };
    let mut blocks = parsed.children.as_slice();
    for _ in 0..quote_depth {
        if blocks.len() != 1 {
            return false;
        }
        let Some(BlockNode::BlockQuote(quote)) = blocks.first() else {
            return false;
        };
        blocks = &quote.children;
    }
    let Some(BlockNode::List(mut_list)) = blocks.first() else {
        return false;
    };
    if blocks.len() != 1 {
        return false;
    }
    let mut read_list = mut_list;
    for _ in 1..list_depth {
        if read_list.items.len() != 1 {
            return false;
        }
        let Some(next) = read_list.items.first().and_then(|parent| {
            parent.children.iter().find_map(|block| match block {
                BlockNode::List(list) => Some(list),
                _ => None,
            })
        }) else {
            return false;
        };
        read_list = next;
    }
    if read_list.items.len() != 1 {
        return false;
    }
    if read_list.tight != list.tight {
        return false;
    }
    let mut target_list = list.clone();
    target_list.items = vec![item.clone()];
    let target = comparable_document(Document {
        frontmatter: Default::default(),
        frontmatter_raw: None,
        footnote_defs: Default::default(),
        footnote_def_pos: Default::default(),
        children: vec![BlockNode::List(target_list)],
        source_len: 0,
        ingest_payload_len: 0,
    });
    let mut actual = target.clone();
    let BlockNode::List(actual_list) = &mut actual.children[0] else {
        return false;
    };
    actual_list.items = read_list.items.clone();
    crate::ast_json::to_json(&actual) == crate::ast_json::to_json(&target)
}

pub(super) fn render_list(session: &RenderSession, node: &List, ctx: &mut CarveContext) -> String {
    ctx.list_depth += 1;
    let mut out = String::new();
    let mut counter = node.start.unwrap_or(1);
    // The marker is semantic (§11: a different bullet char / ordered delim
    // starts a new list), so emit it as authored - normalizing would merge
    // adjacent sibling lists on re-parse (carve issue 286).
    let delim = node.delim.unwrap_or('.');
    let bullet = node.bullet_char.unwrap_or('-');
    let items = node.items.as_ptr() as usize;
    for (idx, item) in node.items.iter().enumerate() {
        let escape_window::Visit::Render(recorded) = escape_window::visit(session, items, idx)
        else {
            if node.ordered {
                counter += 1;
            }
            continue;
        };
        // NO absolute depth term. The parent item's continuation prefix is
        // already the child list's indentation, so adding `"  " * (depth - 1)`
        // on top indented every level twice - and the two-space strip below was
        // compensating for it. Output grew as O(depth^3) where the source is
        // O(depth^2), and `05-lists-5` came back with four spaces where it was
        // written with two (carve-rs#594, the same defect carve-js fixed in its
        // #653).
        let mut prefix = if node.ordered {
            let marker = if node.bare_marker {
                String::new()
            } else {
                ordered_marker(counter, node.ol_type)
            };
            counter += 1;
            format!("{marker}{delim} ")
        } else if item.checked.is_some() {
            format!("{bullet} {} ", task_marker(item))
        } else {
            format!("{bullet} ")
        };
        let continuation_width = if node.ordered { prefix.len() } else { 2 };
        let item_attrs = render_attrs(&item.attrs);
        if !item_attrs.is_empty() {
            prefix = if node.ordered {
                format!("{}{item_attrs} ", prefix.trim_end())
            } else if item.checked.is_some() {
                format!("{bullet}{item_attrs} {} ", task_marker(item))
            } else {
                format!("{bullet}{item_attrs} ")
            };
        }
        let inserted_before_item = session.inserted.with(std::cell::Cell::get);
        let seen_before_item = session.seen.with(std::cell::Cell::get);
        let mut relative_boundaries = Vec::new();
        let mut content = if item.children.is_empty() {
            item.pos
                .as_ref()
                .and_then(|pos| definition_at_line(session, pos.start_line, ctx))
                .unwrap_or_default()
        } else {
            at_item_content_column(ctx, |ctx| {
                render_item_blocks(
                    session,
                    &item.children,
                    node.tight,
                    item.pos.as_ref(),
                    ctx,
                    &mut relative_boundaries,
                )
            })
        };
        // EMPTINESS is the only thing that earns `+`. A second, text-shaped test
        // (content starts with `[^` and holds a colon-space) stood for "the item
        // spelled a footnote definition that collection hoisted" - but collection
        // always empties the item, so that test could only fire on authored
        // inline content, and it replaced the whole item with `+` (carve-rs#2097).
        if trim_non_nbsp(&content).is_empty() {
            content = "+".to_string();
        }
        let raw_content = content;
        let mut content = trim_non_nbsp(&raw_content).to_string();
        let mut item_source = spell_list_item(session, &prefix, &content, continuation_width);
        let nested_tightness_loss =
            node.tight && ctx.list_depth > 1 && nested_tight_item_loses_tightness(item);
        if nested_tightness_loss {
            crate::render_loss::record_list_tightness_lost(item.pos.as_ref());
        }
        // Another separated body at the last list level would cross the parse cap.
        let capped_boundary =
            ctx.list_depth >= crate::parse::MAX_NESTING_DEPTH && !relative_boundaries.is_empty();
        if capped_boundary {
            note_unspellable_list_nesting();
        }
        let needs_readback = !capped_boundary
            && node.tight
            && ((ctx.list_depth > 1 && (!relative_boundaries.is_empty() || nested_tightness_loss))
                || item.children.windows(2).any(|pair| {
                    matches!(pair[0], BlockNode::List(_))
                        && matches!(
                            pair[1],
                            BlockNode::CodeBlock(_)
                                | BlockNode::Figure(_)
                                | BlockNode::BlockQuote(_)
                        )
                        || matches!(pair[0], BlockNode::BlockQuote(_))
                            && matches!(pair[1], BlockNode::BlockQuote(_))
                }));
        if needs_readback
            && !list_item_reads_back(
                session,
                &item_source,
                node,
                item,
                ctx.list_depth,
                ctx.quote_depth,
            )
        {
            let relative = relative_item_content(session, &raw_content, &relative_boundaries);
            let relative = trim_non_nbsp(&relative).to_string();
            let relative_source = spell_list_item(session, &prefix, &relative, continuation_width);
            if !nested_tightness_loss
                && !relative_boundaries.is_empty()
                && list_item_reads_back(
                    session,
                    &relative_source,
                    node,
                    item,
                    ctx.list_depth,
                    ctx.quote_depth,
                )
            {
                content = relative;
                item_source = relative_source;
            } else {
                session.inserted.with(|cell| cell.set(inserted_before_item));
                session.seen.with(|cell| cell.set(seen_before_item));
                let mut unused_boundaries = Vec::new();
                content = at_item_content_column(ctx, |ctx| {
                    render_item_blocks(
                        session,
                        &item.children,
                        false,
                        item.pos.as_ref(),
                        ctx,
                        &mut unused_boundaries,
                    )
                });
                if trim_non_nbsp(&content).is_empty() {
                    content = "+".to_string();
                }
                content = trim_non_nbsp(&content).to_string();
                item_source = spell_list_item(session, &prefix, &content, continuation_width);
                if ctx.list_depth >= crate::parse::MAX_NESTING_DEPTH {
                    note_unspellable_list_nesting();
                }
                if !nested_tightness_loss {
                    crate::render_loss::record_list_tightness_lost(item.pos.as_ref());
                }
            }
        }
        // COUNT THE MARKER-COLUMN TAGS STANDING IN THE ASSEMBLED ITEM, here and
        // not at restore time: this loop is what consumes them, so this is the
        // last moment an authored one is still visible. Counted over the whole
        // item rather than only over the lines the loop strips - a tag the item
        // dropped would otherwise hide an authored occurrence behind a matching
        // insertion count, and answering from the item's own text cannot.
        note_seen(
            session,
            S_MARKER_COLUMN,
            content.matches(marker_column(session).as_str()).count(),
        );
        out.push_str(&item_source);
        let ends_with_nested_list = content.lines().last().is_some_and(|line| {
            line.starts_with(' ') && is_rendered_list_marker(line.trim_start())
        });
        if !node.tight && idx < node.items.len() - 1 && !ends_with_nested_list {
            out.push('\n');
        }
        escape_window::leave(session, recorded);
    }
    ctx.list_depth -= 1;
    trim_end_non_nbsp(&out).to_string()
}

pub(crate) fn ordered_marker(n: usize, ty: Option<OrderedListType>) -> String {
    match ty {
        Some(OrderedListType::LowerAlpha) => alpha_marker(n, false),
        Some(OrderedListType::UpperAlpha) => alpha_marker(n, true),
        Some(OrderedListType::LowerRoman) => roman_marker(n).to_ascii_lowercase(),
        Some(OrderedListType::UpperRoman) => roman_marker(n),
        None => n.to_string(),
    }
}

pub(super) fn is_rendered_list_marker(line: &str) -> bool {
    line.starts_with("- ")
        || line.starts_with("* ")
        || line.starts_with("- [")
        || line.starts_with("* [")
        || [". ", ") "].iter().any(|sep| {
            line.split_once(sep).is_some_and(|(marker, _)| {
                !marker.is_empty() && marker.chars().all(|ch| ch.is_ascii_alphanumeric())
            })
        })
}

pub(super) fn alpha_marker(n: usize, upper: bool) -> String {
    let base = ((n.saturating_sub(1) % 26) as u8) + if upper { b'A' } else { b'a' };
    (base as char).to_string()
}

pub(super) fn roman_marker(mut n: usize) -> String {
    let values = [
        (1000, "M"),
        (900, "CM"),
        (500, "D"),
        (400, "CD"),
        (100, "C"),
        (90, "XC"),
        (50, "L"),
        (40, "XL"),
        (10, "X"),
        (9, "IX"),
        (5, "V"),
        (4, "IV"),
        (1, "I"),
    ];
    let mut out = String::new();
    for (value, token) in values {
        while n >= value {
            out.push_str(token);
            n -= value;
        }
    }
    if out.is_empty() {
        "I".to_string()
    } else {
        out
    }
}

/// Every entry writes its own description line, so consecutive `::` lines never
/// end up sharing one: a `<dl>` writes back as ONE list with the grouping it
/// parsed from, and no term acquires the next entry's description.
pub(super) fn render_definition_list(
    session: &RenderSession,
    items: &[DefinitionItem],
    ctx: &mut CarveContext,
) -> String {
    let mut out: Vec<String> = Vec::new();
    // No separating blank: markup-carve/carve#1970 ruled this as two entries,
    // matching the other engines. The old blank worked around carve-rs#1559;
    // a nested lead fence closed by an indented closer now releases ownership.
    let list = items.as_ptr() as usize;
    for (index, item) in items.iter().enumerate() {
        let escape_window::Visit::Render(recorded) = escape_window::visit(session, list, index)
        else {
            continue;
        };
        for term in &item.terms {
            // A comment opening a line must stay past the term's column, or it
            // would end the term (carve#2411).
            let outer_term = std::mem::replace(&mut ctx.in_term, true);
            let rendered = render_inlines(session, term, ctx);
            ctx.in_term = outer_term;
            if rendered.is_empty() {
                crate::render_carve_error::record_unspellable(
                    "definition_term",
                    "an empty definition term has no Carve source spelling",
                );
            }
            out.push(format!(":: {rendered}"));
        }
        for def in &item.definitions {
            // An EMPTY description whose line carries a hoisted definition is one
            // the author wrote that definition on: write it back there
            // (markup-carve/carve#805). Without this the line came out as a bare
            // `:`, which re-parses into the term above it.
            if def.children.is_empty() {
                let line = def.pos.as_ref().map(|pos| pos.start_line);
                let written = line.and_then(|line| definition_at_line(session, line, ctx));
                if let Some(written) = written {
                    let mut written_lines = written.split('\n');
                    out.push(format!(": {}", written_lines.next().unwrap_or_default()));
                    // A footnote body can be multi-line; its continuation lines
                    // carry the body's own indent and sit under the description.
                    for written_line in written_lines {
                        out.push(format!("  {written_line}"));
                    }
                    continue;
                }
            }
            let body = trim_non_nbsp(&at_item_content_column(ctx, |ctx| {
                render_blocks(session, def, ctx, None)
            }))
            .to_string();
            if body.is_empty() {
                out.push(": {empty}".to_string());
                continue;
            }
            let mut lines = body.split('\n');
            out.push(format!(": {}", lines.next().unwrap_or_default()));
            for line in lines {
                out.push(format!("  {line}"));
            }
        }
        escape_window::leave(session, recorded);
    }
    out.join("\n")
}
