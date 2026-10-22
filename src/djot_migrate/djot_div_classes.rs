use super::*;
use crate::ast::BlockNode;

struct Frame {
    source_width: usize,
    target_width: usize,
    parent: Option<usize>,
    bare: bool,
}

pub(super) fn normalize(source: &str) -> String {
    if !source.contains(":::") {
        return source.to_owned();
    }
    let mut lines: Vec<String> = source.split('\n').map(str::to_owned).collect();
    let masked = mask_code_and_destinations(source);
    let masks: Vec<_> = masked.split('\n').collect();
    let named = cached_regex!(r"^(:{3,})[ \t]+((?:[A-Za-z0-9_-]|\\[_-])+)[ \t]*\r?$").unwrap();
    let bare = cached_regex!(r"^:{3,}[ \t]*\r?$").unwrap();
    let run = cached_regex!(r"^:{3,}").unwrap();
    let mut classes = HashMap::new();
    let mut safe = lines.clone();
    for (line, text) in lines.iter().enumerate() {
        let at = djot_content_start(text);
        if masks[line].as_bytes().get(at) != Some(&b':') {
            continue;
        }
        let Some(fence) = named.captures(&text[at..]) else {
            continue;
        };
        let class = fence[2].replace("\\_", "_").replace("\\-", "-");
        if !class.starts_with('-')
            && class == fence[2]
            && class != "figure"
            && !crate::profile::ADMONITION_TIER1_KINDS.contains(&class.as_str())
            && !crate::ast::is_generated_content_kind(&class)
        {
            continue;
        }
        safe[line] = format!("{}{} carve-djot-div", &text[..at], &fence[1]);
        classes.insert(line, class);
    }
    if classes.is_empty() {
        return source.to_owned();
    }
    let document = crate::parse_with_options(
        &safe.join("\n"),
        &crate::Options::default().with_positions(true),
    );
    let mut pending: Vec<(&[BlockNode], Option<usize>)> = vec![(&document.children, None)];
    for children in document.footnote_defs.values() {
        pending.push((children, None));
    }
    let mut frames: Vec<Frame> = Vec::new();
    let mut sites: HashMap<usize, (usize, usize, Option<String>)> = HashMap::new();
    let mut mapped = HashSet::new();
    while let Some((blocks, parent)) = pending.pop() {
        for node in blocks {
            let pos = match node {
                BlockNode::Div(node) => node.pos.as_ref(),
                BlockNode::Admonition(node) => node.pos.as_ref(),
                BlockNode::Directive(node) => node.pos.as_ref(),
                _ => None,
            };
            let mut owner = parent;
            if let Some(pos) = pos {
                let line = pos.start_line.saturating_sub(1);
                if let Some(text) = lines.get(line) {
                    let at = djot_content_start(text);
                    if let Some(fence) = run.find(&text[at..]) {
                        let class = classes.get(&line).cloned();
                        let width = fence.as_str().len();
                        let index = frames.len();
                        frames.push(Frame {
                            source_width: width,
                            target_width: width,
                            parent,
                            bare: class.is_some() || bare.is_match(&text[at..]),
                        });
                        sites.insert(line, (at, index, class.clone()));
                        if class.is_some() {
                            mapped.insert(line);
                        }
                        let close_line = pos.end_line.saturating_sub(1);
                        if let Some(closer) = lines.get(close_line) {
                            let close_at = djot_content_start(closer);
                            if masks[close_line].as_bytes().get(close_at) == Some(&b':')
                                && bare.is_match(&closer[close_at..])
                                && run
                                    .find(&closer[close_at..])
                                    .is_some_and(|run| run.as_str().len() == width)
                            {
                                sites.insert(close_line, (close_at, index, None));
                            }
                        }
                        owner = Some(index);
                    }
                }
            }
            match node {
                BlockNode::Div(node) => pending.push((&node.children, owner)),
                BlockNode::Admonition(node) => pending.push((&node.children, owner)),
                BlockNode::Directive(node) => pending.push((&node.children, owner)),
                BlockNode::Section(node) => pending.push((&node.children, owner)),
                BlockNode::LineBlock(node) => pending.push((&node.children, owner)),
                BlockNode::BlockQuote(node) => pending.push((&node.children, None)),
                BlockNode::List(node) => {
                    for item in &node.items {
                        pending.push((&item.children, owner));
                    }
                }
                BlockNode::DefinitionList(node) => {
                    for item in &node.items {
                        for definition in &item.definitions {
                            pending.push((&definition.children, owner));
                        }
                    }
                }
                BlockNode::Table(node) => {
                    for row in &node.rows {
                        for cell in &row.cells {
                            if let Some(blocks) = &cell.blocks {
                                pending.push((blocks, None));
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }
    if mapped.len() != classes.len() {
        return source.to_owned();
    }
    for index in (0..frames.len()).rev() {
        if frames[index].bare {
            if let Some(parent) = frames[index].parent {
                if frames[parent].target_width <= frames[index].target_width {
                    frames[parent].target_width = frames[index].target_width + 1;
                }
            }
        }
    }
    let note = cached_regex!(r"\[\^[^\]\n]+\]:[ \t]*").unwrap();
    let task = cached_regex!(r"\[[ xX]\][ \t]+").unwrap();
    for (line, (at, index, class)) in sites {
        let prefix = &lines[line][..at];
        let fence = ":".repeat(frames[index].target_width);
        lines[line] = if let Some(class) = class {
            let prefix_continuation = note.replace_all(prefix, "  ");
            let prefix_continuation = task.replace_all(&prefix_continuation, "");
            let continuation: String = prefix_continuation
                .chars()
                .map(|ch| {
                    if matches!(ch, '>' | ' ' | '\t') {
                        ch
                    } else {
                        ' '
                    }
                })
                .collect();
            format!("{prefix}{{class=\"{class}\"}}\n{continuation}{fence}")
        } else {
            format!(
                "{prefix}{fence}{}",
                &lines[line][at + frames[index].source_width..]
            )
        };
    }
    lines.join("\n")
}
