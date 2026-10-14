use super::*;
use std::collections::HashMap;

fn starts_quote(text: &str) -> bool {
    text.starts_with('>')
        && text
            .as_bytes()
            .get(1)
            .map_or(true, |b| matches!(b, b' ' | b'\t'))
}

fn thematic(text: &str) -> bool {
    let bytes: Vec<_> = text.bytes().filter(|b| !b.is_ascii_whitespace()).collect();
    bytes.len() >= 3 && bytes.iter().all(|b| matches!(b, b'*' | b'-'))
}

fn separator(text: &str) -> bool {
    text.len() >= 2
        && text.starts_with('|')
        && text.ends_with('|')
        && text[1..text.len() - 1].split('|').all(|cell| {
            let cell = cell.trim();
            cell.contains('-') && cell.bytes().all(|b| matches!(b, b'-' | b':'))
        })
}

#[derive(Clone)]
struct List {
    source: usize,
    column: usize,
    target: usize,
    target_column: usize,
    written: u8,
    kind: String,
    start: usize,
    lazy: bool,
    loose: bool,
}

/// Djot attaches an attribute line with no block after it to nothing, so drop it.
fn drop_orphan_attribute_line(result: &mut Vec<String>) {
    let Some(last) = result.last() else { return };
    let last = last.trim_end_matches(DJOT_LINE_SPACING);
    let at = last.find(|c: char| !DJOT_LINE_SPACING.contains(&c));
    let Some(at) = at else { return };
    if !last[at..].starts_with('{') {
        return;
    }
    if read_djot_word_attributes(last, at).is_some_and(|(end, _)| end == last.len()) {
        result.pop();
    }
}

fn djot_list_kind(marker: &str) -> String {
    let label = cached_regex!(r"[0-9A-Za-z]+").unwrap();
    let Some(found) = label.find(marker) else {
        return marker.to_owned();
    };
    let word = found.as_str();
    let lower = word == word.to_ascii_lowercase();
    let mut mask = if word.bytes().all(|byte| byte.is_ascii_digit()) {
        1
    } else if lower {
        2
    } else {
        8
    };
    if cached_regex!(r"^[ivxlcdm]+$")
        .unwrap()
        .is_match(&word.to_ascii_lowercase())
    {
        mask = (if word.len() == 1 { mask } else { 0 }) | if lower { 4 } else { 16 };
    }
    format!("{mask}:{}", label.replace(marker, ""))
}

fn shared_djot_list_kind(prior: &str, current: &str) -> Option<String> {
    if prior == current {
        return Some(current.to_owned());
    }
    let (first, first_delimiter) = prior.split_once(':')?;
    let (second, second_delimiter) = current.split_once(':')?;
    if first_delimiter != second_delimiter {
        return None;
    }
    let common = first.parse::<u8>().ok()? & second.parse::<u8>().ok()?;
    (common != 0).then(|| format!("{common}:{first_delimiter}"))
}

fn deep_quoted_item_attributes(
    source: &str,
    attribute_prefix: &str,
    item_attributes: &mut Vec<String>,
) -> String {
    if !source.contains('{') || !source.contains('>') || attribute_prefix.is_empty() {
        return source.to_owned();
    }
    struct Frame {
        column: usize,
        start: usize,
        kind: String,
        attributes: Vec<(usize, String)>,
    }
    let _paragraph_cache = ParagraphCacheScope::new(source);
    let lines: Vec<_> = source.split('\n').collect();
    let mask = mask_code_and_destinations(source);
    let masks: Vec<_> = mask.split('\n').collect();
    let table_rows = if source.contains('|') {
        djot_table_rows(source, &mask)
    } else {
        vec![false; lines.len()]
    };
    let marker = cached_regex!(r"^([-*+]|(?:[0-9]+|[A-Za-z]|[ivxlcdm]+|[IVXLCDM]+)[.)]|\((?:[0-9]+|[A-Za-z]|[ivxlcdm]+|[IVXLCDM]+)\))[ \t]+").unwrap();
    let block = cached_regex!(
        r"^(?:[-*+][ \t]|(?:[0-9]+|[A-Za-z]|[ivxlcdm]+|[IVXLCDM]+)[.)][ \t]|#{1,6}(?:[ \t]|$)|:{3,}|[`~]{3,}|\[(?:\^)?[^\]\n]*\]:)"
    )
    .unwrap();
    let task = cached_regex!(r"^\[[ xX]\](?:[ \t]+|$)").unwrap();
    let prefix_part = cached_regex!(r"^[ \t]*>(?:[ \t]|$)").unwrap();
    let mut frames: HashMap<usize, Vec<Frame>> = HashMap::new();
    let mut before: HashMap<usize, Vec<String>> = HashMap::new();
    let mut removed = vec![false; lines.len()];
    let mut replacements = HashMap::<usize, String>::new();
    let mut offset = 0;
    let mut previous_depth = 0;
    let mut previous_paragraph = false;
    let mut literal_depth = None;
    for (row, line) in lines.iter().enumerate() {
        let mut prefix_ends = vec![0];
        while let Some(part) = prefix_part.find(&line[*prefix_ends.last().unwrap()..]) {
            prefix_ends.push(prefix_ends.last().unwrap() + part.end());
        }
        let depth = prefix_ends.len() - 1;
        let prefix_end = *prefix_ends.last().unwrap();
        let prefix = &line[..prefix_end];
        let body = &line[prefix_end..];
        let indent = body.len() - body.trim_start_matches([' ', '\t']).len();
        let text = &body[indent..];
        let visible = &masks[row][prefix_end + indent..];
        let at = offset + prefix_end + indent;
        offset += line.len() + 1;
        let opening_column = if depth > previous_depth {
            let rest = &prefix[prefix_ends[previous_depth]..];
            rest.len() - rest.trim_start_matches([' ', '\t']).len()
        } else {
            0
        };
        let ends_item = masks[row].as_bytes().get(prefix.rfind('>').unwrap_or(0)) == Some(&b'>')
            && frames
                .get(&previous_depth)
                .is_some_and(|owners| owners.iter().any(|owner| owner.column >= opening_column));
        if literal_depth.is_some_and(|held| depth <= held) {
            literal_depth = None;
        }
        if depth > previous_depth && previous_paragraph && !ends_item {
            literal_depth.get_or_insert(previous_depth);
        }
        frames.retain(|level, _| *level <= depth);
        if depth > previous_depth {
            if (ends_item || !previous_paragraph)
                && masks[row].as_bytes().get(prefix.rfind('>').unwrap_or(0)) == Some(&b'>')
            {
                if let Some(owners) = frames.get_mut(&previous_depth) {
                    owners.retain(|owner| owner.column < opening_column);
                }
            }
            if let Some(owner) = frames
                .get_mut(&previous_depth)
                .and_then(|stack| stack.last_mut())
            {
                owner.attributes.clear();
            }
        }
        let prior_depth = previous_depth;
        let prior_paragraph = previous_paragraph;
        previous_depth = depth;
        previous_paragraph = !text.is_empty()
            && !cached_regex!(r"^(?:`{3,}|~{3,})[ \t]*=?[A-Za-z0-9_+#.-]*[ \t]*$")
                .unwrap()
                .is_match(text)
            && !table_rows[row]
            && !thematic(visible)
            && !block.is_match(visible)
            && !(visible.starts_with('{')
                && read_djot_word_attributes(body, indent)
                    .is_some_and(|(end, _)| end == body.trim_end_matches(DJOT_LINE_SPACING).len()));
        if depth == 0 {
            if let Some(item) = marker.captures(visible) {
                let (quote_depth, quote_body) = quoted(&text[item[0].len()..]);
                if quote_depth > 0 {
                    previous_depth = quote_depth;
                    let quote_body = quote_body.trim_start_matches([' ', '\t']);
                    previous_paragraph = !quote_body.is_empty() && !block.is_match(quote_body);
                }
            }
        }
        if literal_depth.is_some_and(|held| depth > held) || depth == 0 || text.is_empty() {
            continue;
        }
        let mut stack = frames.remove(&depth).unwrap_or_default();
        let lazy = depth == prior_depth
            && prior_paragraph
            && previous_paragraph
            && !marker.is_match(visible);
        let mut closed_nested = false;
        while !lazy && stack.last().is_some_and(|frame| frame.column > indent) {
            stack.pop();
            closed_nested = true;
        }
        if let Some(item) = marker.captures(visible) {
            if closed_nested {
                let mut blank_row = row;
                while blank_row > 0
                    && quoted(lines[blank_row - 1]).0 == depth
                    && quoted(lines[blank_row - 1])
                        .1
                        .trim_matches([' ', '\t'])
                        .is_empty()
                {
                    blank_row -= 1;
                    removed[blank_row] = true;
                }
            }
            if depth == prior_depth
                && prior_paragraph
                && stack.last().map_or(true, |frame| indent > frame.column)
            {
                previous_paragraph = true;
                if !stack.is_empty() {
                    frames.insert(depth, stack);
                }
                continue;
            }
            let item_text = text[item[0].len()..].trim_start_matches([' ', '\t']);
            let native_task_marker = matches!(&item[0], "- " | "* " | "+ ");
            let item_text = if native_task_marker {
                task.find(item_text)
                    .map_or(item_text, |task| &item_text[task.end()..])
            } else {
                item_text
            };
            previous_paragraph = !item_text.is_empty()
                && !block.is_match(item_text)
                && !item_text.starts_with(['>', '|']);
            let mut kind = djot_list_kind(&item[1]);
            if native_task_marker && task.is_match(&text[item[0].len()..]) {
                kind.push_str(":task");
            }
            if let Some(owner) = stack.last_mut().filter(|owner| indent > owner.column) {
                owner.attributes.clear();
            }
            let frame = stack.last();
            let same = frame
                .filter(|frame| indent == frame.column)
                .and_then(|frame| shared_djot_list_kind(&frame.kind, &kind));
            let start = if same.is_some() {
                frame.unwrap().start
            } else {
                row
            };
            if let Some(frame) =
                frame.filter(|frame| depth >= 2 && same.is_some() && !frame.attributes.is_empty())
            {
                let wire = format!(
                    "{{{}}}",
                    frame
                        .attributes
                        .iter()
                        .map(|(_, wire)| &wire[1..wire.len() - 1])
                        .collect::<Vec<_>>()
                        .join(" ")
                );
                for (row, _) in &frame.attributes {
                    removed[*row] = true;
                }
                let token = format!("{attribute_prefix}{}\0", item_attributes.len());
                item_attributes.push(wire);
                let loose = lines[frame.attributes[0].0.saturating_sub(1)..row]
                    .iter()
                    .any(|line| quoted(line).1.trim_matches([' ', '\t']).is_empty());
                let mut held = vec![format!("{prefix}{}{token}", " ".repeat(indent))];
                if loose {
                    held.push(prefix.trim_end_matches([' ', '\t']).to_owned());
                }
                before.insert(row, held);
                for (owner_depth, owners) in frames.iter().chain(std::iter::once((&depth, &stack)))
                {
                    for owner in owners {
                        if !loose
                            || *owner_depth > depth
                            || *owner_depth == depth && owner.column != frame.column
                        {
                            continue;
                        }
                        let rest = &prefix[prefix_ends[*owner_depth]..];
                        let owner_indent = rest.len() - rest.trim_start_matches([' ', '\t']).len();
                        if *owner_depth < depth && owner_indent <= owner.column {
                            continue;
                        }
                        if let std::collections::hash_map::Entry::Vacant(entry) =
                            before.entry(owner.start)
                        {
                            let start_line = lines[owner.start];
                            let mut end = 0;
                            while let Some(part) = prefix_part.find(&start_line[end..]) {
                                end += part.end();
                            }
                            let start_prefix = &start_line[..end];
                            let token = format!("{attribute_prefix}B{}\0", item_attributes.len());
                            item_attributes.push("{loose}".to_owned());
                            entry.insert(vec![
                                format!("{start_prefix}{}{token}", " ".repeat(owner.column)),
                                start_prefix.trim_end_matches([' ', '\t']).to_owned(),
                            ]);
                        }
                    }
                }
            }
            if stack.last().is_some_and(|frame| frame.column == indent) {
                stack.pop();
            }
            stack.push(Frame {
                column: indent,
                start,
                kind: same.unwrap_or(kind),
                attributes: Vec::new(),
            });
            frames.insert(depth, stack);
        } else if visible.starts_with('{') {
            if depth >= 1 && stack.is_empty() && !prior_paragraph {
                if let Some((end, wire)) = read_djot_word_attributes(body, indent) {
                    if end == body.trim_end_matches(DJOT_LINE_SPACING).len() {
                        replacements.insert(row, format!("{prefix}{wire}"));
                        let mut next = row + 1;
                        while next < lines.len()
                            && lines[next].trim_end_matches(DJOT_LINE_SPACING)
                                == prefix.trim_end_matches([' ', '\t'])
                        {
                            removed[next] = true;
                            next += 1;
                        }
                    }
                }
            }
            if let Some(frame) = stack.last_mut() {
                if indent <= frame.column && !lazy {
                    stack.clear();
                } else if let Some((end, wire)) = read_djot_word_attributes(body, indent) {
                    if end == body.trim_end_matches(DJOT_LINE_SPACING).len() {
                        if depth >= 2 && !comment_continues_paragraph(source, at) {
                            frame.attributes.push((row, wire));
                        }
                    } else {
                        frame.attributes.clear();
                    }
                } else {
                    frame.attributes.clear();
                }
            }
            if !stack.is_empty() {
                frames.insert(depth, stack);
            }
        } else {
            if let Some(frame) = stack.last_mut() {
                frame.attributes.clear();
                if indent <= frame.column && !lazy {
                    stack.clear();
                }
            }
            if !stack.is_empty() {
                frames.insert(depth, stack);
            }
        }
    }
    let mut output = Vec::new();
    for (row, line) in lines.iter().enumerate() {
        if removed[row] {
            continue;
        }
        output.extend(before.remove(&row).unwrap_or_default());
        output.push(
            replacements
                .remove(&row)
                .unwrap_or_else(|| (*line).to_owned()),
        );
    }
    output.join("\n")
}

fn quoted_list_attribute_layout(
    source: &str,
    attribute_prefix: &str,
    item_attributes: &mut Vec<String>,
) -> String {
    if !source.contains('>') || !source.contains('{') || attribute_prefix.is_empty() {
        return source.to_owned();
    }
    let lines: Vec<_> = source.split('\n').collect();
    if !lines.iter().any(|line| {
        (line.starts_with('>') || quoted(line).0 > 0) && {
            let body = quoted(line).1;
            body.starts_with([' ', '\t'])
                && body.trim_start_matches(DJOT_LINE_SPACING).starts_with('{')
        }
    }) {
        return source.to_owned();
    }
    let _paragraph_cache = ParagraphCacheScope::new(source);
    let mut source_offset = 0;
    let starts: Vec<_> = lines
        .iter()
        .map(|line| {
            let start = source_offset;
            source_offset += line.len() + 1;
            start
        })
        .collect();
    let list_marker = cached_regex!(r"^(?:[-*+]|(?:[0-9]+|[A-Za-z]|[ivxlcdm]+|[IVXLCDM]+)[.)]|\((?:[0-9]+|[A-Za-z]|[ivxlcdm]+|[IVXLCDM]+)\))[ \t]+").unwrap();
    let prior_block = cached_regex!(r"^(?:#{1,6}(?:[ \t]|$)|:{3,}|[`~]{3,}|\[[^\]]+\]:)").unwrap();
    let mask = mask_code_and_destinations(source);
    let masks: Vec<_> = mask.split('\n').collect();
    let depths: Vec<_> = lines
        .iter()
        .map(|line| {
            if quoted(line).0 > 0 {
                quoted(line).0
            } else {
                0
            }
        })
        .collect();
    let mut ends = vec![lines.len(); lines.len()];
    let mut open = Vec::new();
    let mut next_attributes = HashMap::new();
    let mut next_lists = HashMap::new();
    let mut has_attributes = vec![None; lines.len()];
    let mut has_lists = vec![None; lines.len()];
    for n in (0..lines.len()).rev() {
        while open.last().is_some_and(|&at| depths[at] >= depths[n]) {
            open.pop();
        }
        ends[n] = open.last().copied().unwrap_or(lines.len());
        open.push(n);
        let body = quoted(lines[n]).1;
        let text = body.trim_start_matches(DJOT_LINE_SPACING);
        if body.len() > text.len()
            && text.starts_with('{')
            && read_djot_word_attributes(text, 0)
                .is_some_and(|(end, _)| end == text.trim_end_matches(DJOT_LINE_SPACING).len())
        {
            next_attributes.insert(depths[n], n);
        }
        if list_marker.is_match(text) {
            next_lists.insert(depths[n], n);
        }
        has_attributes[n] = next_attributes.get(&depths[n]).copied();
        has_lists[n] = next_lists.get(&depths[n]).copied();
    }
    let mut result = Vec::new();
    let mut n = 0;
    while n < lines.len() {
        let depth = depths[n];
        let previous_body = n
            .checked_sub(1)
            .map_or("", |row| quoted(lines[row]).1.trim_start());
        let lazy_quote = depth > 0
            && n > 0
            && depths[n - 1] < depth
            && ((!previous_body.is_empty()
                && !prior_block.is_match(previous_body)
                && !list_marker.is_match(previous_body)
                && !thematic(previous_body)
                && !read_djot_word_attributes(previous_body, 0).is_some_and(|(end, _)| {
                    end == previous_body.trim_end_matches(DJOT_LINE_SPACING).len()
                }))
                || comment_continues_paragraph(
                    source,
                    starts[n]
                        + lines[n][..lines[n].len() - quoted(lines[n]).1.len()]
                            .rfind('>')
                            .unwrap_or(0),
                ));
        if lazy_quote {
            let parent_depth = depths[n - 1];
            let mut opening = 0;
            for _ in 0..parent_depth {
                let rest = &lines[n][opening..];
                opening += rest.len() - rest.trim_start_matches([' ', '\t']).len() + 1;
                if lines[n]
                    .as_bytes()
                    .get(opening)
                    .is_some_and(|byte| *byte == b' ' || *byte == b'\t')
                {
                    opening += 1;
                }
            }
            opening += lines[n][opening..].len()
                - lines[n][opening..].trim_start_matches([' ', '\t']).len();
            let literal_empty = previous_body.starts_with('|')
                && quoted(lines[n])
                    .1
                    .trim_matches(DJOT_LINE_SPACING)
                    .is_empty()
                && masks[n].as_bytes().get(opening) == Some(&b'>')
                && comment_continues_paragraph(source, starts[n] + opening);
            while n < lines.len() && depths[n] >= depth {
                let mut line = lines[n].to_owned();
                if literal_empty {
                    let mut at = 0;
                    for _ in 0..parent_depth {
                        let rest = &line[at..];
                        at += rest.len() - rest.trim_start_matches([' ', '\t']).len() + 1;
                        if line
                            .as_bytes()
                            .get(at)
                            .is_some_and(|byte| *byte == b' ' || *byte == b'\t')
                        {
                            at += 1;
                        }
                    }
                    at += line[at..].len() - line[at..].trim_start_matches([' ', '\t']).len();
                    if masks[n].as_bytes().get(at) == Some(&b'>') {
                        line.insert(at, '\\');
                    }
                }
                result.push(line);
                n += 1;
            }
            continue;
        }
        if depth == 0
            || quoted(lines[n]).0 == 0
            || masks[n].as_bytes().get(lines[n].find('>').unwrap_or(0)) != Some(&b'>')
            || (n > 0
                && !lines[n - 1].trim_matches(DJOT_LINE_SPACING).is_empty()
                && quoted(lines[n - 1]).0 == 0
                && !prior_block.is_match(lines[n - 1].trim_start_matches(DJOT_LINE_SPACING))
                && !thematic(lines[n - 1])
                && !read_djot_word_attributes(lines[n - 1], 0).is_some_and(|(end, _)| {
                    end == lines[n - 1].trim_end_matches(DJOT_LINE_SPACING).len()
                }))
        {
            result.push(lines[n].to_owned());
            n += 1;
            continue;
        }
        let start = n;
        let end = ends[n];
        if !has_attributes[n].is_some_and(|at| at < end) || !has_lists[n].is_some_and(|at| at < end)
        {
            result.push(lines[n].to_owned());
            n += 1;
            continue;
        }
        n = end;
        let bodies: Vec<_> = lines[start..n]
            .iter()
            .map(|line| {
                let mut body = *line;
                for _ in 0..depth {
                    body = body.trim_start_matches([' ', '\t']);
                    body = &body[1..];
                    if body.starts_with([' ', '\t']) {
                        body = &body[1..];
                    }
                }
                body
            })
            .collect();
        let has_list_attributes = bodies.iter().any(|body| {
            let text = body.trim_start_matches(DJOT_LINE_SPACING);
            body.len() > text.len()
                && text.starts_with('{')
                && read_djot_word_attributes(text, 0)
                    .is_some_and(|(end, _)| end == text.trim_end_matches(DJOT_LINE_SPACING).len())
        }) && bodies.iter().any(|body| {
            let text = body.trim_start_matches(DJOT_LINE_SPACING);
            list_marker.is_match(text)
        });
        if has_list_attributes {
            let mut returned = Vec::new();
            let normalized = blocks_with_slot_inner(
                &bodies.join("\n"),
                None,
                None,
                attribute_prefix,
                item_attributes,
                source.contains("\\|"),
                Some(&mut returned),
            );
            let prefix = cached_regex!(r"^(?:[ \t]*>(?:[ \t]|$))*")
                .unwrap()
                .find(lines[start])
                .unwrap()
                .as_str();
            result.extend(normalized.split('\n').map(|body| format!("{prefix}{body}")));
            if n < lines.len() {
                let returned_depth = depths[n].min(depth.saturating_sub(1));
                let mut parent_end = 0;
                for _ in 0..returned_depth {
                    parent_end += cached_regex!(r"^[ \t]*>(?:[ \t]|$)")
                        .unwrap()
                        .find(&prefix[parent_end..])
                        .unwrap()
                        .end();
                }
                parent_end += prefix[parent_end..].len()
                    - prefix[parent_end..].trim_start_matches([' ', '\t']).len();
                for attributes in returned {
                    result.push(format!("{}{attributes}", &prefix[..parent_end]));
                }
            }
        } else {
            result.push(lines[start].to_owned());
            n = start + 1;
        }
    }
    result.join("\n")
}

fn compact_djot_list_prefix(text: &str) -> std::borrow::Cow<'_, str> {
    compact_djot_list_prefix_with_bullets(text, false)
}

fn compact_djot_list_prefix_with_bullets(
    text: &str,
    canonical_bullets: bool,
) -> std::borrow::Cow<'_, str> {
    if !canonical_bullets
        && !text.contains('0')
        && !text.contains('(')
        && !text.contains('\t')
        && !text.contains('[')
        && !text.contains('+')
    {
        return std::borrow::Cow::Borrowed(text);
    }
    let marker = cached_regex!(r"^(?:[-*+]|(?:[0-9]+|[A-Za-z]|[ivxlcdm]+|[IVXLCDM]+)[.)]|\((?:[0-9]+|[A-Za-z]|[ivxlcdm]+|[IVXLCDM]+)\))[ \t]+").unwrap();
    let quote = cached_regex!(r"^>(?:[ \t]|$)").unwrap();
    let task = cached_regex!(r"^\[[ xX\-_>?]\](?:[ \t]+|$)").unwrap();
    let mut result = String::with_capacity(text.len());
    let mut at = 0;
    let mut changed = false;
    while at < text.len() {
        let indent = text[at..]
            .bytes()
            .take_while(|b| matches!(b, b' ' | b'\t'))
            .count();
        result.push_str(&text[at..at + indent]);
        at += indent;
        if let Some(prefix) = quote.find(&text[at..]) {
            result.push_str(prefix.as_str());
            at += prefix.end();
            continue;
        }
        let Some(item) = marker.find(&text[at..]) else {
            break;
        };
        let prefix = item.as_str();
        changed |= prefix.contains('\t');
        let start = usize::from(prefix.starts_with('('));
        let mut end = start;
        while prefix.as_bytes().get(end) == Some(&b'0')
            && prefix
                .as_bytes()
                .get(end + 1)
                .is_some_and(u8::is_ascii_digit)
        {
            end += 1;
        }
        changed |= end > start;
        if prefix.starts_with('(') {
            let close = prefix.find(')').unwrap();
            result.push_str(&prefix[end..close]);
            result.push('.');
            result.push_str(&prefix[close + 1..].replace('\t', " "));
            changed = true;
        } else if prefix.starts_with('+') || canonical_bullets && prefix.starts_with(['-', '*']) {
            result.push('-');
            result.push_str(&prefix[1..].replace('\t', " "));
            changed = true;
        } else {
            result.push_str(&prefix[end..].replace('\t', " "));
        }
        at += item.end();
        if matches!(prefix.as_bytes()[0], b'-' | b'*' | b'+') {
            if let Some(prefix) = task.find(&text[at..]) {
                let accepted = item.as_str().len() == 2
                    && item.as_str().as_bytes()[1] == b' '
                    && matches!(prefix.as_str().as_bytes()[1], b' ' | b'x' | b'X');
                if accepted {
                    changed |= prefix.as_str().len() > 3 && &prefix.as_str()[3..] != " ";
                    result.push_str(&prefix.as_str()[..3]);
                    if prefix.as_str().len() > 3 {
                        result.push(' ');
                    }
                } else {
                    changed = true;
                    result.push('\\');
                    result.push_str(prefix.as_str());
                }
                at += prefix.end();
                if !accepted {
                    break;
                }
            }
        }
    }
    result.push_str(&text[at..]);
    if changed {
        std::borrow::Cow::Owned(result)
    } else {
        std::borrow::Cow::Borrowed(text)
    }
}

