use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

type DivHost = (usize, usize, usize, Option<usize>, usize, Option<usize>);

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
    host_depth: usize,
}

fn kind_bit(kind: u8) -> u8 {
    match kind {
        b'_' => 1,
        b'*' => 2,
        b'~' => 4,
        b'^' => 8,
        b'=' => 16,
        b'\'' => 32,
        b'"' => 64,
        _ => unreachable!(),
    }
}
fn clear(openers: &mut [Vec<(usize, usize, bool)>; 12], from: usize) {
    for stack in openers {
        while stack.last().is_some_and(|opener| opener.0 >= from) {
            stack.pop();
        }
    }
}

pub(super) fn convert(source: &str, mask: &str, convert_plain: impl Fn(&str) -> String) -> String {
    process(source, mask, convert_plain, None, &mut Vec::new(), false)
}

pub(super) fn convert_image_child(
    source: &str,
    mask: &str,
    convert_plain: impl Fn(&str) -> String,
) -> String {
    process(source, mask, convert_plain, None, &mut Vec::new(), true)
}

pub(super) fn paired_openers(source: &str, mask: &str) -> HashMap<usize, usize> {
    let mut paired = HashMap::new();
    process(
        source,
        mask,
        str::to_string,
        Some(&mut paired),
        &mut Vec::new(),
        false,
    );
    paired
}

pub(super) fn convert_with_losses(
    source: &str,
    mask: &str,
    convert_plain: impl Fn(&str) -> String,
    losses: &mut Vec<usize>,
) -> String {
    process(source, mask, convert_plain, None, losses, false)
}

fn process(
    source: &str,
    mask: &str,
    convert_plain: impl Fn(&str) -> String,
    mut paired: Option<&mut HashMap<usize, usize>>,
    losses: &mut Vec<usize>,
    omit_quote_delimiters: bool,
) -> String {
    let bytes = source.as_bytes();
    let mut mask = mask.as_bytes().to_vec();
    let cells = table_cells(source);
    let code_mask = super::mask_code_and_destinations(source);
    let block_attributes = scan_block_attributes(source, &code_mask, &mut mask);
    let mut brace_scan = scan_braces(source, &mask, &cells, paired.as_deref_mut());
    let Delimiters {
        mut pairs,
        structural,
        omitted_quote_braces,
        bracket_pairs,
    } = DelimiterScan::new(
        source,
        &mask,
        &cells,
        omit_quote_delimiters,
        &mut brace_scan,
        paired.as_deref_mut(),
    )
    .run();
    if let Some(paired) = paired {
        for pair in pairs {
            paired.insert(pair.open_end - 1, pair.end);
        }
        return String::new();
    }
    let roots = build_pair_tree(bytes, &mut pairs, &brace_scan);
    let starts: HashMap<_, _> = pairs
        .iter()
        .enumerate()
        .map(|(index, pair)| (pair.start, index))
        .collect();
    let bracket_closes = bracket_pairs.iter().map(|(_, close)| *close).collect();
    let literal_brackets =
        find_literal_brackets(source, &mask, &pairs, &starts, &brace_scan, &bracket_pairs);
    let host_changes = host_changes(
        bytes,
        &starts,
        &brace_scan,
        &bracket_pairs,
        &literal_brackets,
    );
    assign_host_depths(source, &mask, &mut pairs, &starts, &host_changes);
    let BraceScan {
        valid_braces,
        valid_brace_closers,
        literal_dashes,
        ..
    } = brace_scan;
    let renderer = Renderer {
        source,
        mask: &mask,
        attributes: &block_attributes.attributes,
        empty_block_attributes: &block_attributes.empty_block_attributes,
        list_boundary_comments: &block_attributes.list_boundary_comments,
        bracket_closes,
        pairs: &pairs,
        structural: &structural,
        literal_brackets: &literal_brackets,
        valid_braces: &valid_braces,
        valid_brace_closers: &valid_brace_closers,
        literal_dashes: &literal_dashes,
        omitted_quote_braces: &omitted_quote_braces,
        omit_quote_delimiters,
        literal_prefix: super::djot_placeholder_prefix(source, "\0DJOTLITERAL\0"),
        literals: RefCell::new(Vec::new()),
        rendered: RefCell::new(HashMap::new()),
    };
    render_pairs(&renderer, &pairs, &roots, losses);
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

fn block_start_regex() -> &'static regex::Regex {
    cached_regex!(r"^[ \t]*(?:`{3,}|~{3,}|:{3,}|#{1,6}[ \t])").unwrap()
}

fn marker_regex() -> &'static regex::Regex {
    cached_regex!(r"^[ \t]*(?:[-*+][ \t]|[0-9]+[.)][ \t]|\|)").unwrap()
}

