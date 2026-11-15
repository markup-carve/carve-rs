//! Inline elements (C2-H-069, C2-H-072 to C2-H-101, C2-H-139 to C2-H-142).

use super::Renderer;
use super::attrs::El;
use super::block::attr_str;
use super::{esc, url};
use crate::ast::{self, Attrs, ImageRefForm, LinkRefForm, Node};
use crate::options::Mode;
use crate::resolve::order::addr;
use crate::tables::{PROFILES, SMART_RUNS};
use std::collections::HashSet;
use std::fmt::Write as _;

impl<'a> Renderer<'a> {
    /// The inline content of a leaf with its comment-only lines and one break
    /// beside each left out (C2-H-069).
    pub(super) fn leaf(&mut self, nodes: &'a [Node]) -> String {
        // Gap G-020: children degraded at the nesting bound keep their breaks.
        if !self.past_bound() {
            hide_comment_lines(nodes, &mut self.hidden);
        }
        let mut out = String::new();
        self.inlines(nodes, &mut out);
        out
    }

    pub(super) fn inlines(&mut self, nodes: &'a [Node], out: &mut String) {
        for n in nodes {
            self.inline(n, out);
        }
    }

    fn inline(&mut self, n: &'a Node, out: &mut String) {
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

    fn inline_inner(&mut self, n: &'a Node, out: &mut String) {
        if self.simple_inline(n, out) {
            return;
        }
        match n {
            Node::Link(l) => self.link(n, l, out),
            Node::Autolink(a) => self.autolink(a, out),
            Node::Image(i) => self.image(i, out),
            Node::FootnoteRef(_) | Node::InlineFootnote(_) => self.note_mark(n, out),
            Node::Xref(x) => self.xref(x, out),
            Node::Span(s) => self.span(s, out),
            Node::Math(m) => self.math(m, out),
            Node::Abbreviation(a) => match (&a.expansion, self.in_abbr) {
                (Some(e), 0) => {
                    let mut el = El::new(a.attrs.as_ref(), a.lang.as_deref());
                    el.structural.push(("title", Some(e.clone())));
                    let _ = write!(out, "<abbr{}>", el.write(self.filter()));
                    esc::text(&a.key, out);
                    out.push_str("</abbr>");
                }
                _ => esc::text(&a.key, out),
            },
            // Gap HTML-018: a label is the visible text, written without the at-sign.
            Node::Mention(m) => {
                let text = m.label.clone().unwrap_or_else(|| format!("@{}", m.target));
                self.badge("mention", &text, m.attrs.as_ref(), m.lang.as_deref(), out);
            }
            Node::Tag(t) => self.badge(
                "tag",
                &format!("#{}", t.name),
                t.attrs.as_ref(),
                t.lang.as_deref(),
                out,
            ),
            Node::Symbol(s) => {
                let mut inner = String::new();
                match self.opts.symbols.get(&s.name) {
                    Some(v) => inner.push_str(v),
                    None => esc::text(&format!(":{}:", s.name), &mut inner),
                }
                self.wrap_span(s.attrs.as_ref(), s.lang.as_deref(), &inner, out);
            }
            Node::RawInline(r) => self.raw_inline(r, out),
            Node::InlineExtension(e) => self.inline_extension(e, out),
            Node::Ruby(r) => {
                let _ = write!(
                    out,
                    "<ruby{}>",
                    El::new(r.attrs.as_ref(), r.lang.as_deref()).write(self.filter())
                );
                for p in &r.pairs {
                    if let Node::RubyPair(p) = p {
                        self.inlines(&p.base, out);
                        out.push_str("<rp>(</rp><rt>");
                        self.inlines(&p.annotation, out);
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
            Node::Text(t) => esc::text(&t.value, out),
            Node::SoftBreak(_) => out.push('\n'),
            Node::HardBreak(_) => out.push_str("<br>\n"),
            Node::NonBreakingSpace(_) => out.push_str("&nbsp;"),
            Node::SmartPunctuation(s) => {
                if self.opts.smart_typography {
                    let glyph = s.glyph.as_deref().or_else(|| {
                        SMART_RUNS
                            .iter()
                            .find(|r| r.2 == s.kind.as_str())
                            .map(|r| r.3)
                    });
                    esc::text(glyph.unwrap_or(&s.source), out);
                } else {
                    esc::text(&s.source, out);
                }
            }
            Node::Comment(_) => {}
            Node::CriticComment(c) => {
                out.push_str("<span class=\"critic-comment\">");
                esc::text(&c.value, out);
                out.push_str("</span>");
            }
            Node::CitationGroup(c) => esc::text(&c.raw, out),
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
    fn wrapper(&mut self, n: &'a Node, out: &mut String) {
        let tag = match n {
            Node::Emphasis(_) => "em",
            Node::Strong(_) => "strong",
            Node::Underline(_) => "u",
            Node::Strike(_) => "s",
            Node::Highlight(_) => "mark",
            Node::Superscript(_) => "sup",
            Node::Subscript(_) => "sub",
            Node::Insert(_) => "ins",
            Node::Delete(_) => "del",
            Node::Code(c) => {
                let el = El::new(c.attrs.as_ref(), c.lang.as_deref());
                let _ = write!(out, "<code{}>", el.write(self.filter()));
                esc::text(&c.value, out);
                out.push_str("</code>");
                return;
            }
            Node::LiteralInline(l) => {
                let inner = esc::text_str(&l.value);
                if l.attrs.as_ref().is_some_and(|a| !a.is_empty()) || l.lang.is_some() {
                    self.wrap_span(l.attrs.as_ref(), l.lang.as_deref(), &inner, out);
                } else {
                    out.push_str(&inner);
                }
                return;
            }
            Node::SmallCaps(_) => "span",
            _ => {
                if let Some(c) = n.children() {
                    self.inlines(c, out);
                }
                return;
            }
        };
        let mut el = El::new(n.attrs(), lang_of(n));
        if matches!(n, Node::SmallCaps(_)) {
            el.base.push("smallcaps".into());
        }
        let _ = write!(out, "<{tag}{}>", el.write(self.filter()));
        if let Some(c) = n.children() {
            self.inlines(c, out);
        }
        let _ = write!(out, "</{tag}>");
    }

    fn wrap_span(&self, attrs: Option<&Attrs>, lang: Option<&str>, inner: &str, out: &mut String) {
        if attrs.is_some_and(|a| !a.is_empty()) || lang.is_some() {
            let _ = write!(
                out,
                "<span{}>{inner}</span>",
                El::new(attrs, lang).write(self.filter())
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
            el.structural.push(("rel", Some(words.join(" "))));
        }
    }

    /// A destination through the URL policy (C2-H-082, C2-H-160).
    fn destination(&mut self, dest: &str) -> String {
        if url::allowed(dest, &self.opts.url_policy) {
            dest.to_string()
        } else {
            self.losses.push("destination-denied");
            String::new()
        }
    }

    /// C2-H-079, C2-H-081, C2-H-083, C2-H-085, C2-H-088, C2-H-141.
    fn link(&mut self, n: &'a Node, l: &'a ast::Link, out: &mut String) {
        if self.in_link > 0 {
            self.inlines(&l.children, out);
            return;
        }
        let Some(href) = &l.href else {
            if let Some(key) = &l.key {
                out.push('[');
                self.inlines(&l.children, out);
                out.push(']');
                push_label(out, l.ref_form.map(ref_suffix), key);
                out.push_str(&attr_text(l.attrs.as_ref(), l.lang.as_deref()));
                return;
            }
            return self.anchor(n, l, "", out);
        };
        self.anchor(n, l, href, out);
    }

    fn anchor(&mut self, _n: &'a Node, l: &'a ast::Link, href: &str, out: &mut String) {
        let def = l.key.as_deref().and_then(|k| self.links.get(k)).copied();
        let merged = merge_def(def.and_then(|d| d.attrs.as_ref()), l.attrs.as_ref());
        let mut el = El::new(merged.as_ref(), l.lang.as_deref());
        el.structural.push(("href", Some(self.destination(href))));
        if let Some(t) = l.tooltip.as_ref().or(def.and_then(|d| d.tooltip.as_ref())) {
            el.structural.push(("title", Some(t.clone())));
        }
        self.rel(&mut el);
        if let (true, Some(key)) = (self.opts.roundtrip, &l.key) {
            el.carriers.push(("data-carve-ref", Some(key.clone())));
            match l.ref_form {
                Some(LinkRefForm::Collapsed) => {
                    el.carriers.push(("data-carve-ref-collapsed", None));
                }
                Some(LinkRefForm::Shortcut) => el.carriers.push(("data-carve-ref-shortcut", None)),
                _ => {}
            }
        }
        let _ = write!(out, "<a{}>", el.write(self.filter()));
        self.in_link += 1;
        self.inlines(&l.children, out);
        self.in_link -= 1;
        out.push_str("</a>");
        if self.in_link == 0 {
            out.push_str(&std::mem::take(&mut self.pending));
        }
    }

    fn autolink(&mut self, a: &ast::Autolink, out: &mut String) {
        let text = a.text.as_deref().unwrap_or(&a.href);
        if self.in_link > 0 {
            esc::text(text, out);
            return;
        }
        let mut el = El::new(a.attrs.as_ref(), a.lang.as_deref());
        el.structural
            .push(("href", Some(self.destination(&a.href))));
        self.rel(&mut el);
        if self.opts.roundtrip {
            el.carriers.push(("data-carve-autolink", None));
        }
        let _ = write!(out, "<a{}>", el.write(self.filter()));
        esc::text(text, out);
        out.push_str("</a>");
    }

    /// C2-H-080, C2-H-084, C2-H-085.
    fn image(&mut self, i: &'a ast::Image, out: &mut String) {
        if let Some(d) = &i.long_description {
            self.long_desc.push(d);
        }
        let alt = if i.decorative == Some(true) {
            ""
        } else {
            i.alt.as_deref().unwrap_or("")
        };
        let Some(src) = i.src.as_deref().or(i.key.is_none().then_some("")) else {
            out.push_str("![");
            esc::text(alt, out);
            out.push(']');
            push_label(
                out,
                i.ref_form.map(image_suffix),
                i.key.as_deref().unwrap_or(""),
            );
            out.push_str(&attr_text(i.attrs.as_ref(), i.lang.as_deref()));
            return;
        };
        let def = i.key.as_deref().and_then(|k| self.links.get(k)).copied();
        let merged = merge_def(def.and_then(|d| d.attrs.as_ref()), i.attrs.as_ref());
        let mut el = El::new(merged.as_ref(), i.lang.as_deref());
        el.structural.push(("src", Some(self.destination(src))));
        el.structural.push(("alt", Some(alt.to_string())));
        if let Some(t) = i.tooltip.as_ref().or(def.and_then(|d| d.tooltip.as_ref())) {
            el.structural.push(("title", Some(t.clone())));
        }
        if let (true, Some(key)) = (self.opts.roundtrip, &i.key) {
            el.carriers.push(("data-carve-ref", Some(key.clone())));
            match i.ref_form {
                Some(ImageRefForm::Collapsed) => {
                    el.carriers.push(("data-carve-ref-collapsed", None));
                }
                Some(ImageRefForm::Shortcut) => el.carriers.push(("data-carve-ref-shortcut", None)),
                _ => {}
            }
        }
        let _ = write!(out, "<img{}>", el.write(self.filter()));
    }

    /// C2-H-086, C2-H-088.
    fn note_mark(&mut self, n: &'a Node, out: &mut String) {
        let Some(num) = self.order.number.get(&addr(n)).copied() else {
            if let Node::FootnoteRef(r) = n {
                esc::text(&format!("[^{}]", r.key), out);
            }
            return;
        };
        let mut el = El::new(n.attrs(), lang_of(n));
        el.id = self.mark_ids.get(&addr(n)).cloned();
        el.id_first = true;
        el.structural.push(("href", Some(format!("#fn{num}"))));
        el.structural.push(("role", Some("doc-noteref".into())));
        if let (true, Node::FootnoteRef(r)) = (self.opts.roundtrip, n) {
            el.carriers
                .push(("data-carve-footnote-label", Some(r.key.clone())));
        }
        let s = format!("<a{}><sup>{num}</sup></a>", el.write(self.filter()));
        if self.in_link > 0 {
            self.pending.push_str(&s);
        } else {
            out.push_str(&s);
        }
    }

    /// C2-H-095 to C2-H-097.
    fn xref(&mut self, x: &ast::Xref, out: &mut String) {
        let Some(href) = &x.href else {
            esc::text(&format!("</#{}>", x.key), out);
            return;
        };
        let text = self
            .xref_text
            .get(&x.key)
            .cloned()
            .unwrap_or_else(|| x.key.clone());
        if self.in_link > 0 {
            esc::text(&text, out);
            return;
        }
        let mut el = El::new(x.attrs.as_ref(), x.lang.as_deref());
        el.structural.push(("href", Some(self.destination(href))));
        if self.opts.roundtrip {
            el.carriers.push(("data-carve-xref", None));
        }
        let _ = write!(out, "<a{}>", el.write(self.filter()));
        esc::text(&text, out);
        out.push_str("</a>");
    }

    /// C2-H-075, C2-H-099, C2-H-175.
    fn span(&mut self, s: &'a ast::Span, out: &mut String) {
        let get = |k: &str| {
            s.attrs
                .as_ref()
                .and_then(|a| a.key_values.iter().find(|(x, _)| x.eq_ignore_ascii_case(k)))
                .map(|(x, v)| url::filter_value(x, v))
        };
        let (abbr, time, kbd) = (get("abbr"), get("time"), get("kbd"));
        let mut layers: Vec<(&'static str, Option<(&'static str, String)>)> = Vec::new();
        if let Some(v) = abbr.as_ref() {
            // Gap HTML-010: emptiness is tested before the value filter, as the oracle does.
            let raw = s.attrs.as_ref().and_then(|a| a.get("abbr")).unwrap_or("");
            let title = get("title").or_else(|| (!raw.is_empty()).then(|| v.clone()));
            layers.push(("abbr", title.map(|t| ("title", t))));
        }
        if let Some(v) = time.as_ref() {
            layers.push((
                "time",
                Some(("datetime", get("datetime").unwrap_or_else(|| v.clone()))),
            ));
        }
        if kbd.is_some() {
            layers.push(("kbd", None));
        }
        let mut el = El::new(s.attrs.as_ref(), s.lang.as_deref());
        el.skip_keys = &["abbr", "time", "kbd"];
        let Some((outer, rest)) = layers.split_last() else {
            let _ = write!(out, "<span{}>", el.write(self.filter()));
            self.inlines(&s.children, out);
            out.push_str("</span>");
            return;
        };
        if let Some((k, v)) = &outer.1 {
            el.structural.push((k, Some(v.clone())));
            el.skip_keys = if *k == "title" {
                &["abbr", "time", "kbd", "title"]
            } else {
                &["abbr", "time", "kbd", "datetime"]
            };
        }
        if abbr.is_some() && time.is_some() {
            el.skip_keys = &["abbr", "time", "kbd", "title", "datetime"];
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
        self.inlines(&s.children, out);
        self.in_abbr -= abbr_layer;
        for (tag, _) in rest {
            let _ = write!(out, "</{tag}>");
        }
        let _ = write!(out, "</{}>", outer.0);
    }

    /// C2-H-072, C2-H-151, C2-H-168.
    fn math(&mut self, m: &ast::Math, out: &mut String) {
        let mut el = El::new(m.attrs.as_ref(), m.lang.as_deref());
        if self.opts.mode == Mode::Static {
            el.base.push("language-math".into());
            let _ = write!(out, "<code{}>", el.write(self.filter()));
            esc::text(&m.value, out);
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
        esc::text(&m.value, out);
        let _ = write!(out, "{close}</span>");
        if let (true, Some(n), Some(p)) = (m.display, m.number, &m.number_prefix) {
            let _ = write!(
                out,
                " <span class=\"equation-number\">{} {n}</span>",
                esc::text_str(p)
            );
        }
    }

    /// C2-H-100: an inert span without `social`.
    fn badge(
        &self,
        class: &str,
        text: &str,
        attrs: Option<&Attrs>,
        lang: Option<&str>,
        out: &mut String,
    ) {
        let mut el = El::new(attrs, lang);
        el.base.push(class.to_string());
        el.class_first = true;
        let _ = write!(out, "<span{}><strong>", el.write(self.filter()));
        esc::text(text, out);
        out.push_str("</strong></span>");
    }

    /// C2-H-051, C2-H-053, C2-H-054.
    fn raw_inline(&mut self, r: &ast::RawInline, out: &mut String) {
        if r.format != "html" {
            self.losses.push("raw-format-dropped");
            return;
        }
        let open = if self.opts.roundtrip {
            "<span data-carve-raw=\"html\">"
        } else {
            ""
        };
        out.push_str(open);
        if self.opts.raw_html {
            out.push_str(&r.value);
        } else {
            esc::text(&r.value, out);
        }
        if self.opts.roundtrip {
            out.push_str("</span>");
        }
    }

    /// C2-H-076.
    fn inline_extension(&mut self, e: &'a ast::InlineExtension, out: &mut String) {
        if e.name.contains('.') {
            self.losses.push("extension-fallback-rendered");
            self.inlines(e.fallback.as_deref().unwrap_or(&[]), out);
            return;
        }
        let mut el = El::new(e.attrs.as_ref(), e.lang.as_deref());
        el.base.push(format!("ext-{}", e.name));
        let _ = write!(out, "<span{}>", el.write(self.filter()));
        self.inlines(&e.children, out);
        out.push_str("</span>");
    }
}

fn lang_of(n: &Node) -> Option<&str> {
    match n {
        Node::Emphasis(x) => x.lang.as_deref(),
        Node::Strong(x) => x.lang.as_deref(),
        Node::Underline(x) => x.lang.as_deref(),
        Node::Strike(x) => x.lang.as_deref(),
        Node::Highlight(x) => x.lang.as_deref(),
        Node::Superscript(x) => x.lang.as_deref(),
        Node::Subscript(x) => x.lang.as_deref(),
        Node::Insert(x) => x.lang.as_deref(),
        Node::Delete(x) => x.lang.as_deref(),
        Node::SmallCaps(x) => x.lang.as_deref(),
        Node::FootnoteRef(x) => x.lang.as_deref(),
        Node::InlineFootnote(x) => x.lang.as_deref(),
        _ => None,
    }
}

/// One inline node of a leaf in source order, for the line test of C2-H-069.
enum Item<'n> {
    Break(&'n Node),
    /// A comment, or text of spaces and tabs.
    Quiet(&'n Node, bool),
    /// Other content, or the edge of a node with children.
    Loud,
}

fn flatten<'n>(nodes: &'n [Node], depth: usize, out: &mut Vec<Item<'n>>) {
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

/// Marks the comments of each comment-only line of a leaf, at any nesting
/// depth, with the break behind the line, or before it at the end (C2-H-069).
fn hide_comment_lines(nodes: &[Node], hidden: &mut HashSet<usize>) {
    let mut items = Vec::new();
    flatten(nodes, 0, &mut items);
    let mut prev_break: Option<&Node> = None;
    let mut line: Vec<&Node> = Vec::new();
    let (mut comment, mut quiet) = (false, true);
    let mut it = items.iter().peekable();
    while let Some(item) = it.next() {
        match item {
            Item::Quiet(n, c) => {
                line.push(n);
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
            hidden.extend(line.iter().map(|n| addr(n)));
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
        for (k, v) in &a.key_values {
            if v.is_empty() {
                parts.push(k.clone());
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

/// C2-H-141: the definition's attributes, then the reference's.
fn merge_def(def: Option<&Attrs>, own: Option<&Attrs>) -> Option<Attrs> {
    match (def, own) {
        (None, None) => None,
        // Gap HTML-011: the definition's keys that the reference does not
        // replace come first, then the reference's keys, as the oracle does.
        (Some(d), o) => {
            let own = o.cloned().unwrap_or_default();
            let mut key_values: Vec<(String, String)> = d
                .key_values
                .iter()
                .filter(|(k, _)| own.get(k).is_none())
                .cloned()
                .collect();
            key_values.extend(own.key_values);
            let classes = d.classes.iter().chain(&own.classes).cloned().collect();
            Some(Attrs {
                id: own.id.or_else(|| d.id.clone()),
                classes,
                key_values,
            })
        }
        (None, Some(o)) => Some(o.clone()),
    }
}
