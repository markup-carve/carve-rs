//! Port of carve-js `test/svg-sanitize-payloads.test.ts`: a curated corpus of
//! known SVG-based XSS / resource-fetch vectors (PortSwigger SVG cheatsheet,
//! cure53 mXSS research, OWASP SVG payloads).
//!
//! Each payload is fed through [`sanitize_svg`] and the OUTPUT is asserted inert
//! (no active markup, event handlers, dangerous schemes, or external refs
//! survive), under both default and maximally-permissive opts. The exact
//! carve-js output for every payload is also pinned, so the two engines agree
//! byte-for-byte on both the accept/reject decision and the sanitized result.

use carve::{sanitize_svg, SanitizeSvgOptions};

const PAYLOADS: &[&str] = &[
    // -- script / event handlers --
    r##"<svg onload="alert(1)"><rect/></svg>"##,
    r##"<svg><script>alert(1)</script></svg>"##,
    r##"<svg><script href="data:,alert(1)"/></svg>"##,
    r##"<svg><script xlink:href="data:,alert(1)"/></svg>"##,
    r##"<svg><rect onclick="alert(1)" onmouseover="alert(1)" width="1" height="1"/></svg>"##,
    r##"<svg><rect fill=a onload=alert(1) width=1 height=1></rect></svg>"##,
    r##"<svg><g onfocus="alert(1)" tabindex="1"><rect/></g></svg>"##,
    // -- javascript: / dangerous schemes on links --
    r##"<svg><a xlink:href="javascript:alert(1)"><text>x</text></a></svg>"##,
    r##"<svg><a href="javascript:alert(1)"><rect width="1" height="1"/></a></svg>"##,
    r##"<svg><a href="ms-msdt:x"><rect width="1" height="1"/></a></svg>"##,
    r##"<svg><a href="vbscript:msgbox(1)"><rect width="1" height="1"/></a></svg>"##,
    // -- entity / escape obfuscated schemes --
    r##"<svg><a href="jav&#x61;script:alert(1)"><rect width="1" height="1"/></a></svg>"##,
    r##"<svg><a href="javascript&colon;alert(1)"><rect width="1" height="1"/></a></svg>"##,
    r##"<svg><a href="&#106;avascript:alert(1)"><rect width="1" height="1"/></a></svg>"##,
    // -- SMIL animation retargeting --
    r##"<svg><a id="x"><rect width="1" height="1"/></a><animate xlink:href="#x" attributeName="href" values="javascript:alert(1)"/></svg>"##,
    r##"<svg><set attributeName="href" to="javascript:alert(1)"/></svg>"##,
    r##"<svg><animate attributeName="href" values="#a;https://evil.example/x#b"/></svg>"##,
    r##"<svg><animate attributeName="href" values="#a;//evil.example/x#b"/></svg>"##,
    r##"<svg><discard begin="0s" href="javascript:alert(1)"/></svg>"##,
    // -- foreignObject / embedded HTML --
    r##"<svg><foreignObject><iframe src="javascript:alert(1)"></iframe></foreignObject></svg>"##,
    r##"<svg><foreignObject><img src=x onerror=alert(1)></foreignObject></svg>"##,
    r##"<svg><foreignObject><body onload="alert(1)"/></foreignObject></svg>"##,
    // -- external resource fetches --
    r##"<svg><use href="https://evil.example/x.svg#a"/></svg>"##,
    r##"<svg><use xlink:href="//evil.example/x.svg#a"/></svg>"##,
    r##"<svg><image href="https://evil.example/x.png" width="1" height="1"/></svg>"##,
    r##"<svg><feImage href="https://evil.example/x.png"/></svg>"##,
    r##"<svg><rect fill="url(https://evil.example/p.svg#x)" width="1" height="1"/></svg>"##,
    r##"<svg><rect filter="url(https://evil.example/f.svg#x)" width="1" height="1"/></svg>"##,
    r##"<svg><rect fill='url("https://evil.example/a)b.svg#x")' width='1' height='1'/></svg>"##,
    r##"<svg><rect clip-path="url(//evil.example/c)" width="1" height="1"/></svg>"##,
    // -- style element / attribute --
    r##"<svg><style>@import url('https://evil.example/x.css');</style><rect/></svg>"##,
    r##"<svg><style>* { background: url(javascript:alert(1)) }</style><rect/></svg>"##,
    r##"<svg><rect style="background:url(javascript:alert(1))" width="1" height="1"/></svg>"##,
    r##"<svg><rect style="fill:u\72l(https://evil.example/x)" width="1" height="1"/></svg>"##,
    // -- handler / listener elements --
    r##"<svg><handler xmlns:ev="http://www.w3.org/2001/xml-events" ev:event="load">alert(1)</handler></svg>"##,
    r##"<svg><listener event="load" handler="#h"/><rect/></svg>"##,
    // -- comments / CDATA / PI / doctype tricks --
    r##"<svg><!--<script>alert(1)</script>--><rect width="1" height="1"/></svg>"##,
    r##"<svg><![CDATA[<script>alert(1)</script>]]><rect width="1" height="1"/></svg>"##,
    r##"<?xml-stylesheet type="text/xsl" href="javascript:alert(1)"?><svg><rect/></svg>"##,
    r##"<!DOCTYPE svg [<!ENTITY x "y">]><svg><rect width="1" height="1"/></svg>"##,
    // -- mutation-ish reparse candidates --
    r##"<svg><title><style><img src=1 onerror=alert(1)></style></title><rect width="1" height="1"/></svg>"##,
    r##"<svg><desc><![CDATA[</desc><script>alert(1)</script>]]></desc><rect width="1" height="1"/></svg>"##,
    r##"<svg><![CDATA[]><svg onload=alert(1)>]]><rect width="1" height="1"/></svg>"##,
];