fn item_prefix_regex() -> &'static regex::Regex {
    cached_regex!(r"^[ \t]*(?:[-*+]|[0-9]+[.)])[ \t]+").unwrap()
}

fn table_cells(source: &str) -> HashSet<usize> {
    if !source.contains('|') {
        return HashSet::new();
    }
    let bytes = source.as_bytes();
    let cell_mask = super::mask_djot_forms_with_options(
        source,
        false,
        false,
        None,
        &[],
        super::OpaqueOptions {
            destinations: false,
            autolinks: false,
            attribute_values: false,
            ..super::OpaqueOptions::default()
        },
    );
    super::djot_inline_boundaries(source, &cell_mask)
        .into_iter()
        .filter(|&at| bytes.get(at) == Some(&b'|') && cell_mask.as_bytes().get(at) == Some(&b'|'))
        .collect()
}

#[derive(Default)]
struct BlockAttributes {
    attributes: HashMap<usize, (usize, String)>,
    empty_block_attributes: HashSet<usize>,
    list_boundary_comments: HashMap<usize, String>,
}

struct AttributeLine<'a> {
    source: &'a str,
    line_start: usize,
    line_end: usize,
    first_attribute_line: bool,
    previous_line: &'a str,
    prefix_end: usize,
    list_attribute: bool,
    previous_content: &'a str,
    previous_item_width: usize,
    attribute_indent: usize,
    line_prefix: &'a str,
    active_list_column: Option<usize>,
}

impl<'a> AttributeLine<'a> {
    fn new(source: &'a str) -> Self {
        Self {
            source,
            line_start: 0,
            line_end: 0,
            first_attribute_line: true,
            previous_line: "",
            prefix_end: 0,
            list_attribute: false,
            previous_content: "",
            previous_item_width: 0,
            attribute_indent: 0,
            line_prefix: "",
            active_list_column: None,
        }
    }

    fn advance_to(&mut self, at: usize) {
        let source = self.source;
        let quote_prefix = cached_regex!(r"^(?:[ \t]*>[ \t]?)*").unwrap();
        let previous_item = cached_regex!(r"^[ \t]*(?:[-*+]|(?:[0-9]+|[A-Za-z]|[ivxlcdm]+|[IVXLCDM]+)[.)]|\((?:[0-9]+|[A-Za-z]|[ivxlcdm]+|[IVXLCDM]+)\))[ \t]+").unwrap();
        let attribute_prefix =
            cached_regex!(r"^[ \t>]*(?:(?:[-*+]|(?:[0-9]+|[A-Za-z]|[ivxlcdm]+|[IVXLCDM]+)[.)]|\((?:[0-9]+|[A-Za-z]|[ivxlcdm]+|[IVXLCDM]+)\))[ \t]+)?").unwrap();
        while self.first_attribute_line || self.line_end < at {
            if !self.first_attribute_line {
                self.previous_line = &source[self.line_start..self.line_end];
                self.line_start = self.line_end + 1;
            }
            self.first_attribute_line = false;
            self.line_end = source[self.line_start..]
                .find('\n')
                .map_or(source.len(), |offset| self.line_start + offset);
            let prefix = attribute_prefix
                .find(&source[self.line_start..self.line_end])
                .unwrap()
                .as_str();
            self.prefix_end = self.line_start + prefix.len();
            self.list_attribute = prefix.bytes().any(|byte| b"-*+.)".contains(&byte));
            self.line_prefix = prefix;
            self.previous_content =
                &self.previous_line[quote_prefix.find(self.previous_line).unwrap().end()..];
            self.previous_item_width = previous_item
                .find(self.previous_content)
                .map_or(0, |value| value.len());
            self.attribute_indent = prefix.len() - quote_prefix.find(prefix).unwrap().end();
            self.list_attribute = self.list_attribute
                && (self.line_start == 0
                    || self.previous_content.trim().is_empty()
                    || self.active_list_column.is_some());
            if let Some(column) = self.active_list_column {
                if !source[self.line_start..self.line_end].trim().is_empty()
                    && self.attribute_indent < column
                    && !self.list_attribute
                {
                    self.previous_item_width = column;
                    self.active_list_column = None;
                }
            }
            if self.list_attribute {
                self.active_list_column = Some(self.attribute_indent);
            }
            if self.line_end == source.len() {
                break;
            }
        }
    }

    fn is_empty_block_attribute(
        &self,
        at: usize,
        end: usize,
        first_attribute_end: usize,
        wire: &str,
    ) -> bool {
        wire == "{}"
            && end == first_attribute_end
            && at == self.prefix_end
            && end <= self.line_end
            && self.source[end..self.line_end].trim().is_empty()
            && (self.list_attribute
                || self.line_start == 0
                || self.previous_content.trim().is_empty()
                || self.previous_content.trim().starts_with('{')
                    && self.previous_content.trim().ends_with('}')
                || self.attribute_indent < self.previous_item_width)
    }
}

