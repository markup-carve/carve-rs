//! The options of `options.json` that affect parse, resolution and HTML
//! (C2-H-137). Defaults come from the generated tables.

use crate::tables;
use std::collections::BTreeMap;

/// `headingIds` (C2-H-131).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HeadingIds {
    Off,
    #[default]
    KeepCase,
    Lower,
    Ascii,
    AsciiStrict,
    AsciiLower,
    AsciiStrictLower,
}

impl HeadingIds {
    /// Parses an option value.
    #[must_use]
    pub fn from_name(s: &str) -> Option<Self> {
        match s {
            "off" => Some(Self::Off),
            "keep-case" => Some(Self::KeepCase),
            "lower" => Some(Self::Lower),
            "ascii" => Some(Self::Ascii),
            "ascii-strict" => Some(Self::AsciiStrict),
            "ascii-lower" => Some(Self::AsciiLower),
            "ascii-strict-lower" => Some(Self::AsciiStrictLower),
            _ => None,
        }
    }
}

/// `urlPolicy` (C2-H-128): a deny list or an allow list of schemes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UrlPolicy {
    Deny(Vec<String>),
    Allow(Vec<String>),
}

impl Default for UrlPolicy {
    fn default() -> Self {
        Self::Deny(
            tables::DEFAULT_DENIED_SCHEMES
                .iter()
                .map(|s| (*s).to_string())
                .collect(),
        )
    }
}

/// `profile` (C2-H-129).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Profile {
    #[default]
    Full,
    Article,
    Comment,
    Minimal,
}

impl Profile {
    /// Parses an option value.
    #[must_use]
    pub fn from_name(s: &str) -> Option<Self> {
        match s {
            "full" => Some(Self::Full),
            "article" => Some(Self::Article),
            "comment" => Some(Self::Comment),
            "minimal" => Some(Self::Minimal),
            _ => None,
        }
    }

    /// The name as `url-policy.json` spells it.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::Article => "article",
            Self::Comment => "comment",
            Self::Minimal => "minimal",
        }
    }
}

/// `softBreak` (C2-H-203): how a `soft_break` is written.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SoftBreak {
    #[default]
    Newline,
    Space,
    Br,
}

impl SoftBreak {
    /// Parses an option value.
    #[must_use]
    pub fn from_name(s: &str) -> Option<Self> {
        match s {
            "newline" => Some(Self::Newline),
            "space" => Some(Self::Space),
            "br" => Some(Self::Br),
            _ => None,
        }
    }
}

/// `mode` (C2-H-122).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    #[default]
    Interactive,
    Static,
}

/// The render options (and `headingIds`, which the resolution reads).
#[derive(Debug, Clone)]
#[allow(clippy::struct_excessive_bools)] // the option list is booleans by spec (C2-H-137)
pub struct Options {
    pub heading_ids: HeadingIds,
    pub mode: Mode,
    pub symbols: BTreeMap<String, String>,
    pub labels: BTreeMap<String, String>,
    pub source_lines: bool,
    pub sections: bool,
    pub smart_typography: bool,
    pub roundtrip: bool,
    pub raw_html: bool,
    pub url_policy: UrlPolicy,
    pub profile: Profile,
    pub strict_losses: bool,
    /// C2-H-203.
    pub soft_break: SoftBreak,
    /// C2-H-204: render-loss codes left out of the record and of `strict_losses`.
    pub allow_losses: Vec<String>,
    /// C2-H-205: the render-loss entries the record keeps.
    pub max_losses: usize,
    /// C2-H-208: added to every heading level of the render view, 0 to 5.
    pub heading_shift: u8,
    pub social: Option<Social>,
    /// C2-H-126: the `resolveMention`, `resolveTag` and `context` fields of
    /// `social`, kept beside the templates of [`Social`].
    pub social_resolvers: Option<SocialResolvers>,
    /// C2-N-003: the resolver and limits of include expansion; `None` expands nothing.
    pub includes: Option<crate::include::Includes>,
    /// C2-H-206, C2-H-207: the host's renderer of ordinary code blocks.
    pub code_block_renderer: Option<CodeBlockRenderer>,
}

/// `codeBlockRenderer` (C2-H-206, C2-H-207): a host function from a code
/// block node, as its JSON value, and the block's default HTML to the HTML
/// written in its place, or `None` for the default. The HTML is trusted: no
/// filter or profile touches it.
///
/// ```
/// use carve::options::{CodeBlockRenderer, Options};
///
/// let opts = Options {
///     code_block_renderer: Some(CodeBlockRenderer::new(|node, default_html| {
///         (node["syntax"] == "ts").then(|| format!("<div class=\"hl\">{default_html}</div>"))
///     })),
///     ..Options::default()
/// };
/// let html = carve::api::render("```ts\nx\n```\n", &opts)?;
/// assert_eq!(html, "<div class=\"hl\"><pre><code class=\"language-ts\">x\n</code></pre></div>");
/// # Ok::<(), carve::api::Error>(())
/// ```
#[derive(Clone)]
pub struct CodeBlockRenderer(pub std::sync::Arc<CodeBlockFn>);

