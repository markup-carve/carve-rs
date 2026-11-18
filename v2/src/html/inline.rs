//! Inline elements (C2-H-069, C2-H-072 to C2-H-101, C2-H-139 to C2-H-142).

use super::Renderer;
use super::attrs::El;
use super::block::attr_str;
use super::{esc, url};
use crate::ast::{self, Attrs, ImageRefForm, LinkRefForm, Node};
use crate::hash::AddrSet;
use crate::options::Mode;
use crate::options::Profile;
use crate::resolve::order::addr;
use crate::tables::PROFILES;
use std::borrow::Cow;
use std::fmt::Write as _;

fn wrapper_tag(n: &Node) -> Option<&'static str> {
    Some(match n {
        Node::Emphasis(_) => "em",
        Node::Strong(_) => "strong",
        Node::Underline(_) => "u",
        Node::Strike(_) => "s",
        Node::Highlight(_) => "mark",
        Node::Superscript(_) => "sup",
        Node::Subscript(_) => "sub",
        Node::Insert(_) => "ins",
        Node::Delete(_) => "del",
        Node::SmallCaps(_) => "span",
        _ => return None,
    })
}

fn span_value(s: &ast::Span<'_>, key: &str, exact: bool) -> Option<String> {
    s.attrs
        .as_ref()
        .and_then(|attrs| {
            attrs.key_values.iter().find(|(name, _)| {
                if exact {
                    *name == key
                } else {
                    name.eq_ignore_ascii_case(key)
                }
            })
        })
        .map(|(name, value)| url::filter_value(name, value).to_string())
}

