//! Profile AST transform (port of carve-php's `ProfileFilter` / carve-js'
//! `profile-filter.ts`).
//!
//! Walks a parsed [`Document`] and, for every disallowed node, applies the
//! profile's [`DisallowedAction`]:
//!   - `ToText`: replace the node with its rendered text content (default)
//!   - `Strip`: remove node + subtree
//!   - `Error`: collect violations and return [`ProfileViolationError`]
//!
//! Also enforces `max_nesting` (block-container depth) and applies the link
//! policy (URL gating + rel attribute injection) in the same pass.
//!
//! carve-rs' AST is enum variants with heterogeneous child fields. The walk
//! mirrors carve-php's `getChildren()` / `removeChild()` / `replaceChildNode()`
//! semantics: each `Vec<BlockNode>` / `Vec<InlineNode>` is filtered in place.
//! Depth increments on every descend (block and inline alike), matching
//! carve-php, which includes inline children in `getChildren()`.

use crate::ast::*;
use crate::extension::SmartTypographyMode;
use crate::profile::{
    canonical_block_type, canonical_inline_type, DisallowedAction, LinkPolicy, Profile,
    ProfileViolation, ProfileViolationError,
};

/// Result of a profile transform.
pub struct ProfileFilterResult {
    pub doc: Document,
    pub violations: Vec<ProfileViolation>,
}

/// Apply a profile to a parsed [`Document`], returning the filtered document
/// and any violations. `base_host` is used by the link policy to distinguish
/// internal vs external links. When the profile's action is
/// [`DisallowedAction::Error`] and a violation occurs, returns
/// [`ProfileViolationError`].
pub fn apply_profile(
    doc: Document,
    profile: &Profile,
    base_host: Option<&str>,
) -> Result<ProfileFilterResult, ProfileViolationError> {
    apply_profile_with_typography(
        doc,
        profile,
        base_host,
        crate::extension::SmartTypographyMode::Glyph,
    )
}

/// [`apply_profile`], told which smart-typography mode the document is being
/// rendered under.
///
/// `to_text` replaces a denied node with "its rendered text content" (profiles
/// §"Actions on a disallowed node"), and this pass runs once on the parsed AST
/// before any renderer, so it is the pass that decides what that text says. The
/// smart-typography switch is DOCUMENT-GLOBAL and applies to EVERY target
/// (PART 9 §19): with it off/source, "every trigger character survives as the
/// ASCII the author typed". Resolving the glyph here would settle that question
/// in a subsystem the switch cannot reach, so a degraded `[x -- y](...)` came
/// out `x - y` even when the caller asked for the source run.
///
/// The GUARANTEE the profile makes stays renderer-agnostic either way: which
/// nodes are denied, stripped or degraded does not depend on the mode. Only the
/// spelling of the text a degrade substitutes does.
pub fn apply_profile_with_typography(
    doc: Document,
    profile: &Profile,
    base_host: Option<&str>,
    smart: crate::extension::SmartTypographyMode,
) -> Result<ProfileFilterResult, ProfileViolationError> {
    let mut filter = ProfileFilter {
        profile,
        base_host,
        smart,
        violations: Vec::new(),
    };
    let mut doc = doc;
    // `frontmatter` lives on the `Document` root (`frontmatter_raw` /
    // `frontmatter`), not as a walkable `BlockNode`, so the child walk below
    // never encounters it. Check it here instead, against the same
    // vocabulary and the same violation reporting every other block type
    // gets. Denial always STRIPS regardless of `disallowed_action`: the
    // content is document metadata, and degrading it to text (as `ToText`
    // would for any other node) would leak `title:`/`author:` values into the
    // rendered body. carve-php treats it exactly like a comment for this
    // reason (carve issue 422).
    //
    // Checked against EITHER field, not just `frontmatter_raw`: `parse()`
    // always sets both together, but `Document`'s fields are public, so a
    // programmatically built document can carry a populated `frontmatter`
    // map with no `frontmatter_raw`. `render_carve` serializes the map
    // independently of the raw form, so leaving it behind on that path would
    // still leak the metadata a denial is supposed to remove.
    if (doc.frontmatter_raw.is_some() || !doc.frontmatter.is_empty())
        && !profile.is_type_allowed_on("frontmatter", true)
    {
        filter.record("frontmatter", "element_not_allowed")?;
        doc.frontmatter_raw = None;
        doc.frontmatter = std::collections::BTreeMap::new();
    }
    filter.filter_blocks(&mut doc.children, 0)?;
    // Footnote definitions live in a separate map (keyed by label) but every
    // renderer emits them, so a denied node inside a referenced definition
    // must be filtered too. carve-php keeps definitions in the tree; we mirror
    // that by walking each definition's block list at depth 1.
    let labels: Vec<String> = doc.footnote_defs.keys().cloned().collect();
    for label in labels {
        if let Some(mut blocks) = doc.footnote_defs.remove(&label) {
            filter.filter_blocks(&mut blocks, 1)?;
            doc.footnote_defs.insert(label, blocks);
        }
    }
    // The two node types this engine keeps on the Document rather than in the
    // tree. profiles.md lists `frontmatter` and `footnote` in the Block
    // vocabulary, so a profile can name them - but the walk above only reaches
    // `children`, so naming either did nothing at all: no violation, no change
    // (carve#422). A silent no-op is the specific failure a normative
    // vocabulary exists to prevent, and it is worst here, because frontmatter
    // is exactly the content a host restricting untrusted input wants gone.
    //
    // Denial REMOVES rather than degrades: both render nothing, so there is no
    // text form to fall back to. Rendered output is unchanged either way; what
    // changes is the tree and the violation report.
    if (doc.frontmatter_raw.is_some() || !doc.frontmatter.is_empty())
        && !profile.is_type_allowed_on("frontmatter", true)
    {
        filter.record("frontmatter", "element_not_allowed")?;
        doc.frontmatter_raw = None;
        doc.frontmatter.clear();
    }
    if !doc.footnote_defs.is_empty() && !profile.is_type_allowed_on("footnote", true) {
        filter.record("footnote", "element_not_allowed")?;
        doc.footnote_defs.clear();
    }
    cleanup_blocks(&mut doc.children);
    for blocks in doc.footnote_defs.values_mut() {
        cleanup_blocks(blocks);
    }
    // The numbers were assigned during the parse, BEFORE this filter ran, so a
    // denied definition or a dropped reference leaves the survivors numbered for
    // a document that no longer exists. The HTML renderer numbers off the
    // filtered tree, so without this the published AST claims a numbered
    // reference where the rendered output has literal text (carve-rs#641).
    //
    // Unconditional, unlike carve-js, which has to gate the same call: its
    // numbering pass carries the renderers' depth ceiling and its filter does
    // not, so renumbering every profiled document there would refuse deep trees
    // that filter fine. This pass has no ceiling of its own, and `parse` already
    // runs it over every document that can reach this point.
    let _ = crate::render::collect_footnotes(&mut doc, false);
    Ok(ProfileFilterResult {
        doc,
        violations: filter.violations,
    })
}

