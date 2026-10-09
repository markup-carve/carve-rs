use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

#[derive(Clone)]
struct Pair {
    start: usize,
    open_end: usize,
    close: usize,
    end: usize,
    kind: u8,
    forced: bool,
    children: Vec<usize>,
    kinds: u8,
}

fn kind_bit(kind: u8) -> u8 {
    if kind == b'_' {
        1
    } else {
        2
    }
}
fn clear(openers: &mut [Vec<(usize, usize, bool)>; 8], from: usize) {
    for stack in openers {
        while stack.last().is_some_and(|opener| opener.0 >= from) {
            stack.pop();
        }
    }
}

pub(super) fn convert(source: &str, mask: &str, convert_plain: impl Fn(&str) -> String) -> String {
    process(source, mask, convert_plain, None)
}

pub(super) fn paired_openers(source: &str, mask: &str) -> HashMap<usize, usize> {
    let mut paired = HashMap::new();
    process(source, mask, str::to_string, Some(&mut paired));
    paired
}

fn process(
    source: &str,
    mask: &str,
    convert_plain: impl Fn(&str) -> String,
    paired: Option<&mut HashMap<usize, usize>>,
) -> String {
    let bytes = source.as_bytes();
    let mut mask = mask.as_bytes().to_vec();
    let code_mask = super::mask_code_and_destinations(source);
    let mut attributes = HashMap::new();
    let mut empty_block_attributes = HashSet::new();
    let mut list_boundary_comments = HashMap::new();
    let mut line_start = 0;
    let mut line_end = 0;
    let mut first_attribute_line = true;
    let mut previous_line = "";
    let mut prefix_end = 0;
    let mut list_attribute = false;
    let mut previous_content = "";
    let mut previous_item_width = 0;
    let mut attribute_indent = 0;
    let mut line_prefix = "";
    let mut active_list_column = None;
    let quote_prefix = cached_regex!(r"^(?:[ \t]*>[ \t]?)*").unwrap();
    let previous_item = cached_regex!(r"^[ \t]*(?:[-*+]|(?:[0-9]+|[A-Za-z]|[ivxlcdm]+|[IVXLCDM]+)[.)]|\((?:[0-9]+|[A-Za-z]|[ivxlcdm]+|[IVXLCDM]+)\))[ \t]+").unwrap();
    let attribute_prefix =
        cached_regex!(r"^[ \t>]*(?:(?:[-*+]|(?:[0-9]+|[A-Za-z]|[ivxlcdm]+|[IVXLCDM]+)[.)]|\((?:[0-9]+|[A-Za-z]|[ivxlcdm]+|[IVXLCDM]+)\))[ \t]+)?").unwrap();
    let mut read_native = super::NativeAttributeReader::new(source);
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] == b'\\' {
            at += 2;
            continue;
        }
        if code_mask.as_bytes()[at] == b'{' && mask[at] != b' ' {
            if let Some((mut end, mut wire)) = read_native.read(at) {
                let first_attribute_end = end;
                while bytes.get(end) == Some(&b'{') {
                    let Some((next_end, next_wire)) = read_native.read(end) else {
                        break;
                    };
                    if wire == "{}" {
                        wire.clear();
                    }
                    if next_wire != "{}" {
                        wire.push_str(&next_wire);
                    }
                    if wire.is_empty() {
                        wire.push_str("{}");
                    }
                    end = next_end;
                }
                while first_attribute_line || line_end < at {
                    if !first_attribute_line {
                        previous_line = &source[line_start..line_end];
                        line_start = line_end + 1;
                    }
                    first_attribute_line = false;
                    line_end = source[line_start..]
                        .find('\n')
                        .map_or(source.len(), |offset| line_start + offset);
                    let prefix = attribute_prefix
                        .find(&source[line_start..line_end])
                        .unwrap()
                        .as_str();
                    prefix_end = line_start + prefix.len();
                    list_attribute = prefix.bytes().any(|byte| b"-*+.)".contains(&byte));
                    line_prefix = prefix;
                    previous_content =
                        &previous_line[quote_prefix.find(previous_line).unwrap().end()..];
                    previous_item_width = previous_item
                        .find(previous_content)
                        .map_or(0, |value| value.len());
                    attribute_indent = prefix.len() - quote_prefix.find(prefix).unwrap().end();
                    list_attribute = list_attribute
                        && (line_start == 0
                            || previous_content.trim().is_empty()
                            || active_list_column.is_some());
                    if let Some(column) = active_list_column {
                        if !source[line_start..line_end].trim().is_empty()
                            && attribute_indent < column
                            && !list_attribute
                        {
                            previous_item_width = column;
                            active_list_column = None;
                        }
                    }
                    if list_attribute {
                        active_list_column = Some(attribute_indent);
                    }
                    if line_end == source.len() {
                        break;
                    }
                }
                if at == prefix_end && end != first_attribute_end && end <= line_end {
                    wire = "{%%}".to_owned();
                }
                if wire == "{}"
                    && end == first_attribute_end
                    && at == prefix_end
                    && end <= line_end
                    && source[end..line_end].trim().is_empty()
                    && (list_attribute
                        || line_start == 0
                        || previous_content.trim().is_empty()
                        || previous_content.trim().starts_with('{')
                            && previous_content.trim().ends_with('}')
                        || attribute_indent < previous_item_width)
                {
                    empty_block_attributes.insert(at);
                    if attribute_indent < previous_item_width {
                        list_boundary_comments.insert(at, format!("%%%\n{line_prefix}%%%"));
                    }
                }
                for byte in &mut mask[at..end] {
                    if *byte != b'\n' {
                        *byte = b' ';
                    }
                }
                attributes.insert(at, (end, wire));
                at = end;
                continue;
            }
        }
        at += 1;
    }
    let mut valid_braces = HashSet::new();
    let mut valid_brace_closers = HashSet::new();
    let mut pending_braces: HashMap<u8, Vec<usize>> = HashMap::new();
    let mut brace_line_start = 0;
    let mut last_escaped = None;
    let raw_attribute = cached_regex!(r"^\{=[^\s{}`]+\}").unwrap();
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] == b'\n' {
            if source[brace_line_start..at]
                .trim_matches([' ', '\t', '>'])
                .is_empty()
            {
                pending_braces.clear();
            }
            brace_line_start = at + 1;
        }
        if bytes[at] == b'\\' && bytes.get(at + 1) != Some(&b'\n') {
            last_escaped = Some(at + 1);
            at += 2;
            continue;
        }
        if mask[at] == bytes[at] {
            if bytes[at] == b'{'
                && bytes
                    .get(at + 1)
                    .is_some_and(|byte| b"+-=^~_*".contains(byte))
            {
                pending_braces.entry(bytes[at + 1]).or_default().push(at);
            } else if bytes[at] == b'}' && at > 0 && last_escaped != Some(at - 1) {
                if let Some(start) = pending_braces
                    .get_mut(&bytes[at - 1])
                    .and_then(|stack| stack.pop())
                {
                    if at > start + 2 {
                        valid_braces.insert(start);
                        valid_brace_closers.insert(at - 1);
                        for stack in pending_braces.values_mut() {
                            while stack.last().is_some_and(|at| *at > start) {
                                stack.pop();
                            }
                        }
                    }
                }
            }
        }
        at += 1;
    }
    let mut openers: [Vec<(usize, usize, bool)>; 8] = std::array::from_fn(|_| Vec::new());
    let mut pairs = Vec::<Pair>::new();
    let mut structural = HashSet::new();
    let mut brackets = Vec::new();
    let mut braces = Vec::new();
    let mut bracket_pairs = Vec::new();
    let quoted_prefix = cached_regex!(r"^(?:[ \t]*>[ \t]*)*").unwrap();
    let stars = cached_regex!(r"^(?:[ \t]*>)*[ \t]*(?:\*[ \t]*){3,}$").unwrap();
    let block_start = cached_regex!(r"^[ \t]*(?:`{3,}|~{3,}|:{3,}|#{1,6}[ \t])").unwrap();
    let marker = cached_regex!(r"^[ \t]*(?:[-*+][ \t]|[0-9]+[.)][ \t]|\|)").unwrap();
    let mut previous_blank = true;
    let mut container = false;
    let mut list_column = None;
    let item_prefix = cached_regex!(r"^[ \t]*(?:[-*+]|[0-9]+[.)])[ \t]+").unwrap();
    let mut line_start = 0;
    let mut line_end = bytes.len();
    let mut structural_end = 0;
    let mut thematic_line = false;
    let mut i = 0;
    while i < bytes.len() {
        if i == line_start {
            let end = source[i..]
                .find('\n')
                .map_or(bytes.len(), |offset| i + offset);
            line_end = end;
            structural_end = i + structural_prefix_end(&source[i..end]);
            thematic_line = stars.is_match(&source[i..end]);
            let line = quoted_prefix.replace(&source[i..end], "");
            let indent = line.len() - line.trim_start_matches([' ', '\t']).len();
            if !line.trim().is_empty()
                && list_column.is_some_and(|column| indent < column)
                && !item_prefix.is_match(&line)
            {
                list_column = None;
            }
            if marker.is_match(&line) && (previous_blank || container) {
                clear(&mut openers, 0);
                container = true;
            } else if previous_blank {
                container = list_column.is_some_and(|column| indent >= column);
            }
            if marker.is_match(&line) && container {
                if let Some(item) = item_prefix.find(&line) {
                    list_column = Some(item.end());
                }
            }
            let was_boundary = previous_blank || container;
            previous_blank = line.trim().is_empty()
                || block_start.is_match(&line)
                || line.trim_start().starts_with('{');
            if block_start.is_match(&line) && line.trim_start().starts_with(['`', '~'])
                || line.trim_start().starts_with('#')
                || was_boundary && line.trim_start().starts_with(":::")
            {
                clear(&mut openers, 0);
            }
        }
        let ch = bytes[i];
        if ch == b'\n' {
            if quoted_prefix
                .replace(&source[line_start..i], "")
                .trim()
                .is_empty()
            {
                clear(&mut openers, 0);
                brackets.clear();
                braces.clear();
            }
            line_start = i + 1;
            i += 1;
            continue;
        }
        if ch == b'\\' && bytes.get(i + 1) != Some(&b'\n') {
            i += 2;
            continue;
        }
        if mask[i] != ch {
            i += 1;
            continue;
        }
        if ch == b'{' && valid_braces.contains(&i) {
            braces.push(i);
            i += 1;
            continue;
        }
        if ch == b'}'
            && braces
                .last()
                .is_some_and(|at| bytes[i - 1] == bytes[at + 1])
        {
            clear(&mut openers, braces.pop().unwrap());
            i += 1;
            continue;
        }
        if ch == b'[' {
            brackets.push(i);
            i += 1;
            continue;
        }
        if ch == b']' {
            if let Some(start) = brackets.pop() {
                clear(&mut openers, start);
                bracket_pairs.push((start, i));
            }
            i += 1;
            continue;
        }
        if ch != b'_' && ch != b'*' && !(paired.is_some() && b"~^".contains(&ch)) {
            i += 1;
            continue;
        }
        if ch == b'*' && i <= structural_end {
            if thematic_line {
                for (at, byte) in bytes.iter().enumerate().take(line_end).skip(i) {
                    if *byte == b'*' {
                        structural.insert(at);
                    }
                }
                i = line_end;
                continue;
            }
            if bytes.get(i + 1).is_some_and(|byte| b" \t".contains(byte)) {
                structural.insert(i);
                i += 1;
                continue;
            }
        }
        let forced_open = i > 0
            && bytes[i - 1] == b'{'
            && mask[i - 1] == b'{'
            && !super::is_escaped(bytes, i - 1);
        let forced_close = bytes.get(i + 1) == Some(&b'}');
        let can_open = forced_open
            || (!forced_close
                && bytes
                    .get(i + 1)
                    .is_some_and(|byte| !b" \t\r\n".contains(byte)));
        let can_close =
            !forced_open && (forced_close || (i > 0 && !b" \t\r\n".contains(&bytes[i - 1])));
        let marker = match ch {
            b'_' => 0,
            b'*' => 1,
            b'~' => 2,
            b'^' => 3,
            _ => unreachable!(),
        };
        let key = marker + usize::from(forced_close) * 4;
        if let Some((start, end, forced)) = openers[key].last().copied().filter(|opener| {
            can_close
                && opener.1 < i
                && braces
                    .last()
                    .map_or(true, |at| opener.0 > *at || (opener.2 && opener.0 == *at))
        }) {
            clear(&mut openers, start);
            pairs.push(Pair {
                start,
                open_end: end,
                close: i,
                end: i + if forced_close { 2 } else { 1 },
                kind: ch,
                forced,
                children: Vec::new(),
                kinds: kind_bit(ch),
            });
            if forced_close && braces.last() == Some(&start) {
                braces.pop();
            }
            i += if forced_close { 2 } else { 1 };
        } else if can_open {
            let key = marker + usize::from(forced_open) * 4;
            openers[key].push((i - usize::from(forced_open), i + 1, forced_open));
            i += 1;
        } else {
            i += if forced_close { 2 } else { 1 };
        }
    }
    if let Some(paired) = paired {
        for pair in pairs {
            paired.insert(pair.open_end - 1, pair.end);
        }
        return String::new();
    }
    pairs.sort_by_key(|pair| (pair.start, std::cmp::Reverse(pair.end)));
    let mut roots = Vec::new();
    let mut stack: Vec<usize> = Vec::new();
    for index in 0..pairs.len() {
        while stack
            .last()
            .is_some_and(|parent| pairs[index].start >= pairs[*parent].end)
        {
            stack.pop();
        }
        if let Some(parent) = stack.last() {
            pairs[*parent].children.push(index);
        } else {
            roots.push(index);
        }
        stack.push(index);
    }
    for index in (0..pairs.len()).rev() {
        let kinds = pairs[index]
            .children
            .iter()
            .fold(pairs[index].kinds, |all, child| all | pairs[*child].kinds);
        pairs[index].kinds = kinds;
    }
    let starts: HashMap<_, _> = pairs
        .iter()
        .enumerate()
        .map(|(index, pair)| (pair.start, index))
        .collect();
    let ends: HashSet<_> = pairs.iter().map(|pair| pair.end).collect();
    let mut contexts = HashMap::new();
    let mut active = Vec::new();
    for (at, byte) in bytes.iter().enumerate() {
        if ends.contains(&at) {
            active.pop();
        }
        if let Some(index) = starts.get(&at) {
            active.push(*index);
        }
        if *byte == b'[' || *byte == b']' {
            contexts.insert(at, active.last().copied());
        }
    }
    let mut literal_brackets = HashSet::new();
    let bracket_closes = bracket_pairs.iter().map(|(_, close)| *close).collect();
    for (start, end) in bracket_pairs {
        if contexts.get(&start) != contexts.get(&end) {
            literal_brackets.insert(start);
            literal_brackets.insert(end);
        }
    }
    let mut literal_prefix = "\0DJOTLITERAL\0".to_string();
    while source.contains(&literal_prefix) {
        literal_prefix.push('\0');
    }
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] == b'\\' {
            at += 2;
            continue;
        }
        if mask[at] == b'{'
            && bytes
                .get(at + 1)
                .is_some_and(|kind| b"+-=^~_*".contains(kind))
            && !valid_braces.contains(&at)
            && !starts.contains_key(&at)
        {
            if at > 0
                && bytes[at - 1] == b'`'
                && mask[at - 1] == b' '
                && raw_attribute.is_match(&source[at..])
            {
                at += 1;
                continue;
            }
            literal_brackets.insert(at);
            literal_brackets.insert(at + 1);
        }
        at += 1;
    }
    let renderer = Renderer {
        source,
        mask: &mask,
        attributes: &attributes,
        empty_block_attributes: &empty_block_attributes,
        list_boundary_comments: &list_boundary_comments,
        bracket_closes,
        pairs: &pairs,
        structural: &structural,
        literal_brackets: &literal_brackets,
        valid_braces: &valid_braces,
        valid_brace_closers: &valid_brace_closers,
        literal_prefix,
        literals: RefCell::new(Vec::new()),
        rendered: RefCell::new(HashMap::new()),
    };
    let mut work: Vec<_> = roots.iter().rev().map(|index| (*index, 0, false)).collect();
    while let Some((index, outer, ready)) = work.pop() {
        let pair = &pairs[index];
        if ready {
            let rendered = renderer.render(index, outer);
            renderer.rendered.borrow_mut().insert(index, rendered);
            continue;
        }
        work.push((index, outer, true));
        let bit = kind_bit(pair.kind);
        let scope = pair
            .children
            .iter()
            .any(|child| pairs[*child].kinds & outer != 0);
        let inner = if outer & bit != 0 {
            outer
        } else if scope {
            bit
        } else {
            outer | bit
        };
        for child in pair.children.iter().rev() {
            work.push((*child, inner, false));
        }
    }
    let out = convert_plain(&renderer.body(0, source.len(), &roots, 0));
    if renderer.literals.borrow().is_empty() {
        return out;
    }
    let pattern = regex::Regex::new(&format!(
        r"{}([0-9]+)\x00",
        regex::escape(&renderer.literal_prefix)
    ))
    .unwrap();
    pattern
        .replace_all(&out, |caps: &regex::Captures<'_>| {
            renderer.literals.borrow()[caps[1].parse::<usize>().unwrap()].clone()
        })
        .into_owned()
}

