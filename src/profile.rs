//! Profile-based feature restriction (core; port of carve-php's `Profile` +
//! `LinkPolicy` + `ProfileFilter`, and parity with carve-js' `profile.ts` /
//! `profile-filter.ts`).
//!
//! A [`Profile`] controls which markup *features* survive into the output,
//! independent of XSS sanitization. It runs as an AST transform on the parsed
//! [`Document`] before rendering, so it holds identically for the HTML,
//! Markdown, plain-text and ANSI renderers.
//!
//! The allow/deny lists, presets and resolution semantics match
//! carve-php / carve-js. They are expressed in a canonical snake_case
//! node-type vocabulary (see [`CANONICAL_BLOCK_TYPES`] /
//! [`CANONICAL_INLINE_TYPES`]); carve-rs' AST is enum variants, so
//! [`canonical_block_type`] / [`canonical_inline_type`] map each node to its
//! canonical name before the allow/deny check.

use crate::ast::*;

/// Action taken on a disallowed node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DisallowedAction {
    /// Remove the node and its subtree.
    Strip,
    /// Replace the node with its rendered text content (default).
    #[default]
    ToText,
    /// Collect a violation and surface it as an error.
    Error,
}

/// Canonical block node-type vocabulary (snake_case).
pub const CANONICAL_BLOCK_TYPES: &[&str] = &[
    "paragraph",
    "heading",
    "code_block",
    "block_quote",
    "list",
    "list_item",
    "table",
    "table_row",
    "table_cell",
    "thematic_break",
    "div",
    "raw_block",
    "footnote",
    "definition_list",
    "definition_term",
    "definition_description",
    "section",
    "line_block",
    "comment",
    "figure",
    "caption",
];

/// Canonical inline node-type vocabulary (snake_case).
pub const CANONICAL_INLINE_TYPES: &[&str] = &[
    "text",
    "emphasis",
    "strong",
    "underline",
    "strike",
    "inline_extension",
    "mention",
    "code",
    "link",
    "image",
    "soft_break",
    "hard_break",
    "raw_inline",
    "escaped_text",
    "footnote_ref",
    "inline_footnote",
    "span",
    "superscript",
    "subscript",
    "highlight",
    "insert",
    "delete",
    "symbol",
    "math",
    "abbreviation",
];

/// Map a [`BlockNode`] to its canonical snake_case name.
///
/// Returns `None` for nodes that have no canonical mapping (e.g.
/// `AbbreviationDef`); such nodes are denied-by-default by the resolver,
/// matching carve-php's "unknown type -> denied" rule.
pub fn canonical_block_type(node: &BlockNode) -> Option<&'static str> {
    match node {
        BlockNode::Heading(_) => Some("heading"),
        BlockNode::Paragraph(_) => Some("paragraph"),
        BlockNode::CodeBlock(_) => Some("code_block"),
        BlockNode::List(_) => Some("list"),
        BlockNode::BlockQuote(_) => Some("block_quote"),
        BlockNode::Table(_) => Some("table"),
        // An admonition is a typed div; carve-php has no separate admonition
        // node (it is a Div). Treat it under the `div` feature.
        BlockNode::Admonition(_) => Some("div"),
        BlockNode::Div(_) => Some("div"),
        BlockNode::DefinitionList(_) => Some("definition_list"),
        BlockNode::Figure(_) => Some("figure"),
        BlockNode::RawBlock(_) => Some("raw_block"),
        BlockNode::Comment(_) => Some("comment"),
        BlockNode::BlockImage(_) => Some("image"),
        BlockNode::ThematicBreak(_) => Some("thematic_break"),
        // The `details` extension rewrites a `details` admonition (a typed
        // div) into an extension carrier before profile filtering; gate it as
        // a `div` so a restrictive profile denies it exactly as the original
        // admonition (carve-js gates the un-rewritten admonition as a div).
        BlockNode::Extension(e) if e.name == crate::extensions::details::CARRIER => Some("div"),
        // The `list-table` extension likewise rewrites a `list-table`
        // admonition (a typed div) into an extension carrier before profile
        // filtering; gate it as a `div` so a restrictive profile denies it
        // exactly as the original admonition.
        BlockNode::Extension(e) if e.name == crate::extensions::list_table::CARRIER => Some("div"),
        // A block extension is gated under the inline-extension feature, the
        // same name carve-js / carve-php use for both extension axes.
        BlockNode::Extension(_) => Some("inline_extension"),
        BlockNode::AbbreviationDef(_) => None,
    }
}

