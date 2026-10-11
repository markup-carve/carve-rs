use super::*;
use std::collections::HashMap;

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
    let last = last.trim_end();
    let at = last.find(|c: char| !c.is_whitespace());
    let Some(at) = at else { return };
    if !last[at..].starts_with('{') {
        return;
    }
    if read_djot_word_attributes(last, at).is_some_and(|(end, _)| end == last.len()) {
        result.pop();
    }
}

pub(super) fn blocks(source: &str) -> String {
    let mask = mask_code_and_destinations(source);
    let mut div_closers = HashMap::new();
    if source.contains(":::") {
        djot_inline_boundaries_with_div_closers(source, &mask, true, Some(&mut div_closers));
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
    let rows = djot_table_rows(source, &mask);
    let fence_mask = mask_djot_fences(source, None, &rows, true);
    let fence_masks: Vec<_> = fence_mask.split('\n').collect();
    let code_opener = cached_regex!(r"^(`{3,}|~{3,})[ \t]*=?[A-Za-z0-9_+#.-]*[ \t]*$").unwrap();
    let mut open_code: Option<(u8, usize, String)> = None;
    let pipe_pass = source.contains("\\|");
    let marker =
        cached_regex!(r"^([-*+]|(?:[0-9]+|[A-Za-z]|[ivxlcdm]+|[IVXLCDM]+)[.)]|\((?:[0-9]+|[A-Za-z]|[ivxlcdm]+|[IVXLCDM]+)\))([ \t]+)(?:\[[ xX]\][ \t]+)?")
            .unwrap();
    let heading = cached_regex!(r"^#{1,6}(?:[ \t]|$)").unwrap();
    let reference = cached_regex!(r"^\[[^\]\n]+\]:").unwrap();
    let ordered_label = cached_regex!(r"[0-9A-Za-z]+").unwrap();
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
    for (n, raw) in lines.iter().enumerate() {
        let (indent, indent_bytes) = leading_indent(raw);
        let text = &raw[indent_bytes..];
        let visible = masks[n].get(indent_bytes..).unwrap_or("");
        let block = rows[n]
            || fence_masks[n] != *raw
            || heading.is_match(visible)
            || visible.starts_with('>')
            || visible.starts_with(":::")
            || visible.starts_with('|')
            || thematic(visible);
        if !raw.trim().is_empty() {
            while divs.last().is_some_and(|(_, _, _, owner, _)| {
                owner.is_some_and(|column| {
                    indent < column && (marker.is_match(visible) || block || previous_blank)
                })
            }) {
                let (width, prefix, _, _, _) = divs.pop().unwrap();
                let before = result
                    .iter()
                    .rposition(|line| !line.trim().is_empty())
                    .map_or(0, |n| n + 1);
                result.insert(before, format!("{prefix}{}", ":".repeat(width)));
                paragraph = false;
                quote_paragraph_depth = None;
            }
        }
        if block {
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
        let target_indent = lists
            .last()
            .filter(|list| indent >= list.column)
            .map_or(indent, |list| list.target_column + indent - list.column);
        let display = if target_indent == indent {
            (*raw).to_owned()
        } else {
            format!("{}{text}", " ".repeat(target_indent))
        };
        if raw.trim().is_empty() {
            quote_paragraph_depth = None;
            paragraph = false;
            in_heading = false;
            in_quote = false;
            previous_blank = true;
            result.push(display.clone());
            continue;
        }
        if visible.trim().is_empty() {
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
        if visible.starts_with('>')
            && (in_quote || !paragraph || n > 0 && masks[n - 1].trim_start().starts_with('>'))
        {
            let (depth, body) = quoted(raw);
            let block_body = body.trim_start();
            if let Some(held) =
                quote_paragraph_depth.filter(|&held| depth > held && !body.trim().is_empty())
            {
                let mut at = 0;
                for _ in 0..held {
                    let rest = &raw[at..];
                    at += rest.len() - rest.trim_start_matches([' ', '\t']).len() + 1;
                    if raw.as_bytes().get(at) == Some(&b' ') {
                        at += 1;
                    }
                }
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
                result.push(raw[..raw.len() - body.len()].trim_end().to_owned());
                quote_paragraph_depth = None;
            }
            quote_paragraph_depth = (!body.trim().is_empty()
                && !heading.is_match(block_body)
                && !marker.is_match(block_body)
                && !block_body.starts_with(":::"))
            .then_some(depth.max(quote_paragraph_depth.unwrap_or(depth)));
            result.push(display.clone());
            in_quote = true;
            paragraph = false;
            previous_blank = body.trim().is_empty();
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
        if let Some(item) = marker.captures(visible).filter(|_| !thematic(text)) {
            if in_quote && !previous_blank {
                result.push(String::new());
                in_quote = false;
                quote_paragraph_depth = None;
            }
            let previous_list = lists.last().cloned();
            let ends_nested = previous_list
                .as_ref()
                .is_some_and(|list| indent < list.source);
            while lists.last().is_some_and(|list| indent < list.source) {
                lists.pop();
            }
            if let Some(parent) = lists
                .last_mut()
                .filter(|list| previous_blank && indent == list.source)
            {
                if ends_nested && !parent.loose {
                    while result.last().is_some_and(|line| line.trim().is_empty()) {
                        result.pop();
                    }
                    let start = previous_list.as_ref().unwrap().start;
                    if !parent.lazy
                        && start > 0
                        && result
                            .get(start - 1)
                            .is_some_and(|line| line.trim().is_empty())
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
            let kind = if is_bullet {
                item[1].to_owned()
            } else {
                ordered_label.replace(&item[1], "1").into_owned()
            };
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
                    if kind != parent.kind {
                        written = if parent.written == b'-' { b'*' } else { b'-' };
                        separate = true;
                    }
                    lists.pop();
                }
            }
            if separate && result.last().is_some_and(|line| !line.is_empty()) {
                result.push(String::new());
            }
            let list_start = top
                .as_ref()
                .filter(|_| !nested && !separate)
                .map_or(result.len(), |list| list.start);
            let marker_end = item.get(2).unwrap().end();
            let payload_start = item.get(0).unwrap().end();
            let mut payload = text.to_owned();
            if is_bullet {
                payload.replace_range(..1, &(written as char).to_string());
            }
            if top
                .as_ref()
                .is_some_and(|list| !nested && list.loose && list.lazy)
                && result.last().is_some_and(|line| !line.trim().is_empty())
            {
                result.push(String::new());
            }
            result.push(format!("{}{payload}", " ".repeat(target)));
            lists.push(List {
                source: indent,
                column: indent + marker_end,
                target,
                target_column: target + marker_end,
                written,
                kind,
                start: list_start,
                lazy: false,
                loose: !nested && top.as_ref().is_some_and(|list| list.loose),
            });
            in_quote = text[payload_start..].starts_with('>');
            let item_body = &text[payload_start..];
            let div_width = item_body.bytes().take_while(|b| *b == b':').count();
            let item_div = div_width >= 3;
            if item_div {
                divs.push((
                    div_width,
                    " ".repeat(target + marker_end),
                    crate::parse::lint_invalid_container_metadata(item_body),
                    Some(indent + marker_end),
                    line_offsets[n],
                ));
            }
            in_heading = heading.is_match(item_body);
            paragraph = !in_quote && !item_div && !in_heading;
            previous_blank = item_div;
            continue;
        }
        if previous_blank {
            while lists.last().is_some_and(|list| indent < list.column) {
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
            } else if paragraph || bare && !divs.is_empty() {
                result.push(format!("{}\\{text}", &raw[..indent_bytes]));
            } else {
                divs.push((
                    width,
                    raw[..indent_bytes].to_owned(),
                    crate::parse::lint_invalid_container_metadata(text),
                    lists
                        .last()
                        .filter(|list| indent >= list.column)
                        .map(|list| list.column),
                    line_offsets[n],
                ));
                result.push(display.clone());
                paragraph = false;
            }
        } else if paragraph
            && (heading.is_match(visible)
                || visible.starts_with('>')
                || visible.starts_with('|') && !pipe_pass
                || thematic(visible) && !(visible.contains('*') && visible.contains('-')))
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
        } else if thematic(visible) {
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
                result.push(display.clone());
            }
            paragraph = false;
        } else {
            let attribute = text.starts_with('{')
                && read_djot_word_attributes(text, 0).is_some_and(|(end, _)| end == text.len());
            result.push(display.clone());
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

pub(super) fn inline(source: &str) -> String {
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
            if let Some((token, glyph)) = quote {
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

fn label(text: &str) -> String {
    let decoded = cached_regex!(r"\\([!-/:-@\[-`{-~])")
        .unwrap()
        .replace_all(text, "$1");
    decoded.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn reference_label(text: &str, explicit: &str) -> String {
    if !explicit.is_empty() {
        return label(explicit);
    }
    if !text.bytes().any(|b| b"_*`~^".contains(&b)) {
        return label(text);
    }
    let converted = emphasis::convert(text, &emphasis_mask(text), str::to_owned);
    label(&crate::to_plain_text_with_options(
        &converted,
        &crate::Options {
            smart_typography: crate::SmartTypographyMode::Source,
            ..Default::default()
        },
    ))
}

fn cached_reference_label<'a>(
    text: &'a str,
    explicit: &str,
    cache: &mut HashMap<&'a str, String>,
) -> String {
    const MAX_CACHED_REFERENCE_LABEL_BYTES: usize = 256;
    if !explicit.is_empty() {
        return label(explicit);
    }
    if !text.bytes().any(|byte| b"_*`~^".contains(&byte)) {
        return label(text);
    }
    // Bound retained values when nested labels overlap in the source.
    if text.len() > MAX_CACHED_REFERENCE_LABEL_BYTES {
        return reference_label(text, explicit);
    }
    cache
        .entry(text)
        .or_insert_with(|| reference_label(text, explicit))
        .clone()
}

fn attributes(text: &str) -> Vec<(String, Vec<String>)> {
    let mut result: Vec<(String, Vec<String>)> = Vec::new();
    let mut slots: HashMap<String, usize> = HashMap::new();
    let mut at = 0;
    while let Some((end, _, tokens)) = read_djot_attribute_tokens(text, at) {
        for token in tokens {
            if let Some(&slot) = slots.get(&token.key) {
                let values = &mut result[slot].1;
                if !token.append {
                    values.clear();
                }
                values.push(token.source);
            } else {
                slots.insert(token.key.clone(), result.len());
                result.push((token.key, vec![token.source]));
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
}

fn reference_mask(source: &str) -> String {
    let mut mask =
        mask_djot_forms_with_options(source, false, true, None, &[], OpaqueOptions::default())
            .into_bytes();
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
    let mut stack = Vec::new();
    let mut uses = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' {
            i += 2;
            continue;
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
                if bytes.get(start + 1) != Some(&b'^') && bytes.get(i + 1) == Some(&b'[') {
                    let mut end = i + 2;
                    while end < bytes.len()
                        && mask.as_bytes()[end] != b']'
                        && mask.as_bytes()[end] != b'['
                    {
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

pub(super) fn references(source: &str) -> String {
    let definition = cached_regex!(r"(?m)^\[([^\]\n^]+)\]:[ \t]*(\S*)[ \t]*$").unwrap();
    let mask = reference_mask(source);
    let mut definitions = HashMap::new();
    let mut removed = Vec::new();
    let mut definition_ranges: HashMap<String, Vec<(usize, usize)>> = HashMap::new();
    for c in definition.captures_iter(source) {
        let start = c.get(0).unwrap().start();
        if mask.as_bytes()[start] != b'[' || c[1].contains('|') {
            continue;
        }
        let mut previous_end = start;
        let mut attr_start = start;
        let mut attr_parts = Vec::new();
        while let Some(previous) = source[..previous_end].strip_suffix('\n') {
            let previous_start = previous.rfind('\n').map_or(0, |at| at + 1);
            let previous_line = &previous[previous_start..];
            if !previous_line.starts_with('{')
                || read_djot_word_attributes(previous_line, 0)
                    .map_or(true, |(end, _)| end != previous_line.len())
            {
                break;
            }
            attr_parts.push(previous_line);
            attr_start = previous_start;
            previous_end = previous_start;
        }
        attr_parts.reverse();
        let attr_text = attr_parts.join("");
        let key = label(&c[1]);
        definition_ranges
            .entry(key.clone())
            .or_default()
            .push((start, c.get(0).unwrap().end()));
        definitions.insert(
            key,
            (c[2].to_owned(), attributes(&attr_text), attr_start != start),
        );
        if attr_start != start {
            removed.push((attr_start, c.get(0).unwrap().end()));
        }
    }
    if definitions.is_empty() {
        return source.to_owned();
    }
    let uses = reference_uses(source, &mask);
    let mut label_cache = HashMap::new();
    let inline_keys: HashSet<_> = uses
        .iter()
        .filter(|reference| {
            reference.label.is_empty() && reference.text.bytes().any(|b| b"_*`~^".contains(&b))
        })
        .map(|reference| cached_reference_label(reference.text, reference.label, &mut label_cache))
        .collect();
    let mut trim_definition_space = !removed.is_empty();
    let mut removed_keys = HashSet::new();
    let mut edits = removed
        .into_iter()
        .map(|(start, end)| (start, end, String::new()))
        .collect::<Vec<_>>();
    for reference in uses {
        let start = reference.start;
        let mut end = reference.end;
        let formatted = reference.text.bytes().any(|b| b"_*`~^".contains(&b));
        let key = cached_reference_label(reference.text, reference.label, &mut label_cache);
        let Some((target, inherited, has_attribute_lines)) = definitions.get(&key) else {
            continue;
        };
        if target.is_empty() {
            continue;
        }
        let replacement = if *has_attribute_lines || inline_keys.contains(&key) {
            let mut own_parts = Vec::new();
            while source[end..].starts_with('{') {
                let Some((length, own)) = read_djot_word_attributes(&source[end..], 0) else {
                    break;
                };
                own_parts.push(own);
                end += length;
            }
            let own_attrs = attributes(&own_parts.join(""));
            let mut attrs: Vec<_> = inherited.iter().collect();
            let mut slots: HashMap<&str, usize> = attrs
                .iter()
                .enumerate()
                .map(|(slot, (key, _))| (key.as_str(), slot))
                .collect();
            for own in &own_attrs {
                if let Some(&slot) = slots.get(own.0.as_str()) {
                    attrs[slot] = own;
                } else {
                    slots.insert(own.0.as_str(), attrs.len());
                    attrs.push(own);
                }
            }
            format!(
                "[{}]({target}){}",
                reference.text,
                if attrs.is_empty() {
                    String::new()
                } else {
                    format!(
                        "{{{}}}",
                        attrs
                            .into_iter()
                            .flat_map(|(_, values)| values.iter())
                            .map(|value| {
                                if let Some((key, quoted)) = value.split_once("=\"") {
                                    let atom = quoted.strip_suffix('"').unwrap_or(quoted);
                                    if !atom.is_empty()
                                        && atom.chars().all(|ch| {
                                            ch.is_ascii_alphanumeric() || "_-.".contains(ch)
                                        })
                                    {
                                        return format!("{key}={atom}");
                                    }
                                }
                                value.to_owned()
                            })
                            .collect::<Vec<_>>()
                            .join(" ")
                    )
                }
            )
        } else if reference.label.contains('\n') {
            format!("[{}][{key}]", reference.text)
        } else {
            continue;
        };
        if !*has_attribute_lines
            && reference.label.is_empty()
            && formatted
            && !removed_keys.contains(&key)
        {
            removed_keys.insert(key.clone());
            for &(start, end) in definition_ranges.get(&key).into_iter().flatten() {
                edits.push((start, end, String::new()));
                trim_definition_space = true;
            }
        }
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
    out
}

fn loss_references(source: &str, mask: &str) -> HashMap<String, String> {
    let definition = cached_regex!(r"^\[([^\[\]\n^]+)\]:[ \t]*(\S*)[ \t]*$").unwrap();
    let item = cached_regex!(r"(?:[-*+]|[0-9A-Za-z]+[.)]|\([0-9A-Za-z]+\))[ \t]+").unwrap();
    let block =
        cached_regex!(r"^(?:#{1,6}(?: |$)|:{3,}|[`~]{3,}|\{|\[[^\]]+\]:|(?:[*-][ \t]*){3,}$)")
            .unwrap();
    let container = cached_regex!(r"(?:>|[-*+] |[0-9A-Za-z]+[.)] |\([0-9A-Za-z]+\) )").unwrap();
    let destination_token = cached_regex!(r"^\S+$").unwrap();
    let heading = cached_regex!(r"^(#{1,6})(?:[ \t]+|$)").unwrap();
    let heading_break = cached_regex!(r"^(?:[#>|{]|[-*+][ \t]|[0-9A-Za-z]+[.)][ \t]|:[ \t]|:{2,}|\([0-9a-zA-Z]+\)[ \t]|[`~]{3,}|\[[^\]\n]*\]:|(?:[*-][ \t]*){3,}$)").unwrap();
    let lines: Vec<_> = source.split('\n').collect();
    let masks: Vec<_> = mask.split('\n').collect();
    let rows = djot_table_rows(source, mask);
    let mut definitions = HashMap::new();
    let mut definition_lines = HashSet::new();
    let mut heading_lines = HashSet::new();
    for (n, line) in lines.iter().enumerate() {
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
        if let Some(caps) = definition
            .captures(content)
            .filter(|_| visible.starts_with('[') && boundary)
        {
            let mut destination = caps[2].to_owned();
            for (next, line) in lines.iter().enumerate().skip(n + 1) {
                let next_at = djot_content_start(line);
                if next_at <= at || !destination_token.is_match(&line[next_at..]) {
                    break;
                }
                destination.push_str(&line[next_at..]);
                definition_lines.insert(next);
            }
            definitions.insert(label(&caps[1]), destination);
            definition_lines.insert(n);
        }
        if let Some(heading) = heading
            .captures(visible)
            .filter(|_| boundary && !heading_lines.contains(&n))
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
    definitions
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
    let mask = reference_mask(source);
    let mut findings: Vec<(usize, &'static str)> = Vec::new();
    let mut nested = Vec::new();
    emphasis::convert_with_losses(source, &emphasis_mask(source), str::to_owned, &mut nested);
    findings.extend(nested.into_iter().map(|at| {
        (
            at,
            "Emphasis exceeding the native nesting budget is flattened; its text is preserved.",
        )
    }));
    let definitions = loss_references(source, &mask);
    let references = reference_uses(source, &mask);
    let mut label_cache = HashMap::new();
    for reference in &references {
        let at = reference.start;
        if reference.text.starts_with('^') {
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
    let quote_prefix = cached_regex!(r"^(?:[ \t]*>(?:[ \t]|$))*").unwrap();
    let mut line_offsets = Vec::new();
    let mut quote_depths = Vec::new();
    let mut layout_offset = 0;
    for line in source.split('\n') {
        line_offsets.push(layout_offset);
        layout_offset += line.len() + 1;
        quote_depths.push(
            quote_prefix
                .find(line)
                .unwrap()
                .as_str()
                .bytes()
                .filter(|&byte| byte == b'>')
                .count(),
        );
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
            && lines.get(n + 1).map_or(true, |line| line.trim().is_empty())
        {
            findings.push((offset, "An empty Djot heading cannot be written in Carve."));
        }
        if empty_description(&lines, &masks, n) {
            findings.push((
                offset,
                "An empty definition description cannot be written in Carve.",
            ));
        }
        if rows[n] && text.len() >= 2 {
            if separator(text) {
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
            } else if text.trim_matches('|').trim().is_empty()
                && (n == 0 || !rows[n - 1])
                && rows.get(n + 1) != Some(&true)
            {
                findings.push((
                    offset,
                    "A table containing only blank cells cannot be written in Carve.",
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
            "- a\n  - c\n  `\n  x\n  `\n- d\n"
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
            ("| |\n", 1, "only blank cells"),
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
        assert!(classes.value.contains("{.a .b}"));
        let image = migrate_djot("{title=t}\n[ref]: /url\n\n[![alt](i.png)][ref]\n");
        assert!(image.value.contains("[![alt](i.png)](/url){title=\"t\"}"));
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
            assert_eq!(
                migrate_djot(&format!("{label}(a(b{{}}c))\n")).value,
                format!("{label}(a%28b{{}}c%29)\n")
            );
            assert_eq!(
                migrate_djot(&format!("{label}(a` b `c)\n")).value,
                format!("{label}(a%60%20b%20%60c)\n")
            );
            assert_eq!(
                migrate_djot(&format!("{label}(a` b `c) and ` code `\n")).value,
                format!("{label}(a%60%20b%20%60c) and `  code  `\n")
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
    fn djot_reference_attributes_use_parsed_tokens_and_own_keys() {
        for (base, own, expected) in [
            ("{.a % .bogus #bogus title=bogus %}", "", "<p><a href=\"/u\" class=\"a\">x</a></p>"),
            ("{.a title=\"a .bogus #bogus % comment %\" key=\"a b\"}", "", "<p><a href=\"/u\" class=\"a\" title=\"a .bogus #bogus % comment %\" key=\"a b\">x</a></p>"),
            ("{.a title=base key=\"a b\"}", "{.b % .bogus key=bogus % title=\"own .c #d % value %\"}", "<p><a href=\"/u\" class=\"b\" title=\"own .c #d % value %\" key=\"a b\">x</a></p>"),
            ("{.a title=\"a \\\"quoted\\\" .b #c %\"}", "", "<p><a href=\"/u\" class=\"a\" title=\"a &quot;quoted&quot; .b #c %\">x</a></p>"),
            ("{.a class=b .c}", "", "<p><a href=\"/u\" class=\"b c\">x</a></p>"),
            ("{% note %}", "", "<p><a href=\"/u\">x</a></p>"),
            ("{}", "", "<p><a href=\"/u\">x</a></p>"),
            ("{.a}\n{.b}", "", "<p><a href=\"/u\" class=\"a b\">x</a></p>"),
            ("{key=a}\n{key=b}", "", "<p><a href=\"/u\" key=\"b\">x</a></p>"),
            ("{.a title=base}\n{.b key=value}", "{.c}{.d title=own}", "<p><a href=\"/u\" class=\"c d\" title=\"own\" key=\"value\">x</a></p>"),
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
    fn djot_reference_attribute_runs_stop_at_blank_lines() {
        for source in [
            "{.a}\n\n[r]: u\n\n[x][r]\n",
            "{.a}\n{.b}\n\n[r]: u\n\n[x][r]\n",
        ] {
            let converted = migrate_djot(source).value;
            assert_eq!(
                crate::to_html(&converted),
                "<p><a href=\"u\">x</a></p>",
                "{converted:?}"
            );
        }
        assert_eq!(
            crate::to_html(&migrate_djot("{.a}\n\n{.b}\n[r]: u\n\n[x][r]\n").value),
            "<p><a href=\"u\" class=\"b\">x</a></p>"
        );
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
