use super::footnotes::{Lifted, FOOTNOTE_PLACEMENT_TAG};
use super::ORDERED_TASK_ITEM_UNSPELLABLE;
use super::{
    is_admonition_title, is_div_label, is_layout_only, is_layout_space, share_figure_attrs,
    task_marker_text, text_node, trim_edge_whitespace, Importer, ROUNDTRIP_UNWRAPPED_SECTIONING,
};
use super::{HtmlImportDiagnosticCode, HtmlImportError, HtmlImportMode, HtmlImportSeverity};
use crate::ast::*;
use markup5ever_rcdom::{Handle, NodeData};
use std::rc::Rc;
impl<'a> Importer<'a> {
    pub(super) fn block(
        &mut self,
        h: &Handle,
        path: &str,
        depth: usize,
    ) -> Result<Vec<BlockNode>, HtmlImportError> {
        self.enter(depth)?;
        let tag = Self::tag(h).unwrap();
        let attrs = self.attrs(h, path);
        if tag == "template" && Self::carrier(h, crate::roundtrip::ABBREVIATIONS).is_some() {
            return self.block_abbreviation_template(h, path, depth);
        }
        let children = h.children.borrow();
        if tag == FOOTNOTE_PLACEMENT_TAG {
            return self.block_footnotes();
        }
        if tag == "html" || tag == "head" || tag == "body" {
            // No attribute report here, and deliberately none: `fragment_top_level`
            // strips the scaffold before the walk begins, so an attribute on
            // `<html>` or `<body>` never reaches `attrs` and a check placed
            // here could not fire. That silent drop is older than this policy
            // and is pinned by `unwrapped_wrappers_are_not_a_path` below.
            return self.blocks(&children, path, depth + 1);
        }
        if matches!(tag.as_str(), "h1" | "h2" | "h3" | "h4" | "h5" | "h6") {
            return self.block_heading(h, path, depth, &tag, attrs, &children);
        }
        if tag == "p" {
            return self.block_paragraph(h, path, depth, &tag, attrs, &children);
        }
        if tag == "blockquote" {
            return self.block_quote(path, depth, attrs, &children);
        }
        if tag == "pre" {
            return self.block_pre(h, path, depth, attrs, &children);
        }
        if tag == "hr" {
            return self.block_hr(h, attrs);
        }
        if tag == "ul" || tag == "ol" {
            return self.block_list(h, path, depth, &tag, attrs, &children);
        }
        if tag == "details" {
            return self.block_details(h, path, depth, attrs);
        }
        if tag == "dl" {
            return self.definition_list(h, path, depth, attrs);
        }
        if tag == "table" {
            return self.block_table(h, path, depth, attrs);
        }
        if tag == "div" && Self::carrier(h, crate::roundtrip::LINE_BLOCK).is_some() {
            return self.carried_line_block(&children, path, depth, attrs);
        }
        let carried_div = tag == "div" && Self::carrier(h, crate::roundtrip::DIV).is_some();
        if tag == "div" {
            return self.block_div(h, path, depth, &tag, attrs, &children, carried_div);
        }
        // This engine's own composite-figure shape (PART 9 §4c) comes back as
        // the node it left as. Only the exact own-output classes take these
        // paths; any other <figure> stays an unsupported element below.
        if tag == "figure" && Self::first_class(h).as_deref() == Some("carve-figure-group") {
            return self.figure_group(h, path, depth, attrs);
        }
        if tag == "figure" && Self::first_class(h).as_deref() == Some("carve-figure-panel") {
            return self.figure_panel(h, path, depth, attrs, true);
        }
        if tag == "figure" {
            return self.block_figure(h, path, depth, &tag, attrs);
        }
        self.block_fallback(h, path, depth, &tag, attrs, &children)
    }

    #[inline(never)]
    #[allow(clippy::needless_return, clippy::needless_borrow)]
    pub(super) fn block_footnotes(&self) -> Result<Vec<BlockNode>, HtmlImportError> {
        return Ok(vec![BlockNode::Directive(Directive {
            attrs: None,
            kind: "footnotes".to_string(),
            title: None,
            label: None,
            children: Vec::new(),
            pos: None,
        })]);
    }

    #[inline(never)]
    #[allow(clippy::needless_return, clippy::needless_borrow)]
    pub(super) fn block_quote(
        &mut self,
        path: &str,
        depth: usize,
        attrs: Option<Attrs>,
        children: &[Handle],
    ) -> Result<Vec<BlockNode>, HtmlImportError> {
        return Ok(vec![BlockNode::BlockQuote(BlockQuote {
            attrs,
            children: self.blocks(&children, path, depth + 1)?,
            // An imported quote has no authored spelling: HTML records the
            // element, never how someone typed it.
            fenced: false,
            pos: None,
        })]);
    }

    #[inline(never)]
    #[allow(clippy::needless_return, clippy::needless_borrow)]
    pub(super) fn block_hr(
        &self,
        h: &Handle,
        attrs: Option<Attrs>,
    ) -> Result<Vec<BlockNode>, HtmlImportError> {
        return Ok(vec![BlockNode::ThematicBreak(ThematicBreak {
            // PART 10 §13a `data-char`: without it a rendered `***` imports
            // back as `---`, silently, because both render the same `<hr>`.
            marker: Self::reads_break_char(h),
            attrs,
            pos: None,
        })]);
    }

    #[inline(never)]
    #[allow(clippy::needless_return, clippy::needless_borrow)]
    pub(super) fn block_details(
        &mut self,
        h: &Handle,
        path: &str,
        depth: usize,
        attrs: Option<Attrs>,
    ) -> Result<Vec<BlockNode>, HtmlImportError> {
        return Ok(vec![BlockNode::Admonition(
            self.details(h, path, depth, attrs)?,
        )]);
    }

