//! A `%%%` span opened on a description, note or list marker line is held by
//! that body: a blank inside it is payload (PART 0 S1), a closer below the
//! body's column closes it (PART 9 §28), and the span leaves no paragraph for a
//! later line to fold into (carve#4996, carve#4998). Expected HTML is the spec
//! model's at carve `8af93790`.

fn flat(source: &str) -> String {
    carve::to_html(source)
        .split('\n')
        .map(str::trim)
        .collect::<String>()
}

#[test]
fn a_closer_below_the_column_closes_a_lead_line_span() {
    let cases = [
        (
            ":: u\n: %%%\n%%%\nc\n",
            "<dl><dt>u</dt><dd></dd></dl><p>c</p>",
        ),
        (
            "- a\n  :: u\n  : %%%\n%%%\nc\n",
            "<ul><li>a<dl><dt>u</dt><dd></dd></dl></li></ul><p>c</p>",
        ),
        (
            "- a\n  - %%%\n    %% c\n%%%\nc\n",
            "<ul><li>a<ul><li></li></ul></li></ul><p>c</p>",
        ),
        (
            "- a\n\n  [^n]: %%%\n%%%\nc\n",
            "<ul><li>a</li></ul><p>c</p>",
        ),
    ];
    for (source, html) in cases {
        assert_eq!(flat(source), html, "{source:?}");
    }
}

#[test]
fn a_blank_inside_a_lead_line_span_is_payload() {
    let cases = [
        (
            ":: u\n: %%%\n    n\n\n%%%\nc\n",
            "<dl><dt>u</dt><dd></dd></dl><p>c</p>",
        ),
        (
            "- a\n  - %%%\n    n\n\n%%%\nc\n",
            "<ul><li>a<ul><li></li></ul></li></ul><p>c</p>",
        ),
        (
            "- a\n  :: u\n  : %%%\n\n%%%\n  c\n",
            "<ul><li>a<dl><dt>u</dt><dd></dd></dl>c</li></ul>",
        ),
        ("- %%%\n\n\n%% c\n     x\n", "<ul><li></li></ul>"),
    ];
    for (source, html) in cases {
        assert_eq!(flat(source), html, "{source:?}");
    }
    let note = flat("x[^n]\n\n[^n]: %%%\n    n\n\n%%%\nc\n");
    assert!(note.contains("</p><p>c</p><section"), "{note}");
}

#[test]
fn a_lead_line_span_leaves_no_paragraph_for_a_lazy_line() {
    let item = "<ul><li>a<dl><dt>u</dt><dd></dd></dl></li></ul><p>zz</p>";
    assert_eq!(flat("- a\n  :: u\n  : %%%\n    n\nzz\n"), item);
    assert_eq!(flat("- a\n  :: u\n  : %%%\n    - n\nzz\n"), item);
    assert_eq!(
        flat("> - a\n>   :: u\n>   : %%%\n>     n\nzz\n"),
        "<blockquote><ul><li>a<dl><dt>u</dt><dd></dd></dl></li></ul></blockquote><p>zz</p>"
    );
    assert_eq!(
        flat("> :: u\n> : %%%\n>   n\nzz\n"),
        "<blockquote><dl><dt>u</dt><dd></dd></dl></blockquote><p>zz</p>"
    );
}

#[test]
fn a_definition_after_a_closed_lead_line_span_registers() {
    assert_eq!(
        flat(":: u\n: %%%\n  %%%\n  [d]: u\n\n[t][d]\n"),
        r#"<dl><dt>u</dt><dd></dd></dl><p><a href="u">t</a></p>"#
    );
}
