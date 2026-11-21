//! Helpers shared by the fuzz targets.

use carve::api::{self, EncodeOptions, Options};
use serde_json::Value;
use std::sync::OnceLock;

/// Inputs longer than this are skipped: the targets look for crashes and
/// gate failures, and the timing class has its own budget (`-timeout`).
pub const MAX_INPUT: usize = 64 * 1024;

/// Defer only the saved carve-rs#4538 timeout until the C2-W-088 repair lands.
/// The normal fuzz target still runs it; only the CI smoke job opts out.
pub fn skip_fmt_smoke_input(data: &[u8]) -> bool {
    static SMOKE: OnceLock<bool> = OnceLock::new();
    *SMOKE.get_or_init(|| {
        std::env::var_os("CARVE_FUZZ_SMOKE").as_deref() == Some(std::ffi::OsStr::new("1"))
    }) && is_deferred_fmt_input(data)
}

fn is_deferred_fmt_input(data: &[u8]) -> bool {
    data == include_bytes!("../fixtures/timeout-d1cfc854")
}

/// The tree of `src` without positions, under the equality of C2-A-162.
/// `metadata.fields` is dropped: the frontmatter exception of C2-W-002.
#[must_use]
pub fn bare_tree(src: &str) -> Option<Value> {
    let tree = api::parse(src, &Options::default()).ok()?;
    let mut v: Value = serde_json::from_str(&api::encode_tree_with(
        &tree,
        EncodeOptions { positions: false },
    ))
    .ok()?;
    strip(&mut v);
    Some(v)
}

fn strip(v: &mut Value) {
    match v {
        Value::Object(m) => {
            m.remove("pos");
            m.remove("segments");
            m.remove("srcByteLength");
            m.remove("fields");
            m.values_mut().for_each(strip);
        }
        Value::Array(a) => a.iter_mut().for_each(strip),
        _ => {}
    }
}

/// Render options picked by one selector byte.
#[must_use]
pub fn options(sel: u8) -> Options {
    use carve::api::{HeadingIds, Mode, Profile, SoftBreak};
    let mut o = Options::default();
    o.profile = match sel & 3 {
        0 => Profile::Full,
        1 => Profile::Article,
        2 => Profile::Comment,
        _ => Profile::Minimal,
    };
    o.mode = if sel & 4 == 0 {
        Mode::Interactive
    } else {
        Mode::Static
    };
    o.soft_break = match (sel >> 3) & 3 {
        0 => SoftBreak::Newline,
        1 => SoftBreak::Space,
        _ => SoftBreak::Br,
    };
    o.smart_typography = sel & 0x20 != 0;
    o.sections = sel & 0x40 != 0;
    o.raw_html = sel & 0x80 != 0;
    o.source_lines = sel & 0x10 != 0;
    o.heading_ids = if sel & 0x08 != 0 {
        HeadingIds::AsciiLower
    } else {
        HeadingIds::KeepCase
    };
    o
}

const STANDARD: &[&str] = &[
    "carve.cite",
    "carve.codeGroup",
    "carve.details",
    "carve.fencedRender.mermaid",
    "carve.imgFence.image",
    "carve.imgFence.img",
    "carve.mathBlock",
    "carve.spoiler",
    "carve.spoilerBlock",
    "carve.tab",
    "carve.tabs",
    "carve.wiki",
];

/// A host with every standard extension of `spec-data/extensions.json` and
/// the bundles that take no options.
#[must_use]
pub fn extension_host(renderers: bool) -> carve::api::Carve {
    use carve::api::extensions;
    let mut carve = carve::api::Carve::new();
    for name in STANDARD {
        let _ = carve.enable_standard(name, renderers);
    }
    for bundle in [
        extensions::glossary(),
        extensions::list_table(),
        extensions::code_callouts(),
        extensions::semantic_span(),
    ] {
        let _ = carve.register(bundle);
    }
    carve
}

const DENIED_SCHEMES: &[&str] = &[
    "javascript",
    "vbscript",
    "data",
    "file",
    "ms-msdt",
    "ms-office",
    "ms-word",
    "ms-excel",
    "ms-powerpoint",
    "ms-access",
    "ms-visio",
    "ms-project",
    "ms-publisher",
    "ms-infopath",
    "ms-spd",
    "ms-search",
    "search-ms",
    "ms-cxh",
    "ms-cxh-full",
    "shell",
    "vscode",
    "vscode-insiders",
    "jar",
];