/// Map an [`InlineNode`] to its canonical snake_case name.
///
/// Returns `None` for nodes that have no canonical mapping (e.g. `Emoji`,
/// `CrossRef`, `CaptionNumber`, `CriticSubstitute`, `CriticComment`); such
/// nodes are denied-by-default by the resolver.
pub fn canonical_inline_type(node: &InlineNode) -> Option<&'static str> {
    match node {
        InlineNode::Text(_) => Some("text"),
        InlineNode::Emphasis(e) => Some(match e.kind {
            EmphasisKind::Italic => "emphasis",
            // BoldItalic is nested strong+emphasis; gate it under `strong`
            // (the outer feature), matching carve-js.
            EmphasisKind::Strong | EmphasisKind::BoldItalic => "strong",
            EmphasisKind::Underline => "underline",
            EmphasisKind::Strike => "strike",
            EmphasisKind::Super => "superscript",
            EmphasisKind::Sub => "subscript",
            EmphasisKind::Highlight => "highlight",
        }),
        InlineNode::Code(_, _) => Some("code"),
        InlineNode::Link(_) => Some("link"),
        InlineNode::AutoLink(_) => Some("link"),
        InlineNode::Image(_) => Some("image"),
        InlineNode::Span(_) => Some("span"),
        InlineNode::Math(_) => Some("math"),
        InlineNode::RawInline(_) => Some("raw_inline"),
        InlineNode::Mention(_) => Some("mention"),
        // carve-php / carve-js treat `#tag` under the mention feature.
        InlineNode::Tag(_) => Some("mention"),
        InlineNode::Extension(_) => Some("inline_extension"),
        // Citations are delivered as a Tier-2 extension, so they gate under the
        // inline-extension feature (allowed only where extensions are allowed).
        InlineNode::CitationGroup(_) => Some("inline_extension"),
        InlineNode::Abbreviation(_) => Some("abbreviation"),
        // `^[...]` (inline) carries `inline`; `[^id]` is a reference. Both are
        // denied under the footnote family by the presets, but we distinguish
        // so a profile could allow one and not the other.
        InlineNode::Footnote(f) => Some(if f.inline.is_some() {
            "inline_footnote"
        } else {
            "footnote_ref"
        }),
        InlineNode::SoftBreak => Some("soft_break"),
        InlineNode::HardBreak => Some("hard_break"),
        InlineNode::CriticInsert(_) => Some("insert"),
        InlineNode::CriticDelete(_) => Some("delete"),
        // No canonical mapping -> denied by default.
        InlineNode::Emoji(_)
        | InlineNode::CrossRef(_)
        | InlineNode::CaptionNumber(_)
        | InlineNode::CriticSubstitute(_)
        | InlineNode::CriticComment(_) => None,
    }
}

/// Link URL policy for Profile-based filtering. Controls which URLs are
/// allowed in links and images. Port of carve-php's `LinkPolicy`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkPolicy {
    allowed_schemes: Option<Vec<String>>,
    denied_schemes: Vec<String>,
    allowed_domains: Option<Vec<String>>,
    denied_domains: Vec<String>,
    allow_external: bool,
    allow_internal: bool,
    rel_attributes: Vec<String>,
}

impl Default for LinkPolicy {
    fn default() -> Self {
        Self {
            allowed_schemes: None,
            denied_schemes: vec![
                "javascript".to_string(),
                "vbscript".to_string(),
                "data".to_string(),
                "file".to_string(),
            ],
            allowed_domains: None,
            denied_domains: Vec::new(),
            allow_external: true,
            allow_internal: true,
            rel_attributes: Vec::new(),
        }
    }
}

impl LinkPolicy {
    /// Allow all URLs except dangerous schemes.
    pub fn unrestricted() -> Self {
        Self::default()
    }

    /// Allow only internal links (relative URLs, fragments).
    pub fn internal_only() -> Self {
        Self::default().set_allow_external(false)
    }

    /// Allow only links to specific domains.
    pub fn allowlist(domains: Vec<String>) -> Self {
        Self::default().set_allowed_domains(Some(domains))
    }