fn scan_block_attributes(source: &str, code_mask: &str, mask: &mut [u8]) -> BlockAttributes {
    let bytes = source.as_bytes();
    let mut found = BlockAttributes::default();
    let mut line = AttributeLine::new(source);
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
                line.advance_to(at);
                if at == line.prefix_end && end != first_attribute_end && end <= line.line_end {
                    wire = "{%%}".to_owned();
                }
                if line.is_empty_block_attribute(at, end, first_attribute_end, &wire) {
                    found.empty_block_attributes.insert(at);
                    if line.attribute_indent < line.previous_item_width {
                        found
                            .list_boundary_comments
                            .insert(at, format!("%%%\n{}%%%", line.line_prefix));
                    }
                }
                for byte in &mut mask[at..end] {
                    if *byte != b'\n' {
                        *byte = b' ';
                    }
                }
                found.attributes.insert(at, (end, wire));
                at = end;
                continue;
            }
        }
        at += 1;
    }
    found
}

#[derive(Default)]
struct BraceScan {
    valid_braces: HashSet<usize>,
    valid_brace_closers: HashSet<usize>,
    brace_ends: HashMap<usize, usize>,
    literal_dashes: HashSet<usize>,
}

impl BraceScan {
    fn close(
        &mut self,
        bytes: &[u8],
        mask: &[u8],
        at: usize,
        pending_braces: &mut HashMap<u8, Vec<usize>>,
        paired: Option<&mut HashMap<usize, usize>>,
    ) {
        if let Some(start) = pending_braces
            .get_mut(&bytes[at - 1])
            .and_then(|stack| stack.pop())
        {
            if at > start + 2 {
                self.valid_braces.insert(start);
                self.valid_brace_closers.insert(at - 1);
                self.brace_ends.insert(start, at - 1);
                if let Some(paired) = paired.filter(|_| b"+-=".contains(&bytes[start + 1])) {
                    paired.insert(start + 1, at + 1);
                }
                for stack in pending_braces.values_mut() {
                    while stack.last().is_some_and(|at| *at > start) {
                        stack.pop();
                    }
                }
            }
        } else if bytes[at - 1] == b'-' {
            let mut first = at - 1;
            while first > 0 && bytes[first - 1] == b'-' {
                first -= 1;
            }
            if at - first == 2 && mask[first] == b'-' && mask[first + 1] == b'-' {
                self.literal_dashes.insert(first);
                self.literal_dashes.insert(first + 1);
            } else if at - first > 2 {
                self.literal_dashes.insert(at - 1);
            }
        }
    }
}

fn scan_braces(
    source: &str,
    mask: &[u8],
    cells: &HashSet<usize>,
    mut paired: Option<&mut HashMap<usize, usize>>,
) -> BraceScan {
    let bytes = source.as_bytes();
    let mut found = BraceScan::default();
    let mut pending_braces: HashMap<u8, Vec<usize>> = HashMap::new();
    let mut brace_line_start = 0;
    let mut last_escaped = None;
    let mut at = 0;
    while at < bytes.len() {
        if cells.contains(&at) {
            pending_braces.clear();
        }
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
                found.close(bytes, mask, at, &mut pending_braces, paired.as_deref_mut());
            }
        }
        at += 1;
    }
    found
}

struct Delimiters {
    pairs: Vec<Pair>,
    structural: HashSet<usize>,
    omitted_quote_braces: HashSet<usize>,
    bracket_pairs: Vec<(usize, usize)>,
}

struct DelimiterScan<'a> {
    source: &'a str,
    bytes: &'a [u8],
    mask: &'a [u8],
    cells: &'a HashSet<usize>,
    omit_quote_delimiters: bool,
    brace_scan: &'a mut BraceScan,
    paired: Option<&'a mut HashMap<usize, usize>>,
    found: Delimiters,
    openers: [Vec<(usize, usize, bool)>; 12],
    brackets: Vec<usize>,
    braces: Vec<usize>,
    previous_blank: bool,
    container: bool,
    list_column: Option<usize>,
    line_start: usize,
    line_end: usize,
    structural_end: usize,
    thematic_line: bool,
    // A Djot table caption (`^ text` under a table, blank line allowed) carries
    // the same marker Carve uses, so its `^` must reach the output bare. Only a
    // line in the table's own marker column and quote depth can hold one.
    table_column: Option<usize>,
    table_quote_depth: Option<usize>,
    caption_line: bool,
}

