use super::*;

pub(super) struct ImageBoundaryIndex {
    offsets: Vec<usize>,
    tree: Vec<usize>,
    size: usize,
    item_tree: Vec<isize>,
    stride: isize,
    div_closers: std::collections::HashMap<usize, usize>,
}

impl ImageBoundaryIndex {
    pub(super) fn new(source: &str, mask: &[u8]) -> Self {
        let mut offsets = Vec::new();
        let mut thresholds = Vec::new();
        let mut item_thresholds = Vec::new();
        let stride = source.len() as isize + 1;
        let mut div_closers = std::collections::HashMap::new();
        let mut divs: Vec<(usize, usize, usize)> = Vec::new();
        let mut offset = 0;
        let block = cached_regex!(r"^(?:[-*+](?:[ \t]|$)|(?:[0-9]+|[A-Za-z]|[ivxlcdm]+|[IVXLCDM]+)[.)] |\((?:[0-9]+|[A-Za-z]|[ivxlcdm]+|[IVXLCDM]+)\) |: |#{1,6} |`{3,}|~{3,}|:{3,}(?:[ \t]*[A-Za-z0-9_-]+)?[ \t]*$|\||\^ |\[[^\]]+\]:)").unwrap();
        for line in source.split('\n') {
            let (depth, body) = quoted(line);
            let blank = body.trim_matches([' ', '\t']).is_empty();
            let text = body.trim_start_matches([' ', '\t']);
            let indent = body.len() - text.len();
            let attributes = text.starts_with('{')
                && read_djot_word_attributes(text, 0)
                    .is_some_and(|(end, _)| text[end..].trim_matches([' ', '\t']).is_empty());
            let extra_block = cached_regex!(r"^(?:[-*][ \t]*){3,}$")
                .unwrap()
                .is_match(text)
                || attributes;
            if blank || block.is_match(text) || extra_block {
                offsets.push(offset);
                thresholds.push(depth + usize::from(!blank));
                item_thresholds
                    .push(depth as isize * stride + if blank { -1 } else { indent as isize });
            }
            let content_at = djot_content_start(line);
            if let Some(delimiter) = cached_regex!(r"^(:{3,})(?:[ \t]+\S.*)?[ \t]*$")
                .unwrap()
                .captures(&line[content_at..])
            {
                if mask[offset + content_at] != b' ' {
                    if divs.last().is_some_and(|&(width, owner_depth, _)| {
                        depth == owner_depth
                            && cached_regex!(r"^:{3,}[ \t]*$")
                                .unwrap()
                                .is_match(&line[content_at..])
                            && delimiter[1].len() >= width
                    }) {
                        let (_, _, owner_at) = divs.pop().unwrap();
                        div_closers.insert(owner_at, offset);
                    } else {
                        divs.push((delimiter[1].len(), depth, offset));
                    }
                }
            }
            offset += line.len() + 1;
        }
        let size = offsets.len().max(1).next_power_of_two();
        let mut tree = vec![usize::MAX; size * 2];
        let mut item_tree = vec![isize::MAX; size * 2];
        for (at, threshold) in thresholds.into_iter().enumerate() {
            tree[size + at] = threshold;
            item_tree[size + at] = item_thresholds[at];
        }
        for at in (1..size).rev() {
            tree[at] = tree[at * 2].min(tree[at * 2 + 1]);
            item_tree[at] = item_tree[at * 2].min(item_tree[at * 2 + 1]);
        }
        Self {
            offsets,
            tree,
            size,
            item_tree,
            stride,
            div_closers,
        }
    }

    pub(super) fn valid(
        &self,
        start: usize,
        end: usize,
        depth: usize,
        item_column: usize,
        item_quote: usize,
        div_at: Option<usize>,
    ) -> bool {
        if div_at.is_some_and(|at| self.div_closers.get(&at).is_some_and(|&close| close <= end)) {
            return false;
        }
        let boundary = item_quote as isize * self.stride + item_column as isize;
        let mut left = self.offsets.partition_point(|&offset| offset <= start) + self.size;
        let mut right = self.offsets.partition_point(|&offset| offset <= end) + self.size;
        while left < right {
            if left % 2 != 0 {
                if self.tree[left] <= depth || item_column > 0 && self.item_tree[left] < boundary {
                    return false;
                }
                left += 1;
            }
            if right % 2 != 0 {
                right -= 1;
                if self.tree[right] <= depth || item_column > 0 && self.item_tree[right] < boundary
                {
                    return false;
                }
            }
            left /= 2;
            right /= 2;
        }
        true
    }
}

