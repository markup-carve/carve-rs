//! The Markdown export: `CommonMark` with the GFM tables, task lists,
//! strikethrough and footnotes. What Markdown cannot spell falls back to
//! inline HTML (escaped where it is author content) or flattens, and the
//! flattening is reported.

// The handlers of a dispatch table share one signature, so some return `Ok` only.
#![allow(clippy::unnecessary_wraps)]

use super::common::{
    Exporter, children, each_node, flag, footnote_definitions, int, is_skipped_break,
    is_unresolved, key_value, list, plain_inline_text, reference_label, slot, strip_bidi, sv, ty,
};
use crate::api::Error;
use regex::Regex;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;
use std::sync::LazyLock;

type R = Result<String, Error>;

struct State<'x, 'v> {
    x: &'x mut Exporter<'v>,
    /// Heading slugs by the id the document gave each heading: GFM has no id
    /// syntax, so a link uses the GFM slug.
    slugs: HashMap<String, String>,
    footnotes: HashSet<&'v str>,
    referenced: HashSet<&'v str>,
    abbr_depth: usize,
    /// Above 0 while the children of an inline node are written.
    inline_level: usize,
    /// The delimiter runs open around the inline being written.
    open: Vec<&'static str>,
    /// Inside a table cell or a heading, which hold one line: a hard break is
    /// `<br>` and a soft break a space.
    single_line: usize,
    /// Above 0 while a title is written: bold inside it would close the run early.
    unwrap_strong: usize,
    /// The host whose caption a figure-group panel writes after it.
    uncaptioned: Option<*const Value>,
}

impl State<'_, '_> {
    fn caption<'n>(&self, node: &'n Value) -> Option<&'n Value> {
        if self.uncaptioned == Some(std::ptr::from_ref(node)) {
            return None;
        }
        slot(node, "caption")
    }
}

/// Drops what the target cannot carry: CR, DEL and the C1 range. The other C0 controls are content.
fn clean(text: &str) -> String {
    text.chars()
        .filter(|&c| c != '\r' && !('\u{7f}'..='\u{9f}').contains(&c))
        .collect()
}

fn escape_html(text: &str) -> String {
    clean(text)
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn escape_attribute(text: &str) -> String {
    escape_html(text).replace('"', "&quot;")
}

/// A letter or a digit that one UTF-16 unit holds, as the reference engine reads its neighbors.
fn is_word(c: Option<char>) -> bool {
    c.is_some_and(|c| c.len_utf16() == 1 && crate::chars::is_word(c))
}

/// A compiled pattern; the patterns are fixed, so `None` never occurs.
type Re = LazyLock<Option<Regex>>;

fn hit(re: &Re, text: &str) -> bool {
    re.as_ref().is_some_and(|r| r.is_match(text))
}

static REFERENCE_OPENER: Re = LazyLock::new(|| {
    Regex::new(r"^&(?:#[0-9]{1,7};|#[xX][0-9a-fA-F]{1,6};|[A-Za-z][A-Za-z0-9]*;)").ok()
});

/// Backslash-escapes the characters that would read as Markdown inside a
/// line. An underscore between two letters or digits cannot open or close
/// emphasis, so it stays as written; a `<` is escaped only where it could
/// open HTML. An escaped `]` already keeps a bracket from forming a link, so
/// `[` is escaped only inside a link label, where an unbalanced one would
/// take the label's closing bracket.
fn escape_text(text: &str, label: bool) -> String {
    let value = clean(text);
    let chars: Vec<(usize, char)> = value.char_indices().collect();
    let mut out = String::with_capacity(value.len());
    for (at, &(byte, ch)) in chars.iter().enumerate() {
        let before = at.checked_sub(1).and_then(|b| chars.get(b)).map(|c| c.1);
        let after = chars.get(at + 1).map(|c| c.1);
        match ch {
            '_' => {
                if !(is_word(before) && is_word(after)) {
                    out.push('\\');
                }
            }
            '\\' | '`' | '*' | '~' | ']' => out.push('\\'),
            '[' if label => out.push('\\'),
            '<' if after
                .is_some_and(|c| c.is_ascii_alphabetic() || matches!(c, '/' | '!' | '?')) =>
            {
                out.push('\\');
            }
            '&' => {
                // The reference engine looks at the next 40 UTF-16 units.
                let window: String = value[byte..].chars().take(40).collect();
                if hit(&REFERENCE_OPENER, &window) {
                    out.push('\\');
                }
            }
            _ => {}
        }
        out.push(ch);
    }
    out
}

fn escape_title(title: &str) -> String {
    let mut out = String::new();
    for c in clean(title).chars() {
        if c == '\\' || c == '"' {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

static AMP_REFERENCE: Re = LazyLock::new(|| {
    Regex::new(r"^&(?:#[0-9]{1,8};|#[xX][0-9a-fA-F]{1,8};|[a-zA-Z][a-zA-Z0-9]{0,31};)").ok()
});

/// A link or image destination: spaces, angle brackets and pipes
/// percent-encoded, parentheses escaped, and every ampersand that opens a
/// character reference escaped, since a reader decodes those before it reads
/// the scheme.
fn destination_text(url: &str) -> String {
    let value = clean(url);
    let pipes = value.contains('|');
    let mut out = String::with_capacity(value.len());
    for (byte, ch) in value.char_indices() {
        match ch {
            '\\' => out.push_str(if pipes { "%5C" } else { "\\" }),
            '|' => out.push_str("%7C"),
            ' ' => out.push_str("%20"),
            '\n' => out.push_str("%0A"),
            '<' => out.push_str("%3C"),
            '>' => out.push_str("%3E"),
            '(' | ')' => {
                out.push('\\');
                out.push(ch);
            }
            '&' if hit(&AMP_REFERENCE, &value[byte..]) => out.push_str("&amp;"),
            _ => out.push(ch),
        }
    }
    out
}

fn attr_list<'v>(attrs: Option<&'v Value>, key: &str) -> Vec<&'v str> {
    attrs
        .and_then(|a| a.get(key))
        .and_then(Value::as_array)
        .map(|items| items.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default()
}

/// The HTML attributes of `attrs`, for an element this target writes as inline HTML.
fn html_attributes(attrs: Option<&Value>, base: &[&str]) -> String {
    static NAME: Re = LazyLock::new(|| Regex::new(r"^[A-Za-z_:][A-Za-z0-9_.:-]*$").ok());
    let mut out = String::new();
    if let Some(id) = attrs.and_then(|a| sv(a, "id")) {
        let _ = write!(out, " id=\"{}\"", escape_attribute(id));
    }
    let mut classes: Vec<&str> = base.to_vec();
    classes.extend(
        attr_list(attrs, "classes")
            .into_iter()
            .filter(|c| !base.contains(c)),
    );
    if !classes.is_empty() {
        let _ = write!(out, " class=\"{}\"", escape_attribute(&classes.join(" ")));
    }
    for pair in attrs
        .and_then(|a| a.get("keyValues"))
        .and_then(Value::as_array)
        .map_or(&[][..], Vec::as_slice)
    {
        let key = pair.get(0).and_then(Value::as_str).unwrap_or("");
        let value = pair.get(1).and_then(Value::as_str).unwrap_or("");
        let on = key.len() >= 2 && key[..2].eq_ignore_ascii_case("on");
        if hit(&NAME, key) && !on {
            let _ = write!(out, " {key}=\"{}\"", escape_attribute(value));
        }
    }
    out
}

fn has_attributes(node: &Value) -> bool {
    node.get("attrs").is_some_and(|a| {
        a.get("id").is_some()
            || a.get("classes")
                .and_then(Value::as_array)
                .is_some_and(|c| !c.is_empty())
            || a.get("keyValues")
                .and_then(Value::as_array)
                .is_some_and(|c| !c.is_empty())
    })
}

/// A delimiter run followed by whitespace does not open emphasis
/// (`CommonMark` 6.2), so the spaces at the edges of the content move outside
/// the delimiters. Content of spaces only has no delimiter form and takes the
/// HTML element.
fn delimited(inner: &str, delimiter: &str, tag: &str) -> String {
    let core_start = inner.len() - inner.trim_start().len();
    let core = inner.trim();
    let lead = &inner[..core_start];
    let trail = &inner[core_start + core.len()..];
    if core.is_empty() {
        return if inner.is_empty() {
            String::new()
        } else {
            format!("<{tag}>{inner}</{tag}>")
        };
    }
    // A final hard break after a closing delimiter would end the paragraph and read as a backslash.
    if core.ends_with('\\') && trail.starts_with('\n') {
        return format!("{lead}<{tag}>{core}</{tag}>{trail}");
    }
    format!("{lead}{delimiter}{core}{delimiter}{trail}")
}

/// The list of inlines to write: in a title, each strong without attributes
/// gives way to its children.
fn flattened<'n>(m: &State<'_, '_>, nodes: &'n [Value]) -> Vec<&'n Value> {
    fn push<'n>(nodes: &'n [Value], out: &mut Vec<&'n Value>) {
        for node in nodes {
            if ty(node) == "strong" && !has_attributes(node) {
                push(children(node), out);
            } else {
                out.push(node);
            }
        }
    }
    if m.unwrap_strong == 0 {
        return nodes.iter().collect();
    }
    let mut out = Vec::with_capacity(nodes.len());
    push(nodes, &mut out);
    out
}

