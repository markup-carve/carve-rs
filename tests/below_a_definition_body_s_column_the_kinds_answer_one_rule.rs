//! Definition-shaped lines below a description's content column follow the
//! owner they reach. The later carve#4412 ruling makes an abbreviation line
//! that misses every frame a document definition, even at a nonzero column.
//! A line that reaches an outer item remains text there. Every row runs on
//! both render entry points.

fn html(source: &str) -> String {
    let convenience = carve::to_html(source);
    let cli = carve::try_to_html_with_options(source, &carve::Options::default())
        .expect("the default profile denies nothing");
    // BOTH ENTRY POINTS, ALWAYS. `to_html` opens with a layout fast path and the
    // CLI runs transforms, and a divergence living on one path only is what a
    // single-path assertion cannot see.
    assert_eq!(
        convenience, cli,
        "the two render paths disagree on:\n{source}"
    );
    convenience
}

const ENDS: &str = "<dl>\n  <dt>t</dt>\n  <dd>d</dd>\n</dl>\n";
const FOLDS_HEAD: &str = "<dl>\n  <dt>t</dt>\n  <dd>d\n";

#[test]
fn a_link_definition_below_the_column_folds_like_the_footnote_one() {
    // The pair this file was written about, now both folding. They still have to
    // AGREE - that half of #1438 was right and no clause separates them by
    // column - and carve#1809 supplied the direction.
    for indent in ["  ", " "] {
        assert_eq!(
            html(&format!(":: t\n:  d\n{indent}[r]: /u\ntail\n")),
            format!("{ENDS}<p>tail</p>"),
            "indent {:?}",
            indent
        );
        assert_eq!(
            html(&format!(":: t\n:  d\n{indent}[^f]: n\ntail\n")),
            ENDS.trim_end(),
            "indent {:?}",
            indent
        );
    }
}

#[test]
fn a_link_definition_below_the_column_still_registers_nothing() {
    // Unchanged in substance, and it is the row that makes the fold a whole one:
    // the characters reach the page AND the symbol table stays empty, so a later
    // reference is literal. Text plus a registration is the half fold corpus 430
    // exists to catch.
    let out = html(":: t\n:  d\n  [r]: /u\ntail\n\n[link][r]\n");
    assert!(out.contains("<dd>d</dd>"), "{out}");
    assert!(!out.contains("<p>[link][r]</p>"), "{out}");
    assert!(out.contains("<p><a href=\"/u\">link</a></p>"), "{out}");
}

#[test]
fn an_abbreviation_definition_below_the_column_defines_in_the_document() {
    for indent in ["", " ", "  "] {
        assert_eq!(
            html(&format!(":: t\n:  d\n{indent}*[A]: a\ntail\n")),
            format!("{ENDS}<p>tail</p>"),
            "indent {:?}",
            indent
        );
    }
}

#[test]
fn an_abbreviation_definition_below_the_column_registers() {
    let out = html(":: t\n:  d\n  *[A]: a\ntail\n\nA here\n");
    assert!(
        out.contains("<p><abbr title=\"a\">A</abbr> here</p>"),
        "{out}"
    );
}

#[test]
fn the_nested_spelling_answers_the_same_way_it_always_did() {
    // The line misses the description but still reaches its outer item.
    assert_eq!(
        html("- :: t\n  :  d\n   *[A]: a\n  tail\n"),
        "<ul>\n  <li>\n    <dl>\n      <dt>t</dt>\n      <dd>d\n*[A]: a\ntail</dd>\n    </dl>\n  </li>\n</ul>"
    );
}

#[test]
fn controls_the_plain_line_and_the_comment_bound_the_band() {
    // The plain line is what "folds as text" means and folded from every column
    // all along. The ATTRIBUTE line moved with the definitions under carve#1809 -
    // this row used to assert that it ended the body - while the COMMENT is
    // column-exempt (PART 9 section 24) and renders nothing at any column, which
    // corpus 430-5 pins. The comment is what tells this fix from one that folded
    // the whole invisible set.
    assert_eq!(
        html(":: t\n:  d\n  x\ntail\n"),
        format!("{FOLDS_HEAD}x\ntail</dd>\n</dl>")
    );
    assert_eq!(
        html(":: t\n:  d\n  {.k}\ntail\n"),
        format!("{ENDS}<p class=\"k\">tail</p>")
    );
    assert_eq!(
        html(":: t\n:  d\n  %% c\ntail\n"),
        format!("{ENDS}<p>tail</p>")
    );
}

#[test]
fn controls_at_and_above_the_content_column_are_untouched() {
    // AT the column a link definition is collected and an attribute line is
    // dropped inside the description; the band being fixed is strictly BELOW.
    assert_eq!(
        html(":: t\n:  d\n   [r]: /u\ntail\n\n[link][r]\n"),
        format!("{ENDS}<p>tail</p>\n<p><a href=\"/u\">link</a></p>")
    );
    assert_eq!(
        html(":: t\n:  d\n   {.k}\ntail\n"),
        format!("{ENDS}<p>tail</p>")
    );
}

#[test]
fn control_at_document_level_an_abbreviation_definition_is_still_one() {
    // The waived arm is a CONTAINER-continuation question. At document level,
    // where section 7 recognizes the definition, it still interrupts and still
    // registers.
    assert_eq!(
        html("para\n*[A]: a\n\nA here\n"),
        "<p>para</p>\n<p><abbr title=\"a\">A</abbr> here</p>"
    );
}
