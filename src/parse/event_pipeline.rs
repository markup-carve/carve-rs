use super::*;

enum Event<'a> {
    Heading(u8, &'a str),
    Paragraph(&'a [&'a str]),
}

struct Events<'a> {
    lines: &'a [&'a str],
    next: usize,
}

impl<'a> Iterator for Events<'a> {
    type Item = Event<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        while self.next < self.lines.len() && is_blank_line(self.lines[self.next]) {
            self.next += 1;
        }
        let first = *self.lines.get(self.next)?;
        if let Some((level, text)) = detect_heading(first) {
            self.next += 1;
            return Some(Event::Heading(level, text));
        }
        let start = self.next;
        self.next += 1;
        while self.next < self.lines.len() {
            let line = self.lines[self.next];
            if is_blank_line(line) || detect_heading(line).is_some() {
                break;
            }
            self.next += 1;
        }
        Some(Event::Paragraph(&self.lines[start..self.next]))
    }
}

fn supported_line(line: &str) -> bool {
    if is_blank_line(line) {
        return true;
    }
    if line.starts_with('#') {
        return detect_heading(line).is_some();
    }
    line.as_bytes().first().is_some_and(u8::is_ascii_alphabetic) && !is_list_marker(line)
}

/// Materialize the public nodes from borrowed line events. A complete lexical
/// gate runs first, so an unsupported line never leaves a partly built tree.
pub(super) fn parse_blocks(source: &str, options: &Options<'_>) -> Option<Vec<BlockNode>> {
    if options.positions || options.source_lines || !options.extensions.is_empty() {
        return None;
    }
    let lines: Vec<&str> = source.lines().collect();
    if !lines.iter().copied().all(supported_line) {
        return None;
    }
    let mut out = Vec::new();
    for event in (Events {
        lines: &lines,
        next: 0,
    }) {
        match event {
            Event::Heading(level, text) => out.push(BlockNode::Heading(Heading {
                attrs: None,
                level,
                children: parse_inline_with_options(trim_ascii_end(text), options),
                pos: None,
            })),
            Event::Paragraph(lines) => {
                let joined = lines
                    .iter()
                    .map(|line| trim_ascii_end(trim_ascii_start(line)))
                    .collect::<Vec<_>>()
                    .join("\n");
                out.push(BlockNode::Paragraph(Paragraph {
                    attrs: None,
                    children: parse_inline_with_options(&joined, options),
                    at_content_column: true,
                    block_image: false,
                    pos: None,
                }));
            }
        }
    }
    Some(out)
}

/// The admission gate excludes syntax that needs definition lifting, source
/// normalization, positions, or a container. The standard parser handles it.
pub(super) fn parse_document(source: &str) -> Option<Document> {
    if source.starts_with("---")
        || source.contains(['\r', '\0', '\u{feff}', '\u{e005}', '\u{e006}'])
        || source.contains("[^")
        || source.contains("![")
        || source.contains("</#")
        || source.contains("][")
        || source.contains("*[")
        || source.contains('^')
    {
        return None;
    }
    let options = Options::default();
    let children = parse_blocks(source, &options)?;
    let mut doc = Document {
        frontmatter: BTreeMap::new(),
        frontmatter_raw: None,
        footnote_defs: BTreeMap::new(),
        footnote_def_pos: BTreeMap::new(),
        children,
        source_len: source.len(),
        ingest_payload_len: 0,
    };
    stamp_generated_heading_ids(&mut doc, options.heading_id_options());
    coalesce_text_runs(&mut doc);
    fill_crossref_hrefs(&mut doc, options.heading_id_options());
    Some(doc)
}
