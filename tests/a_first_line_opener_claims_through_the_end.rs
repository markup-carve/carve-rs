//! carve#5016: a first-line `---` (bare or typed) opens frontmatter that ends at
//! its first bare closer or at the end of the document.

#[test]
fn an_unclosed_opener_renders_nothing() {
    for source in [
        "---\nx\n",
        "---\nx",
        "---",
        "---toml\na = 1\n",
        "---\n- a\n\n# T\n",
    ] {
        assert_eq!(carve::to_html(source).trim(), "", "{source:?}");
        assert!(carve::parse(source).frontmatter_raw.is_some(), "{source:?}");
    }
}

#[test]
fn the_unclosed_payload_is_the_frontmatter_content() {
    let doc = carve::parse("---toml\na = 1\nb = 2\n");
    let raw = doc.frontmatter_raw.as_ref().expect("frontmatter");
    assert_eq!(
        (raw.format.as_str(), raw.content.as_str()),
        ("toml", "a = 1\nb = 2")
    );
}

#[test]
fn the_first_closer_still_ends_the_block() {
    assert_eq!(
        carve::to_html("---\nk: v\n---\nbody\n").trim(),
        "<p>body</p>"
    );
}

#[test]
fn the_writer_closes_an_unclosed_block() {
    for (source, written) in [
        ("---\nx\n", "---yaml\nx\n---\n"),
        ("---toml\na = 1", "---toml\na = 1\n---\n"),
    ] {
        let out = carve::to_carve(source);
        assert_eq!(out, written, "{source:?}");
        assert_eq!(carve::to_carve(&out), out, "idempotent: {source:?}");
    }
}

#[test]
fn a_line_that_is_not_an_opener_stays_a_break_or_text() {
    assert_eq!(carve::to_html("----\nx\n").trim(), "<hr>\n<p>x</p>");
    assert_eq!(carve::to_html(" ---\nx\n").trim(), "<hr>\n<p>x</p>");
}

#[test]
fn an_erased_leading_line_does_not_move_the_frontmatter_gate() {
    for lead in ["%% c\n", "  %% c\n", "[x]: /u\n", "*[X]: expanded\n"] {
        for opener in ["---", "---yaml"] {
            let source = format!("{lead}{opener}\nx\n");
            let doc = carve::parse(&source);
            assert!(doc.frontmatter_raw.is_none(), "{source:?}");
            let html = carve::to_html(&source);
            if opener == "---" {
                assert_eq!(html, "<hr>\n<p>x</p>", "{source:?}");
            } else {
                assert_eq!(html, "<p>---yaml\nx</p>", "{source:?}");
            }
        }
    }
}
