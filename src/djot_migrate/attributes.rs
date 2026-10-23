use super::*;

pub(super) fn djot_attribute_context(source: &str, start: usize) -> (usize, Option<usize>, usize) {
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
pub(super) fn djot_attribute_line(mut line: &str, depth: usize) -> Option<&str> {
    let quote = cached_regex!(r"^[ \t]*>(?:[ \t]|$)").unwrap();
    for _ in 0..depth {
        line = &line[quote.find(line)?.len()..];
    }
    Some(line)
}

pub(super) fn mask_djot_attribute_source(source: &str) -> String {
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

pub(super) fn remove_inherited_attribute_markers(
    source: &str,
    inherited: &HashSet<String>,
) -> String {
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

pub(super) fn protect_rejected_comment_quotes(source: &str) -> String {
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

pub(super) fn escape_djot_table_attribute_boundaries(source: &str) -> std::borrow::Cow<'_, str> {
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

pub(super) fn escape_invalid_djot_attributes(source: &str) -> (String, HashSet<String>) {
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

pub(super) struct DjotAttributeToken {
    pub(super) key: String,
    pub(super) source: String,
    pub(super) append: bool,
}

pub(super) fn read_djot_attribute_tokens(
    source: &str,
    start: usize,
) -> Option<(usize, String, Vec<DjotAttributeToken>)> {
    read_djot_attributes_with_tokens(source, start, false, false)
}

pub(super) fn rejected_comment_starts_block(line: &str) -> bool {
    let Some(capture) = cached_regex!(r"^\\\{\x00DJOTINVALIDATTR\x00[0-9]+\x00(%[^\n]*)")
        .unwrap()
        .captures(line)
    else {
        return false;
    };
    read_djot_word_attributes(&format!("{{{}}}", &capture[1]), 0).is_some()
}

pub(super) fn read_djot_word_attributes(source: &str, start: usize) -> Option<(usize, String)> {
    read_djot_attributes(source, start, false, false)
}

pub(super) fn read_djot_attributes(
    source: &str,
    start: usize,
    carve: bool,
    table: bool,
) -> Option<(usize, String)> {
    read_djot_attributes_with_tokens(source, start, carve, table).map(|(end, wire, _)| (end, wire))
}

pub(super) fn read_djot_attributes_with_tokens(
    source: &str,
    start: usize,
    carve: bool,
    table: bool,
) -> Option<(usize, String, Vec<DjotAttributeToken>)> {
    read_djot_attributes_with_context(source, start, carve, table, true)
}

pub(super) fn read_djot_attributes_with_context(
    source: &str,
    start: usize,
    carve: bool,
    table: bool,
    paragraph_context: bool,
) -> Option<(usize, String, Vec<DjotAttributeToken>)> {
    read_djot_attributes_with_source_form(source, start, carve, table, paragraph_context, false)
}

pub(super) fn read_djot_attributes_with_source_form(
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

pub(super) struct NativeAttributeReader<'a> {
    source: &'a str,
    line_end: usize,
    table: bool,
}

impl<'a> NativeAttributeReader<'a> {
    pub(super) fn new(source: &'a str) -> Self {
        let line_end = source.find('\n').unwrap_or(source.len());
        let line = &source[..line_end];
        Self {
            source,
            line_end,
            table: line.as_bytes().get(emphasis::structural_prefix_end(line)) == Some(&b'|'),
        }
    }

    pub(super) fn read(&mut self, start: usize) -> Option<(usize, String)> {
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

pub(super) fn djot_word_whitespace(ch: char) -> bool {
    (ch.is_whitespace() && ch != '\u{85}') || ch == '\u{feff}'
}

pub(super) fn protect_attributed_words(
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
