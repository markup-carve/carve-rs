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
    if list.effective_tight() || list.items.is_empty() {
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
    let wrap = |source: &str| {
        if nested {
            format!("- {}", source.replace('\n', "\n  "))
        } else {
            source.to_string()
        }
    };
    // Re-parsing the whole body at every level of a nested ladder is
    // superlinear in depth (carve#5193). A last sub-list's interior cannot
    // undo a blank above it, so a pruned probe that reads loose answers alone.
    if let Some(pruned) = above_last_sub_list_interior(list, &restored) {
        if reads_loose_as(comparable_tree(&wrap(pruned)).as_ref(), list, nested) {
            return false;
        }
    }
    let probe = wrap(&restored);
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

/// The written item up to the first line of its last child, when that child is
/// a sub-list whose remaining lines all sit past the item's content column.
fn above_last_sub_list_interior<'a>(list: &List, restored: &'a str) -> Option<&'a str> {
    let item = list.items.first()?;
    if !matches!(item.children.last(), Some(BlockNode::List(_))) {
        return None;
    }
    let (_, width) = item_prefix(list, item, list.start.unwrap_or(1));
    let indent = |line: &str| line.len() - line.trim_start_matches(' ').len();
    let lines: Vec<&str> = restored.split('\n').collect();
    let start = (1..lines.len())
        .rev()
        .find(|&at| !lines[at].trim().is_empty() && indent(lines[at]) <= width)?;
    if indent(lines[start]) != width
        || !lines[start - 1].trim().is_empty()
        || start + 1 == lines.len()
    {
        return None;
    }
    let end: usize = lines[..=start]
        .iter()
        .map(|line| line.len() + 1)
        .sum::<usize>()
        - 1;
    Some(&restored[..end])
}

