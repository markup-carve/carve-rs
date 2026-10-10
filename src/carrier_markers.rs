//! The carrier marker PART 11 §10s defines: an HTML comment holding the Carve
//! opener or closer of a container the Markdown target writes as its children
//! alone (CARVE-P11-063).
//!
//! The payload is Carve source, so Carve's own escape is the one the reader
//! already has and only the comment's own terminator is rewritten.

use crate::ast::{BlockNode, Document};

pub(crate) const CARRIER_PREFIX: &str = "<!-- carve: ";
pub(crate) const CARRIER_SUFFIX: &str = " -->";

/// A `-->` the payload carries becomes `--\>`; a backslash run already sitting
/// where that escape would put one grows by one, so the transform reverses
/// exactly.
pub(crate) fn escape_carrier_payload(payload: &str) -> String {
    rewrite_terminator(payload, false)
}

pub(crate) fn unescape_carrier_payload(payload: &str) -> String {
    rewrite_terminator(payload, true)
}

/// Walk `--` followed by a backslash run and a `>`, adding one backslash to the
/// run (`escaped`) or removing one (`unescape`).
fn rewrite_terminator(payload: &str, unescape: bool) -> String {
    let bytes = payload.as_bytes();
    let mut out = String::with_capacity(payload.len());
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] != b'-' || bytes.get(at + 1) != Some(&b'-') {
            out.push(bytes[at] as char);
            at += 1;
            continue;
        }
        let mut end = at + 2;
        while bytes.get(end) == Some(&b'\\') {
            end += 1;
        }
        let slashes = end - (at + 2);
        if bytes.get(end) != Some(&b'>') || (unescape && slashes == 0) {
            out.push_str("--");
            at += 2;
            continue;
        }
        out.push_str("--");
        let kept = if unescape { slashes - 1 } else { slashes + 1 };
        for _ in 0..kept {
            out.push('\\');
        }
        out.push('>');
        at = end + 1;
    }

    out
}

/// Write a payload into a marker line.
pub(crate) fn carrier_line(payload: &str) -> String {
    format!(
        "{CARRIER_PREFIX}{}{CARRIER_SUFFIX}",
        escape_carrier_payload(payload)
    )
}

/// The payload a marker line carries, or `None` when the line is not one.
pub(crate) fn carrier_payload(line: &str) -> Option<String> {
    let inner = line
        .strip_prefix(CARRIER_PREFIX)?
        .strip_suffix(CARRIER_SUFFIX)?;
    // An unescaped `-->` inside would have ended the comment at that point, so
    // the line this engine is reading is not one this engine wrote.
    if inner.contains("-->") {
        return None;
    }

    Some(unescape_carrier_payload(inner))
}

/// The colon-fence width a payload opens or closes with, or 0 when the payload
/// is neither (an attribute line travelling with its opener).
pub(crate) fn carrier_fence_width(payload: &str) -> usize {
    let colons = payload.bytes().take_while(|byte| *byte == b':').count();
    if colons >= 3 {
        colons
    } else {
        0
    }
}

/// Whether a colon-fence payload is a bare closer.
pub(crate) fn carrier_is_closer(payload: &str) -> bool {
    carrier_fence_width(payload) == payload.len()
}

/// The Carve lines a container's carrier markers carry.
pub(crate) struct CarrierMarkers {
    /// The attribute line travelling above the opener, if the container has one.
    pub(crate) prelude: Vec<String>,
    pub(crate) opener: String,
    pub(crate) closer: String,
}

