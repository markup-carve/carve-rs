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
use crate::escape::is_url_probe_skippable;

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

/// The Tier-1 admonition kinds: the only `::: kind` fences that are callouts.
/// A fence opened with any other word is a generic container - it renders as
/// a plain `<div>` (see `render::render_admonition`'s `canonical` check) and
/// is classified as `div`, not `admonition`, for profile purposes (see
/// [`canonical_block_type`]). Both call sites read this ONE list so they
/// cannot drift (carve issue 431: they had drifted - the renderer already drew
/// this line, the profile classifier did not).
pub const ADMONITION_TIER1_KINDS: &[&str] = &[
    "note", "tip", "warning", "danger", "info", "success", "example", "quote",
];

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
    "admonition",
    "raw_block",
    "footnote",
    "frontmatter",
    "definition_list",
    "definition_term",
    "definition_description",
    "section",
    "line_block",
    "comment",
    "figure",
    "caption",
    // Both definition kinds are in the normative Block vocabulary
    // (markup-carve/carve#771, ruled by markup-carve/carve#826). Without them
    // here, the string API takes its "outside the vocabulary" branch and
    // answers allowed for a type the same profile denies on the node path.
    "abbreviation_def",
    "link_reference_definition",
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
    "autolink",
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
    // Listed in profiles.md's inline vocabulary and missing here, so a profile
    // could not name them and the resolver denied the nodes outright.
    "heading_ref",
    "citation_group",
    "caption_number",
    "substitution",
    "critic_comment",
];

/// Map a [`BlockNode`] to its canonical snake_case name.
///
/// Returns `None` for nodes that have no canonical mapping (e.g.
/// `AbbreviationDef`); such nodes are denied-by-default by the resolver,
/// matching carve-php's "unknown type -> denied" rule.
///
/// `frontmatter` has no arm here: carve-rs keeps it on `Document.frontmatter_raw`
/// rather than as a `BlockNode` variant (spec PART 12 section 4 permits an
/// internal representation that differs from the published tree, as long as
/// the mapping happens on the way out - see `ast_json::write_document`). It is
/// still in [`CANONICAL_BLOCK_TYPES`] and still deniable: `apply_profile`
/// checks it directly against the `Document` root instead of through this
/// per-node match (carve issue 422).
pub fn canonical_block_type(node: &BlockNode) -> Option<&'static str> {
    match node {
        BlockNode::LinkReferenceDefinition(_) => Some("link_reference_definition"),
        BlockNode::Heading(_) => Some("heading"),
        BlockNode::Paragraph(_) => Some("paragraph"),
        BlockNode::CodeBlock(_) => Some("code_block"),
        BlockNode::List(_) => Some("list"),
        BlockNode::BlockQuote(_) => Some("block_quote"),
        BlockNode::Table(_) => Some("table"),
        // A Tier-1 kind (`note`, `tip`, ...) is a callout: gate it as
        // `admonition`. Any other named fence (`::: sidebar`) is a generic
        // container - gate it as `div`, matching the `with_supertype` subtype
        // rule below (denying `div` still catches every admonition, Tier-1 or
        // not) and matching what the renderer already does with
        // `ADMONITION_TIER1_KINDS` (carve issue 431). This is a profile-only
        // reclassification: the published AST type stays `admonition` for
        // every kind (see `ast_json::write_block`), same as the `tag` ->
        // `mention` fold.
        BlockNode::Admonition(a) if ADMONITION_TIER1_KINDS.contains(&a.kind.as_str()) => {
            Some("admonition")
        }
        BlockNode::Admonition(_) => Some("div"),
        BlockNode::Div(_) => Some("div"),
        BlockNode::LineBlock(_) => Some("line_block"),
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
        // The `glossary` / `index` extensions likewise rewrite a `::: glossary`
        // / `::: index` admonition (a typed div) into a carrier before profile
        // filtering; gate them as `div` so a restrictive profile denies them
        // exactly as the original admonition.
        BlockNode::Extension(e) if e.name == crate::extensions::glossary::CARRIER => Some("div"),
        BlockNode::Extension(e) if e.name == crate::extensions::index_terms::LIST_CARRIER => {
            Some("div")
        }
        // A block extension is gated under the inline-extension feature, the
        // same name carve-js / carve-php use for both extension axes.
        BlockNode::Extension(_) => Some("inline_extension"),
        // Not in profiles.md's vocabulary (a definition renders nothing, so
        // denying it would express nothing), but naming it truthfully beats
        // reporting "unknown": it resolves on its axis like any unmapped type.
        BlockNode::AbbreviationDef(_) => Some("abbreviation_def"),
    }
}

