//! A tab and four spaces reach the same column past a definition body, and are
//! therefore read the same way (carve-rs#793).
//!
//! A recognized opener past the minimum establishes an authored block base
//! (markup-carve/carve#1729). PART 9 §24 C1 gives a tab a COLUMN VALUE, so a
//! bare tab reaches column 4 exactly as four spaces do.
//!
//! The form-A dedent asked `slice_columns` for three columns and did not keep
//! the residual, so the tab - one codepoint spanning four columns - was
//! consumed whole and the residue landed FLUSH LEFT, where a `>` is a block
//! opener. Same column, two answers.
//!
//! The tab characters here are built from `\t` escapes in the literal, never
//! typed: a tab in a fixture file is invisible and an editor that expands it
//! turns each of these into a duplicate of its space control.

fn html(src: &str) -> String {
    carve::to_html(src)
}

/// What every spelling at or past the minimum produces for a block opener.
const OPENS: &str =
    "<dl>\n  <dt>t</dt>\n  <dd>\n    <p>body</p>\n    <blockquote><p>q</p></blockquote>\n  </dd>\n</dl>";

#[test]
fn the_tab_literals_really_are_tabs() {
    // The guard the cases below stand on. If an editor or a rewrite expands
    // these, this fails first and names the reason rather than letting each
    // case quietly become a copy of its space control.
    assert_eq!("\t".as_bytes(), &[0x09]);
    for src in [":: t\n:  body\n\t> q\n", ":: t\n:  body\n \t> q\n"] {
        assert!(src.contains('\t'), "{src:?} lost its tab");
    }
}

#[test]
fn a_bare_tab_opens_at_the_same_authored_base_as_four_spaces() {
    // The reported document. A tab reaches column 4; so do four spaces.
    assert_eq!(html(":: t\n:  body\n\t> q\n"), OPENS);
    assert_eq!(html(":: t\n:  body\n    > q\n"), OPENS);
}

#[test]
fn a_space_then_a_tab_reaches_the_same_column_and_answers_the_same_way() {
    // One space then a tab is columns 1 -> 4, the same stop. This spelling
    // answered like the bare tab before the fix and like four spaces after it,
    // so it moves with the case above rather than being a separate rule.
    assert_eq!(html(":: t\n:  body\n \t> q\n"), OPENS);
}

#[test]
fn at_the_column_a_block_still_opens() {
    // CONTROL, and the boundary. Three spaces is AT the body's column, which is
    // the band that opens a block inside the description - the answer the fix
    // must not reach.
    assert_eq!(html(":: t\n:  body\n   > q\n"), OPENS);
}

#[test]
fn further_past_the_column_still_opens_at_the_authored_base() {
    assert_eq!(html(":: t\n:  body\n     > q\n"), OPENS);
    assert_eq!(html(":: t\n:  body\n\t > q\n"), OPENS);
}

#[test]
fn ordinary_indented_body_text_still_folds() {
    // CONTROL for the dedent itself, which every definition body goes through.
    // A tab-indented continuation line that is plain text is body text either
    // way, so this passes before and after - it is here to catch a fix that
    // narrows the dedent instead of correcting its residual.
    assert_eq!(
        html(":: t\n:  body\n\tmore\n"),
        "<dl>\n  <dt>t</dt>\n  <dd>body\nmore</dd>\n</dl>"
    );
    assert_eq!(
        html(":: t\n:  body\n   more\n"),
        "<dl>\n  <dt>t</dt>\n  <dd>body\nmore</dd>\n</dl>"
    );
}

/// The `(startLine, startColumn)` of the first text node whose value is `want`.
fn text_pos(src: &str, want: &str) -> (usize, usize) {
    let options = carve::Options {
        positions: true,
        ..Default::default()
    };
    let doc = carve::parse_with_options(src, &options);
    let carve::ast::BlockNode::DefinitionList(list) = &doc.children[0] else {
        panic!("the fixture did not parse as a definition list");
    };
    for item in &list.items {
        for def in &item.definitions {
            for block in &def.children {
                if let carve::ast::BlockNode::Paragraph(p) = block {
                    for node in &p.children {
                        if let carve::ast::InlineNode::Text(t) = node {
                            if t.value == want {
                                let pos = t.pos.as_ref().expect("a position");
                                return (pos.start_line, pos.start_column);
                            }
                        }
                    }
                }
            }
        }
    }
    panic!("no text node {want:?} with a position")
}

#[test]
fn the_dedented_line_still_indexes_the_original_file() {
    // PART 12 §4: positions index the ORIGINAL file, and a tab is ONE
    // character there however many columns it buys. The dedent counts what it
    // actually removed rather than assuming three columns, which is the half of
    // the fix the HTML cannot see - both spellings render the same text and
    // only the span moves.
    assert_eq!(
        text_pos(":: t\n:  body\n\tmore text\n", "more text"),
        (3, 2)
    );
    assert_eq!(
        text_pos(":: t\n:  body\n   more text\n", "more text"),
        (3, 4)
    );
    assert_eq!(
        text_pos(":: t\n:  body\n    more text\n", "more text"),
        (3, 5)
    );
}

#[test]
fn the_round_trip_holds_for_every_spelling() {
    // PART 11 §1. The writer has to reproduce whatever these documents mean,
    // and a dedent that loses columns is exactly the shape that breaks it.
    for src in [
        ":: t\n:  body\n\t> q\n",
        ":: t\n:  body\n \t> q\n",
        ":: t\n:  body\n   > q\n",
        ":: t\n:  body\n    > q\n",
        ":: t\n:  body\n\tmore\n",
    ] {
        let formatted = carve::to_carve(src);
        assert_eq!(
            carve::to_html(&formatted),
            carve::to_html(src),
            "fmt changed what {src:?} means"
        );
        assert_eq!(carve::to_carve(&formatted), formatted, "fmt not idempotent");
    }
}
