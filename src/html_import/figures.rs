use super::{dom_text_is_layout_only, is_layout_space, trim_edge_whitespace, Importer};
use super::{HtmlImportDiagnosticCode, HtmlImportError, HtmlImportMode, HtmlImportSeverity};
use crate::ast::*;
use crate::profile::ADMONITION_TIER1_KINDS;
use markup5ever_rcdom::{Handle, NodeData};
impl<'a> Importer<'a> {
    /// `<dl>` to `definition_list`.
    ///
    /// Before this the element had no branch at all, so it fell through to the
    /// unwrapping path: every term and every definition became inline content
    /// of one paragraph, and a two-entry glossary imported as a single run of
    /// words with no separator between a term and its own definition. The
    /// diagnostics said `element-unwrapped`, which is true of the element and
    /// says nothing about the document losing its structure.
    ///
    /// HTML5 gives `dl` two content models: `dt`/`dd` as direct children, or
    /// one `div` per group wrapping them. Both spell the same list, so the
    /// wrapper is unwrapped transparently - one level, which is the only level
    /// HTML5 allows. A `div` nested inside a wrapper is not a group; its terms
    /// stay unread rather than this importer inventing a flattening the source
    /// did not say.
    ///
    /// A group is a run of terms followed by a run of definitions, which is the
    /// same grouping the parser builds from `::` and `:` lines, so an imported
    /// list and a hand-written one produce the same tree.
    #[inline(never)]
    pub(super) fn definition_list(
        &mut self,
        h: &Handle,
        path: &str,
        depth: usize,
        attrs: Option<Attrs>,
    ) -> Result<Vec<BlockNode>, HtmlImportError> {
        let entries = self.definition_list_collect(h, path, depth)?;
        self.definition_list_entries(h, path, attrs, entries)
    }
    #[inline(never)]
    pub(super) fn definition_list_collect(
        &mut self,
        h: &Handle,
        path: &str,
        depth: usize,
    ) -> Result<Vec<(String, Handle, usize, String)>, HtmlImportError> {
        let mut entries: Vec<(String, Handle, usize, String)> = Vec::new();
        for (i, child) in h.children.borrow().iter().enumerate() {
            let Some(tag) = Self::tag(child) else {
                continue;
            };
            self.enter(depth + 1)?;
            match tag.as_str() {
                "dt" | "dd" => {
                    let p = Self::child_path(path, child, i);
                    entries.push((tag, child.clone(), depth + 1, p));
                }
                "div" => {
                    let p = format!("{path}/div[{}]", i + 1);
                    // The wrapper groups rows for styling and carries nothing
                    // the `::` form spells, so it goes. Its attributes are read
                    // anyway: that is what reports a `style` or an event
                    // handler on it, and what makes an id or a class it does
                    // carry a stated loss rather than a silent one.
                    if self.attrs(child, &p).is_some() {
                        self.diag(
                            HtmlImportDiagnosticCode::AttributeDropped,
                            "Dropped the attributes of a <div> group wrapper in <dl>; a definition-list group has no slot for them".into(),
                            HtmlImportSeverity::Info,
                            &p,
                            child,
                        );
                    }
                    for (j, wrapped) in child.children.borrow().iter().enumerate() {
                        let Some(inner) = Self::tag(wrapped) else {
                            continue;
                        };
                        self.enter(depth + 2)?;
                        if inner == "dt" || inner == "dd" {
                            let inner_path = Self::child_path(&p, wrapped, j);
                            entries.push((inner, wrapped.clone(), depth + 2, inner_path));
                        } else {
                            // One level unwraps, which is the only level HTML5
                            // allows, so a `div` inside the wrapper is not a
                            // group. It is still reported: a doubly-wrapped
                            // list otherwise imports to nothing at all, which
                            // is the silent shape this row exists to remove.
                            self.dropped_in_dl(wrapped, &inner, &format!("{p}/{inner}[{}]", j + 1));
                        }
                    }
                }
                other => self.dropped_in_dl(child, other, &format!("{path}/{other}[{}]", i + 1)),
            }
        }

        Ok(entries)
    }

