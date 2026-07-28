use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};

use crate::ast::*;
use crate::escape::{escape_attr, escape_text};
use crate::extension::{
    BeforeRenderContext, CarveExtension, InlineMatch, MatcherContext, RenderContext,
};

const REFS_BLOCK: &str = "citations-references";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CitationMode {
    Numbered,
    AuthorDate,
}

impl From<CitationMode> for CitationRenderMode {
    fn from(mode: CitationMode) -> Self {
        match mode {
            CitationMode::Numbered => CitationRenderMode::Numbered,
            CitationMode::AuthorDate => CitationRenderMode::AuthorDate,
        }
    }
}

/// A CSL-JSON name object (the subset the minimal formatter reads). The host
/// builds these from parsed CSL-JSON; the extension does no file I/O or parsing.
#[derive(Debug, Clone, Default)]
pub struct CslName {
    pub family: Option<String>,
    pub given: Option<String>,
    pub literal: Option<String>,
}

/// A CSL-JSON `issued` date (the subset the minimal formatter reads).
#[derive(Debug, Clone, Default)]
pub struct CslDate {
    /// `date-parts[0][0]` is the year.
    pub date_parts: Option<Vec<Vec<i64>>>,
    pub literal: Option<String>,
}

/// A CSL-JSON bibliography entry (the subset the minimal formatter reads).
#[derive(Debug, Clone, Default)]
pub struct CslEntry {
    pub id: String,
    pub author: Option<Vec<CslName>>,
    pub issued: Option<CslDate>,
    pub title: Option<String>,
}

#[derive(Debug, Clone)]
struct Def {
    entry: Vec<InlineNode>,
    author: Option<String>,
    year: Option<String>,
    /// Pre-formatted entry text for a CSL-JSON-sourced def (HTML-escaped at
    /// render time); when set, used instead of the parsed inline `entry`.
    csl_text: Option<String>,
}

pub struct Citations {
    mode: CitationMode,
    /// A supplied pool (even empty) activates the Tier-3 Bibliography behavior:
    /// external resolution + back-links (#199).
    bibliography: Option<Vec<CslEntry>>,
    defs: RefCell<BTreeMap<String, Def>>,
    order: RefCell<Vec<String>>,
    /// Per-key use-site count, populated in before_render; drives back-links.
    uses: RefCell<BTreeMap<String, usize>>,
}

impl Citations {
    pub fn new() -> Self {
        Self::with_mode(CitationMode::Numbered)
    }

    pub fn author_date() -> Self {
        Self::with_mode(CitationMode::AuthorDate)
    }

    pub fn with_mode(mode: CitationMode) -> Self {
        Self {
            mode,
            bibliography: None,
            defs: RefCell::new(BTreeMap::new()),
            order: RefCell::new(Vec::new()),
            uses: RefCell::new(BTreeMap::new()),
        }
    }

    /// Attach an external CSL-JSON pool (Tier-3 Bibliography, #199). Citation
    /// keys resolve against in-document `[@key]:` defs first, then this pool;
    /// in-text citations and the references list gain footnote-style back-links.
    pub fn with_bibliography(mut self, bibliography: Vec<CslEntry>) -> Self {
        self.bibliography = Some(bibliography);
        self
    }

    fn has_bib(&self) -> bool {
        self.bibliography.is_some()
    }
}

impl Default for Citations {
    fn default() -> Self {
        Self::new()
    }
}