fn quoted_list_boundaries<'a>(
    source: &'a str,
    mask: &str,
    fence_mask: &str,
    rows: &[bool],
    slot_prefix: Option<&str>,
    has_definition_syntax: bool,
    mut literal_quotes: Option<&mut HashMap<usize, usize>>,
) -> std::borrow::Cow<'a, str> {
    if !source.contains('>')
        || !source.contains('\n') && !source.contains('\t') && !source.contains('[')
    {
        return std::borrow::Cow::Borrowed(source);
    }
    let quote = cached_regex!(r"^(?:[ \t]*>(?:[ \t]|$))*").unwrap();
    let marker = cached_regex!(r"^((?:[-*+]|(?:[0-9]+|[A-Za-z]|[ivxlcdm]+|[IVXLCDM]+)[.)]|\((?:[0-9]+|[A-Za-z]|[ivxlcdm]+|[IVXLCDM]+)\))[ \t]+)(\[[ xX]\][ \t]+)?([^ \t])").unwrap();
    let embedded = cached_regex!(r"^(?:>(?:[ \t]+|$))+").unwrap();
    let heading = cached_regex!(r"^#{1,6}(?:[ \t]|$)").unwrap();
    let mut insertions = Vec::new();
    let mut owned_divs: Vec<(usize, usize, usize)> = Vec::new();
    let mut div_closers = HashMap::new();
    if source.contains(":::") {
        djot_inline_boundaries_with_div_closers(source, mask, true, Some(&mut div_closers), None);
    }
    let reference = cached_regex!(r"^\[[^\]\n]*\]:").unwrap();
    let mut offset = 0;
    let mut depth = 0;
    let mut boundary = String::new();
    let mut paragraph = false;
    let mut previous_blank = true;
    let mut lists: Vec<(usize, usize, usize, usize, usize, usize)> = Vec::new();
    for (row_index, ((line, masked), fenced)) in source
        .split('\n')
        .zip(mask.split('\n'))
        .zip(fence_mask.split('\n'))
        .enumerate()
    {
        if slot_prefix.is_some_and(|prefix| line.starts_with(prefix)) {
            paragraph = false;
            previous_blank = true;
            offset += line.len() + 1;
            continue;
        }
        let line_offset = offset;
        let mut item_edit = None;
        let mut item_target = None;
        offset += line.len() + 1;
        if fenced != line {
            if line.starts_with(['`', '~']) {
                lists.clear();
            }
            depth = 0;
            paragraph = false;
            previous_blank = true;
            continue;
        }
        let prefix = quote.find(line).unwrap().as_str();
        let quote_depth = prefix.bytes().filter(|&b| b == b'>').count();
        let mut columns = Vec::new();
        let mut bases = Vec::new();
        let mut positions = Vec::new();
        let bytes = line.as_bytes();
        let mut at = 0;
        for level in 0..=quote_depth {
            let start = at;
            bases.push(start);
            while bytes.get(at).is_some_and(|b| matches!(b, b' ' | b'\t')) {
                at += 1;
            }
            columns.push(at - start);
            if level < quote_depth {
                positions.push(at);
                at += 1;
                if bytes.get(at).is_some_and(|b| matches!(b, b' ' | b'\t')) {
                    at += 1;
                }
            }
        }
        if positions
            .first()
            .is_some_and(|&at| masked.as_bytes().get(at) != Some(&b'>'))
        {
            previous_blank = false;
            continue;
        }
        let remainder = &line[prefix.len()..];
        let raw_indent = remainder.len() - remainder.trim_start_matches([' ', '\t']).len();
        let source_prefix = lists
            .last()
            .filter(|&&(owner_depth, _, _, _, _, _)| owner_depth == quote_depth)
            .map_or(prefix.len(), |&(_, _, _, source_prefix, _, _)| {
                source_prefix
            });
        let indent = (raw_indent + prefix.len()).saturating_sub(source_prefix);
        columns[quote_depth] = indent;
        let column_at = |owner_depth: usize, owner_prefix: usize| {
            if quote_depth > owner_depth {
                (columns[owner_depth] + bases[owner_depth]).saturating_sub(owner_prefix)
            } else {
                indent
            }
        };
        let text = &remainder[raw_indent..];
        let visible = masked.get(prefix.len() + raw_indent..).unwrap_or("");

        if let Some(caps) = cached_regex!(r"^\x00DJOTITEMATTR\x00(?:[0-9]+\x00)?(B?)[0-9]+\x00$")
            .unwrap()
            .captures(text)
        {
            paragraph = false;
            previous_blank = &caps[1] == "B";
            continue;
        }
        if previous_blank && !line.trim_matches([' ', '\t']).is_empty() && !text.is_empty() {
            if depth > quote_depth
                && !paragraph
                && !marker.is_match(visible)
                && lists.last().is_some_and(
                    |&(owner_depth, column, _width, _source_prefix, _, _)| {
                        owner_depth > quote_depth
                            || owner_depth == quote_depth
                                && column_at(owner_depth, _source_prefix) < column + 1
                    },
                )
            {
                insertions.push((line_offset, line_offset, format!("{}\n", prefix.trim_end())));
            }
            while lists.last().is_some_and(
                |&(owner_depth, column, _width, _source_prefix, _, _)| {
                    owner_depth > quote_depth
                        || owner_depth == quote_depth
                            && column_at(owner_depth, _source_prefix) < column + 1
                },
            ) {
                lists.pop();
            }
        }
        if text.is_empty() {
            while lists
                .last()
                .is_some_and(|&(owner_depth, _, _, _, _, _)| owner_depth > quote_depth)
            {
                lists.pop();
            }
        }
        let ends_item =
            lists
                .last()
                .is_some_and(|&(owner_depth, column, _, _source_prefix, _, _)| {
                    owner_depth > quote_depth || owner_depth == quote_depth && indent <= column
                });
        if thematic(visible)
            || heading.is_match(visible)
            || visible.starts_with(":::")
            || visible.starts_with('|')
        {
            while lists
                .last()
                .is_some_and(|&(owner_depth, column, _, _source_prefix, _, _)| {
                    owner_depth > quote_depth || owner_depth == quote_depth && indent <= column
                })
            {
                lists.pop();
            }
        }
        if let Some(index) = lists
            .iter()
            .take_while(|&&(owner_depth, _, _, _, _, _)| owner_depth < quote_depth)
            .position(|&(owner_depth, column, _, owner_prefix, _, _)| {
                owner_depth < quote_depth && column_at(owner_depth, owner_prefix) <= column
            })
        {
            let owner_depth = lists[index].0;
            if paragraph && !previous_blank && !(owner_depth == 0 && positions.first() == Some(&0))
            {
                let end = boundary
                    .match_indices('>')
                    .nth(owner_depth)
                    .map_or(boundary.len(), |(at, _)| at + 1);
                insertions.push((line_offset, line_offset, format!("{}\n", &boundary[..end])));
            }
            lists.truncate(index);
            paragraph = false;
        }
        let outside_item =
            lists
                .last()
                .is_some_and(|&(owner_depth, column, _width, _source_prefix, _, _)| {
                    quote_depth > owner_depth && column_at(owner_depth, _source_prefix) < column + 1
                });
        if quote_depth < depth
            && visible.starts_with('{')
            && read_djot_word_attributes(text, 0)
                .is_some_and(|(end, _)| end == visible.trim_end_matches(DJOT_LINE_SPACING).len())
        {
            depth = quote_depth;
            boundary = prefix.trim_end().to_owned();
            paragraph = false;
        }
        if quote_depth > depth && paragraph && !outside_item {
            if let Some(depths) = literal_quotes.as_deref_mut() {
                depths.insert(line_offset, depth);
            }
            let mut owned = vec![None; depth];
            for &(owner_depth, column, width, _source_prefix, target, _) in lists
                .iter()
                .take_while(|&&(owner_depth, _, _, _, _, _)| owner_depth < depth)
            {
                if owner_depth < depth && column_at(owner_depth, _source_prefix) > column {
                    owned[owner_depth] = Some(target + width);
                }
            }
            let mut normalized = String::new();
            for column in owned {
                normalized.push_str(&" ".repeat(column.unwrap_or(0)));
                normalized.push_str("> ");
            }
            normalized.push('\\');
            normalized.push_str(&prefix[positions[depth]..]);
            insertions.push((line_offset, line_offset + prefix.len(), normalized));
            continue;
        }
        owned_divs.retain(|&(held_depth, held_indent, _)| {
            quote_depth >= held_depth
                && (text.is_empty() || quote_depth > held_depth || indent >= held_indent)
        });
        let div_width = visible.bytes().take_while(|&byte| byte == b':').count();
        let div_owner = (div_width >= 3
            && visible[div_width..].trim_matches([' ', '\t']).is_empty())
        .then(|| {
            owned_divs
                .iter()
                .position(|&(held_depth, _, width)| held_depth == quote_depth && div_width >= width)
        })
        .flatten();
        let div_close = div_owner.is_some();
        if let Some(owner) = div_owner {
            owned_divs.truncate(owner);
        } else if !paragraph
            && cached_regex!(r"^(:{3,})(?:[ \t]*[A-Za-z0-9_-]+)?[ \t]*$")
                .unwrap()
                .is_match(visible)
        {
            owned_divs.push((quote_depth, indent, div_width));
        }
        if paragraph
            && !outside_item
            && !ends_item
            && !div_closers.contains_key(&line_offset)
            && !div_close
            && quote_depth == depth
            && quote_depth > 0
            && (heading.is_match(visible) || thematic(visible) || visible.starts_with(":::"))
        {
            if !has_definition_syntax {
                let mut owned = vec![None; quote_depth];
                for &(owner_depth, column, width, _source_prefix, target, _) in lists
                    .iter()
                    .take_while(|&&(owner_depth, _, _, _, _, _)| owner_depth < quote_depth)
                {
                    if owner_depth < quote_depth && column_at(owner_depth, _source_prefix) > column
                    {
                        owned[owner_depth] = Some(target + width);
                    }
                }
                let mut normalized = String::new();
                for (level, column) in owned.into_iter().enumerate() {
                    normalized.push_str(&" ".repeat(column.unwrap_or(0)));
                    normalized.push('>');
                    if level + 1 < quote_depth || !prefix.ends_with('>') {
                        normalized.push(' ');
                    }
                }
                if normalized != prefix {
                    insertions.push((line_offset, line_offset + prefix.len(), normalized));
                }
            }
            insertions.push((
                line_offset + prefix.len() + raw_indent,
                line_offset + prefix.len() + raw_indent,
                "\\".to_owned(),
            ));
            continue;
        }
        if outside_item {
            while lists.last().is_some_and(
                |&(owner_depth, column, _width, _source_prefix, _, _)| {
                    quote_depth > owner_depth && column_at(owner_depth, _source_prefix) < column + 1
                },
            ) {
                lists.pop();
            }
            paragraph = false;
        }
        let literal_item = paragraph
            && !previous_blank
            && quote_depth == depth
            && !thematic(visible)
            && marker.is_match(visible)
            && !lists
                .last()
                .is_some_and(|&(owner_depth, column, _, _, _, _)| {
                    owner_depth == quote_depth && indent <= column
                });
        previous_blank =
            line.trim_matches([' ', '\t']).is_empty() || quote_depth > 0 && text.is_empty();
        if line.trim_matches([' ', '\t']).is_empty() {
            while lists
                .last()
                .is_some_and(|&(owner_depth, _, _, _, _, _)| owner_depth > quote_depth)
            {
                lists.pop();
            }
            depth = 0;
            paragraph = false;
        } else if let Some(item) = marker.captures(visible).filter(|_| {
            !thematic(visible)
                && (!paragraph
                    || quote_depth < depth
                    || lists.last().is_some_and(
                        |&(owner_depth, column, _, _source_prefix, _, _)| {
                            owner_depth == quote_depth && indent <= column
                        },
                    ))
        }) {
            while lists
                .last()
                .is_some_and(|&(owner_depth, column, _, _source_prefix, _, _)| {
                    owner_depth > quote_depth || owner_depth == quote_depth && column > indent
                })
            {
                lists.pop();
            }
            if quote_depth < depth {
                if paragraph
                    && !lists.last().is_some_and(
                        |&(owner_depth, column, _, _source_prefix, _, _)| {
                            owner_depth == quote_depth && indent <= column
                        },
                    )
                {
                    let end = boundary
                        .match_indices('>')
                        .nth(quote_depth)
                        .map_or(boundary.len(), |(at, _)| at + 1);
                    insertions.push((line_offset, line_offset, format!("{}\n", &boundary[..end])));
                }
                depth = quote_depth;
                boundary = prefix.trim_end().to_owned();
            }
            let width = compact_djot_list_prefix(item.get(1).unwrap().as_str()).len();
            let compact = compact_djot_list_prefix(text);
            if compact != text {
                item_edit = Some((
                    line_offset + prefix.len() + raw_indent,
                    line_offset + line.len(),
                    compact.into_owned(),
                ));
            }
            if !lists
                .last()
                .is_some_and(|&(owner_depth, column, _, _source_prefix, _, _)| {
                    owner_depth == quote_depth && column == indent
                })
            {
                let target = lists
                    .last()
                    .filter(|&&(owner_depth, column, _, _, _, _)| {
                        owner_depth == quote_depth && indent > column
                    })
                    .map_or(0, |&(_, _, width, _, target, _)| target + width);
                lists.push((
                    quote_depth,
                    indent,
                    width,
                    source_prefix,
                    target,
                    item.get(1).unwrap().as_str().len(),
                ));
            }
            item_target = lists.last().map(|&(_, _, _, _, target, _)| target);
            let body_start = if item.get(2).is_some()
                && !matches!(item.get(1).unwrap().as_str(), "- " | "* " | "+ ")
            {
                item.get(1).unwrap().end()
            } else {
                item.get(3).unwrap().start()
            };
            let mut quote_body = &text[body_start..];
            let mut nested_depth = 0;
            let mut nested_prefix = String::new();
            while let Some(nested) = embedded.find(quote_body) {
                nested_depth += nested.as_str().bytes().filter(|&b| b == b'>').count();
                nested_prefix.push_str(nested.as_str());
                quote_body = &quote_body[nested.end()..];
                let Some(child) = marker.captures(quote_body) else {
                    break;
                };
                let child_width = compact_djot_list_prefix(child.get(1).unwrap().as_str()).len();
                let column = nested
                    .as_str()
                    .bytes()
                    .rev()
                    .take_while(|b| matches!(b, b' ' | b'\t'))
                    .count()
                    .saturating_sub(1);
                lists.push((
                    quote_depth + nested_depth,
                    column,
                    child_width,
                    line.len() - quote_body.len() - column,
                    column,
                    child.get(1).unwrap().as_str().len(),
                ));
                let child_body = if child.get(2).is_some()
                    && !matches!(child.get(1).unwrap().as_str(), "- " | "* " | "+ ")
                {
                    child.get(1).unwrap().end()
                } else {
                    child.get(3).unwrap().start()
                };
                if !embedded.is_match(&quote_body[child_body..]) {
                    break;
                }
                nested_prefix.push_str(&" ".repeat(child_width));
                quote_body = &quote_body[child_body..];
            }
            if nested_depth > 0 {
                depth = quote_depth
                    + if quote_body.is_empty() {
                        0
                    } else {
                        nested_depth
                    };
                boundary = format!(
                    "{prefix}{}{}",
                    " ".repeat(item_target.unwrap_or(0) + width),
                    nested_prefix.trim_end()
                );
            } else if quote_depth > 0 {
                depth = quote_depth;
                boundary = prefix.trim_end().to_owned();
            }
            paragraph = !(quote_body.is_empty()
                || quote_body.trim_end_matches([' ', '\t']) == "+"
                || reference.is_match(quote_body)
                || thematic(quote_body)
                || quote_body.starts_with(":::")
                || heading.is_match(quote_body)
                || read_djot_word_attributes(quote_body, 0)
                    .is_some_and(|(end, _)| end == quote_body.len()));
        } else if quote_depth > 0 && (!paragraph || depth > 0) {
            if quote_depth >= depth || text.is_empty() || !paragraph {
                depth = quote_depth;
                boundary = prefix.trim_end().to_owned();
            }
            paragraph = !(text.is_empty()
                || reference.is_match(text)
                || (!paragraph
                    && visible.starts_with('{')
                    && read_djot_word_attributes(text, 0).is_some_and(|(end, _)| {
                        end == visible.trim_end_matches(DJOT_LINE_SPACING).len()
                    }))
                || thematic(visible)
                || visible.starts_with(":::")
                || visible.starts_with('|')
                || rows[row_index]
                || heading.is_match(visible));
        } else if reference.is_match(text)
            || heading.is_match(visible)
            || (!paragraph
                && visible.starts_with('{')
                && read_djot_word_attributes(text, 0).is_some_and(|(end, _)| {
                    end == visible.trim_end_matches(DJOT_LINE_SPACING).len()
                }))
            || visible.starts_with('|')
            || visible.starts_with(":::")
            || thematic(visible)
        {
            depth = quote_depth;
            paragraph = false;
        } else {
            if !paragraph {
                depth = quote_depth;
            }
            paragraph = true;
        }
        if quote_depth > 0 && !has_definition_syntax {
            let mut owned_columns = vec![None; quote_depth];
            for &(owner_depth, column, width, _source_prefix, target, _) in lists
                .iter()
                .take_while(|&&(owner_depth, _, _, _, _, _)| owner_depth < quote_depth)
            {
                if owner_depth < quote_depth && column_at(owner_depth, _source_prefix) > column {
                    owned_columns[owner_depth] = Some(target + width);
                }
            }
            let mut normalized_prefix = String::new();
            for (level, owned_column) in owned_columns.into_iter().enumerate() {
                let column = owned_column.unwrap_or(0);
                normalized_prefix.push_str(&" ".repeat(column));
                normalized_prefix.push('>');
                if level + 1 < quote_depth || !prefix.ends_with('>') {
                    normalized_prefix.push(' ');
                }
            }
            if normalized_prefix != prefix {
                if boundary.trim_end() == prefix.trim_end() {
                    boundary = normalized_prefix.trim_end().to_owned();
                } else if let Some(suffix) = boundary.strip_prefix(prefix) {
                    boundary = normalized_prefix.clone() + suffix;
                }
                insertions.push((line_offset, line_offset + prefix.len(), normalized_prefix));
            }
        }
        let native_indent = item_target.unwrap_or_else(|| {
            lists
                .last()
                .filter(|&&(owner_depth, column, _, _, _, _)| {
                    owner_depth == quote_depth && indent > column
                })
                .map_or(indent, |&(_, column, width, _, target, source_width)| {
                    target + width + indent.saturating_sub(column + source_width)
                })
        });
        if quote_depth > 0 && raw_indent != native_indent && !rows[row_index] {
            insertions.push((
                line_offset + prefix.len(),
                line_offset + prefix.len() + raw_indent,
                " ".repeat(native_indent),
            ));
        }
        if rows[row_index]
            && quote_depth > 0
            && !has_definition_syntax
            && !lists
                .iter()
                .any(|&(owner_depth, _, _, _, _, _)| owner_depth >= quote_depth)
            && indent > 0
        {
            insertions.push((
                line_offset + prefix.len(),
                line_offset + prefix.len() + raw_indent,
                String::new(),
            ));
        }
        if literal_item {
            let at = if matches!(text.as_bytes().first(), Some(b'-' | b'*' | b'+')) {
                0
            } else {
                text.find(['.', ')']).unwrap()
            };
            insertions.push((
                line_offset + prefix.len() + raw_indent + at,
                line_offset + prefix.len() + raw_indent + at,
                "\\".to_owned(),
            ));
        }
        if let Some(edit) = item_edit {
            insertions.push(edit);
        }
    }
    if insertions.is_empty() {
        return std::borrow::Cow::Borrowed(source);
    }
    let mut result = String::with_capacity(source.len());
    let mut start = 0;
    for (at, end, text) in insertions {
        result.push_str(&source[start..at]);
        result.push_str(&text);
        start = end;
    }
    result.push_str(&source[start..]);
    std::borrow::Cow::Owned(result)
}

struct NoteBuffer {
    depth: usize,
    column: usize,
    label: String,
    body: Vec<String>,
    converted_body: Option<String>,
    carry_task_attributes: bool,
    paragraph: bool,
    blank: bool,
    paragraph_depth: usize,
    item_column: Option<usize>,
    slot: Option<String>,
}

fn normalize_note_body(
    note: &mut NoteBuffer,
    empty_note: &str,
    attribute_prefix: &str,
    item_attributes: &mut Vec<String>,
) -> Vec<String> {
    let mut body_lines = note.body.clone();
    let mut returned = Vec::new();
    if note.carry_task_attributes {
        if let Some(first) = body_lines.first_mut() {
            let (_, indent) = leading_indent(first);
            if let Some(end) = empty_djot_task_tail_end(&first[indent..], true) {
                returned.push(first[indent + end..].trim().to_owned());
                first.truncate(indent + end);
            }
        }
    }
    let allow_attributes = body_lines
        .get(1)
        .map_or(true, |line| line.trim().is_empty());
    if let Some(first) = body_lines.first_mut() {
        let (_, indent) = leading_indent(first);
        let text = &first[indent..];
        if let Some(end) = empty_djot_task_tail_end(text, allow_attributes) {
            let keep_star = text.starts_with("* ");
            let mut compact =
                compact_djot_list_prefix_with_bullets(text[..end].trim_end(), true).into_owned();
            if keep_star {
                compact.replace_range(..1, "*");
            }
            *first = format!("{}{compact}", &first[..indent]);
        }
    }
    let body = normalize_djot_fences(&body_lines.join("\n"));
    let body = convert_djot_block_markers_with(&body, true);
    let deep = deep_quoted_item_attributes(&body, attribute_prefix, item_attributes);
    let quoted = quoted_list_attribute_layout(&deep, attribute_prefix, item_attributes);
    note.converted_body = Some(super::convert_djot_block_markers(&blocks_with_slot_inner(
        &quoted,
        None,
        Some(empty_note),
        attribute_prefix,
        item_attributes,
        body.contains("\\|"),
        Some(&mut returned),
    )));
    returned
}

