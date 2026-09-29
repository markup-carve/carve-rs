//! A surviving quote owns its unmarked paragraph continuations. Other blocks
//! at the host's minimum column allow a following opener to take its authored
//! base (carve-rs#2183).

fn html(source: &str) -> String {
    carve::to_html(source).trim().to_string()
}

#[test]
fn a_block_opener_below_an_open_quote_paragraph_folds() {
    for (body, expected) in [
        (
            "::: >\n      b\n      :::",
            "<blockquote><p>q\n::: &gt;\nb\n:::</p></blockquote>",
        ),
        ("# h", "<blockquote><p>q\n# h</p></blockquote>"),
        ("| A |", "<blockquote><p>q\n| A |</p></blockquote>"),
    ] {
        let output = html(&format!("[^a]: > q\n      {body}\n\nsee[^a]\n"));
        assert!(output.contains(expected), "{body:?}: {output}");
    }
}

#[test]
fn the_line_above_does_not_have_to_be_a_quote() {
    // A heading and a table close themselves, so nothing folds - but the
    // opener below them was left at its authored column all the same, and a
    // block that never opened rendered as literal paragraph text.
    for above in ["# a", "| A |"] {
        let output = html(&format!("[^a]: {above}\n      # h\n\nsee[^a]\n"));
        assert!(
            output.contains("<h1 id=\"h\">h</h1>"),
            "{above:?}: {output}"
        );
        assert!(!output.contains("<p># h"), "{above:?}: {output}");
    }
}

#[test]
fn a_run_of_openers_keeps_folding_into_the_quote() {
    let output = html("[^a]: > q\n      # h\n      # i\n\nsee[^a]\n");
    assert!(
        output.contains("<blockquote><p>q\n# h\n# i</p></blockquote>"),
        "{output}"
    );
}

#[test]
fn a_definition_body_and_a_list_item_spell_the_same_rule() {
    for source in [":: t\n:  > q\n       # h\n", "- > q\n      # h\n"] {
        let output = html(source);
        assert!(
            output.contains("<blockquote><p>q\n# h</p></blockquote>"),
            "{source:?}: {output}"
        );
    }
}

#[test]
fn an_over_indented_line_that_opens_nothing_still_folds() {
    for source in [
        "[^a]: > q\n      plain\n\nsee[^a]\n",
        ":: t\n:  > q\n       plain\n",
        "- > q\n      plain\n",
    ] {
        let output = html(source);
        assert!(
            output.contains("<blockquote><p>q\nplain</p></blockquote>"),
            "{source:?}: {output}"
        );
    }
}

#[test]
fn at_the_top_level_an_indented_opener_still_folds() {
    // The near miss one container out. The document has no minimum content
    // column, so an indented line carries no residual indent to rebase and
    // stays the lazy continuation §24 C3 leaves it as - which is what all
    // three engines already read here.
    for source in ["> q\n  # h\n", "> q\n    # h\n"] {
        assert_eq!(
            html(source),
            "<blockquote><p>q\n# h</p></blockquote>",
            "{source:?}"
        );
    }
}
