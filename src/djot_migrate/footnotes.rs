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

struct NoteAliases {
    labels: HashMap<String, String>,
    serial: usize,
    reserved: HashSet<usize>,
}

impl NoteAliases {
    fn alias(&mut self, key: &str) -> String {
        djot_note_alias(key, &mut self.labels, &mut self.serial, &self.reserved)
    }
}

fn djot_note_key(key: &str, inherited: &HashSet<String>) -> String {
    let key = remove_inherited_attribute_markers(key, inherited);
    cached_regex!(r"[ \t\r\n]+")
        .unwrap()
        .replace_all(
            key.trim_matches(|ch: char| (ch.is_whitespace() && ch != '\u{85}') || ch == '\u{feff}'),
            " ",
        )
        .into_owned()
}

fn unsupported_note_key(key: &str) -> bool {
    key.contains(['[', ']', '`', '<', '>', '|', '\\', '\u{c}', '\0'])
}

fn note_line_prefix() -> &'static regex::Regex {
    cached_regex!(
        r"^[ \t]*(?:>[ \t]+|>$)*(?:(?:[-*+]|(?:[0-9]+|[A-Za-z]|[ivxlcdm]{2,}|[IVXLCDM]{2,})[.)]|\((?:[0-9]+|[A-Za-z]|[ivxlcdm]{2,}|[IVXLCDM]{2,})\)|:)[ \t]+)*"
    )
    .unwrap()
}

fn note_item_pattern() -> &'static regex::Regex {
    cached_regex!(r"(?:[-*+]|(?:[0-9]+|[A-Za-z]|[ivxlcdm]{2,}|[IVXLCDM]{2,})[.)]|\((?:[0-9]+|[A-Za-z]|[ivxlcdm]{2,}|[IVXLCDM]{2,})\)|:)[ \t]+").unwrap()
}

fn note_block_pattern() -> &'static regex::Regex {
    cached_regex!(r"^(?:#{1,6} |`{3,}|~{3,}|:{3,}|\{|\[[^\]]*\]:|(?:[-*][ \t]*){3,}$)").unwrap()
}

fn note_thematic_pattern() -> &'static regex::Regex {
    cached_regex!(r"^(?:[ \t]*>[ \t]*)*[ \t]*(?:[-*][ \t]*){3,}$").unwrap()
}

fn djot_non_row_pipes(source: &str, mask: &str, rows: &[bool]) -> HashSet<usize> {
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
    non_rows
}

#[derive(Default)]
struct NoteDefinitions {
    line_heads: HashSet<usize>,
    definitions: HashMap<usize, (String, usize)>,
    defined: HashSet<String>,
    empty_definitions: HashSet<usize>,
    reference_payloads: Vec<(usize, usize)>,
    literal_reference_colons: HashSet<usize>,
}

struct NoteLine<'a> {
    n: usize,
    line: &'a str,
    at: usize,
    content: &'a str,
    content_column: usize,
    quote_depth: usize,
    item_prefix: bool,
}

struct NoteDefinitionScan<'a> {
    mask: &'a str,
    rows: &'a [bool],
    boundaries: Option<&'a DjotFootnoteAttributeStrip>,
    inherited: &'a HashSet<String>,
    found: NoteDefinitions,
    offset: usize,
    reference_allowed: bool,
    reference_indent: Option<(usize, usize)>,
    reference_containers: Vec<(usize, usize)>,
    reference_divs: Vec<(usize, usize)>,
}

impl<'a> NoteDefinitionScan<'a> {
    fn mask_at(&self, at: usize) -> Option<&u8> {
        self.mask.as_bytes().get(at)
    }

    fn row_before(&self, n: usize) -> bool {
        n.checked_sub(1)
            .is_some_and(|n| self.rows.get(n) == Some(&true))
    }

