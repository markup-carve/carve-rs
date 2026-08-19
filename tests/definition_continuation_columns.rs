//! The definition-continuation matrix from markup-carve/carve#1376.

fn source(
    head: &str,
    content_column: usize,
    definition: &str,
    continuation_column: usize,
) -> String {
    format!(
        "{head}\n{}{definition}\n{}more\ntail\n",
        " ".repeat(content_column),
        " ".repeat(continuation_column),
    )
}

fn tail_is_inside(html: &str) -> bool {
    html.find("tail").unwrap() < html.find("</li>").unwrap()
}

const HEADS: [(&str, usize); 3] = [("- a", 2), ("1. a", 3), (". a", 2)];

#[test]
fn collected_definitions_close_the_below_content_column_path() {
    for (head, content_column) in HEADS {
        for definition in ["[^f]: t", "[r]: /u"] {
            for column in 1..content_column {
                let html = carve::to_html(&source(head, content_column, definition, column));
                assert!(
                    !tail_is_inside(&html),
                    "{head:?} {definition:?} column {column}: {html}"
                );
                assert!(html.contains("<p>more\ntail</p>"), "{html}");
            }
        }
    }
}

#[test]
fn a_definition_on_the_marker_line_closes_the_below_content_column_path() {
    for head in ["- [r]: /u", "1. [r]: /u", ". [r]: /u"] {
        for tail in [" :", "%%\n:"] {
            let html = carve::to_html(&format!("{head}\n{tail}\n"));
            assert!(html.contains("<li></li>"), "{head:?} {tail:?}: {html}");
            assert!(html.contains("<p>:</p>"), "{head:?} {tail:?}: {html}");
            assert!(html.find("<p>:</p>").unwrap() > html.find("</li>").unwrap());
        }
    }
}

#[test]
fn item_prose_reopens_at_its_column_and_one_short_of_a_footnote_body() {
    for (head, content_column) in HEADS {
        for definition in ["[^f]: t", "[r]: /u"] {
            let html = carve::to_html(&source(head, content_column, definition, content_column));
            assert!(tail_is_inside(&html), "{head:?} {definition:?}: {html}");
        }
        let html = carve::to_html(&source(head, content_column, "[^f]: t", content_column + 1));
        assert!(tail_is_inside(&html), "{head:?}: {html}");
    }
}

#[test]
fn a_line_at_the_footnote_body_column_stays_in_the_definition_block() {
    for (head, content_column) in HEADS {
        let mut src = source(head, content_column, "[^f]: t", content_column + 2);
        src.push_str("\nx[^f]\n");
        let html = carve::to_html(&src);
        assert!(!tail_is_inside(&html), "{head:?}: {html}");
        assert!(html.contains("t\nmore"), "{html}");
    }
}

#[test]
fn abbreviation_shape_is_item_prose_not_a_definition() {
    for (head, content_column) in HEADS {
        let html = carve::to_html(&source(head, content_column, "*[A]: expansion", 1));
        assert!(tail_is_inside(&html), "{head:?}: {html}");
    }
}