    pub fn allowed_schemes(&self) -> Option<&[String]> {
        self.allowed_schemes.as_deref()
    }

    pub fn set_allowed_schemes(mut self, schemes: Option<Vec<String>>) -> Self {
        self.allowed_schemes = schemes.map(|s| s.iter().map(|x| x.to_lowercase()).collect());
        self
    }

    pub fn denied_schemes(&self) -> &[String] {
        &self.denied_schemes
    }

    pub fn set_denied_schemes(mut self, schemes: Vec<String>) -> Self {
        self.denied_schemes = schemes.iter().map(|x| x.to_lowercase()).collect();
        self
    }

    pub fn allowed_domains(&self) -> Option<&[String]> {
        self.allowed_domains.as_deref()
    }

    pub fn set_allowed_domains(mut self, domains: Option<Vec<String>>) -> Self {
        self.allowed_domains = domains;
        self
    }

    pub fn denied_domains(&self) -> &[String] {
        &self.denied_domains
    }

    pub fn set_denied_domains(mut self, domains: Vec<String>) -> Self {
        self.denied_domains = domains;
        self
    }

    pub fn allow_external(&self) -> bool {
        self.allow_external
    }

    pub fn set_allow_external(mut self, allow: bool) -> Self {
        self.allow_external = allow;
        self
    }

    pub fn allow_internal(&self) -> bool {
        self.allow_internal
    }

    pub fn set_allow_internal(mut self, allow: bool) -> Self {
        self.allow_internal = allow;
        self
    }

    pub fn rel_attributes(&self) -> &[String] {
        &self.rel_attributes
    }

    pub fn set_rel_attributes(mut self, attrs: Vec<String>) -> Self {
        self.rel_attributes = attrs;
        self
    }

    /// Add a rel attribute applied to all surviving links.
    pub fn add_rel_attribute(mut self, attr: impl Into<String>) -> Self {
        let attr = attr.into();
        if !self.rel_attributes.contains(&attr) {
            self.rel_attributes.push(attr);
        }
        self
    }

    /// Check whether a URL is permitted by this policy.
    ///
    /// `base_host` is the current document's host (for external detection).
    pub fn is_url_allowed(&self, url: &str, base_host: Option<&str>) -> bool {
        let url = url.trim();
        if url.is_empty() {
            return true;
        }

        // Fragment-only URLs are always internal.
        if url.starts_with('#') {
            return self.allow_internal;
        }

        // Protocol-relative URLs are absolute external URLs, not internal paths.
        if url.starts_with("//") {
            return self.is_protocol_relative_url_allowed(url, base_host);
        }

        // Relative paths are internal.
        if url.starts_with('/') || url.starts_with("./") || url.starts_with("../") {
            return self.allow_internal;
        }

        if let Some(colon_pos) = url.find(':') {
            let scheme = url[..colon_pos].to_lowercase();

            if self.denied_schemes.iter().any(|s| s == &scheme) {
                return false;
            }
            if let Some(allowed) = &self.allowed_schemes {
                if !allowed.iter().any(|s| s == &scheme) {
                    return false;
                }
            }

            // mailto: and tel: are considered internal for simplicity.
            if scheme == "mailto" || scheme == "tel" {
                return true;
            }

            if scheme == "http" || scheme == "https" {
                if let Some(host) = parse_host(url) {
                    if self.is_domain_denied(&host) {
                        return false;
                    }
                    if self.allowed_domains.is_some() && !self.is_domain_allowed(&host) {
                        return false;
                    }
                    if !self.allow_external {
                        match base_host {
                            Some(bh) if !is_same_host(&host, bh) => return false,
                            None => return false,
                            _ => {}
                        }
                    }
                }
            }
        }

        true
    }

    fn is_protocol_relative_url_allowed(&self, url: &str, base_host: Option<&str>) -> bool {
        if let Some(schemes) = &self.allowed_schemes {
            let has_http = schemes.iter().any(|s| s == "http" || s == "https");
            if !has_http {
                return false;
            }
        }

        let Some(host) = parse_host(&format!("https:{url}")) else {
            return false;
        };
        if self.is_domain_denied(&host) {
            return false;
        }
        if self.allowed_domains.is_some() && !self.is_domain_allowed(&host) {
            return false;
        }
        if !self.allow_external {
            match base_host {
                Some(bh) if !is_same_host(&host, bh) => return false,
                None => return false,
                _ => {}
            }
        }
        true
    }