impl<'a> DelimiterScan<'a> {
    fn new(
        source: &'a str,
        mask: &'a [u8],
        cells: &'a HashSet<usize>,
        omit_quote_delimiters: bool,
        brace_scan: &'a mut BraceScan,
        paired: Option<&'a mut HashMap<usize, usize>>,
    ) -> Self {
        let bytes = source.as_bytes();
        Self {
            source,
            bytes,
            mask,
            cells,
            omit_quote_delimiters,
            brace_scan,
            paired,
            found: Delimiters {
                pairs: Vec::new(),
                structural: HashSet::new(),
                omitted_quote_braces: HashSet::new(),
                bracket_pairs: Vec::new(),
            },
            openers: std::array::from_fn(|_| Vec::new()),
            brackets: Vec::new(),
            braces: Vec::new(),
            previous_blank: true,
            container: false,
            list_column: None,
            line_start: 0,
            line_end: bytes.len(),
            structural_end: 0,
            thematic_line: false,
            table_column: None,
            table_quote_depth: None,
            caption_line: false,
        }
    }

    fn run(mut self) -> Delimiters {
        let mut i = 0;
        while i < self.bytes.len() {
            i = self.step(i);
        }
        self.found
    }

    fn step(&mut self, i: usize) -> usize {
        let bytes = self.bytes;
        if self.cells.contains(&i) {
            clear(&mut self.openers, 0);
            self.brackets.clear();
            self.braces.clear();
        }
        if i == self.line_start {
            self.begin_line(i);
        }
        let ch = bytes[i];
        if ch == b'\n' {
            self.end_line(i);
            return i + 1;
        }
        if ch == b'\\' && bytes.get(i + 1) != Some(&b'\n') {
            return i + 2;
        }
        if self.mask[i] != ch {
            return i + 1;
        }
        if let Some(next) = self.bracket_or_brace(i, ch) {
            return next;
        }
        if !b"_*~^".contains(&ch) && !(self.omit_quote_delimiters && b"\"'".contains(&ch)) {
            return i + 1;
        }
        if let Some(next) = self.structural_marker(i, ch) {
            return next;
        }
        self.delimiter(i, ch)
    }

    fn begin_line(&mut self, i: usize) {
        let source = self.source;
        let bytes = self.bytes;
        let quoted_prefix = cached_regex!(r"^(?:[ \t]*>[ \t]*)*").unwrap();
        let stars = cached_regex!(r"^(?:[ \t]*>)*[ \t]*(?:\*[ \t]*){3,}$").unwrap();
        let block_start = block_start_regex();
        let marker = marker_regex();
        let item_prefix = item_prefix_regex();
        let end = source[i..]
            .find('\n')
            .map_or(bytes.len(), |offset| i + offset);
        self.line_end = end;
        self.structural_end = i + structural_prefix_end(&source[i..end]);
        let prefix = &source[i..self.structural_end];
        let quote_depth = prefix.bytes().filter(|byte| *byte == b'>').count();
        self.caption_line = self
            .table_column
            .is_some_and(|column| prefix.len() >= column)
            && self.table_quote_depth == Some(quote_depth);
        // A blank line leaves the table in scope, which is how a caption
        // separated from its table by one keeps binding to it.
        if !source[self.structural_end..end].trim().is_empty() {
            let table_line = bytes.get(self.structural_end) == Some(&b'|')
                && self.mask.get(self.structural_end) == Some(&b'|');
            self.table_column = table_line.then_some(prefix.len());
            self.table_quote_depth = table_line.then_some(quote_depth);
        }
        self.thematic_line = stars.is_match(&source[i..end]);
        let line = quoted_prefix.replace(&source[i..end], "");
        let indent = line.len() - line.trim_start_matches([' ', '\t']).len();
        if !line.trim().is_empty()
            && self.list_column.is_some_and(|column| indent < column)
            && !item_prefix.is_match(&line)
        {
            self.list_column = None;
        }
        if marker.is_match(&line) && (self.previous_blank || self.container) {
            clear(&mut self.openers, 0);
            self.container = true;
        } else if self.previous_blank {
            self.container = self.list_column.is_some_and(|column| indent >= column);
        }
        if marker.is_match(&line) && self.container {
            if let Some(item) = item_prefix.find(&line) {
                self.list_column = Some(item.end());
            }
        }
        let was_boundary = self.previous_blank || self.container;
        self.previous_blank = line.trim().is_empty()
            || block_start.is_match(&line)
            || line.trim_start().starts_with('{');
        if block_start.is_match(&line) && line.trim_start().starts_with(['`', '~'])
            || line.trim_start().starts_with('#')
            || was_boundary && line.trim_start().starts_with(":::")
        {
            clear(&mut self.openers, 0);
        }
    }

    fn end_line(&mut self, i: usize) {
        let quoted_prefix = cached_regex!(r"^(?:[ \t]*>[ \t]*)*").unwrap();
        if quoted_prefix
            .replace(&self.source[self.line_start..i], "")
            .trim()
            .is_empty()
        {
            clear(&mut self.openers, 0);
            self.brackets.clear();
            self.braces.clear();
        }
        self.line_start = i + 1;
    }