fn inlines(m: &mut State<'_, '_>, nodes: &[Value]) -> R {
    m.x.enter()?;
    let inside = m.inline_level > 0;
    let list = flattened(m, nodes);
    let mut out = String::new();
    m.inline_level += 1;
    for (at, node) in list.iter().enumerate() {
        if matches!(ty(node), "soft_break" | "hard_break") && is_skipped_break(&list, at, inside) {
            continue;
        }
        let written = match inline(m, node) {
            Ok(w) => w,
            Err(e) => {
                m.inline_level -= 1;
                return Err(e);
            }
        };
        let before = out.chars().last();
        let safe = flanking_safe(node, written, before, || first_character(m, &list, at + 1));
        out.push_str(&safe);
    }
    m.inline_level -= 1;
    m.x.leave();
    Ok(out)
}

/// Inlines of a slot other than `children`, which a title's unwrapping does not reach.
fn slot_inlines(m: &mut State<'_, '_>, nodes: &[Value]) -> R {
    let unwrap = std::mem::take(&mut m.unwrap_strong);
    let out = inlines(m, nodes);
    m.unwrap_strong = unwrap;
    out
}

/// The first character the inlines from `from` on would write.
fn first_character(m: &State<'_, '_>, list: &[&Value], from: usize) -> Option<char> {
    for node in list.iter().skip(from) {
        match ty(node) {
            "comment" => {}
            "text" if sv(node, "value").unwrap_or("").is_empty() => {}
            _ => return leading_character(m, node),
        }
    }
    None
}

fn leading_character(m: &State<'_, '_>, node: &Value) -> Option<char> {
    Some(match ty(node) {
        "text" => return sv(node, "value").and_then(|v| v.chars().next()),
        "soft_break" | "hard_break" => '\n',
        "code" => '`',
        "smart_punctuation" => return m.x.smart_text(node).chars().next(),
        "link" | "image" | "autolink" => '[',
        // Every other inline starts with markup or a letter of its own; a letter stands for both.
        _ => 'a',
    })
}

fn single_line(m: &mut State<'_, '_>, f: impl FnOnce(&mut State<'_, '_>) -> R) -> R {
    m.single_line += 1;
    let out = f(m);
    m.single_line -= 1;
    out
}

fn element(m: &mut State<'_, '_>, tag: &str, node: &Value, base: &[&str]) -> R {
    Ok(format!(
        "<{tag}{}>{}</{tag}>",
        html_attributes(node.get("attrs"), base),
        inlines(m, children(node))?
    ))
}

/// A delimiter run inside a run of the same delimiter would close the outer
/// one, so it takes the HTML element.
fn delimited_node(m: &mut State<'_, '_>, node: &Value, delimiter: &'static str, tag: &str) -> R {
    if has_attributes(node) || m.open.contains(&delimiter) {
        return element(m, tag, node, &[]);
    }
    m.open.push(delimiter);
    let inner = inlines(m, children(node));
    m.open.pop();
    Ok(delimited(&inner?, delimiter, tag))
}

fn is_punctuation(c: char) -> bool {
    static P: Re = LazyLock::new(|| Regex::new(r"^[\p{P}\p{S}]$").ok());
    hit(&P, c.encode_utf8(&mut [0; 4]))
}

/// `CommonMark` 6.2: whether a run between `before` and `after` (none at a line edge) can open or close.
fn flanks(before: Option<char>, after: Option<char>, opening: bool) -> bool {
    let (inside, outside) = if opening {
        (after, before)
    } else {
        (before, after)
    };
    let Some(inside) = inside.filter(|c| !c.is_whitespace()) else {
        return false;
    };
    !is_punctuation(inside) || outside.is_none_or(|c| c.is_whitespace() || is_punctuation(c))
}

/// A delimiter run written next to a letter on one side and punctuation
/// inside does not flank (`CommonMark` 6.2), so such an emphasis is written
/// with its HTML element instead.
fn flanking_safe(
    node: &Value,
    written: String,
    before: Option<char>,
    next: impl FnOnce() -> Option<char>,
) -> String {
    let (delimiter, tag) = match ty(node) {
        "emphasis" => ("*", "em"),
        "strong" => ("**", "strong"),
        "strike" => ("~~", "del"),
        _ => return written,
    };
    if !written.starts_with(delimiter)
        || !written.ends_with(delimiter)
        || written.len() <= delimiter.len() * 2
    {
        return written;
    }
    let core = &written[delimiter.len()..written.len() - delimiter.len()];
    let after = next();
    if flanks(before, core.chars().next(), true) && flanks(core.chars().last(), after, false) {
        return written;
    }
    format!("<{tag}>{core}</{tag}>")
}

/// A code span whose fence is longer than any backtick run inside; a tab or
/// a line break needs the HTML element.
fn code_span(value: &str) -> String {
    let content = clean(value);
    if content.is_empty() {
        return "<code></code>".into();
    }
    // Markdown reads the text between two HTML tags, so every ASCII
    // punctuation character is a reference there; an empty comment around a
    // line break keeps the line from opening a block, and after `@` from
    // forming an autolink.
    if content.contains(['\n', '\t']) {
        let mut out = String::from("<code>");
        for ch in content.chars() {
            match ch {
                '\n' => out.push_str("<!---->&#10;<!---->"),
                '\t' | ' '..='/' | ':'..='@' | '['..='`' | '{'..='~' => {
                    let _ = write!(out, "&#{};", u32::from(ch));
                    if ch == '@' {
                        out.push_str("<!---->");
                    }
                }
                _ => out.push(ch),
            }
        }
        out.push_str("</code>");
        return out;
    }
    let mut longest = 0;
    let mut run = 0;
    for ch in content.chars() {
        if ch == '`' {
            run += 1;
            longest = longest.max(run);
        } else {
            run = 0;
        }
    }
    let fence = "`".repeat(longest + 1);
    let spaced =
        content.starts_with(' ') && content.ends_with(' ') && content.contains(|c| c != ' ');
    let pad = content.starts_with('`') || content.ends_with('`') || spaced;
    if pad {
        format!("{fence} {content} {fence}")
    } else {
        format!("{fence}{content}{fence}")
    }
}

