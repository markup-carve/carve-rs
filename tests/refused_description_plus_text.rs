#[test]
fn an_ordinary_follower_of_a_refused_description_plus_stays_in_the_description() {
    let expected = "<dl>\n  <dt>t</dt>\n  <dd>\n    <ol>\n      <li>li</li>\n    </ol>\n    <p>+ sib</p>\n  </dd>\n</dl>";
    for indent in ["  ", "   "] {
        let source = format!(":: t\n: +\n1. li\n+\n{indent}+ sib\n");
        assert_eq!(carve::to_html(&source), expected, "{source:?}");
    }
}