pub(super) fn djot_image_empty_destinations(source: &str) -> String {
    if !source.contains("](") {
        return source.to_owned();
    }
    let mask = mask_code_and_destinations(source);
    let ranges = djot_simple_destination_ranges(source).unwrap_or_else(|| {
        let code_mask = mask_djot_forms_with_options(
            source,
            true,
            true,
            None,
            &[],
            OpaqueOptions {
                destinations: false,
                ..OpaqueOptions::default()
            },
        );
        djot_destination_ranges(source, &code_mask)
    });
    let empty: HashMap<usize, usize> = ranges
        .into_iter()
        .filter(|&(start, end)| {
            source[start + 1..end - 1]
                .trim_matches([' ', '\t', '\r', '\n'])
                .is_empty()
        })
        .collect();
    if empty.is_empty() {
        return source.to_owned();
    }
    let bytes = source.as_bytes();
    let mut brackets = Vec::new();
    let mut removed = vec![false; bytes.len()];
    for at in 0..bytes.len() {
        if mask.as_bytes()[at] != bytes[at] || is_escaped(bytes, at) {
            continue;
        }
        if bytes[at] == b'[' {
            brackets.push(at);
        } else if bytes[at] == b']' {
            let open = brackets.pop();
            if let (Some(open), Some(&end)) = (open, empty.get(&(at + 1))) {
                if bytes.get(open + 1) == Some(&b'^') {
                    continue;
                }
                removed[open] = true;
                removed[at..end].fill(true);
                if open > 0 && bytes[open - 1] == b'!' && !is_escaped(bytes, open - 1) {
                    removed[open - 1] = true;
                }
            }
        }
    }
    let mut output = String::new();
    let mut cursor = 0;
    let mut at = 0;
    while at < removed.len() {
        if removed[at] {
            output.push_str(&source[cursor..at]);
            while at < removed.len() && removed[at] {
                at += 1;
            }
            cursor = at;
        } else {
            at += 1;
        }
    }
    output.push_str(&source[cursor..]);
    output
}

pub(super) fn djot_image_label_child_text(
    label: &str,
    inherited: &HashSet<String>,
    omit_quote_delimiters: bool,
    nul_shielded: bool,
    preserve_source_nuls: bool,
) -> String {
    if let Some(inner) = simple_nested_image_label(label) {
        return djot_image_label_child_text(
            inner,
            inherited,
            omit_quote_delimiters,
            nul_shielded,
            preserve_source_nuls,
        );
    }
    #[cfg(test)]
    IMAGE_LABEL_FULL_PARSES.with(|count| count.set(count.get() + 1));
    let options = crate::Options {
        smart_typography: crate::SmartTypographyMode::Source,
        ..crate::Options::default()
    };
    let empty_destinations = djot_image_empty_destinations(label);
    let normalized = normalize::image_label_attributes(&empty_destinations);
    let label = normalized.as_str();
    let (protected_label, scalars, scalar_prefix) = protect_image_alt_scalars(
        label,
        preserve_source_nuls.then_some(if nul_shielded { "\0U\0" } else { "\0" }),
    );
    let native = djot_to_carve_prepared_with_quotes(
        &strip_footnote_definition_attributes(&format!("DJOTALT {} DJOTEND", protected_label)),
        omit_quote_delimiters,
        nul_shielded,
        true,
    );
    let native = if nul_shielded {
        native.replace("\0U\0", "\0")
    } else {
        native
    };
    let native = remove_inherited_attribute_markers(&native, inherited);
    let mut document = crate::parse_with_options(&native, &options);
    use crate::include_walk::SubtreeVisitor;
    ImageAltChildText {
        preserve_breaks: label.contains('\n'),
    }
    .blocks(&mut document.children);
    let plain = crate::render_plain_text_with_options(&document, &options).unwrap_or_default();
    let plain = plain
        .trim_end_matches('\n')
        .strip_prefix("DJOTALT")
        .unwrap_or("");
    let plain = plain.strip_prefix(' ').unwrap_or(plain);
    let plain = plain.strip_suffix("DJOTEND").unwrap_or("");
    let plain = plain.strip_suffix(' ').unwrap_or(plain);
    let plain = if scalars.is_empty() {
        std::borrow::Cow::Borrowed(plain)
    } else {
        cached_regex!(r"DJOTALTSCALAR([0-9]+)Z([0-9]+)Z")
            .unwrap()
            .replace_all(plain, |caps: &regex::Captures<'_>| {
                if caps[0].starts_with(&scalar_prefix) {
                    caps[2]
                        .parse::<usize>()
                        .ok()
                        .and_then(|at| scalars.get(at))
                        .cloned()
                        .unwrap_or_else(|| caps[0].to_string())
                } else {
                    caps[0].to_string()
                }
            })
    };
    plain.into_owned()
}

