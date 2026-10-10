//! Bare same-kind openers remain literal; explicit braced openers nest.

/// Both routes: `to_html`, which may take the layout fast path, and a full parse.
fn html(source: &str) -> String {
    let parsed = carve::render_html(&carve::parse(source)).unwrap();
    assert_eq!(carve::to_html(source), parsed, "the two routes disagree");
    parsed.trim_end().to_string()
}

macro_rules! cases {
    ($($name:ident: $source:expr => $expected:expr,)*) => {
        $(
            mod $name {
                #[test]
                fn reads_as_ruled() {
                    assert_eq!(super::html($source), $expected);
                }

                #[test]
                fn formats_to_the_same_document() {
                    let written = carve::to_carve(&format!("{}\n", $source));
                    assert_eq!(super::html(&written), $expected, "{written}");
                    assert_eq!(carve::to_carve(&written), written);
                }
            }
        )*
    };
}

cases! {
    a_forced_opener_inside_a_bare_span: "*a {*b*} c*" => "<p><strong>a <strong>b</strong> c</strong></p>",
    a_bare_opener_inside_a_forced_span: "{*a *b* c*}" => "<p><strong>a *b* c</strong></p>",
    a_forced_opener_inside_a_forced_span: "a{*{*x*}*}b" => "<p>a<strong><strong>x</strong></strong>b</p>",
    a_forced_pair_the_outer_scan_runs_past: "*a {*b *} c*" => "<p><strong>a <strong>b </strong> c</strong></p>",
    a_forced_opener_through_a_bare_span: "{/a *b {/c/}*/}" => "<p><em>a <strong>b <em>c</em></strong></em></p>",
    a_braced_scope_nests_its_outer_kind: "{*a {/b {*c*} d/} e*}"
        => "<p><strong>a <em>b <strong>c</strong> d</em> e</strong></p>",
    a_closer_inside_a_braced_scope_stays_there: "{*a {/b *} d/} e*}"
        => "<p><strong>a <em>b *} d</em> e</strong></p>",
    a_bare_outer_span_around_a_braced_scope: "*a {/b *c* d/} e*"
        => "<p><strong>a <em>b <strong>c</strong> d</em> e</strong></p>",
    a_bare_span_keeps_the_outer_kind_open: "{*a /b *c* d/ e*}" => "<p><strong>a <em>b *c* d</em> e</strong></p>",
    an_insertion_scope:"{+a {/b {+c+} d/} e+}" => "<p><ins>a <em>b <ins>c</ins> d</em> e</ins></p>",
}

// Controls: nothing of the same kind is open.
cases! {
    a_span_of_another_kind: "{*a /b/ c*}" => "<p><strong>a <em>b</em> c</strong></p>",
    a_forced_span_of_another_kind: "*a {/b/} c*" => "<p><strong>a <em>b</em> c</strong></p>",
    a_bold_italic: "/*x*/" => "<p><strong><em>x</em></strong></p>",
    a_substitution_inside_a_strike: "~a {~b~>c~} d~" => "<p><s>a <del>b</del><ins>c</ins> d</s></p>",
    sibling_spans_of_one_kind: "a {*b*} c {*d*} e" => "<p>a <strong>b</strong> c <strong>d</strong> e</p>",
}

/// Explicit braces preserve direct same-kind nesting.
#[test]
fn the_writer_refuses_a_direct_same_kind_nesting() {
    let doc = carve::html_to_ast(
        "<p><strong>a <strong>b</strong> c</strong></p>",
        &carve::HtmlImportOptions::default(),
    )
    .unwrap()
    .value;
    let source = carve::render_carve(&doc).unwrap();
    assert_eq!(html(&source), "<p><strong>a <strong>b</strong> c</strong></p>");
}

/// Through a span of another kind the writer braces that span to scope it.
#[test]
fn the_writer_braces_the_scope_between() {
    let doc = carve::html_to_ast(
        "<p><strong>a <em>b <strong>c</strong> d</em> e</strong></p>",
        &carve::HtmlImportOptions::default(),
    )
    .unwrap()
    .value;
    assert_eq!(carve::render_carve(&doc).unwrap(), "{*a /b {*c*} d/ e*}\n");
}
