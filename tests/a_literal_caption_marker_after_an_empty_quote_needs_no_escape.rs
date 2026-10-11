use carve::{to_carve, to_html};

#[test]
fn an_empty_quote_leaves_the_literal_caret_unescaped() {
    for (source, expected) in [
        (">\n^ cap\n", ">\n\n^ cap\n"),
        (">\n\n^ cap\n", ">\n\n^ cap\n"),
        ("| a |\n\n>\n\n^ cap\n", "| a |\n\n>\n\n^ cap\n"),
        (">\n>\n\n^ cap\n", ">\n\n^ cap\n"),
    ] {
        let formatted = to_carve(source);
        assert_eq!(formatted, expected);
        assert_eq!(to_carve(&formatted), formatted);
        assert_eq!(to_html(&formatted), to_html(source));
    }
}

#[test]
fn a_nonempty_quote_keeps_its_authored_caption() {
    let source = "> q\n>\n\n^ cap\n";
    let formatted = to_carve(source);
    assert_eq!(formatted, "> q\n^ cap\n");
    assert_eq!(to_html(&formatted), to_html(source));
    assert_eq!(to_carve(&formatted), formatted);
}

#[test]
fn caption_hosts_keep_the_required_caret_escape() {
    for source in [
        "> q\n\n\\^ cap\n",
        "```\n```\n\n\\^ cap\n",
        "::: >\n:::\n\n\\^ cap\n",
    ] {
        let formatted = to_carve(source);
        assert!(formatted.contains("\\^ cap"));
        assert_eq!(to_html(&formatted), to_html(source));
        assert_eq!(to_carve(&formatted), formatted);
    }
}