impl CarveExtension for Citations {
    fn name(&self) -> &'static str {
        "citations"
    }

    fn match_inline(
        &self,
        text: &str,
        pos: usize,
        ctx: &MatcherContext<'_>,
    ) -> Option<InlineMatch> {
        match_citation(text, pos, ctx)
    }

    fn after_parse(&self, mut doc: Document) -> Document {
        self.defs.borrow_mut().clear();
        self.order.borrow_mut().clear();
        self.uses.borrow_mut().clear();
        doc.children = collect_defs(doc.children, &mut self.defs.borrow_mut());
        // Seed the CSL-JSON pool: in-document defs win on collision (§6.2).
        if let Some(pool) = &self.bibliography {
            let mut defs = self.defs.borrow_mut();
            for entry in pool {
                if !entry.id.is_empty() && !defs.contains_key(&entry.id) {
                    defs.insert(entry.id.clone(), csl_to_def(entry));
                }
            }
        }
        doc
    }

    fn before_render(&self, mut doc: Document, _ctx: &BeforeRenderContext<'_>) -> Document {
        let defs = self.defs.borrow();
        let mut seen = BTreeSet::new();
        let mut order = Vec::new();
        let mut uses = BTreeMap::new();
        let has_bib = self.has_bib();
        for block in &mut doc.children {
            annotate_citations_block(
                block, &defs, self.mode, has_bib, &mut seen, &mut order, &mut uses,
            );
        }
        drop(defs);
        *self.order.borrow_mut() = order;
        *self.uses.borrow_mut() = uses;
        if !self.order.borrow().is_empty() {
            inject_references_block(&mut doc.children);
        }
        doc
    }

    fn render_block_extension(
        &self,
        node: &BlockExtension,
        ctx: &RenderContext<'_>,
    ) -> Option<String> {
        if node.name == REFS_BLOCK {
            Some(render_refs_list(
                ctx,
                self.mode,
                &self.order.borrow(),
                &self.defs.borrow(),
                &self.uses.borrow(),
                self.has_bib(),
            ))
        } else {
            None
        }
    }
}

fn match_citation(text: &str, pos: usize, ctx: &MatcherContext<'_>) -> Option<InlineMatch> {
    if !text.get(pos..)?.starts_with('[') {
        return None;
    }
    let close = close_bracket(text, pos)?;
    if matches!(text.as_bytes().get(close + 1), Some(b'(' | b'[' | b'{')) {
        return None;
    }
    let inner = &text[pos + 1..close];
    if !inner.contains('@') {
        return None;
    }
    let mut integral = false;
    let inner_str = if let Some(stripped) = inner.strip_prefix('+') {
        integral = true;
        stripped
    } else {
        inner
    };
    let mut items = Vec::new();
    for part in inner_str.split(';') {
        items.push(parse_item(part, ctx)?);
    }
    if items.is_empty() {
        return None;
    }
    Some(InlineMatch {
        node: InlineNode::CitationGroup(CitationGroup {
            items,
            raw: text[pos..close + 1].to_string(),
            mode: None,
            integral,
        }),
        end: close + 1,
    })
}

fn close_bracket(text: &str, open: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut depth = 0usize;
    let mut i = open;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 2,
            b'[' => {
                depth += 1;
                i += 1;
            }
            b']' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(i);
                }
                i += 1;
            }
            _ => i += 1,
        }
    }
    None
}

fn parse_item(raw: &str, ctx: &MatcherContext<'_>) -> Option<Citation> {
    let trimmed = raw.trim();
    for (at, _) in trimmed.match_indices('@') {
        if at > 0 && trimmed.as_bytes().get(at - 1) == Some(&b'\\') {
            continue;
        }
        let (key, key_end) = parse_key(trimmed, at + 1)?;
        let rest = trimmed[key_end..].trim_start();
        let (locator, locator_label, locator_value, suffix) = if rest.is_empty() {
            (None, None, None, None)
        } else if let Some(loc_raw) = rest.strip_prefix(',') {
            let loc_raw = loc_raw.trim();
            if loc_raw.is_empty() {
                // A trailing comma with no locator text is ignored - the item is
                // a normal citation, not a verbatim fallback (matches carve-js /
                // carve-php; markup-carve/carve#227).
                (None, None, None, None)
            } else {
                let locator = Some(ctx.parse_inlines(loc_raw));
                let p = parse_locator(loc_raw);
                let suffix = p.suffix_text.as_deref().map(|st| ctx.parse_inlines(st));
                (locator, p.label, p.value, suffix)
            }
        } else {
            continue;
        };
        let suppress_author = at > 0 && trimmed.as_bytes()[at - 1] == b'-';
        let prefix_end = if suppress_author { at - 1 } else { at };
        let prefix_text = trimmed[..prefix_end].trim_end();
        let prefix = (!prefix_text.is_empty()).then(|| ctx.parse_inlines(prefix_text));
        return Some(Citation {
            key: key.to_string(),
            prefix,
            locator,
            locator_label,
            locator_value,
            suffix,
            suppress_author,
            number: None,
            label: None,
            use_index: None,
        });
    }
    None
}