/// The exact carve-js `sanitizeSvg(p).svg` for each payload under DEFAULT opts.
/// `""` marks `ok: false` (the caller then shows source).
const DEF: &[&str] = &[
    "<svg xmlns=\"http://www.w3.org/2000/svg\"><rect/></svg>","<svg xmlns=\"http://www.w3.org/2000/svg\"></svg>","<svg xmlns=\"http://www.w3.org/2000/svg\"></svg>","<svg xmlns=\"http://www.w3.org/2000/svg\"></svg>","<svg xmlns=\"http://www.w3.org/2000/svg\"><rect width=\"1\" height=\"1\"/></svg>","<svg xmlns=\"http://www.w3.org/2000/svg\"><rect fill=\"a\" width=\"1\" height=\"1\"></rect></svg>","<svg xmlns=\"http://www.w3.org/2000/svg\"><g><rect/></g></svg>","<svg xmlns=\"http://www.w3.org/2000/svg\"></svg>","<svg xmlns=\"http://www.w3.org/2000/svg\"></svg>","<svg xmlns=\"http://www.w3.org/2000/svg\"></svg>","<svg xmlns=\"http://www.w3.org/2000/svg\"></svg>","<svg xmlns=\"http://www.w3.org/2000/svg\"></svg>","<svg xmlns=\"http://www.w3.org/2000/svg\"></svg>","<svg xmlns=\"http://www.w3.org/2000/svg\"></svg>","<svg xmlns=\"http://www.w3.org/2000/svg\"></svg>","<svg xmlns=\"http://www.w3.org/2000/svg\"></svg>","<svg xmlns=\"http://www.w3.org/2000/svg\"></svg>","<svg xmlns=\"http://www.w3.org/2000/svg\"></svg>","<svg xmlns=\"http://www.w3.org/2000/svg\"></svg>","<svg xmlns=\"http://www.w3.org/2000/svg\"></svg>","","<svg xmlns=\"http://www.w3.org/2000/svg\"></svg>","<svg xmlns=\"http://www.w3.org/2000/svg\"><use/></svg>","<svg xmlns=\"http://www.w3.org/2000/svg\"><use/></svg>","<svg xmlns=\"http://www.w3.org/2000/svg\"></svg>","<svg xmlns=\"http://www.w3.org/2000/svg\"><feImage/></svg>","<svg xmlns=\"http://www.w3.org/2000/svg\"><rect width=\"1\" height=\"1\"/></svg>","<svg xmlns=\"http://www.w3.org/2000/svg\"><rect width=\"1\" height=\"1\"/></svg>","<svg xmlns=\"http://www.w3.org/2000/svg\"><rect width=\"1\" height=\"1\"/></svg>","<svg xmlns=\"http://www.w3.org/2000/svg\"><rect width=\"1\" height=\"1\"/></svg>","<svg xmlns=\"http://www.w3.org/2000/svg\"><rect/></svg>","<svg xmlns=\"http://www.w3.org/2000/svg\"><rect/></svg>","<svg xmlns=\"http://www.w3.org/2000/svg\"><rect width=\"1\" height=\"1\"/></svg>","<svg xmlns=\"http://www.w3.org/2000/svg\"><rect width=\"1\" height=\"1\"/></svg>","<svg xmlns=\"http://www.w3.org/2000/svg\"></svg>","<svg xmlns=\"http://www.w3.org/2000/svg\"><rect/></svg>","<svg xmlns=\"http://www.w3.org/2000/svg\"><rect width=\"1\" height=\"1\"/></svg>","<svg xmlns=\"http://www.w3.org/2000/svg\"><rect width=\"1\" height=\"1\"/></svg>","<svg xmlns=\"http://www.w3.org/2000/svg\"><rect/></svg>","","","<svg xmlns=\"http://www.w3.org/2000/svg\"><desc></desc><rect width=\"1\" height=\"1\"/></svg>","<svg xmlns=\"http://www.w3.org/2000/svg\"><rect width=\"1\" height=\"1\"/></svg>",
];