    fn is_domain_denied(&self, host: &str) -> bool {
        let host = host.to_lowercase();
        self.denied_domains.iter().any(|d| {
            let d = d.to_lowercase();
            host == d || host.ends_with(&format!(".{d}"))
        })
    }

    fn is_domain_allowed(&self, host: &str) -> bool {
        let Some(allowed) = &self.allowed_domains else {
            return true;
        };
        let host = host.to_lowercase();
        allowed.iter().any(|d| {
            let d = d.to_lowercase();
            host == d || host.ends_with(&format!(".{d}"))
        })
    }
}

fn is_same_host(a: &str, b: &str) -> bool {
    a.to_lowercase() == b.to_lowercase()
}

/// Extract the host of an http(s) URL the way PHP's `parse_url` does for the
/// cases [`LinkPolicy`] needs (host only). Returns `None` when no host can be
/// determined.
fn parse_host(url: &str) -> Option<String> {
    // scheme://authority/...; authority ends at /, ?, or #.
    let rest = url.split_once("://")?.1;
    let authority_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let mut authority = &rest[..authority_end];
    // Strip userinfo.
    if let Some(at) = authority.rfind('@') {
        authority = &authority[at + 1..];
    }
    // Strip port. IPv6 literals are in [..]; keep brackets out of scope (rare).
    if !authority.contains(']') {
        if let Some(colon) = authority.rfind(':') {
            authority = &authority[..colon];
        }
    }
    if authority.is_empty() {
        None
    } else {
        Some(authority.to_string())
    }
}

/// A recorded profile violation (surfaced when action is [`DisallowedAction::Error`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileViolation {
    /// Canonical node type that was disallowed.
    pub node_type: String,
    /// Machine reason: `element_not_allowed` | `max_nesting_exceeded` |
    /// `link_not_allowed` | `image_not_allowed`.
    pub reason: String,
    /// Human-readable feature reason from the profile, if any.
    pub reason_description: Option<String>,
}

impl ProfileViolation {
    /// Format the violation into a human-readable message (matches carve-php / js).
    pub fn message(&self) -> String {
        let mut msg = format!("'{}' is not allowed: {}", self.node_type, self.reason);
        if let Some(desc) = &self.reason_description {
            msg.push_str(&format!(" ({desc})"));
        }
        msg
    }
}

/// Error returned by [`apply_profile`](crate::profile_filter::apply_profile)
/// when the profile's action is [`DisallowedAction::Error`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileViolationError {
    pub violations: Vec<ProfileViolation>,
}

impl std::fmt::Display for ProfileViolationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let parts: Vec<String> = self.violations.iter().map(|v| v.message()).collect();
        write!(f, "Profile violations: {}", parts.join("; "))
    }
}

impl std::error::Error for ProfileViolationError {}

/// Profile: feature restriction for a rendering context. Port of carve-php's
/// `Profile`, including the four presets (full / article / comment / minimal).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Profile {
    name: String,
    description: String,
    feature_reasons: std::collections::BTreeMap<String, String>,
    allowed_inline: Option<Vec<String>>,
    allowed_block: Option<Vec<String>>,
    denied_inline: Vec<String>,
    denied_block: Vec<String>,
    link_policy: Option<LinkPolicy>,
    max_nesting: usize,
    max_length: usize,
    disallowed_action: DisallowedAction,
}

impl Default for Profile {
    fn default() -> Self {
        Self {
            name: "custom".to_string(),
            description: String::new(),
            feature_reasons: std::collections::BTreeMap::new(),
            allowed_inline: None,
            allowed_block: None,
            denied_inline: Vec::new(),
            denied_block: Vec::new(),
            link_policy: None,
            max_nesting: 0,
            max_length: 0,
            disallowed_action: DisallowedAction::ToText,
        }
    }
}

fn to_strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| s.to_string()).collect()
}

impl Profile {
    /// All features enabled. Use only for trusted content.
    pub fn full() -> Self {
        Self {
            name: "full".to_string(),
            description: "All features enabled. Use only for trusted content.".to_string(),
            ..Self::default()
        }
    }

