//! A `%%%` span opened on a list marker line three or more items deep closes
//! at a closer below the item's column, as it does one or two levels up
//! (PART 9 §28, carve#5028). Expected HTML is the spec model's at carve `6c163012`.

fn flat(source: &str) -> String {
    carve::to_html(source)
        .split('\n')
        .map(str::trim)
        .collect::<String>()
}

#[test]
fn a_closer_below_a_deep_marker_line_span_closes_it() {
    let cases = [
        (
            "- a\n  - b\n    - %%%\n%%%\nc\n",
            "<ul><li>a<ul><li>b<ul><li></li></ul></li></ul></li></ul><p>c</p>",
        ),
        (
            "- a\n  - b\n    - c\n      - %%%\n%%%\nc\n",
            "<ul><li>a<ul><li>b<ul><li>c<ul><li></li></ul></li></ul></li></ul></li></ul><p>c</p>",
        ),
        (
            "- a\n  - b\n    - %%%\n%%%\n- c\n",
            "<ul><li>a<ul><li>b<ul><li></li></ul></li></ul></li><li>c</li></ul>",
        ),
        (
            "1. a\n   1. b\n      1. %%%\n   %%%\nc\n",
            "<ol><li>a<ol><li>b<ol><li></li></ol></li></ol></li></ol><p>c</p>",
        ),
    ];
    for (source, html) in cases {
        assert_eq!(flat(source), html, "{source:?}");
    }
}