struct Renderer<'a> {
    source: &'a str,
    mask: &'a [u8],
    attributes: &'a HashMap<usize, (usize, String)>,
    empty_block_attributes: &'a HashSet<usize>,
    list_boundary_comments: &'a HashMap<usize, String>,
    bracket_closes: HashSet<usize>,
    pairs: &'a [Pair],
    structural: &'a HashSet<usize>,
    literal_brackets: &'a HashSet<usize>,
    valid_braces: &'a HashSet<usize>,
    valid_brace_closers: &'a HashSet<usize>,
    literal_prefix: String,
    literals: RefCell<Vec<String>>,
    rendered: RefCell<HashMap<usize, String>>,
}
impl Renderer<'_> {
    fn plain(&self, start: usize, end: usize) -> String {
        let bytes = self.source.as_bytes();
        let mut out = Vec::new();
        let mut i = start;
        while i < end {
            if let Some((attribute_end, wire)) = self
                .attributes
                .get(&i)
                .filter(|(attribute_end, _)| *attribute_end <= end)
            {
                let written = if wire == "{}" {
                    if i > 0
                        && self.bracket_closes.contains(&(i - 1))
                        && !self.literal_brackets.contains(&(i - 1))
                    {
                        "{}"
                    } else if let Some(comment) = self.list_boundary_comments.get(&i) {
                        comment
                    } else if self.empty_block_attributes.contains(&i) {
                        "%%"
                    } else {
                        "{%%}"
                    }
                } else {
                    wire
                };
                out.extend_from_slice(self.protect(written).as_bytes());
                i = *attribute_end;
                continue;
            }
            let ch = bytes[i];
            if ch == b'\\' {
                out.push(ch);
                i += 1;
                if i < end {
                    out.push(bytes[i]);
                    i += 1;
                }
                continue;
            }
            if ch == b'='
                && (self.valid_brace_closers.contains(&i)
                    || i > 0 && self.valid_braces.contains(&(i - 1)))
            {
                out.extend_from_slice(self.protect("=").as_bytes());
                i += 1;
                continue;
            }
            if self.mask[i] == ch
                && ((b"~^".contains(&ch)
                    && bytes.get(i + 1) == Some(&b'}')
                    && !self.valid_brace_closers.contains(&i))
                    || (b"_*".contains(&ch) && !self.structural.contains(&i))
                    || self.literal_brackets.contains(&i))
            {
                out.extend_from_slice(self.protect(&format!("\\{}", char::from(ch))).as_bytes());
                i += 1;
                continue;
            }
            out.push(ch);
            i += 1;
        }
        String::from_utf8(out).expect("ASCII delimiter escaping preserves UTF-8")
    }
    fn body(&self, start: usize, end: usize, children: &[usize], _outer: u8) -> String {
        let mut out = String::new();
        let mut cursor = start;
        for index in children {
            let child = &self.pairs[*index];
            out.push_str(&self.plain(cursor, child.start));
            out.push_str(&self.rendered.borrow()[index]);
            cursor = child.end;
        }
        out.push_str(&self.plain(cursor, end));
        out
    }
    fn render(&self, index: usize, outer: u8) -> String {
        let pair = &self.pairs[index];
        let bit = kind_bit(pair.kind);
        if outer & bit != 0 {
            return self.body(pair.open_end, pair.close, &pair.children, outer);
        }
        let scope = pair
            .children
            .iter()
            .any(|child| self.pairs[*child].kinds & outer != 0);
        let content = self.body(
            pair.open_end,
            pair.close,
            &pair.children,
            if scope { bit } else { outer | bit },
        );
        let delimiter = if pair.kind == b'_' { '/' } else { '*' };
        let bytes = self.source.as_bytes();
        let word = |byte: u8| byte.is_ascii_alphanumeric() || byte == b'_';
        let forced = pair.forced
            || scope
            || content.starts_with('\0')
            || content.ends_with('\0')
            || (pair.start > 0 && word(bytes[pair.start - 1]))
            || bytes.get(pair.end).is_some_and(|byte| word(*byte))
            || content.starts_with([' ', '\t', '\r', '\n'])
            || content.ends_with([' ', '\t', '\r', '\n'])
            || content.starts_with(delimiter)
            || content.ends_with(delimiter)
            || (delimiter == '/' && content.starts_with('*') && content.ends_with('*'));
        if forced {
            format!(
                "{}{}{}",
                self.protect(&format!("{{{delimiter}")),
                content,
                self.protect(&format!("{delimiter}}}"))
            )
        } else {
            format!(
                "{}{}{}",
                self.protect(&delimiter.to_string()),
                content,
                self.protect(&delimiter.to_string())
            )
        }
    }
    fn protect(&self, value: &str) -> String {
        let mut literals = self.literals.borrow_mut();
        let token = format!("{}{}\0", self.literal_prefix, literals.len());
        literals.push(value.to_string());
        token
    }
}

