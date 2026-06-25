//! Linkify bare URLs and email addresses (Tier-2 extension).
//!
//! Port of carve-js `autolink.ts` / carve-php's `AutolinkExtension`. Carve core
//! leaves bare URLs literal and only treats angle autolinks `<url>` as links;
//! this opt-in extension linkifies plain `https://…`, `mailto:…`, and bare
//! `a@b.com` text via the inline matcher contract.

use crate::ast::{InlineNode, Link};
use crate::extension::{CarveExtension, InlineMatch, MatcherContext};

/// Options for [`Autolink`].
#[derive(Debug, Clone)]
pub struct AutolinkOptions {
    /// URL schemes to linkify. `mailto` also enables `mailto:` links and bare
    /// email addresses. Default `["https", "http", "mailto"]`.
    pub allowed_schemes: Vec<String>,
}

impl Default for AutolinkOptions {
    fn default() -> Self {
        Self {
            allowed_schemes: vec!["https".into(), "http".into(), "mailto".into()],
        }
    }
}

/// Linkify bare URLs and email addresses.
///
/// ```
/// use carve::{Autolink, Options};
/// let ext = Autolink::new();
/// let opts = Options::new().with_extension(&ext);
/// let html = carve::to_html_with_options("Visit https://example.com today.", &opts);
/// assert_eq!(
///     html,
///     "<p>Visit <a href=\"https://example.com\">https://example.com</a> today.</p>"
/// );
/// ```
///
/// A trailing sentence punctuation mark is left outside the link, matching
/// carve-php / carve-js.
pub struct Autolink {
    url_schemes: Vec<String>,
    mailto: bool,
}

impl Autolink {
    /// Create an autolink extension with default options.
    pub fn new() -> Self {
        Self::with_options(AutolinkOptions::default())
    }

    /// Create an autolink extension with explicit options.
    pub fn with_options(opts: AutolinkOptions) -> Self {
        let mailto = opts.allowed_schemes.iter().any(|s| s == "mailto");
        let url_schemes = opts
            .allowed_schemes
            .into_iter()
            .filter(|s| s != "mailto")
            .collect();
        Self {
            url_schemes,
            mailto,
        }
    }

    /// Try to match a scheme URL at `rest`. Mirrors the js regex
    /// `^(?:scheme|…)://[^\s<>\[\](){}]*[^\s<>\[\](){}.,;:!?'"]`: consume the
    /// scheme, `://`, then a run of "url" chars, dropping a single trailing
    /// terminator so `https://x.com.` links `https://x.com`.
    fn match_url(&self, rest: &str) -> Option<usize> {
        let scheme = self
            .url_schemes
            .iter()
            .find(|s| starts_with_scheme(rest, s))?;
        let after_scheme = scheme.len() + "://".len();
        let body = &rest[after_scheme..];
        // Need at least one url char after `://`.
        let mut last_kept: Option<usize> = None;
        for (idx, ch) in body.char_indices() {
            if is_url_stop(ch) {
                break;
            }
            let consumed = idx + ch.len_utf8();
            if !is_url_trailing_stop(ch) {
                last_kept = Some(consumed);
            }
        }
        let end_body = last_kept?;
        Some(after_scheme + end_body)
    }
}

impl Default for Autolink {
    fn default() -> Self {
        Self::new()
    }
}

const EMAIL_TLD_MIN: usize = 2;

impl CarveExtension for Autolink {
    fn name(&self) -> &'static str {
        "autolink"
    }

    fn match_inline(
        &self,
        text: &str,
        pos: usize,
        _ctx: &MatcherContext<'_>,
    ) -> Option<InlineMatch> {
        let rest = text.get(pos..)?;

        if let Some(len) = self.match_url(rest) {
            let url = &rest[..len];
            return Some(InlineMatch {
                node: link_node(url, url),
                end: pos + len,
            });
        }

        if self.mailto {
            if let Some(addr) = match_mailto(rest) {
                // Display without the `mailto:` prefix, like carve-php.
                let display = &addr["mailto:".len()..];
                return Some(InlineMatch {
                    node: link_node(addr, display),
                    end: pos + addr.len(),
                });
            }
            if let Some(addr) = match_email(rest) {
                let href = format!("mailto:{addr}");
                return Some(InlineMatch {
                    node: link_node(&href, addr),
                    end: pos + addr.len(),
                });
            }
        }

        None
    }
}

