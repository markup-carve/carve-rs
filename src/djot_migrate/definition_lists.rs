use super::*;

#[derive(Clone, Copy)]
pub(super) struct DefinitionFrame {
    source: usize,
    target: usize,
    body: bool,
    ready: bool,
    term_end: usize,
}

pub(super) fn definition_term_block_boundaries(source: &str) -> String {
    if !source.contains(':') {
        return source.to_owned();
    }
    let mask = mask_code_and_destinations(source);
    let masks: Vec<&str> = mask.split('\n').collect();
    let boundaries: HashSet<usize> = djot_inline_boundaries_with_code_scopes(source, &mask, true)
        .into_iter()
        .collect();
    let item = cached_regex!(r"^(?:[ \t]*(?:[-*+]|(?:[0-9]+|[A-Za-z]|[ivxlcdm]{2,}|[IVXLCDM]{2,})[.)]|\((?:[0-9]+|[A-Za-z]|[ivxlcdm]{2,}|[IVXLCDM]{2,})\))[ \t]+)+(:)").unwrap();
    let marker = cached_regex!(r"^(?:#{1,6}(?:[ \t]|$)|:{3,}|:(?:[ \t]|$)|[-*+](?:[ \t]|$)|(?:[0-9]+|[A-Za-z]|[ivxlcdm]{2,}|[IVXLCDM]{2,})[.)](?:[ \t]|$)|\((?:[0-9]+|[A-Za-z]|[ivxlcdm]{2,}|[IVXLCDM]{2,})\)(?:[ \t]|$)|(?:[*-][ \t]*){3,}$)").unwrap();
    let quote = cached_regex!(r"^(?:[ \t]*>(?:[ \t]|$))*").unwrap();
    fn project<'a>(line: &'a str, quote: &regex::Regex) -> (&'a str, &'a str, usize, usize, usize) {
        let prefix = quote.find(line).unwrap().as_str();
        let body = &line[prefix.len()..];
        let (indent, first) = leading_indent(body);
        (
            prefix,
            body,
            prefix.bytes().filter(|byte| *byte == b'>').count(),
            indent,
            first,
        )
    }
    let mut held: Option<(usize, usize)> = None;
    let mut previous_blank = true;
    let mut previous_block = true;
    let mut offset = 0;
    let mut lines: Vec<String> = Vec::new();
    for (index, line) in source.split('\n').enumerate() {
        let (prefix, body, depth, indent, first) = project(line, quote);
        let visible = &masks[index][prefix.len()..];
        let first = item
            .captures(body)
            .map_or(first, |capture| capture.get(1).unwrap().start());
        let blank = body.trim_matches([' ', '\t']).is_empty();
        if blank {
            held = None;
        }
        if visible.as_bytes().get(first) == Some(&b':')
            && cached_regex!(r"^:(?:[ \t]|$)")
                .unwrap()
                .is_match(&body[first..])
            && (previous_blank
                || previous_block
                || held.is_some_and(|(_, owner)| owner == depth)
                || boundaries.contains(&offset))
        {
            let column = body[..first].bytes().fold(0, |column, byte| {
                if byte == b'\t' {
                    column + 4 - column % 4
                } else {
                    column + 1
                }
            });
            if !held.is_some_and(|(prior, owner)| depth == owner && column > prior) {
                held = Some((column, depth));
            }
        } else if held
            .is_some_and(|(column, owner)| depth < owner || depth == owner && indent <= column)
        {
            let text = &body[leading_indent(body).1..];
            let masked = &visible[leading_indent(body).1..];
            let pipe = text.starts_with('|') && cached_regex!(r"\|[ \t]*$").unwrap().is_match(text);
            let code = cached_regex!(r"^(?:`{3,}|~{3,})[ \t]*=?[A-Za-z0-9_+#.-]*[ \t]*$")
                .unwrap()
                .is_match(text);
            if pipe || code || marker.is_match(text) {
                let mut at = lines.len();
                while at > 0 {
                    let (_, prior, prior_depth, prior_indent, first) =
                        project(&lines[at - 1], quote);
                    let attrs = if prior.as_bytes().get(first) == Some(&b'{') {
                        read_djot_word_attributes(prior, first)
                    } else {
                        None
                    };
                    if prior_depth != depth
                        || prior_indent > indent
                        || !attrs.is_some_and(|(end, _)| {
                            prior[end..].trim_matches([' ', '\t']).is_empty()
                        })
                    {
                        break;
                    }
                    at -= 1;
                }
                lines.insert(at, format!("{prefix}{}", " ".repeat(indent)));
                held = None;
            } else if masked.starts_with('>') {
                held = None;
            }
        }
        lines.push(line.to_owned());
        previous_blank = blank;
        previous_block = blank
            || cached_regex!(r"^:{3,}(?:[ \t]+[A-Za-z0-9_-]+)?[ \t]*$")
                .unwrap()
                .is_match(&visible[leading_indent(body).1..]);
        offset += line.len() + 1;
    }
    lines.join("\n")
}