    fn scan_line(&mut self, n: usize, note_lines: &[&'a str], aliases: &mut NoteAliases) {
        let prefix = note_line_prefix();
        let item = note_item_pattern();
        let line = note_lines[n];
        let at = prefix.find(line).unwrap().len();
        self.found.line_heads.insert(self.offset + at);
        let previous_line = n.checked_sub(1).map_or("", |n| note_lines[n]);
        let previous_prefix = prefix.find(previous_line).unwrap().as_str();
        let boundary = self.definition_boundary(n, line, at, previous_line, previous_prefix);
        let content = &line[at..];
        let item_prefix = item.is_match(&line[..at]);
        let (quote_depth, quote_content) = quoted(line);
        let quote_prefix = line.len() - quote_content.len();
        let note_line = NoteLine {
            n,
            line,
            at,
            content,
            content_column: at.saturating_sub(quote_prefix),
            quote_depth,
            item_prefix,
        };
        let exited_container = self.track_reference_containers(&note_line);
        let allowed = self.reference_allowed
            || item_prefix
            || exited_container
            || quote_depth < quoted(previous_line).0
            || self.row_before(n)
            || self
                .boundaries
                .is_some_and(|strip| strip.is_boundary(previous_line));
        let closed_div = self.track_reference_div(&note_line, allowed);
        self.scan_reference(&note_line, allowed, closed_div);
        self.scan_definition_head(&note_line, boundary, aliases);
        self.offset += line.len() + 1;
    }

    fn definition_boundary(
        &self,
        n: usize,
        line: &str,
        at: usize,
        previous_line: &str,
        previous_prefix: &str,
    ) -> bool {
        let item = note_item_pattern();
        let previous = previous_line[previous_prefix.len()..].trim();
        let mut boundary = self
            .boundaries
            .is_some_and(|strip| strip.is_boundary(previous_line))
            || previous.is_empty()
            || note_thematic_pattern().is_match(previous_line)
            || at < previous_prefix.len() && item.is_match(previous_prefix)
            || self.row_before(n)
            || item.is_match(&line[..at]) && item.is_match(previous_prefix)
            || line[..at].matches('>').count() < previous_prefix.matches('>').count()
            || note_block_pattern().is_match(previous);
        if at > previous_prefix.len() {
            if let Some(previous_note) = cached_regex!(r"^\[\^[^\]\n]+\]:[ \t]*(.+)$")
                .unwrap()
                .captures(previous)
            {
                if !cached_regex!(r"^(?:>(?:[ \t]|$)|[-*+][ \t]|#{1,6}(?:[ \t]|$)|[`~]{3,}|:{3,}|[|]|\{|(?:[0-9]+|[A-Za-z]|[ivxlcdm]+|[IVXLCDM]+)[.)][ \t]|\((?:[0-9]+|[A-Za-z]|[ivxlcdm]+|[IVXLCDM]+)\)[ \t])")
                    .unwrap()
                    .is_match(&previous_note[1])
                {
                    boundary = false;
                }
            }
        }
        boundary
    }

    fn track_reference_containers(&mut self, l: &NoteLine<'_>) -> bool {
        let (quote_depth, content_column) = (l.quote_depth, l.content_column);
        let mut exited_container = false;
        while self
            .reference_containers
            .last()
            .is_some_and(|&(column, depth)| {
                quote_depth < depth || (quote_depth == depth && content_column < column)
            })
        {
            self.reference_containers.pop();
            exited_container = true;
        }
        if l.item_prefix {
            while self
                .reference_containers
                .last()
                .is_some_and(|&(column, depth)| quote_depth == depth && content_column <= column)
            {
                self.reference_containers.pop();
            }
            self.reference_containers
                .push((content_column, quote_depth));
        }
        exited_container
    }

    fn track_reference_div(&mut self, l: &NoteLine<'_>, allowed: bool) -> bool {
        let Some(caps) = cached_regex!(r"^(:{3,})(?:[ \t]+\S.*)?[ \t]*$")
            .unwrap()
            .captures(l.content)
        else {
            return false;
        };
        let width = caps[1].len();
        if l.content.trim() == &caps[1]
            && self
                .reference_divs
                .last()
                .is_some_and(|&(open_width, depth)| width >= open_width && l.quote_depth == depth)
        {
            self.reference_divs.pop();
            return true;
        } else if allowed {
            self.reference_divs.push((width, l.quote_depth));
        }
        false
    }

    fn scan_reference(&mut self, l: &NoteLine<'_>, allowed: bool, closed_div: bool) {
        let (offset, at, content) = (self.offset, l.at, l.content);
        let reference = allowed && self.mask_at(offset + at) == Some(&b'[');
        if let Some(caps) = reference
            .then(|| {
                cached_regex!(r"^\[(?:[^\^\]\r\n][^\]\r\n]*)?\]:([ \t]+[^ \t\r\n]*|)\r?$")
                    .unwrap()
                    .captures(content)
            })
            .flatten()
        {
            let target = caps.get(1).unwrap();
            self.found
                .reference_payloads
                .push((offset + at + target.start(), offset + at + target.end()));
            self.reference_indent = Some((l.content_column, l.quote_depth));
            self.reference_allowed = true;
        } else if self
            .reference_indent
            .is_some_and(|(indent, depth)| l.content_column > indent && l.quote_depth == depth)
            && !l.item_prefix
            && cached_regex!(r"^[^ \t\r\n]+\r?$")
                .unwrap()
                .is_match(content)
        {
            self.found
                .reference_payloads
                .push((offset + at, offset + l.line.len()));
        } else {
            if self.mask_at(offset + at) == Some(&b'[') {
                if let Some(candidate) = cached_regex!(r"^\[(?:[^\^\]\r\n][^\]\r\n]*)?\]:")
                    .unwrap()
                    .find(content)
                {
                    self.found
                        .literal_reference_colons
                        .insert(offset + at + candidate.end() - 1);
                }
            }
            self.reference_indent = None;
            self.reference_allowed = content.trim().is_empty()
                || (allowed && !content.starts_with('[') && note_block_pattern().is_match(content))
                || (allowed && note_thematic_pattern().is_match(l.line))
                || self.rows.get(l.n) == Some(&true)
                || self
                    .boundaries
                    .is_some_and(|strip| strip.is_boundary(l.line))
                || closed_div
                || (cached_regex!(r"^(?:`{3,}|~{3,})[ \t]*$")
                    .unwrap()
                    .is_match(content)
                    && self.mask_at(offset + at) == Some(&b' '));
        }
    }

    fn define(&mut self, at: usize, raw_key: &str) -> String {
        let key = djot_note_key(raw_key, self.inherited);
        self.found.defined.insert(key.clone());
        self.found
            .definitions
            .insert(at, (key.clone(), at + 2 + raw_key.len()));
        key
    }

    fn scan_definition_head(
        &mut self,
        l: &NoteLine<'_>,
        boundary: bool,
        aliases: &mut NoteAliases,
    ) {
        let (offset, line, at) = (self.offset, l.line, l.at);
        let Some(caps) = cached_regex!(r"^\[\^([^\]\n]+)\]:(?:[ \t]|$)")
            .unwrap()
            .captures(&line[at..])
        else {
            return;
        };
        if !(boundary && self.mask_at(offset + at) == Some(&b'[')) {
            return;
        }
        self.reference_containers
            .push((l.content_column + 2, l.quote_depth));
        let key = self.define(offset + at, &caps[1]);
        if line[at + caps.get(0).unwrap().len()..].trim().is_empty() {
            self.found.empty_definitions.insert(offset + at);
            self.reference_allowed = true;
        }
        if unsupported_note_key(&key) {
            aliases.alias(&key);
        }
        let body_at = at + caps.get(0).unwrap().end();
        for nested in cached_regex!(r"\[\^([^\]\n]+)\]:(?:[ \t]|$)")
            .unwrap()
            .captures_iter(&line[body_at..at + djot_content_start(&line[at..])])
        {
            let nested_at = body_at + nested.get(0).unwrap().start();
            if self.mask_at(offset + nested_at) != Some(&b'[') {
                continue;
            }
            let nested_key = self.define(offset + nested_at, &nested[1]);
            self.found.line_heads.insert(offset + nested_at);
            if unsupported_note_key(&nested_key) {
                aliases.alias(&nested_key);
            }
        }
    }
}

fn scan_djot_note_definitions(
    source: &str,
    mask: &str,
    rows: &[bool],
    boundaries: Option<&DjotFootnoteAttributeStrip>,
    inherited: &HashSet<String>,
    aliases: &mut NoteAliases,
) -> NoteDefinitions {
    let note_lines: Vec<_> = source.split('\n').collect();
    let mut scan = NoteDefinitionScan {
        mask,
        rows,
        boundaries,
        inherited,
        found: NoteDefinitions::default(),
        offset: 0,
        reference_allowed: true,
        reference_indent: None,
        reference_containers: Vec::new(),
        reference_divs: Vec::new(),
    };
    for n in 0..note_lines.len() {
        scan.scan_line(n, &note_lines, aliases);
    }
    scan.found
}

fn djot_autolink_spans(source: &str) -> HashMap<usize, usize> {
    cached_regex!(r"<[^<>\s]+>")
        .unwrap()
        .find_iter(source)
        .filter(|m| {
            cached_regex!(r"[^:]@|[A-Za-z]:")
                .unwrap()
                .is_match(m.as_str())
        })
        .map(|m| (m.start(), m.end()))
        .collect()
}

fn djot_link_destinations(bytes: &[u8], rows: &[bool]) -> HashMap<usize, usize> {
    let mut i = 0;
    let mut destinations = HashMap::new();
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
    destinations
}

fn djot_malformed_attribute_ends(
    source: &str,
    inherited: &HashSet<String>,
) -> HashMap<usize, usize> {
    let bytes = source.as_bytes();
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
    malformed_ends
}

#[derive(Default)]
struct NoteOutput {
    bytes: Vec<u8>,
    used: Vec<String>,
    used_keys: HashSet<String>,
}

struct NoteRewrite<'a> {
    source: &'a str,
    mask: String,
    rows: &'a [bool],
    inherited: &'a HashSet<String>,
    empty_note: &'a str,
    notes: NoteDefinitions,
    non_rows: HashSet<usize>,
    angles: HashMap<usize, usize>,
    destinations: HashMap<usize, usize>,
    malformed_ends: HashMap<usize, usize>,
    brackets: HashMap<usize, usize>,
    accepted_destinations: HashSet<usize>,
    image_ends: HashMap<usize, usize>,
}