// A chain of images whose entire label is another image has the innermost
// label as its plain text. Reading each enclosing label independently would
// reparse the same suffix at every depth.
pub(super) fn simple_nested_image_label(label: &str) -> Option<&str> {
    let bytes = label.as_bytes();
    let mut start = 0;
    while bytes.get(start..start + 2) == Some(b"![") {
        start += 2;
    }
    if start == 0 {
        return None;
    }
    let mut end = bytes.len();
    let mut count = 0;
    while end > start && bytes[end - 1] == b')' {
        let open = label[..end].rfind("](")?;
        if !bytes[open + 2..end - 1]
            .iter()
            .all(|b| b.is_ascii_alphanumeric() || b"/._-".contains(b))
        {
            return None;
        }
        end = open;
        count += 1;
    }
    if count != start / 2 || start >= end {
        return None;
    }
    let mut at = start;
    while at < end {
        match bytes[at] {
            b'\\' if at + 1 < end && !matches!(bytes[at + 1], b'\n' | b'\r') => at += 2,
            b'[' | b']' | b'\\' => return None,
            _ => at += 1,
        }
    }
    Some(&label[start..end])
}

#[cfg(test)]
thread_local! {
    #[allow(clippy::missing_const_for_thread_local)]
    pub(super) static IMAGE_LABEL_FULL_PARSES: std::cell::Cell<usize> = std::cell::Cell::new(0);
}

pub(super) fn image_quote_layout(source: &str, omit_quote_delimiters: bool) -> String {
    let has_forced_quotes = source.contains("{\"")
        || source.contains("{'")
        || source.contains("\"}")
        || source.contains("'}");
    if !omit_quote_delimiters && (!source.contains("![") || !has_forced_quotes) {
        return normalize::inline(source);
    }
    let source = if !omit_quote_delimiters {
        replace_image_labels_with_targets(
            source,
            &mask_code_and_destinations(source),
            |image, label, _, complete| {
                if complete
                    || label.contains("![")
                    || !(label.contains("{\"")
                        || label.contains("{'")
                        || label.contains("\"}")
                        || label.contains("'}"))
                {
                    image.to_owned()
                } else {
                    format!("![{}]", normalize::quote_spellings(label))
                }
            },
            true,
        )
    } else {
        source.to_owned()
    };
    let mut positions = HashSet::new();
    if !omit_quote_delimiters {
        let mask = mask_code_and_destinations(&source);
        let (_, image_lines) = image_line_owners(&source, &mask);
        let mut line = 0;
        replace_image_labels(
            &source,
            &mask_code_and_destinations(&source),
            |image, label, at| {
                if label.starts_with('^') || cached_regex!(r"\n[ \t]*\n").unwrap().is_match(label) {
                    return image.to_owned();
                }
                while image_lines[line].0 <= at {
                    line += 1;
                }
                let (_, quotes, item_column, item_quote, div, _) = image_lines[line];
                if !image_alt_layout(label, quotes, item_column, item_quote, div).2 {
                    return image.to_owned();
                }
                for (i, byte) in label.bytes().enumerate() {
                    if matches!(byte, b'"' | b'\'') {
                        positions.insert(at + 2 + i);
                        positions.insert(at + 1 + i);
                    }
                }
                image.to_owned()
            },
        );
    }
    normalize::inline_with_quote_preservation(&source, |at| {
        omit_quote_delimiters || positions.contains(&at)
    })
}