impl<'r, 'a> Renderer<'r, 'a> {
    /// The inline content of a leaf with its comment-only lines and one break
    /// beside each left out (C2-H-069).
    pub(super) fn leaf(&mut self, nodes: &'r [Node<'a>]) -> String {
        let mut out = String::new();
        self.leaf_into(nodes, &mut out);
        out
    }

    /// `leaf`, appended to `out`.
    pub(super) fn leaf_into(&mut self, nodes: &'r [Node<'a>], out: &mut String) {
        // Gap G-020: children degraded at the nesting bound keep their breaks.
        if !self.past_bound() {
            hide_comment_lines(nodes, &mut self.hidden);
        }
        self.inlines(nodes, out);
    }

    pub(super) fn inlines(&mut self, nodes: &'r [Node<'a>], out: &mut String) {
        for n in nodes {
            self.inline(n, out);
        }
    }

    fn inline(&mut self, n: &'r Node<'a>, out: &mut String) {
        if !self.hidden.is_empty() && self.hidden.contains(&addr(n)) {
            return;
        }
        if !self.profile_allows(n, false) {
            self.degrade_inline(n, out);
            return;
        }
        if self.level > crate::resolve::WALK_CAP {
            return;
        }
        self.level += 1;
        self.inline_inner(n, out);
        self.level -= 1;
    }

    fn inline_inner(&mut self, n: &'r Node<'a>, out: &mut String) {
        if self.simple_inline(n, out) {
            return;
        }
        match n {
            Node::Link(l) => self.link(n, l, out),
            Node::Autolink(a) => self.autolink(a, out),
            Node::Image(i) => self.image(i, None, None, out),
            Node::FootnoteRef(_) | Node::InlineFootnote(_) => self.note_mark(n, out),
            Node::Xref(x) => self.xref(n, x, out),
            Node::Span(s) => self.span(s, out),
            Node::Math(m) => self.math(m, out),
            _ => self.inline_inner_other(n, out),
        }
    }

    #[inline]
    fn inline_inner_other(&mut self, n: &'r Node<'a>, out: &mut String) {
        match n {
            Node::Abbreviation(a) => match (&a.expansion, self.in_abbr) {
                (Some(e), 0) if self.charge_derived(e.len(), n) => {
                    let mut el = El::new(a.attrs, a.lang).with_dir(a.dir);
                    el.structural.push(("title", Some((*e).into())));
                    out.push_str("<abbr");
                    el.write_into(self.filter(), out);
                    out.push('>');
                    esc::text(a.key, out);
                    out.push_str("</abbr>");
                }
                _ => esc::text(a.key, out),
            },
            // Gap HTML-018: a label is the visible text, written without the at-sign.
            Node::Mention(m) => {
                let text = m
                    .label
                    .map_or_else(|| format!("@{}", m.target), str::to_string);
                let b = Badge {
                    class: "mention",
                    name: m.target,
                    attrs: m.attrs,
                    lang: m.lang,
                    dir: m.dir,
                };
                self.badge(&b, &text, out);
            }
            Node::Tag(t) => {
                let b = Badge {
                    class: "tag",
                    name: t.name,
                    attrs: t.attrs,
                    lang: t.lang,
                    dir: t.dir,
                };
                self.badge(&b, &format!("#{}", t.name), out);
            }
            Node::Symbol(s) => {
                let mut inner = String::new();
                match self.opts.symbols.get(s.name) {
                    Some(v) => inner.push_str(v),
                    None => esc::text(&format!(":{}:", s.name), &mut inner),
                }
                self.wrap_span(s.attrs, s.lang, s.dir, &inner, out);
            }
            Node::RawInline(r) => self.raw_inline(r, out),
            Node::InlineExtension(e) => self.inline_extension(e, out),
            Node::Ruby(r) => {
                let _ = write!(
                    out,
                    "<ruby{}>",
                    El::new(r.attrs, r.lang)
                        .with_dir(r.dir)
                        .write(self.filter())
                );
                for p in r.pairs.iter() {
                    if let Node::RubyPair(p) = p {
                        self.inlines(p.base, out);
                        out.push_str("<rp>(</rp><rt>");
                        self.inlines(p.annotation, out);
                        out.push_str("</rt><rp>)</rp>");
                    }
                }
                out.push_str("</ruby>");
            }
            _ => self.wrapper(n, out),
        }
    }

    /// The nodes without children or attributes of their own.
    fn simple_inline(&mut self, n: &Node, out: &mut String) -> bool {
        match n {
            Node::Text(t) => esc::text(t.value, out),
            Node::SoftBreak(_) => out.push('\n'),
            Node::HardBreak(_) => out.push_str("<br>\n"),
            Node::NonBreakingSpace(_) => out.push_str("&nbsp;"),
            Node::SmartPunctuation(s) => {
                if self.opts.smart_typography {
                    esc::text(s.glyph, out);
                } else {
                    esc::text(s.source, out);
                }
            }
            Node::Comment(_) => {}
            Node::CriticComment(c) => {
                out.push_str("<span class=\"critic-comment\">");
                esc::text(c.value, out);
                out.push_str("</span>");
            }
            Node::CitationGroup(c) => esc::text(c.raw, out),
            Node::CaptionNumber(c) => match c.n {
                Some(v) if self.opts.roundtrip => {
                    let _ = write!(out, "<span data-carve-caption-number>{v}</span>");
                }
                Some(v) => out.push_str(&v.to_string()),
                None => out.push('#'),
            },
            _ => return false,
        }
        true
    }

    /// Elements that wrap their children (C2-H-072).
    fn wrapper(&mut self, n: &'r Node<'a>, out: &mut String) {
        let Some(tag) = wrapper_tag(n) else {
            match n {
                Node::Code(c) => {
                    let el = El::new(c.attrs, c.lang).with_dir(c.dir);
                    out.push_str("<code");
                    el.write_into(self.filter(), out);
                    out.push('>');
                    esc::text(c.value, out);
                    out.push_str("</code>");
                }
                Node::LiteralInline(l) => {
                    let inner = esc::text_str(l.value);
                    if l.attrs.is_some_and(|a| !a.is_empty()) || l.lang.is_some() || l.dir.is_some()
                    {
                        self.wrap_span(l.attrs, l.lang, l.dir, &inner, out);
                    } else {
                        out.push_str(&inner);
                    }
                }
                _ => {
                    if let Some(c) = n.children() {
                        self.inlines(c, out);
                    }
                }
            }
            return;
        };
        let mut el = El::new(n.attrs(), lang_of(n)).with_dir(n.dir());
        if matches!(n, Node::SmallCaps(_)) {
            el.base.push("smallcaps".into());
        }
        out.push('<');
        out.push_str(tag);
        el.write_into(self.filter(), out);
        out.push('>');
        if let Some(c) = n.children() {
            self.inlines(c, out);
        }
        out.push_str("</");
        out.push_str(tag);
        out.push('>');
    }

    fn wrap_span(
        &self,
        attrs: Option<&Attrs>,
        lang: Option<&str>,
        dir: Option<&str>,
        inner: &str,
        out: &mut String,
    ) {
        if attrs.is_some_and(|a| !a.is_empty()) || lang.is_some() || dir.is_some() {
            let _ = write!(
                out,
                "<span{}>{inner}</span>",
                El::new(attrs, lang).with_dir(dir).write(self.filter())
            );
        } else {
            out.push_str(inner);
        }
    }

    /// The rel words of the profile (C2-H-167).
    fn rel(&self, el: &mut El<'_>) {
        let profile: &[&str] = PROFILES
            .iter()
            .find(|p| p.name == self.opts.profile.name())
            .map_or(&[], |p| p.link_rel);
        let authored = el
            .attrs
            .and_then(|a| {
                a.key_values
                    .iter()
                    .find(|(k, _)| k.eq_ignore_ascii_case("rel"))
            })
            .map(|(k, v)| url::filter_value(k, v));
        let mut words: Vec<String> = authored
            .iter()
            .flat_map(|v| v.split_ascii_whitespace().map(str::to_string))
            .collect();
        for w in profile {
            if !words.iter().any(|x| x == w) {
                words.push((*w).to_string());
            }
        }
        if authored.is_some() || !words.is_empty() {
            el.skip_keys = &["rel"];
        }
        if !words.is_empty() {
            el.structural.push(("rel", Some(words.join(" ").into())));
        }
    }

    /// A destination through the URL policy (C2-H-082, C2-H-160).
    fn destination<'d>(
        &mut self,
        dest: &'d str,
        pos: Option<ast::Pos>,
        image: bool,
    ) -> Cow<'d, str> {
        if url::allowed(dest, &self.opts.url_policy) {
            Cow::Borrowed(dest)
        } else {
            self.losses.push("destination-denied");
            self.destination_losses.push(super::DestinationLoss {
                image,
                attribute: false,
                pos,
                value: dest.to_string(),
            });
            Cow::Borrowed("")
        }
    }

    /// C2-H-079, C2-H-081, C2-H-083, C2-H-085, C2-H-088, C2-H-141.
    fn link(&mut self, n: &'r Node<'a>, l: &'r ast::Link<'a>, out: &mut String) {
        if self.in_link > 0 {
            self.inlines(&*l.children, out);
            return;
        }
        let Some(href) = l.href.filter(|href| {
            l.key.is_none_or(|key| self.links.contains_key(key)) || !self.hidden_heading(href)
        }) else {
            if let Some(key) = &l.key {
                out.push('[');
                self.inlines(&*l.children, out);
                out.push(']');
                push_label(out, l.ref_form.map(ref_suffix), key);
                out.push_str(&attr_text(l.attrs, l.lang));
                return;
            }
            return self.anchor(n, l, "", out);
        };
        if self.opts.profile == Profile::Comment && !url::allowed(href, &self.opts.url_policy) {
            self.losses.push("destination-denied");
            self.destination_losses.push(super::DestinationLoss {
                image: false,
                attribute: false,
                pos: l.pos,
                value: href.to_string(),
            });
            esc::text(
                &crate::resolve::plain::text_of_degraded_block(&*l.children),
                out,
            );
            return;
        }
        self.anchor(n, l, href, out);
    }

    fn anchor(&mut self, _n: &'r Node<'a>, l: &'r ast::Link<'a>, href: &str, out: &mut String) {
        let def = l.key.and_then(|k| self.links.get(k)).copied();
        let merged = merge_def(def.and_then(|d| d.attrs), l.attrs);
        let merged = merged.as_ref().map(Merged::attrs);
        let mut el = El::new(merged.as_ref(), l.lang).with_dir(l.dir);
        el.structural
            .push(("href", Some(self.destination(href, l.pos, false))));
        if let Some(t) = l.tooltip.as_ref().or(def.and_then(|d| d.tooltip.as_ref())) {
            el.structural.push(("title", Some((*t).into())));
        }
        self.rel(&mut el);
        if let (true, Some(key)) = (self.opts.roundtrip, &l.key) {
            el.carriers.push(("data-carve-ref", Some((*key).into())));
            match l.ref_form {
                Some(LinkRefForm::Collapsed) => {
                    el.carriers.push(("data-carve-ref-collapsed", None));
                }
                Some(LinkRefForm::Shortcut) => el.carriers.push(("data-carve-ref-shortcut", None)),
                _ => {}
            }
        }
        out.push_str("<a");
        el.write_into(self.filter(), out);
        out.push('>');
        self.in_link += 1;
        self.inlines(&*l.children, out);
        self.in_link -= 1;
        out.push_str("</a>");
        if self.in_link == 0 {
            out.push_str(&std::mem::take(&mut self.pending));
        }
    }

    fn autolink(&mut self, a: &ast::Autolink, out: &mut String) {
        let text = a.text.unwrap_or(a.href);
        if self.opts.profile == Profile::Comment && !url::allowed(a.href, &self.opts.url_policy) {
            self.losses.push("destination-denied");
            self.destination_losses.push(super::DestinationLoss {
                image: false,
                attribute: false,
                pos: a.pos,
                value: a.href.to_string(),
            });
            esc::text(text, out);
            return;
        }
        if self.in_link > 0 {
            esc::text(text, out);
            return;
        }
        let mut el = El::new(a.attrs, a.lang).with_dir(a.dir);
        el.structural
            .push(("href", Some(self.destination(a.href, a.pos, false))));
        self.rel(&mut el);
        if self.opts.roundtrip {
            el.carriers.push(("data-carve-autolink", None));
        }
        out.push_str("<a");
        el.write_into(self.filter(), out);
        out.push('>');
        esc::text(text, out);
        out.push_str("</a>");
    }

    /// C2-H-080, C2-H-084, C2-H-085.
    pub(super) fn image(
        &mut self,
        i: &'r ast::Image<'a>,
        paragraph_attrs: Option<&Attrs<'a>>,
        paragraph_lang: Option<&'a str>,
        out: &mut String,
    ) {
        if let Some(d) = &i.long_description {
            self.long_desc.push(d);
        }
        let alt = if i.decorative == Some(true) {
            ""
        } else {
            i.alt.unwrap_or("")
        };
        let Some(src) = i.src.or(i.key.is_none().then_some("")) else {
            out.push_str("![");
            esc::text(alt, out);
            out.push(']');
            push_label(out, i.ref_form.map(image_suffix), i.key.unwrap_or(""));
            out.push_str(&attr_text(i.attrs, i.lang));
            return;
        };
        let def = i.key.and_then(|k| self.links.get(k)).copied();
        let merged = merge_image_attrs(def.and_then(|d| d.attrs), paragraph_attrs, i.attrs);
        let merged = merged.as_ref().map(Merged::attrs);
        let mut el = El::new(merged.as_ref(), i.lang.or(paragraph_lang)).with_dir(i.dir);
        el.structural
            .push(("src", Some(self.destination(src, i.pos, true))));
        el.structural.push(("alt", Some(alt.into())));
        if let Some(t) = i.tooltip.as_ref().or(def.and_then(|d| d.tooltip.as_ref())) {
            el.structural.push(("title", Some((*t).into())));
        }
        if let (true, Some(key)) = (self.opts.roundtrip, &i.key) {
            el.carriers.push(("data-carve-ref", Some((*key).into())));
            match i.ref_form {
                Some(ImageRefForm::Collapsed) => {
                    el.carriers.push(("data-carve-ref-collapsed", None));
                }
                Some(ImageRefForm::Shortcut) => el.carriers.push(("data-carve-ref-shortcut", None)),
                _ => {}
            }
        }
        out.push_str("<img");
        el.write_into(self.filter(), out);
        out.push('>');
    }

    /// C2-H-086, C2-H-088.
    fn note_mark(&mut self, n: &'r Node<'a>, out: &mut String) {
        let Some(num) = self.order.number.get(&addr(n)).copied() else {
            if let Node::FootnoteRef(r) = n {
                esc::text(&format!("[^{}]", r.key), out);
                out.push_str(&attr_text(r.attrs, r.lang));
            }
            return;
        };
        let mut el = El::new(n.attrs(), lang_of(n)).with_dir(n.dir());
        el.id = self
            .mark_ids
            .get(&addr(n))
            .map(|s| Cow::Borrowed(s.as_str()));
        el.id_first = true;
        let note_id = self
            .note_ids
            .get(num - 1)
            .cloned()
            .unwrap_or_else(|| format!("fn{num}"));
        el.structural
            .push(("href", Some(format!("#{note_id}").into())));
        el.structural.push(("role", Some("doc-noteref".into())));
        if let (true, Node::FootnoteRef(r)) = (self.opts.roundtrip, n) {
            el.carriers
                .push(("data-carve-footnote-label", Some(r.key.into())));
        }
        let s = format!("<a{}><sup>{num}</sup></a>", el.write(self.filter()));
        if self.in_link > 0 {
            self.pending.push_str(&s);
        } else {
            out.push_str(&s);
        }
    }

    /// C2-H-095 to C2-H-097.
    fn xref(&mut self, n: &'r Node<'a>, x: &ast::Xref, out: &mut String) {
        let Some(href) = x.href.filter(|href| !self.hidden_heading(href)) else {
            esc::text(&format!("</#{}>", x.key), out);
            return;
        };
        let target = href.strip_prefix('#').unwrap_or(x.key);
        let cost = self.xref_text.get(target).map_or(x.key.len(), String::len);
        let generated = self.charge_derived(cost, n);
        if self.in_link > 0 {
            self.xref_label(target, x.key, generated, out);
            return;
        }
        let mut el = El::new(x.attrs, x.lang).with_dir(x.dir);
        el.structural
            .push(("href", Some(self.destination(href, x.pos, false))));
        if self.opts.roundtrip {
            el.carriers.push(("data-carve-xref", None));
        }
        out.push_str("<a");
        el.write_into(self.filter(), out);
        out.push('>');
        self.xref_label(target, x.key, generated, out);
        out.push_str("</a>");
    }

    fn xref_label(&self, target: &str, key: &str, generated: bool, out: &mut String) {
        if generated && let Some(text) = self.xref_text.get(target) {
            out.push_str(text);
        } else {
            esc::text(key, out);
        }
    }

    /// C2-H-075, C2-H-099, C2-H-175.
    fn span(&mut self, s: &'r ast::Span<'a>, out: &mut String) {
        let (abbr, time, kbd) = (
            span_value(s, "abbr", true),
            span_value(s, "time", true),
            span_value(s, "kbd", true),
        );
        let mut layers: Vec<(&'static str, Option<(&'static str, String)>)> = Vec::new();
        if let Some(v) = abbr.as_ref() {
            // Gap HTML-010: emptiness is tested before the value filter, as the oracle does.
            let title =
                span_value(s, "title", false).or_else(|| (!v.is_empty()).then(|| v.clone()));
            layers.push(("abbr", title.map(|t| ("title", t))));
        }
        if let Some(v) = time.as_ref() {
            layers.push((
                "time",
                span_value(s, "datetime", false)
                    .or_else(|| (!v.is_empty()).then(|| v.clone()))
                    .map(|value| ("datetime", value)),
            ));
        }
        layers.extend(kbd.as_ref().map(|_| ("kbd", None)));
        let mut el = El::new(s.attrs, s.lang).with_dir(s.dir);
        el.skip_exact_keys = &["abbr", "time", "kbd"];
        let Some((outer, rest)) = layers.split_last() else {
            out.push_str("<span");
            el.write_into(self.filter(), out);
            out.push('>');
            self.inlines(&*s.children, out);
            out.push_str("</span>");
            return;
        };
        if let Some((k, v)) = &outer.1 {
            el.structural.push((k, Some(v.clone().into())));
            el.skip_keys = if *k == "title" {
                &["title"]
            } else {
                &["datetime"]
            };
        }
        if abbr.is_some() && time.is_some() {
            el.skip_keys = &["title", "datetime"];
        }
        let _ = write!(out, "<{}{}>", outer.0, el.write(self.filter()));
        for (tag, a) in rest.iter().rev() {
            out.push('<');
            out.push_str(tag);
            if let Some((k, v)) = a {
                let _ = write!(out, " {k}=\"{}\"", attr_str(v));
            }
            out.push('>');
        }
        let abbr_layer = usize::from(abbr.is_some());
        self.in_abbr += abbr_layer;
        self.inlines(&*s.children, out);
        self.in_abbr -= abbr_layer;
        for (tag, _) in rest {
            out.push_str("</");
            out.push_str(tag);
            out.push('>');
        }
        let _ = write!(out, "</{}>", outer.0);
    }

    /// C2-H-072, C2-H-151, C2-H-168.
    fn math(&mut self, m: &ast::Math, out: &mut String) {
        let mut el = El::new(m.attrs, m.lang).with_dir(m.dir);
        if self.opts.mode == Mode::Static {
            el.base.push("language-math".into());
            out.push_str("<code");
            el.write_into(self.filter(), out);
            out.push('>');
            esc::text(m.value, out);
            out.push_str("</code>");
            return;
        }
        el.base.push("math".into());
        el.base
            .push(if m.display { "display" } else { "inline" }.into());
        el.generated.push(("role", Some("math".into())));
        let (open, close) = if m.display {
            ("\\[", "\\]")
        } else {
            ("\\(", "\\)")
        };
        let _ = write!(out, "<span{}>{open}", el.write(self.filter()));
        esc::text(m.value, out);
        let _ = write!(out, "{close}</span>");
        if let (true, Some(n), Some(p)) = (m.display, m.number, &m.number_prefix) {
            let _ = write!(
                out,
                " <span class=\"equation-number\">{} {n}</span>",
                esc::text_str(p)
            );
        }
    }

    /// C2-H-100, C2-H-126: an inert span, or a link when `social` gives a
    /// URL template for the kind.
    fn badge(&mut self, b: &Badge<'_>, text: &str, out: &mut String) {
        let mut el = El::new(b.attrs, b.lang).with_dir(b.dir);
        el.base.push(b.class.to_string());
        el.class_first = true;
        let template = self.opts.social.as_ref().and_then(|s| match b.class {
            "mention" => s.mention_url.as_deref(),
            _ => s.tag_url.as_deref(),
        });
        // Gap G-022: inside a link the badge stays a span, as no anchor nests.
        let tag = match template.filter(|t| {
            !t.is_empty() && self.in_link == 0 && self.profile_allows_type("link", false)
        }) {
            Some(t) => {
                let full = t.replace("{name}", &encode_social_name(b.name));
                let href = self.destination(&full, None, false).into_owned();
                el.structural.push(("href", Some(href.into())));
                self.rel(&mut el);
                "a"
            }
            None => "span",
        };
        let _ = write!(out, "<{tag}{}>", el.write(self.filter()));
        if tag == "span" {
            out.push_str("<strong>");
        }
        esc::text(text, out);
        if tag == "span" {
            out.push_str("</strong>");
        }
        let _ = write!(out, "</{tag}>");
    }

    /// C2-H-051, C2-H-053, C2-H-054.
    fn raw_inline(&mut self, r: &ast::RawInline, out: &mut String) {
        if r.format != "html" {
            self.losses.push("raw-format-dropped");
            self.raw_losses.push(super::RawLoss {
                kind: "inline",
                format: r.format.to_string(),
                pos: r.pos,
            });
            return;
        }
        let open = if self.opts.roundtrip && !self.opts.raw_html {
            "<span data-carve-raw=\"html\">"
        } else {
            ""
        };
        out.push_str(open);
        if self.opts.raw_html {
            out.push_str(r.value);
        } else {
            esc::text(r.value, out);
        }
        if self.opts.roundtrip && !self.opts.raw_html {
            out.push_str("</span>");
        }
    }

    /// C2-H-076.
    fn inline_extension(&mut self, e: &'r ast::InlineExtension<'a>, out: &mut String) {
        if e.name.contains('.') {
            self.losses.push("extension-fallback-rendered");
            let wrapped = e.attrs.is_some_and(|attrs| !attrs.is_empty());
            if wrapped {
                let el = El::new(e.attrs, e.lang).with_dir(e.dir);
                let _ = write!(out, "<span{}>", el.write(self.filter()));
            }
            self.inlines(e.fallback.as_deref().unwrap_or(&[]), out);
            if wrapped {
                out.push_str("</span>");
            }
            return;
        }
        let mut el = El::new(e.attrs, e.lang).with_dir(e.dir);
        el.base.push(format!("ext-{}", e.name));
        out.push_str("<span");
        el.write_into(self.filter(), out);
        out.push('>');
        self.inlines(&*e.children, out);
        out.push_str("</span>");
    }
}