impl NoteRewrite<'_> {
    fn match_brackets(&mut self) {
        let source = self.source;
        let bytes = source.as_bytes();
        let blank = cached_regex!(r"^\n[ \t]*(?:>[ \t]+)*>?\n").unwrap();
        let mut stack = Vec::new();
        let mut line_index = 0;
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'\n' {
                line_index += 1;
                if blank.is_match(&source[i..]) {
                    stack.clear();
                }
            }
            if let Some(next) = self.bracket_skip(i) {
                i = next;
                continue;
            }
            if self.rows.get(line_index) == Some(&true)
                && bytes[i] == b'|'
                && i.checked_sub(1).and_then(|p| bytes.get(p)) != Some(&b'\\')
            {
                stack.clear();
                i += 1;
                continue;
            }
            if let Some(definition) = self.notes.definitions.get(&i) {
                i = definition.1 + 1;
                continue;
            }
            i = self.bracket_step(i, &mut stack);
        }
    }

    fn bracket_skip(&self, i: usize) -> Option<usize> {
        let bytes = self.source.as_bytes();
        if bytes[i] == b'\\' {
            return Some(i + 2);
        }
        if bytes[i] == b'{' {
            if let Some((end, _)) = read_djot_word_attributes(self.source, i) {
                return Some(end);
            }
            if let Some(&end) = self.malformed_ends.get(&i) {
                return Some(end);
            }
        }
        if let Some(&end) = self.angles.get(&i) {
            return Some(end);
        }
        if self.mask.as_bytes()[i] == b' ' {
            return Some(i + 1);
        }
        None
    }

    fn bracket_step(&mut self, i: usize, stack: &mut Vec<usize>) -> usize {
        let bytes = self.source.as_bytes();
        if bytes[i] == b'[' {
            stack.push(i);
        } else if bytes[i] == b']' {
            if let Some(&open) = stack.last() {
                if bytes.get(open + 1) == Some(&b'^') {
                    self.brackets.insert(stack.pop().unwrap(), i);
                } else if bytes.get(i + 1) == Some(&b'(')
                    && self.destinations.contains_key(&(i + 1))
                {
                    self.brackets.insert(stack.pop().unwrap(), i);
                    self.accepted_destinations.insert(i + 1);
                    return self.destinations[&(i + 1)];
                } else if bytes.get(i + 1) == Some(&b'[') {
                    let mut end = i + 2;
                    while end < bytes.len() && bytes[end] != b']' {
                        if bytes[end] == b'\\' {
                            end += 1;
                        }
                        end += 1;
                    }
                    if bytes.get(end) == Some(&b']') {
                        self.brackets.insert(stack.pop().unwrap(), i);
                        return end + 1;
                    }
                } else if bytes.get(i + 1) == Some(&b'{')
                    && read_djot_word_attributes(self.source, i + 1).is_some()
                {
                    self.brackets.insert(stack.pop().unwrap(), i);
                }
            }
        }
        i + 1
    }

    fn collect_image_ends(&self) -> HashMap<usize, usize> {
        let bytes = self.source.as_bytes();
        self.brackets
            .iter()
            .filter(|(&at, &close)| {
                at > 0
                    && at < bytes.len()
                    && bytes[at - 1] == b'!'
                    && (bytes.get(at + 1) != Some(&b'^') || close == at + 2)
                    && matches!(bytes.get(close + 1), Some(b'[' | b'('))
            })
            .map(|(&at, &close)| (at, close))
            .collect()
    }

    fn emit(&self, aliases: &mut NoteAliases) -> NoteOutput {
        let source = self.source;
        let bytes = source.as_bytes();
        let mut output = NoteOutput::default();
        let mut i = 0;
        while i < source.len() {
            if let Some(&end) = self
                .destinations
                .get(&i)
                .filter(|_| self.accepted_destinations.contains(&i))
            {
                output.bytes.extend_from_slice(&bytes[i..end]);
                i = end;
                continue;
            }
            if self.notes.literal_reference_colons.contains(&i) {
                output.bytes.push(b'\\');
            }
            if self.non_rows.contains(&i) {
                output.bytes.push(b'\\');
            }
            if let Some(&end) = self.image_ends.get(&i) {
                self.emit_image(i, end, &mut output.bytes);
                i = end + 1;
                continue;
            }
            if let Some(end) = self.attribute_end(i) {
                output.bytes.extend_from_slice(&bytes[i..end]);
                i = end;
                continue;
            }
            if let Some(next) = self.emit_note_label(i, &mut output, aliases) {
                i = next;
                continue;
            }
            if bytes[i] == b'!'
                && !is_escaped(bytes, i)
                && bytes.get(i + 1..i + 3) == Some(b"[^")
                && self.brackets.get(&(i + 1)).is_some_and(|&end| end > i + 3)
                && self.mask.as_bytes().get(i + 1) == Some(&b'[')
            {
                output.bytes.push(b'\\');
            }
            output.bytes.push(bytes[i]);
            i += 1;
        }
        output
    }

    fn emit_image(&self, i: usize, end: usize, output: &mut Vec<u8>) {
        let bytes = self.source.as_bytes();
        let mut cursor = i;
        let mut at = i + 1;
        while at < end {
            if bytes[at..].starts_with(b"[^") {
                if let Some(&close) = self.brackets.get(&at) {
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
    }

    fn attribute_end(&self, i: usize) -> Option<usize> {
        let bytes = self.source.as_bytes();
        if bytes[i] == b'{' && read_djot_word_attributes(self.source, i).is_none() {
            if let Some(&end) = self.malformed_ends.get(&i) {
                return Some(end);
            }
        }
        if bytes[i] == b'{' {
            if let Some((end, _)) = read_djot_word_attributes(self.source, i) {
                return Some(end);
            }
        }
        None
    }

    fn emit_note_label(
        &self,
        i: usize,
        output: &mut NoteOutput,
        aliases: &mut NoteAliases,
    ) -> Option<usize> {
        let source = self.source;
        let bytes = source.as_bytes();
        let definition = self.notes.definitions.get(&i);
        let end = definition
            .map(|d| d.1)
            .or_else(|| self.brackets.get(&i).copied());
        if !(bytes[i..].starts_with(b"[^")
            && (definition.is_some() || self.mask.as_bytes()[i] == b'['))
        {
            return None;
        }
        let end = end.filter(|&end| end > i + 2)?;
        let key = definition.map_or_else(
            || djot_note_key(&source[i + 2..end], self.inherited),
            |d| d.0.clone(),
        );
        let rename = aliases.labels.contains_key(&key)
            || unsupported_note_key(&key)
            || !self.notes.defined.contains(&key);
        let name = if rename {
            aliases.alias(&key)
        } else {
            key.clone()
        };
        let at = i;
        if definition.is_none() && output.used_keys.insert(key.clone()) {
            output.used.push(key.clone());
        }
        if rename || key != source[i + 2..end] {
            output
                .bytes
                .extend_from_slice(format!("[^{name}]").as_bytes());
        } else {
            output.bytes.extend_from_slice(&bytes[i..=end]);
        }
        let mut i = end + 1;
        if definition.is_some() && self.notes.empty_definitions.contains(&at) {
            output.bytes.extend_from_slice(b": ");
            output.bytes.extend_from_slice(self.empty_note.as_bytes());
            i += 1;
        }
        if definition.is_some() && bytes.get(i) == Some(&b':') && bytes.get(i + 1) == Some(&b'\t') {
            output.bytes.extend_from_slice(b": ");
            i += 2;
        }
        if definition.is_none()
            && bytes.get(i) == Some(&b':')
            && self.notes.line_heads.contains(&at)
        {
            output.bytes.extend_from_slice(b"\\:");
            i += 1;
        }
        Some(i)
    }
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
    let non_rows = djot_non_row_pipes(source, &mask, &rows);
    let mut aliases = NoteAliases {
        labels: HashMap::new(),
        serial: 0,
        reserved: cached_regex!(r"(?i)carve-djot-note-(\d+)")
            .unwrap()
            .captures_iter(source)
            .filter_map(|c| c[1].parse().ok())
            .collect(),
    };
    let mut notes =
        scan_djot_note_definitions(source, &mask, &rows, boundaries, inherited, &mut aliases);
    let mut mask = mask.into_bytes();
    for (start, end) in std::mem::take(&mut notes.reference_payloads) {
        blank_out(&mut mask, start, end);
    }
    let mask = String::from_utf8(mask).expect("reference masks preserve UTF-8");
    let mut rewrite = NoteRewrite {
        source,
        mask,
        rows: &rows,
        inherited,
        empty_note,
        notes,
        non_rows,
        angles: djot_autolink_spans(source),
        destinations: djot_link_destinations(source.as_bytes(), &rows),
        malformed_ends: djot_malformed_attribute_ends(source, inherited),
        brackets: HashMap::new(),
        accepted_destinations: HashSet::new(),
        image_ends: HashMap::new(),
    };
    rewrite.match_brackets();
    rewrite.image_ends = rewrite.collect_image_ends();
    let output = rewrite.emit(&mut aliases);
    let mut stubs = String::new();
    for key in output.used {
        if !rewrite.notes.defined.contains(&key) {
            let alias = aliases.alias(&key);
            stubs.push_str(&format!("[^{alias}]: {empty_note}\n\n"));
        }
    }
    stubs.push_str(&String::from_utf8(output.bytes).expect("footnote replacements preserve UTF-8"));
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

struct NoteAttributeGroup {
    line: usize,
    end: usize,
    start: usize,
    wire: String,
}

struct AttributeLine<'a> {
    n: usize,
    line: &'a str,
    prefix: &'a str,
    content: &'a str,
    depth: usize,
    marker: bool,
}

fn attribute_quote_pattern() -> &'static regex::Regex {
    cached_regex!(r"^(?:[ \t]*>[ \t]?)*").unwrap()
}

fn attribute_note_head_pattern() -> &'static regex::Regex {
    cached_regex!(r"^\[\^[^\]\n]+\]:(?:[ \t]|$)").unwrap()
}

