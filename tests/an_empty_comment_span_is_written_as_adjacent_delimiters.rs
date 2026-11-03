//! An empty `%%%` span is written as its opener directly followed by its
//! closer (markup-carve/carve#5022, ruling A). An empty payload line is a blank
//! inside a list item, which loosens the list before a sibling (PART 17 L1c).

fn round_trip(source: &str, expected: &str) {
    let written = carve::to_carve(source);
    assert_eq!(written, expected, "source: {source:?}");
    assert_eq!(
        carve::to_html(&written),
        carve::to_html(source),
        "source: {source:?}"
    );
    assert_eq!(
        carve::parse(&written).children,
        carve::parse(source).children,
        "source: {source:?}"
    );
    assert_eq!(carve::to_carve(&written), written, "source: {source:?}");
}

#[test]
fn every_host_writes_no_empty_payload_line() {
    for (source, expected) in [
        (
            "- a\n  :: u\n  : %%%\n  %%%\n- c\n",
            "- a\n  :: u\n  : %%%\n    %%%\n- c\n",
        ),
        ("- %%%\n  %%%\n- c\n", "- %%%\n  %%%\n- c\n"),
        ("- a\n  %%%\n  %%%\n- c\n", "- a\n  %%%\n  %%%\n- c\n"),
        ("x[^n]\n\n[^n]: %%%\n  %%%\n", "x[^n]\n\n[^n]: %%%\n  %%%\n"),
        ("%%%\n%%%\n", "%%%\n%%%\n"),
        ("%%%\n", "%%%\n%%%\n"),
        ("%%%%\n%%%%\n", "%%%\n%%%\n"),
    ] {
        round_trip(source, expected);
    }
}

#[test]
fn a_payload_of_empty_lines_keeps_them() {
    round_trip("%%%\n\n\n%%%\n", "%%%\n\n\n%%%\n");
}