/// The result of parsing the locator text after a citation key's comma.
pub struct ParsedLocator {
    pub label: Option<String>,
    pub value: Option<String>,
    pub suffix_text: Option<String>,
}

/// Parse the locator portion of a citation (the text after the comma and key).
///
/// Implements the CSL locator-label vocabulary: matches known terms at word
/// boundaries, extracts the numeric/roman value, and captures any trailing text
/// as a suffix.
pub fn parse_locator(loc: &str) -> ParsedLocator {
    // All (matcher, canonical) pairs — sorted by matcher length descending so
    // longer forms beat shorter prefixes (e.g. "pages" before "page").
    const VOCAB: &[(&str, &str)] = &[
        ("sub verbo", "sub verbo"),
        ("paragraph", "paragraph"),
        ("section", "section"),
        ("chapter", "chapter"),
        ("volume", "volume"),
        ("figure", "figure"),
        ("column", "column"),
        ("verse", "verse"),
        ("pages", "page"),
        ("folio", "folio"),
        ("issue", "issue"),
        ("note", "note"),
        ("opus", "opus"),
        ("part", "part"),
        ("line", "line"),
        ("book", "book"),
        ("chaps.", "chapter"),
        ("chap.", "chapter"),
        ("cols.", "column"),
        ("col.", "column"),
        ("figs.", "figure"),
        ("fig.", "figure"),
        ("fols.", "folio"),
        ("fol.", "folio"),
        ("opp.", "opus"),
        ("op.", "opus"),
        ("pp.", "page"),
        ("page", "page"),
        ("paras.", "paragraph"),
        ("para.", "paragraph"),
        ("pts.", "part"),
        ("pt.", "part"),
        ("secs.", "section"),
        ("sec.", "section"),
        ("s.vv.", "sub verbo"),
        ("s.v.", "sub verbo"),
        ("vols.", "volume"),
        ("vol.", "volume"),
        ("vv.", "verse"),
        ("ll.", "line"),
        ("nn.", "note"),
        ("bk.", "book"),
        ("no.", "issue"),
        ("p.", "page"),
        ("v.", "verse"),
        ("l.", "line"),
        ("n.", "note"),
        ("¶¶", "paragraph"),
        ("¶", "paragraph"),
        ("§§", "section"),
        ("§", "section"),
    ];

    let s = loc.trim_start_matches([' ', '\t']);
    if s.is_empty() {
        return ParsedLocator {
            label: None,
            value: None,
            suffix_text: None,
        };
    }

    // Try each matcher (longest first).
    let matched = VOCAB.iter().find_map(|(matcher, canonical)| {
        let ml = matcher.len();
        let sl = s.len();
        if sl < ml {
            return None;
        }
        // The matcher's byte length may land mid-char when `s` begins with a
        // multibyte char (e.g. a 2-byte matcher over a 3-byte `€`). Guard the
        // slice so an exotic locator does not panic — carve-js indexes by code
        // unit and never throws here.
        if !s.is_char_boundary(ml) {
            return None;
        }
        // Case-insensitive prefix match.
        if !s[..ml].eq_ignore_ascii_case(matcher) {
            return None;
        }
        // Boundary check: char immediately after the match must be a boundary
        // or the string ends.
        let rest_after = &s[ml..];
        let boundary = match rest_after.chars().next() {
            None => true,
            Some(' ') | Some('\t') => true,
            Some(c) if c.is_ascii_digit() => true,
            Some('§') | Some('¶') => true,
            _ => false,
        };
        if !boundary {
            return None;
        }
        // Strip leading whitespace from rest.
        let rest = rest_after.trim_start_matches([' ', '\t']);
        Some((*canonical, rest))
    });

    let (label, rest) = if let Some((canonical, rest)) = matched {
        (Some(canonical.to_string()), rest)
    } else {
        // No label matched — if the first char is a digit, default to "page".
        if s.chars()
            .next()
            .map(|c| c.is_ascii_digit())
            .unwrap_or(false)
        {
            (Some("page".to_string()), s)
        } else {
            // No label, treat entire text as suffix.
            let st = s.to_string();
            return ParsedLocator {
                label: None,
                value: None,
                suffix_text: if st.is_empty() { None } else { Some(st) },
            };
        }
    };

    // Parse value: consume VALUE_CHAR chars from start of rest.
    // VALUE_CHAR = [0-9IVXLCDMivxlcdm.,&\- ]
    let value_end = rest
        .char_indices()
        .find(|(_, c)| {
            !matches!(
                c,
                '0'..='9'
                    | 'I'
                    | 'V'
                    | 'X'
                    | 'L'
                    | 'C'
                    | 'D'
                    | 'M'
                    | 'i'
                    | 'v'
                    | 'x'
                    | 'l'
                    | 'c'
                    | 'd'
                    | 'm'
                    | '.'
                    | ','
                    | '&'
                    | '-'
                    | ' '
            )
        })
        .map(|(i, _)| i)
        .unwrap_or(rest.len());
    let raw_value = &rest[..value_end];
    // Trim trailing [ ,&\-.] from value.
    let value = raw_value
        .trim_end_matches([' ', ',', '&', '-', '.'])
        .to_string();

    let suffix_start = &rest[value_end..];
    let suffix_text_str = suffix_start.trim_start_matches([' ', '\t']).to_string();

    ParsedLocator {
        label,
        value: if value.is_empty() { None } else { Some(value) },
        suffix_text: if suffix_text_str.is_empty() {
            None
        } else {
            Some(suffix_text_str)
        },
    }
}