struct NoteAttributeStripper<'a> {
    body: &'a str,
    mask: String,
    fence_lines: HashSet<usize>,
    header_lines: usize,
    reserved: HashSet<usize>,
    lines: Vec<String>,
    losses: Vec<usize>,
    comments: HashSet<usize>,
    serial: usize,
    offset: usize,
    boundary: bool,
    pending: Vec<NoteAttributeGroup>,
    quote_depth: usize,
    list_column: Option<usize>,
    list_quote_depth: usize,
    consumed_until: usize,
    heading: bool,
    table: bool,
    dedent: Option<(usize, usize, usize)>,
    note_parents: Vec<usize>,
    metadata_note: bool,
    note_column: Option<usize>,
    reference_column: Option<usize>,
    div_widths: Vec<usize>,
}

impl<'a> NoteAttributeStripper<'a> {
    fn mask_at_content(&self, prefix: &str) -> Option<&u8> {
        self.mask.as_bytes().get(self.offset + prefix.len())
    }

    fn next_comment(&mut self) -> usize {
        while self.reserved.contains(&self.serial) {
            self.serial += 1;
        }
        let id = self.serial;
        self.comments.insert(id);
        self.serial += 1;
        id
    }

    fn strip_line(&mut self, n: usize, line: &'a str) {
        if self.offset < self.consumed_until {
            self.offset += line.len() + 1;
            return;
        }
        let prefix = cached_regex!(r"^(?:[ \t]*>[ \t]?|[ \t]*(?:[-*+]|(?:[0-9]+|[A-Za-z]|[ivxlcdm]+|[IVXLCDM]+)[.)]|\((?:[0-9]+|[A-Za-z]|[ivxlcdm]+|[IVXLCDM]+)\))[ \t]+)*[ \t]*").unwrap().find(line).unwrap().as_str();
        let l = AttributeLine {
            n,
            line,
            prefix,
            content: line[prefix.len()..].trim_end_matches([' ', '\t']),
            depth: prefix.bytes().filter(|ch| *ch == b'>').count(),
            marker: prefix.bytes().any(|ch| b"-*+.)".contains(&ch)),
        };
        let column = self.expand_prefix_tabs(&l);
        self.apply_dedent(&l);
        let exited_note = self.exit_notes(&l, column);
        let column = self.align_note_continuation(&l, column);
        let parent_note_column = self.note_column;
        let (opens_item, opens_quote) = self.track_containers(&l, column);
        if self.continues_reference(&l, column) {
            self.offset += line.len() + 1;
            return;
        }
        let block_allowed = self.boundary || !self.pending.is_empty() || opens_item || opens_quote;
        let (standalone, handled_note) =
            self.collect_attributes(&l, column, parent_note_column, block_allowed);
        self.classify_block(&l, column, block_allowed, standalone, handled_note);
        let content = l.content;
        if exited_note
            && !l.marker
            && content.starts_with('{')
            && read_djot_word_attributes(content, 0).is_some_and(|(end, _)| end == content.len())
            && n > 0
            && !self.lines[n - 1].trim().is_empty()
            && !empty_djot_footnote_task_attributes(&self.lines[n - 1])
        {
            self.lines[n] = format!("{prefix}\n{}", self.lines[n]);
        }
        self.offset += line.len() + 1;
    }