    /// Blog posts and articles: all formatting, no raw HTML.
    pub fn article() -> Self {
        let mut p = Self {
            name: "article".to_string(),
            description: "Blog posts and articles. All formatting, no raw HTML.".to_string(),
            ..Self::default()
        };
        p = p.deny_block(&["raw_block"]).deny_inline(&["raw_inline"]);
        p.feature_reasons.insert(
            "raw_block".to_string(),
            "Raw HTML blocks are disabled to prevent XSS attacks. Use djot markup instead."
                .to_string(),
        );
        p.feature_reasons.insert(
            "raw_inline".to_string(),
            "Raw HTML is disabled to prevent XSS attacks. Use djot markup instead.".to_string(),
        );
        p
    }

    /// User comments: basic formatting only, nofollow links.
    pub fn comment() -> Self {
        let mut p = Self {
            name: "comment".to_string(),
            description: "User comments. Basic formatting only, nofollow links.".to_string(),
            ..Self::default()
        };
        p = p
            .allow_inline(Some(&[
                "text",
                "emphasis",
                "strong",
                "underline",
                "strike",
                "inline_extension",
                "mention",
                "code",
                "link",
                "soft_break",
                "hard_break",
                "delete",
                "insert",
                "highlight",
                "superscript",
                "subscript",
            ]))
            .allow_block(Some(&[
                "paragraph",
                "list",
                "list_item",
                "block_quote",
                "code_block",
            ]))
            .set_link_policy(Some(
                LinkPolicy::unrestricted()
                    .add_rel_attribute("nofollow")
                    .add_rel_attribute("ugc"),
            ))
            .set_max_nesting(4);
        for (k, v) in [
            (
                "heading",
                "Headings are disabled in comments to prevent disrupting page structure.",
            ),
            (
                "image",
                "Images are disabled to prevent spam, inappropriate content, and bandwidth abuse.",
            ),
            (
                "table",
                "Tables are disabled as they are too complex for comment formatting.",
            ),
            (
                "footnote",
                "Footnotes are disabled as they are unnecessary for comments.",
            ),
            (
                "footnote_ref",
                "Footnotes are disabled as they are unnecessary for comments.",
            ),
            (
                "inline_footnote",
                "Footnotes are disabled as they are unnecessary for comments.",
            ),
            ("raw_block", "Raw HTML is disabled for security reasons."),
            ("raw_inline", "Raw HTML is disabled for security reasons."),
            ("div", "Custom containers are disabled in comments."),
            ("section", "Sections are disabled in comments."),
            (
                "definition_list",
                "Definition lists are disabled in comments.",
            ),
            (
                "definition_term",
                "Definition lists are disabled in comments.",
            ),
            (
                "definition_description",
                "Definition lists are disabled in comments.",
            ),
            (
                "thematic_break",
                "Horizontal rules are disabled in comments.",
            ),
            ("line_block", "Line blocks are disabled in comments."),
            ("span", "Custom spans are disabled in comments."),
            ("symbol", "Symbol markup is disabled in comments."),
            ("math", "Math markup is disabled in comments."),
            ("abbreviation", "Abbreviations are disabled in comments."),
        ] {
            p.feature_reasons.insert(k.to_string(), v.to_string());
        }
        p
    }