fn parse_key(text: &str, start: usize) -> Option<(&str, usize)> {
    let bytes = text.as_bytes();
    let first = *bytes.get(start)?;
    if !(first.is_ascii_alphanumeric() || first == b'_') {
        return None;
    }
    let mut end = start + 1;
    while let Some(&b) = bytes.get(end) {
        if b.is_ascii_alphanumeric()
            || matches!(
                b,
                b'_' | b':'
                    | b'.'
                    | b'#'
                    | b'$'
                    | b'%'
                    | b'&'
                    | b'+'
                    | b'?'
                    | b'<'
                    | b'>'
                    | b'~'
                    | b'/'
                    | b'-'
            )
        {
            end += 1;
        } else {
            break;
        }
    }
    Some((&text[start..end], end))
}

fn collect_defs(blocks: Vec<BlockNode>, defs: &mut BTreeMap<String, Def>) -> Vec<BlockNode> {
    let mut out = Vec::new();
    for block in blocks {
        match block {
            BlockNode::Paragraph(mut p) => {
                let original_children = p.children;
                let lines = split_on_soft_breaks(original_children);
                let mut kept = Vec::new();
                for line in lines {
                    if let Some((key, def)) = as_definition(&line) {
                        defs.insert(key, def);
                    } else {
                        kept.push(line);
                    }
                }
                if kept.is_empty() {
                    continue;
                }
                p.children = join_with_soft_breaks(kept);
                out.push(BlockNode::Paragraph(p));
            }
            BlockNode::List(mut l) => {
                for item in &mut l.items {
                    item.children = collect_defs(std::mem::take(&mut item.children), defs);
                }
                out.push(BlockNode::List(l));
            }
            BlockNode::BlockQuote(mut b) => {
                b.children = collect_defs(b.children, defs);
                out.push(BlockNode::BlockQuote(b));
            }
            BlockNode::Admonition(mut a) => {
                a.children = collect_defs(a.children, defs);
                out.push(BlockNode::Admonition(a));
            }
            BlockNode::Div(mut d) => {
                d.children = collect_defs(d.children, defs);
                out.push(BlockNode::Div(d));
            }
            other => out.push(other),
        }
    }
    out
}

fn split_on_soft_breaks(nodes: Vec<InlineNode>) -> Vec<Vec<InlineNode>> {
    let mut lines = vec![Vec::new()];
    for node in nodes {
        if matches!(node, InlineNode::SoftBreak) {
            lines.push(Vec::new());
        } else {
            lines.last_mut().unwrap().push(node);
        }
    }
    lines
}

fn join_with_soft_breaks(lines: Vec<Vec<InlineNode>>) -> Vec<InlineNode> {
    let mut out = Vec::new();
    for (idx, line) in lines.into_iter().enumerate() {
        if idx > 0 {
            out.push(InlineNode::SoftBreak);
        }
        out.extend(line);
    }
    out
}