pub(super) fn replace_image_labels(
    source: &str,
    mask: &str,
    mut replace: impl FnMut(&str, &str, usize) -> String,
) -> String {
    replace_image_labels_with_targets(
        source,
        mask,
        |image, label, at, _| replace(image, label, at),
        false,
    )
}

pub(super) fn replace_image_labels_with_targets(
    source: &str,
    mask: &str,
    mut replace: impl FnMut(&str, &str, usize, bool) -> String,
    allow_incomplete: bool,
) -> String {
    if !source.contains("![") {
        return source.to_owned();
    }
    let bytes = source.as_bytes();
    let visible = mask.as_bytes();
    let mut opens: Vec<usize> = Vec::new();
    let mut closes = HashMap::new();
    let mut boundaries: HashSet<_> = djot_inline_boundaries(source, mask).into_iter().collect();
    for blank in cached_regex!(r"\n[ \t]*\n").unwrap().find_iter(source) {
        boundaries.insert(blank.start());
    }
    let mut reference_ends = vec![None; bytes.len()];
    let mut reference_end = None;
    for at in (0..bytes.len()).rev() {
        if boundaries.contains(&at) {
            reference_end = None;
        }
        if bytes[at] == b']' && !is_escaped(bytes, at) {
            reference_end = Some(at);
        }
        reference_ends[at] = reference_end;
    }
    for at in 0..bytes.len() {
        if boundaries.contains(&at) {
            opens.clear();
        }
        if visible[at] == b'[' && !is_escaped(bytes, at) {
            opens.push(at);
        } else if visible[at] == b']' && !is_escaped(bytes, at) {
            if let Some(open) = opens.pop() {
                let complete_target = match bytes.get(at + 1) {
                    Some(b'(') => visible.get(at + 1) == Some(&b' '),
                    Some(b'[') => reference_ends
                        .get(at + 2)
                        .copied()
                        .flatten()
                        .is_some_and(|end| end >= at + 2),
                    _ => false,
                };
                if open > 0
                    && bytes[open - 1] == b'!'
                    && visible[open - 1] == b'!'
                    && !is_escaped(bytes, open - 1)
                    && (complete_target
                        || allow_incomplete && matches!(bytes.get(at + 1), Some(b'(' | b'[')))
                {
                    closes.insert(open - 1, (at, complete_target));
                }
            }
        }
    }
    if closes.is_empty() {
        return source.to_owned();
    }
    let mut output = String::with_capacity(source.len());
    let mut cursor = 0;
    let mut at = 0;
    while at < bytes.len() {
        if let Some(&(end, complete_target)) = closes.get(&at) {
            let image = &source[at..end + 1];
            let replaced = replace(image, &source[at + 2..end], at, complete_target);
            if replaced == image {
                at += 1;
                continue;
            }
            output.push_str(&source[cursor..at]);
            output.push_str(&replaced);
            cursor = end + 1;
            at = end;
        }
        at += 1;
    }
    output.push_str(&source[cursor..]);
    output
}

pub(super) type ImageLineOwner = (
    usize,
    usize,
    usize,
    usize,
    Option<(usize, usize, usize)>,
    bool,
);