pub(super) fn note_blocks(
    source: &str,
    metadata: &DjotFootnoteAttributeStrip,
    empty_note: &str,
    attribute_prefix: &str,
    item_attributes: &mut Vec<String>,
) -> String {
    if !source.contains("[^") {
        return blocks_with_slot(source, None, None, attribute_prefix, item_attributes);
    }
    let lines: Vec<_> = source.split('\n').collect();
    let quote = cached_regex!(r"^(?:[ \t]*>(?:[ \t]|$))*").unwrap();
    let header = cached_regex!(r"^(\[\^[^\]\n]+\]:[ \t]*)(.*)$").unwrap();
    let first_header = cached_regex!(r"^\[\^[^\]\n]+\]:[ \t]*").unwrap();
    let item = cached_regex!(r"^(?:[-*+]|(?:[0-9]+|[A-Za-z]|[ivxlcdm]+|[IVXLCDM]+)[.)]|\((?:[0-9]+|[A-Za-z]|[ivxlcdm]+|[IVXLCDM]+)\))[ \t]+").unwrap();
    let inline_header = cached_regex!(r"^((?:[-*+]|(?:[0-9]+|[A-Za-z]|[ivxlcdm]+|[IVXLCDM]+)[.)]|\((?:[0-9]+|[A-Za-z]|[ivxlcdm]+|[IVXLCDM]+)\))[ \t]+)((?:>[ \t](?:[ \t]*>[ \t])*)?)[ \t]*(\[\^[^\]\n]+\]:[ \t]*)(.*)$").unwrap();
    let block = cached_regex!(r"^(?:#{1,6}(?:[ \t]|$)|:{3,}|[|]|[`~]{3,}|[-*+][ \t]|(?:[0-9]+|[A-Za-z]|[ivxlcdm]+|[IVXLCDM]+)[.)][ \t]|\((?:[0-9]+|[A-Za-z]|[ivxlcdm]+|[IVXLCDM]+)\)[ \t]|\[[^\]^\n][^\]\n]*\]:)").unwrap();
    let code = cached_regex!(r"^(`{3,}|~{3,})[ \t]*[A-Za-z0-9_+#.-]*$").unwrap();
    let mask = mask_code_and_destinations(source);
    let rows = djot_table_rows(source, &mask);
    let fence_mask = mask_djot_fences(source, None, &rows, true);
    let slot_prefix = djot_placeholder_prefix(source, "\0DJOTNOTESLOT\0");
    let mut main = Vec::<String>::new();
    let mut notes = Vec::<NoteBuffer>::new();
    let mut completed = Vec::<NoteBuffer>::new();
    let mut paragraph = false;
    let mut blank = true;
    let mut previous_depth = 0;
    let mut last_boundary = false;
    let mut main_item: Option<(usize, usize)> = None;
    let mut held_code: Option<(u8, usize, usize, Option<usize>)> = None;
    for (note_line_index, ((line, masked), fenced)) in source
        .split('\n')
        .zip(mask.split('\n'))
        .zip(fence_mask.split('\n'))
        .enumerate()
    {
        let physical_prefix = quote.find(line).unwrap().as_str();
        let physical_depth = physical_prefix.bytes().filter(|&b| b == b'>').count();
        let physical_rest = &line[physical_prefix.len()..];
        let physical_indent = physical_rest
            .bytes()
            .take_while(|b| matches!(b, b' ' | b'\t'))
            .count();
        let may_open_inline = blank
            || !paragraph
            || last_boundary
            || physical_depth < previous_depth
            || main_item.is_some_and(|(owner_depth, column)| {
                physical_depth <= owner_depth && physical_indent <= column
            })
            || notes.last().is_some_and(|note| {
                physical_depth <= note.depth
                    && (physical_indent <= note.column
                        || note
                            .item_column
                            .is_some_and(|column| physical_indent <= column))
            });
        let inline = may_open_inline
            .then(|| inline_header.captures(&physical_rest[physical_indent..]))
            .flatten();
        let virtual_prefix = inline
            .as_ref()
            .filter(|caps| !caps[2].is_empty())
            .map(|caps| {
                format!(
                    "{physical_prefix}{}{}{}",
                    " ".repeat(physical_indent),
                    &caps[1],
                    &caps[2]
                )
            });
        let prefix = virtual_prefix.as_deref().unwrap_or(physical_prefix);
        let depth = prefix.bytes().filter(|&b| b == b'>').count();
        let rest = &line[prefix.len()..];
        let indent = rest
            .bytes()
            .take_while(|b| matches!(b, b' ' | b'\t'))
            .count();
        let inline_width = inline
            .as_ref()
            .filter(|caps| caps[2].is_empty())
            .map_or(0, |caps| caps[1].len());
        let header_column = indent + inline_width;
        let text = &rest[indent + inline_width..];
        let visible = masked
            .get(prefix.len() + indent + inline_width..)
            .unwrap_or("");
        let quote_view = virtual_prefix.as_ref().map(|_| {
            let marker = &inline.as_ref().unwrap()[1];
            let start = physical_prefix.len() + physical_indent;
            format!(
                "{}{}{}",
                &line[..start],
                " ".repeat(marker.len()),
                &line[start + marker.len()..]
            )
        });
        let column_line = quote_view.as_deref().unwrap_or(line);
        let mut columns = Vec::with_capacity(depth + 1);
        let mut ends = Vec::with_capacity(depth);
        let mut at = 0;
        for level in 0..=depth {
            let start = at;
            while column_line
                .as_bytes()
                .get(at)
                .is_some_and(|b| matches!(b, b' ' | b'\t'))
            {
                at += 1;
            }
            columns.push(at - start);
            if level < depth {
                at += 1;
                if column_line
                    .as_bytes()
                    .get(at)
                    .is_some_and(|b| matches!(b, b' ' | b'\t'))
                {
                    at += 1;
                }
                ends.push(at);
            }
        }
        if held_code.is_some_and(|(_, _, host, floor)| {
            depth < host
                || floor.is_some_and(|floor| depth == host && indent < floor && !text.is_empty())
        }) {
            held_code = None;
        }
        let active = notes.last();
        let outside_main_item = active.is_none()
            && main_item.is_some_and(|(owner_depth, column)| {
                depth < owner_depth
                    || columns
                        .get(owner_depth)
                        .is_some_and(|current| *current <= column)
            });
        let can_define = inline.is_some()
            || blank
            || !paragraph
            || last_boundary
            || outside_main_item
            || depth < previous_depth
            || active.is_some_and(|n| depth <= n.depth && indent <= n.column);
        let head = can_define
            .then(|| header.captures(text))
            .flatten()
            .filter(|caps| {
                let colon = caps[1].trim_end_matches([' ', '\t']).len() - 1;
                masked.as_bytes().get(prefix.len() + header_column + colon) == Some(&b':')
                    || held_code.is_none()
                        && fenced != line
                        && code.is_match(&caps[2][djot_content_start(&caps[2])..])
            });
        let mut returned_note_attributes = Vec::new();
        let mut returned_scope: Option<(usize, usize)> = None;
        while let Some(note) = notes.last() {
            if fenced != line && (held_code.is_some() || !code.is_match(text)) {
                break;
            }
            let column = if inline.is_some() && note.depth == physical_depth {
                physical_indent
            } else {
                columns.get(note.depth).copied().unwrap_or(indent)
            };
            let outside = depth < note.depth || column <= note.column;
            let starts_block = head.is_some()
                || code.is_match(text)
                || depth > note.depth
                || depth < note.depth && text.is_empty()
                || block.is_match(visible)
                || (text.starts_with('{')
                    && read_djot_word_attributes(text, 0)
                        .is_some_and(|(end, _)| end == text.len()))
                || metadata.is_boundary(line);
            if outside
                && !(note.paragraph && !note.blank && !starts_block && depth <= note.depth)
                && (!text.is_empty() || depth != note.depth)
            {
                let mut closed = notes.pop().unwrap();
                let returned =
                    normalize_note_body(&mut closed, empty_note, attribute_prefix, item_attributes);
                returned_note_attributes.extend(returned);
                if !returned_note_attributes.is_empty() {
                    returned_scope = Some(match returned_scope {
                        Some((depth, column)) if depth < closed.depth => (depth, column),
                        Some((depth, column)) if depth == closed.depth => {
                            (depth, column.min(closed.column))
                        }
                        _ => (closed.depth, closed.column),
                    });
                }
                if let Some(parent) = notes.last_mut() {
                    if (closed.blank || !closed.paragraph)
                        && parent.item_column.is_some_and(|column| indent <= column)
                        && !block.is_match(visible)
                        && !item.is_match(text)
                    {
                        parent.body.push(String::new());
                        parent.blank = true;
                    }
                    parent.paragraph = false;
                }
                if notes.is_empty()
                    && (code.is_match(text)
                        || cached_regex!(r"^\[[^\]\n^][^\]\n]*\]:")
                            .unwrap()
                            .is_match(text)
                        || (text.starts_with('{')
                            && read_djot_word_attributes(text, 0)
                                .is_some_and(|(end, _)| end == text.len())))
                    && main.last().is_some_and(|line| line.contains(empty_note))
                {
                    main.push(line[..djot_content_start(line)].to_owned());
                }
                completed.push(closed);
            } else {
                break;
            }
        }
        if let Some((scope_depth, scope_column)) = returned_scope {
            let scope_depth = scope_depth.min(depth);
            let end = ends.get(scope_depth.wrapping_sub(1)).copied().unwrap_or(0);
            let padding = columns
                .get(scope_depth)
                .copied()
                .unwrap_or(indent)
                .min(scope_column);
            let mut prefix = format!("{}{}", &line[..end], " ".repeat(padding));
            if prefix.ends_with('>') {
                prefix.push(' ');
            }
            for attributes in returned_note_attributes {
                if let Some(parent) = notes.last_mut() {
                    let host = ends.get(parent.depth.wrapping_sub(1)).copied().unwrap_or(0);
                    let prefix = &prefix[host..];
                    let padding = prefix
                        .bytes()
                        .take_while(|b| matches!(b, b' ' | b'\t'))
                        .count();
                    parent.body.push(format!(
                        "{}{attributes}",
                        &prefix[padding.min(parent.column)..]
                    ));
                } else {
                    main.push(format!("{prefix}{attributes}"));
                }
            }
        }
        if let Some(caps) = head {
            if let Some(inline) = &inline {
                let dummy = format!(
                    "{physical_prefix}{}{}{}{empty_note}",
                    " ".repeat(physical_indent),
                    &inline[1],
                    &inline[2]
                );
                if let Some(parent) = notes.last_mut() {
                    let host = ends.get(parent.depth.wrapping_sub(1)).copied().unwrap_or(0);
                    let payload = &dummy[host..];
                    let padding = payload
                        .bytes()
                        .take_while(|b| matches!(b, b' ' | b'\t'))
                        .count();
                    parent
                        .body
                        .push(payload[padding.min(parent.column)..].to_owned());
                } else {
                    main.push(dummy);
                    main_item = Some((physical_depth, physical_indent));
                }
            }
            let parent = notes.last_mut();
            let slot = if parent.is_none() && depth == 0 && header_column == 0 {
                Some(format!("{slot_prefix}{}\0", completed.len()))
            } else {
                None
            };
            let separator = if let Some(parent) = parent {
                let kept = prefix
                    .get(ends.get(parent.depth.wrapping_sub(1)).copied().unwrap_or(0)..)
                    .unwrap_or("")
                    .trim_end()
                    .to_owned();
                if inline.is_none() {
                    parent.body.push(kept);
                }
                None
            } else {
                if depth > 0 && inline.is_none() {
                    main.push(prefix.trim_end().to_owned());
                }
                Some(())
            };
            if separator.is_some() {
                if let Some(slot) = &slot {
                    main.push(slot.clone());
                }
            }
            let body = &caps[2];
            if let Some(opener) = code.captures(&body[djot_content_start(body)..]) {
                held_code = Some((
                    opener[1].as_bytes()[0],
                    opener[1].len(),
                    depth,
                    Some(header_column + 1),
                ));
            }
            let quoted_item = quote
                .find(body)
                .is_some_and(|prefix| !prefix.is_empty() && item.is_match(&body[prefix.end()..]));
            let empty_task = empty_djot_task_tail_end(
                body,
                lines
                    .get(note_line_index + 1)
                    .map_or(true, |line| line.trim().is_empty()),
            )
            .is_some();
            let first_padding = if item.is_match(body) || quoted_item {
                caps[1].encode_utf16().count()
            } else if code.is_match(body) {
                2
            } else {
                0
            };
            let body_paragraph =
                !empty_task && !body.is_empty() && !block.is_match(body) && !code.is_match(body);
            notes.push(NoteBuffer {
                depth,
                column: header_column,
                label: caps[1].to_owned(),
                body: vec![format!("{}{body}", " ".repeat(first_padding))],
                converted_body: None,
                carry_task_attributes: !empty_task
                    && empty_djot_task_tail_end(body, true).is_some()
                    && lines.get(note_line_index + 1).is_some_and(|line| {
                        let text = &line[djot_content_start(line)..];
                        read_djot_word_attributes(text, 0)
                            .is_some_and(|(end, _)| text[end..].trim().is_empty())
                    }),
                item_column: item
                    .is_match(body)
                    .then_some(header_column + caps[1].encode_utf16().count()),
                paragraph_depth: depth
                    + body[..djot_content_start(body)]
                        .bytes()
                        .filter(|&b| b == b'>')
                        .count(),
                paragraph: !empty_task
                    && (body_paragraph || item.is_match(body) || starts_quote(body)),
                blank: empty_task || !body_paragraph && !item.is_match(body) && !starts_quote(body),
                slot,
            });
            let mut nested_body = body;
            while let Some(marker) = item.find(nested_body) {
                let after_marker = &nested_body[marker.end()..];
                let Some(nested) = first_header.find(after_marker) else {
                    break;
                };
                let parent = notes.last_mut().unwrap();
                let child_column = parent.column
                    + parent.label.encode_utf16().count()
                    + marker.as_str().encode_utf16().count();
                parent.body[0] = format!(
                    "{}{}{empty_note}",
                    " ".repeat(parent.label.encode_utf16().count()),
                    marker.as_str()
                );
                nested_body = &after_marker[nested.end()..];
                let child_item = item.is_match(nested_body);
                let child_paragraph = !nested_body.is_empty()
                    && !block.is_match(nested_body)
                    && !code.is_match(nested_body);
                let child_padding = if child_item {
                    nested.as_str().encode_utf16().count()
                } else if code.is_match(nested_body) {
                    2
                } else {
                    0
                };
                if let Some(opener) = code.captures(nested_body) {
                    held_code = Some((
                        opener[1].as_bytes()[0],
                        opener[1].len(),
                        depth,
                        Some(child_column + 1),
                    ));
                }
                notes.push(NoteBuffer {
                    depth,
                    column: child_column,
                    label: nested.as_str().to_owned(),
                    body: vec![if item
                        .find(nested_body)
                        .is_some_and(|marker| first_header.is_match(&nested_body[marker.end()..]))
                    {
                        String::new()
                    } else {
                        format!("{}{nested_body}", " ".repeat(child_padding))
                    }],
                    converted_body: None,
                    carry_task_attributes: false,
                    item_column: child_item
                        .then_some(child_column + nested.as_str().encode_utf16().count()),
                    paragraph_depth: depth,
                    paragraph: child_paragraph || child_item || starts_quote(nested_body),
                    blank: !child_paragraph && !child_item && !starts_quote(nested_body),
                    slot: None,
                });
            }
            paragraph = notes.last().unwrap().paragraph;
            blank = notes.last().unwrap().blank;
            previous_depth = depth;
            last_boundary = false;
            continue;
        }
        if fenced != line {
            if let Some((kind, width, host, _)) = held_code {
                let start = ends.get(host.wrapping_sub(1)).copied().unwrap_or(0);
                let content = line[start..].trim_matches([' ', '\t']);
                let run = content.bytes().take_while(|&b| b == kind).count();
                if run >= width && content[run..].trim_matches([' ', '\t']).is_empty() {
                    held_code = None;
                }
            } else if let Some(opener) = code.captures(&line[djot_content_start(line)..]) {
                held_code = Some((
                    opener[1].as_bytes()[0],
                    opener[1].len(),
                    depth,
                    notes.last().map(|note| note.column + 1),
                ));
            }
        }
        if let Some(note) = notes.last_mut() {
            let host = ends.get(note.depth.wrapping_sub(1)).copied().unwrap_or(0);
            let payload = &line[host..];
            let padding = payload
                .bytes()
                .take_while(|b| matches!(b, b' ' | b'\t'))
                .count();
            let removed = padding.min(note.column);
            let mut payload = payload[removed..].to_owned();
            let content_at = djot_content_start(line);
            if !rows[note_line_index]
                && line.as_bytes().get(content_at) == Some(&b'|')
                && masked.as_bytes().get(content_at) == Some(&b'|')
            {
                payload.insert(content_at - host - removed, '\\');
            }
            note.body.push(payload);
            let outside_item = note.item_column.is_some_and(|column| {
                columns.get(note.depth).copied().unwrap_or(indent) <= column
                    && (note.blank || depth > note.depth || block.is_match(visible))
            });
            if outside_item {
                note.item_column = None;
                note.paragraph = false;
                note.paragraph_depth = note.depth;
            }
            if item.is_match(text) && (!note.paragraph || outside_item) {
                note.item_column = Some(indent);
            }
            let literal_quote = depth > note.paragraph_depth && note.paragraph;
            note.blank = text.is_empty() && !literal_quote;
            if !note.paragraph && depth > note.depth {
                note.paragraph_depth = depth;
            }
            note.paragraph = !note.blank && fenced == line && !block.is_match(visible);
            paragraph = note.paragraph;
            blank = note.blank;
        } else {
            if outside_main_item
                && (blank || !paragraph || block.is_match(visible))
                && !item.is_match(text)
                && !text.is_empty()
            {
                main_item = None;
            }
            main.push(line.to_owned());
            blank = text.is_empty();
            if item.is_match(text) && (!paragraph || outside_main_item) {
                main_item = Some((depth, indent));
                let content = &text[djot_content_start(text)..];
                paragraph =
                    !content.is_empty() && !block.is_match(content) && !code.is_match(content);
            } else {
                paragraph = !blank && !block.is_match(visible);
            }
        }
        previous_depth = depth;
        last_boundary = metadata.is_boundary(line);
    }
    while let Some(note) = notes.pop() {
        completed.push(note);
    }
    if completed.is_empty() {
        return blocks_with_slot(source, None, None, attribute_prefix, item_attributes);
    }
    let mut definitions = HashMap::<String, String>::new();
    let mut unplaced = Vec::new();
    for mut note in completed {
        if note.converted_body.is_none() {
            normalize_note_body(&mut note, empty_note, attribute_prefix, item_attributes);
        }
        let converted = note.converted_body.as_ref().unwrap();
        let mut lines = converted.split('\n');
        let first_line = lines.next().unwrap_or("").trim_start_matches([' ', '\t']);
        let mut definition = if first_line.starts_with("* ")
            && note.body.first().is_some_and(|line| {
                empty_djot_task_tail_end(line.trim_start_matches([' ', '\t']), true).is_some()
            }) {
            format!("{}{empty_note}\n  {first_line}", note.label)
        } else {
            format!("{}{first_line}", note.label)
        };
        for line in lines {
            definition.push('\n');
            if !line.is_empty() {
                definition.push_str("  ");
                definition.push_str(line);
            }
        }
        if let Some(slot) = note.slot {
            definitions.insert(slot, definition);
        } else {
            unplaced.push(definition);
        }
    }
    let mut main_source = String::new();
    for (index, line) in main.iter().enumerate() {
        if index > 0 {
            main_source.push('\n');
        }
        main_source.push_str(line);
        if line.starts_with(&slot_prefix)
            && main
                .get(index + 1)
                .is_some_and(|next| code.is_match(next.trim_start_matches([' ', '\t'])))
        {
            main_source.push('\n');
        }
    }
    let main = normalize_djot_fences(&main_source);
    let normalized_main = blocks_with_slot(
        &main,
        Some(&slot_prefix),
        Some(empty_note),
        attribute_prefix,
        item_attributes,
    );
    let mut result = unplaced.join("\n\n");
    if !result.is_empty() {
        result.push_str("\n\n");
    }
    for (n, line) in normalized_main.split('\n').enumerate() {
        if n > 0 {
            result.push('\n');
        }
        if let Some(definition) = definitions.remove(line) {
            result.push_str(&definition);
        } else {
            result.push_str(line);
        }
    }
    result
}

#[cfg(test)]
pub(super) fn blocks(source: &str) -> String {
    blocks_with_slot(source, None, None, "", &mut Vec::new())
}

fn blocks_with_slot(
    source: &str,
    slot_prefix: Option<&str>,
    empty_note: Option<&str>,
    attribute_prefix: &str,
    item_attributes: &mut Vec<String>,
) -> String {
    let deep_quotes = deep_quoted_item_attributes(source, attribute_prefix, item_attributes);
    let normalized_quotes =
        quoted_list_attribute_layout(&deep_quotes, attribute_prefix, item_attributes);
    blocks_with_slot_inner(
        &normalized_quotes,
        slot_prefix,
        empty_note,
        attribute_prefix,
        item_attributes,
        source.contains("\\|"),
        None,
    )
}