    #[inline(never)]
    pub(super) fn definition_list_entries(
        &mut self,
        h: &Handle,
        path: &str,
        attrs: Option<Attrs>,
        entries: Vec<(String, Handle, usize, String)>,
    ) -> Result<Vec<BlockNode>, HtmlImportError> {
        let mut before: Vec<BlockNode> = Vec::new();
        let mut items: Vec<DefinitionItem> = Vec::new();
        let mut terms: Vec<DefinitionTerm> = Vec::new();
        let mut definitions: Vec<DefinitionDef> = Vec::new();
        let mut dropped_term = false;
        // Every entry is written, so a `<dl>` imports and writes back as ONE
        // list: no term acquires the next entry's description and nothing
        // splits. See `render_definition_list`.
        for (tag, node, node_depth, entry_path) in entries.iter() {
            let p = entry_path.clone();
            if tag == "dt" {
                if !definitions.is_empty() {
                    items.push(DefinitionItem {
                        terms: std::mem::take(&mut terms),
                        definitions: std::mem::take(&mut definitions),
                        pos: None,
                    });
                }
                let term_attrs = self.attrs(node, &p);
                let children = trim_edge_whitespace(self.inlines(
                    &node.children.borrow(),
                    &p,
                    node_depth + 1,
                )?);
                if self.writing
                    && children.is_empty()
                    && self.opts.mode != HtmlImportMode::Roundtrip
                {
                    self.diag(
                        HtmlImportDiagnosticCode::ElementDropped,
                        "Dropped an empty definition term: it has no Carve source spelling".into(),
                        HtmlImportSeverity::Warning,
                        &p,
                        node,
                    );
                    self.report_unplaceable_attrs(
                        node,
                        term_attrs,
                        "dt",
                        "an empty term has no Carve source spelling",
                        &p,
                    );
                    dropped_term = true;
                    continue;
                }
                dropped_term = false;
                terms.push(DefinitionTerm {
                    attrs: term_attrs,
                    children,
                    pos: None,
                });
                continue;
            }
            if terms.is_empty() && definitions.is_empty() {
                if dropped_term && !items.is_empty() {
                    let children = self.blocks(&node.children.borrow(), &p, node_depth + 1)?;
                    let attrs = self.attrs(node, &p);
                    self.diag(
                        HtmlImportDiagnosticCode::ElementUnwrapped,
                        "A definition whose empty term was dropped is attached to the preceding group to keep document order".into(),
                        HtmlImportSeverity::Warning,
                        &p,
                        node,
                    );
                    items.last_mut().unwrap().definitions.push(DefinitionDef {
                        attrs,
                        children,
                        pos: None,
                    });
                    continue;
                }
                // A `dd` before any `dt` is not valid HTML5, but a sliced-up
                // editor export produces one. It cannot become a group: a
                // definition line under an empty `::` re-parses as a paragraph,
                // so writing one would trade a silent loss for a corrupt
                // document. The content is emitted ahead of the list instead,
                // which keeps every word and stays valid Carve, and the
                // diagnostic states the role that did not survive.
                self.diag(
                    HtmlImportDiagnosticCode::ElementUnwrapped,
                    if dropped_term {
                        "A definition whose empty term was dropped kept its content as blocks ahead of the definition list".into()
                    } else {
                        "A <dd> with no <dt> before it kept its content but not its role: it is emitted as blocks ahead of the definition list".into()
                    },
                    HtmlImportSeverity::Warning,
                    &p,
                    node,
                );
                // AND IT LOSES ITS ATTRIBUTES WITH ITS ROLE (carve-rs#1257).
                // Every other `<dd>` puts them on the `DefinitionDef` below;
                // this one has no node to carry them, and reading only its
                // CHILDREN meant an `onclick` here went the way the table
                // caption's did - stripped, with the row above naming the role
                // and nothing naming the attribute. Same category, second site.
                let carried = self.attrs(node, &p);
                self.report_unplaceable_attrs(
                    node,
                    carried,
                    "dd",
                    "a <dd> with no <dt> keeps its content as blocks, and blocks ahead of the list have no slot for it",
                    &p,
                );
                before.extend(self.blocks(&node.children.borrow(), &p, node_depth + 1)?);
                continue;
            }
            let children = self.blocks(&node.children.borrow(), &p, node_depth + 1)?;
            // A `<dd>` that writes nothing takes the `{empty}` sentinel, which
            // reads back as a description holding no blocks, so the entry
            // survives the round trip and owes no row (markup-carve/carve#1827).
            definitions.push(DefinitionDef {
                attrs: self.attrs(node, &p),
                children,
                pos: None,
            });
        }
        if !terms.is_empty() || !definitions.is_empty() {
            items.push(DefinitionItem {
                terms,
                definitions,
                pos: None,
            });
        }
        if self.writing
            && self.opts.mode == HtmlImportMode::Roundtrip
            && items
                .iter()
                .any(|item| item.terms.iter().any(|term| term.children.is_empty()))
        {
            self.keep_raw(h, path, "dl");
            return Ok(vec![BlockNode::RawBlock(RawBlock {
                attrs: None,
                format: "html".into(),
                content: Self::html(h),
                pos: None,
            })]);
        }
        if items.is_empty() {
            // No list is written, so nothing carries the `<dl>`'s attributes.
            for name in attrs.as_ref().map(Self::attr_names).unwrap_or_default() {
                self.diag(
                    HtmlImportDiagnosticCode::AttributeDropped,
                    format!(
                        "Dropped {name} on <dl>: a definition list holding no entry is not written"
                    ),
                    HtmlImportSeverity::Warning,
                    path,
                    h,
                );
            }
            return Ok(before);
        }
        before.push(BlockNode::DefinitionList(DefinitionList {
            attrs,
            items,
            // The importer reads the `<dd>` wrappers it was given, and a
            // description that already holds a block says its own looseness -
            // so nothing here is the SPELLED fact §17 L7's field records.
            loose: false,
            pos: None,
        }));
        Ok(before)
    }

