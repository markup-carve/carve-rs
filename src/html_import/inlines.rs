use super::footnotes::node_key;
use super::{
    candidate_mark, coalesce, collapse, drop_space_after_hard_break, fallback_image,
    hoist_edge_space, image_alt_content, is_braced_pair_tag, is_flattened_block,
    is_semantic_span_tag, linear_math_text, names_no_destination, needs_separator,
    text_ends_with_layout, unwrapped_content, Importer, CELL_BREAK_FLATTENED,
};
use super::{HtmlImportDiagnosticCode, HtmlImportError, HtmlImportMode, HtmlImportSeverity};
use crate::ast::*;
use crate::escape::{has_denied_url_scheme, is_dangerous_attr_name};
use crate::render::semantic_value_target;
use markup5ever_rcdom::{Handle, NodeData};
use std::rc::Rc;
impl<'a> Importer<'a> {
    pub(super) fn inlines(
        &mut self,
        handles: &[Handle],
        parent: &str,
        depth: usize,
    ) -> Result<Vec<InlineNode>, HtmlImportError> {
        self.inlines_at(handles, None, parent, depth)
    }

    /// The inline walk, with the same index-parallel path override
    /// [`Self::blocks_at`] takes, and for the same reason.
    pub(super) fn inlines_at(
        &mut self,
        handles: &[Handle],
        paths: Option<&[String]>,
        parent: &str,
        depth: usize,
    ) -> Result<Vec<InlineNode>, HtmlImportError> {
        let mut out: Vec<InlineNode> = Vec::new();
        // A BLOCK BOUNDARY IN AN INLINE SLOT SURVIVES ONLY IN THE BYTES (PART
        // 11 §1b): ONE space, merging with layout already on either side, and
        // a block that contributes nothing is not a side (carve-js `inlines`).
        let mut previous_was_block = false;
        let flattening = handles
            .iter()
            .any(|h| Self::tag(h).as_deref().is_some_and(Self::is_block_tag));
        for (i, h) in handles.iter().enumerate() {
            let tag = Self::tag(h).unwrap_or_else(|| {
                // A comment names itself, the same way `child_path` spells it:
                // a row about one has to be findable as a comment rather than
                // read as a text node the document does not have.
                if matches!(&h.data, NodeData::Comment { .. }) {
                    "comment()".into()
                } else {
                    "text()".into()
                }
            });
            let path = match paths {
                Some(given) => given[i].clone(),
                None => format!("{parent}/{tag}[{}]", i + 1),
            };
            let is_block = is_flattened_block(h);
            let mut produced = self.inline(h, &path, depth)?;
            // Layout between flattened blocks is the boundary itself.
            if flattening && Self::inlines_are_blank(&produced) {
                continue;
            }
            let at_boundary = previous_was_block || is_block;
            if at_boundary && needs_separator(&out, &produced) {
                out.push(InlineNode::text(" ".to_string()));
            }
            if at_boundary && text_ends_with_layout(out.last()) {
                if let Some(InlineNode::Text(first)) = produced.first_mut() {
                    first.value = first.value.trim_start_matches(' ').to_string();
                    if first.value.is_empty() {
                        produced.remove(0);
                    }
                }
            }
            if !produced.is_empty() {
                previous_was_block = is_block;
            }
            out.extend(produced);
        }
        let mut out = drop_space_after_hard_break(coalesce(hoist_edge_space(out)));
        if self.opts.mode == HtmlImportMode::Roundtrip {
            for node in &mut out {
                if let InlineNode::Text(text) = node {
                    text.value = collapse(&text.value);
                }
            }
        }
        Ok(out)
    }
    /// An HTML comment in an INLINE position, as the delimited Carve comment
    /// (markup-carve/carve#1709).
    pub(super) fn comment(&mut self, content: &str, path: &str, node: &Handle) -> Vec<InlineNode> {
        let closes_early = content.contains("%}");
        // A blank line lies BETWEEN two newlines; the edge segments sit beside
        // the `{%` and `%}` delimiters (carve-js reads `\n[ \t]*\n`).
        let segments: Vec<&str> = content.split('\n').collect();
        let ends_the_run = segments.len() > 2
            && segments[1..segments.len() - 1]
                .iter()
                .any(|line| line.chars().all(|c| c == ' ' || c == '\t'));
        if closes_early || ends_the_run {
            let why = if closes_early {
                "holds the comment closer"
            } else {
                "holds a blank line"
            };
            self.diag(
                HtmlImportDiagnosticCode::ElementDropped,
                format!(
                    "Dropped an HTML comment: its text {why}, which ends a Carve inline comment early, and the comment is not moved out of the run to make it spellable"
                ),
                HtmlImportSeverity::Warning,
                path,
                node,
            );
            return Vec::new();
        }
        // A pipe-table row is one line, so a line break has no spelling in a
        // cell (markup-carve/carve#2372).
        if self.cell_depth > 0 && content.contains(['\n', '\r']) {
            self.diag(
                HtmlImportDiagnosticCode::ElementDropped,
                "Dropped an HTML comment in a table cell: its text holds a line break, and a table row is one line".into(),
                HtmlImportSeverity::Warning,
                path,
                node,
            );
            return Vec::new();
        }
        // A heading is one line too (markup-carve/carve#2396).
        if self.heading_depth > 0 && content.contains(['\n', '\r']) {
            self.diag(
                HtmlImportDiagnosticCode::ElementDropped,
                "Dropped an HTML comment in a heading: its text holds a line break, and a heading is one line".into(),
                HtmlImportSeverity::Warning,
                path,
                node,
            );
            return Vec::new();
        }
        vec![InlineNode::Comment(Comment {
            block: false,
            delimited: true,
            content: content.to_string(),
            pos: None,
        })]
    }