fn blocks_with_slot_inner(
    source: &str,
    slot_prefix: Option<&str>,
    empty_note: Option<&str>,
    attribute_prefix: &str,
    item_attributes: &mut Vec<String>,
    pipe_pass: bool,
    returned_attributes: Option<&mut Vec<String>>,
) -> String {
    let initial_mask = mask_code_and_destinations(source);
    let row_source = slot_prefix.map(|prefix| {
        source
            .split('\n')
            .map(|line| {
                if line.starts_with(prefix) {
                    " ".repeat(line.len())
                } else {
                    line.to_owned()
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    });
    let initial_rows = djot_table_rows_with_empty_note(
        row_source.as_deref().unwrap_or(source),
        &initial_mask,
        empty_note,
    );
    let initial_fences = mask_djot_fences(source, None, &initial_rows, true);
    let has_definition_syntax = cached_regex!(r"(?:^|\n)(?:[ \t]*>(?:[ \t]|$))*[ \t]*:[ \t]")
        .unwrap()
        .is_match(&initial_mask);
    let normalized = quoted_list_boundaries(
        source,
        &initial_mask,
        &initial_fences,
        &initial_rows,
        slot_prefix,
        has_definition_syntax,
        None,
    );
    let changed = matches!(&normalized, std::borrow::Cow::Owned(_));
    let source = normalized.as_ref();
    let mask = if changed {
        mask_code_and_destinations(source)
    } else {
        initial_mask
    };
    let mut div_closers = HashMap::new();
    if source.contains(":::") {
        djot_inline_boundaries_with_div_closers(source, &mask, true, Some(&mut div_closers), None);
    }
    let mut line_offset = 0;
    let line_offsets: Vec<_> = source
        .split('\n')
        .map(|line| {
            let at = line_offset;
            line_offset += line.len() + 1;
            at
        })
        .collect();
    let masks: Vec<_> = mask.split('\n').collect();
    let lines: Vec<_> = source.split('\n').collect();
    let mut following_quote_depths = Vec::new();
    if source.contains('{') && source.contains('>') {
        following_quote_depths.resize(lines.len(), None);
        let mut following_depth = None;
        let mut following_source_depth = None;
        for (n, line) in lines.iter().enumerate().rev() {
            let (depth, body) = quoted(line);
            let body = body.trim_matches(DJOT_LINE_SPACING);
            let attributes = body.starts_with('{')
                && read_djot_word_attributes(body, 0).is_some_and(|(end, _)| end == body.len());
            if body.is_empty() || attributes {
                let target = if following_source_depth.is_some_and(|next| next > depth) {
                    Some(depth)
                } else {
                    following_depth
                };
                if attributes {
                    following_quote_depths[n] = target;
                }
                following_depth = target;
            } else {
                following_depth = Some(depth);
            }
            following_source_depth = Some(depth);
        }
    }
    let rows = if changed {
        let row_source = slot_prefix.map(|prefix| {
            source
                .split('\n')
                .map(|line| {
                    if line.starts_with(prefix) {
                        " ".repeat(line.len())
                    } else {
                        line.to_owned()
                    }
                })
                .collect::<Vec<_>>()
                .join("\n")
        });
        djot_table_rows_with_empty_note(row_source.as_deref().unwrap_or(source), &mask, empty_note)
    } else {
        initial_rows
    };
    let fence_mask = if changed {
        mask_djot_fences(source, None, &rows, true)
    } else {
        initial_fences
    };

    let fence_masks: Vec<_> = fence_mask.split('\n').collect();
    let code_opener = cached_regex!(r"^(`{3,}|~{3,})[ \t]*=?[A-Za-z0-9_+#.-]*[ \t]*$").unwrap();
    let mut open_code: Option<(u8, usize, String)> = None;
    let marker =
        cached_regex!(r"^([-*+]|(?:[0-9]+|[A-Za-z]|[ivxlcdm]+|[IVXLCDM]+)[.)]|\((?:[0-9]+|[A-Za-z]|[ivxlcdm]+|[IVXLCDM]+)\))([ \t]+)")
            .unwrap();
    let heading = cached_regex!(r"^#{1,6}(?:[ \t]|$)").unwrap();
    let reference = cached_regex!(r"^\[[^\]\n]+\]:").unwrap();
    let continuation_lines = table_continuation_lines(source);
    let mut result: Vec<String> = Vec::new();
    let mut lists: Vec<List> = Vec::new();
    let mut definition_columns = Vec::new();
    let mut definition_body = false;
    let mut divs: Vec<(usize, String, bool, Option<usize>, usize)> = Vec::new();
    let mut paragraph = false;
    let mut previous_blank = true;
    let mut in_heading = false;
    let mut in_quote = false;
    let mut quote_paragraph_depth = None;
    let track_quote_owners = source.contains("\0DJOTINVALIDATTR\0");
    let mut owned_quote_depths = HashSet::new();
    let mut quote_paragraph_item_column = None;
    let mut quote_attribute_depth = None;
    let mut moved_quote_attributes = None;
    let mut pending_item_attributes: Vec<(usize, usize, String)> = Vec::new();
    let mut pending_root_attributes: Vec<(usize, String)> = Vec::new();
    for (n, raw) in lines.iter().enumerate() {
        if track_quote_owners && n > 0 {
            let (depth, body) = quoted(lines[n - 1]);
            if body.trim_matches(DJOT_LINE_SPACING).is_empty() {
                owned_quote_depths.clear();
            } else if marker.is_match(body.trim_start_matches(DJOT_LINE_SPACING))
                || heading.is_match(body.trim_start_matches(DJOT_LINE_SPACING))
            {
                owned_quote_depths.insert(depth);
            }
        }
        if slot_prefix.is_some_and(|prefix| raw.starts_with(prefix)) {
            paragraph = false;
            previous_blank = true;
            in_heading = false;
            result.push((*raw).to_owned());
            continue;
        }
        let prior_move = moved_quote_attributes.take();
        if let Some((from, to)) = prior_move {
            let (row_depth, row_body) = quoted(raw);
            if row_body.trim_matches(DJOT_LINE_SPACING).is_empty()
                && row_depth > 0
                && row_depth <= from
                && row_depth >= to
            {
                moved_quote_attributes = Some((row_depth, to));
                previous_blank = true;
                continue;
            }
        }
        if let Some(attribute_depth) = quote_attribute_depth {
            let (row_depth, row_body) = quoted(raw);
            if row_body.trim_matches(DJOT_LINE_SPACING).is_empty() && attribute_depth == row_depth {
                previous_blank = true;
                continue;
            }
            quote_attribute_depth = None;
        }
        let (indent, indent_bytes) = leading_indent(raw);
        let text = &raw[indent_bytes..];
        let masked_visible = masks[n].get(indent_bytes..).unwrap_or("");
        let carried_block = (!pending_item_attributes.is_empty()
            || marker.is_match(masked_visible))
            && marker.captures(text).is_some_and(|item| {
                let mut body = &text[item.get(0).unwrap().end()..];
                if matches!(item[1].as_bytes()[0], b'-' | b'*' | b'+')
                    && item.get(2).unwrap().as_str() == " "
                {
                    if let Some(task) = cached_regex!(r"^\[[ xX]\][ \t]+").unwrap().find(body) {
                        body = &body[task.end()..];
                    }
                }
                code_opener.is_match(body)
                    || reference.is_match(body)
                    || body.starts_with('|')
                    || body.starts_with(":::")
            });
        let visible = if carried_block { text } else { masked_visible };
        let attribute_only = visible.starts_with('{')
            && read_djot_word_attributes(text, 0)
                .is_some_and(|(end, _)| end == text.trim_end_matches(DJOT_LINE_SPACING).len());
        if !raw.trim_matches([' ', '\t']).is_empty() && !attribute_only {
            pending_root_attributes.clear();
        }
        if !pending_item_attributes.is_empty()
            && !raw.trim_matches(DJOT_LINE_SPACING).is_empty()
            && (!attribute_only || !lists.iter().any(|list| indent > list.source))
            && (!marker.is_match(visible) || thematic(text))
        {
            let target = lists
                .iter()
                .rev()
                .find(|list| indent > list.source)
                .map_or(indent, |list| {
                    list.target_column + indent.saturating_sub(list.column)
                });
            for (_, row, attrs) in pending_item_attributes.drain(..) {
                result[row] = format!("{}{attrs}", " ".repeat(target));
            }
        }
        if rejected_comment_starts_block(visible) {
            let outside_list = lists.first().is_some_and(|list| indent <= list.source);
            let outside_quote = in_quote && !visible.starts_with('>');
            let outside_definition = definition_columns
                .last()
                .is_some_and(|column| indent < *column);
            if outside_definition {
                definition_columns.clear();
                definition_body = false;
                paragraph = false;
                previous_blank = true;
            }
            if in_heading || outside_list || outside_quote || outside_definition {
                result.push(raw[..indent_bytes].to_owned());
            }
            if outside_quote {
                in_quote = false;
                quote_paragraph_depth = None;
                paragraph = false;
                previous_blank = true;
            }
            if outside_list {
                lists.clear();
                paragraph = false;
                previous_blank = true;
            }
            in_heading = false;
        }
        let outer_source = lists.first().map(|list| list.source);
        let block = rows[n]
            || fence_masks[n] != *raw && !carried_block
            || heading.is_match(visible)
            || starts_quote(visible)
            || visible.starts_with(":::")
            || visible.starts_with('|')
            || (thematic(visible) && thematic(text));
        if !raw.trim_matches([' ', '\t']).is_empty() {
            while divs.last().is_some_and(|(_, _, _, owner, _)| {
                owner.is_some_and(|column| {
                    indent < column && (marker.is_match(visible) || block || previous_blank)
                })
            }) {
                let (width, prefix, _, _, _) = divs.pop().unwrap();
                let before = result
                    .iter()
                    .rposition(|line| !line.trim_matches([' ', '\t']).is_empty())
                    .map_or(0, |n| n + 1);
                result.insert(before, format!("{prefix}{}", ":".repeat(width)));
                paragraph = false;
                quote_paragraph_depth = None;
            }
        }
        if block && !(carried_block && marker.is_match(visible)) {
            while lists.last().is_some_and(|list| {
                indent <= list.source || lists.len() > 1 && indent < list.column
            }) {
                lists.pop();
                paragraph = false;
                in_heading = false;
                if let Some(parent) = lists.last_mut() {
                    parent.loose = true;
                }
            }
            if previous_blank {
                for list in &mut lists {
                    if indent > list.source {
                        list.loose = true;
                    }
                }
            }
        }
        if previous_blank
            && definition_columns
                .last()
                .is_some_and(|column| indent >= *column)
        {
            definition_body = true;
        }
        if (block || definition_body && marker.is_match(visible))
            && definition_columns
                .last()
                .is_some_and(|column| indent < *column)
        {
            definition_columns.clear();
            definition_body = false;
            paragraph = false;
            in_heading = false;
        }
        let target_indent = if !has_definition_syntax && quoted(raw).0 > 0 && fence_masks[n] == *raw
        {
            indent
        } else {
            lists
                .last()
                .filter(|list| indent > list.source)
                .map_or(indent, |list| {
                    list.target_column + indent.saturating_sub(list.column)
                })
        };
        let display = if target_indent == indent {
            (*raw).to_owned()
        } else {
            format!("{}{text}", " ".repeat(target_indent))
        };
        if !attribute_prefix.is_empty() && text.starts_with(attribute_prefix) {
            let suffix = &text[attribute_prefix.len()..];
            let index = suffix.strip_prefix('B').unwrap_or(suffix);
            if index
                .strip_suffix('\0')
                .is_some_and(|index| !index.is_empty() && index.bytes().all(|b| b.is_ascii_digit()))
            {
                result.push(display.clone());
                paragraph = false;
                previous_blank = suffix.starts_with('B');
                continue;
            }
        }
        if raw.trim_matches([' ', '\t']).is_empty() {
            quote_paragraph_depth = None;
            paragraph = false;
            in_heading = false;
            in_quote = false;
            previous_blank = true;
            result.push(display.clone());
            continue;
        }
        if visible.trim_matches(DJOT_LINE_SPACING).is_empty() {
            if fence_masks[n].trim().is_empty() {
                if let Some((kind, width, _)) = &open_code {
                    let length = text.bytes().take_while(|byte| byte == kind).count();
                    if length >= *width && text[length..].trim().is_empty() {
                        open_code = None;
                    }
                } else if let Some(caps) = code_opener.captures(text) {
                    open_code = Some((
                        caps[1].as_bytes()[0],
                        caps[1].len(),
                        " ".repeat(target_indent),
                    ));
                }
                paragraph = false;
                in_heading = false;
            } else {
                paragraph = !in_heading;
            }
            previous_blank = false;
            result.push(display.clone());
            continue;
        }
        if continuation_lines.contains(&(n + 1)) {
            result.push(display.clone());
            paragraph = false;
            previous_blank = false;
            continue;
        }
        if in_quote
            && quoted(raw).0 == 0
            && indent == 0
            && visible.starts_with('{')
            && read_djot_word_attributes(text, 0)
                .is_some_and(|(end, _)| end == text.trim_end_matches(DJOT_LINE_SPACING).len())
        {
            result.push(String::new());
            in_quote = false;
            quote_paragraph_depth = None;
            paragraph = false;
        }
        if quoted(raw).0 > 0 && masks[n].as_bytes().get(indent_bytes) != Some(&b'>') {
            result.push(display.clone());
            previous_blank = false;
            continue;
        }
        if quoted(raw).0 > 0
            && (in_quote || !paragraph || n > 0 && masks[n - 1].trim_start().starts_with('>'))
        {
            let (depth, body) = quoted(raw);
            let block_body = body.trim_start_matches(DJOT_LINE_SPACING);
            if rejected_comment_starts_block(block_body) {
                let owned = owned_quote_depths.contains(&depth);
                if owned || depth < quote_paragraph_depth.unwrap_or(depth) {
                    result.push(raw[..raw.len() - body.len()].trim_end().to_owned());
                    quote_paragraph_depth = None;
                }
            }
            let standalone_attributes = block_body.starts_with('{')
                && read_djot_word_attributes(block_body, 0).is_some_and(|(end, _)| {
                    end == block_body.trim_end_matches(DJOT_LINE_SPACING).len()
                });
            let attribute_only =
                standalone_attributes && quote_paragraph_depth.map_or(true, |held| depth < held);
            let attribute_depth = if attribute_only {
                following_quote_depths[n].map_or(depth, |next| depth.min(next))
            } else {
                depth
            };
            let continued_move =
                prior_move.is_some_and(|(from, to)| depth <= from && attribute_depth == to);
            if attribute_only
                && !continued_move
                && (attribute_depth < depth
                    || quote_paragraph_depth.is_some_and(|held| depth < held))
            {
                if attribute_depth < depth && quote_paragraph_depth.is_none() {
                    result.push(raw[..raw.len() - body.len()].trim_end().to_owned());
                }
                result.push(
                    format!(
                        "{}{}",
                        " ".repeat(target_indent),
                        "> ".repeat(attribute_depth)
                    )
                    .trim_end()
                    .to_owned(),
                );
                quote_paragraph_depth = None;
            }
            if let Some(held) = quote_paragraph_depth.filter(|&held| {
                depth > held
                    && !body.trim_matches(DJOT_LINE_SPACING).is_empty()
                    && !quote_paragraph_item_column.is_some_and(|(item_depth, column)| {
                        item_depth == held
                            && djot_attribute_line(raw, held)
                                .is_some_and(|content| leading_indent(content).0 <= column)
                    })
            }) {
                let mut at = 0;
                for _ in 0..held {
                    let rest = &raw[at..];
                    at += rest.len() - rest.trim_start_matches([' ', '\t']).len() + 1;
                    if raw.as_bytes().get(at) == Some(&b' ') {
                        at += 1;
                    }
                }
                at += raw[at..].len() - raw[at..].trim_start_matches([' ', '\t']).len();
                let mut literal = display.clone();
                let at = if target_indent == indent {
                    at
                } else {
                    at - indent_bytes + target_indent
                };
                literal.insert(at, '\\');
                result.push(literal);
                previous_blank = false;
                continue;
            }
            if depth < quote_paragraph_depth.unwrap_or(depth)
                && marker.is_match(block_body)
                && !previous_blank
            {
                quote_paragraph_depth = None;
            }
            let metadata_body = quote_paragraph_depth.is_none()
                && (reference.is_match(block_body)
                    || block_body.starts_with('{')
                        && read_djot_word_attributes(block_body, 0).is_some_and(|(end, _)| {
                            end == block_body.trim_end_matches(DJOT_LINE_SPACING).len()
                        }));

            quote_paragraph_depth = (!body.trim_matches(DJOT_LINE_SPACING).is_empty()
                && !metadata_body
                && !heading.is_match(block_body)
                && (!marker.is_match(block_body)
                    || marker.find(block_body).is_some_and(|item| {
                        let content = block_body[item.end()..].trim_start();
                        let content = if block_body.starts_with(['-', '*', '+']) {
                            cached_regex!(r"^\[[ xX]\][ \t]+")
                                .unwrap()
                                .find(content)
                                .map_or(content, |task| &content[task.end()..])
                        } else {
                            content
                        };
                        !content.is_empty()
                            && content.trim_end_matches([' ', '\t']) != "+"
                            && !heading.is_match(content)
                            && !(thematic(content)
                                && !(content.contains('*') && content.contains('-')))
                            && !rows[n]
                            && !content.starts_with(['>', ':', '`', '~', '{'])
                    }))
                && !block_body.starts_with(":::")
                && !thematic(block_body)
                && !rows[n])
                .then_some(depth.max(quote_paragraph_depth.unwrap_or(depth)));
            quote_paragraph_item_column = if quote_paragraph_depth.is_some() {
                if marker.is_match(block_body) {
                    Some((depth, leading_indent(body).0))
                } else {
                    quote_paragraph_item_column.filter(|&(item_depth, _)| item_depth == depth)
                }
            } else {
                None
            };
            result.push(if attribute_depth < depth {
                format!(
                    "{}{}{}",
                    " ".repeat(target_indent),
                    "> ".repeat(attribute_depth),
                    body
                )
            } else {
                display.clone()
            });
            in_quote = attribute_depth > 0;
            quote_attribute_depth = attribute_only.then_some(attribute_depth);
            moved_quote_attributes =
                (attribute_only && attribute_depth < depth).then_some((depth, attribute_depth));
            paragraph = false;
            previous_blank = body.trim_matches(DJOT_LINE_SPACING).is_empty();
            continue;
        }
        if !paragraph && definition_term(raw).is_some() {
            while definition_columns
                .last()
                .is_some_and(|column| *column >= indent + 2)
            {
                definition_columns.pop();
            }
            definition_columns.push(indent + 2);
            definition_body = false;
        } else if previous_blank {
            while definition_columns
                .last()
                .is_some_and(|column| indent < *column)
            {
                definition_columns.pop();
            }
        }
        if let Some(item) = marker
            .captures(visible)
            .and_then(|_| marker.captures(text))
            .filter(|_| !thematic(text))
        {
            let pending_for_item = std::mem::take(&mut pending_item_attributes);
            for (_, row, _) in &pending_for_item {
                result[*row].clear();
            }
            if in_quote && !previous_blank {
                if !paragraph && !lists.last().is_some_and(|list| indent <= list.source) {
                    result.push(String::new());
                }
                in_quote = false;
                quote_paragraph_depth = None;
            }
            let previous_list = lists.last().cloned();
            let ends_nested = previous_list
                .as_ref()
                .is_some_and(|list| indent < list.source);
            while lists.last().is_some_and(|list| indent < list.source) {
                lists.pop();
                paragraph = false;
                in_heading = false;
            }
            if let Some(parent) = lists
                .last_mut()
                .filter(|list| previous_blank && indent == list.source)
            {
                if ends_nested && !parent.loose {
                    while result
                        .last()
                        .is_some_and(|line| line.trim_matches([' ', '\t']).is_empty())
                    {
                        result.pop();
                    }
                    let start = previous_list.as_ref().unwrap().start;
                    if !parent.lazy
                        && start > 0
                        && result
                            .get(start - 1)
                            .is_some_and(|line| line.trim_matches([' ', '\t']).is_empty())
                    {
                        result.remove(start - 1);
                    }
                } else {
                    parent.loose = true;
                }
            }
            let top = lists.last().cloned();
            if let Some(parent) = top.as_ref().filter(|list| indent > list.source) {
                if !previous_blank && paragraph {
                    let mut literal = text.to_owned();
                    if matches!(item[1].as_bytes()[0], b'-' | b'*' | b'+') {
                        literal.insert(0, '\\');
                    } else if let Some(at) = literal.find(['.', ')']) {
                        literal.insert(at, '\\');
                    }
                    result.push(format!("{}{literal}", " ".repeat(parent.target_column)));
                    lists.last_mut().unwrap().lazy = true;
                    continue;
                }
            }
            let nested = top.as_ref().is_some_and(|list| indent > list.source);
            if top.is_none() && paragraph && !previous_blank {
                result.push(if matches!(item[1].as_bytes()[0], b'-' | b'*' | b'+') {
                    format!("{}\\{text}", &raw[..indent_bytes])
                } else {
                    display.clone()
                });
                continue;
            }
            let bullet = item[1].as_bytes()[0];
            let is_bullet = matches!(bullet, b'-' | b'*' | b'+');
            let mut kind = if is_bullet {
                item[1].to_owned()
            } else {
                djot_list_kind(&item[1])
            };
            if is_bullet
                && item.get(2).unwrap().as_str() == " "
                && cached_regex!(r"^\[[ xX]\][ \t]+")
                    .unwrap()
                    .is_match(&text[item.get(2).unwrap().end()..])
            {
                kind.push_str(":task");
            }
            let shared_kind = top
                .as_ref()
                .and_then(|list| shared_djot_list_kind(&list.kind, &kind));
            let same_kind = shared_kind.is_some();
            if !nested {
                if let Some(shared) = shared_kind {
                    kind = shared;
                }
            }
            let mut target = if definition_columns
                .last()
                .is_some_and(|column| indent >= *column)
            {
                indent
            } else {
                0
            };
            let mut written = if is_bullet {
                if bullet == b'+' {
                    b'-'
                } else {
                    bullet
                }
            } else {
                b'-'
            };
            let mut separate = false;
            if let Some(parent) = &top {
                if nested {
                    target = parent.target_column;
                } else {
                    target = parent.target;
                    written = parent.written;
                    if !same_kind {
                        written = if parent.written == b'-' { b'*' } else { b'-' };
                        separate = true;
                    }
                    lists.pop();
                }
            }
            if separate && result.last().is_some_and(|line| !line.is_empty()) {
                result.push(String::new());
            }
            let marker_end = item.get(2).unwrap().end();
            let marker_body = item.get(0).unwrap().end();
            let payload_start = if matches!(item[1].as_bytes()[0], b'-' | b'*' | b'+')
                && item.get(2).unwrap().as_str() == " "
            {
                cached_regex!(r"^\[[ xX]\][ \t]+")
                    .unwrap()
                    .find(&text[marker_body..])
                    .map_or(marker_body, |task| marker_body + task.end())
            } else {
                marker_body
            };
            let native_marker_end = compact_djot_list_prefix(&text[..marker_end]).len();
            let mut payload = compact_djot_list_prefix(text).into_owned();
            let carried = if !attribute_prefix.is_empty()
                && top.as_ref().is_some_and(|_| !nested && same_kind)
            {
                let mut parts = Vec::new();
                for (owner, _, attrs) in pending_for_item {
                    if owner >= indent {
                        parts.push(attrs);
                    }
                }
                (!parts.is_empty()).then(|| {
                    let merged = attributes(&parts.join(""))
                        .into_iter()
                        .flat_map(|(_, values)| values)
                        .collect::<Vec<_>>()
                        .join(" ");
                    let token = format!("{attribute_prefix}{}\0", item_attributes.len());
                    item_attributes.push(
                        read_djot_attributes(&format!("{{{merged}}}"), 0, true, false)
                            .map_or_else(|| format!("{{{merged}}}"), |(_, wire)| wire),
                    );
                    token
                })
            } else {
                for (_, _, attrs) in pending_for_item {
                    result.push(format!("{}{attrs}", " ".repeat(target)));
                }
                None
            };
            if let Some(token) = carried {
                while result.last().is_some_and(|line| line.trim().is_empty()) {
                    result.pop();
                }
                if top.as_ref().is_some_and(|list| !nested && list.loose) {
                    result.push(String::new());
                }
                result.push(format!("{}{token}", " ".repeat(target)));
                result.push(String::new());
            }

            if is_bullet {
                payload.replace_range(..1, &(written as char).to_string());
            }
            if top
                .as_ref()
                .is_some_and(|list| !nested && list.loose && list.lazy)
                && result
                    .last()
                    .is_some_and(|line| !line.trim_matches([' ', '\t']).is_empty())
            {
                result.push(String::new());
            }
            let list_start = top
                .as_ref()
                .filter(|_| !nested && !separate)
                .map_or(result.len(), |list| list.start);
            result.push(format!("{}{payload}", " ".repeat(target)));
            lists.push(List {
                source: indent,
                column: indent + marker_end,
                target,
                target_column: target + native_marker_end,
                written,
                kind,
                start: list_start,
                lazy: false,
                loose: !nested && top.as_ref().is_some_and(|list| list.loose),
            });
            in_quote = quoted(&text[payload_start..]).0 > 0;
            let item_body = &text[payload_start..];
            let div_width = item_body.bytes().take_while(|b| *b == b':').count();
            let item_div = div_width >= 3;
            if item_div {
                divs.push((
                    div_width,
                    " ".repeat(target + native_marker_end),
                    crate::parse::lint_invalid_container_metadata(item_body),
                    Some(indent + marker_end),
                    line_offsets[n],
                ));
            }
            in_heading = heading.is_match(item_body);
            paragraph = item_body.trim() != "+"
                && !in_quote
                && !item_div
                && !in_heading
                && !code_opener.is_match(item_body)
                && !reference.is_match(item_body)
                && !read_djot_word_attributes(item_body, 0)
                    .is_some_and(|(end, _)| end == item_body.len())
                && !empty_note.is_some_and(|marker| item_body.trim() == marker);
            previous_blank = item_div;
            continue;
        }
        if previous_blank {
            while lists.last().is_some_and(|list| indent <= list.source) {
                lists.pop();
            }
        }
        let width = text.bytes().take_while(|b| *b == b':').count();
        if width >= 3 && visible.starts_with(':') {
            let bare = text[width..].trim().is_empty();
            if let Some(starts) = div_closers.get(&line_offsets[n]) {
                drop_orphan_attribute_line(&mut result);
                let outer_start = *starts.last().unwrap();
                let mut matched = 0;
                while divs.last().is_some_and(|owner| owner.4 >= outer_start) {
                    let (open, prefix, _, _, start) = divs.pop().unwrap();
                    while starts.get(matched).is_some_and(|&open| open > start) {
                        matched += 1;
                    }
                    if starts.get(matched) == Some(&start) {
                        result.push(format!("{prefix}{}", ":".repeat(open)));
                        matched += 1;
                    }
                }
                paragraph = false;
                in_heading = false;
                previous_blank = true;
                continue;
            } else if bare && divs.iter().any(|(open, _, _, _, _)| width >= *open) {
                let boundary = if divs.iter().any(|(_, _, invalid, _, _)| *invalid) {
                    divs.iter()
                        .rposition(|(open, _, _, _, _)| width >= *open)
                        .unwrap()
                } else {
                    divs.iter()
                        .position(|(open, _, _, _, _)| width >= *open)
                        .unwrap()
                };
                drop_orphan_attribute_line(&mut result);
                while divs.len() > boundary {
                    let (open, prefix, _, _, _) = divs.pop().unwrap();
                    result.push(format!("{prefix}{}", ":".repeat(open)));
                }
                paragraph = false;
                in_heading = false;
                previous_blank = true;
                continue;
            } else if paragraph {
                result.push(format!("{}\\{text}", &raw[..indent_bytes]));
            } else {
                divs.push((
                    width,
                    raw[..indent_bytes].to_owned(),
                    crate::parse::lint_invalid_container_metadata(text),
                    lists
                        .last()
                        .filter(|list| indent > list.source)
                        .map(|list| list.column),
                    line_offsets[n],
                ));
                result.push(display.clone());
                paragraph = false;
                in_heading = false;
                previous_blank = true;
                continue;
            }
        } else if paragraph
            && (heading.is_match(visible)
                || starts_quote(visible)
                || visible.starts_with('|') && !pipe_pass
                || (thematic(visible) && thematic(text))
                    && !(visible.contains('*') && visible.contains('-')))
        {
            result.push(format!("{}\\{text}", &raw[..indent_bytes]));
        } else if heading.is_match(visible) {
            in_heading = true;
            let level = text.bytes().take_while(|b| *b == b'#').count();
            result.push(format!(
                "{}{}{}",
                if lists.is_empty() && definition_columns.is_empty() {
                    String::new()
                } else {
                    " ".repeat(target_indent)
                },
                "#".repeat(level),
                if text[level..].trim().is_empty() {
                    text[level..].to_owned()
                } else {
                    format!(" {}", text[level..].trim_start())
                }
            ));
            paragraph = false;
        } else if thematic(visible) && thematic(text) {
            result.push(
                if !paragraph && lists.is_empty() && definition_columns.is_empty() {
                    "***".to_owned()
                } else {
                    display.clone()
                },
            );
            paragraph = false;
        } else if rows[n] {
            let leading = n == 0 || !rows[n - 1];
            if !(leading
                && separator(text)
                && !text.contains(':')
                && rows.get(n + 1) == Some(&true))
            {
                result.push(if lists.is_empty() && definition_columns.is_empty() {
                    text.to_owned()
                } else {
                    display.clone()
                });
            }
            paragraph = false;
        } else {
            let attribute = text.starts_with('{')
                && read_djot_word_attributes(text, 0).is_some_and(|(end, _)| end == text.len());
            if attribute_only && !paragraph && !lists.iter().any(|list| indent > list.source) {
                if let Some((_, wire)) = read_djot_word_attributes(text, 0) {
                    pending_root_attributes.push((result.len(), wire));
                }
            }
            if attribute_only && !paragraph {
                if let Some(list) = lists.last_mut().filter(|list| indent > list.source) {
                    if let Some((_, wire)) = read_djot_word_attributes(text, 0) {
                        if wire != "{}" {
                            let held_loose =
                                cached_regex!(r"\x00DJOTITEMATTR\x00(?:[0-9]+\x00)?B[0-9]+\x00$")
                                    .unwrap()
                                    .is_match(
                                        list.start
                                            .checked_sub(2)
                                            .and_then(|row| result.get(row))
                                            .map_or("", String::as_str),
                                    );
                            if !attribute_prefix.is_empty()
                                && previous_blank
                                && !held_loose
                                && !result[list.start].contains(&format!("{attribute_prefix}B"))
                            {
                                let token =
                                    format!("{attribute_prefix}B{}\0", item_attributes.len());
                                item_attributes.push("{loose}".to_owned());
                                result[list.start] = format!(
                                    "{}{token}\n\n{}",
                                    " ".repeat(list.target),
                                    result[list.start]
                                );
                                list.loose = true;
                            }
                            pending_item_attributes.push((list.source, result.len(), wire));
                        }
                    }
                }
            }
            result.push(
                if previous_blank
                    && lists.is_empty()
                    && outer_source.is_some_and(|source| indent <= source)
                {
                    text.to_owned()
                } else {
                    display.clone()
                },
            );
            if previous_blank && !attribute && !reference.is_match(text) {
                for list in &mut lists {
                    if indent > list.source {
                        list.loose = true;
                    }
                }
            }
            paragraph = !in_heading && (paragraph || !(attribute || reference.is_match(text)));
        }
        previous_blank = false;
    }
    if let Some(returned) = returned_attributes {
        for (_, row, attrs) in pending_item_attributes {
            result[row].clear();
            returned.push(attrs);
        }
        for (row, attrs) in pending_root_attributes {
            result[row].clear();
            returned.push(attrs);
        }
    }
    if !divs.is_empty() {
        let trailing = result.last().is_some_and(|line| line.is_empty());
        if trailing {
            result.pop();
        }
        drop_orphan_attribute_line(&mut result);
        if trailing && !result.last().is_some_and(|line| line.is_empty()) {
            result.push(String::new());
        }
    }
    let mut output = result.join("\n");
    if !divs.is_empty() {
        if let Some((kind, width, prefix)) = open_code {
            if !output.ends_with('\n') {
                output.push('\n');
            }
            output.push_str(&format!(
                "{prefix}{}\n",
                (kind as char).to_string().repeat(width)
            ));
        }
    }
    while let Some((width, prefix, _, _, _)) = divs.pop() {
        if !output.ends_with('\n') {
            output.push('\n');
        }
        output.push_str(&format!("{prefix}{}\n", ":".repeat(width)));
    }
    output
}

pub(super) fn table_edges(source: &str) -> String {
    let mask = mask_code_and_destinations(source);
    let rows = djot_table_rows(source, &mask);
    let mut escaped = false;
    let mut offset = 0;
    let masked: Vec<_> = source
        .split('\n')
        .map(|line| {
            // A line the mask hides is code, so its pipes are payload, not table edges.
            let hidden = mask.as_bytes().get(offset) != Some(&b'|');
            offset += line.len() + 1;
            hidden
        })
        .collect();
    let mut lines: Vec<_> = source
        .split('\n')
        .enumerate()
        .map(|(n, line)| {
            if !rows[n] && !masked[n] && line.starts_with("|`") && line.trim_end().ends_with('|') {
                escaped = true;
                format!("\\{line}")
            } else {
                line.to_owned()
            }
        })
        .collect();
    if escaped && source.trim_start_matches('\n').starts_with("|`") {
        while lines.first().is_some_and(|line| line.is_empty()) {
            lines.remove(0);
        }
    }
    lines.join("\n")
}

fn protected_mask(source: &str) -> Vec<u8> {
    mask_djot_opaque(
        &mask_djot_fences(source, None, &[], false),
        OpaqueOptions {
            code: false,
            unclosed_code: false,
            ..OpaqueOptions::default()
        },
    )
}

pub(super) fn quote_spellings(source: &str) -> String {
    let comments = mask_djot_opaque_with_comments(
        &mask_djot_fences(source, None, &[], false),
        OpaqueOptions::default(),
    )
    .1;
    let mut source = source.to_owned();
    for (start, end) in comments.into_iter().rev() {
        let attrs = read_djot_word_attributes(&source, start);
        if attrs.as_ref().is_some_and(|(_, attrs)| attrs != "{}") {
            continue;
        }
        source.replace_range(start..attrs.map_or(end, |(end, _)| end), "{%%}");
    }
    let source = source.as_str();
    let mut mask = mask_code_and_destinations(source).into_bytes();
    let mut at = 0;
    while at < source.len() {
        if mask[at] == b'{' && !is_escaped(source.as_bytes(), at) {
            if let Some((end, attrs)) = read_djot_word_attributes(source, at) {
                if attrs != "{}" {
                    for byte in &mut mask[at..end] {
                        if *byte != b'\n' {
                            *byte = b' ';
                        }
                    }
                }
                at = end;
                continue;
            }
        }
        at += 1;
    }
    cached_regex!(r#"\{["']|["']\}|\{\}"#)
        .unwrap()
        .replace_all(source, |caps: &regex::Captures<'_>| {
            let token = caps.get(0).unwrap();
            if mask[token.start()] != source.as_bytes()[token.start()]
                || is_escaped(source.as_bytes(), token.start())
            {
                return token.as_str().to_owned();
            }
            if token.as_str() == "{}" {
                return if token
                    .start()
                    .checked_sub(1)
                    .and_then(|at| source.as_bytes().get(at))
                    == Some(&b']')
                {
                    "{}"
                } else {
                    "{%%}"
                }
                .to_owned();
            }
            match token.as_str() {
                "{'" => "‘",
                "'}" => "’",
                "{\"" => "“",
                "\"}" => "”",
                _ => token.as_str(),
            }
            .to_owned()
        })
        .into_owned()
}

pub(super) fn inline(source: &str) -> String {
    inline_with_quote_preservation(source, |_| false)
}

pub(super) fn inline_with_quote_preservation(
    source: &str,
    preserve_quote: impl Fn(usize) -> bool,
) -> String {
    let mask = mask_code_and_destinations(source);
    let protected = protected_mask(source);
    let comments: HashMap<_, _> = mask_djot_opaque_with_comments(
        &mask_djot_fences(source, None, &[], false),
        OpaqueOptions::default(),
    )
    .1
    .into_iter()
    .collect();
    let boundary = cached_regex!(r"\n[ \t]*(?:>[ \t]*)*\n").unwrap();
    let bytes = source.as_bytes();
    let mut breaks: Vec<_> = if bytes.contains(&b'`') {
        boundary.find_iter(source).map(|m| m.start()).collect()
    } else {
        Vec::new()
    };
    if bytes.contains(&b'`') && bytes.contains(&b'\n') {
        breaks.extend(
            djot_inline_boundaries_with_code_scopes(
                source,
                &String::from_utf8_lossy(&protected),
                true,
            )
            .into_iter()
            .filter(|&at| at > 0 && at < bytes.len() && bytes[at - 1] == b'\n' && bytes[at] != b'|')
            .map(|at| at - 1),
        );
        breaks.sort_unstable();
    }
    let mut paragraph = 0;
    let mut output = String::new();
    let mut i = 0;
    while i < bytes.len() {
        if let Some(end) = comments.get(&i).copied() {
            if bytes[end - 2] != b'%' && source[i + 2..end].contains('{') {
                output.push_str("{%%}");
                i = end;
                continue;
            }
        }
        if bytes[i] == b'`' && protected[i] == b'`' && !is_escaped(bytes, i) {
            let run = bytes[i..].iter().take_while(|b| **b == b'`').count();
            while breaks.get(paragraph).is_some_and(|end| *end <= i) {
                paragraph += 1;
            }
            let paragraph_end = breaks.get(paragraph).copied().unwrap_or(bytes.len());
            if let Some(end) = find_backtick_close(&bytes[..paragraph_end], i + run, run) {
                let payload = &source[i + run..end];
                let fenced = run >= 3
                    && payload.starts_with('\n')
                    && source[source[..i].rfind('\n').map_or(0, |at| at + 1)..i]
                        .trim()
                        .is_empty();
                output.push_str(&source[i..i + run]);
                let pad = !fenced
                    && payload.starts_with(' ')
                    && payload.ends_with(' ')
                    && !payload.trim().is_empty()
                    && !(payload.trim().starts_with('`') && payload.trim().ends_with('`'));
                if pad {
                    output.push(' ');
                }
                output.push_str(payload);
                if pad {
                    output.push(' ');
                }
                output.push_str(&source[end..end + run]);
                i = end + run;
                continue;
            }
            output.push_str(&source[i..paragraph_end]);
            i = paragraph_end;
            continue;
        }
        if mask.as_bytes()[i] == bytes[i] && protected[i] == bytes[i] && !is_escaped(bytes, i) {
            if bytes[i] == b'{' {
                if let Some((end, _)) = read_djot_word_attributes(source, i) {
                    output.push_str(&source[i..end]);
                    i = end;
                    continue;
                }
            }
            if source[i..].starts_with("{}") {
                if i > 0 && mask.as_bytes()[i - 1] == b']' && !is_escaped(bytes, i - 1) {
                    output.push_str("{}");
                    i += 2;
                    continue;
                }
                let mut end = i + 2;
                while source[end..].starts_with("{}") {
                    end += 2;
                }
                if i > 0
                    && matches!(bytes[i - 1], b'_' | b'*')
                    && bytes.get(end) == Some(&bytes[i - 1])
                {
                    output.push_str(&source[i..end]);
                    i = end;
                    continue;
                }
                let empty_line =
                    (i == 0 || bytes[i - 1] == b'\n') && bytes.get(i + 2) == Some(&b'\n');
                i += if empty_line { 3 } else { 2 };
                continue;
            }
            let quote = [("{'", "‘"), ("'}", "’"), ("{\"", "“"), ("\"}", "”")]
                .into_iter()
                .find(|(token, _)| source[i..].starts_with(token));
            if let Some((token, glyph)) = quote.filter(|_| !preserve_quote(i)) {
                output.push_str(glyph);
                i += token.len();
                continue;
            }
            if bytes[i] == b'\\' {
                let mut end = i + 1;
                while matches!(bytes.get(end), Some(b' ' | b'\t')) {
                    end += 1;
                }
                let line_start = source[..i].rfind('\n').map_or(0, |at| at + 1);
                let prefix = source[line_start..i].trim();
                let fence = prefix.len() >= 3 && prefix.bytes().all(|byte| byte == b':');
                if bytes.get(end) == Some(&b'\n') && output.ends_with([' ', '\t']) && !fence {
                    let mut trim = output.len();
                    while trim > 0
                        && matches!(output.as_bytes()[trim - 1], b' ' | b'\t')
                        && !is_escaped(output.as_bytes(), trim - 1)
                    {
                        trim -= 1;
                    }
                    output.truncate(trim);
                    output.push_str("\\\n");
                    i = end + 1;
                    continue;
                }
            }
        }
        let ch = source[i..].chars().next().unwrap();
        output.push(ch);
        i += ch.len_utf8();
    }
    output
}

fn definition_label(text: &str) -> String {
    text.trim()
        .split([' ', '\t', '\r', '\n'])
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn label(text: &str) -> String {
    let decoded = cached_regex!(r"\\([ \t!-/:-@\[-`{-~])")
        .unwrap()
        .replace_all(text, "$1");
    definition_label(&decoded)
}

pub(super) fn image_label_attributes(source: &str) -> String {
    let mut mask = mask_djot_forms_with_options(
        source,
        true,
        false,
        None,
        &[],
        OpaqueOptions {
            autolinks: false,
            ..Default::default()
        },
    )
    .into_bytes();
    let (_, comments) = mask_djot_opaque_with_comments(source, OpaqueOptions::default());
    for (start, _) in comments {
        mask[start] = source.as_bytes()[start];
    }
    let framed = format!("DJOTALT {source}");
    let bytes = source.as_bytes();
    let mut result = String::new();
    let mut cursor = 0;
    let mut note_end = None;
    let mut at = 0;
    while at < bytes.len() {
        if mask[at] != bytes[at] || is_escaped(bytes, at) {
            at += 1;
            continue;
        }
        if bytes[at] == b':' {
            if let Some(symbol) = cached_regex!(r"^:[a-zA-Z0-9_+-]+:")
                .unwrap()
                .find(&source[at..])
            {
                result.push_str(&source[cursor..at]);
                result.push_str("{%%}");
                cursor = at + symbol.end();
                at = cursor;
                continue;
            }
        }
        if bytes[at] == b'<' {
            if let Some(autolink) = cached_regex!(r"^<([^<>\s]+)>")
                .unwrap()
                .captures(&source[at..])
            {
                let body = &autolink[1];
                if cached_regex!(r"[^:]@|[A-Za-z]:").unwrap().is_match(body) {
                    let width = cached_regex!(r"`+")
                        .unwrap()
                        .find_iter(body)
                        .map(|run| run.len())
                        .max()
                        .unwrap_or(0)
                        + 1;
                    let ticks = "`".repeat(width);
                    result.push_str(&source[cursor..at]);
                    result.push_str(&format!("{ticks} {body} {ticks}"));
                    cursor = at + autolink[0].len();
                    at = cursor;
                    continue;
                }
            }
        }
        if bytes[at] == b'['
            && bytes.get(at + 1) == Some(&b'^')
            && !(note_end != Some(at)
                && at > 0
                && bytes[at - 1] == b']'
                && !is_escaped(bytes, at - 1))
        {
            let mut end = at + 2;
            while end < bytes.len()
                && !(mask[end] == bytes[end]
                    && !is_escaped(bytes, end)
                    && matches!(bytes[end], b'[' | b']'))
            {
                end += 1;
            }
            if bytes.get(end) == Some(&b']') {
                result.push_str(&source[cursor..at]);
                result.push_str("{%%}");
                cursor = end + 1;
                note_end = Some(cursor);
                at = cursor;
                continue;
            }
        }
        if bytes[at] == b'{' {
            if let Some((end, _)) = read_djot_word_attributes(&framed, at + 8) {
                result.push_str(&source[cursor..at]);
                result.push_str(
                    if note_end != Some(at)
                        && at > 0
                        && bytes[at - 1] == b']'
                        && !is_escaped(bytes, at - 1)
                    {
                        "{}"
                    } else {
                        "{%%}"
                    },
                );
                cursor = end - 8;
                at = cursor;
                continue;
            }
        }
        at += 1;
    }
    result.push_str(&source[cursor..]);
    result
}

fn reference_labels_as_spans(source: &str) -> String {
    let mask = mask_code_and_destinations(source);
    let bytes = source.as_bytes();
    let destinations = djot_simple_destination_ranges(source).unwrap_or_else(|| {
        djot_destination_ranges(
            source,
            &mask_djot_forms_with_options(
                source,
                false,
                false,
                None,
                &[],
                OpaqueOptions {
                    destinations: false,
                    ..Default::default()
                },
            ),
        )
    });
    let destinations: HashMap<_, _> = destinations.into_iter().collect();
    let mut reference_ends = vec![None; bytes.len() + 1];
    let mut end = None;
    for at in (0..bytes.len()).rev() {
        if !is_escaped(bytes, at) {
            if bytes[at] == b'[' {
                end = None;
            } else if bytes[at] == b']' {
                end = Some(at + 1);
            }
        }
        reference_ends[at] = end;
    }
    let mut brackets = Vec::new();
    let mut edits = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        if mask.as_bytes()[at] == bytes[at] && !is_escaped(bytes, at) {
            if bytes[at] == b'[' {
                brackets.push(at);
            } else if bytes[at] == b']' {
                if let Some(open) = brackets.pop() {
                    if bytes.get(open + 1) != Some(&b'^') {
                        let end = destinations.get(&(at + 1)).copied().or_else(|| {
                            (bytes.get(at + 1) == Some(&b'['))
                                .then(|| reference_ends[at + 2])
                                .flatten()
                        });
                        if let Some(end) = end {
                            if open > 0 && bytes[open - 1] == b'!' && !is_escaped(bytes, open - 1) {
                                edits.push((open - 1, open, ""));
                            }
                            edits.push((at + 1, end, "{}"));
                            at = end - 1;
                        }
                    }
                }
            }
        }
        at += 1;
    }
    edits.sort_unstable_by_key(|edit| edit.0);
    let mut result = String::new();
    let mut cursor = 0;
    for (start, end, replacement) in edits {
        result.push_str(&source[cursor..start]);
        result.push_str(replacement);
        cursor = end;
    }
    result.push_str(&source[cursor..]);
    result
}

fn reference_label(text: &str, explicit: &str) -> String {
    if !explicit.is_empty() {
        return label(explicit);
    }
    if !text.bytes().any(|b| b"_*`{~^\\[]=!/<:\n\r\"'".contains(&b)) {
        return label(text);
    }
    let source = reference_labels_as_spans(&image_label_attributes(text));
    let source = if source.contains('\0') {
        let (attributes, inherited) = escape_invalid_djot_attributes(&source);
        remove_inherited_attribute_markers(&attributes, &inherited)
    } else {
        source
    };
    let converted =
        emphasis::convert_image_child(&source, &mask_code_and_destinations(&source), |text| {
            escape_plain_carve_syntax(text, HandledDelimiters::DJOT)
        });
    let options = crate::Options {
        smart_typography: crate::SmartTypographyMode::Source,
        ..Default::default()
    };
    let nul_token = if converted.contains('\0') {
        let occupied = crate::sentinel_run::occupied_private_use(&converted);
        Some(crate::sentinel_run::pick_sentinel_run::<1>(&occupied, 0xe001)[0].to_string())
    } else {
        None
    };
    let converted = nul_token.as_ref().map_or_else(
        || std::borrow::Cow::Borrowed(converted.as_str()),
        |token| std::borrow::Cow::Owned(converted.replace('\0', token)),
    );
    let nodes = crate::parse::parse_inline_with_options(&converted, &options);
    let mut pending: Vec<_> = nodes.iter().rev().collect();
    let mut text = String::new();
    while let Some(node) = pending.pop() {
        use crate::ast::InlineNode;
        let children: Option<&[InlineNode]> = match node {
            InlineNode::NonBreakingSpace(_) | InlineNode::Footnote(_) => continue,
            InlineNode::SoftBreak(_) | InlineNode::HardBreak(_) => {
                text.push('\n');
                continue;
            }
            InlineNode::RawInline(node) => {
                text.push_str(&node.content);
                continue;
            }
            InlineNode::Emphasis(node) => Some(&node.children),
            InlineNode::Span(node) => Some(&node.children),
            InlineNode::Link(node) => Some(&node.children),
            InlineNode::Extension(node) => Some(&node.children),
            _ => None,
        };
        if let Some(children) = children {
            pending.extend(children.iter().rev());
        } else {
            text.push_str(&crate::render::plain_inlines_typography(
                std::slice::from_ref(node),
                crate::SmartTypographyMode::Source,
            ));
        }
    }
    if let Some(token) = nul_token {
        text = text.replace(&token, "\0");
    }
    definition_label(&text)
}

fn literal_quote_depths(source: &str, mask: &str, rows: &[bool]) -> HashMap<usize, usize> {
    let mut depths = HashMap::new();
    let fences = mask_djot_fences(source, None, &[], false);
    let has_definitions = cached_regex!(r"(?:^|\n)(?:[ \t]*>(?:[ \t]|$))*[ \t]*:[ \t]")
        .unwrap()
        .is_match(mask);
    quoted_list_boundaries(
        source,
        mask,
        &fences,
        rows,
        None,
        has_definitions,
        Some(&mut depths),
    );
    depths
}

fn projected_reference_text(text: &str, depth: usize) -> String {
    let mut projected = String::new();
    for (n, line) in text.split('\n').enumerate() {
        let mut line = line;
        if n > 0 {
            projected.push('\n');
            for _ in 0..depth {
                let trimmed = line.trim_start_matches([' ', '\t']);
                if !starts_quote(trimmed) {
                    break;
                }
                line = &trimmed[1..];
                if line.starts_with([' ', '\t']) {
                    line = &line[1..];
                }
            }
        }
        projected.push_str(line);
    }
    projected
}

fn cached_reference_label<'a>(
    text: &'a str,
    explicit: &str,
    depth: usize,
    cache: &mut HashMap<(&'a str, usize), String>,
) -> String {
    const MAX_CACHED_REFERENCE_LABEL_BYTES: usize = 256;
    if !explicit.is_empty() {
        return label(explicit);
    }
    if !text
        .bytes()
        .any(|byte| b"_*`{~^\\[]=!/<:\n\r\"'".contains(&byte))
    {
        return label(text);
    }
    // Bound retained values when nested labels overlap in the source.
    if text.len() > MAX_CACHED_REFERENCE_LABEL_BYTES {
        return reference_label(&projected_reference_text(text, depth), explicit);
    }
    cache
        .entry((text, depth))
        .or_insert_with(|| {
            let projected = projected_reference_text(text, depth);
            reference_label(&projected, explicit)
        })
        .clone()
}

fn attributes(text: &str) -> Vec<(String, Vec<String>)> {
    let mut result: Vec<(String, Vec<String>)> = Vec::new();
    let mut slots: HashMap<String, usize> = HashMap::new();
    let mut at = 0;
    while let Some((end, _, tokens)) = read_djot_attribute_tokens(text, at) {
        let mut group: Vec<(String, Vec<String>)> = Vec::new();
        let mut group_slots: HashMap<String, usize> = HashMap::new();
        for token in tokens {
            if let Some(&slot) = group_slots.get(&token.key) {
                let values = &mut group[slot].1;
                if !token.append || token.key == "class" && values.as_slice() == ["class=\"\""] {
                    values.clear();
                }
                values.push(token.source);
            } else {
                group_slots.insert(token.key.clone(), group.len());
                group.push((token.key, vec![token.source]));
            }
        }
        for (key, values) in group {
            if let Some(&slot) = slots.get(&key) {
                let previous = &mut result[slot].1;
                if key != "class" || previous.as_slice() == ["class=\"\""] {
                    previous.clear();
                }
                previous.extend(values);
            } else {
                slots.insert(key.clone(), result.len());
                result.push((key, values));
            }
        }
        at = end;
        if at == text.len() {
            break;
        }
    }
    result
}

struct ReferenceUse<'a> {
    start: usize,
    end: usize,
    text: &'a str,
    label: &'a str,
    quote_depth: usize,
}

struct AttributedReferenceUse<'a> {
    reference: ReferenceUse<'a>,
    key: String,
    own: Vec<(String, Vec<String>)>,
    end: usize,
    table: bool,
}

fn reference_mask(source: &str) -> String {
    reference_mask_with_options(source, OpaqueOptions::default())
}

fn reference_mask_with_options(source: &str, options: OpaqueOptions<'_>) -> String {
    let mut mask =
        mask_djot_forms_with_options(source, false, true, None, &[], options).into_bytes();
    let mut at = 0;
    while at < source.len() {
        if mask[at] == b'{' && !is_escaped(source.as_bytes(), at) {
            if let Some((end, _)) = read_djot_word_attributes(source, at) {
                blank_out(&mut mask, at, end);
                at = end;
                continue;
            }
        }
        at += 1;
    }
    String::from_utf8(mask).unwrap()
}

fn reference_uses<'a>(source: &'a str, mask: &str) -> Vec<ReferenceUse<'a>> {
    let bytes = source.as_bytes();
    let mut table_pipes = HashSet::new();
    if source.contains('|') {
        let pipe_mask = mask_djot_forms_with_options(
            source,
            false,
            true,
            None,
            &[],
            OpaqueOptions {
                destinations: false,
                autolinks: false,
                attribute_values: false,
                ..Default::default()
            },
        );
        let rows = djot_table_rows(source, &pipe_mask);
        let mut offset = 0;
        for (n, line) in source.split('\n').enumerate() {
            if rows[n] {
                let mut escaped = false;
                for (at, byte) in line.bytes().enumerate() {
                    if byte == b'|' && pipe_mask.as_bytes()[offset + at] == b'|' && !escaped {
                        table_pipes.insert(offset + at);
                    }
                    escaped = byte == b'\\' && !escaped;
                }
            }
            offset += line.len() + 1;
        }
    }
    let mut line_depths = Vec::new();
    let mut line_at = 0;
    for line in source.split_inclusive('\n') {
        let prefix = &line[..djot_content_start(line)];
        line_depths.push((line_at, prefix.bytes().filter(|&byte| byte == b'>').count()));
        line_at += line.len();
    }
    if line_depths.iter().any(|&(_, depth)| depth > 0) {
        let rows = djot_table_rows(source, mask);
        let literal = literal_quote_depths(source, mask, &rows);
        for (at, depth) in &mut line_depths {
            if let Some(&owner) = literal.get(at) {
                *depth = owner;
            }
        }
    }
    let mut stack = Vec::new();
    let mut uses = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' {
            i += 2;
            continue;
        }
        if table_pipes.contains(&i) {
            stack.clear();
        }
        if mask.as_bytes()[i] != bytes[i] {
            i += 1;
            continue;
        }
        if bytes[i] == b'\n' && blank_line_follows(bytes, i) {
            stack.clear();
        }
        if bytes[i] == b'[' {
            stack.push(i);
        }
        if bytes[i] == b']' {
            if let Some(start) = stack.pop() {
                if !(bytes.get(start + 1) == Some(&b'^') && i > start + 2)
                    && bytes.get(i + 1) == Some(&b'[')
                {
                    let mut end = i + 2;
                    while end < bytes.len()
                        && mask.as_bytes()[end] != b']'
                        && mask.as_bytes()[end] != b'['
                    {
                        if table_pipes.contains(&end) {
                            break;
                        }
                        if bytes[end] == b'\n' && blank_line_follows(bytes, end) {
                            break;
                        }
                        if bytes[end] == b'\\' {
                            end += 1;
                        }
                        end += 1;
                    }
                    if mask.as_bytes().get(end) == Some(&b']') {
                        uses.push(ReferenceUse {
                            start,
                            end: end + 1,
                            text: &source[start + 1..i],
                            label: &source[i + 2..end],
                            quote_depth: line_depths[line_depths
                                .partition_point(|&(at, _)| at <= start)
                                .saturating_sub(1)]
                            .1,
                        });
                        i = end;
                    }
                }
            }
        }
        i += 1;
    }
    uses
}