/// `decodeURIComponent`, or the text as it is where it does not decode.
fn safe_decode(fragment: &str) -> String {
    let mut out = Vec::with_capacity(fragment.len());
    let mut rest = fragment.as_bytes();
    while let Some((&b, tail)) = rest.split_first() {
        if b != b'%' {
            out.push(b);
            rest = tail;
            continue;
        }
        let byte = match tail {
            [h, l, ..] if h.is_ascii_hexdigit() && l.is_ascii_hexdigit() => {
                std::str::from_utf8(&[*h, *l])
                    .ok()
                    .and_then(|hex| u8::from_str_radix(hex, 16).ok())
            }
            _ => None,
        };
        let Some(byte) = byte else {
            return fragment.to_string();
        };
        out.push(byte);
        rest = tail.get(2..).unwrap_or_default();
    }
    String::from_utf8(out).unwrap_or_else(|_| fragment.to_string())
}

fn link_destination(m: &mut State<'_, '_>, href: &str, node: &Value) -> String {
    let allowed = m.x.allowed_destination(&strip_bidi(&clean(href)), node);
    if let Some(fragment) = allowed.strip_prefix('#')
        && let Some(slug) = m.slugs.get(&safe_decode(fragment))
    {
        return format!("#{slug}");
    }
    destination_text(&allowed)
}

fn tooltip(m: &State<'_, '_>, node: &Value) -> Option<String> {
    sv(node, "tooltip").map(str::to_string).or_else(|| {
        m.x.definitions
            .get(sv(node, "key").unwrap_or(""))
            .cloned()
            .flatten()
    })
}

fn link(m: &mut State<'_, '_>, node: &Value) -> R {
    if let Some(key) = sv(node, "key").filter(|_| is_unresolved(node)) {
        return Ok(format!(
            "[{}\\]{}",
            inlines(m, children(node))?,
            escape_text(&reference_label(node, key), false)
        ));
    }
    if m.x.link_depth > 0 {
        return inlines(m, children(node));
    }
    let destination = link_destination(m, sv(node, "href").unwrap_or(""), node);
    m.x.link_depth += 1;
    let text = inlines(m, children(node));
    m.x.link_depth -= 1;
    let text = text?;
    Ok(match tooltip(m, node) {
        None => format!("[{text}]({destination})"),
        Some(title) => format!("[{text}]({destination} \"{}\")", escape_title(&title)),
    })
}

fn image(m: &mut State<'_, '_>, node: &Value) -> String {
    let alt = escape_text(sv(node, "alt").unwrap_or(""), true);
    if let Some(key) = sv(node, "key").filter(|_| is_unresolved(node)) {
        return format!(
            "![{alt}\\]{}",
            escape_text(&reference_label(node, key), false)
        );
    }
    let src = strip_bidi(&clean(sv(node, "src").unwrap_or("")));
    let src = destination_text(&m.x.allowed_destination(&src, node));
    match tooltip(m, node) {
        None => format!("![{alt}]({src})"),
        Some(title) => format!("![{alt}]({src} \"{}\")", escape_title(&title)),
    }
}

fn autolink(m: &mut State<'_, '_>, node: &Value) -> String {
    static URI: Re =
        LazyLock::new(|| Regex::new(r"^[A-Za-z][A-Za-z0-9+.-]{1,31}:[^<>\s\\]+$").ok());
    static EMAIL: Re = LazyLock::new(|| {
        Regex::new(r"^[A-Za-z0-9.!#$%&'*+/=?^_`{|}~-]+@[A-Za-z0-9-]+(?:\.[A-Za-z0-9-]+)+$").ok()
    });
    let href_raw = sv(node, "href").unwrap_or("");
    let label = clean(sv(node, "text").unwrap_or(href_raw));
    if m.x.link_depth > 0 {
        return escape_text(&label, false);
    }
    let href = m.x.allowed_destination(&strip_bidi(&clean(href_raw)), node);
    if href == label && hit(&URI, &label) {
        return format!("<{label}>");
    }
    if href == format!("mailto:{label}") && hit(&EMAIL, &label) {
        return format!("<{label}>");
    }
    format!(
        "[{}]({})",
        escape_text(&label, false),
        destination_text(&href)
    )
}

fn inline(m: &mut State<'_, '_>, node: &Value) -> R {
    INLINES
        .iter()
        .find(|(name, _)| *name == ty(node))
        .map_or_else(|| Ok(String::new()), |(_, handler)| handler(m, node))
}

type InlineFn = fn(m: &mut State<'_, '_>, node: &Value) -> R;

const INLINES: &[(&str, InlineFn)] = &[
    ("text", inline_text),
    ("emphasis", inline_emphasis),
    ("strong", inline_strong),
    ("strike", inline_strike),
    ("underline", inline_underline),
    ("subscript", inline_subscript),
    ("superscript", inline_superscript),
    ("highlight", inline_highlight),
    ("insert", inline_insert),
    ("delete", inline_delete),
    ("small_caps", inline_small_caps),
    ("span", inline_span),
    ("code", inline_code),
    ("literal_inline", inline_literal_inline),
    ("math", inline_math),
    ("link", inline_link),
    ("image", inline_image),
    ("autolink", inline_autolink),
    ("xref", inline_xref),
    ("footnote_ref", inline_footnote_ref),
    ("inline_footnote", inline_inline_footnote),
    ("abbreviation", inline_abbreviation),
    ("ruby", inline_ruby),
    ("raw_inline", inline_raw_inline),
    ("symbol", inline_symbol),
    ("mention", inline_mention),
    ("tag", inline_tag),
    ("inline_extension", inline_inline_extension),
    ("critic_comment", inline_critic_comment),
    ("caption_number", inline_caption_number),
    ("citation_group", inline_citation_group),
    ("smart_punctuation", inline_smart_punctuation),
    ("non_breaking_space", inline_non_breaking_space),
    ("soft_break", inline_soft_break),
    ("hard_break", inline_hard_break),
];

fn inline_text(m: &mut State<'_, '_>, node: &Value) -> R {
    Ok(escape_text(
        sv(node, "value").unwrap_or(""),
        m.x.link_depth > 0,
    ))
}

fn inline_emphasis(m: &mut State<'_, '_>, node: &Value) -> R {
    delimited_node(m, node, "*", "em")
}

fn inline_strong(m: &mut State<'_, '_>, node: &Value) -> R {
    delimited_node(m, node, "**", "strong")
}

fn inline_strike(m: &mut State<'_, '_>, node: &Value) -> R {
    delimited_node(m, node, "~~", "del")
}

fn inline_underline(m: &mut State<'_, '_>, node: &Value) -> R {
    element(m, "u", node, &[])
}

fn inline_subscript(m: &mut State<'_, '_>, node: &Value) -> R {
    element(m, "sub", node, &[])
}

fn inline_superscript(m: &mut State<'_, '_>, node: &Value) -> R {
    element(m, "sup", node, &[])
}

fn inline_highlight(m: &mut State<'_, '_>, node: &Value) -> R {
    element(m, "mark", node, &[])
}

fn inline_insert(m: &mut State<'_, '_>, node: &Value) -> R {
    element(m, "ins", node, &[])
}