    #[inline(never)]
    #[allow(clippy::needless_return, clippy::needless_borrow)]
    pub(super) fn block_abbreviation_template(
        &mut self,
        h: &Handle,
        path: &str,
        depth: usize,
    ) -> Result<Vec<BlockNode>, HtmlImportError> {
        self.charge_subtree(h, depth)?;
        let payload = if let NodeData::Element {
            template_contents, ..
        } = &h.data
        {
            template_contents
                .borrow()
                .as_ref()
                .filter(|content| {
                    content
                        .children
                        .borrow()
                        .iter()
                        .all(|child| matches!(child.data, NodeData::Text { .. }))
                })
                .map(Self::text)
                .unwrap_or_default()
        } else {
            String::new()
        };
        let mut parsed = crate::parse(&payload);
        if parsed.frontmatter_raw.is_none()
            && parsed.frontmatter.is_empty()
            && parsed.footnote_defs.is_empty()
            && !parsed.children.is_empty()
            && parsed
                .children
                .iter()
                .all(|node| matches!(node, BlockNode::AbbreviationDef(_)))
        {
            self.carrier_definitions
                .extend(
                    std::mem::take(&mut parsed.children)
                        .into_iter()
                        .map(|node| match node {
                            BlockNode::AbbreviationDef(mut def) => {
                                def.pos = None;
                                BlockNode::AbbreviationDef(def)
                            }
                            _ => unreachable!(),
                        }),
                );
            return Ok(Vec::new());
        }
        self.diag(
            HtmlImportDiagnosticCode::AttributeDropped,
            "Dropped invalid abbreviation carrier".into(),
            HtmlImportSeverity::Warning,
            path,
            h,
        );
        return Ok(Vec::new());
    }

    #[inline(never)]
    pub(super) fn block_table(
        &mut self,
        h: &Handle,
        path: &str,
        depth: usize,
        attrs: Option<Attrs>,
    ) -> Result<Vec<BlockNode>, HtmlImportError> {
        self.table(h, path, depth, attrs).map(|table| {
            if self.writing && matches!(&table, BlockNode::Table(table) if table.rows.is_empty()) {
                Vec::new()
            } else {
                vec![table]
            }
        })
    }

    #[inline(never)]
    #[allow(clippy::needless_return, clippy::needless_borrow)]
    pub(super) fn block_pre(
        &mut self,
        h: &Handle,
        path: &str,
        depth: usize,
        attrs: Option<Attrs>,
        children: &[Handle],
    ) -> Result<Vec<BlockNode>, HtmlImportError> {
        let code = children
            .iter()
            .find(|n| Self::tag(n).as_deref() == Some("code"))
            .unwrap_or(h);
        let lang = self.code_language(h, code);
        let content = self.code_text(h, path, depth, true, (Some(code), lang.as_deref()))?;
        return Ok(vec![BlockNode::CodeBlock(CodeBlock {
            attrs,
            lang,
            title: None,
            label: Self::reads_code_label(h),
            content,
            pos: None,
        })]);
    }

    // carve-rs#2978: keep each recursive debug frame within the 12 KiB budget.
    #[inline(never)]
    #[allow(clippy::needless_return, clippy::needless_borrow)]
    pub(super) fn block_heading(
        &mut self,
        h: &Handle,
        path: &str,
        depth: usize,
        tag: &String,
        attrs: Option<Attrs>,
        children: &[Handle],
    ) -> Result<Vec<BlockNode>, HtmlImportError> {
        let mut attrs = attrs;
        self.heading_depth += 1;
        let inlines = self.inlines(&children, path, depth + 1);
        self.heading_depth -= 1;
        let inlines = inlines?;
        // AN EMPTY HEADING IS DROPPED, WITH ONE ROW (markup-carve/carve#2419).
        // It has no Carve spelling at all - `inline_content` is one-or-more -
        // so the writer wrote a bare marker, which re-reads as a paragraph and
        // breaks `parse(html_to_carve(h)) == html_to_ast(h)`. Dropping it is
        // what this contract already does to every other empty container, and
        // it carries nothing a reader loses.
        //
        // REPORTED WHETHER OR NOT IT CARRIED ATTRIBUTES, unlike the paragraph
        // below: a bare empty `<p>` leaves silently because it carries
        // nothing (carve#2423), while the ruling here names one row for the
        // heading either way.
        if inlines.is_empty() || is_layout_only(&inlines) {
            let message = if inlines.is_empty() {
                format!("Dropped <{tag}> holding no content")
            } else {
                format!("Dropped whitespace-only <{tag}> holding no content character")
            };
            self.diag(
                HtmlImportDiagnosticCode::ElementDropped,
                message,
                HtmlImportSeverity::Warning,
                path,
                h,
            );
            return Ok(Vec::new());
        }
        if let Some(held) = attrs.as_mut().filter(|held| held.id.is_some()) {
            if Self::reads_generated_section(h) {
                held.id = None;
            } else if self.writing || Self::reads_explicit_id(h) {
                // Authored. A non-empty order is exhaustive, so every
                // imported slot has to be carried - and carried in the
                // element's OWN attribute order, which is the order the
                // writer has to spell to render these bytes back.
                //
                // ON THE WRITING EXIT, OR WHERE A CARRIER RECORDED IT. `order`
                // is a source-layout field and an import read no source, so
                // the published tree records none (markup-carve/carve#1647)
                // unless the explicit-id carrier says the slot was authored;
                // see `Importer::writing` for why the writer needs it.
                held.order = Self::slot_order_from_element(h, held);
            }
        }
        let attrs = attrs.filter(|held| *held != Attrs::default());
        return Ok(vec![BlockNode::Heading(Heading {
            attrs,
            level: tag[1..].parse().unwrap(),
            children: inlines,
            pos: None,
        })]);
    }

