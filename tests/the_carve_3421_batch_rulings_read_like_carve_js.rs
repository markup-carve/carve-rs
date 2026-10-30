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
            "<ul><li><blockquote><blockquote><p>+</p></blockquote></blockquote></li></ul><blockquote><p>q</p></blockquote>",
        ),
        (
            "- > > +\n> q\n> r\n",
            "<ul><li><blockquote><blockquote><p>+</p></blockquote></blockquote></li></ul><blockquote><p>qr</p></blockquote>",
        ),
        (
            "- > +\n> q\n",
            "<ul><li><blockquote><p>+</p></blockquote></li></ul><blockquote><p>q</p></blockquote>",
        ),
        (
            "- > > +\n  > q\n",
            "<ul><li><blockquote><blockquote><p>q</p></blockquote></blockquote></li></ul>",
        ),
    ]);
}

#[test]
fn a_hosted_stacked_plus_attaches_a_column_zero_block() {
    check(&[
        (
            "> - > +\n# h\n",
            "<blockquote><ul><li><blockquote><p>+</p></blockquote></li></ul></blockquote><sectionid=\"h\"><h1>h</h1></section>",
        ),
        (
            "> - > +\n| c |\n| d |\n",
            "<blockquote><ul><li><blockquote><p>+</p></blockquote></li></ul></blockquote><table><tbody><tr><td>c</td></tr><tr><td>d</td></tr></tbody></table>",
        ),
        (
            "> - > +\n# h\n> more\n",
            "<blockquote><ul><li><blockquote><p>+</p></blockquote></li></ul></blockquote><sectionid=\"h\"><h1>h</h1><blockquote><p>more</p></blockquote></section>",
        ),
        (
            "> - a\n> # t\n> - > +\n# h\n",
            "<blockquote><ul><li>a</li></ul><h1id=\"t\">t</h1><ul><li><blockquote><p>+</p></blockquote></li></ul></blockquote><sectionid=\"h\"><h1>h</h1></section>",
        ),
        (
            "> - > +\n  > b\n",
            "<blockquote><ul><li><blockquote><p>b</p></blockquote></li></ul></blockquote>",
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
            html.contains("<li><blockquote><p>+</p></blockquote></li>"),
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
            "<ul><li><blockquote><blockquote><p>+b</p></blockquote></blockquote></li></ul>",
        ),
        (
            "- - > +\n    # h\n",
            "<ul><li><ul><li><blockquote><h1id=\"h\">h</h1></blockquote></li></ul></li></ul>",
        ),
        (
            "- - > +\n    - s\n",
            "<ul><li><ul><li><blockquote><ul><li>s</li></ul></blockquote></li></ul></li></ul>",
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
            "<blockquote><ul><li><blockquote><blockquote><p>+b</p></blockquote></blockquote></li></ul></blockquote>",
        ),
    ]);
}

#[test]
fn a_stacked_plus_in_a_colon_div_attaches_without_position_tracking() {
    check(&[
        (
            "> ::: a\n> ::: b\n> x\n> :::\n> - > +\n# h\n",
            "<blockquote><divclass=\"a\"><divclass=\"b\"><p>x</p></div><ul><li><blockquote><p>+</p></blockquote></li></ul></div></blockquote><sectionid=\"h\"><h1>h</h1></section>",
        ),
        (
            "::: a\n- > +\n# h\n:::\n",
            "<divclass=\"a\"><ul><li><blockquote><p>+</p></blockquote></li></ul><h1id=\"h\">h</h1></div>",
        ),
    ]);
}

#[test]
fn a_quote_plus_below_an_item_line_attaches_at_depth_and_after_a_sibling() {
    check(&[
        (
            "> > - > +\n# h\n",
            "<blockquote><blockquote><ul><li><blockquote><p>+</p></blockquote></li></ul></blockquote></blockquote><sectionid=\"h\"><h1>h</h1></section>",
        ),
        (
            "> > - > +\n| c |\n",
            "<blockquote><blockquote><ul><li><blockquote><p>+</p></blockquote></li></ul></blockquote></blockquote><table><tbody><tr><td>c</td></tr></tbody></table>",
        ),
        (
            "> - a\n> b\n> - > +\n# h\n",
            "<blockquote><ul><li>ab</li><li><blockquote><p>+</p></blockquote></li></ul></blockquote><sectionid=\"h\"><h1>h</h1></section>",
        ),
        (
            "> > para\n> > - > +\n# h\n",
            "<blockquote><blockquote><p>para-&gt;+</p></blockquote></blockquote><sectionid=\"h\"><h1>h</h1></section>",
        ),
        (
            "- a\n  > +\n# h\n",
            "<ul><li>a<blockquote><p>+</p></blockquote></li></ul><sectionid=\"h\"><h1>h</h1></section>",
        ),
        (
            "- a\n  > +\nb\n",
            "<ul><li>a<blockquote><p>+b</p></blockquote></li></ul>",
        ),
        (
            "- a\n  > +\n  # h\n",
            "<ul><li>a<blockquote><h1id=\"h\">h</h1></blockquote></li></ul>",
        ),
    ]);
    let lint = carve::lint_carve("- a\n  > +\n# h\n");
    assert!(lint
        .iter()
        .any(|warning| warning.rule == "continuation-marker-refused"));
}

