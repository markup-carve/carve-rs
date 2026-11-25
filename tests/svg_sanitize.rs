//! PROVISIONAL public `sanitize_svg` (carve#6754): byte parity with carve-php
//! `SvgSanitizer` (7f531fa), which carries the fixes of carve#6755,
//! carve#6760, carve#6761, carve#6763, carve#6764, carve#6765, carve#6773 and
//! carve#6774.

use carve::api::UrlPolicy;
use carve::api::extensions::{SanitizeResult, SanitizeSvgOptions, sanitize_svg};
use serde_json::Value;

fn options(value: &Value) -> SanitizeSvgOptions {
    let flag = |name: &str| value[name].as_bool().unwrap_or(false);
    let list = |v: &Value| -> Vec<String> {
        v.as_array()
            .unwrap()
            .iter()
            .map(|s| s.as_str().unwrap().to_string())
            .collect()
    };
    let policy = &value["urlPolicy"];
    SanitizeSvgOptions {
        allow_style: flag("allowStyle"),
        allow_links: flag("allowLinks"),
        allow_animation: flag("allowAnimation"),
        allow_external_images: flag("allowExternalImages"),
        url_policy: if policy.get("allow").is_some() {
            Some(UrlPolicy::Allow(list(&policy["allow"])))
        } else if policy.get("deny").is_some() {
            Some(UrlPolicy::Deny(list(&policy["deny"])))
        } else {
            None
        },
    }
}

fn sanitize(source: &str) -> SanitizeResult {
    sanitize_svg(source, &SanitizeSvgOptions::default())
}

fn kept(source: &str, opts: &SanitizeSvgOptions) -> String {
    sanitize_svg(source, opts).svg
}