    #[inline(never)]
    #[allow(clippy::needless_return, clippy::needless_borrow)]
    pub(super) fn block_paragraph(
        &mut self,
        h: &Handle,
        path: &str,
        depth: usize,
        tag: &String,
        attrs: Option<Attrs>,
        children: &[Handle],
    ) -> Result<Vec<BlockNode>, HtmlImportError> {
        let inlines = self.inlines(&children, path, depth + 1)?;
        // PART 11 §7: a block element holding nothing but LAYOUT builds no
        // node, and the drop is REPORTED because it is a real loss - an
        // element the input had contributes nothing. Keeping owes no report,
        // since a kept character survives the write intact.
        //
        // WHY THE PARAGRAPH AND NOT EVERY BLOCK. §7's argument is that the
        // node left behind is UNSPELLABLE, so the two import exits disagree
        // about it. A heading spells its own marker and a list item its own
        // bullet, so neither is that shape; the paragraph is, and it is the
        // shape all three of §7's rows are written over.
        if is_layout_only(&inlines) {
            self.diag(
                HtmlImportDiagnosticCode::ElementDropped,
                format!("Dropped whitespace-only <{tag}> holding no content character"),
                HtmlImportSeverity::Warning,
                path,
                h,
            );
            return Ok(Vec::new());
        }
        /*
         * CARVE SOURCE CANNOT SPELL A PARAGRAPH HOLDING ONLY AN IMAGE, so
         * the writer loses it and PART 12 section 16 says the writing exit
         * reports that (carve-rs#1331).
         *
         * `resources/examples/edge-cases.md` rules the shape: "a paragraph
         * whose whole content is one image is still the standalone image
         * shape, not a wrapped one". So `![G](g.jpg)` re-reads as a BLOCK
         * image, the `<p>` the author wrote is gone from the source, and
         * `html_to_ast` keeps a paragraph the re-parsed source does not -
         * the one carve-out `docs/html-import.md` allows to
         * `parse(html_to_carve(h)) == html_to_ast(h)`.
         *
         * NOT A CHANGE OF OUTPUT, because there is no other output to
         * write. carve-js measured an indented ` ![G](g.jpg)` as a
         * paragraph holding one image and rejected it as a spelling anyway;
         * this engine reads it as a block image at every indent, so there
         * is not even a near miss to reach for.
         */
        // THE AUTHOR'S PARAGRAPH IS TRIMMED TOO (carve-rs#1336). The
        // synthesized wrapper beside it already was, and the reason given -
        // that the wrapper is not an element the document contains - is real
        // but is not the deciding one. PART 11 section 7's principle is:
        // whitespace that is layout is not content, wherever it sits. Left
        // untrimmed, `<p>` newline `  <img>` newline `</p>` kept two spaces
        // in the tree that the writer drops, so the two exits disagreed
        // about characters no reader can act on.
        let inlines = trim_edge_whitespace(inlines);
        // No spelling holds an empty paragraph, and its attribute line
        // would land on the next block, so the row covers the attributes.
        if inlines.is_empty() && attrs.is_some() {
            self.diag(
                HtmlImportDiagnosticCode::ElementDropped,
                format!("Dropped <{tag}> holding no content"),
                HtmlImportSeverity::Warning,
                path,
                h,
            );
            return Ok(Vec::new());
        }
        // A bare one carries nothing, and the writer writes nothing for it.
        if inlines.is_empty() {
            return Ok(Vec::new());
        }
        if inlines.len() == 1 && matches!(&inlines[0], InlineNode::Image(_)) {
            if let InlineNode::Image(mut image) = inlines.into_iter().next().unwrap() {
                if let Some(leading) = attrs {
                    crate::parse::merge_leading_attrs(&mut image.attrs, leading);
                }
                return Ok(vec![BlockNode::BlockImage(image)]);
            }
            unreachable!();
        }
        let paragraph = Paragraph {
            attrs,
            children: inlines,
            at_content_column: true,
            block_image: false,
            pos: None,
        };
        return Ok(vec![BlockNode::Paragraph(paragraph)]);
    }