/// C2-H-126: the unescaped set of JavaScript's encodeURIComponent.
fn encode_social_name(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for b in name.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&b) {
            out.push(char::from(b));
        } else {
            let _ = write!(out, "%{b:02X}");
        }
    }
    out
}

fn lang_of<'a>(n: &Node<'a>) -> Option<&'a str> {
    match n {
        Node::Emphasis(x) => x.lang,
        Node::Strong(x) => x.lang,
        Node::Underline(x) => x.lang,
        Node::Strike(x) => x.lang,
        Node::Highlight(x) => x.lang,
        Node::Superscript(x) => x.lang,
        Node::Subscript(x) => x.lang,
        Node::Insert(x) => x.lang,
        Node::Delete(x) => x.lang,
        Node::SmallCaps(x) => x.lang,
        Node::FootnoteRef(x) => x.lang,
        Node::InlineFootnote(x) => x.lang,
        _ => None,
    }
}

/// One inline node of a leaf in source order, for the line test of C2-H-069.
enum Item<'n, 'a> {
    Break(&'n Node<'a>),
    /// A comment, or text of spaces and tabs.
    Quiet(&'n Node<'a>, bool),
    /// Other content, or the edge of a node with children.
    Loud,
}

fn flatten<'n, 'a>(nodes: &'n [Node<'a>], depth: usize, out: &mut Vec<Item<'n, 'a>>) {
    for n in nodes {
        match n {
            Node::SoftBreak(_) | Node::HardBreak(_) => out.push(Item::Break(n)),
            Node::Comment(_) => out.push(Item::Quiet(n, true)),
            Node::Text(t) if t.value.trim_matches([' ', '\t']).is_empty() => {
                out.push(Item::Quiet(n, false));
            }
            _ => match n.children() {
                Some(kids) if depth < crate::resolve::WALK_CAP => {
                    out.push(Item::Loud);
                    flatten(kids, depth + 1, out);
                    out.push(Item::Loud);
                }
                _ => out.push(Item::Loud),
            },
        }
    }
}

