//! An invisible definition or attribute line inside a quote leaves no paragraph
//! open (CARVE-P0-011, CARVE-P0-012), so with the quote nested in an item a
//! following line has nothing to fold into and is owned where its column lands.

fn html(source: &str) -> String {
    carve::to_html(source)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .replace("> <", "><")
}

const QUOTED_LIST: &str = "<ul><li><blockquote><ul><li>para</li></ul></blockquote></li></ul>";

#[test]
fn a_column_zero_line_leaves_the_item() {
    for line in ["[r]: /u", "{#x}", "[^f]: n", "# H", "%% c"] {
        let source = format!("- > - para\n  >   {line}\ntail\n");
        let got = html(&source);
        let heading = line == "# H";
        if heading {
            assert!(got.ends_with("</ul><p>tail</p>"), "{line}: {got}");
        } else if line == "[^f]: n" {
            assert_eq!(got, QUOTED_LIST, "{line}");
        } else {
            assert_eq!(got, format!("{QUOTED_LIST}<p>tail</p>"), "{line}");
        }
    }
}

#[test]
fn a_footnote_under_a_quoted_paragraph_leaves_no_lazy_paragraph() {
    assert_eq!(
        html("- > para\n  > [^f]: n\ntail\n"),
        "<ul><li><blockquote><p>para</p></blockquote></li></ul>"
    );
}

#[test]
fn a_line_at_the_outer_item_column_stays_in_the_item() {
    assert_eq!(
        html("- > - para\n  >   [^f]: n\n  tail\n"),
        "<ul><li><blockquote><ul><li>para</li></ul></blockquote></li></ul>"
    );
}

#[test]
fn quoted_prose_still_folds() {
    assert_eq!(
        html("- > - para\n  >   more\ntail\n"),
        "<ul><li><blockquote><ul><li>para more tail</li></ul></blockquote></li></ul>"
    );
}

#[test]
fn a_nested_quote_and_a_wrapped_attribute_block_leave_no_lazy_paragraph() {
    assert_eq!(
        html("- > > - para\n  > >   [r]: /u\ntail\n"),
        "<ul><li><blockquote><blockquote><ul><li>para</li></ul></blockquote></blockquote></li></ul><p>tail</p>"
    );
    assert_eq!(
        html("- > para\n  > {.a\n  > .b}\ntail\n"),
        "<ul><li><blockquote><p>para</p></blockquote></li></ul><p>tail</p>"
    );
}
