//! The minimal/conservative comparison (PART 11 section 4 W3) has to see every
//! node that carries inline children, or an escape inside one makes the two
//! renders differ and escalates the WHOLE document to conservative escaping.
//!
//! Editorial insert and delete were missing, so `{++a++}{.a}` came back
//! `{+\+a\++}{.a}` -- over-escaping content the HTML target shows as a literal
//! `+a+` (carve#352, corpus 126).
//!
//! Third instance of the same shape: footnote definitions (carve-rs#309), inline
//! extensions (carve-rs#310), and now these. All three were hidden by a `_ => {}`
//! catch-all, which is now gone: the match lists the childless variants, so a new
//! node type with children fails to compile instead of quietly over-escaping.

#[test]
fn an_escape_inside_an_editorial_span_does_not_escalate() {
    assert_eq!(carve::to_carve("{++a++}{.a}\n"), "{++a++}{.a}\n");
}

#[test]
fn a_document_full_of_editorial_markup_stays_minimal() {
    let src = "a {+ins+} {-del-} {~old~>new~} b{# note #}\n";
    assert_eq!(carve::to_carve(src), src);
}

#[test]
fn the_content_is_what_the_html_target_shows() {
    // `{++a++}` is an insert whose CONTENT is `+a+`; the inner plus signs are not
    // delimiters, which is why escaping them was wrong.
    assert!(carve::to_html("{++a++}{.a}\n").contains(">+a+<"));
}

#[test]
fn an_escape_that_is_needed_inside_one_still_survives() {
    // The fix must not turn escalation off wholesale: a doubled hyphen would
    // re-derive as an en dash, so its escape is load-bearing.
    let src = "{+literal \\-\\- dashes+}\n";
    let out = carve::to_carve(src);
    assert!(out.contains("\\-\\-"), "escape was dropped: {out:?}");
    assert_eq!(carve::to_html(&out), carve::to_html(src));
    assert_eq!(carve::to_carve(&out), out, "fmt is not idempotent");
}
