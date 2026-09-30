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
fn clear(openers: &mut [Vec<(usize, usize, bool)>; 4], from: usize) {
    for stack in openers {
        while stack.last().is_some_and(|opener| opener.0 >= from) {
            stack.pop();
        }
    }
}

pub(super) fn convert(source: &str, mask: &str, convert_plain: impl Fn(&str) -> String) -> String {
    let bytes = source.as_bytes();
    let mask = mask.as_bytes();
    let mut valid_braces = HashSet::new();
    let mut pending_braces: HashMap<u8, Vec<usize>> = HashMap::new();
    let mut brace_line_start = 0;
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
        if bytes[at] == b'\\' {
            at += 2;
            continue;
        }
        if mask[at] == bytes[at] {
            if bytes[at] == b'{'
                && bytes
                    .get(at + 1)
                    .is_some_and(|byte| b"+-=^~".contains(byte))
            {
                pending_braces.entry(bytes[at + 1]).or_default().push(at);
            } else if bytes[at] == b'}' && at > 0 {
                if let Some(start) = pending_braces
                    .get_mut(&bytes[at - 1])
                    .and_then(|stack| stack.pop())
                {
                    if at > start + 2 {
                        valid_braces.insert(start);
                    }
                }
            }
        }
        at += 1;
    }
    let mut openers: [Vec<(usize, usize, bool)>; 4] = std::array::from_fn(|_| Vec::new());
    let mut pairs = Vec::<Pair>::new();
    let mut structural = HashSet::new();
    let mut brackets = Vec::new();
    let mut braces = Vec::new();
    let mut bracket_pairs = Vec::new();
    let quoted_prefix = cached_regex!(r"^(?:[ \t]*>[ \t]*)*").unwrap();
    let stars = cached_regex!(r"^(?:[ \t]*>[ \t]*)*[ \t]*(?:\*[ \t]*){3,}$").unwrap();
    let block_start = cached_regex!(r"^[ \t]*(?:`{3,}|~{3,}|:{3,}|#{1,6}[ \t])").unwrap();
    let marker = cached_regex!(r"^[ \t]*(?:[-*+][ \t]|[0-9]+[.)][ \t]|\|)").unwrap();
    let mut previous_blank = true;
    let mut container = false;
    let mut list_column = None;
    let structural_prefix = cached_regex!(
        r"^(?:[ \t]*>)*[ \t]*(?:(?:[-*+]|[0-9]+[.)])[ \t]+(?:\[[ xX-]\][ \t]+)?)*[ \t]*$"
    )
    .unwrap();
    let item_prefix = cached_regex!(r"^[ \t]*(?:[-*+]|[0-9]+[.)])[ \t]+").unwrap();
    let mut line_start = 0;
    let mut i = 0;
    while i < bytes.len() {
        if i == line_start {
            let end = source[i..]
                .find('\n')
                .map_or(bytes.len(), |offset| i + offset);
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
            if line.trim_start().starts_with(['`', '~'])
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
        if ch != b'_' && ch != b'*' {
            i += 1;
            continue;
        }
        if ch == b'*' && structural_prefix.is_match(&source[line_start..i]) {
            let end = source[i..]
                .find('\n')
                .map_or(bytes.len(), |offset| i + offset);
            if stars.is_match(&source[line_start..end]) {
                for (at, byte) in bytes.iter().enumerate().take(end).skip(i) {
                    if *byte == b'*' {
                        structural.insert(at);
                    }
                }
                i = end;
                continue;
            }
            if bytes.get(i + 1).is_some_and(|byte| b" \t".contains(byte)) {
                structural.insert(i);
                i += 1;
                continue;
            }
        }
        let forced_open = i > 0 && bytes[i - 1] == b'{' && mask[i - 1] == b'{';
        let forced_close = bytes.get(i + 1) == Some(&b'}');
        let can_open = forced_open
            || (!forced_close
                && bytes
                    .get(i + 1)
                    .is_some_and(|byte| !b" \t\r\n".contains(byte)));
        let can_close =
            !forced_open && (forced_close || (i > 0 && !b" \t\r\n".contains(&bytes[i - 1])));
        let key = usize::from(ch == b'*') + usize::from(forced_close) * 2;
        if let Some((start, end, forced)) = openers[key].last().copied().filter(|opener| {
            can_close && opener.1 < i && braces.last().map_or(true, |at| opener.0 > *at)
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
            i += if forced_close { 2 } else { 1 };
        } else if can_open {
            let key = usize::from(ch == b'*') + usize::from(forced_open) * 2;
            openers[key].push((i - usize::from(forced_open), i + 1, forced_open));
            i += 1;
        } else {
            i += if forced_close { 2 } else { 1 };
        }
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
    let renderer = Renderer {
        source,
        mask,
        pairs: &pairs,
        structural: &structural,
        literal_brackets: &literal_brackets,
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
    pairs: &'a [Pair],
    structural: &'a HashSet<usize>,
    literal_brackets: &'a HashSet<usize>,
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
            if self.mask[i] == ch
                && ((b"_*".contains(&ch) && !self.structural.contains(&i))
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