    pub(super) fn dropped_in_dl(&mut self, node: &Handle, tag: &str, path: &str) {
        self.diag(
            HtmlImportDiagnosticCode::ElementDropped,
            format!("Dropped <{tag}> inside <dl>: only <dt>, <dd> and a single <div> group wrapper are definition-list content"),
            HtmlImportSeverity::Warning,
            path,
            node,
        );
    }
    /// The element's FIRST class, the slot this engine's structural classes
    /// lead from (`carve-figure-group`, `carve-figure-panel`).
    pub(super) fn first_class(handle: &Handle) -> Option<String> {
        Self::attr(handle, "class")?
            .split_whitespace()
            .next()
            .map(str::to_string)
    }

    /// The container an `<aside>` or `<div>` was RENDERED FROM, rebuilt.
    pub(super) fn container_from(tag: &str, attrs: &Option<Attrs>) -> Option<String> {
        let classes = attrs.as_ref().map(|a| a.classes.as_slice()).unwrap_or(&[]);
        let kind = match tag {
            // The class PAIR is what marks a rendered callout. A bare `<aside>`
            // is somebody else's sidebar and keeps the unwrap it has always had.
            "aside" => {
                if !classes.iter().any(|c| c == "admonition") {
                    return None;
                }
                classes
                    .iter()
                    .find(|c| {
                        c.as_str() != "admonition" && ADMONITION_TIER1_KINDS.contains(&c.as_str())
                    })
                    .cloned()?
            }
            "div" => classes.first().cloned()?,
            _ => return None,
        };
        // The writer's own rule, not a copy of it: a class a fence opener cannot
        // spell (`-2col`, `my.class`) would be written after the colons and read
        // back as a paragraph, so such an element keeps the generic `Div` node
        // where the class survives as a class.
        crate::render_carve::is_container_kind(&kind).then_some(kind)
    }

    /// Drop a structural class the renderer injected; what remains is what the
    /// author wrote. `None` when nothing else was carried.
    pub(super) fn without_structural_class(attrs: Option<Attrs>, class: &str) -> Option<Attrs> {
        let mut attrs = attrs?;
        attrs.classes.retain(|c| c != class);
        if attrs.classes.is_empty() && attrs.id.is_none() && attrs.key_values.is_empty() {
            return None;
        }
        Some(attrs)
    }

    /// A CAPTION ELEMENT's inlines, charged and diagnosed like any other
    /// element - `<figcaption>` on a figure, `<caption>` on a table.
    pub(super) fn caption_inlines(
        &mut self,
        h: &Handle,
        path: &str,
        depth: usize,
        tag: &str,
    ) -> Result<Vec<InlineNode>, HtmlImportError> {
        self.enter(depth)?;
        let carried = self.attrs(h, path);
        self.report_unplaceable_attrs(
            h,
            carried,
            tag,
            "a caption line carries no attributes",
            path,
        );
        self.inlines(&h.children.borrow(), path, depth + 1)
    }