/// True when `flatten` would meet a comment: only then can a line be hidden.
fn has_comment(nodes: &[Node], depth: usize) -> bool {
    nodes.iter().any(|n| match n {
        Node::Comment(_) => true,
        _ => {
            depth < crate::resolve::WALK_CAP
                && n.children().is_some_and(|k| has_comment(k, depth + 1))
        }
    })
}

/// Marks the comments of each comment-only line of a leaf, at any nesting
/// depth, with the break behind the line, or before it at the end (C2-H-069).
fn hide_comment_lines(nodes: &[Node], hidden: &mut AddrSet<usize>) {
    if !has_comment(nodes, 0) {
        return;
    }
    let mut items = Vec::new();
    flatten(nodes, 0, &mut items);
    if !items.iter().any(|item| matches!(item, Item::Break(_))) {
        return;
    }
    let mut prev_break: Option<&Node> = None;
    let mut line: Vec<(&Node, bool)> = Vec::new();
    let (mut comment, mut quiet) = (false, true);
    let mut it = items.iter().peekable();
    while let Some(item) = it.next() {
        match item {
            Item::Quiet(n, c) => {
                line.push((n, *c));
                comment |= *c;
            }
            Item::Loud => quiet = false,
            Item::Break(_) => {}
        }
        let at_end = it.peek().is_none();
        let brk = if let Item::Break(b) = item {
            Some(*b)
        } else {
            None
        };
        if brk.is_none() && !at_end {
            continue;
        }
        if comment && quiet {
            hidden.extend(line.iter().filter(|(_, c)| *c).map(|(n, _)| addr(n)));
            if let Some(b) = brk.or(prev_break) {
                hidden.insert(addr(b));
            }
        }
        prev_break = brk;
        line.clear();
        (comment, quiet) = (false, true);
    }
}