pub(super) fn finish_definition_frame(
    frame: DefinitionFrame,
    lines: &mut [String],
    empty_term: &str,
) {
    if !frame.body {
        lines[frame.term_end].push_str(&format!("\n{}:  {empty_term}", " ".repeat(frame.target)));
    }
}

/// Translate Djot's definition-item shape without canonicalizing unrelated
/// links, raw blocks, attributes, or author spelling elsewhere in the file.
pub(super) fn convert_definition_lists(source: &str, empty_term: &str) -> String {
    convert_definition_lists_at(source, empty_term, 0)
}

pub(super) fn convert_definition_lists_at(source: &str, empty_term: &str, depth: usize) -> String {
    if depth >= crate::parse::MAX_NESTING_DEPTH {
        return convert_root_definition_lists(source, empty_term);
    }
    let gate = cached_regex!(r"(?m)^[ \t]*(?:>[ \t]*)+(?:(?:[-*+]|(?:[0-9]+|[A-Za-z]|[ivxlcdm]{2,}|[IVXLCDM]{2,})[.)]|\((?:[0-9]+|[A-Za-z]|[ivxlcdm]{2,}|[IVXLCDM]{2,})\))[ \t]+)*:(?:[ \t]|$)").unwrap();
    if !gate.is_match(source) {
        return convert_unquoted_definition_lists(source, empty_term, depth);
    }
    let lines: Vec<&str> = source.split('\n').collect();
    let mask = mask_code_and_destinations(source);
    let masks: Vec<&str> = mask.split('\n').collect();
    let quote = cached_regex!(r"^(>)(?:[ \t]|$)").unwrap();
    let quote_start = cached_regex!(r"^>").unwrap();
    let mut result = Vec::new();
    let mut at = 0;
    while at < lines.len() {
        let accepted = quote
            .captures(lines[at])
            .filter(|capture| masks[at].as_bytes().get(capture[1].len() - 1) == Some(&b'>'));
        let Some(accepted) = accepted else {
            let mut end = at + 1;
            while end < lines.len() && !quote_start.is_match(masks[end]) {
                end += 1;
            }
            result.push(convert_unquoted_definition_lists(
                &lines[at..end].join("\n"),
                empty_term,
                depth,
            ));
            at = end;
            continue;
        };
        let mut nested = Vec::new();
        let mut term_column = None;
        let mut ends_block = false;
        let mut end = at;
        while end < lines.len() {
            let prefix = quote.find(lines[end]).filter(|prefix| {
                masks[end]
                    .as_bytes()
                    .get(prefix.as_str().find('>').unwrap())
                    == Some(&b'>')
            });
            let Some(prefix) = prefix else {
                let text = lines[end].trim_start_matches([' ', '\t']);
                let visible = masks[end].trim_start_matches([' ', '\t']);
                let block = cached_regex!(r"^(?:#{1,6}(?:[ \t]|$)|>|:(?:[ \t]|$)|:{3,}|[-*+][ \t]|(?:[0-9]+|[A-Za-z]|[ivxlcdm]{2,}|[IVXLCDM]{2,})[.)][ \t]|\((?:[0-9]+|[A-Za-z]|[ivxlcdm]{2,}|[IVXLCDM]{2,})\)[ \t]|(?:[*-][ \t]*){3,}$)").unwrap().is_match(visible)
                    || visible.starts_with('|') && text.ends_with('|')
                    || cached_regex!(r"^(?:`{3,}|~{3,})[ \t]*=?[A-Za-z0-9_+#.-]*[ \t]*$").unwrap().is_match(text);
                if term_column.is_none() || text.is_empty() || block {
                    ends_block = block;
                    break;
                }
                nested.push(lines[end]);
                end += 1;
                continue;
            };
            let child = &lines[end][prefix.end()..];
            let body = quoted(child).1;
            if let Some(term) = cached_regex!(r"^([ \t]*(?:(?:[-*+]|(?:[0-9]+|[A-Za-z]|[ivxlcdm]{2,}|[IVXLCDM]{2,})[.)]|\((?:[0-9]+|[A-Za-z]|[ivxlcdm]{2,}|[IVXLCDM]{2,})\))[ \t]+)*):(?:[ \t]|$)").unwrap().captures(body) {
                term_column = Some(term[1].len());
            } else if body.trim_matches([' ', '\t']).is_empty() || term_column.is_some_and(|column| leading_indent(body).0 <= column)
                && cached_regex!(r"^(?:#{1,6}(?:[ \t]|$)|>|:{3,}|[-*+][ \t]|(?:[0-9]+|[A-Za-z]|[ivxlcdm]{2,}|[IVXLCDM]{2,})[.)][ \t]|\((?:[0-9]+|[A-Za-z]|[ivxlcdm]{2,}|[IVXLCDM]{2,})\)[ \t])").unwrap().is_match(body.trim_start_matches([' ', '\t'])) {
                term_column = None;
            }
            nested.push(child);
            end += 1;
        }
        let converted = convert_definition_lists_at(&nested.join("\n"), empty_term, depth + 1);
        result.push(
            converted
                .split('\n')
                .map(|line| {
                    format!(
                        "{}{}",
                        &accepted[1],
                        if line.is_empty() {
                            String::new()
                        } else {
                            format!(" {line}")
                        }
                    )
                })
                .collect::<Vec<_>>()
                .join("\n"),
        );
        if ends_block {
            result.push(String::new());
        }
        at = end;
    }
    result.join("\n")
}

