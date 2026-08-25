//! A recognized block opener written over-indented inside a body that HAS a
//! minimum content column is that body's block, not lazy text folding into
//! whatever the line above left open.
//!
//! PART 9 §24 C3's authored-base clause states it without asking for a
//! paragraph: "A recognized block opener AT OR PAST a definition body's
//! column 3 or a footnote body's column 2 belongs to that body. Its authored
//! visual column is the local `block_base` for that one block."
//!
//! The engine asked for a paragraph anyway, so a quote, a heading or a table
//! written at the body's own column left the opener below it unrebased - and a
//! quote then swallowed it as a lazy continuation (carve-rs#1415). carve-js and
//! carve-php open the block in every shape below.

fn html(source: &str) -> String {
    carve::to_html(source).trim().to_string()
}

#[test]
fn a_block_opener_below_a_quote_line_in_a_footnote_body_opens() {
    for (body, expected) in [
        (
            "::: >\n      b\n      :::",
            "<blockquote><p>b</p></blockquote>",
        ),
        ("# h", "<h1 id=\"h\">h</h1>"),
        ("| A |", "<table>"),
    ] {
        let output = html(&format!("[^a]: > q\n      {body}\n\nsee[^a]\n"));
        assert!(
            output.contains("<blockquote><p>q</p></blockquote>"),
            "{body:?}: {output}"
        );
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
fn a_run_of_openers_rebases_every_member() {
    let output = html("[^a]: > q\n      # h\n      # i\n\nsee[^a]\n");
    assert!(output.contains("<h1 id=\"h\">h</h1>"), "{output}");
    assert!(output.contains("<h1 id=\"i\">i</h1>"), "{output}");
}

#[test]
fn a_definition_body_and_a_list_item_spell_the_same_rule() {
    for source in [":: t\n:  > q\n       # h\n", "- > q\n      # h\n"] {
        let output = html(source);
        assert!(
            output.contains("<blockquote><p>q</p></blockquote>"),
            "{source:?}: {output}"
        );
        assert!(
            output.contains("<h1 id=\"h\">h</h1>"),
            "{source:?}: {output}"
        );
    }
}

#[test]
fn an_over_indented_line_that_opens_nothing_still_folds() {
    // The other side of the boundary. Only a RECOGNIZED opener takes an
    // authored base; ordinary over-indented text is still the lazy
    // continuation it always was.
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