pub(super) fn image_line_owners(source: &str, mask: &str) -> (HashSet<usize>, Vec<ImageLineOwner>) {
    let mut image_starts = HashSet::new();
    let mut image_lines = Vec::new();
    let mut image_offset = 0;
    let mut image_item_column = 0;
    let mut image_item_quote = 0;
    let mut image_blank = false;
    let mut image_divs: Vec<(usize, usize, usize)> = Vec::new();
    let image_owner_marker = cached_regex!(r"^(?:[-*+]|(?:[0-9]+|[A-Za-z]|[ivxlcdm]+|[IVXLCDM]+)[.)]|\((?:[0-9]+|[A-Za-z]|[ivxlcdm]+|[IVXLCDM]+)\)|:)[ \t]+").unwrap();
    let owner_quote = cached_regex!(r"^[ \t]*>(?:[ \t]|$)").unwrap();
    for line in source.split('\n') {
        let content_start = djot_content_start(line);
        image_starts.insert(image_offset + content_start);
        let (depth, body) = quoted(line);
        let text = body.trim_start_matches([' ', '\t']);
        let indent = body.len() - text.len();
        let mut item_body = line;
        for _ in 0..image_item_quote {
            let Some(prefix) = owner_quote.find(item_body) else {
                break;
            };
            item_body = &item_body[prefix.end()..];
        }
        let owner_indent = item_body.len() - item_body.trim_start_matches([' ', '\t']).len();
        if image_owner_marker.is_match(text) {
            image_item_column = indent + 1;
            image_item_quote = depth;
        } else if !text.is_empty()
            && (depth < image_item_quote
                || image_blank && owner_indent < image_item_column
                || depth == image_item_quote
                    && indent < image_item_column
                    && cached_regex!(r"^(?:#{1,6} |:{3,}|[-*+] |\{|\|)")
                        .unwrap()
                        .is_match(text))
        {
            image_item_column = 0;
        }
        image_blank = text.is_empty();
        if let Some(delimiter) = cached_regex!(r"^(:{3,})(?:[ \t]+\S.*)?[ \t]*$")
            .unwrap()
            .captures(&line[content_start..])
        {
            if mask.as_bytes()[image_offset + content_start] != b' ' {
                if image_divs.last().is_some_and(|&(width, div_depth, _)| {
                    line[..content_start]
                        .bytes()
                        .filter(|&ch| ch == b'>')
                        .count()
                        == div_depth
                        && cached_regex!(r"^:{3,}[ \t]*$")
                            .unwrap()
                            .is_match(&line[content_start..])
                        && delimiter[1].len() >= width
                }) {
                    image_divs.pop();
                } else {
                    image_divs.push((
                        delimiter[1].len(),
                        line[..content_start]
                            .bytes()
                            .filter(|&ch| ch == b'>')
                            .count(),
                        content_start,
                    ));
                }
            }
        }
        image_lines.push((
            image_offset + line.len() + 1,
            line[..content_start].bytes().filter(|&b| b == b'>').count(),
            image_item_column,
            image_item_quote,
            image_divs.last().copied(),
            line.as_bytes().get(content_start) == Some(&b'|'),
        ));
        image_offset += line.len() + 1;
    }
    (image_starts, image_lines)
}

pub(super) fn image_alt_layout(
    label: &str,
    quotes: usize,
    item_column: usize,
    item_quote: usize,
    div: Option<(usize, usize, usize)>,
) -> (String, Vec<String>, bool) {
    let mut normalized = String::new();
    let mut continuations = Vec::new();
    let mut valid = true;
    for (index, line) in label.split('\n').enumerate() {
        if index == 0 {
            normalized.push_str(line);
            continue;
        }
        let (depth, body) = quoted(line);
        let mut owner_body = line;
        for _ in 0..item_quote {
            let trimmed = owner_body.trim_start_matches([' ', '\t']);
            if !trimmed.starts_with('>')
                || trimmed.len() > 1 && !matches!(trimmed.as_bytes()[1], b' ' | b'\t')
            {
                break;
            }
            owner_body = &trimmed[if trimmed.len() > 1 { 2 } else { 1 }..];
        }
        let owner_indent = owner_body.len() - owner_body.trim_start_matches([' ', '\t']).len();
        let text = body.trim_start_matches([' ', '\t']);
        let attributes = text.starts_with('{')
            && read_djot_word_attributes(text, 0)
                .is_some_and(|(end, _)| text[end..].trim_matches([' ', '\t']).is_empty());
        let extra_block = cached_regex!(r"^(?:[-*][ \t]*){3,}$")
            .unwrap()
            .is_match(text)
            || attributes;
        if (extra_block
            || cached_regex!(r"^[ \t]*>(?:[ \t]|$)")
                .unwrap()
                .is_match(owner_body))
            && (depth < quotes || item_column > 0 && owner_indent < item_column)
        {
            valid = false;
        }
        if let Some((width, div_depth, column)) = div {
            if depth == div_depth
                && cached_regex!(r"^[ \t]*:{3,}[ \t]*$")
                    .unwrap()
                    .is_match(body)
                && body.trim().len() >= width
                && body.len() - text.len() <= column + 3
            {
                valid = false;
            }
        }
        if (depth < quotes || item_column > 0 && owner_indent < item_column)
            && cached_regex!(
                r"^(?:[-*+](?:[ \t]|$)|(?:[0-9]+|[A-Za-z]|[ivxlcdm]+|[IVXLCDM]+)[.)] |\((?:[0-9]+|[A-Za-z]|[ivxlcdm]+|[IVXLCDM]+)\) |: |#{1,6} |`{3,}|~{3,}|:{3,}(?:[ \t]*[A-Za-z0-9_-]+)?[ \t]*$|\||\^ |\[[^\]]+\]:)"
            )
            .unwrap()
            .is_match(body.trim_start_matches([' ', '\t']))
        {
            valid = false;
        }
        let bytes = line.as_bytes();
        let mut start = 0;
        for _ in 0..quotes {
            while matches!(bytes.get(start), Some(b' ' | b'\t')) {
                start += 1;
            }
            if bytes.get(start) != Some(&b'>')
                || start + 1 < bytes.len() && !matches!(bytes[start + 1], b' ' | b'\t')
            {
                break;
            }
            start += 1;
        }
        while matches!(bytes.get(start), Some(b' ' | b'\t')) {
            start += 1;
        }
        continuations.push(line[..start].to_string());
        normalized.push('\n');
        normalized.push_str(&line[start..]);
    }
    valid &= !cached_regex!(r"\n[ \t]*\n").unwrap().is_match(&normalized);
    (normalized, continuations, valid)
}