fn ref_suffix(f: LinkRefForm) -> u8 {
    match f {
        LinkRefForm::Full => 0,
        LinkRefForm::Collapsed => 1,
        LinkRefForm::Shortcut => 2,
    }
}

fn image_suffix(f: ImageRefForm) -> u8 {
    match f {
        ImageRefForm::Full => 0,
        ImageRefForm::Collapsed => 1,
        ImageRefForm::Shortcut => 2,
    }
}

/// `[key]` for the full form, `[]` for the collapsed one (C2-H-085).
fn push_label(out: &mut String, form: Option<u8>, key: &str) {
    match form {
        Some(0) => esc::text(&format!("[{key}]"), out),
        Some(1) => out.push_str("[]"),
        _ => {}
    }
}

/// The attribute block of an unresolved reference as text (C2-H-139).
fn attr_text(attrs: Option<&Attrs>, lang: Option<&str>) -> String {
    let mut parts = Vec::new();
    if let Some(a) = attrs {
        if let Some(id) = &a.id {
            parts.push(format!("#{id}"));
        }
        parts.extend(a.classes.iter().map(|c| format!(".{c}")));
    }
    if let Some(l) = lang {
        parts.push(format!(":{l}"));
    }
    if let Some(a) = attrs {
        for (k, v) in a.key_values {
            if v.is_empty() {
                parts.push((*k).to_string());
            } else {
                parts.push(format!("{k}={}", bare_or_quoted(v)));
            }
        }
    }
    if parts.is_empty() {
        return String::new();
    }
    esc::text_str(&format!("{{{}}}", parts.join(" ")))
}

