// CARVE-P2-035 (markup-carve/carve#5613): an unclosed fence after a refused brace run
// is paragraph text, so an indented block opener below it interrupts the
// paragraph as it would interrupt any other paragraph (I1-I5).

use carve::{to_html, to_html_with_options, Options};

fn check(source: &str, expected: &str) {
    assert_eq!(to_html(source), expected);
    let positioned = to_html_with_options(source, &Options::default().with_positions(true));
    assert_eq!(positioned, expected);
}

#[test]
fn a_heading_past_the_description_body_column_interrupts() {
    check(
        ":: t\n: +\n  {.x\n  .y}\n  ```\n   # h\n",
        "<dl>\n  <dt>t</dt>\n  <dd>\n    <p>{.x\n.y}\n<code></code></p>\n    <h1 id=\"h\">h</h1>\n  </dd>\n</dl>",
    );
}

#[test]
fn a_heading_past_the_item_body_column_interrupts() {
    check(
        "- +\n  {.x\n  .y}\n  ```\n   # h\n",
        "<ul>\n  <li>{.x\n.y}\n<code></code>\n    <h1 id=\"h\">h</h1>\n  </li>\n</ul>",
    );
}

#[test]
fn an_indented_heading_at_the_root_interrupts() {
    check(
        "{.x\n.y}\n```\n # h\n",
        "<p>{.x\n.y}\n<code></code></p>\n<section id=\"h\">\n  <h1>h</h1>\n</section>",
    );
}

#[test]
fn an_indented_quote_at_the_root_interrupts() {
    check(
        "{.x\n.y}\n```\nq\n > q\n",
        "<p>{.x\n.y}\n<code>\nq</code></p>\n<blockquote><p>q</p></blockquote>",
    );
}

#[test]
fn a_closed_follower_keeps_its_lines() {
    check(
        "{.x\n.y}\n```\n # h\n```\n",
        "<p>{.x\n.y}\n<code>\n# h\n</code></p>",
    );
}