/// carve-js output under `{ allow_style, allow_links, allow_animation }`.
const ALL: &[&str] = &[
    "<svg xmlns=\"http://www.w3.org/2000/svg\"><rect/></svg>","<svg xmlns=\"http://www.w3.org/2000/svg\"></svg>","<svg xmlns=\"http://www.w3.org/2000/svg\"></svg>","<svg xmlns=\"http://www.w3.org/2000/svg\"></svg>","<svg xmlns=\"http://www.w3.org/2000/svg\"><rect width=\"1\" height=\"1\"/></svg>","<svg xmlns=\"http://www.w3.org/2000/svg\"><rect fill=\"a\" width=\"1\" height=\"1\"></rect></svg>","<svg xmlns=\"http://www.w3.org/2000/svg\"><g><rect/></g></svg>","<svg xmlns=\"http://www.w3.org/2000/svg\"><a><text>x</text></a></svg>","<svg xmlns=\"http://www.w3.org/2000/svg\"><a><rect width=\"1\" height=\"1\"/></a></svg>","<svg xmlns=\"http://www.w3.org/2000/svg\"><a><rect width=\"1\" height=\"1\"/></a></svg>","<svg xmlns=\"http://www.w3.org/2000/svg\"><a><rect width=\"1\" height=\"1\"/></a></svg>","<svg xmlns=\"http://www.w3.org/2000/svg\"><a><rect width=\"1\" height=\"1\"/></a></svg>","<svg xmlns=\"http://www.w3.org/2000/svg\"><a><rect width=\"1\" height=\"1\"/></a></svg>","<svg xmlns=\"http://www.w3.org/2000/svg\"><a><rect width=\"1\" height=\"1\"/></a></svg>","<svg xmlns=\"http://www.w3.org/2000/svg\"><a id=\"x\"><rect width=\"1\" height=\"1\"/></a><animate xlink:href=\"#x\" attributeName=\"href\"/></svg>","<svg xmlns=\"http://www.w3.org/2000/svg\"><set attributeName=\"href\"/></svg>","<svg xmlns=\"http://www.w3.org/2000/svg\"><animate attributeName=\"href\"/></svg>","<svg xmlns=\"http://www.w3.org/2000/svg\"><animate attributeName=\"href\"/></svg>","<svg xmlns=\"http://www.w3.org/2000/svg\"></svg>","<svg xmlns=\"http://www.w3.org/2000/svg\"></svg>","","<svg xmlns=\"http://www.w3.org/2000/svg\"></svg>","<svg xmlns=\"http://www.w3.org/2000/svg\"><use/></svg>","<svg xmlns=\"http://www.w3.org/2000/svg\"><use/></svg>","<svg xmlns=\"http://www.w3.org/2000/svg\"></svg>","<svg xmlns=\"http://www.w3.org/2000/svg\"><feImage/></svg>","<svg xmlns=\"http://www.w3.org/2000/svg\"><rect width=\"1\" height=\"1\"/></svg>","<svg xmlns=\"http://www.w3.org/2000/svg\"><rect width=\"1\" height=\"1\"/></svg>","<svg xmlns=\"http://www.w3.org/2000/svg\"><rect width=\"1\" height=\"1\"/></svg>","<svg xmlns=\"http://www.w3.org/2000/svg\"><rect width=\"1\" height=\"1\"/></svg>","<svg xmlns=\"http://www.w3.org/2000/svg\"><rect/></svg>","<svg xmlns=\"http://www.w3.org/2000/svg\"><rect/></svg>","<svg xmlns=\"http://www.w3.org/2000/svg\"><rect width=\"1\" height=\"1\"/></svg>","<svg xmlns=\"http://www.w3.org/2000/svg\"><rect width=\"1\" height=\"1\"/></svg>","<svg xmlns=\"http://www.w3.org/2000/svg\"></svg>","<svg xmlns=\"http://www.w3.org/2000/svg\"><rect/></svg>","<svg xmlns=\"http://www.w3.org/2000/svg\"><rect width=\"1\" height=\"1\"/></svg>","<svg xmlns=\"http://www.w3.org/2000/svg\"><rect width=\"1\" height=\"1\"/></svg>","<svg xmlns=\"http://www.w3.org/2000/svg\"><rect/></svg>","","","<svg xmlns=\"http://www.w3.org/2000/svg\"><desc></desc><rect width=\"1\" height=\"1\"/></svg>","<svg xmlns=\"http://www.w3.org/2000/svg\"><rect width=\"1\" height=\"1\"/></svg>",
];