pub(super) fn protect_image_alt_scalars(
    label: &str,
    nul_key: Option<&str>,
) -> (String, Vec<String>, String) {
    let names = cached_regex!(r"DJOTALTSCALAR([0-9]+)Z").unwrap();
    let projected: String = label.chars().filter(char::is_ascii_alphanumeric).collect();
    let reserved: HashSet<String> = names
        .captures_iter(&projected)
        .map(|caps| caps[1].to_string())
        .collect();
    let mut serial = 0;
    while reserved.contains(&serial.to_string()) {
        serial += 1;
    }
    let prefix = format!("DJOTALTSCALAR{serial}Z");
    let code_mask = mask_djot_opaque(
        label,
        OpaqueOptions {
            code: false,
            raw_code: false,
            unclosed_code: false,
            ..OpaqueOptions::default()
        },
    );
    let mut runs = Vec::new();
    let mut at = 0;
    while at < code_mask.len() {
        if code_mask[at] != b'`' {
            at += 1;
            continue;
        }
        let start = at;
        while at < code_mask.len() && code_mask[at] == b'`' {
            at += 1;
        }
        runs.push((start, at - start));
    }
    let mut next = HashMap::new();
    let mut ends = HashMap::new();
    for &(at, width) in runs.iter().rev() {
        for offset in 0..width {
            if let Some(&end) = next.get(&(width - offset)) {
                ends.insert(at + offset, end);
            }
        }
        next.insert(width, at + width);
    }
    let raw_formats: HashMap<usize, usize> = cached_regex!(r"\{=[^\x09-\x0d\x20\x{00a0}\x{1680}\x{2000}-\x{200a}\x{2028}\x{2029}\x{202f}\x{205f}\x{3000}\x{feff}{}`]+\}")
        .unwrap()
        .find_iter(label)
        .map(|raw| (raw.start(), raw.end()))
        .collect();
    let mut values = Vec::new();
    let mut code_protected = String::new();
    let mut copied = 0;
    for (mut at, mut width) in runs {
        if is_escaped(label.as_bytes(), at) {
            at += 1;
            width -= 1;
        }
        if width == 0 || at < copied {
            continue;
        }
        let Some(&end) = ends.get(&at) else {
            continue;
        };
        let mut value = &label[at + width..end - width];
        let raw = raw_formats.get(&end).copied();
        if raw.is_some() {
            if value.starts_with(" `") {
                value = &value[1..];
            }
            if value.ends_with("` ") {
                value = &value[..value.len() - 1];
            }
        } else if value.starts_with(' ')
            && value.ends_with(' ')
            && !value.trim_matches([' ', '\t', '\n', '\r']).is_empty()
        {
            value = &value[1..value.len() - 1];
        }
        code_protected.push_str(&label[copied..at]);
        code_protected.push_str(&format!("{prefix}{}Z", values.len()));
        values.push(value.to_string());
        copied = raw.unwrap_or(end);
    }
    code_protected.push_str(&label[copied..]);
    let label = code_protected.as_str();
    let mask = mask_code_and_destinations(label);
    let marker = cached_regex!(
        r"^(#{1,6}|>|[-*+]|:+|\^|(?:[0-9]+|[ivxlcdm]+|[IVXLCDM]+|[a-z]|[A-Z])[.)])(?:[ \t]|$)"
    )
    .unwrap();
    let thematic = cached_regex!(r"^(?:[-*][ \t]*){3,}$").unwrap();
    let mut output = String::new();
    let mut offset = 0;
    for (index, line) in label.split('\n').enumerate() {
        if index > 0 {
            output.push('\n');
        }
        let unicode_blank = !line.is_empty()
            && line
                .chars()
                .all(|ch| ch.is_whitespace() || ch == '\u{feff}')
            && line.chars().any(|ch| !matches!(ch, ' ' | '\t'));
        if unicode_blank {
            output.push_str(&format!("{prefix}{}Z", values.len()));
            values.push(line.to_string());
        } else if index > 0 {
            let text = line.trim_start_matches([' ', '\t']);
            let indent = line.len() - text.len();
            let width = if thematic.is_match(text) {
                text.len()
            } else if text.starts_with('|') {
                1
            } else if text.starts_with("%%") {
                2
            } else {
                marker.captures(text).map_or(0, |caps| caps[1].len())
            };
            if width > 0 && mask.as_bytes()[offset + indent] == text.as_bytes()[0] {
                output.push_str(&line[..indent]);
                output.push_str(&format!("{prefix}{}Z", values.len()));
                values.push(text[..width].to_string());
                output.push_str(&text[width..]);
            } else {
                output.push_str(line);
            }
        } else {
            output.push_str(line);
        }
        offset += line.len() + 1;
    }
    // Reference keys retain NUL identity even though image text renders it as U+FFFD.
    if let Some(value) = nul_key {
        if output.contains(value) {
            let token = format!("{prefix}{}Z", values.len());
            values.push(value.to_string());
            output = output.replace(value, &token);
        }
    }
    (output, values, prefix)
}