fn next_reference_alias(occupied: &mut HashSet<String>, serial: &mut usize) -> String {
    loop {
        *serial += 1;
        let alias = format!("carve-djot-reference-{serial}");
        if occupied.insert(alias.clone()) {
            return alias;
        }
    }
}

fn reference_attribute_text(attrs: &[(String, Vec<String>)], table: bool) -> String {
    let mut text = attrs
        .iter()
        .flat_map(|(key, values)| {
            if key != "class" {
                return values.clone();
            }
            let classes = values
                .iter()
                .map(|value| {
                    if let Some(value) = value.strip_prefix('.') {
                        value.to_owned()
                    } else {
                        let value = value.strip_prefix("class=").unwrap();
                        crate::parse::unescape_title(
                            value
                                .strip_prefix('"')
                                .and_then(|value| value.strip_suffix('"'))
                                .unwrap_or(value),
                        )
                    }
                })
                .collect::<Vec<_>>()
                .join(" ");
            vec![format!(
                "class=\"{}\"",
                classes.replace('\\', "\\\\").replace('"', "\\\"")
            )]
        })
        .map(|value| {
            if let Some((key, quoted)) = value.split_once("=\"") {
                let atom = quoted.strip_suffix('"').unwrap_or(quoted);
                if !atom.is_empty()
                    && atom
                        .chars()
                        .all(|ch| ch.is_ascii_alphanumeric() || "_-.".contains(ch))
                {
                    return format!("{key}={atom}");
                }
            }
            value.to_owned()
        })
        .collect::<Vec<_>>()
        .join(" ");
    if table && text.contains('|') {
        let mut escaped = String::with_capacity(text.len());
        let mut slashes = 0;
        for ch in text.chars() {
            if ch == '|' && slashes % 2 == 0 {
                escaped.push('\\');
            }
            escaped.push(ch);
            slashes = if ch == '\\' { slashes + 1 } else { 0 };
        }
        text = escaped;
    }
    if text.is_empty() {
        text
    } else {
        format!("{{{text}}}")
    }
}

