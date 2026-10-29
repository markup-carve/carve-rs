//! PART 10 I2 (markup-carve/carve#3759): a standalone `@code` directive is a
//! code include by its spelling, so an unknown or malformed option leaves it
//! literal on its own line.

use carve::to_html;

#[test]
fn the_line_is_literal_and_ends_its_paragraph() {
    for line in [
        "{{ data/p2.csv @code:csv @lins:2-1 }}",
        "{{ a.csv @code @foo }}",
        "  {{ data/p2.csv @code:csv @lines01-2 }}",
        "{{ data/p2.csv @code::csv @liness:0-2 }}",
        "{{ a.csv #sec @code:csv }}",
        "{{ a.csv @code:js }}",
    ] {
        assert_eq!(
            to_html(&format!("{line}\nmore\n")).trim(),
            format!("<p>{}</p>\n<p>more</p>", line.trim()),
            "{line}"
        );
    }
}

#[test]
fn a_lazy_line_in_an_item_is_the_next_paragraph() {
    assert_eq!(
        to_html("- {{ a.csv @code @foo }}\n^ cap\n").trim(),
        "<ul>\n  <li>{{ a.csv @code @foo }}\n    ^ cap\n  </li>\n</ul>"
    );
}

#[test]
fn a_bare_code_directive_stays_literal_inside_a_sentence() {
    assert_eq!(
        to_html("See {{ a.csv @code @foo }} here.\n").trim(),
        "<p>See {{ a.csv @code @foo }} here.</p>"
    );
}
