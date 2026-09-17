//! E3 pushes no second level of a kind while one is open, and the forced
//! `{X X}` form shares the stack with the bare one, so an opener of an open kind
//! is content (markup-carve/carve#2078, markup-carve/carve-rs#1741). A braced
//! inline of another kind starts its own scope for E3 and E2
//! (markup-carve/carve#2091, markup-carve/carve-rs#1747).

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
    a_forced_opener_inside_a_bare_span: "*a {*b*} c*" => "<p><strong>a {*b</strong>} c*</p>",
    a_bare_opener_inside_a_forced_span: "{*a *b* c*}" => "<p><strong>a *b* c</strong></p>",
    a_forced_opener_inside_a_forced_span: "a{*{*x*}*}b" => "<p>a<strong>{*x</strong>*}b</p>",
    a_forced_pair_the_outer_scan_runs_past: "*a {*b *} c*" => "<p><strong>a {*b *} c</strong></p>",
    a_forced_opener_through_a_bare_span: "{/a *b {/c/}*/}" => "<p><em>a *b {/c</em>*/}</p>",
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

/// A same-kind nesting with no braced scope between has no spelling.
#[test]
fn the_writer_refuses_a_direct_same_kind_nesting() {
    let doc = carve::html_to_ast(
        "<p><strong>a <strong>b</strong> c</strong></p>",
        &carve::HtmlImportOptions::default(),
    )
    .unwrap()
    .value;
    assert!(matches!(
        carve::render_carve(&doc),
        Err(carve::RenderCarveError::SourceUnspellable(_))
    ));
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
    assert_eq!(carve::render_carve(&doc).unwrap(), "*a {/b *c* d/} e*\n");
}