    /// Whether an inline run carries nothing a reader would see.
    pub(super) fn inlines_are_blank(nodes: &[InlineNode]) -> bool {
        nodes.iter().all(|n| match n {
            InlineNode::Text(t) => t.value.chars().all(is_layout_space),
            _ => false,
        })
    }

    /// Report attributes that survived [`Self::attrs`] but have nowhere to go.
    pub(super) fn has_content_to_unwrap(h: &Handle) -> bool {
        for child in h.children.borrow().iter() {
            match &child.data {
                NodeData::Element { name, .. } => {
                    if !matches!(
                        name.local.as_ref(),
                        "script" | "style" | "template" | "noscript"
                    ) {
                        return true;
                    }
                }
                NodeData::Text { contents } => {
                    if Self::holds_more_than_layout(&contents.borrow()) {
                        return true;
                    }
                }
                NodeData::Comment { contents } if Self::holds_more_than_layout(contents) => {
                    return true;
                }
                _ => {}
            }
        }
        false
    }

    /// An element the import drops whole, whatever it holds.
    pub(super) fn is_active_tag(h: &Handle) -> bool {
        Self::tag(h)
            .is_some_and(|tag| matches!(tag.as_str(), "script" | "style" | "template" | "noscript"))
    }

    /// Whether a run of text is anything other than ASCII layout whitespace.
    pub(super) fn holds_more_than_layout(text: &str) -> bool {
        text.chars()
            .any(|c| !matches!(c, ' ' | '\t' | '\n' | '\r' | '\u{0c}'))
    }

    /// The row for an element this importer has no mapping for, unwrapped or
    /// dropped by what it carried (markup-carve/carve#1738). Returns whether it
    /// was unwrapped, which is what the attribute rows below it say next.
    ///
    /// EVERY ARM THAT WRITES THE GENERAL ROW, which is what keeps the three
    /// engines answering alike. The row is written from a sectioning wrapper
    /// and from the arm that catches everything with no mapping at all, and the
    /// same tag reaches a different one of them in each engine - a `<form>`
    /// takes this engine's inline arm and carve-js's block arm - so a rule
    /// applied to one arm would have fixed thirty-two shapes and broken seven.
    ///
    /// A `<figure>` is NOT one of them. It has its own rows and its own rulings
    /// (markup-carve/carve#1716, markup-carve/carve#1723), and reaches neither
    /// arm.
    pub(super) fn report_unsupported_element(&mut self, h: &Handle, tag: &str, path: &str) -> bool {
        if Self::has_content_to_unwrap(h) {
            self.diag(
                HtmlImportDiagnosticCode::ElementUnwrapped,
                format!("Unwrapped unsupported <{tag}> element"),
                HtmlImportSeverity::Info,
                path,
                h,
            );

            return true;
        }
        self.diag(
            HtmlImportDiagnosticCode::ElementDropped,
            format!("Dropped empty <{tag}> element"),
            HtmlImportSeverity::Warning,
            path,
            h,
        );

        false
    }

    pub(super) fn report_unplaceable_attrs(
        &mut self,
        node: &Handle,
        attrs: Option<Attrs>,
        tag: &str,
        because: &str,
        path: &str,
    ) {
        let Some(attrs) = attrs else {
            return;
        };
        let mut dropped: Vec<String> = Vec::new();
        if let Some(id) = attrs.id {
            dropped.push(format!("id=\"{id}\""));
        }
        if !attrs.classes.is_empty() {
            dropped.push(format!("class=\"{}\"", attrs.classes.join(" ")));
        }
        dropped.extend(attrs.key_values.keys().map(|k| k.to_string()));
        for name in dropped {
            self.diag(
                HtmlImportDiagnosticCode::AttributeDropped,
                format!("Dropped {name} on <{tag}>: {because}"),
                HtmlImportSeverity::Info,
                path,
                node,
            );
        }
    }

