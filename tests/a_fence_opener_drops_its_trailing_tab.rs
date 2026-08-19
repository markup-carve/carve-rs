//! A code fence opener whose tab sits at the END of the line opens normally;
//! one whose tab sits BEFORE the info string still opens nothing
//! (markup-carve/carve#1295, corpus `330-...-2`).
//!
//! Two clauses meet on this line and POSITION decides which governs, not the
//! construct. A tab before content is the marker-to-content separator, which is
//! the `space` terminal and nothing else - the rule the definition, heading,
//! list and task markers already carry. A tab at the end of the line with
//! nothing after it never reaches that slot: it is trailing whitespace on a
//! content line, PART 2 drops it, and what is left is the bare opener.
//!
//! THE TWO ROWS MUST MOVE INDEPENDENTLY, which is what makes the pair the test
//! rather than either case alone. A fix that simply accepted a tab in the
//! separator slot passes the first row and breaks the second, and a fix that
//! left trailing whitespace alone does the reverse. Trailing SPACES already had
//! a path through - they are eaten further down - so only the tab was stranded.
//!
//! carve-php implements this (markup-carve/carve-php#1340) and carve-js in
//! markup-carve/carve-js#1133. The frontmatter half of the same spec section
//! already answered correctly here and is pinned below as a control, so this
//! change is scoped to the fence opener; the fence CLOSER is a separate site
//! and was fixed in markup-carve/carve-rs#1040.

fn html(src: &str) -> String {
    carve::to_html(src).trim().to_string()
}

#[test]
fn a_trailing_tab_leaves_the_bare_opener() {
    // The corpus document. The tab is the last byte on the opener line.
    assert_eq!(html("```\t\nx\n```\n"), "<pre><code>x\n</code></pre>");
}

#[test]
fn a_tab_before_the_info_string_still_opens_nothing() {
    // The row that must NOT move. The separator is a space and nothing else, so
    // the backtick run is an ordinary inline verbatim run reaching the end of
    // the block.
    assert_eq!(html("```\tphp\nx\n```\n"), "<p><code>\tphp\nx\n</code></p>");
}

#[test]
fn a_tilde_fence_answers_the_same_way() {
    // The rule is the separator's, not the backtick's.
    assert_eq!(html("~~~\t\nx\n~~~\n"), "<pre><code>x\n</code></pre>");
}

// ---------------------------------------------------------------------------
// CONTROLS. Every one of these passed before the fix and must go on passing.
// ---------------------------------------------------------------------------

#[test]
fn control_a_bare_opener_is_unchanged() {
    assert_eq!(html("```\nx\n```\n"), "<pre><code>x\n</code></pre>");
}

#[test]
fn control_trailing_spaces_were_already_dropped() {
    // The path the tab did not have. This is why the defect looked like a
    // whitespace rule that was already implemented.
    assert_eq!(html("```  \nx\n```\n"), "<pre><code>x\n</code></pre>");
}

#[test]
fn control_one_space_then_a_language_still_reads_the_language() {
    assert_eq!(
        html("``` php\nx\n```\n"),
        "<pre><code class=\"language-php\">x\n</code></pre>"
    );
}

#[test]
fn control_a_language_with_a_trailing_space_keeps_the_language() {
    // The trailing run is dropped without eating the token before it.
    assert_eq!(
        html("``` php \nx\n```\n"),
        "<pre><code class=\"language-php\">x\n</code></pre>"
    );
}

#[test]
fn control_the_frontmatter_opener_pair_is_unchanged() {
    // The other half of the spec section, which this engine already answered.
    // A trailing tab opens frontmatter, which renders nothing.
    assert_eq!(html("---\t\ntitle: x\n---\n\nbody\n"), "<p>body</p>");
    // And a tab before the format token is the separator, so the line is not a
    // delimiter and no frontmatter is consumed.
    assert_eq!(
        html("---\tyaml\ntitle: x\n---\n\nbody\n"),
        "<p>\u{2014}\tyaml\ntitle: x</p>\n<hr>\n<p>body</p>"
    );
}