pub(super) fn references(source: &str) -> String {
    references_impl(source, None, false, None)
}

pub(super) fn caret_references(
    source: &str,
    accepted: &HashMap<usize, ReferenceDecision>,
) -> String {
    references_impl(source, Some(accepted), false, None)
}

pub(super) fn reference_key_needs_alias(key: &str) -> bool {
    key.bytes().any(|byte| b"`{^\\|[\0".contains(&byte))
        || key.bytes().any(|byte| b"_*~\"'<:".contains(&byte)) && reference_label(key, "") != key
}

pub(super) fn literal_reference_keys(source: &str) -> String {
    references_impl(source, None, true, None)
}

pub(super) fn heading_reference_keys(source: &str, targets: &HashMap<String, String>) -> String {
    references_impl(source, None, true, Some(targets))
}

fn references_impl(
    source: &str,
    accepted: Option<&HashMap<usize, ReferenceDecision>>,
    aliases_only: bool,
    heading_targets: Option<&HashMap<String, String>>,
) -> String {
    let _paragraph_cache = ParagraphCacheScope::new(source);
    let definition = cached_regex!(r"(?m)^\[([^\]\n]+)\]:[ \t]*([^ \t\r\n]*)\r?$").unwrap();
    let quote_prefix = cached_regex!(r"^(?:[ \t]*>[ \t]*)*[ \t]*").unwrap();
    let list_marker = cached_regex!(r"(?:[-*+] |[0-9A-Za-z]+[.)] |\([0-9A-Za-z]+\) )").unwrap();
    let block_end = cached_regex!(
        r"^(?:#{1,6}(?: |$)|:{3,}|[`~]{3,}|\||\[[^\]^][^\]]*\]:|(?:[-*][ \t]*){3,}$)"
    )
    .unwrap();
    let mut quote_openings = HashSet::new();
    let mut previous_quote_depth = 0;
    let mut quote_offset = 0;
    for line in source.split_inclusive('\n') {
        let content = line.trim_end_matches('\n');
        let depth = content[..djot_content_start(content)]
            .bytes()
            .filter(|&byte| byte == b'>')
            .count();
        if depth > previous_quote_depth {
            quote_openings.insert(quote_offset);
        }
        previous_quote_depth = depth;
        quote_offset += line.len();
    }
    let mask = reference_mask(source);
    let mut definitions = HashMap::new();
    let mut removed = Vec::new();
    let mut definition_ranges: HashMap<String, Vec<(usize, usize)>> = HashMap::new();
    let mut definition_keys = Vec::new();
    let mut indented_definitions = Vec::new();
    let mut container_definitions = Vec::new();
    let mut line_offset = 0;
    let mut valid_definition_lines = None;
    for (line_number, line) in source.split_inclusive('\n').enumerate() {
        let line_start = line_offset;
        line_offset += line.len();
        let content_start = djot_content_start(line);
        let Some(c) = definition.captures(line[content_start..].trim_end_matches('\n')) else {
            continue;
        };
        let caret = c[1].starts_with('^');
        if caret
            && c[1].len() > 1
            && matches!(
                line.as_bytes()[content_start..].get(c[1].len() + 3),
                None | Some(b' ' | b'\t' | b'\r' | b'\n')
            )
        {
            continue;
        }
        if accepted.is_some_and(|decisions| !caret || !decisions.contains_key(&line_number)) {
            continue;
        }
        if (content_start > 0 || aliases_only)
            && !valid_definition_lines
                .get_or_insert_with(|| reference_definitions(source, &mask).1)
                .contains(&line_number)
        {
            continue;
        }
        let start = line_start + content_start;
        let definition_end = start + c.get(0).unwrap().end();
        if mask.as_bytes()[start] != b'[' {
            continue;
        }
        if content_start > 0 && line[..content_start].trim().is_empty() {
            indented_definitions.push((line_start, start, definition_end, definition_label(&c[1])));
        } else if content_start > 0 {
            container_definitions.push((start, definition_end, definition_label(&c[1])));
        }
        let mut previous_end = line_start;
        let mut attr_parts = Vec::new();
        let removed_start = removed.len();
        let mut first_attribute = line_start;
        let mut attribute_starts = Vec::new();
        let mut attribute_depth: Option<usize> = None;
        while let Some(previous) = source[..previous_end]
            .strip_suffix('\n')
            .filter(|_| accepted.is_none() && !aliases_only)
        {
            let previous_start = previous.rfind('\n').map_or(0, |at| at + 1);
            let previous_line = &previous[previous_start..];
            let attribute_start = djot_content_start(previous_line);
            let attribute_content = &previous_line[attribute_start..];
            let attribute_prefix = &previous_line[..attribute_start];
            let prefix = &line[..content_start];
            let opens_container = prefix.bytes().any(|byte| b"-*+.):".contains(&byte))
                || prefix.bytes().filter(|&byte| byte == b'>').count()
                    > attribute_prefix
                        .bytes()
                        .filter(|&byte| byte == b'>')
                        .count();
            if opens_container {
                break;
            }
            let depth = attribute_prefix
                .bytes()
                .filter(|&byte| byte == b'>')
                .count();
            if attribute_content.trim().is_empty() {
                if quote_openings.contains(&previous_start) {
                    break;
                }
                if attribute_depth.is_some_and(|previous| depth < previous) {
                    break;
                }
                previous_end = previous_start;
                continue;
            }
            if attribute_depth.is_some_and(|previous| depth < previous) {
                break;
            }
            let marker_owned = list_marker.is_match(attribute_prefix);
            let previous_body = &source[..previous_start];
            let previous_body = previous_body
                .trim_end_matches('\n')
                .rsplit('\n')
                .next()
                .unwrap_or("");
            let previous_body = &previous_body[djot_content_start(previous_body)..];
            let after_div_close = cached_regex!(r"^:{3,}[ \t]*$")
                .unwrap()
                .is_match(previous_body);
            if !attribute_content.starts_with('{')
                || !marker_owned
                    && !after_div_close
                    && comment_continues_paragraph(source, previous_start + attribute_start)
                || read_djot_word_attributes(attribute_content, 0)
                    .map_or(true, |(end, _)| end != attribute_content.len())
            {
                break;
            }
            attribute_depth = Some(depth);
            attr_parts.push(attribute_content);
            first_attribute = previous_start;
            attribute_starts.push(previous_start);
            removed.push((
                previous_start + attribute_start,
                previous_start + previous_line.len() + usize::from(attribute_start == 0),
                if attribute_start > 0 {
                    "{%%}".to_owned()
                } else {
                    String::new()
                },
            ));
            previous_end = previous_start;
            if quote_openings.contains(&previous_start) {
                break;
            }
        }
        if !attr_parts.is_empty() && first_attribute > 0 {
            let before = source[..first_attribute].strip_suffix('\n').unwrap_or("");
            let before = before.rsplit('\n').next().unwrap_or("");
            let before_start = djot_content_start(before);
            let first_line = source[first_attribute..].split('\n').next().unwrap_or("");
            let note_at = before[..before_start].find("[^");
            let owner_start = note_at.map_or(before_start, |at| at + 1);
            let quote_depth = |line: &str| {
                quote_prefix
                    .find(line)
                    .unwrap()
                    .as_str()
                    .bytes()
                    .filter(|&byte| byte == b'>')
                    .count()
            };
            let outdented = quote_depth(first_line) < quote_depth(before)
                || djot_content_start(first_line) < owner_start
                    && (note_at.is_some() || list_marker.is_match(&before[..before_start]));
            let before = before[before_start..].trim();
            let before_attribute = before.starts_with('{')
                && read_djot_word_attributes(before, 0).is_some_and(|(end, _)| end == before.len());
            if !before.is_empty() && !outdented && !before_attribute && !block_end.is_match(before)
            {
                let tail = attribute_starts.iter().rposition(|&at| {
                    let previous = source[..at].strip_suffix('\n').unwrap_or("");
                    let previous = previous.rsplit('\n').next().unwrap_or("");
                    previous[djot_content_start(previous)..].trim().is_empty()
                });
                let count = tail.map_or(0, |index| index + 1);
                attr_parts.truncate(count);
                removed.truncate(removed_start + count);
            } else if !before.is_empty() && content_start == 0 {
                removed.push((start, start, "\n".to_owned()));
            }
        }
        attr_parts.reverse();
        let attr_text = attr_parts.join("");
        let key = definition_label(&c[1]);
        definition_keys.push(key.clone());
        definition_ranges
            .entry(key.clone())
            .or_default()
            .push((start, definition_end));
        definitions.insert(
            key,
            (
                c[2].to_owned(),
                attributes(&attr_text),
                !attr_parts.is_empty(),
            ),
        );
    }
    if definitions.is_empty() && heading_targets.map_or(true, HashMap::is_empty) {
        return source.to_owned();
    }
    let mut seen_keys = HashSet::new();
    definition_keys.retain(|key| seen_keys.insert(key.clone()));
    let references = reference_uses(source, &mask);
    let mut table_uses = HashSet::new();
    let mut line_start = 0;
    let mut reference_index = 0;
    for line in source.split_inclusive('\n') {
        let line_end = line_start + line.len();
        let table = line[djot_content_start(line)..].starts_with('|');
        while let Some(reference) = references
            .get(reference_index)
            .filter(|reference| reference.start < line_end)
        {
            if table {
                table_uses.insert(reference.start);
            }
            reference_index += 1;
        }
        line_start = line_end;
    }
    let mut label_cache = HashMap::new();
    let mut use_counts = HashMap::<String, usize>::new();
    let mut override_counts = HashMap::<String, HashMap<String, usize>>::new();
    let uses: Vec<_> = references
        .into_iter()
        .filter(|reference| !reference.text.starts_with('^') || reference.text.len() == 1)
        .map(|reference| {
            let key = cached_reference_label(
                reference.text,
                reference.label,
                reference.quote_depth,
                &mut label_cache,
            );
            let mut end = reference.end;
            let mut own = Vec::new();
            if definitions.get(&key).is_some_and(|(target, _, inherited)| {
                !target.is_empty() && (*inherited || source[end..].starts_with('{'))
            }) {
                let mut parts = Vec::new();
                while source[end..].starts_with('{') {
                    let Some((length, _)) = read_djot_word_attributes(&source[end..], 0) else {
                        break;
                    };
                    parts.push(&source[end..end + length]);
                    end += length;
                }
                own = attributes(&parts.join(""));
                *use_counts.entry(key.clone()).or_default() += 1;
                let counts = override_counts.entry(key.clone()).or_default();
                for (attribute, _) in &own {
                    *counts.entry(attribute.clone()).or_default() += 1;
                }
            }
            AttributedReferenceUse {
                table: table_uses.contains(&reference.start),
                reference,
                key,
                own,
                end,
            }
        })
        .collect();
    for (key, (_, attrs, _)) in &mut definitions {
        if let Some(counts) = override_counts.get(key) {
            attrs.retain(|(attribute, _)| counts.get(attribute) != use_counts.get(key));
        }
    }
    let mut occupied: HashSet<String> = definitions.keys().cloned().collect();
    if let Some(decisions) = accepted {
        occupied.extend(
            decisions
                .values()
                .map(|decision| definition_label(&decision.label)),
        );
    }
    occupied.extend(uses.iter().map(|reference| reference.key.clone()));
    let mut serial = 0;
    let mut aliases = HashMap::new();
    let has_source_nuls = source.contains("\0U\0");
    for key in &definition_keys {
        if (reference_key_needs_alias(key) || aliases_only && has_source_nuls && key.contains('�'))
            && !aliases.contains_key(key)
        {
            aliases.insert(
                key.clone(),
                next_reference_alias(&mut occupied, &mut serial),
            );
        }
    }
    if aliases_only {
        let mut edits = Vec::new();
        let mut heading_edits = Vec::new();
        for (key, alias) in &aliases {
            for &(start, end) in definition_ranges.get(key).into_iter().flatten() {
                if let Some(close) = source[start..end].find("]:") {
                    edits.push((start + 1, start + close, alias.as_str()));
                }
            }
        }
        let mut unresolved = Vec::new();
        if has_source_nuls && aliases.keys().any(|key| key.contains('�')) {
            let native = reference_definitions(source, &mask).0;
            unresolved.extend(
                uses.iter()
                    .filter(|outer| {
                        !native.contains_key(&outer.key)
                            || !outer.reference.label.is_empty()
                                && !definitions.contains_key(&outer.key)
                    })
                    .map(|outer| {
                        (
                            outer.reference.start,
                            outer.reference.start + outer.reference.text.len() + 1,
                        )
                    }),
            );
            unresolved.sort_unstable_by_key(|range| range.0);
            let mut end = 0;
            for range in &mut unresolved {
                end = end.max(range.1);
                range.1 = end;
            }
        }
        for attributed in &uses {
            let before = unresolved.partition_point(|range| range.0 < attributed.reference.start);
            if before > 0 && unresolved[before - 1].1 >= attributed.end {
                continue;
            }
            let heading_key = attributed.key.replace("\0U\0", "\0");
            if heading_key.contains('\0') && !definitions.contains_key(&attributed.key) {
                if let Some(target) = heading_targets.and_then(|targets| targets.get(&heading_key))
                {
                    let reference = &attributed.reference;
                    let target = target
                        .replace('\\', "\\\\")
                        .replace('(', "\\(")
                        .replace(')', "\\)");
                    heading_edits.push((
                        reference.end - reference.label.len() - 2,
                        reference.end,
                        format!("({target})"),
                    ));
                    continue;
                }
            }
            if let Some(alias) = aliases.get(&attributed.key) {
                let reference = &attributed.reference;
                edits.push((
                    reference.end - reference.label.len() - 1,
                    reference.end - 1,
                    alias.as_str(),
                ));
            }
        }
        edits.extend(
            heading_edits
                .iter()
                .map(|(start, end, text)| (*start, *end, text.as_str())),
        );
        edits.sort_unstable_by_key(|edit| edit.0);
        let mut result = String::new();
        let mut cursor = 0;
        for (start, end, replacement) in edits {
            result.push_str(&source[cursor..start]);
            result.push_str(replacement);
            cursor = end;
        }
        result.push_str(&source[cursor..]);
        return result;
    }
    let mut moved_definitions = Vec::new();
    let mut lifted_attributes = HashSet::new();
    for (start, end, key) in container_definitions {
        if definition_ranges
            .get(&key)
            .and_then(|ranges| ranges.last())
            .copied()
            != Some((start, end))
        {
            continue;
        }
        let (target, attrs, _) = &definitions[&key];
        if target.is_empty() || attrs.is_empty() {
            continue;
        }
        let alias = aliases
            .entry(key.clone())
            .or_insert_with(|| next_reference_alias(&mut occupied, &mut serial));
        moved_definitions.push(format!(
            "[{alias}]: {target} {}",
            reference_attribute_text(attrs, false)
        ));
        lifted_attributes.insert(key);
    }
    let mut class_aliases = HashMap::new();
    for key in &definition_keys {
        if class_aliases.contains_key(key) {
            continue;
        }
        let (target, attrs, _) = &definitions[key];
        if target.is_empty()
            || !attrs.iter().any(|(attribute, _)| attribute == "class")
            || !override_counts
                .get(key)
                .is_some_and(|counts| counts.contains_key("class"))
        {
            continue;
        }
        let alias = next_reference_alias(&mut occupied, &mut serial);
        let attrs: Vec<_> = attrs
            .iter()
            .filter(|(attribute, _)| attribute != "class")
            .cloned()
            .collect();
        let text = reference_attribute_text(&attrs, false);
        moved_definitions.push(format!(
            "[{alias}]: {target}{}",
            if text.is_empty() {
                text
            } else {
                format!(" {text}")
            }
        ));
        class_aliases.insert(key.clone(), alias);
    }
    let trim_definition_space = !removed.is_empty();
    let mut edits = removed;
    let mut moved_keys = HashSet::new();
    for (line_start, start, end, key) in indented_definitions {
        if definition_ranges
            .get(&key)
            .and_then(|ranges| ranges.last())
            .copied()
            != Some((start, end))
        {
            continue;
        }
        let close = source[start..end].find("]:").unwrap();
        let name = aliases
            .get(&key)
            .map(String::as_str)
            .unwrap_or(&source[start + 1..start + close]);
        let (target, attrs, _) = &definitions[&key];
        let text = if target.is_empty() {
            String::new()
        } else {
            reference_attribute_text(attrs, false)
        };
        moved_definitions.push(format!(
            "[{name}{}{}",
            &source[start + close..end],
            if text.is_empty() {
                text
            } else {
                format!(" {text}")
            }
        ));
        moved_keys.insert(key.clone());
        edits.push((
            line_start,
            end,
            format!("{}{{%%}}", &source[line_start..start]),
        ));
    }
    for ranges in definition_ranges.values() {
        for &(start, end) in ranges.iter().take(ranges.len().saturating_sub(1)) {
            edits.push((start, end, "{%%}".to_owned()));
        }
    }
    for (key, alias) in &aliases {
        if lifted_attributes.contains(key) {
            continue;
        }
        for &(start, end) in definition_ranges.get(key).into_iter().flatten() {
            if let Some(close) = source[start..end].find("]:") {
                edits.push((start + 1, start + close, alias.clone()));
            }
        }
    }
    for (key, (target, attrs, _)) in &definitions {
        if target.is_empty()
            || attrs.is_empty()
            || moved_keys.contains(key)
            || lifted_attributes.contains(key)
        {
            continue;
        }
        if let Some(&(_, end)) = definition_ranges.get(key).and_then(|ranges| ranges.last()) {
            edits.push((
                end,
                end,
                format!(" {}", reference_attribute_text(attrs, false)),
            ));
        }
    }
    for attributed in uses {
        let AttributedReferenceUse {
            reference,
            key,
            own,
            end,
            table,
        } = attributed;
        let start = reference.start;
        let Some((target, _, has_attribute_lines)) = definitions.get(&key) else {
            continue;
        };
        if accepted.is_some() {
            if let Some(alias) = aliases.get(&key) {
                edits.push((
                    start,
                    end,
                    format!(
                        "[{}][{alias}]{}",
                        reference.text,
                        &source[reference.end..end]
                    ),
                ));
            }
            continue;
        }
        if target.is_empty() {
            continue;
        }
        let replacement = if *has_attribute_lines || end > reference.end {
            let name = class_aliases
                .get(&key)
                .filter(|_| own.iter().any(|(attribute, _)| attribute == "class"))
                .or_else(|| aliases.get(&key))
                .map(String::as_str)
                .unwrap_or(&key);
            format!(
                "[{}][{name}]{}",
                reference.text,
                reference_attribute_text(&own, table)
            )
        } else if let Some(alias) = aliases.get(&key) {
            format!("[{}][{alias}]", reference.text)
        } else if (!reference.label.is_empty() && reference.label != key)
            || (reference.label.is_empty() && definition_label(reference.text) != key)
        {
            format!("[{}][{key}]", reference.text)
        } else {
            continue;
        };
        edits.push((start, end, replacement));
    }
    edits.sort_by_key(|edit| edit.0);
    let mut out = String::new();
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
    if out.starts_with('\n') && !source.starts_with('\n') {
        out = out.trim_start_matches('\n').to_owned();
    }
    if trim_definition_space && source.ends_with('\n') {
        out.truncate(out.trim_end_matches('\n').len());
        if !out.is_empty() {
            out.push('\n');
        }
    }
    if !moved_definitions.is_empty() {
        out = format!("{}\n\n{out}", moved_definitions.join("\n"));
    }
    out
}

fn reference_definitions(source: &str, mask: &str) -> (HashMap<String, String>, HashSet<usize>) {
    let mut accepted = HashMap::new();
    super::fold_djot_references_with_decisions(source, Some(&mut accepted));
    let item = cached_regex!(r"(?:[-*+]|[0-9A-Za-z]+[.)]|\([0-9A-Za-z]+\))[ \t]+").unwrap();
    let block =
        cached_regex!(r"^(?:#{1,6}(?: |$)|:{3,}|[`~]{3,}|\{|\[[^\]]+\]:|(?:[*-][ \t]*){3,}$)")
            .unwrap();
    let container = cached_regex!(r"(?:>|[-*+] |[0-9A-Za-z]+[.)] |\([0-9A-Za-z]+\) )").unwrap();
    let heading = cached_regex!(r"^(#{1,6})(?:[ \t]+|$)").unwrap();
    let heading_break = cached_regex!(r"^(?:[#>|{]|[-*+][ \t]|[0-9A-Za-z]+[.)][ \t]|:[ \t]|:{2,}|\([0-9a-zA-Z]+\)[ \t]|[`~]{3,}|\[[^\]\n]*\]:|(?:[*-][ \t]*){3,}$)").unwrap();
    let lines: Vec<_> = source.split('\n').collect();
    let masks: Vec<_> = mask.split('\n').collect();
    let rows = djot_table_rows(source, mask);
    let mut definitions = HashMap::new();
    let mut definition_lines = HashSet::new();
    let mut heading_lines = HashSet::new();
    for (n, line) in lines.iter().enumerate() {
        if definition_lines.contains(&n) {
            continue;
        }
        let at = djot_content_start(line);
        let content = &line[at..];
        let visible = &masks[n][at..];
        let previous = n.checked_sub(1).map_or("", |n| lines[n]);
        let previous_at = djot_content_start(previous);
        let previous_content = previous[previous_at..].trim();
        let boundary = previous_content.is_empty()
            || n > 0 && rows[n - 1]
            || item.is_match(&line[..at])
            || n > 0 && definition_lines.contains(&(n - 1))
            || block.is_match(previous_content)
            || previous_at > at && container.is_match(&previous[..previous_at]);
        if let Some(decision) = accepted.get(&n) {
            for next in n + 1..=decision.end {
                definition_lines.insert(next);
            }
            definitions.insert(definition_label(&decision.label), decision.target.clone());
            definition_lines.insert(n);
        }
        if let Some(heading) = heading
            .captures(content)
            .filter(|_| visible.starts_with('#') && boundary && !heading_lines.contains(&n))
        {
            let mut text = content[heading.get(0).unwrap().end()..].to_owned();
            let marker = regex::Regex::new(&format!(r"^{}[ \t]+", &heading[1])).unwrap();
            for (next, following) in lines.iter().enumerate().skip(n + 1) {
                let start = djot_content_start(following);
                let prefix = &following[..start];
                if prefix.bytes().filter(|b| *b == b'>').count()
                    != line[..at].bytes().filter(|b| *b == b'>').count()
                    || item.is_match(prefix)
                {
                    break;
                }
                let part = &following[start..];
                if part.trim().is_empty()
                    || text.bytes().rev().take_while(|b| *b == b'\\').count() % 2 == 1
                {
                    break;
                }
                if let Some(m) = marker.find(part) {
                    text.push('\n');
                    text.push_str(&part[m.end()..]);
                } else {
                    if heading_break.is_match(part) {
                        break;
                    }
                    text.push('\n');
                    text.push_str(part);
                }
                heading_lines.insert(next);
            }
            definitions
                .entry(reference_label(&text, ""))
                .or_insert_with(|| "#heading".to_owned());
        }
    }
    (definitions, definition_lines)
}