#[test]
fn output_equals_carve_php_on_its_parity_rows() {
    let mut rows: Vec<Value> =
        serde_json::from_str(include_str!("data/svg-sanitize/parity.json")).unwrap();
    assert_eq!(rows.len(), 770);
    // The rows a js-first ruling changed (carve#6763, #6765, #6774), as
    // carve-php f23005e8 and 7f531fa writes them.
    let deltas: Vec<Value> =
        serde_json::from_str(include_str!("data/svg-sanitize/parity-js-first.json")).unwrap();
    assert_eq!(deltas.len(), 128);
    for delta in &deltas {
        let row = &mut rows[usize::try_from(delta["index"].as_u64().unwrap()).unwrap()];
        assert_eq!(row["src"], delta["src"], "delta {}", delta["index"]);
        row["svg"] = delta["svg"].clone();
        row["ok"] = delta["ok"].clone();
    }
    let mut failures = Vec::new();
    for (i, row) in rows.iter().enumerate() {
        let source = row["src"].as_str().unwrap();
        let got = sanitize_svg(source, &options(&row["opts"]));
        let want_svg = row["svg"].as_str().unwrap();
        let want_ok = row["ok"].as_bool().unwrap();
        if got.svg != want_svg || got.ok != want_ok {
            failures.push(format!(
                "{} {i}: {source:?}\n  want {want_ok} {want_svg:?}\n  got  {} {:?}",
                row["from"], got.ok, got.svg
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} rows differ:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn script_is_dropped_with_its_subtree() {
    assert_eq!(
        sanitize("<svg><script>alert(1)</script><circle r=\"5\"/></svg>"),
        SanitizeResult {
            svg: "<svg xmlns=\"http://www.w3.org/2000/svg\"><circle r=\"5\"/></svg>".into(),
            ok: true,
        }
    );
}

#[test]
fn root_xmlns_inside_a_value_is_kept() {
    // carve#6755: the carve-js regex cut the quote here and turned the rect's
    // id text into an onload attribute.
    assert_eq!(
        sanitize("<svg id=\"a xmlns=b\"><rect id=\"x onload=alert(1) y\"/></svg>").svg,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" id=\"a xmlns=b\"><rect id=\"x onload=alert(1) y\"/></svg>"
    );
}

#[test]
fn root_xmlns_attributes_are_replaced_by_name() {
    assert_eq!(
        sanitize("<svg xmlns=\"x\" XMLNS=\"y\" xmlns:xlink=\"http://www.w3.org/1999/xlink\"><rect/></svg>").svg,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:xlink=\"http://www.w3.org/1999/xlink\"><rect/></svg>"
    );
    assert_eq!(
        sanitize("<svg xmlns><rect/></svg>").svg,
        "<svg xmlns=\"http://www.w3.org/2000/svg\"><rect/></svg>"
    );
}

#[test]
fn style_with_a_css_image_function_is_dropped() {
    // carve#6760: C2-H-162 blanks these in the core renderer.
    let opts = SanitizeSvgOptions {
        allow_style: true,
        ..SanitizeSvgOptions::default()
    };
    for style in [
        "cursor:image-set(\"https://e.example/p.png\" 1x),auto",
        "background:-webkit-image-set(\"https://e.example/p.png\" 1x)",
        "background:image('https://e.example/p.png')",
        "background:src(\"https://e.example/p.png\")",
        "background:IMAGE\\2d set(\"https://e.example/p.png\" 1x)",
    ] {
        assert_eq!(
            kept(&format!("<svg style='{style}'/>"), &opts),
            "<svg xmlns=\"http://www.w3.org/2000/svg\"/>",
            "{style}"
        );
    }
    assert_eq!(
        kept("<svg style=\"fill:red;cursor:pointer\"/>", &opts),
        "<svg xmlns=\"http://www.w3.org/2000/svg\" style=\"fill:red;cursor:pointer\"/>"
    );
}

#[test]
fn an_href_animation_keeps_only_local_references() {
    // carve#6761.
    let opts = SanitizeSvgOptions {
        allow_animation: true,
        ..SanitizeSvgOptions::default()
    };
    assert_eq!(
        kept(
            "<svg><use href=\"#x\"><set attributeName=\"href\" to=\"evil.svg#x\"/></use></svg>",
            &opts
        ),
        "<svg xmlns=\"http://www.w3.org/2000/svg\"><use href=\"#x\"><set attributeName=\"href\"/></use></svg>"
    );
    assert_eq!(
        kept(
            "<svg><use href=\"#x\"><animate attributeName=\"xlink:href\" values=\"#a;evil.svg#x\" dur=\"1s\"/></use></svg>",
            &opts
        ),
        "<svg xmlns=\"http://www.w3.org/2000/svg\"><use href=\"#x\"><animate attributeName=\"xlink:href\" dur=\"1s\"/></use></svg>"
    );
    let local = "<svg xmlns=\"http://www.w3.org/2000/svg\"><use href=\"#x\"><animate attributeName=\"HREF\" values=\"#a;#b;\"/></use></svg>";
    assert_eq!(kept(local, &opts), local);
}

#[test]
fn url_policy_allow_list_limits_link_schemes() {
    let opts = SanitizeSvgOptions {
        allow_links: true,
        url_policy: Some(UrlPolicy::Allow(vec!["HTTPS".into()])),
        ..SanitizeSvgOptions::default()
    };
    assert_eq!(
        kept(
            "<svg><a href=\"https://example.com/\"><text>x</text></a><a href=\"mailto:a@b.c\"><text>y</text></a></svg>",
            &opts
        ),
        "<svg xmlns=\"http://www.w3.org/2000/svg\"><a href=\"https://example.com/\"><text>x</text></a><a><text>y</text></a></svg>"
    );
}

#[test]
fn malformed_input_is_refused() {
    for source in [
        "",
        "x<svg/>",
        "<svg><rect></svg>",
        "<svg/><svg/>",
        "<SVG/>",
        "<svg><g>",
        "<svg a=\"b>",
    ] {
        assert_eq!(
            sanitize(source),
            SanitizeResult {
                svg: String::new(),
                ok: false
            },
            "{source}"
        );
    }
}

#[test]
fn quoted_and_spaced_local_url_references_are_kept() {
    // carve#6763.
    assert_eq!(
        sanitize("<svg><rect fill=\"url('#g')\"/><rect fill='url(\"#g\")'/><rect fill=\"url( #g)\"/></svg>").svg,
        "<svg xmlns=\"http://www.w3.org/2000/svg\"><rect fill=\"url('#g')\"/><rect fill=\"url(&quot;#g&quot;)\"/><rect fill=\"url( #g)\"/></svg>"
    );
    assert_eq!(
        sanitize("<svg><rect fill=\"url(\t'#g')\" stroke=\"url( ' #g' )\"/></svg>").svg,
        "<svg xmlns=\"http://www.w3.org/2000/svg\"><rect fill=\"url(\t'#g')\" stroke=\"url( ' #g' )\"/></svg>"
    );
}

#[test]
fn a_non_css_space_before_the_hash_is_not_local() {
    // carve#6773: the URL keeps a leading U+00A0, U+3000 or U+FEFF.
    for space in ['\u{A0}', '\u{3000}', '\u{FEFF}'] {
        assert_eq!(
            sanitize(&format!(
                "<svg><rect fill=\"url({space}#g)\"/><rect fill=\"url('{space}#g')\"/></svg>"
            ))
            .svg,
            "<svg xmlns=\"http://www.w3.org/2000/svg\"><rect/><rect/></svg>",
            "{space:?}"
        );
    }
}

#[test]
fn output_holds_only_xml_characters() {
    // carve#6764.
    assert_eq!(
        sanitize("<svg><text>&#0;</text><text>a&#x1;b</text></svg>"),
        SanitizeResult {
            svg: "<svg xmlns=\"http://www.w3.org/2000/svg\"><text>&amp;#0;</text><text>a&amp;#x1;b</text></svg>".into(),
            ok: true,
        }
    );
    assert_eq!(
        sanitize(
            "<svg><text id=\"a&#xFFFE;\">&#55296;&#9;&#0000000065;&#x10FFFF;&#x110000;</text></svg>"
        )
        .svg,
        "<svg xmlns=\"http://www.w3.org/2000/svg\"><text id=\"a&amp;#xFFFE;\">&amp;#55296;&#9;&#0000000065;&#x10FFFF;&amp;#x110000;</text></svg>"
    );
    // Raw characters go before any check, so they cannot split a needle.
    assert_eq!(
        sanitize(
            "<svg><rect fill=\"ur\u{1}l(https://e.example/x)\"/><text>a\u{0}b\u{FFFF}c</text></svg>"
        )
        .svg,
        "<svg xmlns=\"http://www.w3.org/2000/svg\"><rect/><text>abc</text></svg>"
    );
}

#[test]
fn camel_case_animation_elements_are_kept() {
    // carve#6765.
    let opts = SanitizeSvgOptions {
        allow_animation: true,
        ..SanitizeSvgOptions::default()
    };
    let source = "<svg xmlns=\"http://www.w3.org/2000/svg\"><rect><animateTransform attributeName=\"transform\" type=\"rotate\" from=\"0\" to=\"360\" dur=\"1s\"/><animateMotion dur=\"1s\"/></rect></svg>";
    assert_eq!(kept(source, &opts), source);
    assert_eq!(
        kept(source, &SanitizeSvgOptions::default()),
        "<svg xmlns=\"http://www.w3.org/2000/svg\"><rect/></svg>"
    );
}

#[test]
fn an_attribute_without_a_value_is_dropped() {
    // carve#6774.
    assert_eq!(
        sanitize("<svg id/>"),
        SanitizeResult {
            svg: "<svg xmlns=\"http://www.w3.org/2000/svg\"/>".into(),
            ok: true,
        }
    );
    assert_eq!(
        sanitize("<svg><rect width fill/></svg>").svg,
        "<svg xmlns=\"http://www.w3.org/2000/svg\"><rect/></svg>"
    );
}
