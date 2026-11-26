//! carve#6507 follow-up and carve#6514 A: block lines end an abbreviation, a
//! link tail spans two lines, a comment-only line ends a definition (C2-B-083,
//! C2-B-084, C2-B-137). carve#6508: captions and terms are labels (C2-B-234).
use carve::api::{Carve, Options, WriteOptions};

fn html(input: &str) -> String {
    Carve::new().render(input, &Options::default()).unwrap()
}

fn fmt(input: &str) -> String {
    Carve::new()
        .format(input, &WriteOptions::default())
        .unwrap()
}

#[test]
fn a_block_line_ends_an_abbreviation() {
    let cases = [
        (
            "*[A]: x\n  ::: note\n  y\n  :::\n\nA\n",
            "<aside class=\"admonition note\" aria-label=\"Note\">\n  <p>y</p>\n</aside>\n<p><abbr title=\"x\">A</abbr></p>",
        ),
        (
            "*[A]: x\n  %%%\n  y\n  %%%\n\nA\n",
            "<p><abbr title=\"x\">A</abbr></p>",
        ),
        // A heading line is text, as below a heading.
        (
            "*[A]: x\n  # y\n\nA\n",
            "<p><abbr title=\"x # y\">A</abbr></p>",
        ),
    ];
    for (input, expected) in cases {
        assert_eq!(html(input), expected, "{input:?}");
    }
}

#[test]
fn a_comment_line_ends_a_definition_whatever_its_indent() {
    let cases = [
        (
            "*[A]: x\n  %% c\n  y\n\nA\n",
            "<p>y</p>\n<p><abbr title=\"x\">A</abbr></p>",
        ),
        (
            "[a]: /u\n  \"T\"\n%% c\n  {.c}\nP\n\n[a]\n",
            "<p class=\"c\">P</p>\n<p><a href=\"/u\" title=\"T\">a</a></p>",
        ),
    ];
    for (input, expected) in cases {
        assert_eq!(html(input), expected, "{input:?}");
    }
    assert_eq!(
        fmt("*[A]: x\n  %% c\n  y\n\nA\n"),
        "*[A]: x\n%% c\n\ny\n\nA\n"
    );
}

#[test]
fn a_link_tail_takes_a_title_line_then_an_attribute_line() {
    let cases = [
        (
            "[a]: /u \"T\"\n  {.c}\n  {.d}\nP\n\n[a]\n",
            "<p class=\"d\">P</p>\n<p><a href=\"/u\" title=\"T\" class=\"c\">a</a></p>",
        ),
        (
            "[a]: /u\n  {.c}\n  {.d}\nP\n\n[a]\n",
            "<p class=\"d\">P</p>\n<p><a href=\"/u\" class=\"c\">a</a></p>",
        ),
    ];
    for (input, expected) in cases {
        assert_eq!(html(input), expected, "{input:?}");
    }
}

#[test]
fn terms_and_captions_continue_only_on_indented_lines() {
    let cases = [
        (
            ":: t\n%% c\n  more\n: d\n",
            "<dl>\n  <dt>t\nmore</dt>\n  <dd>d</dd>\n</dl>",
        ),
        (
            "- :: t\n    more\n  : d\n",
            "<ul>\n  <li>\n    <dl>\n      <dt>t\nmore</dt>\n      <dd>d</dd>\n    </dl>\n  </li>\n</ul>",
        ),
        (
            "> | a |\n> ^ cap\n>   more\n> next\n",
            "<blockquote>\n  <table>\n    <caption>cap\nmore</caption>\n    <tbody>\n      <tr><td>a</td></tr>\n    </tbody>\n  </table>\n  <p>next</p>\n</blockquote>",
        ),
    ];
    for (input, expected) in cases {
        assert_eq!(html(input), expected, "{input:?}");
    }
}