    #[inline(never)]
    #[allow(clippy::needless_return, clippy::needless_borrow)]
    pub(super) fn block_list(
        &mut self,
        h: &Handle,
        path: &str,
        depth: usize,
        tag: &String,
        attrs: Option<Attrs>,
        children: &[Handle],
    ) -> Result<Vec<BlockNode>, HtmlImportError> {
        let ordered = tag == "ol";
        let mut list_items: Vec<Handle> = Vec::new();
        let mut stray: Vec<Handle> = Vec::new();
        let mut stray_paths: Vec<String> = Vec::new();
        for (i, child) in children.iter().enumerate() {
            if Self::tag(child).as_deref() == Some("li") {
                list_items.push(child.clone());
                continue;
            }
            let p = Self::child_path(path, child, i);
            if let Some(stray_tag) = Self::tag(child) {
                // An ACTIVE element is not kept and must not be reported as
                // if it were: the ordinary walk drops it with the
                // `element-dropped` every other site gives it, and a
                // position note beside that would tell the reader the
                // content survived ahead of the list when it did not.
                if !matches!(
                    stray_tag.as_str(),
                    "script" | "style" | "template" | "noscript"
                ) {
                    self.diag(
                            HtmlImportDiagnosticCode::ElementUnwrapped,
                            format!(
                                "A <{stray_tag}> inside <{tag}> kept its content but not its place among the items: it is emitted as blocks ahead of the list"
                            ),
                            HtmlImportSeverity::Warning,
                            &p,
                            child,
                        );
                }
            // `is_layout_space` AND NOT `str::trim`: `trim` is
            // `char::is_whitespace`, so a stray U+00A0 read as blank and the
            // move went UNDECLARED while the same move for an ordinary word
            // was reported (markup-carve/carve-rs#1342). The character was
            // kept either way - what was missing is the row saying it left
            // its place among the items, which is the part a reader cannot
            // see from the output.
            } else if matches!(&child.data, NodeData::Comment { .. }) {
                self.diag(
                        HtmlImportDiagnosticCode::ElementUnwrapped,
                        format!(
                            "An HTML comment directly inside <{tag}> kept its text but not its place among the items: it is emitted as a comment ahead of the list"
                        ),
                        HtmlImportSeverity::Info,
                        &p,
                        child,
                    );
            } else if !Self::text(child).chars().all(is_layout_space) {
                self.diag(
                        HtmlImportDiagnosticCode::ElementUnwrapped,
                        format!(
                            "Text directly inside <{tag}> kept its content but not its place among the items: it is emitted as a paragraph ahead of the list"
                        ),
                        HtmlImportSeverity::Warning,
                        &p,
                        child,
                    );
            }
            stray.push(child.clone());
            stray_paths.push(p);
        }
        let mut before = if stray.is_empty() {
            Vec::new()
        } else {
            self.blocks_at(&stray, Some(&stray_paths), path, depth + 1)?
        };
        // No item, no list: an empty list has no spelling, and its attributes
        // would be an attribute line with no block under it (carve#2367).
        if list_items.is_empty() {
            self.diag(
                HtmlImportDiagnosticCode::ElementDropped,
                format!("Dropped <{tag}> holding no item"),
                HtmlImportSeverity::Warning,
                path,
                h,
            );
            return Ok(before);
        }
        let mut items = Vec::new();
        for (i, li) in list_items.iter().enumerate() {
            let p = format!("{path}/li[{}]", i + 1);
            let li_children = li.children.borrow();
            let checkbox = li_children.iter().enumerate().find(|(_, child)| {
                Self::tag(child).as_deref() == Some("input")
                    && Self::attr(child, "type")
                        .is_some_and(|value| value.eq_ignore_ascii_case("checkbox"))
            });
            // PART 10 §11's carrier, read before the marker text so an
            // ordered item keeps the state character it was written with
            // rather than defaulting to the box.
            let task_state = Self::reads_task_state(li)
                .then(|| Self::attr(li, "data-task-state"))
                .flatten()
                .and_then(|state| state.chars().next())
                // The box already says done; `x` is never a stored state.
                .filter(|state| *state != 'x');
            // UNSPELLABLE BEHIND AN ORDERED MARKER. `task_marker` in
            // `resources/spec/03-blocks-core.ebnf` hangs off
            // `unordered_item` alone, so no Carve source carries a box on an
            // ordered item and the writer dropped both the box and the
            // characters it was read from (carve-rs#1890). The bracket pair
            // is kept as text, the way the Markdown importer keeps it
            // (carve-rs#1886), and the row says what could not be spelled.
            // The AST holds the box either way; only a writer loses it
            // (PART 12 §16), so this runs on the writing exit alone.
            let ordered_task = ordered && checkbox.is_some() && self.writing;
            if let Some((j, input)) = checkbox {
                let input_path = Self::child_path(&p, input, j);
                if ordered_task {
                    self.diag(
                        HtmlImportDiagnosticCode::StructureUnspellable,
                        ORDERED_TASK_ITEM_UNSPELLABLE.into(),
                        HtmlImportSeverity::Warning,
                        &input_path,
                        input,
                    );
                }
                if let Some(kept) = self.attrs(input, &input_path) {
                    let names: Vec<String> = Self::attr_names(&kept)
                        .into_iter()
                        .filter(|name| {
                            !matches!(
                                name.as_str(),
                                "type" | "checked" | "disabled" | "aria-label"
                            )
                        })
                        .collect();
                    if !names.is_empty() {
                        self.diag(
                                HtmlImportDiagnosticCode::AttributeDropped,
                                format!(
                                    "Dropped {} on <input>: a task item's checkbox has no attribute slot",
                                    names.join(", ")
                                ),
                                HtmlImportSeverity::Warning,
                                &input_path,
                                input,
                            );
                    }
                }
            }
            // CONSUMED INTO THE MARKER, so it is not walked as content and
            // leaves no `element-unwrapped` or `attribute-dropped` behind
            // it - reporting the `type` and `checked` lost would name a
            // loss that no longer happens.
            //
            // The siblings after it keep the index they have in the
            // DOCUMENT rather than the one they take in the filtered array,
            // which is why the paths are computed here and handed to
            // `blocks_at`: rebuilding an index from a list something was
            // lifted out of renumbers every sibling after the hole
            // (PART 12 §16, markup-carve/carve#1554).
            //
            // An ordered item's box is the one case where it is not
            // consumed but SUBSTITUTED: a text node standing where the
            // `<input>` stood, so the bracket pair reaches the item's own
            // inline run and keeps the position the marker had among the
            // siblings.
            let (content, content_paths): (Vec<Handle>, Vec<String>) = li_children
                .iter()
                .enumerate()
                .filter_map(|(j, child)| {
                    let path = Self::child_path(&p, child, j);
                    if !checkbox.is_some_and(|(_, input)| Rc::ptr_eq(input, child)) {
                        return Some((child.clone(), path));
                    }
                    ordered_task.then(|| {
                        let checked = Self::attr(child, "checked").is_some();
                        (text_node(&task_marker_text(checked, task_state)), path)
                    })
                })
                .unzip();
            items.push(ListItem {
                attrs: self.attrs(li, &p),
                checked: (!ordered_task)
                    .then(|| checkbox.map(|(_, input)| Self::attr(input, "checked").is_some()))
                    .flatten(),
                task_state: if ordered_task { None } else { task_state },
                children: self.blocks_at(&content, Some(&content_paths), &p, depth + 1)?,
                pos: None,
            });
        }
        let implied = !list_items.iter().any(|li| self.holds_paragraph(li))
            && !items.iter().any(|item| {
                item.children.windows(2).any(|pair| {
                    matches!(pair[0], BlockNode::Paragraph(_))
                        && matches!(pair[1], BlockNode::Paragraph(_))
                })
            });
        // PART 10 §13a `data-carve-tight` wins over what the `<p>`s imply.
        let tight = Self::reads_tight(h).unwrap_or(implied);
        let start = if ordered {
            Self::attr(h, "start")
                .and_then(|s| s.parse().ok())
                .filter(|v| *v != 1)
        } else {
            None
        };
        // PART 19 §1's base class, which the renderer writes back.
        let task_list = !ordered && items.iter().all(|item| item.checked.is_some());
        let mut attrs = if task_list {
            Self::without_structural_class(attrs, "task-list")
        } else {
            attrs
        };
        let ol_type = if ordered {
            self.ordered_list_type(h, start.unwrap_or(1), items.len(), &mut attrs, path)
        } else {
            None
        };
        before.push(BlockNode::List(List {
            attrs,
            ordered,
            start,
            ol_type,
            bare_marker: false,
            // PART 10 §12, the only place the delimiter survives a render.
            delim: (ordered && (Self::reads_delim(h) || Self::reads_ordered_marker(h)))
                .then_some(')'),
            // PART 10 §13a `data-marker`: the bullet a `<ul>` was authored
            // with, which the element itself does not spell.
            bullet_char: (!ordered).then(|| Self::reads_bullet_char(h)).flatten(),
            tight,
            items,
            pos: None,
        }));
        return Ok(before);
    }