    /// Chat / micro-posts: non-destructive inline formatting, paragraphs and lists.
    pub fn minimal() -> Self {
        let mut p = Self {
            name: "minimal".to_string(),
            description:
                "Chat/micro-posts. Non-destructive inline formatting, paragraphs and lists."
                    .to_string(),
            ..Self::default()
        };
        p = p
            .allow_inline(Some(&[
                "text",
                "emphasis",
                "strong",
                "underline",
                "strike",
                "inline_extension",
                "mention",
                "code",
                "delete",
                "insert",
                "superscript",
                "subscript",
                "soft_break",
                "hard_break",
            ]))
            .allow_block(Some(&["paragraph", "list", "list_item"]))
            .set_max_nesting(2);
        for (k, v) in [
            ("link", "Links are disabled in this minimal context."),
            (
                "highlight",
                "Highlighting is disabled in this minimal context.",
            ),
            ("image", "Images are disabled in this minimal context."),
            ("raw_inline", "Raw HTML is disabled for security reasons."),
            (
                "footnote_ref",
                "Footnotes are disabled in this minimal context.",
            ),
            (
                "inline_footnote",
                "Footnotes are disabled in this minimal context.",
            ),
            ("span", "Custom spans are disabled in this minimal context."),
            ("symbol", "Symbols are disabled in this minimal context."),
            ("math", "Math is disabled in this minimal context."),
            (
                "abbreviation",
                "Abbreviations are disabled in this minimal context.",
            ),
            (
                "default",
                "Only basic text formatting and lists are allowed in this context.",
            ),
        ] {
            p.feature_reasons.insert(k.to_string(), v.to_string());
        }
        p
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn description(&self) -> &str {
        &self.description
    }

    /// Reason a node type is disallowed, or `None` if it is allowed / no reason.
    pub fn reason_disallowed(&self, canonical: &str) -> Option<String> {
        if self.is_type_allowed(canonical) {
            return None;
        }
        self.feature_reasons
            .get(canonical)
            .or_else(|| self.feature_reasons.get("default"))
            .cloned()
    }

    pub fn feature_reasons(&self) -> &std::collections::BTreeMap<String, String> {
        &self.feature_reasons
    }

    pub fn set_feature_reason(
        mut self,
        canonical: impl Into<String>,
        reason: impl Into<String>,
    ) -> Self {
        self.feature_reasons.insert(canonical.into(), reason.into());
        self
    }

    /// Set allowed inline types (`None` = all allowed).
    pub fn allow_inline(mut self, types: Option<&[&str]>) -> Self {
        self.allowed_inline = types.map(to_strings);
        self
    }

    /// Set allowed block types (`None` = all allowed).
    pub fn allow_block(mut self, types: Option<&[&str]>) -> Self {
        self.allowed_block = types.map(to_strings);
        self
    }

    pub fn deny_inline(mut self, types: &[&str]) -> Self {
        self.denied_inline.extend(to_strings(types));
        self
    }

    pub fn deny_block(mut self, types: &[&str]) -> Self {
        self.denied_block.extend(to_strings(types));
        self
    }

    pub fn allowed_inline(&self) -> Option<&[String]> {
        self.allowed_inline.as_deref()
    }

    pub fn allowed_block(&self) -> Option<&[String]> {
        self.allowed_block.as_deref()
    }

    pub fn denied_inline(&self) -> &[String] {
        &self.denied_inline
    }

    pub fn denied_block(&self) -> &[String] {
        &self.denied_block
    }

    pub fn link_policy(&self) -> Option<&LinkPolicy> {
        self.link_policy.as_ref()
    }

    pub fn set_link_policy(mut self, policy: Option<LinkPolicy>) -> Self {
        self.link_policy = policy;
        self
    }

    pub fn max_nesting(&self) -> usize {
        self.max_nesting
    }

    /// Set maximum block-container nesting depth (0 = unlimited).
    pub fn set_max_nesting(mut self, max: usize) -> Self {
        self.max_nesting = max;
        self
    }

    pub fn max_length(&self) -> usize {
        self.max_length
    }

    /// Set maximum input length in bytes (0 = unlimited).
    pub fn set_max_length(mut self, max: usize) -> Self {
        self.max_length = max;
        self
    }

    pub fn disallowed_action(&self) -> DisallowedAction {
        self.disallowed_action
    }

    /// Set action for disallowed elements.
    pub fn on_disallowed(mut self, action: DisallowedAction) -> Self {
        self.disallowed_action = action;
        self
    }

    /// Whether a canonical type string is allowed by this profile.
    pub fn is_type_allowed(&self, canonical: &str) -> bool {
        if CANONICAL_INLINE_TYPES.contains(&canonical) {
            return self.is_inline_allowed(canonical);
        }
        if CANONICAL_BLOCK_TYPES.contains(&canonical) {
            return self.is_block_allowed(canonical);
        }
        if canonical == "document" {
            return true;
        }
        // Unknown types are denied by default.
        false
    }

    fn is_inline_allowed(&self, ty: &str) -> bool {
        if self.denied_inline.iter().any(|t| t == ty) {
            return false;
        }
        if let Some(allowed) = &self.allowed_inline {
            return allowed.iter().any(|t| t == ty);
        }
        true
    }

    fn is_block_allowed(&self, ty: &str) -> bool {
        if self.denied_block.iter().any(|t| t == ty) {
            return false;
        }
        if let Some(allowed) = &self.allowed_block {
            return allowed.iter().any(|t| t == ty);
        }
        true
    }
}