pub(super) struct ImageAltChildText {
    pub(super) preserve_breaks: bool,
}

impl crate::include_walk::SubtreeVisitor for ImageAltChildText {
    fn attributes(&mut self, attrs: &mut Option<crate::ast::Attrs>, _inline: bool) {
        *attrs = None;
    }

    fn image(&mut self, image: &mut crate::ast::Image) {
        if image.src.is_empty() {
            image.ref_label = None;
            image.raw_ref = None;
        }
    }

    fn blocks(&mut self, blocks: &mut Vec<crate::ast::BlockNode>) {
        for block in blocks {
            crate::include_walk::visit_block_children(block, self);
        }
    }

    fn inlines(&mut self, inlines: &mut Vec<crate::ast::InlineNode>) {
        for inline in inlines {
            if let crate::ast::InlineNode::CriticDelete(value) = inline {
                *inline = crate::ast::InlineNode::Span(crate::ast::Span {
                    attrs: None,
                    children: std::mem::take(&mut value.children),
                    injected: false,
                    pos: value.pos.take(),
                });
            }
            if matches!(
                inline,
                crate::ast::InlineNode::NonBreakingSpace(_) | crate::ast::InlineNode::Footnote(_)
            ) {
                *inline = crate::ast::InlineNode::Text("".into());
                continue;
            }
            if let crate::ast::InlineNode::Link(link) = inline {
                if link.href.is_empty() {
                    link.ref_label = None;
                    link.raw_ref = None;
                }
            }
            if self.preserve_breaks {
                if let crate::ast::InlineNode::SoftBreak(value) = inline {
                    *inline = crate::ast::InlineNode::HardBreak(value.clone());
                    continue;
                }
            }
            crate::include_walk::visit_inline_children(inline, self);
        }
    }
}
