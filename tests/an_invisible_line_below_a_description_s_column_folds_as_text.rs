//! A definition below a description's content column follows the owner it
//! reaches. Link, note, and attribute lines keep their existing readings.
//! Under the later carve#4412 ruling, an abbreviation line that misses every
//! frame defines in the document. Both render entry points are checked.

fn html(source: &str) -> String {
    let convenience = carve::to_html(source);
    let cli = carve::try_to_html_with_options(source, &carve::Options::default())
        .expect("the default profile denies nothing");
    assert_eq!(
        convenience, cli,
        "the two render paths disagree on:\n{source}"
    );
    convenience
}

const ENDS: &str = "<dl>\n  <dt>t</dt>\n  <dd>d</dd>\n</dl>\n";

fn folds(line: &str) -> String {
    format!("<dl>\n  <dt>t</dt>\n  <dd>d\n{line}\ntail</dd>\n</dl>")
}

#[test]
fn each_kind_uses_its_ownership_rule_at_both_band_columns() {
    // The band is two columns wide under a `:  ` body.
    for indent in [" ", "  "] {
        for (line, expected) in [
            ("[r]: /u", format!("{ENDS}<p>tail</p>")),
            ("[^f]: n", ENDS.trim_end_matches('\n').to_string()),
            ("{.k}", format!("{ENDS}<p class=\"k\">tail</p>")),
            ("*[A]: a", format!("{ENDS}<p>tail</p>")),
            ("x", folds("x")),
        ] {
            assert_eq!(
                html(&format!(":: t\n:  d\n{indent}{line}\ntail\n")),
                expected,
                "indent {indent:?} line {line:?}"
            );
        }
    }
}

#[test]
fn registrations_follow_each_definition_kind() {
    assert_eq!(
        html(":: t\n:  d\n  [r]: /u\ntail\n\nSee [text][r].\n"),
        format!("{ENDS}<p>tail</p>\n<p>See <a href=\"/u\">text</a>.</p>")
    );
    assert_eq!(
        html(":: t\n:  d\n  [^f]: n\ntail\n\nSee[^f]\n"),
        format!(
            "{ENDS}<p>See<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p>n\ntail<a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">\u{21a9}</a></p>\n    </li>\n  </ol>\n</section>"
        )
    );
    let abbr = html(":: t\n:  d\n  *[A]: a\ntail\n\nA here\n");
    assert_eq!(
        abbr,
        format!("{ENDS}<p>tail</p>\n<p><abbr title=\"a\">A</abbr> here</p>")
    );
}

#[test]
fn the_attribute_attaches_to_nothing_inside_the_description() {
    let out = html(":: t\n:  d\n  {.k}\ntail\n");
    assert_eq!(out, format!("{ENDS}<p class=\"k\">tail</p>"));
    assert!(out.contains("class="), "{out}");
}

#[test]
fn control_a_comment_is_column_exempt_and_renders_nothing() {
    // Corpus 430-5. Waiving the comment with the rest would put its characters
    // on the page.
    assert_eq!(
        html(":: t\n:  d\n  %% c\n"),
        "<dl>\n  <dt>t</dt>\n  <dd>d</dd>\n</dl>"
    );
    assert_eq!(
        html(":: t\n:  d\n  %% c\ntail\n"),
        "<dl>\n  <dt>t</dt>\n  <dd>d</dd>\n</dl>\n<p>tail</p>"
    );
}

#[test]
fn a_real_opener_below_the_column_folds_into_the_body() {
    for indent in [" ", "  "] {
        for opener in ["> q", "# h", "| a |", "---"] {
            let out = html(&format!(":: t\n:  d\n{indent}{opener}\n"));
            assert!(
                out.starts_with(ENDS),
                "indent {indent:?} opener {opener:?} did not end the body:\n{out}"
            );
        }
    }
}

#[test]
fn a_colon_container_below_the_column_still_ends_the_body() {
    for indent in [" ", "  "] {
        assert!(html(&format!(":: t\n:  d\n{indent}::: note\n"))
            .starts_with(ENDS.trim_end_matches('\n')));
    }
}

#[test]
fn control_at_column_zero_the_line_acts() {
    // Corpus 431 and 431-4: the document's own opener column, where a definition
    // registers and a floating attribute attaches forward.
    assert_eq!(
        html(":: t\n:  d\n{.k}\ntail\n"),
        format!("{ENDS}<p class=\"k\">tail</p>")
    );
    assert_eq!(
        html(":: t\n:  d\n[r]: /u\n\nSee [text][r].\n"),
        format!("{ENDS}<p>See <a href=\"/u\">text</a>.</p>")
    );
}

#[test]
fn control_at_the_content_column_the_line_is_inside_the_description() {
    // Not this band. A definition at the column is collected, and an attribute
    // at the column is scoped to the description and dropped by §15 A4 - corpus
    // 329-a-floating-attribute-is-scoped-to-the-container-that-holds-it-5.
    assert_eq!(
        html(":: t\n:  d\n   [r]: /u\ntail\n\nSee [text][r].\n"),
        format!("{ENDS}<p>tail</p>\n<p>See <a href=\"/u\">text</a>.</p>")
    );
    assert_eq!(
        html(":: t\n:  d\n   {.k}\ntail\n"),
        format!("{ENDS}<p>tail</p>")
    );
}

#[test]
fn control_the_list_item_host_it_agrees_with() {
    // The host whose answer this is, in the same build: it was their
    // DISAGREEMENT that was the defect, so one host cannot record it.
    assert_eq!(
        html("- d\n [r]: /u\ntail\n"),
        "<ul>\n  <li>d</li>\n</ul>\n<p>tail</p>"
    );
    assert_eq!(
        html("- para\n  {.k}\n\n  more\n"),
        "<ul>\n  <li><p>para</p>\n    <p class=\"k\">more</p>\n  </li>\n</ul>"
    );
}
