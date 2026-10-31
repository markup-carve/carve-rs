// carve-rs#3627: an item whose marker line is only an attribute block still
// gives its later paragraph the lazy continuation every paragraph has.
#[test]
fn a_flush_line_folds_into_the_paragraph_after_a_marker_line_attribute() {
    let cases = [
        ("- \\\n- {x}\n\n  b\nb\n", "<li><p x=\"\">b\nb</p></li>"),
        ("- {x}\n  b\nb\n", "<li><p x=\"\">b\nb</p></li>"),
        (
            "1. {#i .c}\n   b\nb\n",
            "<li><p id=\"i\" class=\"c\">b\nb</p></li>",
        ),
        (
            "- {x}\n  > b\nb\n",
            "<blockquote x=\"\"><p>b\nb</p></blockquote>",
        ),
        ("> - {x}\n>   b\n> b\n", "<li><p x=\"\">b\nb</p></li>"),
    ];
    for (source, expected) in cases {
        let html = carve::to_html(source);
        assert!(html.contains(expected), "{source:?}\n{html}");
        assert!(!html.contains("<p>b</p>"), "{source:?}\n{html}");
    }
}

#[test]
fn a_marker_line_attribute_with_no_paragraph_still_ends_the_item() {
    for source in ["- {x}\nb\n", "- {x}\n\nb\n", "- {x}\n  # h\nb\n"] {
        let html = carve::to_html(source);
        assert!(
            html.trim_end().ends_with("</ul>\n<p>b</p>"),
            "{source:?}\n{html}"
        );
    }
}