    #[inline(never)]
    #[allow(clippy::needless_return, clippy::needless_borrow)]
    #[allow(clippy::too_many_arguments)]
    pub(super) fn block_div(
        &mut self,
        h: &Handle,
        path: &str,
        depth: usize,
        tag: &String,
        attrs: Option<Attrs>,
        children: &[Handle],
        carried_div: bool,
    ) -> Result<Vec<BlockNode>, HtmlImportError> {
        if let Some(math) = Self::carve_math(h, attrs.as_ref()).filter(|_| !carried_div) {
            return self.block_div_math(h, depth, math);
        }
        if let Some(kind) = Self::container_from(&tag, &attrs).filter(|_| !carried_div) {
            return self.block_div_container(h, path, depth, attrs, children, kind);
        }
        // THE LABEL IS LIFTED FIRST, because it is half of the test below.
        // `container_label` takes back PART 9 §10's grouping `[label]` from
        // the `<p class="div-label">` the renderer degraded it to, and the
        // `container_from` arm above has had that lift since
        // markup-carve/carve-rs#1310. This arm never got one, so a `<div>`
        // that DID survive still came back with its label as a paragraph -
        // `<div id="foo">` round-tripped to a fence wrapping
        // `{.div-label}` + `g`. The two changes compose here rather than
        // overlap: the widened test below is what lets the fence survive at
        // all, and this is what puts the label back on its opener.
        let body_paths: Vec<String> = children
            .iter()
            .enumerate()
            .map(|(i, child)| Self::child_path(path, child, i))
            .collect();
        let (label, body, body_paths) =
            self.container_label(children.to_vec(), body_paths, depth)?;
        let blocks = self.blocks_at(&body, Some(&body_paths), path, depth + 1)?;
        if attrs.is_none() && label.is_none() && !carried_div {
            return Ok(blocks);
        }
        return Ok(vec![BlockNode::Div(Div {
            attrs,
            label,
            children: blocks,
            pos: None,
        })]);
    }

    #[inline(never)]
    #[allow(clippy::needless_return, clippy::needless_borrow)]
    pub(super) fn block_div_math(
        &mut self,
        h: &Handle,
        depth: usize,
        math: Math,
    ) -> Result<Vec<BlockNode>, HtmlImportError> {
        // Charged because this arm returns without walking the children
        // `blocks` would have counted one by one - the same reason the
        // `<math>` arm charges by hand. Which arm an element takes must
        // not change what `max_nodes` and `max_depth` see. Only a div
        // whose classes already carry the pair reaches here, and
        // `carve_math` tests them before it reads any text, so a plain
        // div pays for nothing.
        //
        // At `depth + 1`, which is where the skipped traversal would
        // have started: the ordinary arm below hands the children to
        // `blocks` at `depth + 1`. Charging from `depth` undercounts by
        // one, and a math div then imported at a `max_depth` its own
        // twin was rejected at - measured, not assumed, by the depth
        // half of `the_block_recognition_costs_the_same_budget_as_the_
        // div_it_replaces`.
        self.charge_subtree(h, depth + 1)?;
        return Ok(vec![BlockNode::Paragraph(Paragraph {
            attrs: None,
            children: vec![InlineNode::Math(math)],
            at_content_column: true,
            block_image: false,
            pos: None,
        })]);
    }

    #[inline(never)]
    #[allow(clippy::needless_return, clippy::needless_borrow)]
    pub(super) fn block_div_container(
        &mut self,
        h: &Handle,
        path: &str,
        depth: usize,
        attrs: Option<Attrs>,
        children: &[Handle],
        kind: String,
    ) -> Result<Vec<BlockNode>, HtmlImportError> {
        let attrs = Self::without_structural_class(attrs, &kind);
        // A Tier-2 container carries a title the same way a callout
        // does, and renders it with no generated id - so the lift has to
        // reach this arm as well, not only the `<aside>` one below.
        //
        // A DIRECTIVE HAS NO TITLE SLOT (CARVE-P12-057 closes the node
        // without one), so nothing is lifted out of a generated-content
        // container: a `<p class="admonition-title">` inside one stays
        // the ordinary paragraph it reads as, rather than being lifted
        // into a field that does not exist and dropped on the way.
        let generated = crate::ast::is_generated_content_kind(&kind);
        let (mut title, body, body_paths) = self.admonition_title(&children, path, depth, true)?;
        let (label, body, body_paths) = self.container_label(body, body_paths, depth)?;
        let mut children = self.blocks_at(&body, Some(&body_paths), path, depth + 1)?;
        self.keep_title_spellable(&mut title, &mut children, path, h);
        if generated {
            return Ok(vec![BlockNode::Directive(Directive {
                attrs,
                kind,
                title,
                label,
                children,
                pos: None,
            })]);
        }
        return Ok(vec![BlockNode::Admonition(Admonition {
            attrs,
            kind,
            title,
            label,
            children,
            pos: None,
        })]);
    }