    fn bracket_or_brace(&mut self, i: usize, ch: u8) -> Option<usize> {
        let bytes = self.bytes;
        if ch == b'{'
            && self.brace_scan.valid_braces.contains(&i)
            && !bytes.get(i + 1).is_some_and(|ch| b"_*".contains(ch))
        {
            self.braces.push(i);
            return Some(i + 1);
        }
        if ch == b'}'
            && i > 0
            && !super::is_escaped(bytes, i - 1)
            && self
                .braces
                .last()
                .is_some_and(|at| bytes[i - 1] == bytes[at + 1])
        {
            clear(&mut self.openers, self.braces.pop().unwrap());
            return Some(i + 1);
        }
        if ch == b'[' {
            self.brackets.push(i);
            return Some(i + 1);
        }
        if ch == b']' {
            if let Some(start) = self.brackets.pop() {
                clear(&mut self.openers, start);
                self.found.bracket_pairs.push((start, i));
            }
            return Some(i + 1);
        }
        None
    }

    fn structural_marker(&mut self, i: usize, ch: u8) -> Option<usize> {
        let bytes = self.bytes;
        if ch == b'^'
            && self.caption_line
            && i == self.structural_end
            && (i == self.line_start || b" \t".contains(&bytes[i - 1]))
            && bytes.get(i + 1).is_some_and(|byte| b" \t".contains(byte))
        {
            self.found.structural.insert(i);
            return Some(i + 1);
        }
        if ch == b'*' && i <= self.structural_end {
            if self.thematic_line {
                for (at, byte) in bytes.iter().enumerate().take(self.line_end).skip(i) {
                    if *byte == b'*' {
                        self.found.structural.insert(at);
                    }
                }
                return Some(self.line_end);
            }
            if bytes.get(i + 1).is_some_and(|byte| b" \t".contains(byte)) {
                self.found.structural.insert(i);
                return Some(i + 1);
            }
        }
        None
    }

    fn delimiter(&mut self, i: usize, ch: u8) -> usize {
        let bytes = self.bytes;
        let forced_open = i > 0
            && bytes[i - 1] == b'{'
            && self.mask[i - 1] == b'{'
            && !super::is_escaped(bytes, i - 1);
        let forced_close = bytes.get(i + 1) == Some(&b'}');
        if self.omit_quote_delimiters && matches!(ch, b'"' | b'\'') {
            if forced_open {
                self.found.omitted_quote_braces.insert(i - 1);
            }
            if forced_close && !forced_open {
                self.found.omitted_quote_braces.insert(i + 1);
            }
        }
        let single_quote_opens = ch != b'\'' || i == 0 || b" \t\r\n\"'-([".contains(&bytes[i - 1]);
        let can_open = forced_open
            || (!forced_close
                && bytes
                    .get(i + 1)
                    .is_some_and(|byte| !b" \t\r\n".contains(byte))
                && single_quote_opens);
        let can_close =
            !forced_open && (forced_close || (i > 0 && !b" \t\r\n".contains(&bytes[i - 1])));
        let marker = match ch {
            b'_' => 0,
            b'*' => 1,
            b'~' => 2,
            b'^' => 3,
            b'\'' => 4,
            b'"' => 5,
            _ => unreachable!(),
        };
        let key = marker + usize::from(forced_close) * 6;
        let braces = &self.braces;
        if let Some((start, end, forced)) = self.openers[key].last().copied().filter(|opener| {
            can_close
                && opener.1 < i
                && (!opener.2 || braces.last().map_or(true, |at| opener.0 >= *at))
        }) {
            self.close_pair(i, ch, (start, end, forced), forced_close)
        } else if can_open {
            let key = marker + usize::from(forced_open) * 6;
            self.openers[key].push((i - usize::from(forced_open), i + 1, forced_open));
            i + 1
        } else {
            i + if forced_close { 2 } else { 1 }
        }
    }

    fn close_pair(
        &mut self,
        i: usize,
        ch: u8,
        (start, end, forced): (usize, usize, bool),
        forced_close: bool,
    ) -> usize {
        clear(&mut self.openers, start);
        while self.braces.last().is_some_and(|at| *at > start) {
            let at = self.braces.pop().unwrap();
            self.brace_scan.valid_braces.remove(&at);
            let close = self.brace_scan.brace_ends[&at];
            self.brace_scan.valid_brace_closers.remove(&close);
            if let Some(paired) = self.paired.as_deref_mut() {
                paired.remove(&(at + 1));
            }
        }
        self.found.pairs.push(Pair {
            start,
            open_end: end,
            close: i,
            end: i + if forced_close { 2 } else { 1 },
            kind: ch,
            forced,
            children: Vec::new(),
            kinds: kind_bit(ch),
            host_depth: 0,
        });
        if forced_close && self.braces.last() == Some(&start) {
            self.braces.pop();
        }
        i + if forced_close { 2 } else { 1 }
    }
}