/// The markers a container takes, or `None` when the node has no colon-fence
/// spelling.
///
/// The payload is spelled by the CANONICAL WRITER rather than composed a second
/// time here: it is Carve source, and a second spelling would drift from the one
/// `carve fmt` writes. The body is dropped before the render, so a nested
/// container costs one fence line instead of a second render of its subtree, and
/// the fence runs are then widened to the depth the Markdown writer is at.
pub(crate) fn spell_carrier_markers(node: &BlockNode, depth: usize) -> Option<CarrierMarkers> {
    let bodyless = without_children(node)?;
    let document = Document {
        frontmatter: Default::default(),
        frontmatter_raw: None,
        footnote_defs: Default::default(),
        footnote_def_pos: Default::default(),
        children: vec![bodyless],
        source_len: 0,
        ingest_payload_len: 0,
    };
    let rendered = crate::render_carve::render_carve(&document).ok()?;
    let lines: Vec<&str> = rendered.split('\n').collect();
    let mut open = None;
    let mut close = None;
    for (at, line) in lines.iter().enumerate() {
        if carrier_fence_width(line) == 0 {
            continue;
        }
        if open.is_none() {
            open = Some(at);
        }
        if carrier_is_closer(line) {
            close = Some(at);
        }
    }
    let (open, close) = (open?, close?);
    if close == open {
        return None;
    }
    let widen = |line: &str| {
        let width = carrier_fence_width(line);
        format!("{}{}", ":".repeat(3 + depth), &line[width..])
    };

    Some(CarrierMarkers {
        prelude: lines[..open].iter().map(|l| (*l).to_owned()).collect(),
        opener: widen(lines[open]),
        closer: widen(lines[close]),
    })
}

/// The same container with its body dropped, for the opener-only render.
fn without_children(node: &BlockNode) -> Option<BlockNode> {
    Some(match node {
        BlockNode::Admonition(admonition) => {
            let mut bare = admonition.clone();
            bare.children = Vec::new();
            BlockNode::Admonition(bare)
        }
        BlockNode::Directive(directive) => {
            let mut bare = directive.clone();
            bare.children = Vec::new();
            BlockNode::Directive(bare)
        }
        BlockNode::Div(div) => {
            let mut bare = div.clone();
            bare.children = Vec::new();
            BlockNode::Div(bare)
        }
        BlockNode::FigureGroup(group) => {
            let mut bare = group.clone();
            bare.children = Vec::new();
            // The group's own caption sits OUTSIDE the container in Carve - the
            // slot hangs on the closing fence - so it is not part of the opener
            // and is not restored by the round trip (markup-carve/carve#2851).
            bare.caption = None;
            BlockNode::FigureGroup(bare)
        }
        _ => return None,
    })
}