fn bare_or_quoted(v: &str) -> String {
    if !v.is_empty()
        && v.chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | '.' | ':' | '/'))
    {
        v.to_string()
    } else {
        format!("\"{}\"", v.replace('\\', "\\\\").replace('"', "\\\""))
    }
}

/// The attributes of a reference merged with its definition's.
struct Merged<'a> {
    id: Option<&'a str>,
    classes: Vec<&'a str>,
    key_values: Vec<(&'a str, &'a str)>,
}

impl Merged<'_> {
    fn attrs(&self) -> Attrs<'_> {
        Attrs {
            id: self.id,
            classes: &self.classes,
            key_values: &self.key_values,
        }
    }
}

/// C2-H-141: the definition's attributes, then the reference's.
fn merge_def<'a>(def: Option<&Attrs<'a>>, own: Option<&Attrs<'a>>) -> Option<Merged<'a>> {
    match (def, own) {
        (None, None) => None,
        // Gap HTML-011: the definition's keys that the reference does not
        // replace come first, then the reference's keys, as the oracle does.
        (Some(d), o) => {
            let own = o.copied().unwrap_or_default();
            let mut key_values: Vec<(&str, &str)> = d
                .key_values
                .iter()
                .filter(|(k, _)| own.get(k).is_none())
                .copied()
                .collect();
            key_values.extend(own.key_values);
            let classes = d.classes.iter().chain(own.classes).copied().collect();
            Some(Merged {
                id: own.id.or(d.id),
                classes,
                key_values,
            })
        }
        (None, Some(o)) => Some(Merged {
            id: o.id,
            classes: o.classes.to_vec(),
            key_values: o.key_values.to_vec(),
        }),
    }
}