/// Whether `tree` reads back the probed list loose, its item holding the same
/// kinds of child the tree's item holds.
fn reads_loose_as(tree: Option<&Document>, list: &List, nested: bool) -> bool {
    let visible = |blocks: &[BlockNode]| -> Vec<std::mem::Discriminant<BlockNode>> {
        blocks
            .iter()
            .filter(|child| !matches!(child, BlockNode::Comment(_)))
            .map(std::mem::discriminant)
            .collect()
    };
    let Some(doc) = tree else {
        return false;
    };
    let mut blocks = doc.children.as_slice();
    if nested {
        let Some(BlockNode::List(wrapper)) = blocks.first() else {
            return false;
        };
        let Some(parent) = wrapper.items.first() else {
            return false;
        };
        blocks = &parent.children;
    }
    let found = blocks
        .iter()
        .find(|child| !matches!(child, BlockNode::Comment(_)));
    let Some(BlockNode::List(reparsed)) = found else {
        return false;
    };
    let ([read], [item]) = (reparsed.items.as_slice(), list.items.as_slice()) else {
        return false;
    };
    !reparsed.tight && visible(&read.children) == visible(&item.children)
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

fn nested_list_ends_in_a_plus_letter(block: &BlockNode) -> bool {
    let BlockNode::List(list) = block else {
        return false;
    };
    match list.items.last().and_then(|item| item.children.last()) {
        Some(BlockNode::Paragraph(paragraph)) => {
            matches!(paragraph.children.as_slice(), [InlineNode::Text(text)] if text.value == "+")
        }
        Some(nested) => nested_list_ends_in_a_plus_letter(nested),
        None => false,
    }
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

/// A tab immediately after a list separator is content (CARVE-P2-010).
/// Keep it when trimming the wrapper whitespace around a rendered item.
fn trim_list_content(text: &str) -> String {
    let content = trim_end_non_nbsp(text).trim_start_matches([' ', '\n', '\r']);
    if content.starts_with('\t') {
        content.to_string()
    } else {
        trim_non_nbsp(text).to_string()
    }
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
    if read_list.effective_tight() != list.effective_tight() {
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

/// An item's marker as written, attributes included, and the continuation
/// width the item's body lines take.
pub(super) fn item_prefix(node: &List, item: &ListItem, number: usize) -> (String, usize) {
    let delim = node.delim.unwrap_or('.');
    let bullet = node.bullet_char.unwrap_or('-');
    let mut prefix = if node.ordered {
        let marker = if node.bare_marker {
            String::new()
        } else {
            ordered_marker(number, node.ol_type)
        };
        // A letter or roman marker with `.` takes two spaces (carve#4811).
        let gap = if delim == '.' && marker.bytes().any(|b| b.is_ascii_alphabetic()) {
            "  "
        } else {
            " "
        };
        format!("{marker}{delim}{gap}")
    } else if item.checked.is_some() {
        format!("{bullet} {} ", task_marker(item))
    } else {
        format!("{bullet} ")
    };
    let continuation_width = if node.ordered { prefix.len() } else { 2 };
    let item_attrs = render_attrs(&item.attrs);
    if !item_attrs.is_empty() {
        prefix = if node.ordered {
            let gap = &prefix[prefix.trim_end().len()..];
            format!("{}{item_attrs}{gap}", prefix.trim_end())
        } else if item.checked.is_some() {
            format!("{bullet}{item_attrs} {} ", task_marker(item))
        } else {
            format!("{bullet}{item_attrs} ")
        };
    }
    (prefix, continuation_width)
}

pub(super) fn render_list(session: &RenderSession, node: &List, ctx: &mut CarveContext) -> String {
    ctx.list_depth += 1;
    let tight = node.effective_tight();
    let mut out = String::new();
    let mut counter = node.start.unwrap_or(1);
    // The marker is semantic (§11: a different bullet char / ordered delim
    // starts a new list), so emit it as authored - normalizing would merge
    // adjacent sibling lists on re-parse (carve issue 286).
    let items = node.items.as_ptr() as usize;
    let list_followed_by_marker = ctx.item_followed_by_marker;
    let list_followed_by_tight_marker = ctx.item_followed_by_tight_marker;
    for (idx, item) in node.items.iter().enumerate() {
        ctx.item_followed_by_marker = list_followed_by_marker || idx + 1 < node.items.len();
        ctx.item_followed_by_tight_marker = if idx + 1 < node.items.len() {
            tight
        } else {
            list_followed_by_tight_marker
        };
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
        let (prefix, continuation_width) = item_prefix(node, item, counter);
        if node.ordered {
            counter += 1;
        }
        let inserted_before_item = session.inserted.with(std::cell::Cell::get);
        let seen_before_item = session.seen.with(std::cell::Cell::get);
        let mut relative_boundaries = Vec::new();
        let task_box_shift = if item.checked.is_some() && render_attrs(&item.attrs).is_empty() {
            "[ ] ".len()
        } else {
            0
        };
        let mut content = if item.children.is_empty() {
            item.pos
                .as_ref()
                .and_then(|pos| definition_at_line(session, pos.start_line, ctx))
                .unwrap_or_default()
        } else {
            ctx.marker_line_shift = task_box_shift;
            at_item_content_column(ctx, |ctx| {
                render_item_blocks(
                    session,
                    &item.children,
                    tight,
                    item.pos.as_ref(),
                    ctx,
                    &mut relative_boundaries,
                )
            })
        };
        // EMPTINESS is the only thing that earns `{empty}`. A second, text-shaped test
        // (content starts with `[^` and holds a colon-space) stood for "the item
        // spelled a footnote definition that collection hoisted" - but collection
        // always empties the item, so that test could only fire on authored
        // inline content, and it replaced the whole item with the sentinel (carve-rs#2097).
        if trim_non_nbsp(&content).is_empty() {
            content = "{empty}".to_string();
        }
        let raw_content = content;
        let mut content = trim_list_content(&raw_content);
        let mut item_source = spell_list_item(session, &prefix, &content, continuation_width);
        let nested_tightness_loss =
            tight && ctx.list_depth > 1 && nested_tight_item_loses_tightness(item);
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
            && tight
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
        // A blank would make a bare letter `+` hold, so the escape would relax
        // into a blank line; the failed read-back has to reach the escape pass
        // instead (CARVE-P11-042, carve#4868).
        let bare_plus_letter = item.children.windows(2).any(|pair| {
            nested_list_ends_in_a_plus_letter(&pair[0])
                && matches!(
                    pair[1],
                    BlockNode::CodeBlock(_) | BlockNode::Figure(_) | BlockNode::BlockQuote(_)
                )
        }) && item_source.lines().any(|line| line.ends_with(" +"));
        // A blank line before a tight item's child block would loosen it (PART 9
        // §17 L1, carve#5015), so a separated body takes the `+` form (CARVE-P11-042).
        if tight && !capped_boundary && !relative_boundaries.is_empty() {
            let relative = relative_item_content(session, &raw_content, &relative_boundaries);
            let relative = trim_list_content(&relative);
            let relative_source = spell_list_item(session, &prefix, &relative, continuation_width);
            if list_item_reads_back(
                session,
                &relative_source,
                node,
                item,
                ctx.list_depth,
                ctx.quote_depth,
            ) {
                content = relative;
                item_source = relative_source;
            }
        }
        if needs_readback
            && !bare_plus_letter
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
            let relative = trim_list_content(&relative);
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
                ctx.marker_line_shift = task_box_shift;
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
                    content = "{empty}".to_string();
                }
                content = trim_list_content(&content);
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
        // A blank after the last erased comment can combine with the blanks
        // inside the item into a boundary that ends this list.
        let mut trailing_comments = 0;
        let mut blank_between_comments = false;
        let mut crossed_blank = false;
        for line in item_source.lines().rev() {
            let line = line.trim();
            if line.starts_with("%%") && !line.starts_with("%%%") {
                trailing_comments += 1;
                if trailing_comments > 1 && crossed_blank {
                    blank_between_comments = true;
                }
                crossed_blank = false;
            } else if line.is_empty() {
                crossed_blank = true;
            } else {
                break;
            }
        }
        if !tight && idx < node.items.len() - 1 && !ends_with_nested_list && !blank_between_comments
        {
            out.push('\n');
        }
        escape_window::leave(session, recorded);
    }
    ctx.item_followed_by_marker = list_followed_by_marker;
    ctx.item_followed_by_tight_marker = list_followed_by_tight_marker;
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

/// A term's inlines, and each comment that starts a line of it: those are
/// written on lines of their own below the term.
fn split_term_comments(term: &[InlineNode]) -> (Vec<InlineNode>, Vec<&InlineNode>) {
    let mut inline: Vec<InlineNode> = Vec::new();
    let mut comments = Vec::new();
    for node in term {
        let own_line = match (node, inline.last()) {
            (InlineNode::Comment(comment), Some(previous)) if !comment.delimited => {
                matches!(previous, InlineNode::SoftBreak(_))
                    || comment
                        .pos
                        .as_ref()
                        .zip(previous.pos())
                        .is_some_and(|(at, previous)| at.start_line > previous.end_line)
            }
            _ => false,
        };
        if own_line {
            if matches!(inline.last(), Some(InlineNode::SoftBreak(_))) {
                inline.pop();
            }
            comments.push(node);
        } else if comments.is_empty() || !matches!(node, InlineNode::SoftBreak(_)) {
            inline.push(node.clone());
        }
    }
    (inline, comments)
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
            let (inline, comments) = split_term_comments(term);
            let outer_term = std::mem::replace(&mut ctx.in_term, true);
            // A term is one line like a heading (carve#4861), so a break collapses to a space.
            let rendered = super::collapse_breaks(&render_inlines(session, &inline, ctx));
            ctx.in_term = outer_term;
            if rendered.is_empty() {
                crate::render_carve_error::record_unspellable(
                    "definition_term",
                    "an empty definition term has no Carve source spelling",
                );
            }
            out.push(format!(":: {rendered}"));
            for comment in comments {
                out.push(render_inlines(session, std::slice::from_ref(comment), ctx));
            }
        }
        for (def_index, def) in item.definitions.iter().enumerate() {
            let last_description =
                index + 1 == items.len() && def_index + 1 == item.definitions.len();
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
                ctx.join_trailing_comments = last_description && ctx.item_followed_by_marker;
                let outer_body = std::mem::replace(&mut ctx.marker_body, def.as_ptr() as usize);
                let body = render_blocks(session, def, ctx, None);
                ctx.marker_body = outer_body;
                body
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

#[cfg(test)]
mod loose_key_probe_tests {
    use super::super::tests::PARSED_BYTES;

    /// `depth` one-item lists, each a blank line and one step below the last.
    fn loose_ladder(depth: usize) -> String {
        (0..depth)
            .map(|level| format!("{}- x", "  ".repeat(level)))
            .collect::<Vec<_>>()
            .join("\n\n")
            + "\n"
    }

    /// Source bytes the writer re-parses per byte of the ladder it writes.
    fn reparsed_per_byte(depth: usize) -> f64 {
        let source = loose_ladder(depth);
        let doc = crate::parse::parse(&source);
        PARSED_BYTES.with(|n| n.set(0));
        let written = crate::render_carve(&doc).expect("written");
        assert_eq!(written.trim_end(), source.trim_end());
        PARSED_BYTES.with(std::cell::Cell::get) as f64 / source.len() as f64
    }

    /// COUNTED, so it holds under any load: deciding `{loose}` for each level
    /// must not re-parse the levels below it (carve#5193).
    #[test]
    fn a_loose_ladder_is_written_without_reparsing_each_subtree() {
        std::thread::Builder::new()
            .stack_size(64 * 1024 * 1024)
            .spawn(|| {
                let shallow = reparsed_per_byte(20);
                let deep = reparsed_per_byte(80);
                assert!(
                    deep < shallow * 1.5 && deep < 8.0,
                    "re-parsed {shallow:.1} bytes per byte at depth 20, {deep:.1} at depth 80"
                );
            })
            .expect("spawn worker")
            .join()
            .expect("worker must return");
    }
}