fn inline_delete(m: &mut State<'_, '_>, node: &Value) -> R {
    element(m, "del", node, &["critic-delete"])
}

fn inline_small_caps(m: &mut State<'_, '_>, node: &Value) -> R {
    element(m, "span", node, &["smallcaps"])
}

fn inline_span(m: &mut State<'_, '_>, node: &Value) -> R {
    Ok({
        let Some(abbr) = key_value(node, "abbr") else {
            return inlines(m, children(node));
        };
        m.abbr_depth += 1;
        let inner = inlines(m, children(node));
        m.abbr_depth -= 1;
        let inner = inner?;
        if abbr.is_empty() || !m.x.draw_derived(node, abbr) {
            return Ok(inner);
        }
        format!("<abbr title=\"{}\">{inner}</abbr>", escape_attribute(abbr))
    })
}

fn inline_code(_m: &mut State<'_, '_>, node: &Value) -> R {
    Ok(code_span(sv(node, "value").unwrap_or("")))
}

fn inline_literal_inline(_m: &mut State<'_, '_>, node: &Value) -> R {
    Ok(escape_text(sv(node, "value").unwrap_or(""), false))
}

fn inline_math(_m: &mut State<'_, '_>, node: &Value) -> R {
    Ok({
        let value = escape_html(sv(node, "value").unwrap_or(""));
        let display = flag(node, "display");
        let number = match int(node, "number") {
            Some(n) if display => {
                format!(
                    " {} {n}",
                    escape_text(sv(node, "numberPrefix").unwrap_or(""), false)
                )
            }
            _ => String::new(),
        };
        if display {
            format!("$${value}$${number}")
        } else {
            format!("${value}${number}")
        }
    })
}

fn inline_link(m: &mut State<'_, '_>, node: &Value) -> R {
    link(m, node)
}

fn inline_image(m: &mut State<'_, '_>, node: &Value) -> R {
    Ok(image(m, node))
}

fn inline_autolink(m: &mut State<'_, '_>, node: &Value) -> R {
    Ok(autolink(m, node))
}

fn inline_xref(m: &mut State<'_, '_>, node: &Value) -> R {
    Ok({
        let (text, resolved) = m.x.xref_text(node);
        if !resolved {
            return Ok(format!(
                "</#{}>",
                escape_html(sv(node, "key").unwrap_or(""))
            ));
        }
        let href = sv(node, "href").unwrap_or("");
        let slug = m.slugs.get(&safe_decode(href.get(1..).unwrap_or("")));
        match slug {
            Some(slug) if m.x.link_depth == 0 => {
                format!("[{}](#{slug})", escape_text(&text, false))
            }
            _ => escape_text(&text, false),
        }
    })
}

fn inline_footnote_ref(m: &mut State<'_, '_>, node: &Value) -> R {
    Ok({
        let raw = sv(node, "key").unwrap_or("");
        let key = escape_html(raw);
        if node.get("number").is_none() && !m.footnotes.contains(raw) {
            return Ok(format!("\\[^{}\\]", key.replace('\\', "\\\\")));
        }
        format!("[^{key}]")
    })
}

fn inline_inline_footnote(m: &mut State<'_, '_>, node: &Value) -> R {
    Ok({
        let depth = std::mem::take(&mut m.x.link_depth);
        let inner = inlines(m, children(node));
        m.x.link_depth = depth;
        format!("^[{}]", inner?)
    })
}

fn inline_abbreviation(m: &mut State<'_, '_>, node: &Value) -> R {
    Ok({
        let term = escape_html(sv(node, "key").unwrap_or(""));
        match sv(node, "expansion") {
            Some(expansion) if m.abbr_depth == 0 && m.x.draw_derived(node, expansion) => {
                format!(
                    "<abbr title=\"{}\">{term}</abbr>",
                    escape_attribute(expansion)
                )
            }
            _ => term,
        }
    })
}

fn inline_ruby(m: &mut State<'_, '_>, node: &Value) -> R {
    Ok({
        let mut out = format!("<ruby{}>", html_attributes(node.get("attrs"), &[]));
        for pair in list(node, "pairs") {
            out.push_str(&slot_inlines(m, list(pair, "base"))?);
            out.push_str("<rp>(</rp><rt>");
            out.push_str(&slot_inlines(m, list(pair, "annotation"))?);
            out.push_str("</rt><rp>)</rp>");
        }
        out.push_str("</ruby>");
        out
    })
}

fn inline_raw_inline(m: &mut State<'_, '_>, node: &Value) -> R {
    Ok({
        if sv(node, "format") != Some("html") {
            m.x.raw_dropped(node, false);
            return Ok(String::new());
        }
        m.x.loss(node, "raw-html-escaped", "Escaped raw HTML in Markdown");
        escape_html(sv(node, "value").unwrap_or(""))
    })
}

fn inline_symbol(_m: &mut State<'_, '_>, node: &Value) -> R {
    Ok(format!(":{}:", clean(sv(node, "name").unwrap_or(""))))
}

fn inline_mention(_m: &mut State<'_, '_>, node: &Value) -> R {
    Ok(match sv(node, "label") {
        Some(label) => escape_text(label, false),
        None => escape_text(&format!("@{}", sv(node, "target").unwrap_or("")), false),
    })
}

fn inline_tag(_m: &mut State<'_, '_>, node: &Value) -> R {
    Ok(escape_text(
        &format!("#{}", sv(node, "name").unwrap_or("")),
        false,
    ))
}

fn inline_inline_extension(m: &mut State<'_, '_>, node: &Value) -> R {
    Ok({
        let dotted = sv(node, "name").is_some_and(|n| n.contains('.'));
        match node.get("fallback").and_then(Value::as_array) {
            Some(fallback) if dotted => slot_inlines(m, fallback)?,
            _ => inlines(m, children(node))?,
        }
    })
}

fn inline_critic_comment(_m: &mut State<'_, '_>, node: &Value) -> R {
    Ok(format!(
        "<span class=\"critic-comment\">{}</span>",
        escape_text(sv(node, "value").unwrap_or(""), false)
    ))
}

fn inline_caption_number(_m: &mut State<'_, '_>, node: &Value) -> R {
    Ok(int(node, "n").map_or_else(|| "\\#".into(), |n| n.to_string()))
}

fn inline_citation_group(_m: &mut State<'_, '_>, node: &Value) -> R {
    Ok(escape_text(sv(node, "raw").unwrap_or(""), false))
}

fn inline_smart_punctuation(m: &mut State<'_, '_>, node: &Value) -> R {
    Ok(escape_text(m.x.smart_text(node), false))
}

fn inline_non_breaking_space(_m: &mut State<'_, '_>, _node: &Value) -> R {
    Ok("\u{a0}".into())
}

fn inline_soft_break(m: &mut State<'_, '_>, _node: &Value) -> R {
    Ok(if m.single_line > 0 { " " } else { "\n" }.into())
}

fn inline_hard_break(m: &mut State<'_, '_>, _node: &Value) -> R {
    Ok(if m.single_line > 0 { "<br>" } else { "\\\n" }.into())
}