fn merge_image_attrs<'a>(
    definition: Option<&Attrs<'a>>,
    paragraph: Option<&Attrs<'a>>,
    image: Option<&Attrs<'a>>,
) -> Option<Merged<'a>> {
    let mut merged = Merged {
        id: None,
        classes: Vec::new(),
        key_values: Vec::new(),
    };
    let mut any = false;
    for attrs in [definition, paragraph, image].into_iter().flatten() {
        any = true;
        merged.id = attrs.id.or(merged.id);
        merged.classes.extend(attrs.classes);
        for &(key, value) in attrs.key_values {
            if let Some((_, slot)) = merged
                .key_values
                .iter_mut()
                .find(|(old, _)| old.eq_ignore_ascii_case(key))
            {
                *slot = value;
            } else {
                merged.key_values.push((key, value));
            }
        }
    }
    any.then_some(merged)
}

/// A mention or a tag as C2-H-100 writes it.
struct Badge<'n> {
    class: &'static str,
    name: &'n str,
    attrs: Option<&'n Attrs<'n>>,
    lang: Option<&'n str>,
    dir: Option<&'n str>,
}

#[cfg(test)]
mod social_tests {
    use super::encode_social_name;

    #[test]
    fn c2_h_126_encodes_utf8_and_preserves_the_allowed_ascii_set() {
        assert_eq!(
            encode_social_name("A-é /-_.!~*'()"),
            "A-%C3%A9%20%2F-_.!~*'()"
        );
    }
}