    #[inline(never)]
    #[allow(clippy::needless_return, clippy::needless_borrow)]
    pub(super) fn block_figure(
        &mut self,
        h: &Handle,
        path: &str,
        depth: usize,
        tag: &String,
        attrs: Option<Attrs>,
    ) -> Result<Vec<BlockNode>, HtmlImportError> {
        let carried = attrs.clone();
        // Where the report stood before the rebuild ran. A rebuild
        // `roundtrip` then REJECTS has to leave no trace: every row
        // `figure_panel` and the walk under it pushed describes a tree this
        // mode is about to throw away, and the element it preserves instead
        // loses nothing to report. The node BUDGET stays spent - the
        // subtree was walked, and a figure buys no free walk the rest of
        // the importer does not give.
        let reported = self.diagnostics.len();
        let unspellable = self.unspellable.len();
        let displaced = self.displaced_figure_attrs.len();
        let rebuilt = self.figure_panel_captioned(h, path, depth, attrs, false);
        // A LIMIT REACHED INSIDE THE REBUILD IS NOT THE DOCUMENT'S ERROR
        // in this mode. Before the per-target rule, `roundtrip` preserved a
        // figure WITHOUT walking it, so a body 200 levels deep imported
        // fine as an opaque block; failing the whole document for it now
        // would be a regression bought with the fix. The element that
        // cannot be walked is exactly the element no rebuild can reproduce,
        // so it takes the same exit as one whose target has no spelling.
        //
        // Only `DepthLimit` and `NodeLimit` reach here - the other two
        // variants are raised by the WRITER, after this walk is over - so
        // nothing but a budget is being swallowed. The nodes already
        // counted stay counted: the subtree was walked, and refunding them
        // would sell one budget once per figure.
        let mut detached = false;
        let blocks = match rebuilt {
            // A CAPTION LINE THE TARGET WOULD ABSORB IS NOT WRITTEN IN ANY
            // MODE (ruling markup-carve/carve-php#1731). `roundtrip` keeps
            // the bytes for such a target, below; `safe` and `semantic`
            // cannot preserve, so the rebuilt figure is taken back apart
            // here and the arm at the foot of this block declares what the
            // unwrap cost.
            Ok((blocks, _)) if self.opts.mode != HtmlImportMode::Roundtrip => {
                let (blocks, declared) = self.unwrap_absorbed_caption(h, path, blocks);
                // The detach names the `<figcaption>` and what it cost. The
                // generic row below names the WRAPPER, and both would be
                // about the same event - so the arm that already spoke says
                // so rather than letting one shape report twice.
                detached = declared;
                blocks
            }
            // A FIGURE IS THE CAPTIONED WRAPPER (PART 9 §4b). An element
            // carrying no `<figcaption>`, or one whose caption spells
            // nothing, never reaches the rebuild-or-preserve decision at
            // all: it is not a figure to rebuild and not one to preserve,
            // so it unwraps to its content with `element-unwrapped` in
            // EVERY mode. `docs/html-import.md` states that as the behavior
            // this rule leaves untouched, and carve-js pins it; without the
            // arm, `roundtrip` preserved every uncaptioned `<figure>` and
            // the two engines disagreed on exactly that shape.
            Ok((blocks, false)) => blocks,
            Ok((blocks, true)) if Self::roundtrip_keeps_rebuilt_figure(h, &blocks) => blocks,
            Err(error) if self.opts.mode != HtmlImportMode::Roundtrip => return Err(error),
            _ => {
                self.diagnostics.truncate(reported);
                self.unspellable.truncate(unspellable);
                self.displaced_figure_attrs.truncate(displaced);
                self.keep_raw(h, path, &tag);
                return Ok(vec![BlockNode::RawBlock(RawBlock {
                    attrs: None,
                    format: "html".into(),
                    content: Self::html(h),
                    pos: None,
                })]);
            }
        };
        match blocks.as_slice() {
            [BlockNode::Figure(f)] => {
                if matches!(*f.target, FigureTarget::Table(_)) {
                    // THE SENTENCE NO LONGER CLAIMS THE FIGURE'S ATTRIBUTES
                    // SURVIVE, because after the ruling below they may not:
                    // the table writes its OWN attribute line, and where the
                    // two set the same name the table's wins. It said "the
                    // written table carries the caption and the figure's
                    // attributes", which was a false report on exactly the
                    // shape this row is attached to. carve-php recorded that
                    // it could not adopt this wording for that reason
                    // (markup-carve/carve-php#1729); the text is now the one
                    // carve-js and carve-php already share, so one shape
                    // reads the same from all three.
                    self.unspellable.push((
                        h.clone(),
                        path.to_owned(),
                        "A figure wrapping a table has no Carve spelling; the caption is \
                             written on the table, which renders <caption> inside it"
                            .into(),
                        HtmlImportDiagnosticCode::StructureUnspellable,
                    ));
                }
                self.record_displaced_figure_attrs(h, path, f);
            }
            // ALREADY DECLARED, and by the row that can say where the
            // text went. The wrapper is gone either way, but its
            // attributes were not lost - they rode onto the table - so the
            // generic pair would report a drop that did not happen.
            _ if detached => {}
            _ => {
                self.diag(
                    HtmlImportDiagnosticCode::ElementUnwrapped,
                    format!("Unwrapped unsupported <{tag}> element"),
                    HtmlImportSeverity::Info,
                    path,
                    h,
                );
                self.report_unplaceable_attrs(
                    h,
                    carried,
                    tag.as_str(),
                    "the figure did not survive as a figure",
                    path,
                );
            }
        }
        let mut blocks = blocks;
        if let Some(BlockNode::Figure(figure)) = blocks.first_mut() {
            share_figure_attrs(figure);
        }
        return Ok(blocks);
    }