/// Keeps a paragraph line from reading as a block of its own: a list marker,
/// a quote, a heading, a thematic break, a setext underline, or code by its
/// indentation. Spaces and tabs at the edges of a line are layout and go.
/// A line a backslash keeps from opening a list item, a quote, a heading, a
/// thematic break or a setext underline.
fn opens_block(text: &str, first: bool) -> bool {
    static PLUS_FIRST: Re = LazyLock::new(|| Regex::new(r"^[-+]([ \t]|$)").ok());
    static PLUS_LATER: Re = LazyLock::new(|| Regex::new(r"^[-+][ \t]+\S").ok());
    static ATX: Re = LazyLock::new(|| Regex::new(r"^#{1,6}([ \t]|$)").ok());
    static BREAK: Re = LazyLock::new(|| {
        Regex::new(r"^(?:-(?:[ \t]*-){2,}|_(?:[ \t]*_){2,}|\*(?:[ \t]*\*){2,})[ \t]*$").ok()
    });
    static SETEXT: Re = LazyLock::new(|| Regex::new(r"^(?:=+|-+)[ \t]*$").ok());
    hit(if first { &PLUS_FIRST } else { &PLUS_LATER }, text)
        || text.starts_with('>')
        || hit(&ATX, text)
        || hit(&BREAK, text)
        || (!first && hit(&SETEXT, text))
}

fn protect_line(line: &str, first: bool) -> String {
    static ORDERED: Re = LazyLock::new(|| Regex::new(r"^\d{1,9}[.)]([ \t]|$)").ok());
    let mut text = hard_break_end(line.trim_matches([' ', '\t']));
    if text.is_empty() {
        return text;
    }
    if opens_block(&text, first) {
        text.insert(0, '\\');
    }
    if let Some(found) = ORDERED.as_ref().and_then(|r| r.find(&text)) {
        let digits = text[..found.end()].find(['.', ')']).unwrap_or(0);
        text.insert(digits, '\\');
    }
    text
}

/// A hard break keeps its backslash as the last character of the line, with no space before it.
fn hard_break_end(text: &str) -> String {
    let backslashes = text.len() - text.trim_end_matches('\\').len();
    match text.strip_suffix('\\') {
        Some(body) if backslashes % 2 == 1 => format!("{}\\", body.trim_end_matches([' ', '\t'])),
        _ => text.to_string(),
    }
}

fn paragraph_text(text: &str) -> String {
    let lines: Vec<String> = text
        .split('\n')
        .enumerate()
        .map(|(at, line)| protect_line(line, at == 0))
        .collect();
    let last = lines.len().saturating_sub(1);
    lines
        .iter()
        .enumerate()
        .filter(|(at, line)| !line.is_empty() || *at == 0 || *at == last)
        .map(|(_, line)| line.as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

/// The blocks of a container. Two lists in a row keep apart only where their
/// markers differ, so the second of two lists with the same bullet or
/// delimiter takes the other one.
fn blocks(m: &mut State<'_, '_>, nodes: &[Value]) -> R {
    m.x.enter()?;
    let mut out = String::new();
    let mut previous: Option<&'static str> = None;
    for node in nodes {
        if ty(node) != "list" {
            let written = block(m, node, None)?;
            if !written.is_empty() {
                previous = None;
            }
            out.push_str(&written);
            continue;
        }
        let mark = list_mark(node);
        let used = if previous == Some(mark) {
            alternate_mark(mark)
        } else {
            mark
        };
        out.push_str(&block(m, node, Some(used))?);
        previous = Some(used);
    }
    m.x.leave();
    Ok(out)
}

fn list_mark(node: &Value) -> &'static str {
    let marker = node.get("marker").unwrap_or(&Value::Null);
    if sv(marker, "kind") == Some("ordered") {
        return if sv(marker, "delimiter") == Some(")") {
            ")"
        } else {
            "."
        };
    }
    match sv(marker, "char") {
        Some("*") => "*",
        Some("+") => "+",
        _ => "-",
    }
}

fn alternate_mark(mark: &str) -> &'static str {
    match mark {
        "." => ")",
        ")" => ".",
        "-" => "*",
        _ => "-",
    }
}

const ATTRIBUTE_BLOCKS: [&str; 12] = [
    "heading",
    "paragraph",
    "code_block",
    "block_quote",
    "list",
    "table",
    "thematic_break",
    "admonition",
    "directive",
    "line_block",
    "definition_list",
    "figure_group",
];

fn block(m: &mut State<'_, '_>, node: &Value, mark: Option<&'static str>) -> R {
    if ATTRIBUTE_BLOCKS.contains(&ty(node)) && has_attributes(node) {
        m.x.loss(
            node,
            "markdown-attributes-dropped",
            "Dropped block attributes in Markdown",
        );
    }
    match ty(node) {
        "list" => list_block(m, node, mark.unwrap_or_else(|| list_mark(node))),
        _ => BLOCKS
            .iter()
            .find(|(name, _)| *name == ty(node))
            .map_or_else(|| Ok(String::new()), |(_, handler)| handler(m, node)),
    }
}

type BlockFn = fn(m: &mut State<'_, '_>, node: &Value) -> R;

const BLOCKS: &[(&str, BlockFn)] = &[
    ("section", block_section),
    ("heading", block_heading),
    ("paragraph", block_paragraph),
    ("code_block", block_code_block),
    ("block_quote", block_block_quote),
    ("thematic_break", block_thematic_break),
    ("table", block_table),
    ("admonition", block_admonition),
    ("directive", block_admonition),
    ("div", block_div),
    ("line_block", block_line_block),
    ("definition_list", block_definition_list),
    ("figure_group", block_figure_group),
    ("include", block_include),
    ("block_extension", block_block_extension),
    ("raw_block", block_raw_block),
    ("abbreviation_def", block_abbreviation_def),
    ("link_reference_definition", block_link_reference_definition),
];

fn block_section(m: &mut State<'_, '_>, node: &Value) -> R {
    blocks(m, children(node))
}

fn block_heading(m: &mut State<'_, '_>, node: &Value) -> R {
    heading(m, node)
}

fn block_paragraph(m: &mut State<'_, '_>, node: &Value) -> R {
    let written = format!("{}\n\n", paragraph_text(&inlines(m, children(node))?));
    let caption = m.caption(node);
    captioned(m, node, written, caption, "\n")
}

fn block_code_block(m: &mut State<'_, '_>, node: &Value) -> R {
    let written = code_block(m, node);
    let caption = m.caption(node);
    captioned(m, node, written, caption, "\n")
}

fn block_block_quote(m: &mut State<'_, '_>, node: &Value) -> R {
    let written = quote(&blocks(m, children(node))?);
    let caption = m.caption(node);
    captioned(m, node, written, caption, "\n\n")
}

fn block_thematic_break(_m: &mut State<'_, '_>, _node: &Value) -> R {
    Ok("---\n\n".into())
}

fn block_table(m: &mut State<'_, '_>, node: &Value) -> R {
    table(m, node)
}

fn block_admonition(m: &mut State<'_, '_>, node: &Value) -> R {
    Ok(format!(
        "{}{}",
        title_lines(m, node)?,
        blocks(m, children(node))?
    ))
}

fn block_div(m: &mut State<'_, '_>, node: &Value) -> R {
    m.x.loss(
        node,
        "markdown-container-flattened",
        "Flattened a container in Markdown",
    );
    Ok(format!(
        "{}{}",
        title_lines(m, node)?,
        blocks(m, children(node))?
    ))
}

fn block_line_block(m: &mut State<'_, '_>, node: &Value) -> R {
    line_block(m, node)
}

fn block_definition_list(m: &mut State<'_, '_>, node: &Value) -> R {
    definition_list(m, node)
}

fn block_figure_group(m: &mut State<'_, '_>, node: &Value) -> R {
    figure_group(m, node)
}

fn block_include(m: &mut State<'_, '_>, node: &Value) -> R {
    let caption = m.caption(node);
    if node.get("children").is_none() {
        let line = m.x.unexpanded_include(node);
        let written = format!("{}\n\n", paragraph_text(&escape_text(&line, false)));
        return captioned(m, node, written, caption, "\n");
    }
    let written = blocks(m, children(node))?;
    captioned(m, node, written, caption, "\n\n")
}

