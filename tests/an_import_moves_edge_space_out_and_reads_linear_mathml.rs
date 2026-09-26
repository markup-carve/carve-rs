//! markup-carve/carve#2361: a link's or span's edge whitespace stands outside
//! it, presentation-only MathML imports as its text where its tokens are
//! linear, and a formula beside its fallback image imports once.

use carve::{html_to_carve, HtmlImportMode, HtmlImportOptions};

fn import(html: &str) -> (String, Vec<(String, String)>) {
    import_in(html, HtmlImportMode::Safe)
}

fn import_in(html: &str, mode: HtmlImportMode) -> (String, Vec<(String, String)>) {
    let result = html_to_carve(
        html,
        &HtmlImportOptions {
            mode,
            ..Default::default()
        },
    )
    .unwrap();
    let rows = result
        .report
        .diagnostics
        .iter()
        .map(|d| {
            (
                d.code.as_str().to_string(),
                d.path.clone().unwrap_or_default(),
            )
        })
        .collect();
    (result.value, rows)
}

#[test]
fn edge_whitespace_moves_outside_a_link_or_span() {
    for (html, carve) in [
        (
            r#"<p>Source: <a href="https://jma.go.jp/"> Japan Meteorological Agency </a>.</p>"#,
            "Source: [Japan Meteorological Agency](https://jma.go.jp/) .\n",
        ),
        (r#"<p>a <a href="/x"> x</a></p>"#, "a [x](/x)\n"),
        (r#"<p>x<a href="/y"> y</a>z</p>"#, "x [y](/y)z\n"),
        (r#"<p><a href="/s"> start</a></p>"#, "[start](/s)\n"),
        (
            r#"<p>a <a href="/b"> <b>bold</b> </a> b</p>"#,
            "a [*bold*](/b) b\n",
        ),
        (
            r#"<p>b<a href="/i"> <img src="i.png" alt="i"> </a>c</p>"#,
            "b [![i](i.png)](/i) c\n",
        ),
        (r#"<p>a<span id="k"> key </span>b</p>"#, "a [key]{#k} b\n"),
        (
            r#"<p>a<a href="/n"><span class="c"> n </span></a>b</p>"#,
            "a [[n]{.c}](/n) b\n",
        ),
    ] {
        let (value, rows) = import(html);
        assert_eq!(value, carve, "{html}");
        assert!(rows.is_empty(), "{html}: {rows:?}");
    }
}

#[test]
fn whitespace_only_content_a_no_break_space_and_a_strong_stay() {
    for (html, carve) in [
        (r#"<p>a <a href="/w"> </a> b</p>"#, "a [ ](/w) b\n"),
        (
            "<p>a <a href=\"/n\">\u{a0}nb\u{a0}</a> b</p>",
            "a [\u{a0}nb\u{a0}](/n) b\n",
        ),
        (
            r#"<p>a <a href="/s"><b> x </b></a> b</p>"#,
            "a [{* x *}](/s) b\n",
        ),
    ] {
        assert_eq!(import(html).0, carve, "{html}");
    }
}

#[test]
fn a_linear_token_run_imports_as_its_text() {
    let (value, rows) = import(
        "<p>A <math><mi>a</mi><mo>+</mo><mi>a</mi><mo>=</mo><mn>2</mn><mi>a</mi></math> B</p>",
    );
    assert_eq!(value, "A a+a=2a B\n");
    assert_eq!(
        rows,
        [("element-unwrapped".to_string(), "/p[1]/math[2]".to_string())]
    );

    let html = "<p><math><semantics><mrow><mstyle><mi mathvariant=\"normal\">∀</mi><mi>x</mi><mo>∈</mo><mi>X</mi>\
        <mo>,</mo><mspace width=\"1em\"></mspace><mi>y</mi><mtext> if  y </mtext></mstyle></mrow>\
        <annotation encoding=\"text/plain\">not this</annotation></semantics></math></p>";
    assert_eq!(import(html).0, "∀x∈X,yif y\n");
}

#[test]
fn layout_keeps_the_drop() {
    for inner in [
        "<mfrac><mn>1</mn><mn>2</mn></mfrac>",
        "<msup><mi>x</mi><mn>2</mn></msup>",
        "<mphantom><mi>x</mi></mphantom>",
        "<mi><mglyph></mglyph></mi>",
    ] {
        let (value, rows) = import(&format!("<p>v <math>{inner}</math></p>"));
        assert_eq!(value, "v\n", "{inner}");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].0, "element-dropped");
    }
    let (_, rows) = import_in("<p><math><mi>a</mi></math></p>", HtmlImportMode::Roundtrip);
    assert_eq!(
        rows.iter().map(|r| r.0.as_str()).collect::<Vec<_>>(),
        ["raw-preserved"]
    );
}

#[test]
fn a_formula_beside_its_fallback_image_imports_once() {
    let (value, rows) = import(
        "<p>I <span class=\"w\"><span class=\"m\"><math><semantics><mi>a</mi>\
         <annotation encoding=\"application/x-tex\">a^2</annotation></semantics></math></span>\
         <img src=\"f.svg\" alt=\"a^2\"></span> x</p>",
    );
    assert_eq!(value, "I [[$`a^2`]{.m}]{.w} x\n");
    assert_eq!(
        rows,
        [(
            "element-dropped".to_string(),
            "/p[1]/span[2]/img[2]".to_string()
        )]
    );

    let (value, rows) = import(
        "<p>Or <span class=\"m\" style=\"display: none\"><math><mi>b</mi></math></span><img src=\"g.svg\" alt=\"b^2\"> there.</p>",
    );
    assert_eq!(value, "Or [$`b^2`]{.m} there.\n");
    assert_eq!(
        rows.iter().map(|r| r.0.as_str()).collect::<Vec<_>>(),
        ["style-unmapped", "encoding-assumed", "element-dropped"]
    );
    assert_eq!(
        import("<p><math style=\"DISPLAY:none !important\"><mi>b</mi></math><img src=\"g.svg\" alt=\"b^2\"></p>").0,
        "$`b^2`\n"
    );
}

#[test]
fn an_image_that_is_not_the_fallback_stays() {
    assert_eq!(
        import(r#"<p><math><mi>x</mi></math><img src="portrait.png" alt="Portrait of Ada"></p>"#).0,
        "x![Portrait of Ada](portrait.png)\n"
    );
    assert_eq!(
        import(r#"<p>N <math alttext="x"></math> <img src="i.png" alt="icon"> one.</p>"#).0,
        "N $`x` ![icon](i.png) one.\n"
    );
    assert!(
        import(r#"<div><p><math alttext="y"></math></p><img src="j.png" alt="y"></div>"#)
            .0
            .contains("![y](j.png)")
    );
}

#[test]
fn the_effective_display_decides_whether_the_math_is_hidden() {
    let portrait = r#"<img src="portrait.png" alt="Portrait">"#;
    assert_eq!(
        import(&format!(
            r#"<p><math style="display:none;display:block"><mi>x</mi></math>{portrait}</p>"#
        ))
        .0,
        "x![Portrait](portrait.png)\n"
    );
    assert_eq!(
        import(&format!(r#"<p><math style="display:none !important;display:block"><mi>x</mi></math>{portrait}</p>"#)).0,
        "$`Portrait`\n"
    );
}