struct ProfileFilter<'a> {
    profile: &'a Profile,
    base_host: Option<&'a str>,
    /// See [`apply_profile_with_typography`]: the mode a degrade's substituted
    /// text is spelled in.
    smart: crate::extension::SmartTypographyMode,
    violations: Vec<ProfileViolation>,
}

impl ProfileFilter<'_> {
    fn filter_blocks(
        &mut self,
        blocks: &mut Vec<BlockNode>,
        depth: usize,
    ) -> Result<(), ProfileViolationError> {
        let mut i = 0;
        while i < blocks.len() {
            let max_nesting = self.profile.max_nesting();
            let canonical = canonical_block_type(&blocks[i]);

            // Nesting check first (matches carve-php order).
            if max_nesting > 0 && depth > max_nesting {
                let ty = canonical.unwrap_or("unknown").to_string();
                match self.handle_block_violation(&blocks[i], &ty, "max_nesting_exceeded")? {
                    Some(node) => blocks[i] = node,
                    None => {
                        blocks.remove(i);
                        continue;
                    }
                }
                i += 1;
                continue;
            }

            // An unmapped type is outside the vocabulary, NOT disallowed: it
            // resolves through the same three steps under its own name.
            let allowed = self
                .profile
                .is_type_allowed_on(canonical.unwrap_or("unknown"), true);
            if !allowed {
                let ty = canonical.unwrap_or("unknown").to_string();
                match self.handle_block_violation(&blocks[i], &ty, "element_not_allowed")? {
                    Some(node) => blocks[i] = node,
                    None => {
                        blocks.remove(i);
                        continue;
                    }
                }
                i += 1;
                continue;
            }

            // Block images: gate the URL with the link policy.
            if let BlockNode::BlockImage(img) = &blocks[i] {
                if let Some(policy) = self.profile.link_policy() {
                    if !policy.is_url_allowed(&img.src, self.base_host) {
                        let img = img.clone();
                        match self.handle_image_block_violation(&img)? {
                            Some(node) => blocks[i] = node,
                            None => {
                                blocks.remove(i);
                                continue;
                            }
                        }
                        i += 1;
                        continue;
                    }
                }
            }

            self.recurse_block(&mut blocks[i], depth + 1)?;
            i += 1;
        }
        Ok(())
    }

    /// Filter the inline/block children nested inside an (allowed) block node.
    fn recurse_block(
        &mut self,
        block: &mut BlockNode,
        depth: usize,
    ) -> Result<(), ProfileViolationError> {
        match block {
            BlockNode::Heading(h) => self.filter_inlines(&mut h.children, depth)?,
            BlockNode::Paragraph(p) => self.filter_inlines(&mut p.children, depth)?,
            BlockNode::CitationDefinition(d) => self.filter_inlines(&mut d.children, depth)?,
            BlockNode::CodeBlock(_)
            | BlockNode::RawBlock(_)
            | BlockNode::Comment(_)
            | BlockNode::ThematicBreak(_)
            | BlockNode::LinkReferenceDefinition(_)
            | BlockNode::AbbreviationDef(_)
            | BlockNode::BlockImage(_) => {}
            BlockNode::List(list) => {
                // A `list_item` is a node in carve-php / carve-js: it is checked
                // against the allow/deny list and the nesting limit at `depth`
                // (the list was checked at `depth - 1`). carve-rs models items as
                // a struct field, not a BlockNode, so we check them inline. On a
                // violation we flatten the item's text into a paragraph and keep
                // it inside the list-item wrapper (a deliberate structural
                // divergence from carve-php's bare-paragraph output, needed
                // because carve-rs' list renderer assumes ListItem children).
                let max_nesting = self.profile.max_nesting();
                let item_denied = !self.profile.is_type_allowed("list_item");
                let mut i = 0;
                while i < list.items.len() {
                    let over_nesting = max_nesting > 0 && depth > max_nesting;
                    if over_nesting || item_denied {
                        let reason = if over_nesting {
                            "max_nesting_exceeded"
                        } else {
                            "element_not_allowed"
                        };
                        self.record("list_item", reason)?;
                        match self.profile.disallowed_action() {
                            DisallowedAction::Strip => {
                                list.items.remove(i);
                                continue;
                            }
                            DisallowedAction::ToText => {
                                let text = list.items[i]
                                    .children
                                    .iter()
                                    .map(|n| extract_block_text(n, self.smart))
                                    .filter(|s| !s.is_empty())
                                    .collect::<Vec<_>>()
                                    .join(" ");
                                if text.is_empty() {
                                    list.items.remove(i);
                                    continue;
                                }
                                list.items[i] = ListItem {
                                    attrs: None,
                                    checked: None,
                                    children: vec![BlockNode::Paragraph(Paragraph {
                                        attrs: None,
                                        children: text_with_breaks(&text),
                                        ..Default::default()
                                    })],
                                    pos: None,
                                };
                                i += 1;
                                continue;
                            }
                            DisallowedAction::Error => unreachable!(),
                        }
                    }
                    self.filter_blocks(&mut list.items[i].children, depth + 1)?;
                    i += 1;
                }
            }
            BlockNode::BlockQuote(bq) => {
                self.filter_blocks(&mut bq.children, depth)?;
            }
            BlockNode::Table(table) => {
                // `table_row` / `table_cell` are nodes in carve-php / carve-js:
                // each is checked against the allow/deny list (and is denied by
                // default in an allowlist that omits it). carve-rs models rows
                // and cells as struct fields, so we check them inline. Presets
                // never deny these; this only affects custom profiles. To keep
                // the table renderer's row/cell structure valid we re-wrap a
                // to_text survivor rather than emit a bare paragraph.
                let row_denied = !self.profile.is_type_allowed("table_row");
                let cell_denied = !self.profile.is_type_allowed("table_cell");
                let mut r = 0;
                while r < table.rows.len() {
                    if row_denied {
                        self.record("table_row", "element_not_allowed")?;
                        match self.profile.disallowed_action() {
                            DisallowedAction::Strip => {
                                table.rows.remove(r);
                                continue;
                            }
                            DisallowedAction::ToText => {
                                // Flatten the row to a single text cell.
                                let text = row_text(&table.rows[r], self.smart);
                                table.rows[r] = TableRow {
                                    cells: vec![text_cell(&text)],
                                    attrs: None,
                                    pos: None,
                                };
                                r += 1;
                                continue;
                            }
                            DisallowedAction::Error => unreachable!(),
                        }
                    }
                    let row = &mut table.rows[r];
                    let mut c = 0;
                    while c < row.cells.len() {
                        if cell_denied {
                            self.record("table_cell", "element_not_allowed")?;
                            match self.profile.disallowed_action() {
                                DisallowedAction::Strip => {
                                    row.cells.remove(c);
                                    continue;
                                }
                                DisallowedAction::ToText => {
                                    let text: String = row.cells[c]
                                        .children
                                        .iter()
                                        .map(|n| extract_inline_text(n, self.smart))
                                        .collect();
                                    row.cells[c].children = vec![InlineNode::text(text)];
                                    c += 1;
                                    continue;
                                }
                                DisallowedAction::Error => unreachable!(),
                            }
                        }
                        self.filter_inlines(&mut row.cells[c].children, depth + 1)?;
                        c += 1;
                    }
                    r += 1;
                }
                if let Some(caption) = &mut table.caption {
                    self.filter_inlines(caption, depth)?;
                }
            }
            BlockNode::Admonition(adm) => {
                if let Some(title) = &mut adm.title {
                    self.filter_inlines(title, depth)?;
                }
                self.filter_blocks(&mut adm.children, depth)?;
            }
            BlockNode::Div(div) => self.filter_blocks(&mut div.children, depth)?,
            BlockNode::LineBlock(lb) => self.filter_blocks(&mut lb.children, depth)?,
            BlockNode::DefinitionList(dl) => {
                for item in &mut dl.items {
                    for term in &mut item.terms {
                        self.filter_inlines(term, depth + 1)?;
                    }
                    for def in &mut item.definitions {
                        self.filter_blocks(def, depth + 1)?;
                    }
                }
            }
            BlockNode::Figure(fig) => {
                self.filter_inlines(&mut fig.caption, depth + 1)?;
                self.recurse_figure_target(fig, depth + 1)?;
            }
            BlockNode::FigureGroup(group) => {
                if let Some(caption) = &mut group.caption {
                    self.filter_inlines(caption, depth + 1)?;
                }
                self.filter_blocks(&mut group.children, depth)?;
            }
            BlockNode::Extension(ext) => self.filter_blocks(&mut ext.children, depth)?,
        }
        Ok(())
    }

    fn recurse_figure_target(
        &mut self,
        fig: &mut Figure,
        depth: usize,
    ) -> Result<(), ProfileViolationError> {
        // The figure target is a single-node field; carve-php treats it as an
        // ordinary child, so a denied target must be filtered. Wrap it in a
        // one-element block list and re-use the block machinery.
        let target_block: BlockNode = match &fig.target {
            FigureTarget::Image(img) => BlockNode::BlockImage(img.clone()),
            FigureTarget::BlockQuote(bq) => BlockNode::BlockQuote(bq.clone()),
            FigureTarget::Table(t) => BlockNode::Table(t.clone()),
            FigureTarget::CodeBlock(c) => BlockNode::CodeBlock(c.clone()),
            FigureTarget::Paragraph(p) => BlockNode::Paragraph(p.clone()),
        };
        let mut wrapper = vec![target_block];
        self.filter_blocks(&mut wrapper, depth)?;
        match wrapper.into_iter().next() {
            Some(BlockNode::BlockImage(img)) => fig.target = FigureTarget::Image(img),
            Some(BlockNode::BlockQuote(bq)) => fig.target = FigureTarget::BlockQuote(bq),
            Some(BlockNode::Table(t)) => fig.target = FigureTarget::Table(t),
            Some(BlockNode::CodeBlock(c)) => fig.target = FigureTarget::CodeBlock(c),
            Some(BlockNode::Paragraph(p)) => fig.target = FigureTarget::Paragraph(p),
            // Replaced into a different node (to_text paragraph) or stripped:
            // collapse the figure target into a paragraph fallback so the
            // figure still renders something coherent.
            Some(other) => {
                let text = extract_block_text(&other, self.smart);
                fig.target = FigureTarget::Paragraph(Paragraph {
                    attrs: None,
                    children: text_with_breaks(&text),
                    ..Default::default()
                });
            }
            None => {
                fig.target = FigureTarget::Paragraph(Paragraph {
                    attrs: None,
                    children: Vec::new(),
                    ..Default::default()
                });
            }
        }
        Ok(())
    }

    fn filter_inlines(
        &mut self,
        inlines: &mut Vec<InlineNode>,
        depth: usize,
    ) -> Result<(), ProfileViolationError> {
        let mut i = 0;
        while i < inlines.len() {
            let max_nesting = self.profile.max_nesting();
            let canonical = canonical_inline_type(&inlines[i]);

            if max_nesting > 0 && depth > max_nesting {
                let ty = canonical.unwrap_or("unknown").to_string();
                match self.handle_inline_violation(&inlines[i], &ty, "max_nesting_exceeded")? {
                    Some(node) => inlines[i] = node,
                    None => {
                        inlines.remove(i);
                        continue;
                    }
                }
                i += 1;
                continue;
            }

            // An unmapped type is outside the vocabulary, NOT disallowed: it
            // resolves through the same three steps under its own name.
            let allowed = self
                .profile
                .is_type_allowed_on(canonical.unwrap_or("unknown"), false);
            if !allowed {
                let ty = canonical.unwrap_or("unknown").to_string();
                match self.handle_inline_violation(&inlines[i], &ty, "element_not_allowed")? {
                    Some(node) => inlines[i] = node,
                    None => {
                        inlines.remove(i);
                        continue;
                    }
                }
                i += 1;
                continue;
            }

            // Link URL gating.
            if let Some(policy) = self.profile.link_policy() {
                if let Some(url) = link_url(&inlines[i]) {
                    if !policy.is_url_allowed(&url, self.base_host) {
                        match self.handle_link_violation(&inlines[i])? {
                            Some(node) => inlines[i] = node,
                            None => {
                                inlines.remove(i);
                                continue;
                            }
                        }
                        i += 1;
                        continue;
                    }
                    apply_rel_attributes(&mut inlines[i], policy);
                }
                // Inline image URL gating.
                if let InlineNode::Image(img) = &inlines[i] {
                    if !policy.is_url_allowed(&img.src, self.base_host) {
                        let img = img.clone();
                        match self.handle_image_inline_violation(&img)? {
                            Some(node) => inlines[i] = node,
                            None => {
                                inlines.remove(i);
                                continue;
                            }
                        }
                        i += 1;
                        continue;
                    }
                }
            }

            self.recurse_inline(&mut inlines[i], depth + 1)?;
            i += 1;
        }
        Ok(())
    }

    fn recurse_inline(
        &mut self,
        node: &mut InlineNode,
        depth: usize,
    ) -> Result<(), ProfileViolationError> {
        match node {
            InlineNode::Emphasis(e) => self.filter_inlines(&mut e.children, depth)?,
            InlineNode::Link(l) => self.filter_inlines(&mut l.children, depth)?,
            InlineNode::Span(s) => self.filter_inlines(&mut s.children, depth)?,
            InlineNode::Extension(e) => self.filter_inlines(&mut e.children, depth)?,
            InlineNode::CriticInsert(c) => self.filter_inlines(&mut c.children, depth)?,
            InlineNode::CriticDelete(c) => self.filter_inlines(&mut c.children, depth)?,
            InlineNode::Footnote(f) => {
                if let Some(inline) = &mut f.inline {
                    self.filter_inlines(inline, depth)?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    // ---- violation handling ----
    //
    // Each `handle_*` returns:
    //   - `Ok(Some(node))` => replace the node with `node` (to_text).
    //   - `Ok(None)` => remove the node (strip / empty to_text).
    //   - `Err(_)` => the Error action short-circuited (collected violations).

    fn handle_block_violation(
        &mut self,
        node: &BlockNode,
        canonical: &str,
        reason: &str,
    ) -> Result<Option<BlockNode>, ProfileViolationError> {
        self.record(canonical, reason)?;
        match self.profile.disallowed_action() {
            DisallowedAction::Strip => Ok(None),
            DisallowedAction::ToText => self.block_to_text(node, canonical),
            // Error already short-circuited inside `record`.
            DisallowedAction::Error => unreachable!(),
        }
    }

    fn handle_inline_violation(
        &mut self,
        node: &InlineNode,
        canonical: &str,
        reason: &str,
    ) -> Result<Option<InlineNode>, ProfileViolationError> {
        self.record(canonical, reason)?;
        match self.profile.disallowed_action() {
            DisallowedAction::Strip => Ok(None),
            DisallowedAction::ToText => self.inline_to_text(node, canonical),
            DisallowedAction::Error => unreachable!(),
        }
    }

    fn handle_link_violation(
        &mut self,
        node: &InlineNode,
    ) -> Result<Option<InlineNode>, ProfileViolationError> {
        let canonical = canonical_inline_type(node).unwrap_or("link");
        self.record(canonical, "link_not_allowed")?;
        match self.profile.disallowed_action() {
            DisallowedAction::Strip => Ok(None),
            DisallowedAction::ToText => self.inline_to_text(node, canonical),
            DisallowedAction::Error => unreachable!(),
        }
    }

    /// A URL-denied block image: to_text wraps `[img: alt]` in a paragraph.
    fn handle_image_block_violation(
        &mut self,
        img: &Image,
    ) -> Result<Option<BlockNode>, ProfileViolationError> {
        self.record("image", "image_not_allowed")?;
        match self.profile.disallowed_action() {
            DisallowedAction::Strip => Ok(None),
            DisallowedAction::ToText => Ok(Some(BlockNode::Paragraph(Paragraph {
                attrs: None,
                children: text_with_breaks(&image_text(img)),
                ..Default::default()
            }))),
            DisallowedAction::Error => unreachable!(),
        }
    }

    /// A URL-denied inline image: to_text replaces it with `[img: alt]` text.
    fn handle_image_inline_violation(
        &mut self,
        img: &Image,
    ) -> Result<Option<InlineNode>, ProfileViolationError> {
        self.record("image", "image_not_allowed")?;
        match self.profile.disallowed_action() {
            DisallowedAction::Strip => Ok(None),
            DisallowedAction::ToText => Ok(Some(InlineNode::text(image_text(img)))),
            DisallowedAction::Error => unreachable!(),
        }
    }

    /// Convert a disallowed block node to its text replacement. Returns
    /// `Ok(None)` only for a node that provably renders nothing (a comment);
    /// any other node is degraded to text, never deleted. When the node's
    /// payload sits outside `extract_block_text`'s reach and the extraction
    /// comes back empty, that is a missing extractor arm, not "no content" -
    /// record a `to_text_yielded_nothing` violation and substitute the
    /// literal marker `[<canonical>]` rather than silently dropping the node.
    fn block_to_text(
        &mut self,
        node: &BlockNode,
        canonical: &str,
    ) -> Result<Option<BlockNode>, ProfileViolationError> {
        // Nodes that render NOTHING where they were written are removed rather
        // than degraded: there is no text to substitute, so degrading them means
        // putting something on the page the author never wrote there. The
        // violation is still recorded by the caller, so the denial is not silent.
        //
        // A comment is never visible content.
        //
        // The two DEFINITION kinds are the same shape, and both were reaching
        // the diagnostic path instead (carve-rs#645). An abbreviation definition
        // found something to extract - its expansion - so denying it published
        // `HyperText` as a paragraph. A link reference definition found nothing,
        // so it took the missing-extractor branch and printed the engine's own
        // marker, `[link_reference_definition]`, onto the page. profiles.md names
        // the two together: the definition line renders nothing in HTML, and the
        // `link`, `image` or `abbreviation` it FEEDS is the node a profile denies.
        if matches!(
            node,
            BlockNode::Comment(_)
                | BlockNode::AbbreviationDef(_)
                | BlockNode::LinkReferenceDefinition(_)
                | BlockNode::CitationDefinition(_)
        ) {
            return Ok(None);
        }
        let mut text = extract_block_text(node, self.smart);
        if text.is_empty() {
            self.record(canonical, "to_text_yielded_nothing")?;
            text = format!("[{canonical}]");
        }
        Ok(Some(BlockNode::Paragraph(Paragraph {
            attrs: None,
            children: text_with_breaks(&text),
            ..Default::default()
        })))
    }

    /// Convert a disallowed inline node to its text replacement. No inline
    /// node renders nothing by definition (unlike `Comment`/`Frontmatter` on
    /// the block side), so this never deletes a node: an extractor that
    /// comes back empty gets the `[<canonical>]` marker, same as
    /// `block_to_text`.
    fn inline_to_text(
        &mut self,
        node: &InlineNode,
        canonical: &str,
    ) -> Result<Option<InlineNode>, ProfileViolationError> {
        let mut text = extract_inline_text(node, self.smart);
        if text.is_empty() {
            self.record(canonical, "to_text_yielded_nothing")?;
            text = format!("[{canonical}]");
        }
        Ok(Some(InlineNode::text(text)))
    }

    /// Record a violation; for the Error action, short-circuit with the error.
    fn record(&mut self, canonical: &str, reason: &str) -> Result<(), ProfileViolationError> {
        let reason_description = self.profile.reason_disallowed(canonical);
        self.violations.push(ProfileViolation {
            node_type: canonical.to_string(),
            reason: reason.to_string(),
            reason_description,
        });
        if self.profile.disallowed_action() == DisallowedAction::Error {
            return Err(ProfileViolationError {
                violations: self.violations.clone(),
            });
        }
        Ok(())
    }
}

/// The URL of a link-shaped inline node, if any.
fn link_url(node: &InlineNode) -> Option<String> {
    match node {
        InlineNode::Link(l) => Some(l.href.clone()),
        InlineNode::AutoLink(a) => Some(a.href.clone()),
        _ => None,
    }
}

/// Inject the policy's rel attributes into a link node's attribute set.
fn apply_rel_attributes(node: &mut InlineNode, policy: &LinkPolicy) {
    let rel_attrs = policy.rel_attributes();
    if rel_attrs.is_empty() {
        return;
    }
    let attrs_slot = match node {
        InlineNode::Link(l) => &mut l.attrs,
        // An autolink has no attribute slot in the AST; skip (carve-php's
        // applyRelAttributes only runs on Link nodes too).
        _ => return,
    };
    let attrs = attrs_slot.get_or_insert_with(Attrs::default);
    let existing = attrs.key_values.get("rel").cloned().unwrap_or_default();
    let mut parts: Vec<String> = if existing.is_empty() {
        Vec::new()
    } else {
        existing.split(' ').map(|s| s.to_string()).collect()
    };
    for rel in rel_attrs {
        if !parts.contains(rel) {
            parts.push(rel.clone());
        }
    }
    attrs.key_values.insert("rel".to_string(), parts.join(" "));
    if !attrs.order.is_empty()
        && !attrs
            .order
            .iter()
            .any(|s| matches!(s, AttrSlot::Key(k) if k == "rel"))
    {
        attrs.order.push(AttrSlot::Key("rel".to_string()));
    }
}

// ---- to_text conversion ----
//
// See `ProfileFilter::block_to_text` / `ProfileFilter::inline_to_text` above;
// they need `&mut self` to record the `to_text_yielded_nothing` violation
// when an extractor comes back empty for a node that does not provably
// render nothing.

/// Build inline nodes from content, converting `\n` to hard breaks.
fn text_with_breaks(content: &str) -> Vec<InlineNode> {
    let lines: Vec<&str> = content.split('\n').collect();
    let last = lines.len().saturating_sub(1);
    let mut out = Vec::new();
    for (idx, line) in lines.iter().enumerate() {
        if !line.is_empty() {
            out.push(InlineNode::text(line.to_string()));
        }
        if idx < last {
            out.push(InlineNode::hard_break());
        }
    }
    out
}

/// Join a table row's cell text the way `extract_block_text` renders a table
/// row (cells separated by " | ").
fn row_text(row: &TableRow, smart: SmartTypographyMode) -> String {
    row.cells
        .iter()
        .map(|c| {
            c.children
                .iter()
                .map(|n| extract_inline_text(n, smart))
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join(" | ")
}

/// A plain body table cell holding a single text node.
fn text_cell(text: &str) -> TableCell {
    TableCell {
        header: false,
        span: None,
        align: None,
        valign: None,
        attrs: None,
        children: vec![InlineNode::text(text.to_string())],
        // A cell this filter synthesizes has no source of its own.
        pos: None,
    }
}

fn image_text(img: &Image) -> String {
    if img.alt.is_empty() {
        "[img]".to_string()
    } else {
        format!("[img: {}]", img.alt)
    }
}

/// Render a block node to source-flavored plain text (matches carve-php's
/// `extractTextContent`), so to_text output matches byte-for-byte.
fn extract_block_text(node: &BlockNode, smart: SmartTypographyMode) -> String {
    match node {
        // The definition line renders nothing, so it contributes no text.
        BlockNode::LinkReferenceDefinition(_) | BlockNode::CitationDefinition(_) => String::new(),
        BlockNode::Heading(h) => {
            let prefix = "#".repeat(h.level as usize) + " ";
            let text: String = h
                .children
                .iter()
                .map(|n| extract_inline_text(n, smart))
                .collect();
            prefix + &text
        }
        BlockNode::CodeBlock(c) => {
            if c.content.contains('\n') {
                format!("```\n{}\n```", c.content)
            } else {
                format!("`{}`", c.content)
            }
        }
        BlockNode::Table(t) => {
            let mut rows = Vec::new();
            for row in &t.rows {
                let cells: Vec<String> = row
                    .cells
                    .iter()
                    .map(|c| {
                        c.children
                            .iter()
                            .map(|n| extract_inline_text(n, smart))
                            .collect()
                    })
                    .collect();
                rows.push(cells.join(" | "));
            }
            rows.join("\n")
        }
        BlockNode::BlockQuote(bq) => {
            let mut paras = Vec::new();
            for child in &bq.children {
                let text = extract_block_text(child, smart);
                if !text.is_empty() {
                    paras.push(format!("> {text}"));
                }
            }
            paras.join("\n")
        }
        BlockNode::DefinitionList(dl) => {
            let mut parts = Vec::new();
            for item in &dl.items {
                for term in &item.terms {
                    let t: String = term.iter().map(|n| extract_inline_text(n, smart)).collect();
                    if !t.is_empty() {
                        parts.push(t);
                    }
                }
                for def in &item.definitions {
                    let ds: Vec<String> = def
                        .iter()
                        .map(|n| extract_block_text(n, smart))
                        .filter(|s| !s.is_empty())
                        .collect();
                    if !ds.is_empty() {
                        parts.push(format!("- {}", ds.join(" ")));
                    }
                }
            }
            parts.join("\n")
        }
        BlockNode::List(list) => {
            let mut items = Vec::new();
            let mut index = list.start.unwrap_or(1);
            for item in &list.items {
                let t: String = item
                    .children
                    .iter()
                    .map(|n| extract_block_text(n, smart))
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>()
                    .join(" ");
                if !t.is_empty() {
                    let marker = if list.ordered {
                        format!("{index}. ")
                    } else {
                        "- ".to_string()
                    };
                    items.push(format!("{marker}{t}"));
                    index += 1;
                }
            }
            items.join("\n")
        }
        BlockNode::ThematicBreak(_) => "---".to_string(),
        BlockNode::RawBlock(r) => r.content.clone(),
        BlockNode::Comment(_) => String::new(),
        BlockNode::BlockImage(img) => image_text(img),
        BlockNode::Paragraph(p) => p
            .children
            .iter()
            .map(|n| extract_inline_text(n, smart))
            .collect(),
        BlockNode::Admonition(adm) => block_children_join(&adm.children, smart),
        BlockNode::Div(div) => block_children_join(&div.children, smart),
        BlockNode::LineBlock(lb) => block_children_join(&lb.children, smart),
        BlockNode::Extension(ext) => block_children_join(&ext.children, smart),
        BlockNode::FigureGroup(group) => {
            let mut parts: Vec<String> = group
                .children
                .iter()
                .map(|child| extract_block_text(child, smart))
                .filter(|text| !text.is_empty())
                .collect();
            if let Some(caption) = &group.caption {
                let caption: String = caption
                    .iter()
                    .map(|n| extract_inline_text(n, smart))
                    .collect();
                if !caption.is_empty() {
                    parts.push(caption);
                }
            }
            parts.join("\n")
        }
        BlockNode::Figure(fig) => {
            let target = match &fig.target {
                FigureTarget::Image(img) => image_text(img),
                FigureTarget::BlockQuote(bq) => {
                    extract_block_text(&BlockNode::BlockQuote(bq.clone()), smart)
                }
                FigureTarget::Table(t) => extract_block_text(&BlockNode::Table(t.clone()), smart),
                FigureTarget::CodeBlock(c) => {
                    extract_block_text(&BlockNode::CodeBlock(c.clone()), smart)
                }
                FigureTarget::Paragraph(p) => p
                    .children
                    .iter()
                    .map(|n| extract_inline_text(n, smart))
                    .collect(),
            };
            let caption: String = fig
                .caption
                .iter()
                .map(|n| extract_inline_text(n, smart))
                .collect();
            [target, caption]
                .into_iter()
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join(" ")
        }
        BlockNode::AbbreviationDef(a) => a.expansion.clone(),
    }
}

fn block_children_join(children: &[BlockNode], smart: SmartTypographyMode) -> String {
    children
        .iter()
        .map(|n| extract_block_text(n, smart))
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Render an inline node to source-flavored plain text.
fn extract_inline_text(node: &InlineNode, smart: SmartTypographyMode) -> String {
    match node {
        InlineNode::Text(t) => t.value.clone(),
        InlineNode::EscapedText(t) => t.value.clone(),
        InlineNode::SmartPunctuation(s) => {
            // The document-global switch decides this, not the extractor
            // (PART 9 §19). Resolving the glyph here put the answer out of
            // every renderer's reach; see `apply_profile_with_typography`.
            if smart == SmartTypographyMode::Source {
                s.value.clone()
            } else {
                smart_punctuation_glyph(s).to_string()
            }
        }
        InlineNode::Code(c) => c.value.clone(),
        InlineNode::Math(m) => m.content.clone(),
        InlineNode::RawInline(r) => r.content.clone(),
        InlineNode::LiteralInline(l) => l.content.clone(),
        InlineNode::SoftBreak(_) => " ".to_string(),
        InlineNode::HardBreak(_) => "\n".to_string(),
        InlineNode::Image(img) => image_text(img),
        InlineNode::Mention(m) => format!("@{}", m.user),
        InlineNode::Tag(t) => format!("#{}", t.name),
        InlineNode::Abbreviation(a) => a.abbr.clone(),
        InlineNode::Symbol(e) => format!(":{}:", e.name),
        InlineNode::Link(l) => l
            .children
            .iter()
            .map(|n| extract_inline_text(n, smart))
            .collect(),
        InlineNode::AutoLink(a) => a.href.clone(),
        InlineNode::Footnote(f) => match &f.inline {
            // Inline footnote: join its inline content.
            Some(inline) => inline
                .iter()
                .map(|n| extract_inline_text(n, smart))
                .collect(),
            // Reference: `[^label]`. carve-rs stores the label in `id`.
            None => format!("[^{}]", f.id.clone().unwrap_or_default()),
        },
        // Inline containers concatenate their children.
        InlineNode::Emphasis(e) => e
            .children
            .iter()
            .map(|n| extract_inline_text(n, smart))
            .collect(),
        InlineNode::Span(s) => s
            .children
            .iter()
            .map(|n| extract_inline_text(n, smart))
            .collect(),
        InlineNode::Extension(e) => e
            .children
            .iter()
            .map(|n| extract_inline_text(n, smart))
            .collect(),
        InlineNode::CriticInsert(c) => c
            .children
            .iter()
            .map(|n| extract_inline_text(n, smart))
            .collect(),
        InlineNode::CriticDelete(c) => c
            .children
            .iter()
            .map(|n| extract_inline_text(n, smart))
            .collect(),
        // Both texts, matching carve-php and carve-js. Returning only the new
        // one silently dropped the wording the author replaced.
        InlineNode::CriticSubstitute(c) => format!("{}{}", c.old_text, c.new_text),
        InlineNode::Comment(_) => String::new(),
        InlineNode::CriticComment(_) => String::new(),
        InlineNode::CrossRef(c) => c.target.clone(),
        InlineNode::CaptionNumber(_) => String::new(),
        // Tier-2 citation node: its source-flavored text is the verbatim `[...]`.
        InlineNode::CitationGroup(g) => g.raw.clone(),
    }
}

// ---- empty-container cleanup (mirrors carve-php) ----

fn cleanup_blocks(blocks: &mut Vec<BlockNode>) {
    let mut i = 0;
    while i < blocks.len() {
        cleanup_block_children(&mut blocks[i]);
        if is_empty_block(&blocks[i]) {
            blocks.remove(i);
            continue;
        }
        i += 1;
    }
}

fn cleanup_inlines(inlines: &mut Vec<InlineNode>) {
    let mut i = 0;
    while i < inlines.len() {
        cleanup_inline_children(&mut inlines[i]);
        if is_empty_inline(&inlines[i]) {
            inlines.remove(i);
            continue;
        }
        i += 1;
    }
}

fn cleanup_block_children(block: &mut BlockNode) {
    match block {
        BlockNode::Heading(h) => cleanup_inlines(&mut h.children),
        BlockNode::Paragraph(p) => cleanup_inlines(&mut p.children),
        BlockNode::List(list) => {
            let mut i = 0;
            while i < list.items.len() {
                cleanup_blocks(&mut list.items[i].children);
                if list.items[i].children.is_empty() {
                    list.items.remove(i);
                    continue;
                }
                i += 1;
            }
        }
        BlockNode::BlockQuote(bq) => {
            cleanup_blocks(&mut bq.children);
        }
        BlockNode::Table(t) => {
            for row in &mut t.rows {
                for cell in &mut row.cells {
                    cleanup_inlines(&mut cell.children);
                }
            }
            if let Some(caption) = &mut t.caption {
                cleanup_inlines(caption);
            }
        }
        BlockNode::Admonition(adm) => {
            if let Some(title) = &mut adm.title {
                cleanup_inlines(title);
            }
            cleanup_blocks(&mut adm.children);
        }
        BlockNode::Div(div) => cleanup_blocks(&mut div.children),
        BlockNode::LineBlock(lb) => cleanup_blocks(&mut lb.children),
        BlockNode::DefinitionList(dl) => {
            for item in &mut dl.items {
                for def in &mut item.definitions {
                    cleanup_blocks(def);
                }
            }
        }
        BlockNode::Figure(fig) => cleanup_inlines(&mut fig.caption),
        BlockNode::FigureGroup(group) => {
            if let Some(caption) = &mut group.caption {
                cleanup_inlines(caption);
            }
            cleanup_blocks(&mut group.children);
        }
        BlockNode::Extension(ext) => cleanup_blocks(&mut ext.children),
        _ => {}
    }
}

fn cleanup_inline_children(node: &mut InlineNode) {
    match node {
        InlineNode::Emphasis(e) => cleanup_inlines(&mut e.children),
        InlineNode::Link(l) => cleanup_inlines(&mut l.children),
        InlineNode::Span(s) => cleanup_inlines(&mut s.children),
        InlineNode::Extension(e) => cleanup_inlines(&mut e.children),
        InlineNode::CriticInsert(c) => cleanup_inlines(&mut c.children),
        InlineNode::CriticDelete(c) => cleanup_inlines(&mut c.children),
        InlineNode::Footnote(f) => {
            if let Some(inline) = &mut f.inline {
                cleanup_inlines(inline);
            }
        }
        _ => {}
    }
}

/// Whether a block node is now an empty container that should be removed.
fn is_empty_block(node: &BlockNode) -> bool {
    match node {
        // A definition always carries a label and a destination, so it is never
        // the empty node this asks about.
        BlockNode::LinkReferenceDefinition(_) => false,
        // A citation definition always carries its key, so an entry with no
        // inline content is still the line the author wrote (PART 12 section 18
        // requires the field, not its contents).
        BlockNode::CitationDefinition(_) => false,
        // Content-bearing nodes are non-empty if they have content.
        BlockNode::CodeBlock(c) => c.content.is_empty(),
        BlockNode::RawBlock(r) => r.content.is_empty(),
        BlockNode::Comment(c) => c.content.is_empty(),
        // Structural / self-contained nodes preserved even when "empty".
        BlockNode::ThematicBreak(_) => false,
        BlockNode::BlockImage(_) => false,
        BlockNode::AbbreviationDef(_) => false,
        BlockNode::Heading(h) => all_inlines_empty(&h.children),
        BlockNode::Paragraph(p) => all_inlines_empty(&p.children),
        BlockNode::List(list) => list.items.is_empty(),
        BlockNode::BlockQuote(bq) => bq.children.is_empty(),
        BlockNode::Table(t) => {
            // A table cell is structural and kept even when empty, so a table
            // is empty only when it has no rows.
            t.rows.is_empty()
        }
        BlockNode::Admonition(adm) => adm.children.is_empty(),
        BlockNode::Div(div) => div.children.is_empty(),
        BlockNode::LineBlock(lb) => lb.children.is_empty(),
        BlockNode::DefinitionList(dl) => dl.items.is_empty(),
        BlockNode::Figure(_) => false,
        // The panels div is unconditional (PART 9 SS4c), so an emptied group
        // still renders a coherent shell only when it truly has nothing left:
        // no children and no caption.
        BlockNode::FigureGroup(group) => group.children.is_empty() && group.caption.is_none(),
        BlockNode::Extension(ext) => ext.children.is_empty(),
    }
}

fn is_empty_inline(node: &InlineNode) -> bool {
    match node {
        InlineNode::Text(t) => t.value.is_empty(),
        InlineNode::Comment(_) => false,
        InlineNode::EscapedText(t) => t.value.is_empty(),
        InlineNode::SmartPunctuation(_) => false,
        InlineNode::Code(c) => c.value.is_empty(),
        InlineNode::Math(m) => m.content.is_empty(),
        InlineNode::RawInline(r) => r.content.is_empty(),
        InlineNode::LiteralInline(l) => l.content.is_empty(),
        // Self-contained / structural inline nodes are never "empty containers".
        InlineNode::Image(_)
        | InlineNode::Mention(_)
        | InlineNode::Tag(_)
        | InlineNode::Symbol(_)
        | InlineNode::Abbreviation(_)
        | InlineNode::CrossRef(_)
        | InlineNode::CaptionNumber(_)
        | InlineNode::CitationGroup(_)
        | InlineNode::SoftBreak(_)
        | InlineNode::HardBreak(_)
        | InlineNode::AutoLink(_)
        | InlineNode::CriticSubstitute(_)
        | InlineNode::CriticComment(_) => false,
        InlineNode::Emphasis(e) => all_inlines_empty(&e.children),
        InlineNode::Link(l) => all_inlines_empty(&l.children),
        InlineNode::Span(s) => all_inlines_empty(&s.children),
        InlineNode::Extension(e) => all_inlines_empty(&e.children),
        InlineNode::CriticInsert(c) => all_inlines_empty(&c.children),
        InlineNode::CriticDelete(c) => all_inlines_empty(&c.children),
        InlineNode::Footnote(f) => match &f.inline {
            Some(inline) => all_inlines_empty(inline),
            None => false,
        },
    }
}

fn all_inlines_empty(inlines: &[InlineNode]) -> bool {
    inlines.iter().all(is_empty_inline)
}
