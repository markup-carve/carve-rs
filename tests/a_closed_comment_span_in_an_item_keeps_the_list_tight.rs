//! A blank inside an item's `%%%` span with only the span's closer after it is
//! interior, in every host (carve#5030, ruled B).

fn html(source: &str) -> String {
    carve::to_html(source)
}

#[test]
fn every_host_reads_the_blank_as_interior() {
    for (source, expected) in [
        ("- %%%\n\n  %%%\n- d\n", "<ul>\n  <li></li>\n  <li>d</li>\n</ul>"),
        ("- a\n  %%%\n\n  %%%\n- d\n", "<ul>\n  <li>a</li>\n  <li>d</li>\n</ul>"),
        (
            "- a\n  - b\n    %%%\n\n    %%%\n- c\n",
            "<ul>\n  <li>a\n    <ul>\n      <li>b</li>\n    </ul>\n  </li>\n  <li>c</li>\n</ul>",
        ),
        (
            "- a\n  :: u\n  : %%%\n\n    %%%\n- c\n",
            "<ul>\n  <li>a\n    <dl>\n      <dt>u</dt>\n      <dd></dd>\n    </dl>\n  </li>\n  <li>c</li>\n</ul>",
        ),
    ] {
        assert_eq!(html(source), expected, "{source:?}");
    }
}

#[test]
fn a_blank_after_the_span_still_loosens() {
    let source = "- a\n  %%%\n\n- d\n";
    assert_eq!(
        html(source),
        "<ul>\n  <li><p>a</p></li>\n  <li><p>d</p></li>\n</ul>"
    );
}