    /// `<figure class="carve-figure-group">` back to the `figure_group` node
    /// it rendered from (PART 9 §4c). The panels nest DIRECTLY, so the group's
    /// own caption is its LAST direct `<figcaption>` child - a panel's
    /// figcaption sits a level down inside the panel figure and is never read
    /// as the group's - and everything else is the child list, each panel
    /// routed back through [`Self::figure_panel`].
    pub(super) fn figure_group(
        &mut self,
        h: &Handle,
        path: &str,
        depth: usize,
        attrs: Option<Attrs>,
    ) -> Result<Vec<BlockNode>, HtmlImportError> {
        let attrs = Self::without_structural_class(attrs, "carve-figure-group");
        let nodes = h.children.borrow();
        let caption_at = nodes
            .iter()
            .rposition(|c| Self::tag(c).as_deref() == Some("figcaption"));
        let mut caption = None;
        if let Some(i) = caption_at {
            let p = format!("{path}/figcaption[{}]", i + 1);
            caption = Some(self.caption_inlines(&nodes[i], &p, depth + 1, "figcaption")?);
        }
        let mut body: Vec<Handle> = Vec::new();
        let mut body_paths: Vec<String> = Vec::new();
        for (i, child) in nodes.iter().enumerate() {
            if Some(i) == caption_at {
                continue;
            }
            // Lifting the caption out is not a renumbering of everything after
            // it (markup-carve/carve#1554).
            body_paths.push(Self::child_path(path, child, i));
            body.push(child.clone());
        }
        // The children list is read out before the walk, so nothing below runs
        // while this node's `RefCell` is still borrowed.
        drop(nodes);
        let children = self.blocks_at(&body, Some(&body_paths), path, depth + 1)?;
        Ok(vec![BlockNode::FigureGroup(FigureGroup {
            attrs,
            children,
            caption,
            pos: None,
        })])
    }

    /// Whether `roundtrip` keeps what the foreign-`<figure>` rebuild produced,
    /// or throws it away and preserves the element instead
    /// (markup-carve/carve#1704).
    pub(super) fn roundtrip_keeps_rebuilt_figure(h: &Handle, blocks: &[BlockNode]) -> bool {
        let [BlockNode::Figure(figure)] = blocks else {
            return false;
        };
        let captions_last = h
            .children
            .borrow()
            .iter()
            .filter_map(Self::tag)
            .next_back()
            .is_some_and(|tag| tag == "figcaption");
        if !captions_last {
            return false;
        }
        Self::caption_line_binds(&figure.target)
    }

    /// Does the caption line the rebuild writes BIND to this target, so that it
    /// re-reads as the figure it was written from?
    ///
    /// THE PROPERTY, AND ONE ANSWER FOR EVERY MODE
    /// (markup-carve/carve#1704, ruling markup-carve/carve-php#1731). The modes
    /// differ in what they do with a target the line does not bind to, never in
    /// which targets those are: `roundtrip` keeps the element's bytes, and
    /// `safe` and `semantic`, which cannot preserve, unwrap and declare. Neither
    /// writes the line.
    ///
    /// Image, code block and quote each write a caption line the parser reads
    /// back as the same figure. `Table` is the deliberate carve-out named above:
    /// the line binds to the TABLE, which is why a figure is built for it and
    /// why the writer still has something to declare.
    ///
    /// PROSE ABSORBS THE LINE INSTEAD OF CARRYING IT, which is a different
    /// failure from losing it. `x` then `^ Cap` re-reads as ONE paragraph
    /// holding a literal caret, so the document gains a character its author
    /// never wrote - and an addition cannot be declared away the way a loss can.
    pub(super) fn caption_line_binds(target: &FigureTarget) -> bool {
        match target {
            FigureTarget::Image(_) | FigureTarget::CodeBlock(_) | FigureTarget::BlockQuote(_) => {
                true
            }
            FigureTarget::Table(table) => match table.caption.as_deref() {
                None => true,
                Some(caption) => Self::inlines_are_blank(caption),
            },
            FigureTarget::Paragraph(_) => false,
        }
    }

