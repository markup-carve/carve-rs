use carve::{to_html, to_html_with_options, Options};

#[test]
fn shortcut_keeps_indented_row_outside_table() {
    let source = "|a|\n|---|\n |c|";
    let expected = "<table>\n  <thead>\n    <tr><th scope=\"col\">a</th></tr>\n  </thead>\n</table>\n<p>|c|</p>";
    assert_eq!(to_html(source), expected);
    assert_eq!(
        to_html(source),
        to_html_with_options(source, &Options::default().with_positions(true))
    );
}
