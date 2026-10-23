use super::*;

// ---- Inline ----

pub(crate) fn render_inlines_with_options(nodes: &[InlineNode], options: &Options<'_>) -> String {
    render_inlines_at_link_depth(nodes, options, 0)
}

/// A container label's inline run, rendered.
///
/// ITS CONTENT IS AN INLINE RUN, not the characters the author typed: a container
/// label is a delimited region parsed as `inline_content` (`carve#2604`'s
/// definition, ruled on markup-carve/carve#2572). Escaping it instead published
/// `a /b/` where every other inline host publishes `a <em>b</em>`, and it made one
/// host answer two ways, since a trailing `%%` comment in a label is already read
/// as a run and cut there.
///
/// Every `<p class="div-label">` goes through here - the core floor and the
/// extensions that write their own - so the caption floor has one answer rather
/// than one per writer. See [`crate::parse::parse_container_label_inlines`] for
/// where the run stops.
pub(crate) fn render_container_label(label: &str, options: &Options<'_>) -> String {
    render_inlines_with_options(
        &crate::parse::parse_container_label_inlines(label, options),
        options,
    )
}

/// Render inline nodes that will be placed INSIDE an anchor the caller emits.
///
/// A derived display text is not free-standing prose: a table-of-contents entry
/// is written into an `<a href="#id">` the list builder emits itself, so a
/// construct in it that would open its own anchor has to be told, exactly as it
/// is told when it sits in a link's label. Without this the entry rendered at
/// depth 0 and a heading holding a mention, a tag or a cross-reference published
/// an `<a>` inside the entry's own `<a>` (PART 12 section 3a, LINKS NEVER NEST).
pub(crate) fn render_inlines_inside_anchor(nodes: &[InlineNode], options: &Options<'_>) -> String {
    render_inlines_at_link_depth(nodes, options, 1)
}

pub(super) fn render_inlines_at_link_depth(
    nodes: &[InlineNode],
    options: &Options<'_>,
    link_depth: usize,
) -> String {
    let mut out = String::new();
    let mut state = RenderState {
        heading_id_options: options.heading_id_options(),
        link_depth,
        ..RenderState::default()
    };
    render_inlines(&mut out, nodes, options, &mut state);
    out
}

pub(super) fn render_inlines(
    out: &mut String,
    nodes: &[InlineNode],
    options: &Options<'_>,
    state: &mut RenderState,
) {
    render_inlines_stateful(out, nodes, options, state);
}

pub(super) fn render_inlines_stateful(
    out: &mut String,
    nodes: &[InlineNode],
    options: &Options<'_>,
    state: &mut RenderState,
) {
    if state.inline_depth > MAX_RENDER_DEPTH {
        crate::render_depth::record("html");
        return;
    }
    state.inline_depth += 1;
    for node in nodes {
        render_inline_after(out, node, options, state);
    }
    state.inline_depth -= 1;
}

pub(super) const SEMANTIC_SPAN_ORDER: [&str; 3] = ["abbr", "time", "kbd"];

/// The full order, including the four names an extension may add (PART 9 §10).
pub(crate) const EXTENDED_SEMANTIC_SPAN_ORDER: [&str; 7] =
    ["abbr", "time", "samp", "var", "kbd", "cite", "dfn"];

/// The attribute an authored VALUE on a semantic name reaches the output as.
///
/// `None` says the value only selects the wrapper and is dropped - which is
/// what `lint::lint_carve` reports as `semantic-attribute-value-ignored`. The
/// lint reads this rather than keeping a list of its own, so a name that starts
/// or stops carrying its value cannot be right in one place and stale in the
/// other (markup-carve/carve#1131).
pub(crate) fn semantic_value_target(name: &str) -> Option<&'static str> {
    match name {
        // `dfn` is the SemanticSpan extension's, and maps its value the same
        // way `abbr` does (docs/extensions.md §11.1). The mapping lives here
        // rather than in the extension for the same reason the order does: one
        // implementation, not two that drift.
        "abbr" | "dfn" => Some("title"),
        "time" => Some("datetime"),
        _ => None,
    }
}