    /// A rebuilt figure whose target would ABSORB its caption line, taken back
    /// apart into the blocks the unwrap writes: the body, then the caption as a
    /// paragraph of its own (ruling markup-carve/carve-php#1731).
    ///
    /// The figure is gone either way; what this buys over writing the line
    /// anyway is that no character is invented. `x`, a blank line, then `Cap`
    /// re-reads as two paragraphs - the association is lost, and every byte the
    /// author wrote is still their own. It is the shape carve-php has always
    /// written and the shape the caller declares with `element-unwrapped`.
    ///
    /// THE WRAPPER'S ATTRIBUTES ARE LEFT FOR THE CALLER TO DROP AND REPORT,
    /// rather than landed on the body. An `id` that identified a FIGURE, moved
    /// onto a bare paragraph, identifies something the author never marked; a
    /// declared loss beats a silent substitution.
    ///
    /// Anything else is handed straight back. [`Self::caption_line_binds`] is
    /// the whole test, so a caption target added later inherits this without a
    /// second list to keep in step.
    pub(super) fn unwrap_absorbed_caption(
        &mut self,
        h: &Handle,
        path: &str,
        blocks: Vec<BlockNode>,
    ) -> (Vec<BlockNode>, bool) {
        let [BlockNode::Figure(figure)] = blocks.as_slice() else {
            return (blocks, false);
        };
        if Self::caption_line_binds(&figure.target) {
            return (blocks, false);
        }
        // Read BEFORE the figure is taken apart, because the merge below is what
        // displaces them and the row has to name the names it displaced.
        let table_collision = matches!(&*figure.target, FigureTarget::Table(_));
        if table_collision {
            self.record_displaced_figure_attrs(h, path, figure);
        }
        let Some(BlockNode::Figure(figure)) = blocks.into_iter().next() else {
            unreachable!("the slice pattern above matched a lone figure")
        };
        let caption = BlockNode::Paragraph(Paragraph {
            attrs: None,
            children: figure.caption,
            at_content_column: true,
            block_image: false,
            pos: None,
        });
        match *figure.target {
            FigureTarget::Paragraph(target) => (vec![BlockNode::Paragraph(target), caption], false),
            FigureTarget::Table(mut table) => {
                // THE FIGURE'S ATTRIBUTES RIDE ONTO THE TABLE
                // (markup-carve/carve#1721). The figure's line is the LEADING
                // one and the table's own wins the names both set, which is the
                // order the two lines already stacked in. Dropping them instead
                // would take an `id` an anchor points at with nothing said.
                if let Some(own) = figure.attrs {
                    crate::parse::merge_leading_attrs(&mut table.attrs, own);
                }
                // ATTACHED TO THE `<figcaption>`, not to the figure. The row's
                // path already names it, and the report sorts by NODE - so
                // hanging it on the figure tied it with the figure's own
                // attribute row and left the two in emission order, which puts
                // the deferred one last. On the caption it sorts after the
                // figure, which is where the reader looks for it.
                let (caption_node, caption_path) = Self::figcaption_site(h, path);
                self.diag(
                    HtmlImportDiagnosticCode::ElementUnwrapped,
                    "Detached a <figcaption> into a paragraph after the table: the table's own \
                     <caption> fills Carve's one caption slot, so the figure's caption keeps its \
                     text and loses its role"
                        .into(),
                    HtmlImportSeverity::Warning,
                    &caption_path,
                    &caption_node,
                );
                (vec![BlockNode::Table(table), caption], true)
            }
            _ => unreachable!("only a paragraph and a self-captioning table refuse the line"),
        }
    }

    /// The `<figcaption>` the detach is about, and its path: the FIRST direct
    /// child of that name, which is the one `figure_panel_captioned` captions
    /// with.
    ///
    /// Falls back to the figure itself, which cannot happen on this arm - a
    /// detach needs a caption to detach - and is spelled rather than
    /// `unwrap`ped so a future caller cannot turn a missing child into a panic.
    pub(super) fn figcaption_site(h: &Handle, path: &str) -> (Handle, String) {
        let found = h
            .children
            .borrow()
            .iter()
            .enumerate()
            .find(|(_, c)| Self::tag(c).as_deref() == Some("figcaption"))
            .map(|(index, c)| (c.clone(), index));
        match found {
            Some((node, index)) => (node, format!("{path}/figcaption[{}]", index + 1)),
            None => (h.clone(), path.to_owned()),
        }
    }