    fn expand_prefix_tabs(&mut self, l: &AttributeLine<'_>) -> usize {
        let quote_pattern = attribute_quote_pattern();
        let (line, prefix) = (l.line, l.prefix);
        let mut column = quote_pattern.replace(prefix, "").len();
        if self.mask_at_content(prefix) != Some(&b' ')
            && prefix.contains('\t')
            && (self.boundary || !self.pending.is_empty() || self.list_column.is_some())
        {
            if let Some(col) = self
                .list_column
                .filter(|col| !l.marker && l.depth == self.list_quote_depth && column + 1 == *col)
            {
                column = col;
                let quotes = quote_pattern.find(prefix).unwrap().as_str();
                self.lines[l.n] = format!(
                    "{}{}{}",
                    quotes.replace('\t', " "),
                    " ".repeat(column),
                    &line[prefix.len()..]
                );
            } else {
                self.lines[l.n] = format!("{}{}", prefix.replace('\t', " "), &line[prefix.len()..]);
            }
        }
        column
    }

    fn apply_dedent(&mut self, l: &AttributeLine<'_>) {
        let Some((dedent_column, delta, dedent_depth)) = self.dedent else {
            return;
        };
        let line = l.line;
        let quotes = attribute_quote_pattern().find(line).unwrap().as_str();
        let indent = line[quotes.len()..]
            .bytes()
            .take_while(|ch| *ch == b' ' || *ch == b'\t')
            .count();
        if !l.content.is_empty() && (l.depth != dedent_depth || indent < dedent_column) {
            self.dedent = None;
        } else if !l.content.is_empty() {
            self.lines[l.n] = format!("{}{}", quotes, &line[quotes.len() + delta..]);
        }
    }