    #[inline(never)]
    #[allow(clippy::needless_return, clippy::needless_borrow)]
    pub(super) fn block_fallback(
        &mut self,
        h: &Handle,
        path: &str,
        depth: usize,
        tag: &String,
        attrs: Option<Attrs>,
        children: &[Handle],
    ) -> Result<Vec<BlockNode>, HtmlImportError> {
        // A RENDERED CALLOUT, before the unwrap claims it. `<aside>` reaches
        // here rather than the `<div>` arm above, and unwrapping it dropped the
        // `Admonition` node outright - the construct did not degrade, it left.
        if let Some(kind) = Self::container_from(&tag, &attrs) {
            let attrs = Self::without_structural_class(
                Self::without_structural_class(attrs, "admonition"),
                &kind,
            );
            let generated = crate::ast::is_generated_content_kind(&kind);
            let (mut title, body, body_paths) =
                self.admonition_title(&children, path, depth, true)?;
            let (label, body, body_paths) = self.container_label(body, body_paths, depth)?;
            let mut children = self.blocks_at(&body, Some(&body_paths), path, depth + 1)?;
            self.keep_title_spellable(&mut title, &mut children, path, h);
            if generated {
                return Ok(vec![BlockNode::Directive(Directive {
                    attrs,
                    kind,
                    title,
                    label,
                    children,
                    pos: None,
                })]);
            }
            return Ok(vec![BlockNode::Admonition(Admonition {
                attrs,
                kind,
                title,
                label,
                children,
                pos: None,
            })]);
        }
        if self.opts.mode == HtmlImportMode::Roundtrip
            && !ROUNDTRIP_UNWRAPPED_SECTIONING.contains(&tag.as_str())
        {
            self.keep_raw(h, path, &tag);
            return Ok(vec![BlockNode::RawBlock(RawBlock {
                attrs: None,
                format: "html".into(),
                content: Self::html(h),
                pos: None,
            })]);
        }
        let unwrapped = self.is_derived_wrapper(h, &tag)
            || self.report_unsupported_element(h, tag.as_str(), path);
        // THE BODY IS BUILT BEFORE THE ATTRIBUTE ROWS so the wrapper's id has a
        // heading to land on. Whether that id is the renderer's or the author's
        // is a question about the HEADING's own text, and only the imported
        // node carries the inline projection that answers it.
        //
        // This does not reorder the report. Rows sort by the position of the
        // LOSING ELEMENT on the way out, and a wrapper stands before everything
        // it wraps, so moving one call across another only changes the order
        // rows were CONSTRUCTED in - which the sort keeps only as a tie-break
        // between rows at the same position, and these are not.
        let mut blocks = self.blocks(&children, path, depth + 1)?;
        let mut attrs = attrs;
        self.restore_hoisted_section_id(h, &tag, &mut attrs, &mut blocks, path);
        // AN UNWRAPPED ELEMENT TAKES ITS ATTRIBUTES WITH IT. `<section
        // role="region">`, `<article>`, `<aside>`, `<main>`, `<nav>` and
        // `<form>` all land here and keep only their children. The keep list
        // refused most of these names inside `attrs` and reported them there,
        // so widening retention without this would have converted a reported
        // loss into a silent one - the opposite of what carve-rs#1060 asks for.
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
        Ok(blocks)
    }

    /// Put a `<section id>` back on the heading the renderer hoisted it off
    /// (markup-carve/carve-rs#1380).
    pub(super) fn restore_hoisted_section_id(
        &mut self,
        handle: &Handle,
        tag: &str,
        attrs: &mut Option<Attrs>,
        blocks: &mut [BlockNode],
        path: &str,
    ) {
        if tag != "section" {
            return;
        }
        let Some(held) = attrs.as_mut() else {
            return;
        };
        let Some(id) = held.id.clone() else {
            return;
        };
        // The heading the wrapper was built around is its FIRST block. Anything
        // else and the `<section>` is not this renderer's, so its id is not a
        // hoist and stays reported.
        let Some(BlockNode::Heading(heading)) = blocks.first_mut() else {
            return;
        };
        // A heading that already carries an id was never hoisted off - two ids
        // in the rendered document mean two different facts, and overwriting
        // the heading's own with the wrapper's would lose one of them.
        if heading.attrs.as_ref().is_some_and(|held| held.id.is_some()) {
            return;
        }
        held.id = None;
        let explicit = Self::reads_explicit_id(handle);
        // PART 10 §13a `data-carve-generated-id` says generated outright; an
        // unmarked wrapper falls back to comparing against the default slug.
        if !explicit
            && (Self::reads_generated_section(handle)
                || Self::is_generated_heading_id(
                    &id,
                    &crate::render::plain_inlines(&heading.children),
                ))
        {
            return;
        }
        let heading_attrs = heading.attrs.get_or_insert_with(Attrs::default);
        heading_attrs.id = Some(id);
        if !explicit {
            return;
        }
        // The carrier says the id took an authored slot, which the writer needs
        // to keep an id equal to its slug and which `data-carve-id-slot` places.
        // The other slots keep the order the heading element spells them in.
        let element = handle.children.borrow().iter().find_map(|child| {
            Self::tag(child)
                .is_some_and(|tag| matches!(tag.as_str(), "h1" | "h2" | "h3" | "h4" | "h5" | "h6"))
                .then(|| child.clone())
        });
        let mut order: Vec<AttrSlot> = match &element {
            Some(element) => Self::slot_order_from_element(element, heading_attrs),
            None => heading_attrs.order.clone(),
        };
        order.retain(|slot| *slot != AttrSlot::Id);
        if element.is_none() && order.is_empty() {
            if !heading_attrs.classes.is_empty() {
                order.push(AttrSlot::Class);
            }
            order.extend(heading_attrs.key_values.keys().cloned().map(AttrSlot::Key));
        }
        let slot = match Self::reads_id_slot(handle) {
            Some(slot) if slot > order.len() => {
                self.diag(
                    HtmlImportDiagnosticCode::AttributeDropped,
                    "Dropped data-carve-id-slot beyond the heading's surviving attribute count"
                        .to_owned(),
                    HtmlImportSeverity::Info,
                    path,
                    handle,
                );
                0
            }
            Some(slot) => slot,
            None => 0,
        };
        order.insert(slot, AttrSlot::Id);
        heading_attrs.order = order;
    }
    /// `<details>/<summary>` to a `details` admonition.
    #[allow(clippy::type_complexity)]
    /// `lift_title` is false for a generated-content container: `directive` has
    /// The first title paragraph becomes the container's title field.
    pub(super) fn admonition_title(
        &mut self,
        children: &[Handle],
        path: &str,
        depth: usize,
        lift_title: bool,
    ) -> Lifted<Vec<InlineNode>> {
        let at = children
            .iter()
            .position(is_admonition_title)
            .filter(|_| lift_title);
        let mut title = None;
        if let Some(i) = at {
            let title_path = Self::child_path(path, &children[i], i);
            // The element itself, not only its children: an empty title is a
            // DOM node the caller's `max_nodes` is counting, and reading past it
            // let a document process more nodes than the limit allows.
            self.enter(depth + 1)?;
            let mut own = self.attrs(&children[i], &title_path);
            if let Some(attrs) = own.as_mut() {
                attrs.classes.retain(|class| class != "admonition-title");
                attrs.order.retain(|slot| *slot != AttrSlot::Class);
            }
            if let Some(attrs) =
                own.filter(|a| a.id.is_some() || !a.classes.is_empty() || !a.key_values.is_empty())
            {
                let names = Self::attr_names(&attrs).join(", ");
                self.diag(
                    HtmlImportDiagnosticCode::AttributeDropped,
                    format!("Dropped {names} on <p>: an admonition title has no attribute slot"),
                    HtmlImportSeverity::Warning,
                    &title_path,
                    &children[i],
                );
            }
            title = Some(self.inlines(&children[i].children.borrow(), &title_path, depth + 2)?);
        }
        let mut body = Vec::new();
        let mut body_paths = Vec::new();
        for (i, child) in children.iter().enumerate() {
            if Some(i) == at {
                continue;
            }
            body_paths.push(Self::child_path(path, child, i));
            body.push(child.clone());
        }
        Ok((title, body, body_paths))
    }