fn block_block_extension(m: &mut State<'_, '_>, node: &Value) -> R {
    let fallback = list(node, "fallback");
    blocks(
        m,
        if fallback.is_empty() {
            children(node)
        } else {
            fallback
        },
    )
}

fn block_raw_block(m: &mut State<'_, '_>, node: &Value) -> R {
    if sv(node, "format") != Some("html") {
        m.x.raw_dropped(node, true);
        return Ok(String::new());
    }
    m.x.loss(node, "raw-html-escaped", "Escaped raw HTML in Markdown");
    Ok(format!(
        "{}\n\n",
        escape_html(sv(node, "value").unwrap_or(""))
    ))
}

fn block_abbreviation_def(_m: &mut State<'_, '_>, node: &Value) -> R {
    Ok(format!(
        "*[{}]: {}\n\n",
        escape_html(sv(node, "key").unwrap_or("")),
        escape_html(sv(node, "expansion").unwrap_or(""))
    ))
}

fn block_link_reference_definition(m: &mut State<'_, '_>, node: &Value) -> R {
    Ok(link_definition(m, node))
}

fn line_block(m: &mut State<'_, '_>, node: &Value) -> R {
    let mut lines = Vec::new();
    for line in children(node) {
        if ty(line) != "line" {
            continue;
        }
        let kids = children(line);
        let kids = match kids.split_last() {
            Some((end, rest)) if ty(end) == "hard_break" => rest,
            _ => kids,
        };
        lines.push(paragraph_text(&inlines(m, kids)?));
    }
    Ok(if lines.is_empty() {
        String::new()
    } else {
        format!("{}\n\n", lines.join("\\\n"))
    })
}

fn definition_list(m: &mut State<'_, '_>, node: &Value) -> R {
    let mut out = String::new();
    for child in children(node) {
        match ty(child) {
            "definition_term" => out.push_str(&strong_line(&inlines(m, children(child))?)),
            "definition_description" => out.push_str(&blocks(m, children(child))?),
            _ => {}
        }
    }
    Ok(out)
}

fn figure_group(m: &mut State<'_, '_>, node: &Value) -> R {
    let mut out = String::new();
    for child in children(node) {
        out.push_str(&panel(m, child)?);
    }
    if let Some(caption) = slot(node, "caption") {
        m.x.loss(
            node,
            "markdown-caption-flattened",
            "Flattened a caption in Markdown",
        );
        out.push_str(&strong_line(&caption_text(m, caption)?));
    }
    Ok(out)
}

fn link_definition(m: &mut State<'_, '_>, node: &Value) -> String {
    let key = sv(node, "key").unwrap_or("");
    if m.referenced.contains(key) {
        return String::new();
    }
    let allowed =
        m.x.allowed_destination(&strip_bidi(&clean(sv(node, "href").unwrap_or(""))), node);
    let href = if allowed.is_empty() {
        "<>".to_string()
    } else {
        destination_text(&allowed)
    };
    let title =
        sv(node, "tooltip").map_or_else(String::new, |t| format!(" \"{}\"", escape_title(t)));
    let mut escaped = String::new();
    for c in key.chars() {
        if matches!(c, '[' | ']' | '\\') {
            escaped.push('\\');
        }
        escaped.push(c);
    }
    format!("[{escaped}]: {href}{title}\n\n")
}

/// `text` on a line of its own in bold, or nothing where it is empty.
fn strong_line(text: &str) -> String {
    let run = delimited(&text.replace('\n', " "), "**", "strong");
    let run = run.trim();
    if run.is_empty() {
        String::new()
    } else {
        format!("{run}\n\n")
    }
}

fn title_lines(m: &mut State<'_, '_>, node: &Value) -> R {
    let mut out = String::new();
    if node.get("title").is_some() {
        m.unwrap_strong += 1;
        let text = single_line(m, |m| inlines(m, list(node, "title")));
        m.unwrap_strong -= 1;
        out.push_str(&strong_line(&text?));
    }
    if let Some(label) = sv(node, "groupLabel").filter(|l| !l.is_empty()) {
        out.push_str(&strong_line(&escape_text(label, false)));
    }
    Ok(out)
}

fn caption_text(m: &mut State<'_, '_>, caption: &Value) -> R {
    single_line(m, |m| {
        let mut parts = Vec::new();
        for child in children(caption) {
            if ty(child) == "paragraph" {
                parts.push(inlines(m, children(child))?);
            } else {
                parts.push(blocks(m, std::slice::from_ref(child))?.trim().to_string());
            }
        }
        Ok(parts.join(" ").trim().to_string())
    })
}

/// A host with its caption after it, as a paragraph of its own; Markdown has
/// no caption, so this is reported.
fn captioned(
    m: &mut State<'_, '_>,
    host: &Value,
    written: String,
    caption: Option<&Value>,
    separator: &str,
) -> R {
    let Some(caption) = caption else {
        return Ok(written);
    };
    m.x.loss(
        host,
        "markdown-caption-flattened",
        "Flattened a caption in Markdown",
    );
    Ok(format!(
        "{}{separator}{}\n\n",
        written.trim_end(),
        caption_text(m, caption)?
    ))
}

/// A panel of a figure group: the host, then its caption emphasized,
/// subordinate to the group's bold one.
fn panel(m: &mut State<'_, '_>, node: &Value) -> R {
    let caption = match ty(node) {
        "paragraph" | "code_block" | "block_quote" => slot(node, "caption"),
        _ => None,
    };
    let Some(caption) = caption else {
        return block(m, node, None);
    };
    m.x.loss(
        node,
        "markdown-caption-flattened",
        "Flattened a caption in Markdown",
    );
    let text = delimited(&caption_text(m, caption)?, "*", "em");
    let text = text.trim();
    let held = m.uncaptioned.replace(std::ptr::from_ref(node));
    let host = block(m, node, None);
    m.uncaptioned = held;
    Ok(format!(
        "{}{}",
        host?,
        if text.is_empty() {
            String::new()
        } else {
            format!("{text}\n\n")
        }
    ))
}

fn heading(m: &mut State<'_, '_>, node: &Value) -> R {
    static CLOSING: Re = LazyLock::new(|| Regex::new(r"(^|[ \t])(#+)$").ok());
    let raw = single_line(m, |m| inlines(m, children(node)))?;
    let mut text = String::new();
    let mut space = false;
    for c in raw.chars() {
        if matches!(c, ' ' | '\t' | '\n') {
            space = true;
            continue;
        }
        if space && !text.is_empty() {
            text.push(' ');
        }
        space = false;
        text.push(c);
    }
    let text = text.trim().to_string();
    // A closing run of `#` would be read as the optional closing sequence.
    let text = match CLOSING.as_ref() {
        Some(r) => r.replace(&text, "$1\\$2").into_owned(),
        None => text,
    };
    let level = usize::try_from(int(node, "level").unwrap_or(1).clamp(1, 6)).unwrap_or(1);
    Ok(format!(
        "{}{}\n\n",
        "#".repeat(level),
        if text.is_empty() {
            String::new()
        } else {
            format!(" {text}")
        }
    ))
}

fn fence(content: &str, marker: char) -> String {
    let mut longest = 0;
    for line in content.split('\n') {
        let indent = line.len() - line.trim_start_matches(' ').len();
        if indent > 3 {
            continue;
        }
        let run = line[indent..].chars().take_while(|&c| c == marker).count();
        longest = longest.max(run);
    }
    marker.to_string().repeat(3.max(longest + 1))
}