fn as_definition(line: &[InlineNode]) -> Option<(String, Def)> {
    let InlineNode::CitationGroup(group) = line.first()? else {
        return None;
    };
    if group.items.len() != 1 {
        return None;
    }
    let item = &group.items[0];
    if item.prefix.is_some() || item.locator.is_some() || item.suppress_author {
        return None;
    }
    let InlineNode::Text(second) = line.get(1)? else {
        return None;
    };
    if !second.starts_with(':') {
        return None;
    }

    let mut entry = line[1..].to_vec();
    if let InlineNode::Text(head) = &mut entry[0] {
        *head = head.trim_start_matches(':').trim_start().to_string();
    }
    let mut def = Def {
        entry,
        author: None,
        year: None,
        csl_text: None,
    };
    consume_leading_attrs(&mut def);
    Some((item.key.clone(), def))
}

/// Build a `Def` from a CSL-JSON entry using the minimal fixed template (§6.3):
/// `Family, Given (Year). Title.`, missing fields + separators omitted, trailing
/// period when non-empty. The text is plain (HTML-escaped at render time).
fn csl_to_def(e: &CslEntry) -> Def {
    let names: Vec<String> = e
        .author
        .as_deref()
        .unwrap_or_default()
        .iter()
        .map(format_name)
        .filter(|n| !n.is_empty())
        .collect();
    let authors = names.join("; ");
    let year = csl_year(e.issued.as_ref());
    let head = if let Some(y) = &year {
        if authors.is_empty() {
            format!("({y})")
        } else {
            format!("{authors} ({y})")
        }
    } else {
        authors
    };
    let mut segs: Vec<String> = Vec::new();
    if !head.is_empty() {
        segs.push(head);
    }
    if let Some(title) = &e.title {
        if !title.is_empty() {
            segs.push(title.clone());
        }
    }
    let mut csl_text = segs.join(". ");
    if !csl_text.is_empty() {
        csl_text.push('.');
    }
    // author/year also feed author-date mode; use the first author's family.
    let author = e
        .author
        .as_deref()
        .and_then(|a| a.first())
        .and_then(|n| n.literal.clone().or_else(|| n.family.clone()));
    Def {
        entry: Vec::new(),
        author,
        year,
        csl_text: Some(csl_text),
    }
}

fn format_name(n: &CslName) -> String {
    if let Some(literal) = &n.literal {
        return literal.clone();
    }
    match (&n.family, &n.given) {
        (Some(family), Some(given)) => format!("{family}, {given}"),
        (Some(family), None) => family.clone(),
        _ => String::new(),
    }
}

fn csl_year(issued: Option<&CslDate>) -> Option<String> {
    let issued = issued?;
    if let Some(y) = issued
        .date_parts
        .as_ref()
        .and_then(|p| p.first())
        .and_then(|p| p.first())
    {
        return Some(y.to_string());
    }
    issued.literal.clone()
}

fn consume_leading_attrs(def: &mut Def) {
    let Some(first) = def.entry.first() else {
        return;
    };
    let Some(first_text) = inline_source_text(first) else {
        return;
    };
    if !first_text.starts_with('{') {
        return;
    }

    let mut source = String::new();
    let mut close_at: Option<(usize, usize)> = None;
    for (idx, node) in def.entry.iter().enumerate() {
        let Some(text) = inline_source_text(node) else {
            return;
        };
        if let Some(close) = text.find('}') {
            let attr_end = source.len() + close;
            source.push_str(text);
            close_at = Some((idx, attr_end));
            break;
        }
        source.push_str(text);
    }

    let Some((close_node, close)) = close_at else {
        return;
    };
    let attrs = &source[1..close];
    def.author = attr_value(attrs, "author");
    def.year = attr_value(attrs, "year");
    let tail = source[close + 1..].trim_start().to_string();
    def.entry.drain(0..=close_node);
    if !tail.is_empty() {
        def.entry.insert(0, InlineNode::Text(tail));
    }
}

fn inline_source_text(node: &InlineNode) -> Option<&str> {
    match node {
        InlineNode::Text(text) => Some(text),
        InlineNode::SmartPunctuation(s) => Some(&s.value),
        _ => None,
    }
}

fn attr_value(attrs: &str, key: &str) -> Option<String> {
    for token in attrs.split_whitespace() {
        let (k, v) = token.split_once('=')?;
        if k == key {
            return Some(v.trim_matches('"').trim_matches('\'').to_string());
        }
    }
    None
}

