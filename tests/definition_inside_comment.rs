//! A definition inside a `%%%` comment fence registers nothing (carve-rs#504).
//!
//! The definition pre-passes already tracked the comment fence, but only to
//! gate the line-block opener - they still walked into the body and registered
//! from it. `%%%` / `[^a]: note` / `%%%` therefore produced an endnote nobody
//! wrote, plus a live reference for a later `see [^a]`.
//!
//! A comment renders nothing, so a definition inside one defines nothing.
//! carve-js has never registered there and carve-php stopped in carve-php#698;
//! this was the third engine.

fn html(src: &str) -> String {
    carve::to_html(src)
}

#[test]
fn a_footnote_definition_inside_a_comment_does_not_register() {
    let out = html("%%%\n[^a]: note\n%%%\n\nsee [^a]\n");
    assert!(!out.contains("doc-noteref"), "registered a footnote: {out}");
    assert!(!out.contains("doc-endnotes"), "emitted an endnote: {out}");
    assert!(
        out.contains("see [^a]"),
        "reference should stay literal: {out}"
    );
}

#[test]
fn a_link_reference_definition_inside_a_comment_does_not_register() {
    let out = html("%%%\n[a]: /u\n%%%\n\nsee [a][]\n");
    assert!(!out.contains("href=\"/u\""), "resolved a link: {out}");
}

#[test]
fn an_abbreviation_definition_inside_a_comment_does_not_register() {
    let out = html("%%%\n*[HTML]: HyperText\n%%%\n\nHTML\n");
    assert!(!out.contains("<abbr"), "expanded an abbreviation: {out}");
}

#[test]
fn a_definition_after_a_closed_comment_still_registers() {
    // The state must END at the closer - the point of the fix is to skip the
    // body, not to swallow the rest of the document.
    let out = html("%%%\ncomment\n%%%\n\n[^a]: note\n\nsee [^a]\n");
    assert!(
        out.contains("doc-noteref"),
        "should resolve after the comment: {out}"
    );
}

#[test]
fn an_unterminated_comment_fence_does_not_swallow_a_later_definition() {
    // An unterminated `%%%` degrades to a single-line comment, so the state is
    // never entered and what follows is ordinary document.
    let out = html("%%%\n\n[^a]: note\n\nsee [^a]\n");
    assert!(out.contains("doc-noteref"), "should resolve: {out}");
}
