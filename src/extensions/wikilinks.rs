//! Parse `[[wikilinks]]` into navigational links, like Obsidian / MediaWiki.
//!
//! Port of carve-js `wikilinks.ts` / carve-php's `WikilinksExtension`. Forms:
//! `[[Page]]`, `[[page|Display]]`, `[[page#anchor]]`, `[[folder/page]]`. Uses
//! the inline matcher contract; core leaves `[[…]]` literal, so this adds the
//! syntax without hijacking any core construct.

use crate::ast::{AttrSlot, Attrs, InlineNode, Link};
use crate::extension::{CarveExtension, InlineMatch, MatcherContext};

/// Options for [`Wikilinks`].
#[derive(Debug, Clone)]
pub struct WikilinksOptions {
    /// CSS class(es) added to the anchor. Default `"wikilink"`.
    pub css_class: String,
    /// Open links in a new tab (`target="_blank" rel="noopener"`). Default false.
    pub new_window: bool,
}

impl Default for WikilinksOptions {
    fn default() -> Self {
        Self {
            css_class: "wikilink".into(),
            new_window: false,
        }
    }
}

/// Type of a page-to-URL generator closure. Receives the page (anchor
/// stripped) and returns the href; the anchor, if any, is appended afterwards.
pub type UrlGenerator = Box<dyn Fn(&str) -> String + Send + Sync>;

/// Parse `[[wikilinks]]` into navigational links.
///
/// ```
/// use carve::{Wikilinks, Options};
/// let ext = Wikilinks::new();
/// let opts = Options::new().with_extension(&ext);
/// let html = carve::to_html_with_options("See [[Tigers]].", &opts);
/// assert_eq!(
///     html,
///     "<p>See <a href=\"tigers\" class=\"wikilink\" data-wikilink=\"Tigers\">Tigers</a>.</p>"
/// );
/// ```
pub struct Wikilinks {
    css_class: String,
    new_window: bool,
    url_generator: UrlGenerator,
}

impl Wikilinks {
    /// Create a wikilinks extension with default options and the default
    /// slugifying URL generator.
    pub fn new() -> Self {
        Self::with_options(WikilinksOptions::default())
    }

    /// Create a wikilinks extension with explicit options and the default
    /// slugifying URL generator.
    pub fn with_options(opts: WikilinksOptions) -> Self {
        Self {
            css_class: opts.css_class,
            new_window: opts.new_window,
            url_generator: Box::new(default_slug),
        }
    }

    /// Override the page-to-URL generator (the default lowercases and slugifies).
    pub fn with_url_generator(mut self, generator: UrlGenerator) -> Self {
        self.url_generator = generator;
        self
    }
}

impl Default for Wikilinks {
    fn default() -> Self {
        Self::new()
    }
}

impl CarveExtension for Wikilinks {
    fn name(&self) -> &'static str {
        "wikilinks"
    }

    fn match_inline(
        &self,
        text: &str,
        pos: usize,
        _ctx: &MatcherContext<'_>,
    ) -> Option<InlineMatch> {
        let rest = text.get(pos..)?;
        let inner = rest.strip_prefix("[[")?;
        let close_rel = inner.find("]]")?;
        let body = &inner[..close_rel];

        // The page part forbids `|` and `]` (matches carve-php); the optional
        // display part is everything after the first `|`.
        let (raw_page, display): (&str, Option<&str>) = match body.find('|') {
            Some(bar) => (&body[..bar], Some(body[bar + 1..].trim())),
            None => (body, None),
        };
        if raw_page.contains(']') {
            return None;
        }

        let page_trimmed = raw_page.trim();
        let (page, anchor) = match page_trimmed.find('#') {
            Some(hash) => (
                &page_trimmed[..hash],
                format!("#{}", &page_trimmed[hash + 1..]),
            ),
            None => (page_trimmed, String::new()),
        };

        // A wikilink needs a real target: empty or whitespace-only `[[ ]]`
        // stays literal (an anchor-only `[[#sec]]` is still a link).
        if page.is_empty() && anchor.is_empty() {
            return None;
        }

        let href = format!("{}{}", (self.url_generator)(page), anchor);
        let display_text = match display {
            Some(d) => d.to_string(),
            None => {
                if page.is_empty() {
                    anchor.clone()
                } else {
                    page.to_string()
                }
            }
        };

        let attrs = self.build_attrs(page);

        let end = pos + "[[".len() + close_rel + "]]".len();
        Some(InlineMatch {
            node: InlineNode::Link(Link {
                attrs: Some(attrs),
                href,
                title: None,
                children: vec![InlineNode::text(display_text)],
                ref_label: None,
                raw_ref: None,
                from_crossref: false,
                pos: None,
            }),
            end,
        })
    }
}

impl Wikilinks {
    fn build_attrs(&self, page: &str) -> Attrs {
        let classes: Vec<String> = self
            .css_class
            .split(' ')
            .filter(|c| !c.is_empty())
            .map(|c| c.to_string())
            .collect();

        let mut attrs = Attrs::default();
        // Match the carve-js attribute emission order: class, data-wikilink,
        // then (for new_window) target, rel. carve-rs renders key-values in
        // `order` sequence, so set it explicitly.
        if !classes.is_empty() {
            attrs.classes = classes;
            attrs.order.push(AttrSlot::Class);
        }
        attrs
            .key_values
            .insert("data-wikilink".into(), page.to_string());
        attrs.order.push(AttrSlot::Key("data-wikilink".into()));
        if self.new_window {
            attrs.key_values.insert("target".into(), "_blank".into());
            attrs.order.push(AttrSlot::Key("target".into()));
            attrs.key_values.insert("rel".into(), "noopener".into());
            attrs.order.push(AttrSlot::Key("rel".into()));
        }
        attrs
    }
}

/// Default URL generator: a URL-safe slug, matching carve-php's
/// `WikilinksExtension`. Lowercase, spaces to hyphens, unsafe chars dropped,
/// runs of hyphens collapsed.
fn default_slug(page: &str) -> String {
    let lowered = page.trim().to_lowercase();
    let mut out = String::with_capacity(lowered.len());
    let mut prev_dash = false;
    for ch in lowered.chars() {
        let mapped = if ch.is_whitespace() { '-' } else { ch };
        if mapped == '-' {
            if !prev_dash {
                out.push('-');
            }
            prev_dash = true;
            continue;
        }
        // Keep `a-z0-9_/`, drop everything else (after whitespace->'-').
        if mapped.is_ascii_lowercase() || mapped.is_ascii_digit() || matches!(mapped, '_' | '/') {
            out.push(mapped);
            prev_dash = false;
        }
        // Other chars (including non-ascii) are dropped, leaving prev_dash as-is.
    }
    out
}