fn starts_with_scheme(rest: &str, scheme: &str) -> bool {
    rest.len() >= scheme.len() + 3
        && rest.is_char_boundary(scheme.len())
        && rest[..scheme.len()].eq_ignore_ascii_case(scheme)
        && rest[scheme.len()..].starts_with("://")
}

fn is_url_stop(ch: char) -> bool {
    ch.is_whitespace() || matches!(ch, '<' | '>' | '[' | ']' | '(' | ')' | '{' | '}')
}

fn is_url_trailing_stop(ch: char) -> bool {
    matches!(ch, '.' | ',' | ';' | ':' | '!' | '?' | '\'' | '"')
}

fn link_node(href: &str, text: &str) -> InlineNode {
    InlineNode::Link(Link {
        attrs: None,
        href: href.to_string(),
        title: None,
        children: vec![InlineNode::Text(text.to_string())],
        ref_label: None,
        raw_ref: None,
        from_crossref: false,
    })
}

/// Match a `mailto:` URL: `^mailto:` followed by an email. Returns the whole
/// matched slice (including the `mailto:` prefix).
fn match_mailto(rest: &str) -> Option<&str> {
    let after = rest.strip_prefix("mailto:")?;
    let email = match_email(after)?;
    Some(&rest[.."mailto:".len() + email.len()])
}

/// Match an email address at the start of `s`, mirroring the js regex
/// `[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}`. Returns the matched slice.
fn match_email(s: &str) -> Option<&str> {
    let bytes = s.as_bytes();
    let mut i = 0;
    // local part
    let local_start = i;
    while i < bytes.len() && is_email_local(bytes[i]) {
        i += 1;
    }
    if i == local_start || i >= bytes.len() || bytes[i] != b'@' {
        return None;
    }
    i += 1; // consume '@'
            // domain: one or more domain chars, then must end with `.TLD` of >=2 letters
    let domain_start = i;
    while i < bytes.len() && is_email_domain(bytes[i]) {
        i += 1;
    }
    if i == domain_start {
        return None;
    }
    // Backtrack to find the last `.` followed by >=2 trailing letters that
    // form a valid TLD, mirroring the greedy regex with `[a-zA-Z]{2,}` anchor.
    let domain = &s[domain_start..i];
    let dot = domain.rfind('.')?;
    let tld = &domain[dot + 1..];
    if tld.len() < EMAIL_TLD_MIN || !tld.bytes().all(|b| b.is_ascii_alphabetic()) {
        // The greedy run may have eaten a trailing `.` or digits; trim back to
        // the longest prefix ending in a letter TLD.
        return trim_to_email(s, domain_start, i);
    }
    Some(&s[..i])
}

/// When the greedy domain run does not end on a valid TLD (e.g. trailing dot or
/// digits), trim characters from the end until the domain ends in `.letters`.
fn trim_to_email(s: &str, domain_start: usize, mut end: usize) -> Option<&str> {
    while end > domain_start {
        end -= 1;
        let domain = &s[domain_start..end];
        if let Some(dot) = domain.rfind('.') {
            let tld = &domain[dot + 1..];
            if tld.len() >= EMAIL_TLD_MIN && tld.bytes().all(|b| b.is_ascii_alphabetic()) {
                return Some(&s[..end]);
            }
        }
    }
    None
}

fn is_email_local(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'%' | b'+' | b'-')
}

fn is_email_domain(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-')
}