    /// `<figure class="carve-figure-panel">` back to the node it wrapped: the
    /// host plus its `<figcaption>` rebuild the `figure` the caption pass
    /// produced; a bare wrapped table (whose caption is its own `<caption>`)
    /// unwraps to the table node.
    ///
    /// `own_output` says whether the wrapper was THIS engine's panel, in which
    /// case the `carve-figure-panel` class is structural and comes back off. A
    /// FOREIGN figure keeps every class it carried: only a class in FIRST
    /// position identifies an own-output panel above, so `class="custom
    /// carve-figure-panel"` arrives here as somebody else's markup, and stripping
    /// the name out of it would silently edit an author's class list
    /// (carve#1286).
    pub(super) fn figure_panel(
        &mut self,
        h: &Handle,
        path: &str,
        depth: usize,
        attrs: Option<Attrs>,
        own_output: bool,
    ) -> Result<Vec<BlockNode>, HtmlImportError> {
        self.figure_panel_captioned(h, path, depth, attrs, own_output)
            .map(|(blocks, _)| blocks)
    }

    /// [`Self::figure_panel`], and WHETHER THE ELEMENT WAS CAPTIONED AT ALL.
    ///
    /// A figure is the CAPTIONED wrapper (PART 9 §4b), so an element carrying
    /// no `<figcaption>` - or one whose caption spells nothing - is not a
    /// figure to rebuild or to preserve: it unwraps to its content with
    /// `element-unwrapped`, in every mode, which is the behavior the
    /// per-target rule leaves untouched (`docs/html-import.md`,
    /// markup-carve/carve#1704). `roundtrip` has to know that BEFORE it decides
    /// whether to preserve, and only this function knows it: "spells nothing"
    /// is a question about the PARSED inlines, not about the element's text, so
    /// answering it a second time on the DOM would be the same rule written
    /// twice and free to drift.
    pub(super) fn figure_panel_captioned(
        &mut self,
        h: &Handle,
        path: &str,
        depth: usize,
        attrs: Option<Attrs>,
        own_output: bool,
    ) -> Result<(Vec<BlockNode>, bool), HtmlImportError> {
        let attrs = if own_output {
            Self::without_structural_class(attrs, "carve-figure-panel")
        } else {
            attrs
        };
        let mut caption = None;
        let mut caption_index = usize::MAX;
        // EACH HOST CHILD KEEPS ITS OWN POSITION. The caption is lifted out of
        // the child list here, and every margin the branches below drop leaves
        // another hole in it, so an index rebuilt from this vector names a
        // different node than the one the diagnostic is about (PART 12 §16,
        // markup-carve/carve#1554).
        let mut host: Vec<(usize, Handle)> = Vec::new();
        for (index, child) in h.children.borrow().iter().enumerate() {
            if Self::tag(child).as_deref() == Some("figcaption") {
                // The first NON-BLANK one captions. HTML allows at most one, so
                // a second is somebody's malformed markup - but overwriting the
                // caption with it threw the first one's text away entirely,
                // which is content loss rather than a structural downgrade.
                // Extras fall through to the host instead, where the generic
                // unwrap keeps their text and reports it (carve#1286).
                //
                // Non-blank rather than merely first, because a blank caption is
                // treated as absent below: if the first one is blank, a later
                // one is the only caption the figure has, and captioning with it
                // keeps a visible string in the role its author gave it instead
                // of demoting it to content. No text is lost either way.
                if caption.is_none() {
                    // Wherever the author put it - a literal step with no index
                    // at all said nothing about which `<figcaption>` this was,
                    // and no other step in a path is spelled that way.
                    let p = format!("{path}/figcaption[{}]", index + 1);
                    let inlines = self.caption_inlines(child, &p, depth + 1, "figcaption")?;
                    // An EMPTY caption is not a caption. Kept as one it wrote a
                    // bare `^` line, which re-parses as a literal caret in a
                    // paragraph: the figure destroyed AND a character in the
                    // output the author never typed. Treated as absent it takes
                    // the no-caption path, which writes the content alone and
                    // reports the wrapper as unwrapped (carve#1286). carve-js
                    // writes the bare `^`, so this diverges from it knowingly -
                    // matching it would mean reproducing the corruption.
                    if !Self::inlines_are_blank(&inlines) {
                        caption = Some(inlines);
                        caption_index = index;
                    }
                    continue;
                }
            }
            host.push((index, child.clone()));
        }
        let is_blank_text = dom_text_is_layout_only;
        if own_output {
            host.retain(|(_, c)| !is_blank_text(c));
        } else {
            // A COMMENT is invisible, so a margin does not stop being a margin
            // for sitting on the far side of one. Trimming only text left
            // `<figure><!--x--> <img>` holding a stray space beside the image,
            // which made the host a paragraph rather than an image - the output
            // was unchanged but the figure was then reported as unwrapped when
            // it had not been (carve#1286).
            let is_margin =
                |n: &Handle| is_blank_text(n) || matches!(&n.data, NodeData::Comment { .. });
            let is_margin = |c: &(usize, Handle)| is_margin(&c.1);
            // Found once and drained once. Removing from the front one node at
            // a time shifts the rest on every step, which is quadratic in the
            // number of leading margins - and margins are attacker-controlled
            // and cost nothing to repeat, so the bound has to be reached by
            // charging them rather than by grinding through them.
            let lead = host.iter().take_while(|c| is_margin(c)).count();
            host.drain(..lead);
            let tail = host.iter().rev().take_while(|c| is_margin(c)).count();
            host.truncate(host.len() - tail);
            let trimmed = lead + tail;
            // A node dropped here never reaches `blocks`, so it would never be
            // charged - and a margin is the cheapest thing an author can repeat.
            // Discarding it is not the same as never having walked it, so the
            // budget is spent either way and `<figure>` gains no free ride the
            // rest of the importer does not give.
            for _ in 0..trimmed {
                self.enter(depth + 1)?;
            }
        }
        // An unwrapped figure writes its caption paragraph where the
        // `<figcaption>` stood, so the host is walked in two runs around it.
        let split = host.partition_point(|(index, _)| *index < caption_index);
        let mut blocks = Vec::new();
        let mut caption_at = 0;
        for (run, part) in [&host[..split], &host[split..]].into_iter().enumerate() {
            let paths: Vec<String> = part
                .iter()
                .map(|(index, child)| Self::child_path(path, child, *index))
                .collect();
            let nodes: Vec<Handle> = part.iter().map(|(_, child)| child.clone()).collect();
            blocks.extend(self.blocks_at(&nodes, Some(&paths), path, depth + 1)?);
            if run == 0 {
                caption_at = blocks.len();
            }
        }
        Self::figure_from_caption(blocks, caption, caption_at, attrs)
    }