    /// PART 9 §10's grouping `[label]`, taken back off the paragraph the
    /// renderer degraded it to.
    pub(super) fn container_label(
        &mut self,
        body: Vec<Handle>,
        body_paths: Vec<String>,
        depth: usize,
    ) -> Lifted<String> {
        let Some(at) = body.iter().position(|child| Self::tag(child).is_some()) else {
            return Ok((None, body, body_paths));
        };
        if !is_div_label(&body[at]) {
            return Ok((None, body, body_paths));
        }
        if body[..at]
            .iter()
            .any(|child| !Self::text(child).chars().all(is_layout_space))
        {
            return Ok((None, body, body_paths));
        }
        let label_path = body_paths[at].clone();
        // ITS INLINE CONTENT, WRITTEN BACK AS THE LABEL'S SOURCE. The label is an
        // inline run (ruled on markup-carve/carve#2572), so markup in it has a
        // spelling on the opener and the paragraph lifts whole. This used to
        // refuse anything but text, because the label published escaped and
        // lifting a `<strong>` would have flattened it without a word.
        let kids: Vec<Handle> = body[at].children.borrow().clone();
        let Some(text) = self.container_label_source(&kids, &label_path, depth)? else {
            return Ok((None, body, body_paths));
        };
        // AND NOTHING THE OPENER CANNOT SPELL. `]` closes the label and a newline
        // ends the opener line, so neither can ride back out - a label carrying
        // one would be written into source that re-reads as something else.
        // Whatever else an inline run can spell, the parser answers for; see
        // `container_label_reads_back`. Left as a paragraph, which is what it
        // already is.
        if text.contains(']')
            || text.contains('\n')
            || !crate::render_carve::container_label_reads_back(&text)
        {
            return Ok((None, body, body_paths));
        }
        // The element itself, for the same reason the title charges for its
        // own: it is a DOM node `max_nodes` is counting, and a lift that reads
        // past it without charging lets a document process more than the limit.
        self.enter(depth + 1)?;
        // AND EVERYTHING UNDER IT, for the same reason. The lift removes the
        // paragraph before `blocks_at` can reach it, so its text child is a DOM
        // node nothing else will ever charge - and a labelled container would
        // then cost one node and one level LESS than the same DOM without a
        // label, which is a way to process more than `max_nodes` allows by
        // adding markup rather than removing it.
        self.charge_subtree(&body[at], depth + 1)?;
        let mut own = self.attrs(&body[at], &label_path);
        if let Some(attrs) = own.as_mut() {
            attrs.classes.retain(|class| class != "div-label");
            attrs.order.retain(|slot| *slot != AttrSlot::Class);
        }
        if let Some(attrs) =
            own.filter(|a| a.id.is_some() || !a.classes.is_empty() || !a.key_values.is_empty())
        {
            let names = Self::attr_names(&attrs).join(", ");
            let node = body[at].clone();
            self.diag(
                HtmlImportDiagnosticCode::AttributeDropped,
                format!("Dropped {names} on <p>: a container label has no attribute slot"),
                HtmlImportSeverity::Warning,
                &label_path,
                &node,
            );
        }
        let mut rest = Vec::with_capacity(body.len() - 1);
        let mut rest_paths = Vec::with_capacity(body_paths.len() - 1);
        for (i, (child, child_path)) in body.into_iter().zip(body_paths).enumerate() {
            if i == at {
                continue;
            }
            rest.push(child);
            rest_paths.push(child_path);
        }
        Ok((Some(text), rest, rest_paths))
    }

    /// The Carve source of a container label, read off the inline content of the
    /// `<p class="div-label">` the renderer degraded it to. `None` when the writer
    /// has no spelling for it - see
    /// [`container_label_source`](crate::render_carve::container_label_source).
    ///
    /// A THROWAWAY WALK, REWOUND IN FULL. Its only result is a string: the node
    /// budget is charged once by the `charge_subtree` call the lift already makes,
    /// and every diagnostic this walk files would be filed a second time by the
    /// body walk when the lift is refused - two identical rows for one element,
    /// and the report's cap spent twice.
    ///
    /// At `depth + 2`, which is where the body walk would reach these children:
    /// the container hands its body to `blocks_at(.., depth + 1)`, and that enters
    /// each child at `depth + 2`.
    pub(super) fn container_label_source(
        &mut self,
        kids: &[Handle],
        label_path: &str,
        depth: usize,
    ) -> Result<Option<String>, HtmlImportError> {
        let mark = self.mark();
        let walked = self.inlines(kids, label_path, depth + 2);
        self.restore(mark);
        Ok(crate::render_carve::container_label_source(&walked?))
    }
}