/// Map an [`InlineNode`] to its canonical snake_case name.
///
/// Returns `None` for nodes that have no canonical mapping (e.g. `Symbol`,
/// `CrossRef`, `CaptionNumber`, `CriticSubstitute`, `CriticComment`); such
/// nodes are denied-by-default by the resolver.
pub fn canonical_inline_type(node: &InlineNode) -> Option<&'static str> {
    match node {
        InlineNode::Text(_) | InlineNode::EscapedText(_) | InlineNode::SmartPunctuation(_) => {
            Some("text")
        }
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
        InlineNode::Code(_) => Some("code"),
        InlineNode::Link(_) => Some("link"),
        InlineNode::AutoLink(_) => Some("autolink"),
        InlineNode::Image(_) => Some("image"),
        InlineNode::Span(_) => Some("span"),
        InlineNode::Math(_) => Some("math"),
        InlineNode::RawInline(_) => Some("raw_inline"),
        // An inline literal is a code span with the `<code>` wrapper dropped:
        // same verbatim capture, same escaping, same trailing-attribute surface.
        // So it classifies as `code` -- allowed exactly where a code span is.
        // Aliasing to `text` would be wrong: with attributes it renders a
        // `<span>` carrying class/id just as an attributed code span does.
        InlineNode::LiteralInline(_) => Some("code"),
        InlineNode::Mention(_) => Some("mention"),
        // carve-php / carve-js treat `#tag` under the mention feature.
        InlineNode::Tag(_) => Some("mention"),
        InlineNode::Extension(_) => Some("inline_extension"),
        // `citation_group` is its own entry in profiles.md's vocabulary, so a
        // profile names it directly rather than reaching it through
        // `inline_extension`.
        InlineNode::CitationGroup(_) => Some("citation_group"),
        InlineNode::Abbreviation(_) => Some("abbreviation"),
        // `^[...]` (inline) carries `inline`; `[^id]` is a reference. Both are
        // denied under the footnote family by the presets, but we distinguish
        // so a profile could allow one and not the other.
        InlineNode::Footnote(f) => Some(if f.inline.is_some() {
            "inline_footnote"
        } else {
            "footnote_ref"
        }),
        InlineNode::SoftBreak(_) => Some("soft_break"),
        InlineNode::HardBreak(_) => Some("hard_break"),
        InlineNode::CriticInsert(_) => Some("insert"),
        InlineNode::CriticDelete(_) => Some("delete"),
        // Each of these is in profiles.md's inline vocabulary. Returning None
        // meant the resolver denied them, so `full()` - a profile that denies
        // nothing - deleted a symbol, a cross-reference and a caption number
        // from the output entirely (carve#419).
        InlineNode::Symbol(_) => Some("symbol"),
        InlineNode::CrossRef(_) => Some("heading_ref"),
        InlineNode::CaptionNumber(_) => Some("caption_number"),
        InlineNode::CriticSubstitute(_) => Some("substitution"),
        InlineNode::Comment(_) => Some("comment"),
        InlineNode::CriticComment(_) => Some("critic_comment"),
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
    ///
    /// The scheme is read through [`is_url_probe_skippable`], the renderer's own
    /// probe class, so this rule and PART 9 §25's answer the same way about a
    /// scheme split by a character a URL consumer discards. That is a
    /// NARROWING: filtering only removes characters, so the deny lists can
    /// recognize more and can never recognize less, and no legitimate scheme
    /// carries one (a scheme is a letter followed by letters, digits, `+`, `-`
    /// and `.`).
    ///
    /// Known and deliberate limit: the internal/external classification below
    /// runs on the raw text, so a LEADING probe-class character - which `trim`
    /// does not reach - still reads `<DEL>//host` as neither protocol-relative
    /// nor relative. That is a prefix classification rather than a scheme read,
    /// and normalizing it cannot be done without also deciding what an
    /// allowlist makes of the normalized text, so it is not settled here.
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
            let raw_scheme = url[..colon_pos].to_lowercase();
            // What a URL consumer would read as the scheme: the probe class
            // filtered out, because a consumer may discard any of those
            // characters before it decides what the scheme is. `trim` only
            // reaches the two ends, so while this was the raw text
            // `java<DEL>script:` and `java<U+0001>script:` walked past the
            // denylist (markup-carve/carve-rs#835).
            let scheme: String = raw_scheme
                .chars()
                .filter(|c| !is_url_probe_skippable(*c))
                .collect();

            if self.denied_schemes.iter().any(|s| s == &scheme) {
                return false;
            }
            // The ALLOW lookup deliberately reads the RAW text, not the probe.
            //
            // The two lists ask opposite questions. Deny asks "could a consumer
            // read this as a scheme I refuse", so it has to see through the
            // split. Allow asks "is this exactly a scheme I permit", and a
            // scheme carrying a control character is not one - an allowlist
            // refuses what it does not recognize, which is why this form was
            // never defeated. Reading the probe here would START allowing
            // `htt<DEL>ps:` under `set_allowed_schemes(Some(vec!["https"]))`,
            // turning a fix into a widening.
            if let Some(allowed) = &self.allowed_schemes {
                if !allowed.iter().any(|s| s == &raw_scheme) {
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
///
/// It finds the authority by splitting on `://` and never looks at the scheme,
/// which is why the caller hands it the URL unchanged. carve-js' spelling
/// matches `^[a-zA-Z][a-zA-Z0-9+.-]*://` instead, so a scheme split by a
/// probe-class character makes it return `None` there and skip the domain
/// denylist and the `allow_external` check with it; that engine repairs the
/// scheme before this call and this one has nothing to repair
/// (markup-carve/carve-rs#835). Do NOT port the repair here to match: it would
/// be a step that cannot change an answer, and a check that cannot fail is the
/// thing this repo keeps finding at the bottom of its defects.
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

/// Types that are a SPECIALIZATION of a broader one.
///
/// profiles.md requires both to be nameable on their own: an autolink is not a
/// `link` (folding it in loses the authored form a round trip has to restore),
/// and an admonition is not a `div` (a profile wanting to deny callouts while
/// allowing generic containers cannot say so if the kind lives in a class
/// string). Naming them used to be a silent no-op, because both folded into the
/// broader name before the check (carve issue 362).
///
/// They stay COVERED BY the broader name, though: a profile that denies `link`
/// must keep stripping autolinks, and one that denies `div` must keep stripping
/// admonitions. Otherwise unfolding them would quietly widen every profile that
/// already relies on the broad name - the opposite of what a deny list is for.
fn with_supertype(ty: &str) -> Vec<&str> {
    match ty {
        "autolink" => vec!["autolink", "link"],
        "admonition" => vec!["admonition", "div"],
        other => vec![other],
    }
}

impl Profile {
    /// Default maximum input length (UTF-8 bytes) for the untrusted `comment`
    /// preset - a DoS backstop enforced pre-parse. Generous for a comment body;
    /// override with `set_max_length(0)` to disable or another value to retune.
    pub const COMMENT_MAX_LENGTH: usize = 100_000;
    /// Default maximum input length (UTF-8 bytes) for the untrusted `minimal`
    /// preset (chat / micro-posts). Override with `set_max_length` as needed.
    pub const MINIMAL_MAX_LENGTH: usize = 10_000;

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
            .set_max_nesting(4)
            .set_max_length(Self::COMMENT_MAX_LENGTH);
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
            .set_max_nesting(2)
            .set_max_length(Self::MINIMAL_MAX_LENGTH);
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
        self.resolve(canonical, None)
    }

    /// Resolve a type on a KNOWN axis.
    ///
    /// The axis is what makes a type outside the vocabulary resolvable at all:
    /// block-vs-inline cannot be read off a name the vocabulary does not know,
    /// and it is unambiguous at the node.
    pub fn is_type_allowed_on(&self, canonical: &str, is_block: bool) -> bool {
        self.resolve(canonical, Some(is_block))
    }

    /// profiles.md "Resolution": deny wins, then an allow list is a closed set,
    /// otherwise allowed. Those three steps are EXHAUSTIVE - an implementation
    /// must not add a fourth denying unrecognized types, which is what this did.
    /// A construct whose type the vocabulary does not list rendered as nothing
    /// under any profile, including one that denies nothing (carve#419).
    fn resolve(&self, canonical: &str, is_block: Option<bool>) -> bool {
        if canonical == "document" {
            return true;
        }
        if CANONICAL_INLINE_TYPES.contains(&canonical) {
            return self.is_inline_allowed(canonical);
        }
        if CANONICAL_BLOCK_TYPES.contains(&canonical) {
            return self.is_block_allowed(canonical);
        }
        match is_block {
            Some(true) => self.is_block_allowed(canonical),
            Some(false) => self.is_inline_allowed(canonical),
            // No axis to hand: step 2 would exclude the type on whichever axis
            // it belongs to, so an allow list on either means denied. Fails
            // CLOSED, because the caller cannot say which axis it meant.
            None => self.allowed_inline.is_none() && self.allowed_block.is_none(),
        }
    }

    fn is_inline_allowed(&self, ty: &str) -> bool {
        let names = with_supertype(ty);
        if self
            .denied_inline
            .iter()
            .any(|t| names.iter().any(|n| n == t))
        {
            return false;
        }
        if let Some(allowed) = &self.allowed_inline {
            return allowed.iter().any(|t| names.iter().any(|n| n == t));
        }
        true
    }

    fn is_block_allowed(&self, ty: &str) -> bool {
        let names = with_supertype(ty);
        if self
            .denied_block
            .iter()
            .any(|t| names.iter().any(|n| n == t))
        {
            return false;
        }
        if let Some(allowed) = &self.allowed_block {
            return allowed.iter().any(|t| names.iter().any(|n| n == t));
        }
        true
    }
}