    pub(super) fn code_text(
        &mut self,
        h: &Handle,
        path: &str,
        depth: usize,
        block: bool,
        language: (Option<&Handle>, Option<&str>),
    ) -> Result<String, HtmlImportError> {
        let (language_wrapper, lang) = language;
        let context = if block { "pre" } else { "code" };
        let description = if block { "a code block" } else { "a code span" };
        enum Frame {
            Node(Handle, String, usize),
            Boundary,
        }
        let mut runs = vec![String::new()];
        let mut pending = Vec::new();
        let push_children =
            |pending: &mut Vec<Frame>, parent: &Handle, path: &str, depth: usize| {
                for (i, child) in parent.children.borrow().iter().enumerate().rev() {
                    pending.push(Frame::Node(
                        child.clone(),
                        Self::child_path(path, child, i),
                        depth + 1,
                    ));
                }
            };
        push_children(&mut pending, h, path, depth);
        while let Some(frame) = pending.pop() {
            let Frame::Node(child, child_path, child_depth) = frame else {
                runs.push(String::new());
                continue;
            };
            self.enter(child_depth)?;
            if let NodeData::Text { contents } = &child.data {
                runs.last_mut().unwrap().push_str(&contents.borrow());
                continue;
            }
            if matches!(child.data, NodeData::Comment { .. }) {
                self.diag(
                    HtmlImportDiagnosticCode::ElementDropped,
                    format!("Dropped a comment inside <{context}>: {description} holds only text"),
                    HtmlImportSeverity::Warning,
                    &child_path,
                    &child,
                );
                continue;
            }
            let Some(tag) = Self::tag(&child) else {
                push_children(&mut pending, &child, &child_path, child_depth);
                continue;
            };
            if Self::is_active_tag(&child) {
                self.diag(
                    HtmlImportDiagnosticCode::ElementDropped,
                    format!("Dropped active <{tag}> element"),
                    HtmlImportSeverity::Warning,
                    &child_path,
                    &child,
                );
                self.charge_subtree(&child, child_depth)?;
                continue;
            }
            let raw_attrs = if let NodeData::Element { attrs, .. } = &child.data {
                attrs.borrow().clone()
            } else {
                Vec::new()
            };
            if !(tag == "span" && raw_attrs.is_empty() || block && tag == "code") {
                let dropped = child.children.borrow().is_empty();
                self.diag(
                    if dropped {
                        HtmlImportDiagnosticCode::ElementDropped
                    } else {
                        HtmlImportDiagnosticCode::ElementUnwrapped
                    },
                    format!(
                        "{} <{tag}> inside <{context}>",
                        if dropped { "Dropped" } else { "Unwrapped" }
                    ),
                    if dropped {
                        HtmlImportSeverity::Warning
                    } else {
                        HtmlImportSeverity::Info
                    },
                    &child_path,
                    &child,
                );
            }
            for attr in raw_attrs {
                if language_wrapper.is_some_and(|wrapper| Rc::ptr_eq(wrapper, &child)) {
                    if let Some(lang) = lang {
                        let name = attr.name.local.as_ref();
                        let value = attr.value.as_ref();
                        let consumed = attr.name.prefix.is_none()
                            && attr.name.ns.as_ref().is_empty()
                            && ((name == "data-lang"
                                && value.trim_matches(|c| {
                                    matches!(c, ' ' | '\t' | '\r' | '\n' | '\u{000c}')
                                }) == lang)
                                || (name == "class"
                                    && !value
                                        .trim_matches(|c| {
                                            matches!(c, ' ' | '\t' | '\r' | '\n' | '\u{000c}')
                                        })
                                        .is_empty()
                                    && value
                                        .split(|c| {
                                            matches!(c, ' ' | '\t' | '\r' | '\n' | '\u{000c}')
                                        })
                                        .filter(|token| !token.is_empty())
                                        .all(|token| {
                                            token == format!("language-{lang}")
                                                || token == format!("lang-{lang}")
                                        })));
                        if consumed {
                            continue;
                        }
                    }
                }
                let name = attr.name.prefix.as_ref().map_or_else(
                    || attr.name.local.to_string(),
                    |prefix| format!("{prefix}:{}", attr.name.local),
                );
                let unsafe_attribute = is_dangerous_attr_name(&name)
                    || ((tag == "a" && name == "href" || tag == "img" && name == "src")
                        && has_denied_url_scheme(&attr.value));
                self.diag(
                    HtmlImportDiagnosticCode::AttributeDropped,
                    format!(
                        "Dropped {name} on <{tag}> inside <{context}>: {description} holds only text"
                    ),
                    if block && unsafe_attribute { HtmlImportSeverity::Warning } else { HtmlImportSeverity::Info },
                    &child_path,
                    &child,
                );
            }
            if is_flattened_block(&child)
                || matches!(
                    tag.as_str(),
                    "address" | "dialog" | "fieldset" | "form" | "hgroup" | "menu" | "search"
                )
            {
                runs.push(String::new());
                pending.push(Frame::Boundary);
            }
            push_children(&mut pending, &child, &child_path, child_depth);
        }
        if !block
            && runs
                .iter()
                .filter(|run| {
                    run.chars()
                        .any(|c| !matches!(c, ' ' | '\t' | '\r' | '\n' | '\u{000c}'))
                })
                .count()
                > 1
        {
            self.diag(
                HtmlImportDiagnosticCode::StructureUnspellable,
                "A code span's value cannot hold the block boundary inside <code>".into(),
                HtmlImportSeverity::Warning,
                path,
                h,
            );
        }
        Ok(runs.concat())
    }