pub(super) fn structural_prefix_end(line: &str) -> usize {
    let bytes = line.as_bytes();
    let mut at = 0;
    let spaces = |at: &mut usize| {
        while bytes.get(*at).is_some_and(|ch| b" \t".contains(ch)) {
            *at += 1;
        }
    };
    loop {
        spaces(&mut at);
        if bytes.get(at) != Some(&b'>') {
            break;
        }
        at += 1;
    }
    loop {
        let start = at;
        let mut end = at;
        if bytes.get(end).is_some_and(|ch| b"-*+".contains(ch)) {
            end += 1;
        } else {
            while bytes.get(end).is_some_and(u8::is_ascii_digit) {
                end += 1;
            }
            if end == start || !bytes.get(end).is_some_and(|ch| b".)".contains(ch)) {
                break;
            }
            end += 1;
        }
        if !bytes.get(end).is_some_and(|ch| b" \t".contains(ch)) {
            break;
        }
        at = end;
        spaces(&mut at);
        if bytes.get(at) == Some(&b'[')
            && bytes.get(at + 1).is_some_and(|ch| b" xX-".contains(ch))
            && bytes.get(at + 2) == Some(&b']')
            && bytes.get(at + 3).is_some_and(|ch| b" \t".contains(ch))
        {
            at += 3;
            spaces(&mut at);
        }
    }
    at
}