pub(super) fn convert_unquoted_definition_lists(
    source: &str,
    empty_term: &str,
    depth: usize,
) -> String {
    let gate = cached_regex!(r"(?m)^[ \t]*(?:(?:[-*+]|(?:[0-9]+|[A-Za-z]|[ivxlcdm]{2,}|[IVXLCDM]{2,})[.)]|\((?:[0-9]+|[A-Za-z]|[ivxlcdm]{2,}|[IVXLCDM]{2,})\))[ \t]+)+[:{]").unwrap();
    if !gate.is_match(source) {
        return convert_root_definition_lists(source, empty_term);
    }
    let lines: Vec<&str> = source.split('\n').collect();
    let mask = mask_code_and_destinations(source);
    let masks: Vec<&str> = mask.split('\n').collect();
    let boundaries: HashSet<usize> = djot_inline_boundaries_with_code_scopes(source, &mask, true)
        .into_iter()
        .collect();
    let item = cached_regex!(r"^([ \t]*)(?:[-*+]|(?:[0-9]+|[A-Za-z]|[ivxlcdm]{2,}|[IVXLCDM]{2,})[.)]|\((?:[0-9]+|[A-Za-z]|[ivxlcdm]{2,}|[IVXLCDM]{2,})\))[ \t]+").unwrap();
    let heads = cached_regex!(r"^[ \t]*(?:(?:[-*+]|(?:[0-9]+|[A-Za-z]|[ivxlcdm]{2,}|[IVXLCDM]{2,})[.)]|\((?:[0-9]+|[A-Za-z]|[ivxlcdm]{2,}|[IVXLCDM]{2,})\))[ \t]+)+").unwrap();
    let term_tail = cached_regex!(r"^(?:(?:[-*+]|(?:[0-9]+|[A-Za-z]|[ivxlcdm]{2,}|[IVXLCDM]{2,})[.)]|\((?:[0-9]+|[A-Za-z]|[ivxlcdm]{2,}|[IVXLCDM]{2,})\))[ \t]+)*:(?:[ \t]|$)").unwrap();
    let markers = cached_regex!(r"(?:[-*+]|(?:[0-9]+|[A-Za-z]|[ivxlcdm]{2,}|[IVXLCDM]{2,})[.)]|\((?:[0-9]+|[A-Za-z]|[ivxlcdm]{2,}|[IVXLCDM]{2,})\))[ \t]+").unwrap();
    let block = cached_regex!(r"^(?:[-*+][ \t]|(?:[0-9]+|[A-Za-z]|[ivxlcdm]{2,}|[IVXLCDM]{2,})[.)][ \t]|\((?:[0-9]+|[A-Za-z]|[ivxlcdm]{2,}|[IVXLCDM]{2,})\)[ \t]|#{1,6}(?:[ \t]|$)|>|:{3,}|[`~]{3,}|\|)").unwrap();
    let mut result = Vec::new();
    let mut offset = 0;
    let mut start = 0;
    let mut at = 0;
    while at < lines.len() {
        let line = lines[at];
        let source_at = offset;
        offset += line.len() + 1;
        let accepted = item.captures(line).filter(|capture| {
            if masks[at].as_bytes().get(capture[1].len()) != line.as_bytes().get(capture[1].len())
                || (at != 0
                    && !boundaries.contains(&source_at)
                    && !cached_regex!(r"^[ \t]*\{loose\}[ \t]*$")
                        .unwrap()
                        .is_match(lines[at - 1]))
            {
                return false;
            }
            let prefix_len = capture.get(0).unwrap().len();
            if term_tail.is_match(&line[prefix_len..]) {
                return true;
            }
            let Some(prefix) = heads.find(line) else {
                return false;
            };
            let Some(last) = markers
                .find_iter(prefix.as_str())
                .nth(1)
                .and_then(|second| markers.find_iter(prefix.as_str()).last().or(Some(second)))
            else {
                return false;
            };
            read_djot_word_attributes(line, prefix.end()).is_some_and(|(end, _)| end == line.len())
                && lines.get(at + 1).is_some_and(|next| {
                    cached_regex!(r"^[ \t]*:(?:[ \t]|$)")
                        .unwrap()
                        .is_match(next)
                        && leading_indent(next).0 > last.start()
                })
        });
        let Some(accepted) = accepted else {
            at += 1;
            continue;
        };
        if at > start {
            result.push(convert_root_definition_lists(
                &lines[start..at].join("\n"),
                empty_term,
            ));
        }
        let prefix = accepted.get(0).unwrap().as_str();
        let column = leading_indent(prefix).0
            + prefix
                .trim_start_matches([' ', '\t'])
                .replace('\t', " ")
                .len();
        let mut nested = vec![&line[prefix.len()..]];
        let mut end = at + 1;
        let mut blank = false;
        while end < lines.len() {
            let next = lines[end];
            let indent = leading_indent(next).0;
            let text = next.trim_start_matches([' ', '\t']);
            if !text.is_empty()
                && indent < column
                && (blank || block.is_match(masks[end].trim_start_matches([' ', '\t'])))
            {
                break;
            }
            nested.push(&next[bytes_through_columns(next, column.min(indent))..]);
            blank = text.is_empty();
            offset += next.len() + 1;
            end += 1;
        }
        let converted = convert_definition_lists_at(&nested.join("\n"), empty_term, depth + 1);
        let converted = converted.strip_prefix("{loose}\n").unwrap_or(&converted);
        let converted: Vec<&str> = converted.split('\n').collect();
        let mut removed = HashSet::new();
        let mut index = 0;
        while index < converted.len() {
            if !converted[index].trim_matches([' ', '\t']).is_empty() {
                index += 1;
                continue;
            }
            let mut end = index;
            while end < converted.len() && converted[end].trim_matches([' ', '\t']).is_empty() {
                end += 1;
            }
            if converted
                .get(end)
                .is_some_and(|line| line.trim_start_matches([' ', '\t']).starts_with(":  "))
            {
                removed.extend(index..end);
            }
            index = end;
        }
        let empty = format!(":  {empty_term}");
        for index in 0..converted.len() {
            if converted[index] != empty {
                continue;
            }
            let mut end = index + 1;
            while end < converted.len() && converted[end].trim_matches([' ', '\t']).is_empty() {
                end += 1;
            }
            if end > index + 1
                && end < converted.len()
                && !converted[end].starts_with([' ', '\t'])
                && !converted[end].starts_with("::")
            {
                removed.extend(index + 1..end);
            }
        }
        if end < lines.len()
            && leading_indent(lines[end]).0 == leading_indent(&accepted[1]).0
            && cached_regex!(r"^(?:[-*+]|[0-9A-Za-z]+[.)]|\([0-9A-Za-z]+\))[ \t]")
                .unwrap()
                .is_match(lines[end].trim_start_matches([' ', '\t']))
        {
            let mut tail = converted.len();
            while tail > 0 && converted[tail - 1].trim_matches([' ', '\t']).is_empty() {
                tail -= 1;
            }
            if tail > 0 && converted[tail - 1] == empty {
                removed.extend(tail..converted.len());
            }
        }
        let mut written = vec![format!("{prefix}{empty_term}")];
        for (index, value) in converted.iter().enumerate() {
            if !removed.contains(&index) {
                written.push(format!("{}{value}", " ".repeat(column)));
            }
        }
        result.push(written.join("\n"));
        at = end;
        start = end;
    }
    if start < lines.len() {
        result.push(convert_root_definition_lists(
            &lines[start..].join("\n"),
            empty_term,
        ));
    }
    result.join("\n")
}