/// The info string of a fence: the language, the title in quotes where there
/// is a language, and the group label.
fn fence_info(m: &mut State<'_, '_>, node: &Value) -> String {
    let syntax = clean(sv(node, "syntax").unwrap_or(""));
    let token = syntax
        .split([' ', '\t', '\n'])
        .next()
        .unwrap_or("")
        .to_string();
    // An attribute line's title wins over the one in the fence header, as in HTML.
    let title = key_value(node, "title").map(str::to_string).or_else(|| {
        node.get("title")
            .map(|_| plain_inline_text(m.x, list(node, "title")))
    });
    let mut info = token.clone();
    match title {
        Some(title) if !token.is_empty() => {
            let _ = write!(info, " \"{}\"", escape_title(&title));
        }
        Some(_) => m.x.loss(
            node,
            "fence-title-dropped",
            "Dropped a code fence title without a language in Markdown",
        ),
        None => {}
    }
    let label = sv(node, "groupLabel").unwrap_or("");
    if label.is_empty() {
        info
    } else {
        let label: String = clean(label)
            .chars()
            .filter(|c| !matches!(c, '[' | ']'))
            .collect();
        format!("{info} [{label}]")
    }
}

fn code_block(m: &mut State<'_, '_>, node: &Value) -> String {
    let content = clean(sv(node, "value").unwrap_or(""));
    let info = fence_info(m, node);
    let delimiter = fence(&content, if info.contains('`') { '~' } else { '`' });
    let body = if content.is_empty() || content.ends_with('\n') {
        content
    } else {
        format!("{content}\n")
    };
    format!("{delimiter}{info}\n{body}{delimiter}\n\n")
}

fn quote(inner: &str) -> String {
    let lines: Vec<String> = inner
        .trim_end_matches('\n')
        .split('\n')
        .map(|line| {
            if line.is_empty() {
                ">".into()
            } else {
                format!("> {line}")
            }
        })
        .collect();
    format!("{}\n\n", lines.join("\n"))
}

/// Puts `marker` before the first line and indents the others by `pad`; an
/// empty line takes no indentation.
fn with_marker(marker: &str, content: &str, pad: usize) -> String {
    let indent = " ".repeat(pad);
    let lines: Vec<String> = content
        .trim_end_matches('\n')
        .split('\n')
        .enumerate()
        .map(|(at, line)| {
            if at == 0 {
                format!("{marker}{line}").trim_end().to_string()
            } else if line.is_empty() {
                String::new()
            } else {
                format!("{indent}{line}")
            }
        })
        .collect();
    format!("{}\n", lines.join("\n"))
}

fn list_block(m: &mut State<'_, '_>, node: &Value, mark: &str) -> R {
    let marker = node.get("marker").unwrap_or(&Value::Null);
    let ordered = sv(marker, "kind") == Some("ordered");
    if ordered && let Some(style) = sv(marker, "style").filter(|st| *st != "1") {
        let message = format!("Dropped the ordered list style \"{style}\" in Markdown");
        m.x.loss(node, "list-style-dropped", message);
    }
    let tight = sv(node, "spacing") == Some("tight");
    let mut counter = int(marker, "start").unwrap_or(1);
    let mut out = String::new();
    for (at, item) in children(node).iter().enumerate() {
        if !tight && at > 0 {
            out.push('\n');
        }
        let mut prefix = if ordered {
            let p = format!("{counter}{mark} ");
            counter += 1;
            p
        } else {
            format!("{mark} ")
        };
        let pad = prefix.chars().count();
        if !ordered && let Some(task) = item.get("task") {
            prefix.push_str(if matches!(sv(task, "mark"), Some("x" | "X")) {
                "[x] "
            } else {
                "[ ] "
            });
        }
        let content = item_content(m, children(item), tight)?;
        out.push_str(&with_marker(&prefix, &content, pad));
    }
    out.push('\n');
    Ok(out)
}

static INTERRUPTERS: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    [
        r"^>",
        r"^#{1,6}(?:[ \t]|$)",
        r"^(?:`{3,}|~{3,})",
        r"^[-*+][ \t]+\S",
        r"^1[.)][ \t]+\S",
        r"^(?:\*[ \t]*){3,}$",
    ]
    .iter()
    .filter_map(|p| Regex::new(p).ok())
    .collect()
});

static DELIMITER_ROW: Re = LazyLock::new(|| Regex::new(r"^\|(?:[ \t]*:?-+:?[ \t]*\|)+$").ok());

fn last_line(text: &str) -> &str {
    text.split('\n')
        .rfind(|l| !l.trim().is_empty())
        .unwrap_or("")
}

/// Whether a tight item may write `written` right under the block above,
/// with no blank line that makes it loose.
fn separator_can_go(written: &str, above: Option<&str>, above_written: &str) -> bool {
    let mut lines = written.split('\n');
    let first = lines.next().unwrap_or("");
    let second = lines.next().unwrap_or("");
    if first.starts_with('|') {
        return hit(&DELIMITER_ROW, second)
            && above.is_some_and(|a| {
                matches!(a, "paragraph" | "heading" | "code_block" | "thematic_break")
            });
    }
    if matches!(above, Some("code_block" | "thematic_break" | "heading")) {
        return true;
    }
    if first.starts_with('>') && last_line(above_written).starts_with('>') {
        return false;
    }
    INTERRUPTERS.iter().any(|opener| opener.is_match(first))
}

/// The blocks of an item; a tight item drops the blank line between two
/// blocks wherever the second interrupts.
fn item_content(m: &mut State<'_, '_>, nodes: &[Value], tight: bool) -> R {
    let mut out = String::new();
    let mut above: Option<&str> = None;
    let mut above_written = String::new();
    for child in nodes {
        let mut written = block(m, child, None)?;
        if written.is_empty() {
            continue;
        }
        if tight
            && ty(child) == "thematic_break"
            && matches!(above, Some("paragraph" | "block_quote"))
        {
            written = "***\n\n".into();
        }
        if tight && out.ends_with("\n\n") && separator_can_go(&written, above, &above_written) {
            out.pop();
        }
        out.push_str(&written);
        above = Some(ty(child));
        above_written = written;
    }
    Ok(out)
}

fn cell_text(m: &mut State<'_, '_>, cell: &Value) -> R {
    let text = single_line(m, |m| {
        let mut parts = Vec::new();
        for child in children(cell) {
            if ty(child) == "cell_paragraph" {
                parts.push(inlines(m, children(child))?);
            } else {
                let written = blocks(m, std::slice::from_ref(child))?;
                let mut folded = String::new();
                let mut breaks = false;
                for c in written.trim().chars() {
                    if c == '\n' {
                        breaks = true;
                        continue;
                    }
                    if breaks {
                        folded.push(' ');
                        breaks = false;
                    }
                    folded.push(c);
                }
                parts.push(folded);
            }
        }
        Ok(parts.join(" "))
    })?;
    Ok(escape_pipes(text.replace(['\r', '\n'], " ").trim()))
}

/// GFM splits a row on every `|` that no backslash escapes, code spans included.
fn escape_pipes(cell: &str) -> String {
    let mut out = String::with_capacity(cell.len());
    let mut chars = cell.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\\'
            && let Some(next) = chars.next()
        {
            out.push(ch);
            out.push(next);
            continue;
        }
        if ch == '|' {
            out.push_str("\\|");
        } else {
            out.push(ch);
        }
    }
    out
}