/// How many bold fallback lines the Markdown target wrote for an opener's own
/// metadata: one for a quoted title, one for a `[label]`. The payload carries
/// them now, so leaving the fallback would be the same text twice.
fn carrier_fallback_lines(payload: &str) -> usize {
    static LABEL: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    static TITLE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let label = LABEL.get_or_init(|| regex::Regex::new(r"[ \t]?\[.*\]$").unwrap());
    let title = TITLE.get_or_init(|| regex::Regex::new(r#"[ \t]"[^"]*"$"#).unwrap());
    let width = carrier_fence_width(payload);
    if width == 0 {
        return 0;
    }
    let mut rest = &payload[width..];
    let mut lines = 0;
    if let Some(found) = label.find(rest) {
        lines += 1;
        rest = &rest[..found.start()];
    }
    if title.is_match(rest) {
        lines += 1;
    }

    lines
}

/// Whether a marker set records a structure at all: every opener closed by a
/// bare fence of its own width, every attribute line against an opener, and
/// nothing left open.
fn carrier_set_balances(payloads: &[String]) -> bool {
    let mut open: Vec<usize> = Vec::new();
    let mut prelude = false;
    for payload in payloads {
        let width = carrier_fence_width(payload);
        if width == 0 {
            // An attribute line belongs to the opener on the next marker.
            prelude = true;
            continue;
        }
        if carrier_is_closer(payload) {
            if prelude || open.pop() != Some(width) {
                return false;
            }
            continue;
        }
        prelude = false;
        if open.last().is_some_and(|outer| width <= *outer) {
            return false;
        }
        open.push(width);
    }

    !prelude && open.is_empty()
}

/// Every carrier marker lifted out of a Markdown source, leaving a placeholder.
pub(crate) struct CarrierLift {
    pub(crate) source: String,
    /// Each lifted payload and whether it is a bare closer, in marker order.
    slots: Vec<(String, bool)>,
    token: String,
    /// A set that does not balance records no structure, so nothing is lifted
    /// and the reader is told.
    pub(crate) damaged: bool,
}

/// Lift every carrier marker line out of the source, leaving a placeholder.
///
/// WHICH LINES ARE MARKERS COMES FROM A PARSE, NOT FROM A LINE SCAN. A flat
/// scan cannot tell a marker-shaped line standing at a list item's content
/// column from verbatim text in a code block inside that item, so it either
/// corrupts the verbatim run or refuses the nesting a document most often has.
/// `pulldown_cmark` has already made that call: a marker that is block content
/// arrives as an HTML block whatever host prefix its line carries, and one
/// inside a fenced or indented code block arrives as code text at any indent
/// (markup-carve/carve#2850).
///
/// A set that does not balance is NEVER reconstructed: the source comes back
/// untouched, the markers import as the raw HTML they are, and the caller
/// reports one `carrier-markers-damaged` loss.
pub(crate) fn lift_carrier_markers(markdown: &str) -> CarrierLift {
    let none = |source: &str, damaged: bool| CarrierLift {
        source: source.to_owned(),
        slots: Vec::new(),
        token: String::new(),
        damaged,
    };
    if !markdown.contains(CARRIER_PREFIX) {
        return none(markdown, false);
    }
    let normalized = if markdown.contains('\r') {
        markdown.replace("\r\n", "\n").replace('\r', "\n")
    } else {
        markdown.to_owned()
    };
    let lines: Vec<&str> = normalized.split('\n').collect();
    let payloads = parsed_carrier_markers(&normalized, &lines);
    if payloads.is_empty() {
        return none(markdown, false);
    }
    let only: Vec<String> = payloads.iter().map(|found| found.payload.clone()).collect();
    if !carrier_set_balances(&only) {
        return none(markdown, true);
    }

    let mut token = String::from("CARVECARRIER");
    while normalized.contains(&token) {
        token.push('X');
    }
    let mut slots = Vec::new();
    let mut out: Vec<String> = Vec::new();
    let mut drop_bold = 0usize;
    let mut skip_blank = false;
    let mut host = String::new();
    let mut next = 0;
    for (at, line) in lines.iter().enumerate() {
        if next < payloads.len() && payloads[next].line == at {
            let found = &payloads[next];
            next += 1;
            // The placeholder keeps the host prefix the marker line carried, so
            // a marker inside a list item or a block quote stays in that host
            // when the lifted source is parsed.
            out.push(format!("{}{token}{}Z", found.host, slots.len()));
            let closer = carrier_is_closer(&found.payload);
            drop_bold = if closer {
                0
            } else {
                carrier_fallback_lines(&found.payload)
            };
            skip_blank = false;
            host = found.host.clone();
            slots.push((found.payload.clone(), closer));
            continue;
        }
        let body = strip_host_prefix(line, &host);
        if drop_bold > 0 && body.len() > 4 && body.starts_with("**") && body.ends_with("**") {
            drop_bold -= 1;
            skip_blank = true;
            continue;
        }
        if skip_blank && body.is_empty() {
            skip_blank = false;
            continue;
        }
        skip_blank = false;
        out.push((*line).to_owned());
    }

    CarrierLift {
        source: out.join("\n"),
        slots,
        token,
        damaged: false,
    }
}

/// A carrier marker the parse reported as block content.
struct FoundMarker {
    /// Index of the line the marker stands on.
    line: usize,
    /// The host prefix that line carries before the marker, `> ` and list
    /// indentation included.
    host: String,
    payload: String,
}

/// Every carrier marker a parse of `markdown` reports as an HTML BLOCK.
///
/// Reading only HTML blocks is what makes the verbatim cases free: a marker in
/// a code block arrives as code text instead, so it is never a candidate.
fn parsed_carrier_markers(markdown: &str, lines: &[&str]) -> Vec<FoundMarker> {
    use pulldown_cmark::{Event, Options, Parser};
    let mut line_starts = Vec::with_capacity(lines.len());
    let mut offset = 0;
    for line in lines {
        line_starts.push(offset);
        offset += line.len() + 1;
    }
    // The options the import itself parses with, minus the metadata gate: a
    // frontmatter block holds no HTML block either way.
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);
    options.insert(Options::ENABLE_FOOTNOTES);
    let mut found = Vec::new();
    let take = |at: usize, text: &str| {
        let line = line_starts.partition_point(|start| *start <= at) - 1;
        let host = &lines[line][..at - line_starts[line]];
        carrier_payload(text).map(|payload| FoundMarker {
            line,
            host: host.to_owned(),
            payload,
        })
    };
    for (event, range) in Parser::new_ext(markdown, options).into_offset_iter() {
        match event {
            Event::Html(html) => {
                // An HTML block can span several lines; a marker is one line of
                // its own, so each line of the block is tested on its merits.
                let mut at = range.start;
                for text in html.split_inclusive('\n') {
                    found.extend(take(at, text.trim_end_matches('\n')));
                    at += text.len();
                }
            }
            // A MARKER A BLOCK PREFIX PUSHED OFF COLUMN ZERO arrives as an
            // inline span: a task item's `[ ] ` is inline content, so an HTML
            // block cannot begin after it, and the comment is read mid-line.
            // Requiring the span to BE the whole line, host prefix aside, is
            // what keeps a marker-shaped span inside running text out of this.
            Event::InlineHtml(html) => {
                let line = line_starts.partition_point(|start| *start <= range.start) - 1;
                let column = range.start - line_starts[line];
                if lines[line].len() == column + html.len() {
                    found.extend(take(range.start, &html));
                }
            }
            _ => {}
        }
    }
    found.sort_by_key(|marker| marker.line);

    found
}

/// A line with its host prefix removed, for the fallback lines travelling under
/// an opener. The recorded prefix is tried first; a block quote writes its blank
/// lines as a bare `>`, which that prefix does not cover.
fn strip_host_prefix<'a>(line: &'a str, host: &str) -> &'a str {
    if !host.is_empty() {
        if let Some(rest) = line.strip_prefix(host) {
            return rest;
        }
        return line.trim_start_matches([' ', '\t', '>']);
    }

    line
}