    pub(super) fn code_span(
        &mut self,
        h: &Handle,
        path: &str,
        depth: usize,
        attrs: Option<Attrs>,
    ) -> Result<Vec<InlineNode>, HtmlImportError> {
        let mut value = self.code_text(h, path, depth, false, (None, None))?;
        if self.writing && self.cell_depth > 0 && value.contains(['\r', '\n']) {
            self.diag(
                HtmlImportDiagnosticCode::StructureUnspellable,
                "Flattened a line break in <code> inside a table cell: a table row is one line"
                    .into(),
                HtmlImportSeverity::Warning,
                path,
                h,
            );
            value = value.replace("\r\n", " ").replace(['\r', '\n'], " ");
        }
        if !value.is_empty() {
            return Ok(vec![InlineNode::code(value, attrs)]);
        }
        let pos = Some(candidate_mark(self.empty_code_spans.len()));
        self.empty_code_spans.push((h.clone(), path.to_owned()));
        Ok(vec![InlineNode::Code(Code {
            cell_wraps: None,
            value,
            attrs,
            pos,
        })])
    }

    pub(super) fn inline(
        &mut self,
        h: &Handle,
        path: &str,
        depth: usize,
    ) -> Result<Vec<InlineNode>, HtmlImportError> {
        self.enter(depth)?;
        if let NodeData::Text { contents } = &h.data {
            return Ok(vec![InlineNode::text(collapse(&contents.borrow()))]);
        }
        // AN HTML COMMENT IS A CARVE COMMENT, and this is the INLINE position of
        // it (markup-carve/carve#1709). The block position is `flush_inline_run`.
        //
        // The usual reason this importer drops something is that Carve cannot
        // express the shape. That reason never applied here: Carve HAS comments,
        // so dropping one was a choice to lose bytes the format can hold, in a
        // mode whose whole job is fidelity, made by nobody. A comment renders
        // nothing in either language, so keeping it is invisible in the output
        // and lossless in the source.
        if let NodeData::Comment { contents } = &h.data {
            return Ok(self.comment(contents.as_ref(), path, h));
        }
        // A site the adapter footnote pass recognized. The pass runs before
        // this walk and records the node rather than rewriting the tree into
        // a synthetic element, so nothing here depends on a tag name that
        // real HTML could also spell.
        if let Some((_, note)) = self.footnote_refs.get(&node_key(h)) {
            return Ok(vec![InlineNode::Footnote(note.clone())]);
        }
        let Some(tag) = Self::tag(h) else {
            return Ok(Vec::new());
        };
        if Self::is_active_tag(h) {
            self.diag(
                HtmlImportDiagnosticCode::ElementDropped,
                format!("Dropped active <{tag}> element"),
                HtmlImportSeverity::Warning,
                path,
                h,
            );
            return Ok(Vec::new());
        }
        if self.formula_images.contains(&node_key(h)) {
            self.diag(
                HtmlImportDiagnosticCode::ElementDropped,
                "Dropped <img>: the fallback image of a formula imported as math".into(),
                HtmlImportSeverity::Info,
                path,
                h,
            );
            return Ok(Vec::new());
        }
        if tag == "q" {
            return self.inline_quote(h, path, depth);
        }
        if tag == "math" {
            if let Some(nodes) = self.inline_math(h, path, depth)? {
                return Ok(nodes);
            }
        }
        let attrs = self.attrs(h, path);
        if tag == "code" {
            return self.code_span(h, path, depth, attrs);
        }
        let children = self.inlines(&h.children.borrow(), path, depth + 1)?;
        self.inline_from_children(h, path, &tag, attrs, children)
    }