fn table_cells(m: &mut State<'_, '_>, row: &Value, is_header: bool) -> Result<Vec<String>, Error> {
    let mut cells = Vec::new();
    for cell in children(row) {
        if int(cell, "colspan").unwrap_or(1) > 1 || int(cell, "rowspan").unwrap_or(1) > 1 {
            m.x.loss(
                cell,
                "table-span-flattened",
                "Flattened a spanned table cell while rendering markdown",
            );
        }
        if !is_header && flag(cell, "header") {
            m.x.loss(
                cell,
                "table-header-cell-demoted",
                "Wrote a header cell as an ordinary cell in Markdown",
            );
        }
        cells.push(if cell.get("span").is_none() {
            cell_text(m, cell)?
        } else {
            String::new()
        });
    }
    Ok(cells)
}

/// The rows of a table in order, comments left out.
fn rows_of(node: &Value) -> impl Iterator<Item = &Value> {
    children(node)
        .iter()
        .filter(|section| ty(section) != "comment")
        .flat_map(children)
        .filter(|row| ty(row) != "comment")
}

/// The alignment row cell of a column.
fn align_cell(column: Option<&Value>) -> &'static str {
    let value = column.and_then(|c| sv(c, "align").or_else(|| sv(c, "resolvedAlign")));
    match value {
        Some("left") => ":---",
        Some("center") => ":---:",
        Some("right") => "---:",
        _ => "---",
    }
}

fn table(m: &mut State<'_, '_>, node: &Value) -> R {
    let mut header: Option<Vec<String>> = None;
    let mut rows: Vec<Vec<String>> = Vec::new();
    {
        for row in rows_of(node) {
            let cells_of = children(row);
            let is_header = !cells_of.is_empty()
                && cells_of
                    .iter()
                    .all(|c| flag(c, "header") || c.get("span").is_some());
            let cells = table_cells(m, row, is_header)?;
            if is_header && header.is_none() && rows.is_empty() {
                header = Some(cells);
            } else {
                if is_header {
                    m.x.loss(
                        row,
                        "table-header-row-demoted",
                        "Wrote a header row as an ordinary row in Markdown",
                    );
                }
                rows.push(cells);
            }
        }
    }
    let columns = list(node, "columns");
    let width = rows
        .iter()
        .map(Vec::len)
        .chain([columns.len(), header.as_ref().map_or(0, Vec::len)])
        .max()
        .unwrap_or(0);
    let line = |cells: &[String]| {
        let mut padded: Vec<&str> = cells.iter().map(String::as_str).collect();
        padded.resize(width.max(cells.len()), "");
        format!("| {} |", padded.join(" | "))
    };
    let align = |at: usize| align_cell(columns.get(at));
    let mut out = format!(
        "{}\n| {} |\n",
        line(header.as_deref().unwrap_or(&[])),
        (0..width).map(align).collect::<Vec<_>>().join(" | ")
    );
    for row in &rows {
        out.push_str(&line(row));
        out.push('\n');
    }
    out.push('\n');
    let caption = m.caption(node);
    captioned(m, node, out, caption, "\n\n")
}

fn footnote_section(m: &mut State<'_, '_>, document: &Value) -> R {
    let mut out = String::new();
    for child in footnote_definitions(document) {
        let depth = std::mem::take(&mut m.x.link_depth);
        let body = blocks(m, children(child));
        m.x.link_depth = depth;
        let marker = format!("[^{}]: ", escape_html(sv(child, "key").unwrap_or("")));
        out.push_str(&with_marker(&marker, &body?, 4));
    }
    Ok(out)
}

fn frontmatter(document: &Value) -> String {
    let Some(node) = children(document).iter().find(|c| ty(c) == "frontmatter") else {
        return String::new();
    };
    let value = sv(node, "value").unwrap_or("");
    let value = value.strip_suffix('\n').unwrap_or(value);
    let format = sv(node, "format").unwrap_or("");
    format!(
        "---{}\n{value}\n---\n\n",
        if format == "yaml" { "" } else { format }
    )
}

const SLUG_CONTAINERS: [&str; 9] = [
    "section",
    "include",
    "block_quote",
    "div",
    "admonition",
    "directive",
    "list",
    "list_item",
    "footnote",
];

/// GFM's slug of the text of a heading: lower case, punctuation dropped, spaces as hyphens.
fn gfm_slug(x: &Exporter<'_>, heading: &Value) -> String {
    static DROP: Re = LazyLock::new(|| Regex::new(r"[^\p{L}\p{M}\p{N}\p{Pc} -]").ok());
    let slug = crate::resolve::slug::slug(
        &plain_inline_text(x, children(heading)),
        crate::options::HeadingIds::KeepCase,
    );
    let lower = slug.to_lowercase();
    match DROP.as_ref() {
        Some(r) => r.replace_all(&lower, "").replace(' ', "-"),
        None => lower.replace(' ', "-"),
    }
}

/// The GFM slug of each heading with an id, in document order, with GFM's numbering of repeats.
fn heading_slugs(x: &Exporter<'_>, document: &Value) -> HashMap<String, String> {
    fn visit(
        x: &Exporter<'_>,
        nodes: &[Value],
        slugs: &mut HashMap<String, String>,
        counts: &mut HashMap<String, usize>,
    ) {
        for child in nodes {
            if ty(child) == "heading" {
                let id = child
                    .get("attrs")
                    .and_then(|a| sv(a, "id"))
                    .or_else(|| sv(child, "generatedId"));
                let base = gfm_slug(x, child);
                let seen = *counts.get(&base).unwrap_or(&0);
                counts.insert(base.clone(), seen + 1);
                if let Some(id) = id
                    && !slugs.contains_key(id)
                {
                    slugs.insert(
                        id.to_string(),
                        if seen == 0 {
                            base
                        } else {
                            format!("{base}-{seen}")
                        },
                    );
                }
            } else if SLUG_CONTAINERS.contains(&ty(child)) {
                visit(x, children(child), slugs, counts);
            }
        }
    }
    let mut slugs = HashMap::new();
    visit(x, children(document), &mut slugs, &mut HashMap::new());
    slugs
}

fn collapse_blank_lines(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut run = 0;
    for c in text.chars() {
        if c == '\n' {
            run += 1;
            if run > 2 {
                continue;
            }
        } else {
            run = 0;
        }
        out.push(c);
    }
    out
}

/// The Markdown of a resolved tree.
pub fn export_markdown(x: &mut Exporter<'_>) -> R {
    let document = x.document;
    let mut footnotes = HashSet::new();
    let mut referenced = HashSet::new();
    each_node(document, &mut |node| match ty(node) {
        "footnote" => {
            footnotes.insert(sv(node, "key").unwrap_or(""));
        }
        "link" | "image" => {
            if let Some(key) = sv(node, "key").filter(|_| !is_unresolved(node)) {
                referenced.insert(key);
            }
        }
        _ => {}
    });
    let slugs = heading_slugs(x, document);
    let mut m = State {
        x,
        slugs,
        footnotes,
        referenced,
        abbr_depth: 0,
        inline_level: 0,
        open: Vec::new(),
        single_line: 0,
        unwrap_strong: 0,
        uncaptioned: None,
    };
    let written = format!(
        "{}{}",
        blocks(&mut m, children(document))?,
        footnote_section(&mut m, document)?
    );
    let body = collapse_blank_lines(&written);
    let body = body.trim_start_matches('\n').trim_end();
    let head = frontmatter(document);
    if body.is_empty() {
        return Ok(if head.is_empty() {
            String::new()
        } else {
            format!("{}\n", head.trim_end())
        });
    }
    Ok(strip_bidi(&format!("{head}{body}\n")))
}