    fn exit_notes(&mut self, l: &AttributeLine<'_>, column: usize) -> bool {
        let n = l.n;
        let mut exited_note = false;
        while self
            .note_column
            .is_some_and(|col| !l.content.is_empty() && column + 1 < col)
        {
            exited_note = true;
            if self.metadata_note
                && self.list_column.is_some_and(|col| column >= col)
                && n > 0
                && self.lines[n - 1]
                    .bytes()
                    .all(|ch| ch == b' ' || ch == b'\t')
            {
                let serial = self.next_comment();
                self.lines[n - 1] = format!(
                    "{}\0DJOTNOTEATTR\x00{serial}\0",
                    " ".repeat(self.list_column.unwrap())
                );
            }
            self.note_column = self.note_parents.pop();
            self.metadata_note = false;
            self.boundary = true;
        }
        exited_note
    }

    fn align_note_continuation(&mut self, l: &AttributeLine<'_>, column: usize) -> usize {
        if self
            .note_column
            .is_some_and(|col| !l.content.is_empty() && column + 1 == col)
        {
            let raw = self.lines[l.n].clone();
            let quotes = attribute_quote_pattern().find(&raw).unwrap().as_str();
            self.lines[l.n] = format!("{} {}", quotes, &raw[quotes.len()..]);
            return column + 1;
        }
        column
    }