    /// Recognize footnote pairs, record each reference site for `inline` to
    /// read as a footnote reference, detach the note blocks, and return their
    /// bodies keyed by valid carrier labels or numbered fallbacks.
    /// Navigation ids identify pairs; carrier metadata supplies source labels.
    ///
    /// `heuristic` is the word-processor adapters' licence: with it a mutual
    /// anchor pair alone binds. Without it - `generic` and the editor adapters -
    /// only an anchor the producer MARKED with `role="doc-noteref"` opens a
    /// pair, which is authored DPUB-ARIA semantics rather than a guess, so a
    /// role-less document imports exactly as it did before. That is what lets
    /// Pandoc 2.11+ output import its footnotes without naming an adapter, and
    /// it is what the shared fixtures are written over (carve-rs#1313).
    #[inline(never)]
    pub(super) fn inline_math(
        &mut self,
        h: &Handle,
        path: &str,
        depth: usize,
    ) -> Result<Option<Vec<InlineNode>>, HtmlImportError> {
        // MathML -> `math`, as carve#1210's D6 rules it: a three-tier
        // lookup for TeX the producer already put in the source, and no
        // MathML-to-TeX converter in any engine.
        //
        // THE DECISION IS THE THIRD TIER, and it is a drop rather than a
        // degrade. MathML's children are a token stream, so concatenating
        // them is not a lossy rendering of the equation but a different
        // value: the children of `<math><mfrac><mn>1</mn><mn>2</mn></mfrac>
        // </math>` concatenate to `12`, one half arriving as twelve. A
        // plausible wrong value survives review, where a missing equation
        // and a warning naming it do not. That is the line between math and
        // the embeds at the end of this method: a `<video>`'s children ARE
        // fallback content the author wrote for exactly this case.
        //
        // `roundtrip` keeps the whole element instead, by falling through
        // to the raw arm below. Carve's own HTML spells math as a `<span>`,
        // so a `<math>` reaching that mode is foreign markup by definition
        // and its contract is to preserve it verbatim.
        if let Some((tier, content)) = self.math_tex(h) {
            // The subtree is charged here because the mapping returns
            // without walking it. `max_nodes` and `max_depth` must not
            // depend on which branch an element takes.
            self.charge_subtree(h, depth)?;
            let attrs = self.attrs(h, path);
            // Reported on the tier that SUPPLIED the content, not on which
            // one was available: an annotation holding only whitespace
            // falls through to `alttext`, and reading the presence of the
            // element would make that fall-through the one tier-2 read
            // that says nothing.
            //
            // `encoding-assumed` is the code because the loss is in the
            // OUTPUT: MathML never states what `alttext` holds, so the
            // math node may carry something that is not TeX at all.
            // `element-unwrapped` would describe a structural event the
            // consumer cannot act on (carve#1235).
            if tier == 2 {
                self.diag(
                        HtmlImportDiagnosticCode::EncodingAssumed,
                        "Read <math> through its alttext: MathML does not declare the encoding of alttext, so TeX is assumed".into(),
                        HtmlImportSeverity::Info,
                        path,
                        h,
                    );
            } else if tier == 3 {
                self.diag(
                        HtmlImportDiagnosticCode::EncodingAssumed,
                        "Read <math> through its fallback image's alt: nothing declares the encoding of alt, so TeX is assumed".into(),
                        HtmlImportSeverity::Info,
                        path,
                        h,
                    );
            }
            if let Some(image) = fallback_image(h, &self.sibling_index) {
                if Self::attr(&image, "alt").as_deref().map(str::trim) == Some(content.as_str()) {
                    self.formula_images.insert(node_key(&image));
                }
            }
            return Ok(Some(vec![InlineNode::Math(Math {
                cell_wraps: None,
                attrs,
                display: Self::attr(h, "display").as_deref() == Some("block"),
                content,
                label: None,
                number: None,
                pos: None,
            })]));
        }
        if self.opts.mode != HtmlImportMode::Roundtrip
            || (self.cell_depth > 0 && Self::html(h).contains(['\r', '\n']))
        {
            self.charge_subtree(h, depth)?;
            if self.opts.mode != HtmlImportMode::Roundtrip {
                if let Some(text) = linear_math_text(h) {
                    self.diag(
                            HtmlImportDiagnosticCode::ElementUnwrapped,
                            "Imported <math> as its text: no TeX annotation and no alttext, and its tokens read in order".into(),
                            HtmlImportSeverity::Warning,
                            path,
                            h,
                        );
                    return Ok(Some(vec![InlineNode::text(text)]));
                }
            }
            // No attribute walk on the way out: the element and everything
            // riding on it is gone, and this warning covers all of it.
            self.diag(
                    HtmlImportDiagnosticCode::ElementDropped,
                    "Dropped <math>: no TeX annotation and no alttext, and its children are a token stream, not an equation".into(),
                    HtmlImportSeverity::Warning,
                    path,
                    h,
                );
            return Ok(Some(Vec::new()));
        }
        Ok(None)
    }