const URL_ATTRIBUTES: &[&str] = &[
    "href",
    "src",
    "action",
    "formaction",
    "xlink:href",
    "poster",
    "cite",
    "data",
    "background",
    "ping",
    "longdesc",
    "lowsrc",
    "dynsrc",
    "manifest",
    "codebase",
    "icon",
    "attributionsrc",
];

const LIST_ATTRIBUTES: &[&str] = &["srcset", "imagesrcset"];

const STYLE_PATTERNS: &[&str] = &[
    "-moz-binding",
    "@import",
    "behavior:",
    "expression(",
    "image(",
    "image-set(",
    "src(",
    "url(",
];

/// The scheme a browser reads from an attribute value (WHATWG URL): leading
/// and trailing C0 controls and spaces stripped, tabs and newlines removed.
fn scheme(value: &str) -> Option<String> {
    let v: String = value
        .trim_matches(|c: char| c <= ' ')
        .chars()
        .filter(|c| !matches!(c, '\t' | '\n' | '\r'))
        .collect();
    let colon = v.find(':')?;
    let head = v.get(..colon)?;
    let mut chars = head.chars();
    let first = chars.next()?;
    (first.is_ascii_alphabetic()
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.')))
    .then(|| head.to_ascii_lowercase())
}

fn denied(value: &str) -> bool {
    scheme(value).is_some_and(|s| DENIED_SCHEMES.contains(&s.as_str()))
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        let hex = |c: u8| char::from(c).to_digit(16);
        match (
            b.get(i),
            b.get(i + 1).copied().and_then(hex),
            b.get(i + 2).copied().and_then(hex),
        ) {
            (Some(b'%'), Some(h), Some(l)) => {
                out.push(u8::try_from(h * 16 + l).unwrap_or(0));
                i += 3;
            }
            (Some(&c), _, _) => {
                out.push(c);
                i += 1;
            }
            _ => break,
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The first unsafe thing in `html` as a browser parses it, if any.
#[must_use]
pub fn unsafe_html(html: &str) -> Option<String> {
    unsafe_markup(html, false)
}

/// `svg`: the markup is the image of an imgFence `<img>` (C2-X-059, C2-X-060),
/// which must also hold no style element, foreign content or reference that
/// leaves the image.
fn unsafe_markup(html: &str, svg: bool) -> Option<String> {
    use html5ever::tendril::TendrilSink;
    use markup5ever_rcdom::{NodeData, RcDom};
    let dom = html5ever::parse_fragment(
        RcDom::default(),
        html5ever::ParseOpts::default(),
        html5ever::QualName::new(None, html5ever::ns!(html), html5ever::local_name!("body")),
        Vec::new(),
        false,
    )
    .one(html);
    // The root stays owned by `dom`: dropping an rcdom node empties the
    // children of its whole subtree, even of nodes held elsewhere.
    let mut stack = vec![dom.document.clone()];
    while let Some(node) = stack.pop() {
        if let NodeData::Element { name, attrs, .. } = &node.data {
            let tag = name.local.to_string().to_ascii_lowercase();
            let attrs = attrs.borrow();
            let attr = |n: &str| {
                attrs
                    .iter()
                    .find(|a| a.name.local.to_string().eq_ignore_ascii_case(n))
                    .map(|a| a.value.to_string())
            };
            if tag == "script" && attr("type").as_deref() != Some("application/json") {
                return Some("a script element".into());
            }
            if matches!(
                tag.as_str(),
                "iframe" | "frame" | "frameset" | "object" | "embed" | "base" | "meta" | "applet"
            ) {
                return Some(format!("a {tag} element"));
            }
            if svg && matches!(tag.as_str(), "foreignobject" | "style" | "iframe" | "a") {
                return Some(format!("a {tag} element in an imgFence image"));
            }
            let mut image = false;
            if tag == "img" {
                if let Some(data) = attr("src")
                    .as_deref()
                    .and_then(|v| v.strip_prefix("data:image/svg+xml,"))
                {
                    if svg {
                        return Some("an image inside an imgFence image".into());
                    }
                    if let Some(p) = unsafe_markup(&percent_decode(data), true) {
                        return Some(format!("imgFence image: {p}"));
                    }
                    image = true;
                }
            }
            for a in attrs.iter() {
                let n = a.name.local.to_string().to_ascii_lowercase();
                let qualified = match &a.name.prefix {
                    Some(p) => format!("{p}:{n}"),
                    None => n.clone(),
                };
                let v: &str = &a.value;
                if n.starts_with("on") || n == "srcdoc" {
                    return Some(format!("attribute {n} on {tag}"));
                }
                if (URL_ATTRIBUTES.contains(&n.as_str())
                    || URL_ATTRIBUTES.contains(&qualified.as_str()))
                    && !(image && n == "src")
                    && (denied(v) || n == "ping" && v.split_ascii_whitespace().any(denied))
                {
                    return Some(format!("{n}={v:?} on {tag}"));
                }
                if LIST_ATTRIBUTES.contains(&n.as_str())
                    && v.split(',').any(|c| denied(c.trim_start()))
                {
                    return Some(format!("{n}={v:?} on {tag}"));
                }
                if svg
                    && (n == "href" || qualified == "xlink:href")
                    && !v.trim_start().starts_with('#')
                {
                    return Some(format!("{qualified}={v:?} in an imgFence image"));
                }
                if n == "style" {
                    let lower = v.to_ascii_lowercase();
                    if let Some(p) = STYLE_PATTERNS.iter().find(|p| lower.contains(*p)) {
                        return Some(format!("style with {p} on {tag}"));
                    }
                }
            }
        }
        stack.extend(node.children.borrow().iter().cloned());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::{is_deferred_fmt_input, unsafe_html};

    #[test]
    fn deferred_fmt_fixture_is_one_exact_input() {
        let fixture = include_bytes!("../fixtures/timeout-d1cfc854");
        assert_eq!(fixture.len(), 25_163);
        assert!(is_deferred_fmt_input(fixture));
        assert!(!is_deferred_fmt_input(&fixture[..fixture.len() - 1]));
        let mut changed = fixture.to_vec();
        changed[0] ^= 1;
        assert!(!is_deferred_fmt_input(&changed));
    }

    #[test]
    fn the_safety_check_sees_what_a_browser_runs() {
        assert!(unsafe_html("<a href=\" jav&#x09;ascript:x\">x</a>").is_some());
        assert!(unsafe_html("<img src=\"DATA:text/html,x\">").is_some());
        assert!(unsafe_html("<p onclick=\"x\">x</p>").is_some());
        assert!(unsafe_html("<p><script>x</script></p>").is_some());
        assert!(unsafe_html("<span style=\"background:url(x)\">x</span>").is_some());
        assert!(unsafe_html("<img srcset=\"a.png 1x, javascript:x 2x\">").is_some());
        assert!(unsafe_html("<a href=\"https://example.com/javascript:x\">x</a>").is_none());
        assert!(unsafe_html("<a href=\"./javascript:x\">x</a>").is_none());
        assert!(unsafe_html("<script type=\"application/json\">{}</script>").is_none());
        let image = "<img src=\"data:image/svg+xml,%3Csvg%20xmlns%3D%22http%3A%2F%2Fwww.w3.org%2F2000%2Fsvg%22%3E%3Crect%2F%3E%3C%2Fsvg%3E\">";
        assert!(unsafe_html(image).is_none());
        let handler = image.replace("<img ", "<img onerror=\"x\" ");
        assert!(unsafe_html(&handler).is_some());
        assert!(unsafe_html("<a ping=\"https://ok.test javascript:x\">x</a>").is_some());
        assert!(
            unsafe_html(
                "<img src=\"data:image/svg+xml,%3Csvg%3E%3Cscript%3Ex%3C%2Fscript%3E%3C%2Fsvg%3E\">"
            )
            .is_some()
        );
        assert!(unsafe_html("<img src=\"data:image/svg+xml,%3Csvg%3E%3Cimage%20href%3D%22https%3A%2F%2Fx%22%2F%3E%3C%2Fsvg%3E\">").is_some());
    }
}
