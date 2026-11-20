//! PROVISIONAL standard `imgFence` matchers and sanitizer (C2-X-059, C2-X-060).

use serde_json::{Value, json};

fn render(body: &str) -> Value {
    let source = format!("```img\n{body}\n```\n");
    let request = json!({
        "call": "renderWithDiagnostics",
        "source": source,
        "options": {"extensions": ["carve.imgFence.img"], "renderers": {"carve.imgFence.img": "standard"}},
    });
    serde_json::from_str(&carve::api::json::call(&request.to_string())).unwrap()
}

fn svg(body: &str) -> String {
    let html = render(body)["ok"]["html"]
        .as_str()
        .unwrap_or("")
        .to_string();
    let url = html
        .strip_prefix("<img src=\"data:image/svg+xml,")
        .and_then(|s| s.strip_suffix("\" alt=\"\">"))
        .unwrap_or_else(|| panic!("no image: {html}"));
    let mut out = Vec::new();
    let b = url.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            out.push(u8::from_str_radix(&url[i + 1..i + 3], 16).unwrap());
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8(out).unwrap()
}

#[test]
fn unsafe_elements_attributes_and_references_are_removed() {
    assert_eq!(
        svg(
            r#"<svg onload="x()"><foreignObject><p>x</p></foreignObject><a href="javascript:x()"><rect/></a><image href="https://e.test/i.png"/><use href="https://e.test/s.svg#a"/><style>rect{fill:url(https://e.test)}</style><rect style="fill:red" fill="url(https://e.test/x)" stroke="url(#g)" width="2"/></svg>"#
        ),
        r#"<svg xmlns="http://www.w3.org/2000/svg"><rect stroke="url(#g)" width="2"/></svg>"#
    );
    assert_eq!(
        svg(
            r#"<svg xmlns:xlink="http://www.w3.org/1999/xlink" viewBox="0 0 1 1"><text x="0">a &lt; b</text></svg>"#
        ),
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1 1"><text x="0">a &lt; b</text></svg>"#
    );
}

#[test]
fn a_body_that_is_no_single_svg_root_keeps_the_fallback() {
    for body in [
        "<svg/><svg/>",
        "<svg/>text",
        "<svg></svg><p>x</p>",
        "<svg><rect></svg>",
        "<svg><rect width=2/></svg>",
        "<svg><!DOCTYPE x></svg>",
        "<svg><g></rect></g></svg>",
    ] {
        let out = render(body);
        let html = out["ok"]["html"].as_str().unwrap_or("");
        assert!(
            html.starts_with("<pre><code class=\"language-img\">"),
            "{body}: {html}"
        );
        assert!(
            out["ok"]["diagnostics"]
                .as_array()
                .is_some_and(|d| d.iter().any(|e| e["code"] == "extension-renderer-failed")),
            "{body}: {out}"
        );
    }
}

#[test]
fn without_the_renderer_the_fallback_is_written() {
    let request = json!({
        "call": "render",
        "source": "```img\n<svg/>\n```\n",
        "options": {"extensions": ["carve.imgFence.img"]},
    });
    let out: Value = serde_json::from_str(&carve::api::json::call(&request.to_string())).unwrap();
    assert_eq!(
        out["ok"]["html"],
        "<pre><code class=\"language-img\">&lt;svg/&gt;\n</code></pre>"
    );
}

#[test]
fn c2_x_059_authored_src_and_alt_of_any_case_leave_the_attribute_list() {
    for (attrs, alt) in [
        ("{src=\"https://x/y\" alt=\"X\"}", "X"),
        ("{ALT=\"D\"}", "D"),
        ("{Src=\"q\" Alt=\"z\" .c}", "z"),
    ] {
        let request = serde_json::json!({
            "call": "render",
            "source": format!("{attrs}\n```img\n<svg><rect/></svg>\n```\n"),
            "options": {
                "extensions": ["carve.imgFence.img"],
                "renderers": {"carve.imgFence.img": "standard"},
            },
        });
        let response: serde_json::Value =
            serde_json::from_str(&carve::api::json::call(&request.to_string())).unwrap();
        let html = response["ok"]["html"].as_str().unwrap_or_default();
        assert_eq!(html.matches("src=").count(), 1, "{html}");
        assert_eq!(
            html.to_ascii_lowercase().matches("alt=").count(),
            1,
            "{html}"
        );
        assert!(html.contains(&format!("alt=\"{alt}\"")), "{html}");
    }
}