    fn track_containers(&mut self, l: &AttributeLine<'_>, column: usize) -> (bool, bool) {
        let (content, depth, marker) = (l.content, l.depth, l.marker);
        if depth < self.quote_depth {
            self.boundary = true;
            self.heading = false;
        }
        if self.list_column.is_some_and(|col| {
            depth != self.list_quote_depth || !content.is_empty() && !marker && column < col
        }) {
            self.list_column = None;
            self.boundary = true;
            self.heading = false;
        }
        let opens_item =
            marker && (self.boundary || !self.pending.is_empty() || self.list_column.is_some());
        let opens_quote =
            depth > self.quote_depth && (self.boundary || !self.pending.is_empty() || opens_item);
        if opens_item || opens_quote {
            self.pending.clear();
            self.heading = false;
        }
        if opens_item {
            self.list_column = Some(column);
            self.list_quote_depth = depth;
        }
        if opens_quote || depth < self.quote_depth {
            self.quote_depth = depth;
        }
        (opens_item, opens_quote)
    }

    fn continues_reference(&mut self, l: &AttributeLine<'_>, column: usize) -> bool {
        let content = l.content;
        if let Some(reference) = self.reference_column {
            if !content.is_empty()
                && column > reference
                && !l.marker
                && cached_regex!(r"^\S+$").unwrap().is_match(content)
            {
                self.boundary = false;
                self.pending.clear();
                return true;
            }
            self.reference_column = None;
            if !content.is_empty() {
                self.boundary = true;
            }
        }
        false
    }

    fn collect_attributes(
        &mut self,
        l: &AttributeLine<'_>,
        column: usize,
        parent_note_column: Option<usize>,
        block_allowed: bool,
    ) -> (bool, bool) {
        let (body, prefix, content) = (self.body, l.prefix, l.content);
        let mut handled_note = false;
        let attrs = if content.starts_with('{') && self.mask_at_content(prefix) == Some(&b'{') {
            read_djot_word_attributes(body, self.offset + prefix.len())
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
            self.pending.push(NoteAttributeGroup {
                line: l.n,
                end,
                start: self.offset,
                wire,
            });
            self.consumed_until = end;
        } else if !content.is_empty() {
            if !self.pending.is_empty()
                && attribute_note_head_pattern().is_match(content)
                && self.mask_at_content(prefix) == Some(&b'[')
            {
                handled_note = true;
                self.attach_pending_attributes(l, column, parent_note_column);
            }
            if !content.is_empty() {
                self.pending.clear();
            }
        }
        (standalone, handled_note)
    }

