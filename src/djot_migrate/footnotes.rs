use super::*;

pub(super) fn djot_note_alias(
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

pub(super) fn normalize_djot_footnotes(
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

pub(crate) struct DjotFootnoteAttributeStrip {
    pub source: String,
    pub losses: Vec<usize>,
    pub(super) comments: HashSet<usize>,
}

impl DjotFootnoteAttributeStrip {
    pub(super) fn is_boundary(&self, line: &str) -> bool {
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
