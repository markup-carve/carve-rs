//! A quote written on an item's or a description's marker line hosts
//! definitions like any other quote. A note's extent takes its continuation
//! lines and no lazy line (CARVE-P0-012), a definition registers in the
//! innermost container whose content column it reaches (CARVE-P0-020), and
//! neither leaves a paragraph open for a column-0 line (CARVE-P0-011).

fn html(source: &str) -> String {
    carve::to_html(source)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .replace("> <", "><")
}

const QUOTED_LIST: &str = "<blockquote><ul><li>para</li></ul></blockquote>";

#[test]
fn a_note_continuation_in_a_quote_on_an_item_marker_line_stays_in_the_note() {
    assert_eq!(
        html("- > - para\n  >   [^f]: n\n  >     more\ntail\n"),
        format!("<ul><li>{QUOTED_LIST}</li></ul><p>tail</p>")
    );
    let got = html("- > - para\n  >   [^f]: n\n  >     more\ntail[^f]\n");
    assert!(
        got.starts_with(&format!("<ul><li>{QUOTED_LIST}</li></ul><p>tail<a")),
        "{got}"
    );
    assert!(got.contains("<p>n more<a href=\"#fnref1\""), "{got}");
}

#[test]
fn a_note_continuation_in_a_quoted_paragraph_on_an_item_marker_line() {
    let got = html("- > para\n  > [^f]: n\n  >   more\ntail[^f]\n");
    assert!(
        got.starts_with("<ul><li><blockquote><p>para</p></blockquote></li></ul><p>tail<a"),
        "{got}"
    );
    assert!(got.contains("<p>n more<a href=\"#fnref1\""), "{got}");
}

#[test]
fn a_definition_in_a_quote_on_a_description_marker_line_registers() {
    assert_eq!(
        html(":: t\n: > - para\n  >   [r]: /u\ntail [r][]\n"),
        format!("<dl><dt>t</dt><dd>{QUOTED_LIST}</dd></dl><p>tail <a href=\"/u\">r</a></p>")
    );
}

#[test]
fn a_definition_in_a_quote_under_a_description_leaves_no_lazy_paragraph() {
    assert_eq!(
        html(":: t\n: > para\n  > [r]: /u\ntail [r][]\n"),
        "<dl><dt>t</dt><dd><blockquote><p>para</p></blockquote></dd></dl><p>tail <a href=\"/u\">r</a></p>"
    );
    assert_eq!(
        html(":: t\n: d\n  > - para\n  >   [r]: /u\ntail [r][]\n"),
        format!(
            "<dl><dt>t</dt><dd><p>d</p>{QUOTED_LIST}</dd></dl><p>tail <a href=\"/u\">r</a></p>"
        )
    );
}

#[test]
fn quoted_prose_under_a_description_still_folds() {
    assert_eq!(
        html(":: t\n: > - para\n  >   more\ntail\n"),
        "<dl><dt>t</dt><dd><blockquote><ul><li>para more tail</li></ul></blockquote></dd></dl>"
    );
}

#[test]
fn a_note_on_the_marker_line_of_a_quote_on_a_container_marker_line_keeps_its_continuation() {
    let got = html("- > [^f]: n\n  >   more\ntail[^f]\n");
    assert!(
        got.starts_with("<ul><li><blockquote></blockquote></li></ul><p>tail<a"),
        "{got}"
    );
    assert!(got.contains("<p>n more<a href=\"#fnref1\""), "{got}");
    let got = html(":: t\n: > [^f]: n\n  >   more\ntail[^f]\n");
    assert!(got.contains("<p>n more<a href=\"#fnref1\""), "{got}");
    assert!(!got.contains("<blockquote><p>"), "{got}");
}
