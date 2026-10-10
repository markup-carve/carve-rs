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

/// A Markdown fenced-code opener at an indent a fence is read at, as
/// (fence run, info string).
fn markdown_fence_opener(line: &str) -> Option<(&str, &str)> {
    let indent = line.len() - line.trim_start_matches([' ', '\t']).len();
    if indent > 3 {
        return None;
    }
    let rest = &line[indent..];
    let ch = rest.chars().next().filter(|c| *c == '`' || *c == '~')?;
    let run = rest.len() - rest.trim_start_matches(ch).len();
    if run < 3 {
        return None;
    }
    let info = &rest[run..];
    // A backtick fence's info string cannot hold a backtick (CommonMark).
    if ch == '`' && info.contains('`') {
        return None;
    }

    Some((&rest[..run], info))
}

/// Whether a line closes an open fence of `fence`.
fn markdown_fence_closes(line: &str, fence: &str) -> bool {
    markdown_fence_opener(line).is_some_and(|(run, info)| {
        run.as_bytes()[0] == fence.as_bytes()[0]
            && run.len() >= fence.len()
            && info.trim().is_empty()
    })
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
    // A MARKER INSIDE A FENCED CODE BLOCK IS NOT A MARKER. A code block's
    // payload is verbatim content, so a page documenting the mode holds
    // marker-shaped lines that record no container; lifting one rewrote the
    // sample inside the fence. An indented code block needs no guard: a marker
    // is only read at column 0.
    let mut payloads: Vec<(usize, String)> = Vec::new();
    let mut fence: Option<&str> = None;
    for (at, line) in lines.iter().enumerate() {
        if let Some(open) = fence {
            if markdown_fence_closes(line, open) {
                fence = None;
            }
            continue;
        }
        if let Some((run, _)) = markdown_fence_opener(line) {
            fence = Some(run);
            continue;
        }
        if let Some(payload) = carrier_payload(line) {
            payloads.push((at, payload));
        }
    }
    if payloads.is_empty() {
        return none(markdown, false);
    }
    let only: Vec<String> = payloads.iter().map(|(_, p)| p.clone()).collect();
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
    let mut next = 0;
    for (at, line) in lines.iter().enumerate() {
        if next < payloads.len() && payloads[next].0 == at {
            let payload = payloads[next].1.clone();
            next += 1;
            out.push(format!("{token}{}Z", slots.len()));
            let closer = carrier_is_closer(&payload);
            drop_bold = if closer {
                0
            } else {
                carrier_fallback_lines(&payload)
            };
            skip_blank = false;
            slots.push((payload, closer));
            continue;
        }
        if drop_bold > 0 && line.len() > 4 && line.starts_with("**") && line.ends_with("**") {
            drop_bold -= 1;
            skip_blank = true;
            continue;
        }
        if skip_blank && line.is_empty() {
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

/// Write every lifted payload back as the Carve line it is.
pub(crate) fn restore_carrier_markers(carve: &str, lift: &CarrierLift) -> String {
    if lift.slots.is_empty() {
        return carve.to_owned();
    }
    // Each line as its text plus which kind of marker, if any, produced it:
    // `Some(false)` for an opener or the attribute line travelling with it,
    // `Some(true)` for a bare closer.
    let items: Vec<(String, Option<bool>)> = carve
        .split('\n')
        .map(|line| {
            let slot = line
                .trim()
                .strip_prefix(&lift.token)
                .and_then(|rest| rest.strip_suffix('Z'))
                .and_then(|index| index.parse::<usize>().ok())
                .and_then(|index| lift.slots.get(index));
            match slot {
                Some((payload, closer)) => (payload.clone(), Some(*closer)),
                None => (line.to_owned(), None),
            }
        })
        .collect();

    separate_carrier_lines(&items).join("\n")
}

/// Give every restored marker line the blank lines the canonical writer puts
/// around it: a container's opener and closer hug its body, and what follows a
/// closer is a block of its own.
fn separate_carrier_lines(items: &[(String, Option<bool>)]) -> Vec<String> {
    let mut hugged = vec![false; items.len()];
    let mut at = 0;
    while at < items.len() {
        if items[at].1.is_some() || !items[at].0.is_empty() {
            at += 1;
            continue;
        }
        let mut end = at;
        while end < items.len() && items[end].1.is_none() && items[end].0.is_empty() {
            end += 1;
        }
        let mut before = at;
        let above = loop {
            if before == 0 {
                break None;
            }
            before -= 1;
            if items[before].1.is_some() || !items[before].0.is_empty() {
                break items[before].1;
            }
        };
        let below = items.get(end).and_then(|item| item.1);
        // A blank above a closer and one below an opener are both INSIDE the
        // container, where the canonical writer puts none.
        if below == Some(true) || above == Some(false) {
            for slot in hugged.iter_mut().take(end).skip(at) {
                *slot = true;
            }
        }
        at = end;
    }

    let mut out: Vec<String> = Vec::new();
    let mut last: Option<Option<bool>> = None;
    for (index, (text, kind)) in items.iter().enumerate() {
        if hugged[index] {
            continue;
        }
        if last == Some(Some(true)) && *kind != Some(true) && !text.is_empty() {
            out.push(String::new());
        }
        out.push(text.clone());
        last = Some(*kind);
    }

    out
}