    #[inline(never)]
    #[allow(clippy::needless_return, clippy::needless_borrow)]
    pub(super) fn inline_quote(
        &mut self,
        h: &Handle,
        path: &str,
        depth: usize,
    ) -> Result<Vec<InlineNode>, HtmlImportError> {
        let attrs = self.attrs(h, path);
        let (open, close) = if self.quote_depth % 2 == 0 {
            ('\u{201c}', '\u{201d}')
        } else {
            ('\u{2018}', '\u{2019}')
        };
        self.quote_depth += 1;
        let inner = self.inlines(&h.children.borrow(), path, depth + 1);
        self.quote_depth -= 1;
        let mut quoted = vec![InlineNode::text(open.to_string())];
        quoted.extend(inner?);
        quoted.push(InlineNode::text(close.to_string()));
        // An id or a class on the element still has a home, and a span is
        // the one that keeps it without inventing a node for the quotation
        // itself.
        if attrs.is_some() {
            return Ok(vec![InlineNode::Span(Span {
                attrs,
                children: quoted,
                injected: false,
                pos: None,
            })]);
        }
        return Ok(quoted);
    }

    #[inline(never)]
    #[allow(clippy::needless_return, clippy::needless_borrow)]
    pub(super) fn inline_from_children(
        &mut self,
        h: &Handle,
        path: &str,
        tag: &String,
        attrs: Option<Attrs>,
        children: Vec<InlineNode>,
    ) -> Result<Vec<InlineNode>, HtmlImportError> {
        if tag == "a" && Self::reads_xref(h) {
            let node = InlineNode::CrossRef(CrossRef {
                target: Self::attr(h, "href").unwrap()[1..].to_owned(),
                href: None,
                pos: None,
            });
            return Ok(if attrs.is_some() {
                vec![InlineNode::Span(Span {
                    attrs,
                    children: vec![node],
                    injected: false,
                    pos: None,
                })]
            } else {
                vec![node]
            });
        }
        if tag == "span" && Self::reads_caption_number(h) {
            return Ok(vec![InlineNode::CaptionNumber(CaptionNumber {
                number: None,
                pos: None,
            })]);
        }
        if tag == "span" && Self::reads_substitution(h) {
            if let [InlineNode::CriticDelete(old), InlineNode::CriticInsert(new)] = &children[..] {
                return Ok(vec![InlineNode::CriticSubstitute(CriticSubstitute {
                    old: old.children.clone(),
                    new: new.children.clone(),
                    pos: None,
                })]);
            }
        }
        if tag == "span" {
            // AFTER the children have been walked, which is what keeps the
            // budget honest for free: `inlines` has already charged every node
            // under this element against `max_nodes` and `max_depth`, so which
            // arm the span takes cannot change what the limits see. The
            // `<math>` arm above has to call `charge_subtree` by hand for the
            // same reason - it returns without walking - and a recognition that
            // read text recursively BEFORE charging would let crafted HTML
            // reach the stack ahead of the limit meant to stop it.
            if let Some(math) = Self::carve_math(h, attrs.as_ref()) {
                return Ok(vec![InlineNode::Math(math)]);
            }
        }
        // An empty pair such as `{**}` reads back as text, and an element the
        // HTML leaves empty shows a reader nothing, so it goes without a row.
        // Its attributes can still matter (an `id` is a link target), and an
        // empty span is spellable, so they move onto one.
        if children.is_empty() && is_braced_pair_tag(tag.as_str()) {
            return Ok(attrs
                .map(|attrs| {
                    vec![InlineNode::Span(Span {
                        attrs: Some(attrs),
                        children: Vec::new(),
                        injected: false,
                        pos: None,
                    })]
                })
                .unwrap_or_default());
        }
        let emphasis = |kind| {
            InlineNode::Emphasis(Emphasis {
                attrs: attrs.clone(),
                kind,
                children: children.clone(),
                pos: None,
            })
        };
        let node = match tag.as_str() {
            "em" | "i" => emphasis(EmphasisKind::Italic),
            "strong" | "b" => emphasis(EmphasisKind::Strong),
            // `<s>` and `<strike>` genuinely ARE strike - `~x~` is what Carve
            // spells them with - so they stay here. `<del>` does not: it has an
            // exact node of its own, one arm down (carve-rs#1223).
            "s" | "strike" => emphasis(EmphasisKind::Strike),
            "ins" => InlineNode::CriticInsert(CriticInsert {
                attrs,
                children,
                pos: None,
            }),
            "del" => InlineNode::CriticDelete(CriticDelete {
                attrs,
                children,
                pos: None,
            }),
            "u" => emphasis(EmphasisKind::Underline),
            "mark" => emphasis(EmphasisKind::Highlight),
            "sub" => emphasis(EmphasisKind::Sub),
            "sup" => emphasis(EmphasisKind::Super),
            // PART 9 §25's sink would blank this destination, so it is one
            // Carve cannot carry: imported like an empty one, reported as a
            // dropped attribute (markup-carve/carve#2254).
            "a" if Self::attr(h, "href").is_some_and(|v| has_denied_url_scheme(&v)) => {
                self.diag(
                    HtmlImportDiagnosticCode::AttributeDropped,
                    "Dropped href with a denied URL scheme on <a>".into(),
                    HtmlImportSeverity::Warning,
                    path,
                    h,
                );
                return Ok(unwrapped_content(
                    Self::attrs_with_title(h, attrs),
                    children,
                ));
            }
            "img" if Self::attr(h, "src").is_some_and(|v| has_denied_url_scheme(&v)) => {
                self.diag(
                    HtmlImportDiagnosticCode::AttributeDropped,
                    "Dropped src with a denied URL scheme on <img>".into(),
                    HtmlImportSeverity::Warning,
                    path,
                    h,
                );
                return Ok(unwrapped_content(
                    Self::attrs_with_title(h, attrs),
                    image_alt_content(h),
                ));
            }
            "a" if names_no_destination(Self::attr(h, "href").as_deref()) => {
                self.diag(
                    HtmlImportDiagnosticCode::ElementUnwrapped,
                    "Unwrapped <a> with no destination".into(),
                    HtmlImportSeverity::Info,
                    path,
                    h,
                );
                return Ok(unwrapped_content(
                    Self::attrs_with_title(h, attrs),
                    children,
                ));
            }
            // AN IMAGE'S CONTENT IS ITS ALTERNATIVE TEXT: that is what every
            // target with no image shows for it, and what a browser shows for
            // one it cannot load, so it is the text this document's reader was
            // going to see either way.
            "img" if names_no_destination(Self::attr(h, "src").as_deref()) => {
                self.diag(
                    HtmlImportDiagnosticCode::ElementUnwrapped,
                    "Unwrapped <img> with no source".into(),
                    HtmlImportSeverity::Info,
                    path,
                    h,
                );
                return Ok(unwrapped_content(
                    Self::attrs_with_title(h, attrs),
                    image_alt_content(h),
                ));
            }
            // PART 10 §13a `data-carve-autolink`: an autolink is its own node
            // type, and `<a href>` alone cannot be told from `[text](url)`
            // whose text happens to equal the destination.
            "a" if Self::reads_autolink(h) => InlineNode::AutoLink(AutoLink {
                attrs,
                href: Self::destination_attr(h, "href"),
                text: Self::text(h),
                pos: None,
            }),
            "a" => InlineNode::Link(Link {
                attrs,
                href: Self::destination_attr(h, "href"),
                title: Self::attr(h, "title"),
                children,
                // PART 10 §13a `data-carve-ref`: the label the reference
                // resolved by. Without it the destination is inlined at every
                // use site and the definition line is gone.
                ref_label: self.reference_label(
                    h,
                    &Self::destination_attr(h, "href"),
                    Self::attr(h, "title"),
                    path,
                ),
                raw_ref: None,
                from_crossref: false,
                from_heading_reference: false,
                pos: None,
                resolved_empty_reference: false,
            }),
            "img" => InlineNode::Image(Image {
                attrs,
                src: Self::destination_attr(h, "src"),
                alt: Self::attr(h, "alt").unwrap_or_default(),
                title: Self::attr(h, "title"),
                // PART 10 §13a `data-carve-ref`, as on the anchor above.
                ref_label: self.reference_label(
                    h,
                    &Self::destination_attr(h, "src"),
                    Self::attr(h, "title"),
                    path,
                ),
                raw_ref: None,
                pos: None,
                resolved_empty_reference: false,
            }),
            "br" => {
                // A hard break has NO ATTRIBUTE SLOT: `Break` carries only a
                // position, so `<br clear="all">` has nowhere to put it. Naming
                // the drop is all this can do; staying quiet would be the
                // silent loss carve-rs#1060 is about.
                self.report_unplaceable_attrs(
                    h,
                    attrs,
                    "br",
                    "a hard break carries no attributes",
                    path,
                );
                if self.cell_depth > 0 {
                    self.unspellable.push((
                        h.clone(),
                        path.to_owned(),
                        CELL_BREAK_FLATTENED.into(),
                        HtmlImportDiagnosticCode::StructureUnspellable,
                    ));
                }
                InlineNode::hard_break()
            }
            // PART 10 §13a `data-carve-escaped`: the span holds ONE escaped
            // character and its presence says the author wrote the backslash,
            // so the import writes it back instead of letting the character
            // reparse as markup.
            "span" if Self::reads_escaped(h, self.opts.mode).is_some() => {
                InlineNode::EscapedText(EscapedText {
                    value: Self::reads_escaped(h, self.opts.mode).unwrap_or_default(),
                    pos: None,
                })
            }
            // PART 10 §13a `data-carve-raw`: the span carries the format and
            // holds the content, which is how a `=latex` run survives HTML.
            "span" if Self::reads_raw_format(h, self.opts.mode).is_some() => {
                let format = Self::reads_raw_format(h, self.opts.mode).unwrap_or_default();
                // Decode text payloads; serialize element children as payload markup.
                let holds_elements = h
                    .children
                    .borrow()
                    .iter()
                    .any(|child| matches!(child.data, NodeData::Element { .. }));
                let content = if holds_elements {
                    h.children.borrow().iter().map(Self::html).collect()
                } else {
                    Self::text(h)
                };
                InlineNode::RawInline(RawInline {
                    cell_wraps: None,
                    format,
                    content,
                    injected: false,
                    pos: None,
                })
            }
            "span" if attrs.is_some() => InlineNode::Span(Span {
                attrs,
                children,
                injected: false,
                pos: None,
            }),
            // PART 9 §10, carve#1140. The seven names the compact span
            // attribute spells exactly. They belong here, with the other
            // elements Carve can express, rather than behind a mode branch:
            // `roundtrip` raw-preserves only what Carve CANNOT express, so
            // placing them here maps them in all three modes by construction.
            name if is_semantic_span_tag(name) => {
                let mut attrs = attrs.unwrap_or_default();
                // A name that carries no value, or an element that omits the
                // attribute it would carry it in, gives the bare boolean.
                let value = semantic_value_target(name)
                    .and_then(|source| Self::attr(h, source))
                    .unwrap_or_default();
                attrs.key_values.insert(name.to_string(), value);
                InlineNode::Span(Span {
                    attrs: Some(attrs),
                    children,
                    injected: false,
                    pos: None,
                })
            }
            // CARVE-P10-021: a generic-span carrier no branch above could read
            // is consumed in every mode, so roundtrip unwraps it as safe does
            // instead of keeping the reserved name in raw HTML.
            "span"
                if self.opts.mode == HtmlImportMode::Roundtrip
                    && (Self::carrier(h, crate::roundtrip::RAW).is_some()
                        || Self::carrier(h, crate::roundtrip::ESCAPED).is_some()) =>
            {
                return Ok(children);
            }
            _ if self.opts.mode == HtmlImportMode::Roundtrip
                && !(self.cell_depth > 0 && Self::html(h).contains(['\r', '\n'])) =>
            {
                self.keep_raw(h, path, &tag);
                InlineNode::RawInline(RawInline {
                    cell_wraps: None,
                    format: "html".into(),
                    content: Self::html(h),
                    injected: false,
                    pos: None,
                })
            }
            _ => {
                let unwrapped = self.report_unsupported_element(h, tag.as_str(), path);
                // The inline twin of the block unwrap above: `<small>`,
                // `<bdi dir="rtl">`, `<bdo>`, `<ruby>`, `<button>`, `<label>`
                // keep their children and nothing else. An element that had no
                // children says so instead, or the report would call the same
                // element dropped in one row and unwrapped in the next.
                self.report_unplaceable_attrs(
                    h,
                    attrs,
                    tag.as_str(),
                    if unwrapped {
                        "the element was unwrapped and has no node to carry it"
                    } else {
                        "the empty element was dropped and has no node to carry it"
                    },
                    path,
                );
                return Ok(children);
            }
        };
        let mut node = node;
        if let InlineNode::Emphasis(Emphasis { pos, .. })
        | InlineNode::CriticInsert(CriticInsert { pos, .. })
        | InlineNode::CriticDelete(CriticDelete { pos, .. }) = &mut node
        {
            *pos = Some(candidate_mark(self.braced_kind_spans.len()));
            self.braced_kind_spans.push((h.clone(), path.to_owned()));
        }
        Ok(vec![node])
    }
}