#[cfg(test)]
mod structural_prefix_tests {
    use super::{convert, structural_prefix_end};

    #[test]
    fn agrees_with_the_prefix_grammar_at_each_asterisk() {
        let grammar = regex::Regex::new(
            r"^(?:[ \t]*>)*[ \t]*(?:(?:[-*+]|[0-9]+[.)])[ \t]+(?:\[[ xX-]\][ \t]+)?)*[ \t]*$",
        )
        .unwrap();
        let chunks = [
            "* ", "+ ", "- ", "12. ", "3)\t", "[x] ", "[ ]\t", ">", "> ", " ", "\t", "\\*", "*a*",
            "1.", "[X]", "[*]", "é", "x",
        ];
        let mut seed = 7_u32;
        for _ in 0..1000 {
            let mut line = String::new();
            for _ in 0..12 {
                seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                line.push_str(chunks[seed as usize % chunks.len()]);
            }
            let end = structural_prefix_end(&line);
            for (at, ch) in line.char_indices() {
                if ch == '*' {
                    assert_eq!(at <= end, grammar.is_match(&line[..at]), "{line} at {at}");
                }
            }
        }
    }

    #[test]
    fn long_marker_chains_preserve_the_trailing_emphasis() {
        for count in [1000, 4000, 16000] {
            let markers = "* ".repeat(count);
            let source = format!("{markers}_a_");
            assert_eq!(
                convert(&source, &source, str::to_string),
                format!("{markers}/a/")
            );
        }
    }

    #[test]
    fn line_facts_reset_after_a_thematic_break() {
        let source = "***\n* _a_\n> * _b_";
        assert_eq!(
            convert(source, source, str::to_string),
            "***\n* /a/\n> * /b/"
        );
    }
}