    #[inline(never)]
    pub(super) fn figure_from_caption(
        mut blocks: Vec<BlockNode>,
        caption: Option<Vec<InlineNode>>,
        caption_at: usize,
        attrs: Option<Attrs>,
    ) -> Result<(Vec<BlockNode>, bool), HtmlImportError> {
        let Some(caption) = caption else {
            return Ok((blocks, false));
        };
        if blocks.len() == 1 {
            let target = match blocks.remove(0) {
                BlockNode::Paragraph(p)
                    if p.attrs.is_none()
                        && p.children.len() == 1
                        && matches!(p.children[0], InlineNode::Image(_)) =>
                {
                    match p.children.into_iter().next() {
                        Some(InlineNode::Image(img)) => FigureTarget::Image(img),
                        _ => unreachable!("the match guard saw an image"),
                    }
                }
                BlockNode::Paragraph(p) => FigureTarget::Paragraph(p),
                BlockNode::BlockImage(img) => FigureTarget::Image(img),
                BlockNode::BlockQuote(quote) => FigureTarget::BlockQuote(quote),
                BlockNode::Table(table) => FigureTarget::Table(table),
                BlockNode::CodeBlock(code) => FigureTarget::CodeBlock(code),
                other => {
                    blocks.insert(0, other);
                    blocks.insert(
                        caption_at,
                        BlockNode::Paragraph(Paragraph {
                            attrs: None,
                            children: caption,
                            at_content_column: true,
                            block_image: false,
                            pos: None,
                        }),
                    );
                    return Ok((blocks, true));
                }
            };
            return Ok((
                vec![BlockNode::Figure(Figure {
                    attrs,
                    target: Box::new(target),
                    rendered_target: None,
                    caption,
                    short_caption: None,
                    pos: None,
                })],
                true,
            ));
        }
        blocks.insert(
            caption_at,
            BlockNode::Paragraph(Paragraph {
                attrs: None,
                children: caption,
                at_content_column: true,
                block_image: false,
                pos: None,
            }),
        );
        Ok((blocks, true))
    }
}