/// Write every lifted payload back as the Carve line it is.
pub(crate) fn restore_carrier_markers(carve: &str, lift: &CarrierLift) -> String {
    if lift.slots.is_empty() {
        return carve.to_owned();
    }
    let items: Vec<RestoredLine> = carve
        .split('\n')
        .map(|line| {
            // The host prefix comes off the CARVE line rather than off the
            // Markdown the placeholder was lifted from: the two hosts nest the
            // same way but spell their prefixes differently, and it is the
            // Carve line the marker has to stand on. The prefix is taken as
            // WHATEVER STANDS BEFORE THE TOKEN rather than as a character
            // class, because a container opening a list item puts the marker
            // behind that item's `-` and leaving the token in the output would
            // be corruption rather than a missed restore.
            let found = line.find(&lift.token);
            let (lead, body) = line.split_at(found.unwrap_or(0));
            let slot = found
                .map(|_| body.trim_end())
                .and_then(|body| body.strip_prefix(&lift.token))
                .and_then(|rest| rest.strip_suffix('Z'))
                .and_then(|index| index.parse::<usize>().ok())
                .and_then(|index| lift.slots.get(index));
            match slot {
                Some((payload, closer)) => RestoredLine {
                    lead: lead.to_owned(),
                    text: payload.clone(),
                    kind: Some(*closer),
                },
                // A line that is not a marker keeps its own spelling, and
                // only its host prefix is needed, for the blank test below.
                None => {
                    let lead = line.len() - line.trim_start_matches([' ', '\t', '>']).len();
                    RestoredLine {
                        lead: line[..lead].to_owned(),
                        text: line[lead..].to_owned(),
                        kind: None,
                    }
                }
            }
        })
        .collect();

    separate_carrier_lines(&items).join("\n")
}