#[allow(clippy::too_many_arguments)]
fn annotate_citations_block(
    block: &mut BlockNode,
    defs: &BTreeMap<String, Def>,
    mode: CitationMode,
    has_bib: bool,
    seen: &mut BTreeSet<String>,
    order: &mut Vec<String>,
    uses: &mut BTreeMap<String, usize>,
) {
    match block {
        BlockNode::Heading(h) => {
            annotate_citations_inline(&mut h.children, defs, mode, has_bib, seen, order, uses)
        }
        BlockNode::Paragraph(p) => {
            annotate_citations_inline(&mut p.children, defs, mode, has_bib, seen, order, uses);
        }
        BlockNode::List(l) => {
            for item in &mut l.items {
                for child in &mut item.children {
                    annotate_citations_block(child, defs, mode, has_bib, seen, order, uses);
                }
            }
        }
        BlockNode::BlockQuote(b) => {
            for child in &mut b.children {
                annotate_citations_block(child, defs, mode, has_bib, seen, order, uses);
            }
        }
        BlockNode::Table(t) => {
            for row in &mut t.rows {
                for cell in &mut row.cells {
                    annotate_citations_inline(
                        &mut cell.children,
                        defs,
                        mode,
                        has_bib,
                        seen,
                        order,
                        uses,
                    );
                }
            }
        }
        BlockNode::Admonition(a) => {
            if let Some(title) = &mut a.title {
                annotate_citations_inline(title, defs, mode, has_bib, seen, order, uses);
            }
            for child in &mut a.children {
                annotate_citations_block(child, defs, mode, has_bib, seen, order, uses);
            }
        }
        BlockNode::Div(d) => {
            for child in &mut d.children {
                annotate_citations_block(child, defs, mode, has_bib, seen, order, uses);
            }
        }
        BlockNode::DefinitionList(d) => {
            for item in &mut d.items {
                for term in &mut item.terms {
                    annotate_citations_inline(term, defs, mode, has_bib, seen, order, uses);
                }
                for def_blocks in &mut item.definitions {
                    for child in def_blocks {
                        annotate_citations_block(child, defs, mode, has_bib, seen, order, uses);
                    }
                }
            }
        }
        BlockNode::Figure(f) => {
            annotate_citations_inline(&mut f.caption, defs, mode, has_bib, seen, order, uses);
            match &mut f.target {
                FigureTarget::BlockQuote(b) => {
                    for child in &mut b.children {
                        annotate_citations_block(child, defs, mode, has_bib, seen, order, uses);
                    }
                }
                FigureTarget::Table(t) => {
                    for row in &mut t.rows {
                        for cell in &mut row.cells {
                            annotate_citations_inline(
                                &mut cell.children,
                                defs,
                                mode,
                                has_bib,
                                seen,
                                order,
                                uses,
                            );
                        }
                    }
                }
                FigureTarget::Paragraph(p) => {
                    annotate_citations_inline(
                        &mut p.children,
                        defs,
                        mode,
                        has_bib,
                        seen,
                        order,
                        uses,
                    );
                }
                FigureTarget::Image(_) | FigureTarget::CodeBlock(_) => {}
            }
        }
        _ => {}
    }
}

