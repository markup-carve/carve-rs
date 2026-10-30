//! A marker-line quote that holds no paragraph ends at its line, but a paragraph
//! written after it in the item is open: a line in the band below the item's
//! content column folds into that paragraph (pack plus-baa850a0968e).

fn html(source: &str) -> String {
    carve::to_html(source)
}

#[test]
fn a_band_line_folds_after_a_heading_quote() {
    assert_eq!(
        html("- > # h\n  b\n tail\n").trim(),
        "<ul>\n  <li>\n    <blockquote>\n      <h1 id=\"h\">h</h1>\n    </blockquote>\n    b\ntail\n  </li>\n</ul>"
    );
}

#[test]
fn a_band_line_folds_after_a_blank_and_a_table_quote() {
    let out = html("- > | a |\n\n  b\n tail\n");
    assert!(out.contains("<p>b\ntail</p>\n  </li>\n</ul>"), "{out}");
}

#[test]
fn a_band_line_stays_out_with_no_paragraph_to_take_it() {
    let out = html("- > # h\n tail\n");
    assert!(out.ends_with("<p>tail</p>"), "{out}");
}

#[test]
fn a_column_zero_abbreviation_shape_still_folds() {
    let out = html("- > | a |\n  b |\n*[A]: x\n");
    assert!(out.contains("b |\n*[A]: x\n  </li>"), "{out}");
}