/// The names this render consumes, in the canonical order: core's three plus
/// whatever a registered extension claims.
pub(crate) fn semantic_span_order(options: &Options<'_>) -> Vec<&'static str> {
    if options
        .extensions
        .iter()
        .all(|ext| ext.semantic_span_names().is_empty())
    {
        return SEMANTIC_SPAN_ORDER.to_vec();
    }
    EXTENDED_SEMANTIC_SPAN_ORDER
        .iter()
        .copied()
        .filter(|name| {
            SEMANTIC_SPAN_ORDER.contains(name)
                || options
                    .extensions
                    .iter()
                    .any(|ext| ext.semantic_span_names().contains(name))
        })
        .collect()
}

/// PART 10 §10 compact semantic attributes on an ordinary authored span.
pub(super) fn render_semantic_span(
    out: &mut String,
    span: &Span,
    options: &Options<'_>,
    state: &mut RenderState,
) {
    let Some(attrs) = &span.attrs else {
        out.push_str("<span>");
        render_inlines_stateful(out, &span.children, options, state);
        out.push_str("</span>");
        return;
    };
    let order = semantic_span_order(options);
    let names: Vec<&str> = order
        .iter()
        .copied()
        .filter(|name| attrs.key_values.contains_key(*name))
        .collect();
    if names.is_empty() {
        out.push_str("<span");
        write_attrs(out, &span.attrs);
        out.push('>');
        render_inlines_stateful(out, &span.children, options, state);
        out.push_str("</span>");
        return;
    }

    let mut html = String::new();
    let previous_suppression = state.suppress_automatic_abbreviation;
    if attrs.key_values.contains_key("abbr") {
        state.suppress_automatic_abbreviation = true;
    }
    render_inlines_stateful(&mut html, &span.children, options, state);
    state.suppress_automatic_abbreviation = previous_suppression;
    let mut rest = attrs.clone();
    rest.key_values
        .retain(|name, _| !order.contains(&name.as_str()));
    rest.order.retain(|slot| match slot {
        AttrSlot::Key(name) => !order.contains(&name.as_str()),
        _ => true,
    });
    let outermost = *names.last().expect("semantic names is non-empty");
    for name in names {
        let value = &attrs.key_values[name];
        let mapped = if value.is_empty() {
            None
        } else {
            semantic_value_target(name).map(|key| (key, value.as_str()))
        };

        let mut own = Attrs::default();
        if let Some((key, value)) = mapped {
            // A derived attribute occupies the same slot as an authored one.
            // The authored value in `rest` therefore wins instead of producing
            // a duplicate attribute.
            if name != outermost || !rest.key_values.contains_key(key) {
                own.key_values.insert(key.to_string(), value.to_string());
                own.order.push(AttrSlot::Key(key.to_string()));
            }
        }
        if name == outermost {
            own.id = rest.id.clone();
            own.classes = rest.classes.clone();
            for (key, value) in &rest.key_values {
                own.key_values.insert(key.clone(), value.clone());
            }
            own.order.extend(rest.order.iter().cloned());
        }
        html = format!("<{name}{}>{html}</{name}>", render_attrs(&Some(own)));
    }
    out.push_str(&html);
}

/// Escape text content (`& < >`) and fold the no-break space U+00A0 into
/// `&nbsp;`, writing directly into `out`. Equivalent to
/// `escape_text(s).replace('\u{00a0}', "&nbsp;")` but in one pass with no
/// intermediate allocations.
pub(super) fn write_escaped_text_nbsp(out: &mut String, input: &str) {
    let mut start = 0;
    for (i, ch) in input.char_indices() {
        let entity = match ch {
            '&' => "&amp;",
            '<' => "&lt;",
            '>' => "&gt;",
            // Both a literal U+00A0 and the generated-NBSP placeholder render
            // as `&nbsp;` in HTML; they only diverge in plain/ANSI output.
            '\u{00a0}' | crate::NBSP_PLACEHOLDER => "&nbsp;",
            // Trojan-Source bidi-override / isolate controls are REMOVED (not
            // escaped) from rendered text and code: an entity reference decodes
            // back to the raw control and still reorders the DOM, so only
            // physical removal is inert. See `escape::is_bidi_control`.
            c if crate::escape::is_bidi_control(c) => {
                out.push_str(&input[start..i]);
                start = i + ch.len_utf8();
                continue;
            }
            _ => continue,
        };
        out.push_str(&input[start..i]);
        out.push_str(entity);
        start = i + ch.len_utf8();
    }
    out.push_str(&input[start..]);
}

pub(super) fn render_inline_after(
    out: &mut String,
    node: &InlineNode,
    options: &Options<'_>,
    state: &mut RenderState,
) {
    match node {
        // A comment renders to nothing on every target but the canonical
        // Carve writer, which is where the author gets it back.
        InlineNode::Comment(_) => {}
        InlineNode::Text(s) => {
            // Escape `& < >` AND fold U+00A0 to `&nbsp;` in a single pass over
            // `out`. None of the escaped chars is U+00A0, so the combined pass
            // is byte-identical to `escape_text(..).replace('\u{00a0}', ..)`.
            write_escaped_text_nbsp(out, &s.value);
        }
        InlineNode::EscapedText(s) => {
            // The backslash is authoring syntax; the reader sees the character.
            // PART 10 §13a: under roundtrip export the character is wrapped in a
            // span whose presence carrier says it was escaped, so an import puts
            // the backslash back instead of letting the character reparse as
            // markup. ONE ESCAPE PER ELEMENT, ruled on carve#2902 question 4: a
            // multi-character value imported as `\a\b\c`, which is not Carve.
            if options.html_carriers() {
                out.push_str("<span");
                write_bare_carrier(
                    out,
                    crate::roundtrip::ESCAPED,
                    &None,
                    s.pos.as_ref(),
                    crate::render_loss::RawNodeType::Inline,
                );
                out.push('>');
                write_escaped_text_nbsp(out, &s.value);
                out.push_str("</span>");
            } else {
                write_escaped_text_nbsp(out, &s.value);
            }
        }
        InlineNode::SmartPunctuation(s) => {
            // Source mode reproduces what the author typed; the glyph is a
            // presentation choice a machine consumer cannot reverse. The node
            // carries both halves, so this is a rendering decision and the
            // tree is the same either way (divergence-from-djot section 12).
            //
            // Heading ids are deliberately NOT affected: they slug from
            // `plain_inlines`, which keeps reading the glyph and normalizes it
            // back to ASCII, so `# Don't repeat yourself` gives the same id in
            // both modes.
            let text = if options.smart_typography.uses_source(&s.kind) {
                s.value.as_str()
            } else {
                smart_punctuation_glyph(s)
            };
            write_escaped_text_nbsp(out, text);
        }
        InlineNode::Emphasis(e) => render_emphasis(out, e, options, state),
        InlineNode::Code(s) => {
            out.push_str("<code");
            write_attrs(out, &s.attrs);
            out.push('>');
            // Escape `& < >` AND fold U+00A0 to `&nbsp;`, matching the prose
            // text path: a literal no-break space inside a code span serializes
            // as the named entity in HTML (corpus 49-non-breaking-space-3).
            write_escaped_text_nbsp(out, &s.value);
            out.push_str("</code>");
        }
        InlineNode::Link(l) => render_link(out, l, options, state),
        InlineNode::Image(img) => {
            render_image(out, img, options, crate::render_loss::RawNodeType::Inline)
        }
        InlineNode::Span(s) => {
            render_semantic_span(out, s, options, state);
        }
        InlineNode::Ruby(r) => {
            out.push_str("<ruby");
            write_attrs(out, &r.attrs);
            out.push('>');
            for pair in &r.pairs {
                render_inlines_stateful(out, &pair.base, options, state);
                out.push_str("<rp>(</rp><rt>");
                render_inlines_stateful(out, &pair.annotation, options, state);
                out.push_str("</rt><rp>)</rp>");
            }
            out.push_str("</ruby>");
        }
        InlineNode::Math(m) => {
            let base = if m.display {
                "math display"
            } else {
                "math inline"
            };
            let open = if m.display { "\\[" } else { "\\(" };
            let close = if m.display { "\\]" } else { "\\)" };
            // PART 10 SS1: the `math {inline,display}` class is a mandatory BASE
            // class, so it is prepended INSIDE the author's class slot and the
            // slot keeps its first-appearance position. Emitting `class="..."`
            // unconditionally first put it ahead of an id the author wrote
            // before any class, which reorders what they wrote.
            //
            // carve#1168 fixed exactly this for the generic `ext-NAME` fallback
            // and left the helper below behind; the math span carries a base
            // class the same way and was missed, because no corpus case put an
            // id before a class on it (carve#1164).
            let attrs = render_attrs_with_base_class(&m.attrs, base);
            let authored_role = m.attrs.as_ref().is_some_and(|attrs| {
                attrs
                    .key_values
                    .keys()
                    .any(|key| key.eq_ignore_ascii_case("role"))
            });
            let role = if authored_role { "" } else { " role=\"math\"" };
            // Static mode: when a build-time math renderer is supplied, emit its
            // server-side output (MathML / HTML) inside the math span so the page
            // needs no client KaTeX / MathJax; the renderer output is trusted and
            // emitted verbatim. Absent a renderer (or in interactive mode), fall
            // back to the same delimiter-wrapped, HTML-escaped source - never
            // blank. Mirrors carve-js `render-html.ts` `case 'math'`.
            let body = match (options.is_static(), &options.renderers.math) {
                (true, Some(build)) => build(&m.content, m.display),
                _ => format!("{}{}{}", open, escape_text(&m.content), close),
            };
            out.push_str(&format!("<span{}{}>{}</span>", attrs, role, body,));
            // PART 9 §19: a directly numbered display equation renders its label
            // and number after the math span, one U+0020 apart and unbracketed.
            // `attrs.id` stays on the formula, so a resolved cross-reference
            // lands there rather than on the number.
            if let (Some(label), Some(number)) = (&m.label, m.number) {
                out.push_str(" <span class=\"equation-number\">");
                out.push_str(&escape_text(label));
                out.push(' ');
                out.push_str(&number.to_string());
                out.push_str("</span>");
            }
        }
        InlineNode::RawInline(r) => {
            let html = r.format.trim() == "html";
            // PART 10 §13: carrier payload is escaped text. Trusted HTML keeps
            // its payload bytes; the ordinary disabled-raw path still strips
            // bidi controls through escape_text.
            if options.html_carriers() {
                // The carrier restores the node; the target still did not write
                // the format, so the row is owed in both modes.
                if !html {
                    crate::render_loss::record_raw_drop(
                        &r.format,
                        crate::RawNodeType::Inline,
                        r.pos.clone(),
                    );
                }
                out.push_str("<span");
                write_carrier(
                    out,
                    crate::roundtrip::RAW,
                    &r.format,
                    &None,
                    r.pos.as_ref(),
                    crate::render_loss::RawNodeType::Inline,
                );
                out.push('>');
                if html && options.allow_raw_html {
                    write_escaped_text_inner(out, &r.content);
                } else {
                    out.push_str(&escape_text(&r.content));
                }
                out.push_str("</span>");
            } else if html {
                // Escape instead of emitting when raw HTML is disabled.
                if options.allow_raw_html {
                    out.push_str(&r.content);
                } else {
                    out.push_str(&escape_text(&r.content));
                }
            } else {
                crate::render_loss::record_raw_drop(
                    &r.format,
                    crate::RawNodeType::Inline,
                    r.pos.clone(),
                );
            }
        }
        InlineNode::LiteralInline(lit) => {
            // §27: content is escaped and ALWAYS emitted (never target-routed
            // like raw passthrough), with the `<code>` wrapper dropped. A
            // `<span>` is emitted only when an attribute needs somewhere to live;
            // otherwise the escaped content is bare prose.
            if lit.attrs.is_some() {
                out.push_str("<span");
                write_attrs(out, &lit.attrs);
                out.push('>');
                write_escaped_text_nbsp(out, &lit.content);
                out.push_str("</span>");
            } else {
                write_escaped_text_nbsp(out, &lit.content);
            }
        }
        InlineNode::Symbol(e) => {
            if e.attrs.is_some() {
                out.push_str("<span");
                write_attrs(out, &e.attrs);
                out.push('>');
            }
            if let Some(value) = options.symbols.get(&e.name) {
                out.push_str(value);
            } else {
                out.push(':');
                write_escaped_text(out, &e.name);
                out.push(':');
            }
            if e.attrs.is_some() {
                out.push_str("</span>");
            }
        }
        InlineNode::AutoLink(a) => {
            // Display the raw autolink content (a URI autolink keeps its scheme).
            let display = a.text.as_str();
            out.push_str("<a href=\"");
            write_escaped_attr(
                out,
                &sanitize_destination(
                    &a.href,
                    a.pos.as_ref(),
                    crate::render_loss::DeniedSink::Destination,
                ),
            );
            out.push('"');
            // PART 10 §13a: presence says the link was authored as an autolink,
            // which HTML's `<a href>` cannot tell from `[text](url)` where the
            // text happens to equal the destination.
            if options.html_carriers() {
                write_carrier(
                    out,
                    crate::roundtrip::AUTOLINK,
                    "1",
                    &a.attrs,
                    a.pos.as_ref(),
                    crate::render_loss::RawNodeType::Inline,
                );
                out.push_str(&render_attrs_without_keys(
                    &a.attrs,
                    &[crate::roundtrip::AUTOLINK],
                ));
            } else {
                write_attrs(out, &a.attrs);
            }
            out.push('>');
            write_escaped_text(out, display);
            out.push_str("</a>");
        }
        InlineNode::CrossRef(c) => {
            // The label is the target's cloned inline NODES, so it still
            // carries what the author typed and renders here exactly as it does
            // in the heading itself (PART 9R R4). A caption target has no nodes
            // - its label is LABEL + NUMBER - so that one stays a string.
            let resolved = state.crossref_index.resolve(&c.target).map(|(id, title)| {
                (
                    id.to_string(),
                    if crate::abbr_budget::label_rejected(&c.target) {
                        String::new()
                    } else {
                        title.to_string()
                    },
                )
            });
            if let Some((actual_id, title)) = resolved {
                let rejected = crate::abbr_budget::label_rejected(&c.target);
                let label = if rejected {
                    None
                } else {
                    state.crossref_index.label(&actual_id)
                };
                let opens_anchor = state.link_depth == 0;
                let mut text = String::new();
                match &label {
                    // Inside the anchor this link is about to open.
                    Some(nodes) => {
                        if opens_anchor {
                            state.link_depth += 1;
                        }
                        render_inlines(&mut text, nodes, options, state);
                        if opens_anchor {
                            state.link_depth -= 1;
                        }
                    }
                    None => {
                        write_escaped_text(&mut text, if rejected { &c.target } else { &title })
                    }
                }
                let text = charge_crossref_label(text, &c.target);
                if opens_anchor {
                    out.push_str("<a href=\"#");
                    write_escaped_attr(out, &actual_id);
                    out.push('"');
                    if options.html_carriers() {
                        out.push_str(" data-carve-xref");
                    }
                    out.push('>');
                    out.push_str(&text);
                    out.push_str("</a>");
                } else {
                    out.push_str(&text);
                }
            } else {
                write_escaped_text(out, &format!("</#{}>", c.target));
            }
        }
        InlineNode::CaptionNumber(n) => {
            match n.number {
                Some(number) if options.html_carriers() => {
                    write!(out, "<span data-carve-caption-number>{number}</span>").unwrap();
                }
                Some(number) => out.push_str(&number.to_string()),
                // An unresolved placeholder stays the literal `#` the author
                // wrote - the visible failure this language prefers to a
                // silent one (PART 9 §4c names the panel-caption case), and
                // what the Markdown, plain and terminal targets already emit.
                None => out.push('#'),
            }
        }
        InlineNode::Mention(m) => {
            // LINKS NEVER NEST (PART 12 section 3a). A mention with a URL
            // template opens its own anchor, so inside one it renders the
            // template-less form instead - the same test the cross-reference
            // above makes, and the reason it is made HERE rather than by
            // pruning the node is that only the renderer knows whether a
            // template was configured at all.
            let href = if state.link_depth != 0 {
                None
            } else if let Some(resolver) = options.mention_resolver {
                resolver(&SocialLinkResolverInput {
                    kind: SocialLinkKind::Mention,
                    name: &m.user,
                    attrs: m.attrs.as_ref(),
                    context: options.social_context,
                })
                .ok()
                .flatten()
            } else {
                options.mention_url.as_ref().map(|template| {
                    let encoded = percent_encode(&m.user);
                    template
                        .replace("{name}", &encoded)
                        .replace("{user}", &encoded)
                })
            };
            // Asked, not blanked: a denied URL here suppresses the anchor
            // entirely, so no destination is emitted and PART 9 §25 owes no row.
            let href = href.filter(|href| !href.is_empty() && !has_denied_url_scheme(href));
            if let Some(href) = href {
                let (class, _) = structural_attrs("mention", &m.attrs);
                out.push_str("<a class=\"");
                write_escaped_attr(out, &class);
                out.push_str("\" href=\"");
                write_escaped_attr(out, &href);
                out.push('"');
                out.push_str(&render_attrs_after_class_without_keys(&m.attrs, &["href"]));
                out.push_str(">@");
                write_escaped_text(out, &m.user);
                out.push_str("</a>");
            } else {
                let (class, rest) = structural_attrs("mention", &m.attrs);
                out.push_str("<span class=\"");
                write_escaped_attr(out, &class);
                out.push('"');
                out.push_str(&rest);
                out.push_str("><strong>@");
                write_escaped_text(out, &m.user);
                out.push_str("</strong></span>");
            }
        }
        InlineNode::Tag(t) => {
            // LINKS NEVER NEST (PART 12 section 3a); see the mention above.
            let href = if state.link_depth != 0 {
                None
            } else if let Some(resolver) = options.tag_resolver {
                resolver(&SocialLinkResolverInput {
                    kind: SocialLinkKind::Tag,
                    name: &t.name,
                    attrs: t.attrs.as_ref(),
                    context: options.social_context,
                })
                .ok()
                .flatten()
            } else {
                options.tag_url.as_ref().map(|template| {
                    let encoded = percent_encode(&t.name);
                    template.replace("{name}", &encoded)
                })
            };
            // Asked, not blanked: a denied URL here suppresses the anchor
            // entirely, so no destination is emitted and PART 9 §25 owes no row.
            let href = href.filter(|href| !href.is_empty() && !has_denied_url_scheme(href));
            if let Some(href) = href {
                let (class, _) = structural_attrs("tag", &t.attrs);
                out.push_str("<a class=\"");
                write_escaped_attr(out, &class);
                out.push_str("\" href=\"");
                write_escaped_attr(out, &href);
                out.push('"');
                out.push_str(&render_attrs_after_class_without_keys(&t.attrs, &["href"]));
                out.push_str(">#");
                write_escaped_text(out, &t.name);
                out.push_str("</a>");
            } else {
                let (class, rest) = structural_attrs("tag", &t.attrs);
                out.push_str("<span class=\"");
                write_escaped_attr(out, &class);
                out.push('"');
                out.push_str(&rest);
                out.push_str("><strong>#");
                write_escaped_text(out, &t.name);
                out.push_str("</strong></span>");
            }
        }
        InlineNode::CitationGroup(g) => render_citation_group(out, g, options, state),
        InlineNode::Extension(e) => render_inline_extension(out, e, options, state),
        InlineNode::Abbreviation(a) => {
            if state.suppress_automatic_abbreviation {
                write_escaped_text(out, &a.abbr);
                return;
            }
            // Bound cumulative expansion bytes: once the budget is exhausted,
            // degrade to plain key text (no `<abbr>`, no title) so a large
            // expansion repeated many times cannot amplify output without limit.
            if crate::abbr_budget::try_spend(a.expansion.len()) {
                out.push_str("<abbr title=\"");
                write_escaped_attr(out, &a.expansion);
                out.push_str("\">");
                write_escaped_text(out, &a.abbr);
                out.push_str("</abbr>");
            } else {
                write_escaped_text(out, &a.abbr);
            }
        }
        InlineNode::Footnote(f) => {
            if let (Some(number), Some(ref_id)) = (f.number, &f.ref_id) {
                if state.link_depth > 0 {
                    out.push_str("<sup id=\"");
                    write_escaped_attr(out, ref_id);
                    out.push('"');
                    if options.html_carriers() {
                        if let Some(label) = &f.id {
                            write_carrier(
                                out,
                                crate::roundtrip::FOOTNOTE_LABEL,
                                label,
                                &f.attrs,
                                f.pos.as_ref(),
                                crate::render_loss::RawNodeType::Inline,
                            );
                        }
                        out.push_str(&render_attrs_without_id_and_keys(
                            &f.attrs,
                            &[crate::roundtrip::FOOTNOTE_LABEL],
                        ));
                    } else {
                        out.push_str(&render_attrs_without_id_for(&f.attrs, "sup"));
                    }
                    // The reference keeps its back-link target inside the link text.
                    write!(out, ">{number}</sup>").unwrap();
                    return;
                }
                out.push_str("<a id=\"");
                write_escaped_attr(out, ref_id);
                write!(out, "\" href=\"#fn{number}\" role=\"doc-noteref\"").unwrap();
                // PART 10 §13a: the authored label, on the reference and on the
                // endnote item. The rendered `1` is a position, and a document
                // whose labels were `[^note]` comes back numbered without this.
                if options.html_carriers() {
                    if let Some(label) = &f.id {
                        write_carrier(
                            out,
                            crate::roundtrip::FOOTNOTE_LABEL,
                            label,
                            &f.attrs,
                            f.pos.as_ref(),
                            crate::render_loss::RawNodeType::Inline,
                        );
                    }
                }
                out.push_str(&render_attrs_without_id_and_keys(
                    &f.attrs,
                    &[crate::roundtrip::FOOTNOTE_LABEL],
                ));
                write!(out, "><sup>{number}</sup></a>").unwrap();
            } else if let Some(id) = &f.id {
                write_escaped_text(out, &format!("[^{id}]"));
            }
        }
        InlineNode::NonBreakingSpace(n) => {
            if n.attrs.is_some() {
                out.push_str(&format!("<span{}>&nbsp;</span>", render_attrs(&n.attrs)));
            } else {
                out.push_str("&nbsp;");
            }
        }
        InlineNode::SoftBreak(_) => out.push('\n'),
        InlineNode::CellWrap(_) => out.push(' '),
        InlineNode::HardBreak(_) => out.push_str("<br>\n"),
        InlineNode::CriticInsert(c) => {
            out.push_str("<ins");
            write_attrs(out, &c.attrs);
            out.push('>');
            render_inlines_stateful(out, &c.children, options, state);
            out.push_str("</ins>");
        }
        InlineNode::CriticDelete(c) => {
            out.push_str("<del");
            write_attrs(out, &c.attrs);
            out.push('>');
            render_inlines_stateful(out, &c.children, options, state);
            out.push_str("</del>");
        }
        InlineNode::CriticSubstitute(c) => {
            let carried = options.html_carriers();
            if carried {
                write!(out, "<span {}>", crate::roundtrip::SUBSTITUTION).unwrap();
            }
            out.push_str("<del>");
            render_inlines_stateful(out, &c.old, options, state);
            out.push_str("</del><ins>");
            render_inlines_stateful(out, &c.new, options, state);
            out.push_str("</ins>");
            if carried {
                out.push_str("</span>");
            }
        }
        InlineNode::CriticComment(c) => out.push_str(&format!(
            "<span class=\"critic-comment\">{}</span>",
            escape_text(&c.text)
        )),
    }
}

pub(super) const INTEGRAL_OPEN: &str = "<span class=\"citation\" data-cite-mode=\"integral\">";

pub(super) fn render_citation_group(
    out: &mut String,
    g: &CitationGroup,
    options: &Options<'_>,
    state: &mut RenderState,
) {
    if g.items.iter().any(|item| item.label.is_none()) {
        out.push_str(&escape_text(&g.raw));
        return;
    }

    // Whole-group wrapper when every item is integral; otherwise the marking
    // sits on the items that carry it, which is the only way a mixed group
    // renders its two halves differently (carve-rs#1858).
    let all_integral = g.integral();
    if all_integral {
        out.push_str(INTEGRAL_OPEN);
    }

    let (open, close, separator) = match g.render_mode.unwrap_or(CitationRenderMode::Numbered) {
        CitationRenderMode::Numbered => ('[', ']', ", "),
        CitationRenderMode::AuthorDate => ('(', ')', "; "),
    };
    out.push(open);
    for (idx, item) in g.items.iter().enumerate() {
        if idx > 0 {
            out.push_str(separator);
        }
        let wrap_item = !all_integral && item.mode == Some(CitationItemMode::Integral);
        if wrap_item {
            out.push_str(INTEGRAL_OPEN);
        }
        render_citation_item(out, item, options, state);
        if wrap_item {
            out.push_str("</span>");
        }
    }
    out.push(close);

    if all_integral {
        out.push_str("</span>");
    }
}

/// Flatten inline nodes to plain text (for `data-*` attribute values).
pub(super) fn flatten_text(nodes: &[InlineNode]) -> String {
    flatten_text_at(nodes, 0)
}

pub(super) fn flatten_text_at(nodes: &[InlineNode], depth: usize) -> String {
    if depth > MAX_RENDER_DEPTH {
        crate::render_depth::record("html");
        return String::new();
    }
    let mut out = String::new();
    for node in nodes {
        match node {
            InlineNode::NonBreakingSpace(_) => out.push('\u{00a0}'),
            InlineNode::Text(t) => out.push_str(&t.value),
            InlineNode::SmartPunctuation(s) => out.push_str(smart_punctuation_glyph(s)),
            InlineNode::Emphasis(e) => out.push_str(&flatten_text_at(&e.children, depth + 1)),
            InlineNode::Link(l) => out.push_str(&flatten_text_at(&l.children, depth + 1)),
            InlineNode::Span(s) => out.push_str(&flatten_text_at(&s.children, depth + 1)),
            InlineNode::Ruby(r) => {
                for pair in &r.pairs {
                    out.push_str(&flatten_text_at(&pair.base, depth + 1));
                    out.push('(');
                    out.push_str(&flatten_text_at(&pair.annotation, depth + 1));
                    out.push(')');
                }
            }
            InlineNode::Extension(e) => out.push_str(&flatten_text_at(&e.children, depth + 1)),
            _ => {}
        }
    }
    out
}

pub(super) fn render_citation_item(
    out: &mut String,
    item: &Citation,
    options: &Options<'_>,
    state: &mut RenderState,
) {
    if let Some(prefix) = &item.prefix {
        render_inlines(out, prefix, options, state);
        out.push(' ');
    }

    // Build data-* attribute string (order is normative per spec).
    let mut attrs = format!("data-cite-key=\"{}\"", escape_attr(&item.key));
    if item.suppress_author {
        attrs.push_str(" data-suppress-author=\"true\"");
    }
    let pfx = flatten_text(item.prefix.as_deref().unwrap_or(&[]));
    if !pfx.is_empty() {
        attrs.push_str(&format!(" data-cite-prefix=\"{}\"", escape_attr(&pfx)));
    }
    if let Some(ll) = &item.locator_label {
        attrs.push_str(&format!(" data-locator-label=\"{}\"", escape_attr(ll)));
    }
    if let Some(lv) = &item.locator_value {
        attrs.push_str(&format!(" data-locator=\"{}\"", escape_attr(lv)));
    }
    let sfx = flatten_text(item.suffix.as_deref().unwrap_or(&[]));
    if !sfx.is_empty() {
        attrs.push_str(&format!(" data-suffix=\"{}\"", escape_attr(&sfx)));
    }

    // A use_index is only set when a bibliography pool is active (#199); it adds
    // the per-use back-link anchor to the existing forward link. Both ids are
    // read from the per-render document id namespace (extensions contract
    // §2.6), so a collision with an explicit `{#id}` or a heading id bumps the
    // anchor id / href consistently with the references list.
    let ref_id = crate::document_ids::ref_id(&item.key);
    match item.use_index {
        Some(n) => out.push_str(&format!(
            "<a id=\"{}\" {} href=\"#{}\">{}</a>",
            escape_attr(&crate::document_ids::cite_id(&item.key, n)),
            attrs,
            escape_attr(&ref_id),
            escape_text(item.label.as_deref().unwrap_or_default())
        )),
        None => out.push_str(&format!(
            "<a {} href=\"#{}\">{}</a>",
            attrs,
            escape_attr(&ref_id),
            escape_text(item.label.as_deref().unwrap_or_default())
        )),
    }
    if let Some(locator) = &item.locator {
        out.push_str(", ");
        render_inlines(out, locator, options, state);
    }
}

pub(super) fn percent_encode(input: &str) -> String {
    let mut out = String::new();
    for byte in input.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

pub(super) fn render_emphasis(
    out: &mut String,
    e: &Emphasis,
    options: &Options<'_>,
    state: &mut RenderState,
) {
    let (open, close) = match e.kind {
        EmphasisKind::Italic => ("em", "em"),
        EmphasisKind::Strong => ("strong", "strong"),
        EmphasisKind::Underline => ("u", "u"),
        EmphasisKind::Strike => ("s", "s"),
        EmphasisKind::Super => ("sup", "sup"),
        EmphasisKind::Sub => ("sub", "sub"),
        EmphasisKind::Highlight => ("mark", "mark"),
        EmphasisKind::BoldItalic => ("strong", "em"),
        EmphasisKind::SmallCaps => ("span", "span"),
    };
    // CARVE-P12-050: the small-caps span's `smallcaps` is a BASE class, merged
    // into the author's class slot the way the math span's is, so an id written
    // before a class keeps its place.
    let attrs = if e.kind == EmphasisKind::SmallCaps {
        render_attrs_with_base_class(&e.attrs, "smallcaps")
    } else {
        render_attrs(&e.attrs)
    };
    if e.kind == EmphasisKind::BoldItalic {
        // The combined token's trailing attribute block is the one
        // `[attributes]` slot of `bold_italic`, carried by the outer element.
        out.push_str(&format!("<{open}{attrs}><{close}>"));
        render_inlines_stateful(out, &e.children, options, state);
        out.push_str(&format!("</{close}></{open}>"));
    } else {
        out.push_str(&format!("<{}{}>", open, attrs));
        render_inlines_stateful(out, &e.children, options, state);
        out.push_str(&format!("</{}>", close));
    }
}

pub(super) fn render_link(
    out: &mut String,
    l: &Link,
    options: &Options<'_>,
    state: &mut RenderState,
) {
    if crate::parse::is_unresolved_reference(l) {
        // The literal source is text content (PART 10 SS2), so it gets the
        // same nbsp fold every other text node does, not a bare `& < >` pass.
        write_escaped_text_nbsp(out, l.raw_ref.as_deref().unwrap_or_default());
        return;
    }
    // `href`, then the destination title, then the attribute block. A
    // destination title occupies the HTML `title` slot, so an authored
    // case-insensitive title key cannot produce a duplicate attribute.
    out.push_str(&format!(
        "<a href=\"{}\"",
        escape_attr(&sanitize_destination(
            &l.href,
            l.pos.as_ref(),
            crate::render_loss::DeniedSink::Destination,
        ))
    ));
    if let Some(title) = &l.title {
        out.push_str(&format!(" title=\"{}\"", escape_attr(title)));
    }
    // PART 10 §13a: the reference label the link resolved by.
    let mut blocked: Vec<&str> = if l.title.is_some() {
        vec!["href", "title"]
    } else {
        vec!["href"]
    };
    if options.html_carriers() {
        if let Some(label) = &l.ref_label {
            write_carrier(
                out,
                crate::roundtrip::REF,
                label,
                &l.attrs,
                l.pos.as_ref(),
                crate::render_loss::RawNodeType::Inline,
            );
            blocked.push(crate::roundtrip::REF);
        }
    }
    out.push_str(&render_attrs_without_keys(&l.attrs, &blocked));
    out.push('>');
    state.link_depth += 1;
    // Render the label through the anchor-unwrapping view.
    let children = unwrap_nested_anchors(&l.children);
    render_inlines_stateful(out, children.as_ref(), options, state);
    state.link_depth -= 1;
    out.push_str("</a>");
}

pub(super) fn render_inline_extension(
    out: &mut String,
    node: &InlineExtension,
    options: &Options<'_>,
    state: &mut RenderState,
) {
    let ctx = RenderContext::new(options);
    for ext in &options.extensions {
        if let Some(html) = ext.render_inline_extension(node, &ctx) {
            out.push_str(&html);
            return;
        }
    }
    // PART 9 §10: the SemanticSpan extension re-registers the seven names as a
    // SOFT-DEPRECATED spelling. Core registers none, so without the extension
    // every name falls through to the readable `ext-NAME` span below.
    if options
        .extensions
        .iter()
        .any(|ext| !ext.semantic_span_names().is_empty())
        && EXTENDED_SEMANTIC_SPAN_ORDER.contains(&node.name.as_str())
    {
        out.push_str(&format!("<{}{}>", node.name, render_attrs(&node.attrs)));
        render_inlines_stateful(out, &node.children, options, state);
        out.push_str(&format!("</{}>", node.name));
        return;
    }
    // The `ext-NAME` class is structural and emitted first; a trailing
    // attribute block merges its classes into the SAME `class` attribute
    // (`:foo[a]{.cls}` -> `class="ext-foo cls"`, never two `class` attrs) and
    // contributes id / key-values after. Matches the math-span merge and
    // carve-js / carve-php.
    let base = format!("ext-{}", node.name);
    out.push_str(&format!(
        "<span{}>",
        render_attrs_with_base_class(&node.attrs, &base)
    ));
    render_inlines_stateful(out, &node.children, options, state);
    out.push_str("</span>");
}
