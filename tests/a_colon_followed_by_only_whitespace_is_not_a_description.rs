//! A description marker takes a separator space AND non-empty content, so a `:`
//! line carrying nothing but whitespace opens no description
//! (markup-carve/carve#1830).
//!
//! It is a plain line under whatever is open, which folds it as a soft break
//! and drops the line's own trailing run. That makes the space spellings
//! identical to the bare `:` line and to the tab spelling rather than merely
//! similar.
//!
//! THE TWO HALVES REACH THE SAME PLACE FROM DIFFERENT DIRECTIONS. A MARKER
//! SEPARATOR is spelled `space` and a tab never satisfies it (PART 1), which is
//! why `:` followed straight by a tab already folded here (carve-rs#518). What
//! was missing is MARKER REQUIRES CONTENT (PART 2) - a separator that IS a
//! space, followed by nothing - so the space spellings ended the list and
//! emitted the colon as their own paragraph instead.
//!
//! carve-php is the reference reading and agrees on every shape below.

/// Whitespace-collapsed, because the interesting difference is the STRUCTURE:
/// whether the colon folded into the open block or became a paragraph beside a
/// closed list.
fn flat(source: &str) -> String {
    carve::to_html(source)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

const FOLDED: &str = "<dl> <dt>t : x</dt> </dl>";

#[test]
fn a_colon_plus_spaces_folds_into_the_term() {
    for source in [":: t\n: \nx\n", ":: t\n:  \nx\n", ":: t\n:   \nx\n"] {
        assert_eq!(flat(source), FOLDED, "source: {source:?}");
    }
}

/// THE CONTROLS THAT MAKE THE ROWS ABOVE MEAN SOMETHING. Both already read this
/// way, so the space spellings are routed to a branch that exists - and they
/// have to land on the SAME output, not merely on a definition list.
#[test]
fn it_reads_exactly_as_the_tab_and_bare_spellings_do() {
    assert_eq!(flat(":: t\n:\t\nx\n"), FOLDED);
    assert_eq!(flat(":: t\n:\n\nx\n"), "<dl> <dt>t :</dt> </dl> <p>x</p>");
    assert_eq!(flat(":: t\n:\nx\n"), FOLDED);
}

/// A marker WITH content still opens a description, so it is the content test
/// that decides and not the colon.
#[test]
fn a_marker_that_has_content_still_opens_a_description() {
    assert_eq!(flat(":: t\n: {}\nx\n"), "<dl> <dt>t</dt> <dd>{} x</dd> </dl>");
    assert_eq!(flat(":: t\n: y\nx\n"), "<dl> <dt>t</dt> <dd>y x</dd> </dl>");
    assert_eq!(flat(":: t\n:  y\nx\n"), "<dl> <dt>t</dt> <dd>y x</dd> </dl>");
}

/// CONTENT IS NOT THE SAME QUESTION AS VISIBLE TEXT. PART 7's one whitespace
/// definition makes a vertical tab content, and a no-break space is content
/// everywhere else in the language, so both open a description.
#[test]
fn a_body_of_one_content_space_is_content() {
    assert_eq!(flat(":: t\n: \u{0b}\nx\n"), "<dl> <dt>t</dt> <dd> x</dd> </dl>");
    assert_eq!(
        flat(":: t\n: \u{a0}\nx\n"),
        "<dl> <dt>t</dt> <dd>&nbsp; x</dd> </dl>"
    );
}

/// THE TRAILING RUN IS DROPPED, and the TREE is where that shows: a trailing
/// run before a soft break does not reach the HTML either way, so an
/// HTML-only assertion here would pass on a term that kept it.
#[test]
fn the_folded_line_keeps_no_trailing_run() {
    for source in [":: t\n: \n", ":: t\n:  \n", ":: t\n:   \n", ":: t\n:\t\n", ":: t\n:\n"] {
        let json = carve::ast_json::to_json(&carve::parse(source));
        assert!(
            json.contains("\"definition_list\""),
            "source: {source:?}, json: {json}"
        );
        assert!(
            json.contains("\\\"value\\\":\\\":\\\"") || json.contains("\"value\":\":\""),
            "the folded text is exactly the colon - source: {source:?}, json: {json}"
        );
        for kept in ["\"value\":\": \"", "\"value\":\":  \"", "\"value\":\":\\t\""] {
            assert!(
                !json.contains(kept),
                "trailing run survived {kept} - source: {source:?}"
            );
        }
    }
}

/// A DESCRIPTION IS THE OTHER HOST. The line folds into whatever block is open
/// under the term, so an already-open description takes it too.
#[test]
fn it_folds_into_an_open_description() {
    assert_eq!(flat(":: t\n: d\n: \nx\n"), "<dl> <dt>t</dt> <dd>d : x</dd> </dl>");
}

#[test]
fn it_is_a_paragraph_with_no_term_open_above_it() {
    assert_eq!(flat(": \nx\n"), "<p>: x</p>");
}

/// THE TERM MARKER IS UNTOUCHED. `::` plus whitespace still closes the list,
/// and `::` followed straight by a tab still folds - this engine already read
/// both the way carve-php does, and narrowing the description marker must not
/// reach them.
#[test]
fn the_term_marker_keeps_its_own_readings() {
    assert_eq!(flat(":: t\n:: \nx\n"), "<dl> <dt>t</dt> </dl> <p>:: x</p>");
    assert_eq!(flat(":: t\n:: \t\nx\n"), "<dl> <dt>t</dt> </dl> <p>:: x</p>");
    assert_eq!(flat(":: t\n::\t\nx\n"), "<dl> <dt>t :: x</dt> </dl>");
    assert_eq!(flat(":: t\n::\nx\n"), "<dl> <dt>t :: x</dt> </dl>");
}
