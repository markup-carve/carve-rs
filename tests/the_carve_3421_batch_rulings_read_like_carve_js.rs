//! The 22 October rulings on markup-carve/carve#3421 (comment 6351464749), as
//! carve-js main reads them (corpus 651-656).

use carve::to_html;

fn flat(source: &str) -> String {
    to_html(source).split_whitespace().collect()
}

fn check(cases: &[(&str, &str)]) {
    for (source, expected) in cases {
        assert_eq!(flat(source), *expected, "{source:?}");
    }
}

#[test]
fn a_wrapped_attribute_at_a_container_column_reads_like_the_one_line_form() {
    for source in [
        "- a\n  {.c\n}\npara\n",
        "- a\n  {.c\n .d}\npara\n",
        "- a\n\n  {.c\n}\npara\n",
        "- a\n  {.c\n}\n para\n",
        "1. a\n   {.c\n}\n  para\n",
    ] {
        let one_line = source
            .replacen("{.c\n}", "{.c}", 1)
            .replacen("{.c\n .d}", "{.c .d}", 1);
        assert_eq!(flat(source), flat(&one_line), "{source:?}");
    }
    check(&[(
        ":: t\n: d\n  {.c\n}\npara\n",
        "<dl><dt>t</dt><dd>d</dd></dl><p>para</p>",
    )]);
}

#[test]
fn a_quote_follower_at_column_zero_nests_in_the_innermost_quote() {
    check(&[
        (
            "- > > +\n> q\n",
            "<ul><li><blockquote><blockquote><blockquote><p>q</p></blockquote></blockquote></blockquote></li></ul>",
        ),
        (
            "- > > +\n> q\n> r\n",
            "<ul><li><blockquote><blockquote><blockquote><p>q\nr</p></blockquote></blockquote></blockquote></li></ul>"
                .replace('\n', "")
                .as_str(),
        ),
        (
            "- > +\n> q\n",
            "<ul><li><blockquote><blockquote><p>q</p></blockquote></blockquote></li></ul>",
        ),
        (
            "- > > +\n  > q\n",
            "<ul><li><blockquote><blockquote><p>+q</p></blockquote></blockquote></li></ul>",
        ),
    ]);
}

#[test]
fn a_hosted_stacked_plus_attaches_a_column_zero_block() {
    check(&[
        (
            "> - > +\n# h\n",
            "<blockquote><ul><li><blockquote><h1id=\"h\">h</h1></blockquote></li></ul></blockquote>",
        ),
        (
            "> - > +\n| c |\n| d |\n",
            "<blockquote><ul><li><blockquote><table><tbody><tr><td>c</td></tr><tr><td>d</td></tr></tbody></table></blockquote></li></ul></blockquote>",
        ),
        (
            "> - > +\n# h\n> more\n",
            "<blockquote><ul><li><blockquote><h1id=\"h\">h</h1></blockquote></li></ul><p>more</p></blockquote>",
        ),
        (
            "> - a\n> # t\n> - > +\n# h\n",
            "<blockquote><ul><li>a</li></ul><h1id=\"t\">t</h1><ul><li><blockquote><h1id=\"h\">h</h1></blockquote></li></ul></blockquote>",
        ),
        (
            "> - > +\n  > b\n",
            "<blockquote><ul><li><blockquote><p>+&gt;b</p></blockquote></li></ul></blockquote>",
        ),
    ]);
}

#[test]
fn a_hosted_stacked_plus_after_a_closed_block_attaches_and_one_in_text_does_not() {
    for before in [
        "> # t",
        "> | a |",
        "> ```\n> x\n> ```",
        "> ::: note\n> x\n> :::",
        "> :::\n> x\n> :::",
        "> ***",
        "> {.c}",
        "> %% c",
        "> [r]: /u",
    ] {
        let html = flat(&format!("{before}\n> - > +\n# h\n"));
        assert!(
            html.contains("<li><blockquote><h1id=\"h\">h</h1></blockquote></li>"),
            "{before:?}: {html}"
        );
    }
    for before in [
        "> ```\n> x",
        "> ::: note\n> x",
        "> :::\n> x",
        "> > q",
        "> para",
    ] {
        let html = flat(&format!("{before}\n> - > +\n# h\n"));
        assert!(
            !html.contains("<blockquote><h1id=\"h\">"),
            "{before:?}: {html}"
        );
    }
}

#[test]
fn the_owning_quote_column_stays_eligible_and_an_intervening_one_is_refused() {
    check(&[
        (
            "- > > +\n    b\n",
            "<ul><li><blockquote><blockquote><p>b</p></blockquote></blockquote></li></ul>",
        ),
        (
            "- - > +\n    # h\n",
            "<ul><li><ul><li><blockquote><h1id=\"h\">h</h1></blockquote></li></ul></li></ul>",
        ),
        (
            "- - > +\n    - s\n",
            "<ul><li><ul><li><blockquote><p>+-s</p></blockquote></li></ul></li></ul>",
        ),
        (
            "- > > +\n  # h\n",
            "<ul><li><blockquote><blockquote><p>+</p></blockquote></blockquote><h1id=\"h\">h</h1></li></ul>",
        ),
        (
            "- [ ] > +\n      | c |\n",
            "<ulclass=\"task-list\"><li><inputtype=\"checkbox\"disabled><blockquote><table><tbody><tr><td>c</td></tr></tbody></table></blockquote></li></ul>",
        ),
        (
            "> - > > +\n>     b\n",
            "<blockquote><ul><li><blockquote><blockquote><p>b</p></blockquote></blockquote></li></ul></blockquote>",
        ),
    ]);
}

#[test]
fn a_stacked_plus_in_a_colon_div_attaches_without_position_tracking() {
    check(&[
        (
            "> ::: a\n> ::: b\n> x\n> :::\n> - > +\n# h\n",
            "<blockquote><divclass=\"a\"><divclass=\"b\"><p>x</p></div><ul><li><blockquote><h1id=\"h\">h</h1></blockquote></li></ul></div></blockquote>",
        ),
        (
            "::: a\n- > +\n# h\n:::\n",
            "<divclass=\"a\"><ul><li><blockquote><h1id=\"h\">h</h1></blockquote></li></ul></div>",
        ),
    ]);
}
