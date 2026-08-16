//! A tab on a delimiter line is decided by WHERE it sits, not by which
//! construct the line opens (markup-carve/carve#1295).
//!
//! - A tab BEFORE content is a MARKER SEPARATOR. The terminal is `space` alone,
//!   a tab does not satisfy it, and the construct is not recognized.
//! - A tab with NOTHING after it is TRAILING. PART 2's NO TRAILING WHITESPACE
//!   drops it - the run there is `whitespace`, `' ' | '\t'` - so it is not
//!   content and the construct is recognized normally.
//!
//! The two clauses never overlap, so neither needs an exception. Two lines in
//! this language take no content at all and therefore always land on the
//! trailing side: a FENCE CLOSER and a FRONTMATTER DELIMITER. Both were refused
//! here.
//!
//! The frontmatter half also removes an internal inconsistency: `---<TAB>` was
//! not a frontmatter delimiter and still a thematic break, so the same trailing
//! tab disqualified one construct on that line and not the other.
//!
//! The OPENER is the control and does not move. `` ```<TAB>php `` has content
//! after the tab, so the separator clause governs it and it still opens
//! nothing.

use carve::to_html;

fn parse(source: &str) -> carve::ast::Document {
    carve::parse(source)
}

// ---------------------------------------------------------------------------
// The fence closer.
// ---------------------------------------------------------------------------

/// A tab-padded closer closes the fence. Without this the closer is swallowed
/// as content and the block runs to end of input.
#[test]
fn a_tab_padded_closer_closes_the_fence() {
    assert_eq!(
        to_html("```\nx\n```\t\n"),
        "<pre><code>x\n</code></pre>",
        "a trailing tab is dropped, so the line is a bare closer"
    );
}

/// CONTROL: a space-padded closer was already accepted and stays accepted. The
/// change widens the terminal to `whitespace`; it does not move the space.
#[test]
fn a_space_padded_closer_still_closes_the_fence() {
    assert_eq!(to_html("```\nx\n``` \n"), "<pre><code>x\n</code></pre>");
}

/// The tilde fence is the same production, so it takes the same tail.
#[test]
fn a_tilde_fence_takes_a_tab_padded_closer_too() {
    assert_eq!(to_html("~~~\nx\n~~~\t\n"), "<pre><code>x\n</code></pre>");
}

/// THE CLOSER INDEX makes the same test, and a `false` from it is FINAL - the
/// exact scan never re-examines a line the index rejected. A fence that has to
/// INTERRUPT an open paragraph asks the index first, so this shape fails while
/// the closer itself is already right.
///
/// The INFO STRING is what makes the case reach the index at all: a bare
/// ```` ``` ```` opener is closer-shaped itself and seeds the index on its own
/// line, which hides a narrow prefilter. ```` ```php ```` is not, so the
/// tab-padded closer is the only entry there is.
#[test]
fn a_tab_padded_closer_lets_the_fence_interrupt_a_paragraph() {
    assert_eq!(
        to_html("para\n```php\nx\n```\t\n"),
        "<p>para</p>\n<pre><code class=\"language-php\">x\n</code></pre>",
        "the index refused the closer, so the fence never interrupted"
    );
}

/// CONTROL: the same shape with a space-padded closer, which the index took
/// before this change too.
#[test]
fn a_space_padded_closer_lets_the_fence_interrupt_a_paragraph() {
    assert_eq!(
        to_html("para\n```php\nx\n``` \n"),
        "<p>para</p>\n<pre><code class=\"language-php\">x\n</code></pre>"
    );
}

/// A tab with CONTENT after it is not a closer at all - the separator half of
/// the rule, seen from the closing end. The line stays inside the block.
#[test]
fn a_tab_before_content_does_not_close_a_fence() {
    assert_eq!(
        to_html("```\nx\n```\tphp\n"),
        "<pre><code>x\n```\tphp\n</code></pre>",
        "the tail is not whitespace, so the line is body text"
    );
}

/// CONTROL: the OPENER is untouched. Its tab sits before an info string, where
/// the separator clause governs and the terminal is `space` alone, so the fence
/// does not open and the invalid-fence fallback applies.
#[test]
fn an_opener_with_a_tab_before_its_info_string_still_refuses() {
    assert_eq!(
        to_html("```\tphp\nx\n```\n"),
        "<p><code>\tphp\nx\n</code></p>",
        "the opener half of the ruling is unchanged"
    );
}

// ---------------------------------------------------------------------------
// The frontmatter delimiter.
// ---------------------------------------------------------------------------

/// A frontmatter opener takes no content on its line, so its tab is trailing
/// and the block opens.
#[test]
fn a_tab_padded_frontmatter_opener_opens_the_block() {
    assert_eq!(to_html("---\t\ntitle: x\n---\n\nbody\n"), "<p>body</p>");
}

/// And the block is really consumed as frontmatter, not merely rendered away:
/// the key reaches the document's metadata and the raw block records the
/// default format.
#[test]
fn the_tab_padded_opener_yields_a_frontmatter_block() {
    let doc = parse("---\t\ntitle: x\n---\n\nbody\n");
    assert_eq!(doc.frontmatter.get("title").map(String::as_str), Some("x"));
    let raw = doc
        .frontmatter_raw
        .as_ref()
        .expect("the block is recorded as written");
    assert_eq!(raw.format, "yaml", "a bare fence defaults to yaml");
    assert_eq!(raw.content, "title: x");
}

/// THE INCONSISTENCY THIS REMOVES. `---<TAB>` used to be no frontmatter
/// delimiter and a thematic break all the same, so one trailing tab
/// disqualified one construct on that line and not the other. It is now a
/// delimiter, and nothing on the line renders.
#[test]
fn a_tab_padded_opener_is_no_longer_a_thematic_break_as_well() {
    let html = to_html("---\t\ntitle: x\n---\n\nbody\n");
    assert!(
        !html.contains("<hr>"),
        "the line is a delimiter, not a rule: {html}"
    );
}

/// CONTROL: a space-padded opener was already accepted and stays accepted.
#[test]
fn a_space_padded_frontmatter_opener_still_opens_the_block() {
    assert_eq!(to_html("--- \ntitle: x\n---\n\nbody\n"), "<p>body</p>");
}

/// CONTROL: with a FORMAT TOKEN after it the tab is a separator again, and the
/// line is not a typed opener - the metadata lines fold into it as prose.
#[test]
fn a_tab_before_the_format_token_still_refuses() {
    let doc = parse("---\tyaml\ntitle: x\n---\n\nbody\n");
    assert!(
        doc.frontmatter_raw.is_none(),
        "a tab before the token is a separator, so no block opens"
    );
}

/// CONTROL: a NO-BREAK SPACE is content, not whitespace, at either position -
/// it does not open a bare block and it does not separate a token.
#[test]
fn a_no_break_space_after_the_marker_is_still_content() {
    let doc = parse("---\u{a0}\ntitle: x\n---\n\nbody\n");
    assert!(
        doc.frontmatter_raw.is_none(),
        "U+00A0 is not `whitespace`, so it is content and no block opens"
    );
}