fn build_pair_tree(bytes: &[u8], pairs: &mut Vec<Pair>, brace_scan: &BraceScan) -> Vec<usize> {
    for &start in &brace_scan.valid_braces {
        if bytes[start + 1] != b'=' {
            continue;
        }
        let close = brace_scan.brace_ends[&start];
        pairs.push(Pair {
            start,
            open_end: start + 2,
            close,
            end: close + 2,
            kind: b'=',
            forced: true,
            children: Vec::new(),
            kinds: kind_bit(b'='),
            host_depth: 0,
        });
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
    roots
}

fn find_literal_brackets(
    source: &str,
    mask: &[u8],
    pairs: &[Pair],
    starts: &HashMap<usize, usize>,
    brace_scan: &BraceScan,
    bracket_pairs: &[(usize, usize)],
) -> HashSet<usize> {
    let bytes = source.as_bytes();
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
    for &(start, end) in bracket_pairs {
        if contexts.get(&start) != contexts.get(&end) {
            literal_brackets.insert(start);
            literal_brackets.insert(end);
        }
    }
    let raw_attribute = cached_regex!(r"^\{=[^\s{}`]+\}").unwrap();
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
            && !brace_scan.valid_braces.contains(&at)
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
    literal_brackets
}

fn host_changes(
    bytes: &[u8],
    starts: &HashMap<usize, usize>,
    brace_scan: &BraceScan,
    bracket_pairs: &[(usize, usize)],
    literal_brackets: &HashSet<usize>,
) -> HashMap<usize, isize> {
    let mut host_changes: HashMap<usize, isize> = HashMap::new();
    for &start in &brace_scan.valid_braces {
        if starts.contains_key(&start) {
            continue;
        }
        let end = brace_scan.brace_ends[&start];
        *host_changes.entry(start + 2).or_default() += 1;
        *host_changes.entry(end).or_default() -= 1;
    }
    for &(start, end) in bracket_pairs {
        if literal_brackets.contains(&start) || literal_brackets.contains(&end) {
            continue;
        }
        if !bytes
            .get(end + 1)
            .is_some_and(|ch| b"([{ ".contains(ch) && *ch != b' ')
            && start.checked_sub(1).and_then(|at| bytes.get(at)) != Some(&b'^')
        {
            continue;
        }
        *host_changes.entry(start + 1).or_default() += 1;
        *host_changes.entry(end).or_default() -= 1;
    }
    host_changes
}

struct LinePrefix {
    cursor: usize,
    physical_depth: usize,
    physical_quote_column: Option<usize>,
    physical_quotes: usize,
    pending_lists: usize,
    literal_list_marker: bool,
}

#[derive(Default)]
struct BlockHosts {
    quote_depth: usize,
    quote_paragraph_open: bool,
    quote_column: Option<usize>,
    list_columns: Vec<(usize, usize, usize, usize)>,
    quote_count: usize,
    paragraph_open: bool,
    paragraph_list_id: Option<usize>,
    div_fences: Vec<DivHost>,
    div_depth: usize,
    block_host_depth: usize,
}

impl BlockHosts {
    fn line(&mut self, source: &str, mask: &[u8], at: usize) {
        let bytes = source.as_bytes();
        let end = source[at..]
            .find('\n')
            .map_or(bytes.len(), |offset| at + offset);
        let prefix = self.scan_prefix(source, at, end);
        let cursor = prefix.cursor;
        let content = &source[cursor..end];
        let paragraph = !content.trim().is_empty()
            && !block_start_regex().is_match(content)
            && (prefix.literal_list_marker || !marker_regex().is_match(content));
        self.update_quotes(&prefix, paragraph);
        if !content.trim().is_empty() {
            self.close_lists(cursor - at, paragraph);
        }
        if !content.trim().is_empty() {
            self.close_divs(cursor - at, paragraph);
        }
        self.div_fence(content, mask, cursor);
        self.paragraph_open = paragraph;
        self.paragraph_list_id = self.list_columns.last().map(|entry| entry.2);
        let quoted_lists = self
            .list_columns
            .partition_point(|(_, scope, _, _)| *scope < self.quote_count);
        let div_lists = self.div_fences.last().map_or(0, |fence| fence.4);
        self.block_host_depth = self.quote_depth + self.div_depth + quoted_lists.max(div_lists);
    }

    fn scan_prefix(&mut self, source: &str, at: usize, end: usize) -> LinePrefix {
        let bytes = source.as_bytes();
        let item_prefix = item_prefix_regex();
        let mut physical_depth = 0;
        let mut physical_quote_column = None;
        let mut physical_quotes = 0;
        let mut pending_lists = 0;
        let mut literal_list_marker = false;
        let mut cursor = at;
        loop {
            while cursor < end && matches!(bytes[cursor], b' ' | b'\t') {
                cursor += 1;
            }
            if bytes.get(cursor) == Some(&b'>')
                && (cursor + 1 == end
                    || bytes
                        .get(cursor + 1)
                        .is_some_and(|byte| b" \t".contains(byte)))
            {
                loop {
                    let prefix = self
                        .list_columns
                        .partition_point(|(_, scope, _, _)| *scope <= physical_quotes);
                    if prefix == 0 || self.list_columns[prefix - 1].0 <= cursor - at {
                        break;
                    }
                    self.list_columns.truncate(prefix - 1);
                }
                if self.paragraph_open
                    && physical_quotes >= self.quote_count
                    && self.list_columns.last().map(|entry| entry.2) == self.paragraph_list_id
                {
                    break;
                }
                physical_quote_column.get_or_insert(cursor - at);
                physical_depth += 1;
                physical_quotes += 1;
                pending_lists = 0;
                cursor += 1;
            } else if let Some(item) = item_prefix.find(&source[cursor..end]) {
                if pending_lists == 0
                    && self.paragraph_open
                    && physical_quotes == self.quote_count
                    && self
                        .list_columns
                        .last()
                        .map_or(true, |(_, _, _, marker_column)| {
                            cursor - at > *marker_column
                        })
                {
                    literal_list_marker = true;
                    break;
                }
                while self
                    .list_columns
                    .last()
                    .is_some_and(|(column, scope, _, _)| {
                        *scope > physical_quotes || *column > cursor - at
                    })
                {
                    self.list_columns.pop();
                }
                pending_lists += 1;
                let marker_column = cursor - at;
                let marker_at = cursor;
                cursor += item.end();
                self.list_columns
                    .push((cursor - at, physical_quotes, marker_at, marker_column));
                if bytes.get(cursor) == Some(&b'[')
                    && bytes
                        .get(cursor + 1)
                        .is_some_and(|byte| b" xX-".contains(byte))
                    && bytes.get(cursor + 2) == Some(&b']')
                    && bytes
                        .get(cursor + 3)
                        .is_some_and(|byte| b" \t".contains(byte))
                {
                    cursor += 3;
                }
            } else {
                break;
            }
        }
        LinePrefix {
            cursor,
            physical_depth,
            physical_quote_column,
            physical_quotes,
            pending_lists,
            literal_list_marker,
        }
    }

    fn update_quotes(&mut self, prefix: &LinePrefix, paragraph: bool) {
        if prefix.physical_depth > 0 {
            let continues = self.quote_paragraph_open
                && paragraph
                && self.quote_column == prefix.physical_quote_column;
            self.quote_depth = if continues {
                self.quote_depth.max(prefix.physical_depth)
            } else {
                prefix.physical_depth
            };
            self.quote_count = if continues {
                self.quote_count.max(prefix.physical_quotes)
            } else {
                prefix.physical_quotes
            };
            self.quote_column = prefix.physical_quote_column;
            self.quote_paragraph_open = paragraph;
        } else if prefix.pending_lists > 0 || !self.quote_paragraph_open || !paragraph {
            self.quote_depth = 0;
            self.quote_column = None;
            self.quote_count = 0;
            self.quote_paragraph_open = false;
        }
    }

    fn close_lists(&mut self, column_at: usize, paragraph: bool) {
        let quote_count = self.quote_count;
        let paragraph_open = self.paragraph_open;
        while self
            .list_columns
            .last()
            .is_some_and(|(column, scope, _, _)| {
                *scope > quote_count || (*column > column_at && !(paragraph_open && paragraph))
            })
        {
            self.list_columns.pop();
        }
    }

    fn close_divs(&mut self, column_at: usize, paragraph: bool) {
        while self
            .div_fences
            .last()
            .is_some_and(|(_, _, quote, column, count, id)| {
                self.quote_depth < *quote
                    || (*count > 0 && self.list_columns.get(count - 1).map(|entry| entry.2) != *id)
                    || (column.is_some_and(|column| column_at < column)
                        && !(self.paragraph_open && paragraph))
            })
        {
            let (_, depth, _, _, _, _) = self.div_fences.pop().unwrap();
            self.div_depth -= depth;
        }
    }

    fn div_fence(&mut self, content: &str, mask: &[u8], cursor: usize) {
        let colon_width = content.bytes().take_while(|byte| *byte == b':').count();
        if colon_width >= 3 && mask.get(cursor) == Some(&b':') {
            let tail = content[colon_width..].trim();
            if tail.is_empty()
                && self
                    .div_fences
                    .last()
                    .is_some_and(|(width, _, _, _, _, _)| colon_width >= *width)
            {
                let (_, depth, _, _, _, _) = self.div_fences.pop().unwrap();
                self.div_depth -= depth;
            } else {
                let depth = 1;
                self.div_fences.push((
                    colon_width,
                    depth,
                    self.quote_depth,
                    self.list_columns.last().map(|(column, _, _, _)| *column),
                    self.list_columns.len(),
                    self.list_columns.last().map(|entry| entry.2),
                ));
                self.div_depth += depth;
            }
        }
    }
}

fn assign_host_depths(
    source: &str,
    mask: &[u8],
    pairs: &mut [Pair],
    starts: &HashMap<usize, usize>,
    host_changes: &HashMap<usize, isize>,
) {
    let bytes = source.as_bytes();
    let mut host_depth = 0isize;
    let mut blocks = BlockHosts::default();
    for at in 0..bytes.len() {
        if at == 0 || bytes[at - 1] == b'\n' {
            blocks.line(source, mask, at);
        }
        host_depth += host_changes.get(&at).copied().unwrap_or(0);
        if let Some(&index) = starts.get(&at) {
            pairs[index].host_depth = host_depth.max(0) as usize + blocks.block_host_depth;
        }
    }
}

fn render_pairs(renderer: &Renderer<'_>, pairs: &[Pair], roots: &[usize], losses: &mut Vec<usize>) {
    let mut work: Vec<_> = roots
        .iter()
        .rev()
        .map(|index| (*index, 0, 0, false))
        .collect();
    while let Some((index, outer, depth, ready)) = work.pop() {
        let pair = &pairs[index];
        if ready {
            let rendered = renderer.render(index, outer, depth);
            renderer.rendered.borrow_mut().insert(index, rendered);
            continue;
        }
        work.push((index, outer, depth, true));
        let bit = kind_bit(pair.kind);
        if depth + pair.host_depth >= crate::parse::MAX_NESTING_DEPTH - 2 {
            losses.push(pair.start);
        }
        let inner = outer | bit;
        for child in pair.children.iter().rev() {
            work.push((*child, inner, depth + 1, false));
        }
    }
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
    literal_dashes: &'a HashSet<usize>,
    omitted_quote_braces: &'a HashSet<usize>,
    omit_quote_delimiters: bool,
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
            if self.omitted_quote_braces.contains(&i) {
                i += 1;
                continue;
            }
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
                && (b"~^_*".contains(&ch) && !self.structural.contains(&i)
                    || self.literal_brackets.contains(&i))
            {
                out.extend_from_slice(self.protect(&format!("\\{}", char::from(ch))).as_bytes());
                i += 1;
                continue;
            }
            if self.literal_dashes.contains(&i) {
                out.push(b'\\');
            }
            out.push(ch);
            i += 1;
            // Carve opens a caption on `^ `, never on `^\t`, so a tab-separated
            // Djot caption needs the space the marker requires.
            if ch == b'^' && self.structural.contains(&(i - 1)) && bytes.get(i) == Some(&b'\t') {
                out.push(b' ');
                i += 1;
            }
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
    fn render(&self, index: usize, outer: u8, depth: usize) -> String {
        let pair = &self.pairs[index];
        let bit = kind_bit(pair.kind);
        if depth + pair.host_depth >= crate::parse::MAX_NESTING_DEPTH - 2 {
            return self.body(pair.open_end, pair.close, &pair.children, outer);
        }
        let scope = outer & bit != 0
            || pair
                .children
                .iter()
                .any(|child| self.pairs[*child].kinds & bit != 0);
        let content = self.body(pair.open_end, pair.close, &pair.children, outer | bit);
        if matches!(pair.kind, b'"' | b'\'') {
            return content;
        }
        let delimiter = match pair.kind {
            b'_' => '/',
            b'~' => ',',
            b'^' => '^',
            b'=' => '=',
            _ => '*',
        };
        let bytes = self.source.as_bytes();
        let word = |byte: u8| byte.is_ascii_alphanumeric() || byte == b'_';
        let forced = self.omit_quote_delimiters
            || b"~^".contains(&pair.kind)
            || pair.forced
            || scope
            || content.starts_with('\0')
            || content.ends_with('\0')
            || (pair.start > 0 && word(bytes[pair.start - 1]))
            || bytes.get(pair.end).is_some_and(|byte| word(*byte))
            || content.starts_with([' ', '\t', '\r', '\n'])
            || content.ends_with([' ', '\t', '\r', '\n'])
            || content.starts_with(delimiter)
            || content.ends_with(delimiter)
            || (delimiter == '/'
                && (pair.start.checked_sub(1).and_then(|at| bytes.get(at)) == Some(&b'/')
                    || bytes.get(pair.end) == Some(&b'/')))
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