    fn attach_pending_attributes(
        &mut self,
        l: &AttributeLine<'_>,
        column: usize,
        parent_note_column: Option<usize>,
    ) {
        let (n, line, prefix) = (l.n, l.line, l.prefix);
        let quote_prefix = attribute_quote_pattern().find(prefix).unwrap().as_str();
        let target_column = self
            .list_column
            .unwrap_or(0)
            .max(parent_note_column.unwrap_or(0));
        let note_prefix = format!("{}{}", quote_prefix, " ".repeat(target_column));
        if column > target_column {
            self.dedent = Some((column, column - target_column, l.depth));
        }
        self.lines[n] = format!("{}{}", note_prefix, &line[prefix.len()..]);
        let pending = std::mem::take(&mut self.pending);
        for group in &pending {
            if group.wire != "{}" {
                self.losses.push(self.header_lines + group.line + 1);
            }
            let mut at = group.line;
            let mut position = group.start;
            while at < n && position < group.end {
                let raw = self.lines[at].clone();
                let lead = if at == group.line {
                    &raw[..raw.find('{').unwrap()]
                } else {
                    cached_regex!(r"^(?:[ \t]*>[ \t]?)*[ \t]*")
                        .unwrap()
                        .find(&raw)
                        .unwrap()
                        .as_str()
                };
                let serial = self.next_comment();
                self.lines[at] = format!("{lead}\0DJOTNOTEATTR\x00{serial}\0");
                position += raw.len() + 1;
                at += 1;
            }
        }
        self.pending = pending;
    }

    fn classify_block(
        &mut self,
        l: &AttributeLine<'_>,
        column: usize,
        block_allowed: bool,
        standalone: bool,
        handled_note: bool,
    ) {
        let (n, line, prefix, content) = (l.n, l.line, l.prefix, l.content);
        let reference = block_allowed
            && self.mask_at_content(prefix) == Some(&b'[')
            && cached_regex!(r"^\[(?:[^\^\]\n][^\]\n]*)?\]:(?:[ \t]+\S*[ \t]*|)$")
                .unwrap()
                .is_match(content);
        if reference {
            self.reference_column = Some(column);
            self.heading = false;
        }
        let note = attribute_note_head_pattern().is_match(content);
        if note && block_allowed && self.mask_at_content(prefix) == Some(&b'[') {
            if let Some(parent) = self.note_column {
                self.note_parents.push(parent);
            }
            self.note_column = Some(column + 2);
            self.metadata_note = handled_note;
            self.heading = false;
        }
        if standalone || self.fence_lines.contains(&n) || content.is_empty() {
            self.heading = false;
        } else if block_allowed
            && cached_regex!(r"^#{1,6}(?:[ \t]|$)")
                .unwrap()
                .is_match(content)
        {
            self.heading = true;
        }
        let row = (block_allowed || self.table)
            && content.starts_with('|')
            && content.ends_with('|')
            && self.mask_at_content(prefix) == Some(&b'|')
            && !is_escaped(
                self.body.as_bytes(),
                self.offset + line.trim_end_matches([' ', '\t']).len() - 1,
            );
        self.table = row;
        let div = self.track_div(content, block_allowed);
        self.boundary = content.is_empty()
            || reference
            || self.fence_lines.contains(&n)
            || row
            || div
            || self.heading
            || block_allowed
                && cached_regex!(r"^(?:[-*][ \t]*){3,}$")
                    .unwrap()
                    .is_match(content);
    }

    fn track_div(&mut self, content: &str, block_allowed: bool) -> bool {
        let Some(colon) = cached_regex!(r"^(:{3,})(?:[ \t].*)?$")
            .unwrap()
            .captures(content)
        else {
            return false;
        };
        let bare = content.bytes().all(|ch| ch == b':');
        if !(block_allowed || !self.div_widths.is_empty() && bare) {
            return false;
        }
        let width = colon[1].len();
        if self.div_widths.last().is_some_and(|old| width >= *old) && bare {
            self.div_widths.pop();
        } else {
            self.div_widths.push(width);
        }
        self.heading = false;
        true
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
    let mut stripper = NoteAttributeStripper {
        body,
        mask,
        fence_lines,
        header_lines,
        reserved: cached_regex!(r"\x00DJOTNOTEATTR\x00(\d+)\x00")
            .unwrap()
            .captures_iter(&source)
            .filter_map(|caps| caps[1].parse().ok())
            .collect(),
        lines: original.iter().map(|line| (*line).to_owned()).collect(),
        losses: Vec::new(),
        comments: HashSet::new(),
        serial: 0,
        offset: 0,
        boundary: true,
        pending: Vec::new(),
        quote_depth: 0,
        list_column: None,
        list_quote_depth: 0,
        consumed_until: 0,
        heading: false,
        table: false,
        dedent: None,
        note_parents: Vec::new(),
        metadata_note: false,
        note_column: None,
        reference_column: None,
        div_widths: Vec::new(),
    };
    for (n, &line) in original.iter().enumerate() {
        stripper.strip_line(n, line);
    }
    DjotFootnoteAttributeStrip {
        source: header + &stripper.lines.join("\n"),
        losses: stripper.losses,
        comments: stripper.comments,
    }
}