#[test]
fn a_wrapped_attribute_in_a_nested_item_is_one_construct_for_the_outer_item() {
    check(&[
        (
            "- a\n  - b\n    {.c\n}\npara\n",
            "<ul><li>a<ul><li>b</li></ul></li></ul><p>para</p>",
        ),
        (
            "- a\n  - b\n\n    {.c\n}\npara\n",
            "<ul><li>a<ul><li>b</li></ul></li></ul><p>para</p>",
        ),
        (
            "- a\n  - b\n    {.c\n.d}\npara\n",
            "<ul><li>a<ul><li>b</li></ul></li></ul><p>para</p>",
        ),
        (
            "- a\n  - b\n    {.c\n }\npara\n",
            "<ul><li>a<ul><li>b</li></ul></li></ul><p>para</p>",
        ),
        (
            "- a\n  - b\n    {.c\n}\n- s\n",
            "<ul><li>a<ul><li>b</li></ul></li><li>s</li></ul>",
        ),
        (
            "- a\n  - b\n    {.c\n}\n    para\n",
            "<ul><li>a<ul><li>b<pclass=\"c\">para</p></li></ul></li></ul>",
        ),
    ]);
}

#[test]
fn stacked_quote_plus_readings_shared_by_the_spec_model_and_carve_js() {
    check(&[
        (
            "- > > +\n    # h\n",
            "<ul><li><blockquote><blockquote><p>+</p></blockquote></blockquote><h1id=\"h\">h</h1></li></ul>",
        ),
        (
            "- > > +\n    ```\nx\n```\n",
            "<ul><li><blockquote><blockquote><p>+</p></blockquote></blockquote><pre><code></code></pre></li></ul><p>x</p><pre><code></code></pre>",
        ),
        (
            "- a\n  > - > +\n# h\n",
            "<ul><li>a<blockquote><ul><li><blockquote><p>+</p></blockquote></li></ul></blockquote></li></ul><sectionid=\"h\"><h1>h</h1></section>",
        ),
        (
            "- a\n  > - > +\nb\n",
            "<ul><li>a<blockquote><ul><li><blockquote><p>+b</p></blockquote></li></ul></blockquote></li></ul>",
        ),
        (
            "> > - a\n> > - > +\n# h\n",
            "<blockquote><blockquote><ul><li>a</li><li><blockquote><p>+</p></blockquote></li></ul></blockquote></blockquote><sectionid=\"h\"><h1>h</h1></section>",
        ),
        (
            "> - a\n> > q\n> - > +\n# h\n",
            "<blockquote><ul><li>a</li></ul><blockquote><p>q-&gt;+</p></blockquote></blockquote><sectionid=\"h\"><h1>h</h1></section>",
        ),
        (
            "> > q\n> - > +\n# h\n",
            "<blockquote><blockquote><p>q-&gt;+</p></blockquote></blockquote><sectionid=\"h\"><h1>h</h1></section>",
        ),
    ]);
    let lint = carve::lint_carve("- a\n  > - > +\n# h\n");
    assert!(lint
        .iter()
        .any(|warning| warning.rule == "continuation-marker-refused"));
}

#[test]
fn a_deep_quote_plus_and_a_misaligned_host_prefix_read_like_carve_js() {
    check(&[
        (
            "> > > > +\n> b\n",
            "<blockquote><blockquote><blockquote><blockquote><p>+b</p></blockquote></blockquote></blockquote></blockquote>",
        ),
        (
            "> > > > +\n> # h\n",
            "<blockquote><blockquote><blockquote><blockquote><p>+</p></blockquote></blockquote></blockquote><h1id=\"h\">h</h1></blockquote>",
        ),
        (
            "> > > +\n> b\n",
            "<blockquote><blockquote><blockquote><p>+b</p></blockquote></blockquote></blockquote>",
        ),
        (
            "> > - > +\n>  > q\n",
            "<blockquote><blockquote><ul><li><blockquote><p>+q</p></blockquote></li></ul></blockquote></blockquote>",
        ),
        (
            "> > - > +\n> > q\n",
            "<blockquote><blockquote><ul><li><blockquote><p>+q</p></blockquote></li></ul></blockquote></blockquote>",
        ),
    ]);
}