#[allow(clippy::too_many_arguments)]
fn annotate_citations_inline(
    nodes: &mut [InlineNode],
    defs: &BTreeMap<String, Def>,
    mode: CitationMode,
    has_bib: bool,
    seen: &mut BTreeSet<String>,
    order: &mut Vec<String>,
    uses: &mut BTreeMap<String, usize>,
) {
    for node in nodes {
        match node {
            InlineNode::CitationGroup(g) => {
                g.mode = Some(mode.into());
                // A group with any unresolved key renders verbatim (§6.4): its
                // keys are literal text, not citations, so they are neither
                // numbered, listed, nor a back-link use site. Skip the whole
                // group.
                if !g.items.iter().all(|it| defs.contains_key(&it.key)) {
                    continue;
                }
                for item in &mut g.items {
                    let def = defs.get(&item.key).expect("all keys resolved above");
                    if seen.insert(item.key.clone()) {
                        order.push(item.key.clone());
                    }
                    let number = order.iter().position(|key| key == &item.key).unwrap() + 1;
                    item.number = Some(number);
                    if has_bib {
                        let n = uses.entry(item.key.clone()).or_insert(0);
                        *n += 1;
                        item.use_index = Some(*n);
                    }
                    item.label = Some(match mode {
                        CitationMode::Numbered => number.to_string(),
                        CitationMode::AuthorDate => {
                            if item.suppress_author {
                                def.year.clone().unwrap_or_else(|| number.to_string())
                            } else {
                                let label = format!(
                                    "{} {}",
                                    def.author.as_deref().unwrap_or_default(),
                                    def.year.as_deref().unwrap_or_default()
                                )
                                .trim()
                                .to_string();
                                if label.is_empty() {
                                    number.to_string()
                                } else {
                                    label
                                }
                            }
                        }
                    });
                }
            }
            InlineNode::Emphasis(e) => {
                annotate_citations_inline(&mut e.children, defs, mode, has_bib, seen, order, uses)
            }
            InlineNode::Link(l) => {
                annotate_citations_inline(&mut l.children, defs, mode, has_bib, seen, order, uses)
            }
            InlineNode::Span(s) => {
                annotate_citations_inline(&mut s.children, defs, mode, has_bib, seen, order, uses)
            }
            InlineNode::Extension(e) => {
                annotate_citations_inline(&mut e.children, defs, mode, has_bib, seen, order, uses);
            }
            _ => {}
        }
    }
}

fn inject_references_block(blocks: &mut Vec<BlockNode>) {
    let carrier = BlockNode::Extension(BlockExtension {
        attrs: None,
        name: REFS_BLOCK.to_string(),
        children: Vec::new(),
        summary: None,
        label: None,
    });
    for block in blocks.iter_mut() {
        match block {
            BlockNode::Div(d) if has_class(&d.attrs, "references") => {
                d.children.push(carrier);
                return;
            }
            BlockNode::Admonition(a) if a.kind == "references" => {
                a.children.push(carrier);
                return;
            }
            _ => {}
        }
    }
    blocks.push(carrier);
}

fn has_class(attrs: &Option<Attrs>, class: &str) -> bool {
    attrs
        .as_ref()
        .is_some_and(|attrs| attrs.classes.iter().any(|c| c == class))
}

fn render_refs_list(
    ctx: &RenderContext<'_>,
    mode: CitationMode,
    order: &[String],
    defs: &BTreeMap<String, Def>,
    uses: &BTreeMap<String, usize>,
    has_bib: bool,
) -> String {
    let mut keys = order.to_vec();
    if mode == CitationMode::AuthorDate {
        keys.sort_by(|a, b| {
            let left = defs.get(a).and_then(|d| d.author.as_deref()).unwrap_or(a);
            let right = defs.get(b).and_then(|d| d.author.as_deref()).unwrap_or(b);
            left.cmp(right)
        });
    }
    let tag = if mode == CitationMode::AuthorDate {
        "ul"
    } else {
        "ol"
    };
    let mut out = format!("<{tag} class=\"references\">");
    for key in keys {
        if let Some(def) = defs.get(&key) {
            // A CSL-sourced entry is plain text (escaped); an in-doc def is AST.
            let body = match &def.csl_text {
                Some(text) => escape_text(text),
                None => ctx.render_inlines(&def.entry),
            };
            // Ids come from the per-render document id namespace (extensions
            // contract §2.6): a `<li id>` or back-link target bumped by a
            // collision stays consistent with the in-text citation anchors.
            let mut backlinks = String::new();
            if has_bib {
                let n = uses.get(&key).copied().unwrap_or(0);
                let links: Vec<String> = (1..=n)
                    .map(|m| {
                        format!(
                            "<a href=\"#{}\" class=\"ref-backref\">\u{21a9}</a>",
                            escape_attr(&crate::document_ids::cite_id(&key, m))
                        )
                    })
                    .collect();
                if !links.is_empty() {
                    if !body.is_empty() {
                        backlinks.push(' ');
                    }
                    backlinks.push_str(&links.join(" "));
                }
            }
            out.push('\n');
            out.push_str(&format!(
                "  <li id=\"{}\">{}{}</li>",
                escape_attr(&crate::document_ids::ref_id(&key)),
                body,
                backlinks
            ));
        }
    }
    out.push('\n');
    out.push_str(&format!("</{tag}>"));
    out
}
