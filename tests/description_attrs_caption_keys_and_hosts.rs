//! An attributed description paragraph, a caption label's bucket key and a
//! comment-only quote as a caption host.

fn html(source: &str) -> String {
    carve::to_html(source).trim().to_owned()
}

#[test]
fn a_description_keeps_its_attributed_paragraph() {
    assert_eq!(
        html(":: T\n: {.c}\n  q"),
        "<dl>\n  <dt>T</dt>\n  <dd><p class=\"c\">q</p></dd>\n</dl>"
    );
    assert_eq!(
        html(":: T\n: +\n{.c}\nq"),
        "<dl>\n  <dt>T</dt>\n  <dd><p class=\"c\">q</p></dd>\n</dl>"
    );
    let stamped = carve::to_html_with_options(
        ":: T\n: q\n",
        &carve::Options::new().with_source_lines(true),
    );
    assert!(
        stamped.contains("<dd data-source-line=\"2\">q</dd>"),
        "{stamped}"
    );
    assert_eq!(html(":: T\n: q"), "<dl>\n  <dt>T</dt>\n  <dd>q</dd>\n</dl>");
}

#[test]
fn a_symbol_counts_toward_a_caption_label_key_and_a_comment_does_not() {
    let rendered = html("||\n^ Fig #: a\n\n||\n^ Fig :smile: #: b\n\n||\n^ Fig {#c#} #: c");
    assert!(
        rendered.contains("<caption>Fig 1: a</caption>"),
        "{rendered}"
    );
    assert!(
        rendered.contains("<caption>Fig :smile: 1: b</caption>"),
        "{rendered}"
    );
    assert!(
        rendered.contains("<caption>Fig <span class=\"critic-comment\">c</span> 2: c</caption>"),
        "{rendered}"
    );
}

#[test]
fn a_comment_only_quote_is_no_caption_host() {
    for source in [
        "> %%\n^ cap",
        "> %%%\n> x\n> %%%\n^ cap",
        "> {.c}\n> %%\n^ cap",
    ] {
        let rendered = html(source);
        assert!(!rendered.contains("<figure>"), "{source:?}: {rendered}");
        assert!(rendered.contains("<p>^ cap</p>"), "{source:?}: {rendered}");
    }
    assert!(html("> a %% c\n^ cap").contains("<figure>"));
}