pub(super) fn convert_root_definition_lists(source: &str, empty_term: &str) -> String {
    let _paragraph_cache = ParagraphCacheScope::new(source);
    let mut lines: Vec<String> = source.split('\n').map(str::to_owned).collect();
    let mut removed_blanks = HashSet::new();
    let masked_source = mask_code_and_destinations(source);
    let masked: Vec<&str> = masked_source.split('\n').collect();
    let fence_starts = std::cell::RefCell::new(HashSet::new());
    if source.contains("```") || source.contains("~~~") {
        let _ = mask_djot_fences_with_open(
            source,
            None,
            &[],
            true,
            Some(&|line| {
                fence_starts.borrow_mut().insert(line);
            }),
        );
    }
    let mut stack: Vec<DefinitionFrame> = Vec::new();
    let mut boundaries: Option<HashSet<usize>> = None;
    let mut source_offset = 0;
    for index in 0..lines.len() {
        let source_at = source_offset;
        source_offset += lines[index].len() + 1;
        let raw_term = definition_term(&lines[index]);
        let raw_fence = raw_term.is_some_and(|(_, content)| {
            let payload = &lines[index][content..];
            (payload.starts_with("~~~")
                || (payload.starts_with("```") && !payload.trim_start_matches('`').contains('`')))
                && masked[index]
                    .trim_start_matches([' ', '\t'])
                    .starts_with(':')
        });
        let container_body = raw_term.is_some_and(|(_, content)| {
            cached_regex!(r"^:{3,}(?:[ \t]*[A-Za-z0-9_-]+)?[ \t]*$")
                .unwrap()
                .is_match(&lines[index][content..])
        });
        let starts_definition_body = raw_fence || container_body;
        if let (Some((indent, _)), Some((raw_indent, content))) = (
            if masked[index]
                .trim_start_matches([' ', '\t'])
                .starts_with(':')
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
                finish_definition_frame(stack.pop().unwrap(), &mut lines, empty_term);
            }
            let top = stack.last().copied();
            let may_start = top.is_some()
                || index == 0
                || lines[index - 1].trim_matches([' ', '\t']).is_empty()
                || cached_regex!(r"^ *:{3,}(?:[ \t]+[A-Za-z0-9_-]+)?[ \t]*$")
                    .unwrap()
                    .is_match(masked[index - 1])
                || boundaries
                    .get_or_insert_with(|| {
                        djot_inline_boundaries_with_code_scopes(source, &masked_source, true)
                            .into_iter()
                            .collect()
                    })
                    .contains(&source_at);
            if may_start {
                let mut term = lines[index][content..].to_owned();
                if term.trim_matches([' ', '\t']).is_empty()
                    || term.trim_start_matches([' ', '\t']).starts_with('{')
                        && cached_regex!(r"^(?:[ \t]|\{%%\})*$")
                            .unwrap()
                            .is_match(&normalize::image_label_attributes(&term))
                {
                    term = empty_term.to_owned();
                }
                if top.is_none() || top.is_some_and(|frame| indent == frame.source) {
                    let target = top.map_or(indent, |frame| frame.target);
                    let starts = top.is_none();
                    if let Some(previous) = top {
                        finish_definition_frame(previous, &mut lines, empty_term);
                    }
                    if let Some(frame) = stack.last_mut() {
                        frame.term_end = index;
                        frame.body = false;
                        frame.ready = false;
                    } else {
                        stack.push(DefinitionFrame {
                            source: indent,
                            target,
                            body: false,
                            ready: false,
                            term_end: index,
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
                if top.is_some_and(|frame| frame.ready && indent > frame.source) {
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
                        term_end: index,
                    });
                    continue;
                }
            }
        }
        if stack.is_empty() {
            continue;
        }
        if lines[index].trim_matches([' ', '\t']).is_empty() {
            if let Some(frame) = stack.last_mut() {
                if !frame.body
                    && lines.get(index + 1).is_some_and(|line| {
                        rejected_comment_starts_block(line.trim_start_matches([' ', '\t']))
                    })
                {
                    lines[index] = format!("{}:  {empty_term}\n", " ".repeat(frame.target));
                    frame.body = true;
                }
                frame.ready = true;
            }
            continue;
        }
        if !stack.last().is_some_and(|frame| frame.ready) {
            let continuation = lines[index].trim_start_matches([' ', '\t']);
            let indent = leading_indent(&lines[index]).0;
            let first = leading_indent(&lines[index]).1;
            let visible = masked[index].as_bytes().get(first) == lines[index].as_bytes().get(first)
                || fence_starts.borrow().contains(&index);
            let marker = cached_regex!(r"^(?:#{1,6}(?:[ \t]|$)|>|:{3,}|[-*+][ \t]|(?:[0-9]+|[A-Za-z]|[ivxlcdm]+|[IVXLCDM]+)[.)][ \t]|\((?:[0-9]+|[A-Za-z]|[ivxlcdm]+|[IVXLCDM]+)\)[ \t])").unwrap().is_match(continuation) || cached_regex!(r"^(?:[*-][ \t]*){3,}$").unwrap().is_match(continuation)
                || fence_starts.borrow().contains(&index);
            if visible && indent <= stack.last().unwrap().source && marker {
                while stack.last().is_some_and(|frame| frame.source >= indent) {
                    finish_definition_frame(stack.pop().unwrap(), &mut lines, empty_term);
                }
                if index > 0 {
                    lines[index - 1].push('\n');
                }
                if stack.is_empty() {
                    continue;
                }
            } else {
                stack.last_mut().unwrap().term_end = index;
                if visible
                    && marker
                    && indent > stack.last().unwrap().source
                    && cached_regex!(r"^(?:[-*+][ \t]|(?:[0-9]+|[A-Za-z]|[ivxlcdm]+|[IVXLCDM]+)[.)][ \t]|\((?:[0-9]+|[A-Za-z]|[ivxlcdm]+|[IVXLCDM]+)\)[ \t])").unwrap().is_match(continuation)
                {
                    let literal = cached_regex!(r"^([-*+])").unwrap().replace(continuation, "\\$1");
                    let literal = cached_regex!(r"^([0-9A-Za-z]+)([.)])([ \t])").unwrap().replace(&literal, "$1\\$2$3");
                    lines[index] = cached_regex!(r"^\(").unwrap().replace(&literal, "\\(").into_owned();
                    continue;
                }
                if !cached_regex!(r"^(?:[#>|{]|:{3,}|[`~]{3,}|[-*+] |(?:[0-9]+|[A-Za-z]|[ivxlcdm]+|[IVXLCDM]+)[.)] )").unwrap().is_match(continuation) {
                    let literal = if visible && !marker {
                        cached_regex!(r"^([ivxlcdmIVXLCDM]+)([.)])([ \t])").unwrap().replace(continuation, "$1\\$2$3").into_owned()
                    } else { continuation.to_owned() };
                    let literal = if visible && (literal.starts_with(": ") || literal.starts_with(":\t") || literal == ":") { format!("\\{literal}") } else { literal };
                    lines[index] = format!("{}{}", " ".repeat(stack.last().unwrap().target), literal);
                }
                continue;
            }
        }
        let (indent, _) = leading_indent(&lines[index]);
        while stack.last().is_some_and(|frame| indent <= frame.source) {
            finish_definition_frame(stack.pop().unwrap(), &mut lines, empty_term);
        }
        let Some(frame) = stack.last_mut() else {
            continue;
        };
        if !frame.body && frame.source > 0 {
            let mut blank = index;
            while blank > frame.term_end + 1
                && lines[blank - 1].trim_matches([' ', '\t']).is_empty()
            {
                blank -= 1;
                removed_blanks.insert(blank);
            }
        }
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
    while let Some(frame) = stack.pop() {
        finish_definition_frame(frame, &mut lines, empty_term);
    }
    lines
        .into_iter()
        .enumerate()
        .filter_map(|(index, line)| (!removed_blanks.contains(&index)).then_some(line))
        .collect::<Vec<_>>()
        .join("\n")
}