fn all_on() -> SanitizeSvgOptions {
    SanitizeSvgOptions {
        allow_style: true,
        allow_links: true,
        allow_animation: true,
        allow_external_images: false,
    }
}

/// Strip `xmlns` / `xmlns:foo` declarations - the forced canonical namespace and
/// the xlink decl legitimately contain a w3.org http URL that is not a fetch.
fn strip_ns(out: &str) -> String {
    let chars: Vec<char> = out.chars().collect();
    let n = chars.len();
    let mut res = String::new();
    let mut i = 0;
    while i < n {
        if let Some(end) = ns_decl_at(&chars, i) {
            i = end;
        } else {
            res.push(chars[i]);
            i += 1;
        }
    }
    res
}

/// Match `xmlns(:word)?\s*=\s*"[^"]*"` at `start`, returning the end index.
fn ns_decl_at(chars: &[char], start: usize) -> Option<usize> {
    let n = chars.len();
    let mut i = start;
    for w in ['x', 'm', 'l', 'n', 's'] {
        if i >= n || chars[i] != w {
            return None;
        }
        i += 1;
    }
    if i < n && chars[i] == ':' {
        i += 1;
        let s = i;
        while i < n && (chars[i].is_ascii_alphanumeric() || chars[i] == '-') {
            i += 1;
        }
        if i == s {
            return None;
        }
    }
    while i < n && chars[i].is_whitespace() {
        i += 1;
    }
    if i >= n || chars[i] != '=' {
        return None;
    }
    i += 1;
    while i < n && chars[i].is_whitespace() {
        i += 1;
    }
    if i >= n || chars[i] != '"' {
        return None;
    }
    i += 1;
    while i < n && chars[i] != '"' {
        i += 1;
    }
    if i >= n {
        return None;
    }
    Some(i + 1)
}

/// Assert the sanitized output carries nothing executable or externally
/// fetching (port of the carve-js `assertInert`).
fn assert_inert(raw_out: &str) {
    let out = strip_ns(raw_out);
    let lower = out.to_ascii_lowercase();
    assert!(!contains_tag(&lower, "script"), "script survived: {out}");
    assert!(!lower.contains("<foreignobject"), "foreignObject: {out}");
    assert!(!contains_word_tag(&lower, "handler"), "handler: {out}");
    assert!(!lower.contains("<iframe"), "iframe: {out}");
    assert!(!has_event_handler_attr(&out), "event handler attr: {out}");
    assert!(!lower.contains("javascript:"), "javascript: {out}");
    assert!(!lower.contains("vbscript:"), "vbscript: {out}");
    assert!(!lower.contains("ms-msdt:"), "ms-msdt: {out}");
    assert!(
        !lower.contains("http://") && !lower.contains("https://"),
        "external URL: {out}"
    );
    assert!(
        !has_protocol_relative_url(&lower),
        "protocol-relative url(): {out}"
    );
    assert!(!out.contains("evil.example"), "evil.example: {out}");
    assert!(!lower.contains("@import"), "@import: {out}");
    assert!(!has_nonlocal_url(&lower), "non-local url(): {out}");
}

/// `<script[\s/>]` (case-insensitive on an already-lowercased haystack).
fn contains_tag(lower: &str, tag: &str) -> bool {
    let needle = format!("<{tag}");
    let b = lower.as_bytes();
    let mut from = 0;
    while let Some(p) = lower[from..].find(&needle).map(|x| from + x) {
        let after = p + needle.len();
        match b.get(after) {
            Some(c) if c.is_ascii_whitespace() || *c == b'/' || *c == b'>' => return true,
            _ => {}
        }
        from = p + 1;
    }
    false
}