fn empty_description(lines: &[&str], masks: &[&str], n: usize) -> bool {
    let term_pattern = cached_regex!(r":[ \t]+$").unwrap();
    let item_pattern = cached_regex!(r"(?:[-*+]|[0-9A-Za-z]+[.)]|\([0-9A-Za-z]+\))[ \t]+").unwrap();
    let sibling_pattern =
        cached_regex!(r"^[ \t]*(?:[-*+]|[0-9A-Za-z]+[.)]|\([0-9A-Za-z]+\)|:)[ \t]+").unwrap();
    let line = lines[n];
    let at = djot_content_start(line);
    let Some(term) = term_pattern.find(&masks[n][..at]) else {
        return false;
    };
    let previous = n.checked_sub(1).map_or("", |n| lines[n]);
    let previous_content = previous[djot_content_start(previous)..].trim();
    let item = item_pattern.is_match(&line[..at]);
    if term.start() > 0 && !masks[n].as_bytes()[term.start() - 1].is_ascii_whitespace()
        || !masks[n][at..]
            .chars()
            .next()
            .is_some_and(|ch| !ch.is_whitespace())
        || !(previous_content.is_empty() || n == 0 || item)
    {
        return false;
    }
    let depth = line[..at].bytes().filter(|b| *b == b'>').count();
    let without_quotes = |line: &str| -> String {
        let prefix = &line[..djot_content_start(line)];
        prefix
            .rfind('>')
            .map_or(line, |at| {
                line[at + 1..].strip_prefix(' ').unwrap_or(&line[at + 1..])
            })
            .to_owned()
    };
    let minimum = without_quotes(&line[..term.start()]).len() + 2;
    let mut end = n + 1;
    while end < lines.len() {
        let next = lines[end];
        let next_at = djot_content_start(next);
        let content = without_quotes(next);
        if next[..next_at].bytes().filter(|b| *b == b'>').count() != depth
            || next[next_at..].trim().is_empty()
            || sibling_pattern.is_match(&content) && leading_indent(&content).0 <= minimum - 2
        {
            break;
        }
        end += 1;
    }
    while end < lines.len()
        && lines[end][djot_content_start(lines[end])..]
            .trim()
            .is_empty()
    {
        end += 1;
    }
    end == lines.len()
        || lines[end][..djot_content_start(lines[end])]
            .bytes()
            .filter(|b| *b == b'>')
            .count()
            != depth
        || leading_indent(&without_quotes(lines[end])).0 < minimum
}

pub(super) fn losses(source: &str) -> Vec<crate::MigrationDiagnostic> {
    let empty_heading = cached_regex!(r"^#{1,6}$").unwrap();
    let heading_boundary = cached_regex!(r"^#{1,6}(?:[ \t]|$)").unwrap();

    use crate::{HtmlImportSeverity, MigrationConfidence, MigrationDiagnostic, MigrationFidelity};
    let normalized = source.replace("\r\n", "\n").replace('\r', "\n");
    let (frontmatter, front_separator, source) = split_frontmatter(&normalized);
    let line_offset = frontmatter
        .bytes()
        .chain(front_separator.bytes())
        .filter(|b| *b == b'\n')
        .count();
    const RAW_FORMAT_MESSAGE: &str = "A Djot raw-format identifier containing U+0085 cannot be spelled in Carve; its raw-output behavior is lost.";
    let mut raw_fence_lines = HashMap::new();
    let mask_source = if source.contains('\u{0085}') {
        let header = cached_regex!(r#"(`{3,}|~{3,})([ \t]*=)([^\x09-\x0d\x20\x{00a0}\x{1680}\x{2000}-\x{200a}\x{2028}\x{2029}\x{202f}\x{205f}\x{3000}\x{feff}{}%"'`~\\]+)([ \t]*)$"#).unwrap();
        let mut offset = 0;
        let lines: Vec<_> = source
            .split('\n')
            .enumerate()
            .map(|(n, line)| {
                let replaced = header
                    .replace(line, |caps: &regex::Captures<'_>| {
                        if !caps[3].contains('\u{0085}') {
                            return caps[0].to_owned();
                        }
                        raw_fence_lines.insert(n, offset);
                        let format: String = caps[3]
                            .bytes()
                            .map(|byte| {
                                if byte.is_ascii_alphanumeric() || b"_+#.-".contains(&byte) {
                                    char::from(byte)
                                } else {
                                    'x'
                                }
                            })
                            .collect();
                        format!("{}{}{}{}", &caps[1], &caps[2], format, &caps[4])
                    })
                    .into_owned();
                offset += line.len() + 1;
                replaced
            })
            .collect();
        std::borrow::Cow::Owned(lines.join("\n"))
    } else {
        std::borrow::Cow::Borrowed(source)
    };
    let raw_formats = std::cell::RefCell::new(Vec::new());
    let raw_blocks = std::cell::RefCell::new(Vec::new());
    let on_raw_format = |start: usize, end: usize, format: &str| {
        if format.contains('\u{0085}')
            && !(start > 0
                && source.as_bytes()[start - 1] == b'$'
                && !is_escaped(source.as_bytes(), start - 1))
        {
            raw_formats.borrow_mut().push((start, end));
        }
    };
    let on_fence_open = |line: usize| {
        if let Some(&offset) = raw_fence_lines.get(&line) {
            raw_blocks.borrow_mut().push(offset);
        }
    };
    let mask = reference_mask_with_options(
        &mask_source,
        OpaqueOptions {
            on_raw_format: Some(&on_raw_format),
            on_fence_open: Some(&on_fence_open),
            ..OpaqueOptions::default()
        },
    );
    let mut findings: Vec<(usize, &'static str)> = raw_blocks
        .borrow()
        .iter()
        .map(|&at| (at, RAW_FORMAT_MESSAGE))
        .collect();
    let mut raw_ignored = Vec::new();
    if has_djot_code_layout_candidates(source) {
        let mut raw_code = Vec::new();
        let _ = close_djot_table_code(
            source,
            !source.contains("\\|")
                && !source.lines().any(|line| {
                    cached_regex!(r"^[ \t]+`{3,}")
                        .unwrap()
                        .is_match(quoted(line).1)
                }),
            Some(&mut |at| raw_code.push(at)),
        );
        findings.extend(raw_code.into_iter().map(|at| {
            (
                at,
                "Multiline inline code uses raw HTML; plain-text export omits it.",
            )
        }));
    }
    let mut nested = Vec::new();
    emphasis::convert_with_losses(source, &emphasis_mask(source), str::to_owned, &mut nested);
    findings.extend(nested.into_iter().map(|at| {
        (
            at,
            "Emphasis exceeding the native nesting budget is flattened; its text is preserved.",
        )
    }));
    let (definitions, definition_lines) = reference_definitions(source, &mask);
    let mut offset = 0;
    for (n, line) in source.split('\n').enumerate() {
        if definition_lines.contains(&n) {
            raw_ignored.push((offset, offset + line.len()));
        }
        offset += line.len() + 1;
    }
    let references = reference_uses(source, &mask);
    raw_ignored.extend(
        references
            .iter()
            .map(|reference| (reference.end - reference.label.len() - 1, reference.end - 1)),
    );
    let mut label_cache = HashMap::new();
    for reference in &references {
        let at = reference.start;
        if reference.text.starts_with('^') && reference.text.len() > 1 {
            continue;
        }
        let image =
            at > 0 && source.as_bytes()[at - 1] == b'!' && !is_escaped(source.as_bytes(), at - 1);
        let destination = if definitions.is_empty() {
            None
        } else {
            definitions.get(&cached_reference_label(
                reference.text,
                reference.label,
                reference.quote_depth,
                &mut label_cache,
            ))
        };
        match destination {
            Some(destination) if destination.is_empty() => findings.push((
                at,
                if image {
                    "An image with an empty destination cannot be written in Carve."
                } else {
                    "A link with an empty destination cannot be written in Carve."
                },
            )),
            None => findings.push((
                at,
                if image {
                    "An unresolved Djot image reference loses its image element."
                } else {
                    "An unresolved Djot reference loses its link element."
                },
            )),
            Some(_) if image && reference.text.contains('\n') => {
                let projected = projected_reference_text(reference.text, reference.quote_depth);
                let child =
                    djot_image_label_child_text(&projected, &HashSet::new(), true, false, true);
                if cached_regex!(r"\n[ \t]*\n").unwrap().is_match(&child) {
                    findings.push((
                        at,
                        "An image alternative text with a blank line cannot be spelled in Carve.",
                    ));
                }
            }
            _ => (),
        }
    }
    let destinations = djot_simple_destination_ranges(source).unwrap_or_else(|| {
        let destination_mask = mask_djot_forms_with_options(
            source,
            false,
            true,
            None,
            &[],
            OpaqueOptions {
                destinations: false,
                attribute_values: false,
                autolinks: false,
                ..OpaqueOptions::default()
            },
        );
        djot_destination_ranges(source, &destination_mask)
    });
    let reference_starts: HashSet<_> = references.iter().map(|reference| reference.start).collect();
    let mut line_offsets = Vec::new();
    let mut quote_depths = Vec::new();
    let mut layout_offset = 0;
    for line in source.split('\n') {
        line_offsets.push(layout_offset);
        layout_offset += line.len() + 1;
        quote_depths.push(
            line[..djot_content_start(line)]
                .bytes()
                .filter(|&byte| byte == b'>')
                .count(),
        );
    }
    if quote_depths.iter().any(|&depth| depth > 0) {
        let rows = djot_table_rows(source, &mask);
        let literal = literal_quote_depths(source, &mask, &rows);
        for (line, at) in line_offsets.iter().enumerate() {
            if let Some(&owner) = literal.get(at) {
                quote_depths[line] = owner;
            }
        }
    }
    if source.contains('`') && source.contains(":::") {
        let code_source = replace_image_labels(
            source,
            &mask_code_and_destinations(source),
            |image, _, _| {
                String::from_utf8(
                    image
                        .bytes()
                        .map(|byte| if byte == b'\n' { byte } else { b' ' })
                        .collect(),
                )
                .unwrap()
            },
        );
        let mut code_source = code_source.into_bytes();
        for (line, value) in source.split('\n').enumerate() {
            let mut depth = 0;
            for (i, byte) in value.as_bytes()[..djot_content_start(value)]
                .iter()
                .enumerate()
            {
                if *byte == b'>' {
                    depth += 1;
                    if depth > quote_depths[line] {
                        code_source[line_offsets[line] + i] = b' ';
                    }
                }
            }
        }
        let code_source = String::from_utf8(code_source).unwrap();
        let fenced = mask_djot_fences(&code_source, None, &[], true);
        let mut spans = Vec::new();
        let _ = mask_djot_opaque_with_comments_and_code(
            &fenced,
            OpaqueOptions::default(),
            Some(&mut spans),
        );
        let container = cached_regex!(r"\n[ \t]*:{3,}(?:[ \t]+[^\n]*)?(?:\n|$)").unwrap();
        let mut line_end = source.find('\n');
        let mut code_line = 0;
        let mut span_end = 0;
        for span in spans {
            if span.start < span_end {
                continue;
            }
            while let Some(end) = line_end.filter(|end| *end < span.start) {
                let line_start = end + 1;
                line_end = source[line_start..].find('\n').map(|end| line_start + end);
                code_line += 1;
            }
            if quote_depths.get(code_line).copied().unwrap_or(0) > 0 {
                continue;
            }
            let width = source.as_bytes()[span.start..]
                .iter()
                .take_while(|byte| **byte == b'`')
                .count();
            span_end = span.end;
            let content =
                &source[span.start + width..span.end - if span.closed { width } else { 0 }];
            if container.is_match(content) {
                findings.push((span.start, "A multiline code span contains a Carve container opener on a continuation line."));
            }
        }
    }
    let mut source_line = 0;
    let boundaries = djot_inline_boundaries(source, &mask);
    let mut boundary = 0;
    // Track label brackets separately from destination parentheses.
    let mut brackets: Vec<(usize, bool, usize)> = Vec::new();
    let bytes = source.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        while line_offsets.get(source_line + 1).is_some_and(|at| *at <= i) {
            source_line += 1;
        }
        while boundaries.get(boundary).is_some_and(|at| *at <= i) {
            brackets.clear();
            boundary += 1;
        }
        if mask.as_bytes()[i] != bytes[i] {
            i += 1;
            continue;
        }
        if bytes[i] == b'\\' {
            i += 2;
            continue;
        }
        if bytes[i] == b'[' {
            brackets.push((i, false, quote_depths[source_line]));
        }
        if bytes[i] == b']' {
            if let Some((start, nested, depth)) = brackets.pop() {
                let image = start > 0 && bytes[start - 1] == b'!' && !is_escaped(bytes, start - 1);
                let valid_form =
                    destinations.contains_key(&(i + 1)) || reference_starts.contains(&start);
                if image && valid_form || bytes.get(start + 1) == Some(&b'^') && i > start + 2 {
                    raw_ignored.push((start, i + 1));
                }
                let is_link = !image && bytes.get(start + 1) != Some(&b'^') && valid_form;
                if bytes.get(start + 1) != Some(&b'^')
                    && destinations.get(&(i + 1)).is_some_and(|&end| {
                        djot_destination_lines(&source[i + 2..end - 1], depth).is_empty()
                    })
                {
                    findings.push((
                        start,
                        if image {
                            "An image with an empty destination cannot be written in Carve."
                        } else {
                            "A link with an empty destination cannot be written in Carve."
                        },
                    ));
                }
                if image
                    && source[start + 1..i].contains('\n')
                    && destinations.get(&(i + 1)).is_some_and(|&end| {
                        !djot_destination_lines(&source[i + 2..end - 1], depth).is_empty()
                    })
                {
                    let projected = projected_reference_text(&source[start + 1..i], depth);
                    let child =
                        djot_image_label_child_text(&projected, &HashSet::new(), true, false, true);
                    if cached_regex!(r"\n[ \t]*\n").unwrap().is_match(&child) {
                        findings.push((start, "An image alternative text with a blank line cannot be spelled in Carve."));
                    }
                }
                if is_link && nested {
                    findings.push((start, "A link nested inside another link is flattened."));
                }
                if is_link || nested && !(image && valid_form) {
                    if let Some(parent) = brackets.last_mut() {
                        parent.1 = true;
                    }
                }
            }
        }
        i += 1;
    }
    raw_ignored.sort_unstable_by_key(|range| range.0);
    let mut ignored_index = 0;
    let mut ignored_end = 0;
    for &(start, end) in raw_formats.borrow().iter() {
        while ignored_index < raw_ignored.len() && raw_ignored[ignored_index].0 <= start {
            ignored_end = ignored_end.max(raw_ignored[ignored_index].1);
            ignored_index += 1;
        }
        if ignored_end < end {
            findings.push((start, RAW_FORMAT_MESSAGE));
        }
    }
    let lines: Vec<_> = source.split('\n').collect();
    let masks: Vec<_> = mask.split('\n').collect();
    let rows = djot_table_rows(source, &mask);
    let mut offset = 0;
    let mut table_start = 0;
    for (n, line) in lines.iter().enumerate() {
        if !rows[n] {
            table_start = n + 1;
        }
        let text = line.trim();
        let visible = masks[n].trim();
        let block_allowed = n == 0
            || lines[n - 1].trim().is_empty()
            || heading_boundary.is_match(masks[n - 1].trim_start())
            || definition_term(masks[n - 1]).is_some();
        if block_allowed
            && empty_heading.is_match(visible)
            && reference_label(text.trim_start_matches('#').trim_start(), "").is_empty()
            && lines
                .get(n + 1)
                .map_or(true, |line| line.trim_matches([' ', '\t']).is_empty())
        {
            findings.push((offset, "An empty Djot heading cannot be written in Carve."));
        }
        if empty_description(&lines, &masks, n) {
            findings.push((
                offset,
                "An empty definition description cannot be written in Carve.",
            ));
        }
        if rows[n] && text.len() >= 2 && separator(text) {
            if n > table_start + 1 {
                findings.push((offset, "A table separator inside the table loses the preceding header row and alignment change."));
            }
            if (n == 0 || !rows[n - 1])
                && lines
                    .iter()
                    .enumerate()
                    .skip(n)
                    .take_while(|(k, _)| rows[*k])
                    .all(|(_, row)| separator(row.trim()))
            {
                findings.push((
                    offset,
                    "A table containing only separator rows cannot be written in Carve.",
                ));
            }
        }
        offset += line.len() + 1;
    }
    findings.sort_unstable();
    findings.dedup();
    let mut reported = HashSet::new();
    let mut diagnostics = Vec::new();
    let mut line = line_offset + 1;
    let mut cursor = 0;
    for (at, message) in findings {
        line += source.as_bytes()[cursor..at]
            .iter()
            .filter(|&&b| b == b'\n')
            .count();
        cursor = at;
        if reported.insert((line, message)) {
            diagnostics.push(MigrationDiagnostic {
                code: "structure-unspellable".to_owned(),
                message: message.to_owned(),
                severity: HtmlImportSeverity::Warning,
                fidelity: MigrationFidelity::Dropped,
                confidence: MigrationConfidence::Exact,
                path: Some(format!("line:{line}")),
            });
        }
    }
    diagnostics
}

#[cfg(test)]
mod tests {
    use crate::{migrate_djot, HtmlImportSeverity, MigrationConfidence, MigrationFidelity};

    #[test]
    fn djot_empty_attributes_keep_bracketed_spans() {
        for tail in [
            "",
            "tail",
            "[x]",
            "[x](u)",
            "![x](u)",
            "*s*",
            "_e_",
            "`c`",
            "{=m=}",
            "{-d-}",
            "^u^",
            "~d~",
            "<http://a.b>",
            "word",
            "'q'",
            "\\*",
            "\\{",
        ] {
            let source = format!("[x]{{}}{tail}\n");
            let converted = migrate_djot(&source).value;
            assert!(converted.contains("[x]{}"), "{source:?}: {converted:?}");
            let tail_html = crate::to_html(&migrate_djot(&format!("{tail}\n")).value);
            let content = tail_html
                .strip_prefix("<p>")
                .and_then(|s| s.strip_suffix("</p>"))
                .unwrap_or(&tail_html);
            let content = if tail == "'q'" { "’q’" } else { content };
            assert_eq!(
                crate::to_html(&converted),
                format!("<p><span>x</span>{content}</p>"),
                "{source:?}"
            );
        }
        for (source, expected_html) in [
            ("pre [x]{} post\n", "<p>pre <span>x</span> post</p>"),
            ("*[x]{}*\n", "<p><strong><span>x</span></strong></p>"),
            ("_[x]{}_\n", "<p><em><span>x</span></em></p>"),
            ("[x]{}{}\n", "<p><span>x</span></p>"),
            ("![x]{}\n", "<p>!<span>x</span></p>"),
        ] {
            assert_eq!(
                crate::to_html(&migrate_djot(source).value),
                expected_html,
                "{source:?}"
            );
        }
    }

    #[test]
    fn djot_empty_attributes_without_spans_remain_inert() {
        for (source, expected) in [
            ("\\[x]{}\n", "\\[x]{%%}\n"),
            ("[x\\]{}\n", "[x\\]{%%}\n"),
            ("]{}\n", "]{%%}\n"),
            ("word{}\n", "word{%%}\n"),
            ("[x](u){}\n", "[x](u){%%}\n"),
            ("*a [b* c]{}\n", "*a \\[b* c\\]{%%}\n"),
            ("_a [b_ c]{}\n", "/a \\[b/ c\\]{%%}\n"),
        ] {
            assert_eq!(migrate_djot(source).value, expected, "{source:?}");
        }
    }

    #[test]
    fn djot_hard_breaks_preserve_escaped_spaces() {
        for prefix in ["a", "*a*"] {
            for spaces in ["\\ ", " \\ ", "\\ \\ "] {
                let source = format!("{prefix}{spaces}\\\nb\n");
                assert_eq!(super::inline(&source), source);
                assert_eq!(migrate_djot(&source).value, source);
                let content = if prefix == "a" {
                    "a"
                } else {
                    "<strong>a</strong>"
                };
                let spaces = spaces.replace("\\ ", "&nbsp;");
                assert_eq!(
                    crate::to_html(&source),
                    format!("<p>{content}{spaces}<br>\nb</p>")
                );
            }
        }
        for (source, expected) in [
            ("a\\ \\ \n", "a\\ \\\n"),
            ("a\\  \t\\\nb\n", "a\\ \\\nb\n"),
            ("a\\\\ \t\\\nb\n", "a\\\\\\\nb\n"),
            ("a  \t\\\nb\n", "a\\\nb\n"),
        ] {
            assert_eq!(super::inline(source), expected, "{source:?}");
            assert_eq!(
                crate::to_html(&migrate_djot(source).value),
                crate::to_html(expected),
                "{source:?}"
            );
        }
    }

    #[test]
    fn djot_quote_attribute_lines_do_not_start_a_paragraph() {
        for (source, expected) in [
            (
                "> {.foo}\n> > {.bar}\n> > nested\n",
                "<blockquote><blockquote class=\"foo\"><p class=\"bar\">nested</p></blockquote></blockquote>",
            ),
            (
                "> {.foo}\n> > nested\n",
                "<blockquote><blockquote class=\"foo\"><p>nested</p></blockquote></blockquote>",
            ),
            (
                "> {.foo}\n> {.bar}\n> > nested\n",
                "<blockquote><blockquote class=\"foo bar\"><p>nested</p></blockquote></blockquote>",
            ),
            (
                "> a\n>\n> {.foo}\n> > b\n",
                "<blockquote><p>a</p><blockquote class=\"foo\"><p>b</p></blockquote></blockquote>",
            ),
        ] {
            let value = migrate_djot(source).value;
            let html = crate::to_html(&value);
            let joined = html.lines().map(str::trim).collect::<String>();
            assert_eq!(joined, expected, "{source:?}: {value:?}");
        }
    }

    #[test]
    fn djot_shallower_attributes_close_the_previous_quote_paragraph() {
        let controls: serde_json::Value = serde_json::from_str(include_str!(
            "../tests/fixtures/djot-quote-attribute-boundaries.json"
        ))
        .unwrap();
        for control in controls.as_array().unwrap() {
            let source = control["source"].as_str().unwrap();
            let expected = control["html"].as_str().unwrap();
            let value = migrate_djot(source).value;
            let html = crate::to_html(&value);
            assert_eq!(
                html.lines().map(str::trim).collect::<String>(),
                expected.lines().map(str::trim).collect::<String>(),
                "{source:?}: {value:?}"
            );
        }
    }

    #[test]
    fn djot_block_after_quote_ends_the_container_paragraph() {
        let source = "> a\n# a\n";
        let value = migrate_djot(source).value;
        assert_eq!(value, source);
        assert_eq!(
            crate::to_html(&value),
            "<blockquote><p>a</p></blockquote>\n<section id=\"a\">\n  <h1>a</h1>\n</section>"
        );
    }

    #[test]
    fn djot_list_after_quote_ends_the_container_paragraph() {
        let value = migrate_djot("> a\n- *x*").value;
        assert_eq!(
            crate::to_html(&value),
            "<blockquote><p>a</p></blockquote>\n<ul>\n  <li><strong>x</strong></li>\n</ul>"
        );
        let value = migrate_djot("> a\n> > b").value;
        assert_eq!(
            crate::to_html(&value),
            "<blockquote><p>a\n&gt; b</p></blockquote>"
        );
        let value = migrate_djot("> > a\n> - *x*").value;
        assert_eq!(crate::to_html(&value), "<blockquote>\n  <blockquote><p>a</p></blockquote>\n  <ul>\n    <li><strong>x</strong></li>\n  </ul>\n</blockquote>");
        let value = migrate_djot("> > a\n>  - *x*").value;
        assert_eq!(crate::to_html(&value), "<blockquote>\n  <blockquote><p>a</p></blockquote>\n  <ul>\n    <li><strong>x</strong></li>\n  </ul>\n</blockquote>");
        let value = migrate_djot("> > a\n> b\n> > c").value;
        assert_eq!(
            crate::to_html(&value),
            "<blockquote>\n  <blockquote><p>a\nb\nc</p></blockquote>\n</blockquote>"
        );
    }

    #[test]
    fn djot_div_after_heading_continuation_starts_a_block() {
        let value = migrate_djot("# a\n  body\n::: x\n").value;
        assert_eq!(value, "# a body\n::: x\n:::\n");
        assert_eq!(
            crate::to_html(&value),
            "<section id=\"a-body\">\n  <h1>a body</h1>\n  <div class=\"x\">\n\n  </div>\n</section>"
        );
    }

