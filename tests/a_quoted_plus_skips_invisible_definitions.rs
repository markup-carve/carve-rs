use carve::to_html;

const ATTACHED: &str =
    "<blockquote>\n  <blockquote>\n    <p>a</p>\n    <p>b</p>\n  </blockquote>\n</blockquote>";

#[test]
fn an_indented_quoted_plus_skips_a_link_definition() {
    assert_eq!(to_html("> > a\n > +\n > [r]: /u\n > b\n"), ATTACHED);
}

#[test]
fn the_same_plus_skips_a_run_of_definitions() {
    assert_eq!(
        to_html("> > a\n > +\n > [r]: /u\n > [s]: /v\n > b\n"),
        ATTACHED
    );
}