/// The host function of a [`CodeBlockRenderer`].
pub type CodeBlockFn = dyn Fn(&serde_json::Value, &str) -> Option<String> + Send + Sync;

impl CodeBlockRenderer {
    /// The renderer of a host function.
    pub fn new(
        render: impl Fn(&serde_json::Value, &str) -> Option<String> + Send + Sync + 'static,
    ) -> Self {
        Self(std::sync::Arc::new(render))
    }
}

impl std::fmt::Debug for CodeBlockRenderer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("CodeBlockRenderer(..)")
    }
}

/// URL templates for mentions and tags (C2-H-126). Gap G-022: the
/// specification names no template syntax; as in the oracle, every
/// `{name}` in a template is replaced by the mention's target or the tag's
/// name.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Social {
    pub mention_url: Option<String>,
    pub tag_url: Option<String>,
}

/// C2-H-126: the resolvers of `social`. A resolver is final: where it is
/// given, the template of its pair is not read, and a name it returns `None`
/// or an empty URL for, or panics on, renders as the inert span.
///
/// ```
/// use carve::options::{Options, SocialKind, SocialResolver, SocialResolvers};
///
/// let opts = Options {
///     social_resolvers: Some(SocialResolvers {
///         resolve_mention: Some(SocialResolver::new(|request| {
///             assert_eq!(request.kind, SocialKind::Mention);
///             (request.name == "alice").then(|| "/u/alice".to_string())
///         })),
///         ..SocialResolvers::default()
///     }),
///     ..Options::default()
/// };
/// let html = carve::api::render("@alice @bob\n", &opts)?;
/// assert_eq!(
///     html,
///     "<p><a class=\"mention\" href=\"/u/alice\">@alice</a> <span class=\"mention\"><strong>@bob</strong></span></p>"
/// );
/// # Ok::<(), carve::api::Error>(())
/// ```
#[derive(Debug, Clone, Default)]
pub struct SocialResolvers {
    pub resolve_mention: Option<SocialResolver>,
    pub resolve_tag: Option<SocialResolver>,
    /// A host value passed to the resolvers unchanged and never read.
    pub context: Option<serde_json::Value>,
}

/// The `kind` of a [`SocialRequest`]: which resolver is called.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SocialKind {
    Mention,
    Tag,
}

impl SocialKind {
    /// The string of C2-H-126, `mention` or `tag`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Mention => "mention",
            Self::Tag => "tag",
        }
    }
}

/// What a social resolver is called with (C2-H-126).
#[derive(Debug, Clone, Copy)]
pub struct SocialRequest<'a> {
    pub kind: SocialKind,
    /// The name as written, without its sigil.
    pub name: &'a str,
    /// The `context` of [`SocialResolvers`], absent where it has none.
    pub context: Option<&'a serde_json::Value>,
}

/// A host function from a [`SocialRequest`] to a URL, or `None`.
#[derive(Clone)]
pub struct SocialResolver(pub std::sync::Arc<SocialFn>);

/// The host function of a [`SocialResolver`].
pub type SocialFn = dyn Fn(&SocialRequest<'_>) -> Option<String> + Send + Sync;

impl SocialResolver {
    /// The resolver of a host function.
    pub fn new(
        resolve: impl Fn(&SocialRequest<'_>) -> Option<String> + Send + Sync + 'static,
    ) -> Self {
        Self(std::sync::Arc::new(resolve))
    }
}

impl std::fmt::Debug for SocialResolver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SocialResolver(..)")
    }
}

impl Default for Options {
    fn default() -> Self {
        Self {
            heading_ids: HeadingIds::default(),
            mode: Mode::default(),
            symbols: BTreeMap::new(),
            labels: tables::DEFAULT_LABELS
                .iter()
                .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
                .collect(),
            source_lines: tables::DEFAULT_SOURCE_LINES,
            sections: tables::DEFAULT_SECTIONS,
            smart_typography: tables::DEFAULT_SMART_TYPOGRAPHY,
            roundtrip: tables::DEFAULT_ROUNDTRIP,
            raw_html: tables::DEFAULT_RAW_HTML,
            url_policy: UrlPolicy::default(),
            profile: Profile::default(),
            strict_losses: tables::DEFAULT_STRICT_LOSSES,
            soft_break: SoftBreak::default(),
            allow_losses: Vec::new(),
            max_losses: DEFAULT_MAX_LOSSES,
            heading_shift: 0,
            social: None,
            social_resolvers: None,
            includes: None,
            code_block_renderer: None,
        }
    }
}

/// C2-H-205: the default of `maxLosses`.
pub const DEFAULT_MAX_LOSSES: usize = 100;

impl Options {
    /// C2-H-204: whether `code` is an allowed loss.
    #[must_use]
    pub fn allows(&self, code: &str) -> bool {
        self.allow_losses.iter().any(|c| c == code)
    }

    /// A label string (C2-H-125); a key left out keeps its English default.
    #[must_use]
    pub fn label(&self, key: &str) -> &str {
        self.labels.get(key).map_or("", String::as_str)
    }
}