/// A line of the written Carve on the way back: its host prefix, the rest, and
/// which kind of marker produced it, if any. `Some(false)` is an opener or the
/// attribute line travelling with it, `Some(true)` a bare closer.
struct RestoredLine {
    lead: String,
    text: String,
    kind: Option<bool>,
}

impl RestoredLine {
    /// A line carrying nothing but its host prefix.
    fn blank(&self) -> bool {
        self.kind.is_none() && self.text.is_empty()
    }

    fn spelled(&self) -> String {
        format!("{}{}", self.lead, self.text)
    }
}

/// A host prefix with any list marker in it blanked out, so a closer can stand
/// at its opener's content column without repeating that item's marker.
fn blanked_lead(lead: &str) -> String {
    lead.chars()
        .map(|ch| if ch == '>' || ch == '\t' { ch } else { ' ' })
        .collect()
}

/// Give every restored marker line the blank lines the canonical writer puts
/// around it: a container's opener and closer hug its body, and what follows a
/// closer is a block of its own.
fn separate_carrier_lines(items: &[RestoredLine]) -> Vec<String> {
    // A CLOSER STANDS AT ITS OPENER'S COLUMN. Where a container opens a list
    // item and its body begins with a list, the closer's placeholder is a lazy
    // continuation of that inner item's paragraph, so the written Carve puts it
    // at the inner content column; the container would then close in the wrong
    // host. The opener's own prefix is the answer, with any list marker in it
    // blanked out, because a closer cannot repeat an item's marker.
    let mut open: Vec<String> = Vec::new();
    let leads: Vec<String> = items
        .iter()
        .map(|item| match item.kind {
            Some(false) if carrier_fence_width(&item.text) > 0 => {
                open.push(blanked_lead(&item.lead));
                item.lead.clone()
            }
            Some(true) => open.pop().unwrap_or_else(|| item.lead.clone()),
            _ => item.lead.clone(),
        })
        .collect();
    let items: Vec<RestoredLine> = items
        .iter()
        .zip(leads)
        .map(|(item, lead)| RestoredLine {
            lead,
            text: item.text.clone(),
            kind: item.kind,
        })
        .collect();
    let items = &items[..];
    let mut hugged = vec![false; items.len()];
    let mut at = 0;
    while at < items.len() {
        if !items[at].blank() {
            at += 1;
            continue;
        }
        let mut end = at;
        while end < items.len() && items[end].blank() {
            end += 1;
        }
        let mut before = at;
        let above = loop {
            if before == 0 {
                break None;
            }
            before -= 1;
            if !items[before].blank() {
                break Some(&items[before]);
            }
        };
        let below = items.get(end).filter(|item| item.kind.is_some());
        // A blank above a closer and one below an opener are both INSIDE the
        // container, where the canonical writer puts none. The blank has to
        // share the marker's HOST to be inside it: a `>` line next to a marker
        // standing at column 0 belongs to a block quote of its own.
        let inside = below
            .filter(|item| item.kind == Some(true))
            .or_else(|| above.filter(|item| item.kind == Some(false)))
            .filter(|marker| {
                items[at..end]
                    .iter()
                    .all(|blank| blank.lead.trim_end() == marker.lead.trim_end())
            });
        if inside.is_some() {
            for slot in hugged.iter_mut().take(end).skip(at) {
                *slot = true;
            }
        }
        at = end;
    }

    let mut out: Vec<String> = Vec::new();
    let mut last: Option<&RestoredLine> = None;
    for (index, item) in items.iter().enumerate() {
        if hugged[index] {
            continue;
        }
        // A closer and what follows it are two blocks, so they take a blank
        // line between them - but only where they are SIBLINGS. A list item
        // opening after a closer is a block of the host above, and a blank
        // there would turn a tight list loose.
        if last.is_some_and(|prior| prior.kind == Some(true) && item.lead.len() >= prior.lead.len())
            && item.kind != Some(true)
            && !item.text.is_empty()
        {
            out.push(item.lead.trim_end().to_owned());
        }
        out.push(item.spelled());
        last = Some(item);
    }

    out
}