    #[test]
    fn djot_empty_attributes_preserve_separate_emphasis_spans() {
        for (source, expected) in [
            ("*a*{}*b*\n", "*a*{%%}*b*\n"),
            ("_a_{}{}_b_\n", "/a/{%%}/b/\n"),
            ("*a*{ }*b*\n", "*a*{%%}*b*\n"),
        ] {
            let value = migrate_djot(source).value;
            assert_eq!(value, expected);
            let html = crate::to_html(&value);
            let tag = if source.starts_with('_') {
                "em"
            } else {
                "strong"
            };
            assert_eq!(
                html.matches(&format!("<{tag}>")).count(),
                2,
                "{source:?}: {html}"
            );
        }
        for (source, expected) in [
            ("~a~{}~b~\n", "{,a,}{%%}{,b,}\n"),
            ("^a^{}{}^b^\n", "{^a^}{%%}{^b^}\n"),
            ("{}{}\n*p*\n", "{%%}\n*p*\n"),
        ] {
            assert_eq!(migrate_djot(source).value, expected, "{source:?}");
        }
    }

    #[test]
    fn djot_list_blocks_preserve_looseness_and_parent_boundaries() {
        for source in [
            "- a\n\n  # h\n\n  - c\n\n- d\n",
            "- a\n\n  ```\n  x\n  ```\n\n  - c\n\n- d\n",
        ] {
            assert_eq!(migrate_djot(source).value, source, "{source:?}");
        }
        assert_eq!(
            migrate_djot("- a\n  para text\n\n  - c\n\n- d\n").value,
            "- a\n  para text\n  - c\n- d\n"
        );
        assert_eq!(
            migrate_djot("- a\n\n  - c\n  ```\n  x\n  ```\n\n- d\n").value,
            "- a\n\n  - c\n  ```\n  x\n  ```\n\n- d\n"
        );
    }

    #[test]
    fn djot_unspellable_structures_name_losses_on_source_lines() {
        for (source, line, message) in [
            ("##\n", 1, "empty Djot heading"),
            ("[link][]\n\n[link]:\n[link2]: url\n", 1, "empty destination"),
            ("[link][a and\nb]\n", 1, "unresolved Djot reference"),
            ("[link][a and\nb]\n\n[a and\nb]: url\n", 1, "unresolved Djot reference"),
            ("[Link][]\n\n[link]: /url\n", 1, "unresolved Djot reference"),
            ("[[foo](bar)](baz)\n", 1, "nested inside another link"),
            (": apple\n fruit\n\n  Paragraph one\n\n  Paragraph two\n\n  - sub\n  - list\n\n: orange\n", 11, "empty definition description"),
            ("|a|b|\n|:-|---:|\n|c|d|\n|cc|dd|\n|-:|:-:|\n|e|f|\n|g|h|\n", 5, "separator inside the table"),
            ("|--|--|\n", 1, "only separator rows"),
        ] {
            // Heading folding removes a source line before the loss location.
            for (prefix, shift) in [("", 0), ("# Heading\nlazy continuation\n\n", 3),
                ("# Heading\r\nlazy continuation\r\n\r\n", 3),
                ("---\ntext: *****a*****\n---\n\n", 4)] {
                let result = migrate_djot(&format!("{prefix}{source}"));
                assert!(result.report.diagnostics.iter().any(|d| d.code == "fidelity-unverified"));
                let loss = result.report.diagnostics.iter().find(|d| d.code == "structure-unspellable" && d.message.contains(message)).unwrap_or_else(|| panic!("{source:?}: {:?}", result.report.diagnostics));
                assert_eq!(loss.path.as_deref(), Some(format!("line:{}", line + shift).as_str()), "{source:?}");
                assert_eq!(loss.severity, HtmlImportSeverity::Warning);
                assert_eq!(loss.fidelity, MigrationFidelity::Dropped);
                assert_eq!(loss.confidence, MigrationConfidence::Exact);
            }
        }
    }

    #[test]
    fn djot_loss_detection_ignores_literal_and_supported_forms() {
        for source in [
            "`_({_foo_})_`\n",
            "|\n",
            "para\n#\n",
            "para\n: x\n",
            "[a][]\n\n[a]:\n  /url\n",
            "- [a][]\n\n  [a]: /url\n",
            "> [a][]\n>\n> [a]: /url\n",
            "See [Introduction][].\n\n# Introduction\n",
            "[![badge](b.svg)](https://x)\n",
            "---\ntext: *****a*****\n---\nplain\n",
            "[link _and_ link][]\n\n[link and link]: url\n",
            "```\n##\n[missing][]\n|--|--|\n```\n",
            "\\[missing][]\n",
            "[link][ref]\n\n[ref]: url\n",
            "# Heading\n",
            "*strong _emphasis_*\n",
            "|a|b|\n|--|--|\n",
        ] {
            assert!(
                migrate_djot(source)
                    .report
                    .diagnostics
                    .iter()
                    .all(|d| d.code != "structure-unspellable"),
                "{source:?}"
            );
        }
    }

    #[test]
    fn djot_normalization_preserves_code_urls_and_container_ownership() {
        for source in [
            "~~~\nlet s = ` a `;\n~~~\n",
            "<https://example.com/a{}b>\n",
            "[r]: /a{}b\n",
        ] {
            assert_eq!(super::inline(source), source);
        }
        assert!(super::inline("a `b\n\n{'q'} c\n\nd `e\n").contains("‘q’ c"));
        for (source, expected) in [
            ("- item\n  Note. more\n", "- item\n  Note. more\n"),
            ("- a\n  1. b\n", "- a\n  1\\. b\n"),
            ("a `b\nc`\n# f\n", "a `b\nc`\n\\# f\n"),
            ("para\n[a]: b\n# x\n", "para\n[a]: b\n\\# x\n"),
            (
                "1. one\n\n 1. two\n\n    cont\n",
                "1. one\n\n   1. two\n\n      cont\n",
            ),
            (
                ":::: outer\n::::: inner\nx\n::::\nafter\n",
                ":::: outer\n::::: inner\nx\n:::::\n::::\nafter\n",
            ),
            (
                "::: widget\n```\ncode\n",
                "::: widget\n```\ncode\n```\n:::\n",
            ),
        ] {
            assert_eq!(super::blocks(source), expected, "{source:?}");
        }
        let classes = migrate_djot("{.a .b}\n[ref]: /url\n\n[ref][]\n");
        assert_eq!(
            crate::to_html(&classes.value),
            "<p><a href=\"/url\" class=\"a b\">ref</a></p>"
        );
        assert_eq!(classes.value.matches("/url").count(), 1);
        let image = migrate_djot("{title=t}\n[ref]: /url\n\n[![alt](i.png)][ref]\n");
        assert_eq!(
            crate::to_html(&image.value),
            "<p><a href=\"/url\" title=\"t\"><img src=\"i.png\" alt=\"alt\"></a></p>"
        );
        assert_eq!(image.value.matches("/url").count(), 1);
        assert!(image.value.contains("[![alt](i.png)][ref]"));
        assert!(image.value.contains("[ref]: /url {title=t}"));
        assert_eq!(
            crate::to_html(&migrate_djot("<https://example.com/a{}b>\n").value),
            "<p><a href=\"https://example.com/a{}b\">https://example.com/a{}b</a></p>"
        );
    }

    #[test]
    fn djot_definition_term_lazy_continuation_is_dedented() {
        let result = migrate_djot(": apple\n fruit\n\n  Body\n\n: orange\n");
        assert!(result.value.contains(":: apple\nfruit\n"));
        assert_eq!(
            migrate_djot(": apple\n fruit\n\n  Body\n\n  - sub\n  - list\n\n: orange\n").value,
            "{loose}\n:: apple\nfruit\n\n:  Body\n\n   - sub\n   - list\n\n:: orange\n"
        );
    }

    #[test]
    fn djot_opaque_payloads_do_not_report_reference_losses() {
        for source in [
            "<https://example.com/[x][missing]>\n",
            "[x](a(b[x][missing]c))\n",
            "![x](a(b[x][missing]c))\n",
            "`[x][missing]`\n",
            "`[x][missing]`{=html}\n",
            "$`[x][missing]`\n",
            "[t]{k=\"[x][missing]\"}\n",
            "{% [x][missing] %}\n",
        ] {
            assert!(super::losses(source).is_empty(), "{source:?}");
        }
        for label in ["[x]", "![x]"] {
            let paragraph = if label.starts_with('!') { "{%%}" } else { "" };
            assert_eq!(
                migrate_djot(&format!("{label}(a(b{{}}c))\n")).value,
                format!("{paragraph}{label}(a%28b{{}}c%29)\n")
            );
            assert_eq!(
                migrate_djot(&format!("{label}(a` b `c)\n")).value,
                format!("{paragraph}{label}(a%60%20b%20%60c)\n")
            );
            assert_eq!(
                migrate_djot(&format!("{label}(a` b `c) and ` code `\n")).value,
                format!("{paragraph}{label}(a%60%20b%20%60c) and `  code  `\n")
            );
        }
        for source in [
            "[x](a(` b `)c)\n",
            "[x](a` b\nc `d)\n",
            "`` x ``{=html}\n",
            "$`` x ``\n",
            "{% ` x ` %}\n",
        ] {
            assert_eq!(super::inline(source), source);
        }
        assert_eq!(super::inline("text](a` b `c)\n"), "text](a`  b  `c)\n");
        assert_eq!(super::inline("[` a](b `](url)\n"), "[`  a](b  `](url)\n");
        for destination in ["http://a.b/x{}y", "http://a.b/{\"q\"}"] {
            let source = format!("<{destination}>\n");
            let escaped = destination.replace('"', "&quot;");
            assert_eq!(
                crate::to_html(&migrate_djot(&source).value),
                format!("<p><a href=\"{escaped}\">{destination}</a></p>")
            );
        }
        let source = "{title=foo}\n[x]: /u\n\n[hi]{title=\"[x][]\"}\n";
        assert_eq!(
            crate::to_html(&migrate_djot(source).value),
            "<p><span title=\"[x][]\">hi</span></p>"
        );
    }

    #[test]
    fn djot_reference_attribute_pipes_remain_cell_boundaries() {
        let source = "{.c}\n[x]: /u\n\n| h |\n|---|\n| [a{k=\"a|b\"}][x] |\n";
        let normalized = super::references(source);
        assert!(normalized.contains("[a{k=\"a|b\"}][x]"));
        assert!(!normalized.contains("[x]{.c}"));
    }

    #[test]
    fn djot_reference_attributes_use_parsed_tokens_and_own_keys() {
        for (base, own, expected) in [
            ("{.a % .bogus #bogus title=bogus %}", "", "<p><a href=\"/u\" class=\"a\">x</a></p>"),
            ("{.a title=\"a .bogus #bogus % comment %\" key=\"a b\"}", "", "<p><a href=\"/u\" class=\"a\" title=\"a .bogus #bogus % comment %\" key=\"a b\">x</a></p>"),
            ("{.a title=base key=\"a b\"}", "{.b % .bogus key=bogus % title=\"own .c #d % value %\"}", "<p><a href=\"/u\" key=\"a b\" class=\"b\" title=\"own .c #d % value %\">x</a></p>"),
            ("{.a title=\"a \\\"quoted\\\" .b #c %\"}", "", "<p><a href=\"/u\" class=\"a\" title=\"a &quot;quoted&quot; .b #c %\">x</a></p>"),
            ("{.a class=b .c}", "", "<p><a href=\"/u\" class=\"b c\">x</a></p>"),
            ("{% note %}", "", "<p><a href=\"/u\">x</a></p>"),
            ("{}", "", "<p><a href=\"/u\">x</a></p>"),
            ("{.a}\n{.b}", "", "<p><a href=\"/u\" class=\"a b\">x</a></p>"),
            ("{key=a}\n{key=b}", "", "<p><a href=\"/u\" key=\"b\">x</a></p>"),
            ("{.a title=base}\n{.b key=value}", "{.c}{.d title=own}", "<p><a href=\"/u\" key=\"value\" class=\"c d\" title=\"own\">x</a></p>"),
        ] {
            let source = format!("[x][]{own}\n\n{base}\n[x]: /u\n");
            assert_eq!(crate::to_html(&migrate_djot(&source).value), expected, "{source:?}");
        }
        let source = "{.a #a title=a data-extra=x}\n[x]: /u\n\n[x][]{.b #b title=b}\n";
        let html = crate::to_html(&migrate_djot(source).value);
        for attribute in ["class=\"b\"", "id=\"b\"", "title=\"b\"", "data-extra=\"x\""] {
            assert!(html.contains(attribute), "{html}");
        }
    }

    #[test]
    fn djot_reference_attributes_remain_pending_across_blank_lines() {
        for (source, expected) in [
            (
                "{.a}\n\n[r]: u\n\n[x][r]\n",
                "<p><a href=\"u\" class=\"a\">x</a></p>",
            ),
            (
                "{.a}\n{.b}\n\n[r]: u\n\n[x][r]\n",
                "<p><a href=\"u\" class=\"a b\">x</a></p>",
            ),
            (
                "{.a}\n\n{.b}\n[r]: u\n\n[x][r]\n",
                "<p><a href=\"u\" class=\"a b\">x</a></p>",
            ),
        ] {
            let converted = migrate_djot(source).value;
            assert_eq!(crate::to_html(&converted), expected, "{converted:?}");
        }
    }

    #[test]
    fn djot_formatted_reference_uses_share_definition_removal() {
        assert_eq!(
            crate::to_html(&migrate_djot("[_x_][] [_x_][]\n\n[x]: u\n").value),
            "<p><a href=\"u\"><em>x</em></a> <a href=\"u\"><em>x</em></a></p>"
        );
    }

    #[test]
    fn djot_div_closes_inside_its_owning_item() {
        let sibling_pattern = cached_regex!(r"</div>\s*</li>\s*<li>(?:<p>)?y").unwrap();
        for marker in ["-", "*", "1."] {
            let indent = if marker == "1." { "   " } else { "  " };
            let next = if marker == "1." { "2." } else { marker };
            for gap in ["", "\n", "\n\n"] {
                let source = format!("{marker} ::: foo\n{indent}x\n{gap}{next} y\n");
                let value = migrate_djot(&source).value;
                assert!(
                    value.contains(&format!("{indent}x\n{indent}:::\n{gap}{next} y")),
                    "{source:?}: {value:?}"
                );
                let html = crate::to_html(&value);
                assert_eq!(html.matches("<div").count(), 1);
                assert!(sibling_pattern.is_match(&html), "{html}");
            }
        }
        assert!(migrate_djot("- a\n\n  ::: foo\n  x\n- y\n")
            .value
            .contains("  x\n  :::\n- y\n"));
        let value = migrate_djot("- ::: foo\n  Hi\n  ::::").value;
        assert!(value.contains("\n  :::"));
        assert!(!crate::to_html(&value).contains("::::"));
    }

    #[test]
    fn djot_loss_references_follow_blocks_and_complete_heading_labels() {
        for block in [
            "***",
            "*-*-*",
            "# h",
            "> q",
            "- li",
            "1. li",
            "```\nc\n```",
            "| t |",
            "::: d\nin\n:::",
        ] {
            for source in [
                format!("[r][]\n\n{block}\n[r]: /u\n"),
                format!("{block}\n[r]: /u\n\n[r][]\n"),
            ] {
                assert!(super::losses(&source).is_empty(), "{source:?}");
            }
        }
        for heading in [
            "# a\ncontinued",
            "## a\n## continued",
            "#\na\ncontinued",
            "# a\n  continued",
        ] {
            assert!(
                super::losses(&format!("[a continued][]\n\n{heading}\n")).is_empty(),
                "{heading:?}"
            );
            assert!(
                !super::losses(&format!("[a][]\n\n{heading}\n")).is_empty(),
                "{heading:?}"
            );
        }
        for source in [
            "[r][]\n\npara\n[r]: /u\n",
            "[continued][]\n\n# a\n# continued\n",
            "[a continued][]\n\n# a\n- continued\n",
        ] {
            assert!(
                super::losses(source).iter().any(
                    |d| d.path.as_deref() == Some("line:1") && d.message.contains("unresolved")
                ),
                "{source:?}"
            );
        }
        assert!(super::losses("[a][]\n\n# a\n- continued\n").is_empty());
    }

    #[test]
    fn djot_empty_definition_losses_follow_container_content_and_source_lines() {
        for (source, line) in [
            (": term\n", 1),
            ("> : term\n", 1),
            ("- : term\n", 1),
            ("intro\n\n> : term\n", 3),
            ("- first\n- : term\n", 2),
            ("> > : term\n", 1),
            ("> - : term\n", 1),
            ("- > : term\n", 1),
            ("1. : term\n", 1),
            ("> : term\n>\n> outside\n", 1),
            ("- : term\n\n- outside\n", 1),
            ("> : term\n\n  outside\n", 1),
            ("- : term\n- sibling\n\n    body\n", 1),
            ("- : term\n  - sibling\n\n    body\n", 1),
            ("> : term\n> - sibling\n>\n>   body\n", 1),
        ] {
            let losses = super::losses(source);
            let loss = losses
                .iter()
                .find(|d| d.message.contains("empty definition description"))
                .unwrap_or_else(|| panic!("{source:?}: {losses:?}"));
            assert_eq!(loss.path.as_deref(), Some(format!("line:{line}").as_str()));
            assert_eq!(loss.fidelity, MigrationFidelity::Dropped);
            assert_eq!(loss.confidence, MigrationConfidence::Exact);
            assert_eq!(loss.severity, HtmlImportSeverity::Warning);
        }
        for source in [
            "> : term\n>\n>   body\n",
            "> > : term\n> >\n> >   body\n",
            "> - : term\n>\n>     body\n",
            "- > : term\n  >\n  >   body\n",
            "- : term\n\n    body\n",
            "1. : term\n\n     body\n",
            "- : term\n\n    - nested\n",
            ": term\n - continuation\n\n  body\n",
            "> paragraph\n> : term\n",
            "- paragraph\n  : term\n",
            "> ```\n> : term\n> ```\n",
            "- ```\n  : term\n  ```\n",
            "[^note]: body\n",
            "> [^note]: body\n",
            "- [^note]: body\n",
        ] {
            assert!(super::losses(source).is_empty(), "{source:?}");
        }
    }

    #[test]
    fn djot_ordered_lazy_markers_remain_in_the_open_paragraph() {
        for block in ["# h", "> q", "| t |", "::: n\n   in\n   :::", "***"] {
            let source = format!("1. a\n   {block}\n   1. c\n2. d\n");
            assert!(
                migrate_djot(&source).value.contains("   1\\. c"),
                "{source:?}"
            );
        }
        for marker in ["-", "*"] {
            let source = format!("{marker} a\n  {marker} c\n\n  # h\n{marker} d\n");
            assert!(crate::to_html(&migrate_djot(&source).value).contains("<li><p>d</p></li>"));
        }
        let source = "- a\n\n  second\n\n  - nested\n\n- d\n";
        assert!(crate::to_html(&migrate_djot(source).value).contains("<li><p>d</p></li>"));
    }
    #[test]
    fn djot_nested_formatted_reference_labels_do_not_reenter_the_importer() {
        assert_eq!(super::reference_label("*a*", ""), "a");
        let mut source = "*a*".to_owned();
        for _ in 0..32 {
            source = format!("[{source}][]");
        }
        let result = migrate_djot(&source);
        assert!(result.value.contains("*a*"));
        assert!(result
            .report
            .diagnostics
            .iter()
            .any(|d| d.code == "structure-unspellable"));
    }
    #[test]
    fn empty_heading_report_paths_follow_original_lines() {
        let source = "#\n\n".repeat(512);
        let result = migrate_djot(&source);
        let losses: Vec<_> = result
            .report
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code == "structure-unspellable")
            .collect();
        assert_eq!(losses.len(), 512);
        assert_eq!(losses[0].path.as_deref(), Some("line:1"));
        assert_eq!(losses[511].path.as_deref(), Some("line:1023"));
    }

    #[test]
    fn unfinished_destination_report_is_literal() {
        let source = "x^[[a](b ".repeat(8192);
        let result = migrate_djot(&source);
        assert!(!result
            .report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "structure-unspellable"));
    }
    #[test]
    fn an_unclosed_outer_destination_is_not_a_nested_link() {
        let result = migrate_djot("[[a](u)]( unfinished\n\nx)");
        assert!(!result
            .report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("nested inside another link")));
    }
    #[test]
    fn nested_link_loss_survives_a_plain_bracket() {
        for source in [
            "[a [b [c](u)] d](v)",
            "[text [span [x](u)]{.c}](v)",
            "[![a [b](u)] text](v)",
            "[[a][r]](v)\n\n[r]: u",
            "[[a](u(1))](v)",
        ] {
            let result = migrate_djot(source);
            assert!(
                result
                    .report
                    .diagnostics
                    .iter()
                    .any(|diagnostic| diagnostic.message.contains("nested inside another link")),
                "{source}"
            );
        }
    }

    #[test]
    fn image_alt_links_are_not_nested_anchors() {
        let result = migrate_djot("[![a [b](u)](image)](outer)");
        assert!(!result
            .report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("nested inside another link")));
    }
    #[test]
    fn empty_destinations_fold_container_prefixes() {
        for source in ["> [a](\n> )", "- [a](\n  )", "[a](\n  )"] {
            let report = migrate_djot(source);
            assert!(report
                .report
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.message.contains("empty destination")));
        }
        for source in ["[a][b][c]\n\n[b]: /u", "[a][b]()\n\n[b]: /u"] {
            let report = migrate_djot(source);
            assert!(!report
                .report
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "structure-unspellable"));
        }
    }

    #[test]
    fn unfinished_code_reference_label_is_literal() {
        let report = migrate_djot("[a][b `c] [x][y]` z");
        assert!(!report
            .report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "structure-unspellable"));
    }

    #[test]
    fn adjacent_live_reference_after_footnote_is_reported() {
        let report = migrate_djot("text[^1][see][ref]");
        assert!(report
            .report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("unresolved Djot reference")));
        let report = migrate_djot("[a](\n)");
        assert!(report
            .report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("empty destination")));
    }

    #[test]
    fn missing_image_destinations_have_exact_losses() {
        for source in ["![a]()", "![a][missing]", "![a][]\n\n[a]:"] {
            let report = migrate_djot(source);
            assert!(report
                .report
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "structure-unspellable"
                    && diagnostic.message.contains("image")));
        }
        let report = migrate_djot("[a]( )");
        assert!(!report
            .report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("empty destination")));
    }

    #[test]
    fn adjacent_footnotes_are_not_reference_links() {
        let report = migrate_djot("[see [^1][^2]](v)\n\n[^1]: one\n\n[^2]: two");
        assert!(!report
            .report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "structure-unspellable"));
    }

    #[test]
    fn escaped_bang_keeps_unresolved_link_loss() {
        let report = migrate_djot("\\![x][undefined]");
        assert!(report
            .report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("unresolved Djot reference")));
    }

    #[test]
    fn unfinished_raw_formats_stop_at_line_boundaries() {
        let source = "`a`{=".repeat(8192);
        assert_eq!(
            super::mask_djot_opaque(&source, super::OpaqueOptions::default()),
            "   {=".repeat(8192).as_bytes()
        );
        assert_eq!(
            super::mask_djot_opaque(&(source + "\n}"), super::OpaqueOptions::default()),
            ("   {=".repeat(8192) + "\n}").as_bytes()
        );
    }

    #[test]
    fn inline_pseudo_definitions_stay_literal() {
        let source = "[] [a]: ".repeat(8192);
        assert_eq!(
            super::mask_djot_opaque(&source, super::OpaqueOptions::default()),
            source.as_bytes()
        );
        assert_eq!(
            super::mask_djot_opaque("[a]: u\n[b]: v", super::OpaqueOptions::default()),
            b"[a]:  \n[b]:  "
        );
    }

    #[test]
    fn repeated_unfinished_comments_stay_literal() {
        let source = "{% ".repeat(8192);
        let mask = super::mask_djot_opaque(&source, super::OpaqueOptions::default());
        assert_eq!(mask, source.as_bytes());
        assert_eq!(
            crate::to_html(&migrate_djot(&source).value),
            format!("<p>{}</p>", source.trim_end())
        );
    }
}

#[cfg(test)]
mod collapsed_key_controls {
    use super::*;
    #[test]
    fn nested_image_code_contributes_child_text() {
        let source = "a ![x `b`](c)";
        let projected = reference_labels_as_spans(&image_label_attributes(source));
        let converted = emphasis::convert_image_child(
            &projected,
            &mask_code_and_destinations(&projected),
            |text| escape_plain_carve_syntax(text, HandledDelimiters::DJOT),
        );
        assert_eq!(
            reference_label(source, ""),
            "a x b",
            "projected={projected:?}; converted={converted:?}; mask={:?}",
            mask_code_and_destinations(source)
        );
    }
}