/// `<handler\b`.
fn contains_word_tag(lower: &str, tag: &str) -> bool {
    let needle = format!("<{tag}");
    let b = lower.as_bytes();
    let mut from = 0;
    while let Some(p) = lower[from..].find(&needle).map(|x| from + x) {
        let after = p + needle.len();
        match b.get(after) {
            Some(c) if c.is_ascii_alphanumeric() || *c == b'_' => {}
            _ => return true,
        }
        from = p + 1;
    }
    false
}

/// `\son[a-z]+\s*=` - an event-handler attribute.
fn has_event_handler_attr(out: &str) -> bool {
    let b = out.as_bytes();
    let n = b.len();
    let mut i = 0;
    while i + 3 < n {
        if b[i].is_ascii_whitespace()
            && b[i + 1].eq_ignore_ascii_case(&b'o')
            && b[i + 2].eq_ignore_ascii_case(&b'n')
        {
            let mut j = i + 3;
            let start = j;
            while j < n && b[j].is_ascii_lowercase() {
                j += 1;
            }
            if j > start {
                let mut k = j;
                while k < n && b[k].is_ascii_whitespace() {
                    k += 1;
                }
                if k < n && b[k] == b'=' {
                    return true;
                }
            }
        }
        i += 1;
    }
    false
}

/// `url(\s*['"]?\s*//` - a protocol-relative reference.
fn has_protocol_relative_url(lower: &str) -> bool {
    scan_url_open(lower, |rest| rest.starts_with("//"))
}

/// `url(\s*['"]?\s*(?!#)` - a non-local reference (anything not starting `#`).
fn has_nonlocal_url(lower: &str) -> bool {
    scan_url_open(lower, |rest| !rest.starts_with('#'))
}

fn scan_url_open(lower: &str, pred: impl Fn(&str) -> bool) -> bool {
    let chars: Vec<char> = lower.chars().collect();
    let n = chars.len();
    let mut i = 0;
    while i + 4 <= n {
        if chars[i] == 'u' && chars[i + 1] == 'r' && chars[i + 2] == 'l' && chars[i + 3] == '(' {
            let mut j = i + 4;
            while j < n && chars[j].is_whitespace() {
                j += 1;
            }
            if j < n && (chars[j] == '\'' || chars[j] == '"') {
                j += 1;
            }
            while j < n && chars[j].is_whitespace() {
                j += 1;
            }
            let rest: String = chars[j..].iter().collect();
            if pred(&rest) {
                return true;
            }
        }
        i += 1;
    }
    false
}

#[test]
fn corpus_is_inert_default_opts() {
    for p in PAYLOADS {
        let r = sanitize_svg(p, &SanitizeSvgOptions::default());
        if r.ok {
            assert_inert(&r.svg);
        } else {
            assert_eq!(r.svg, "");
        }
    }
}

#[test]
fn corpus_is_inert_all_capabilities_on() {
    for p in PAYLOADS {
        let r = sanitize_svg(p, &all_on());
        if r.ok {
            assert_inert(&r.svg);
        } else {
            assert_eq!(r.svg, "");
        }
    }
}

#[test]
fn corpus_idempotent_and_inert_on_double_pass() {
    for p in PAYLOADS {
        let once = sanitize_svg(p, &all_on());
        if !once.ok {
            continue;
        }
        let twice = sanitize_svg(&once.svg, &all_on());
        assert_eq!(twice.svg, once.svg, "not idempotent: {p}");
        assert_inert(&twice.svg);
    }
}

#[test]
fn corpus_matches_carve_js_bytes() {
    assert_eq!(PAYLOADS.len(), DEF.len());
    assert_eq!(PAYLOADS.len(), ALL.len());
    for (i, p) in PAYLOADS.iter().enumerate() {
        let d = sanitize_svg(p, &SanitizeSvgOptions::default());
        assert_eq!(d.svg, DEF[i], "default #{i}: {p}");
        assert_eq!(d.ok, !DEF[i].is_empty(), "default ok #{i}");
        let a = sanitize_svg(p, &all_on());
        assert_eq!(a.svg, ALL[i], "all-on #{i}: {p}");
        assert_eq!(a.ok, !ALL[i].is_empty(), "all-on ok #{i}");
    }
}
