//! A FENCED BODY IS NOT A PARAGRAPH, and a definition body is such a container
//! (PART 0 S4, AND A DEFINITION BODY IS SUCH A CONTAINER, markup-carve/carve#956).
//!
//! A fence opened on a `:  ` marker line whose body sits BELOW the body's content
//! column supplies none of the body's indentation, so S1 MATCH PREFIXES stops at
//! the DEFINITION ENTRY and S2 FENCED BODY never fires - S2 wants the innermost
//! MATCHED container to be the body. S4 governs, and its lazy branch continues an
//! open PARAGRAPH, which a verbatim body is not. The containers close, the `dd`
//! holds an EMPTY code block, and the residue re-parses at document level.
//!
//! A definition body is the LAST container kind that collects an indented block,
//! so the answer is not new: the list spelling (corpus 276, markup-carve/carve-rs#772)
//! and the block-quote spelling already give it here. Every case below is
//! asserted BESIDE its list twin, because the claim of the ruling is that the two
//! differ only in which container the walk stops at - a fix that moved the
//! definition somewhere other than where the list already is would satisfy an
//! equality with a golden but not the rule.

fn html(src: &str) -> String {
    carve::to_html(src).trim().to_string()
}

/// Everything after the container in the twin is identical text; only the
/// container itself differs. This lets a case assert the whole document.
fn definition_of(list_html: &str) -> String {
    list_html
        .replace("<ul>\n  <li>", "<dl>\n  <dt>t</dt>\n  <dd>")
        .replace("</li>\n</ul>", "</dd>\n</dl>")
}

// ---------------------------------------------------------------------------
// The rule.
// ---------------------------------------------------------------------------

/// The shape the ruling is stated on. The `dd` keeps an EMPTY code block and
/// `body` re-parses at document level, where the trailing delimiter run is
/// ordinary inline text.
#[test]
fn a_fence_on_the_marker_line_with_a_below_column_body_closes_the_definition() {
    let out = html(":: t\n:  ```\nbody\n```\n");
    assert_eq!(
        out,
        "<dl>\n  <dt>t</dt>\n  <dd>\n    <pre><code>\n</code></pre>\n  </dd>\n</dl>\n<p>body\n<code></code></p>"
    );
    assert_eq!(out, definition_of(&html("- ```\nbody\n```\n")));
}

/// A tilde fence takes the same route, and its unmatched delimiter run stays
/// literal text at document level rather than becoming a code span.
#[test]
fn a_tilde_fence_answers_the_same_way() {
    let out = html(":: t\n:  ~~~\nbody\n~~~\n");
    assert!(out.ends_with("<p>body\n~~~</p>"), "{out}");
    assert_eq!(out, definition_of(&html("- ~~~\nbody\n~~~\n")));
}

/// Seeding the guard from the marker line is not enough on its own: here the
/// body collects a line AT the content column first, so a reader that asks "is a
/// paragraph open" sees one again before the below-column line arrives. The
/// guard is on the OPEN FENCE, not on where the fence was opened.
#[test]
fn a_collected_line_at_the_column_does_not_reopen_the_fold() {
    let out = html(":: t\n:  ```\n   a\nbody\n```\n");
    assert_eq!(
        out,
        "<dl>\n  <dt>t</dt>\n  <dd>\n    <pre><code>a\n</code></pre>\n  </dd>\n</dl>\n<p>body\n<code></code></p>"
    );
    assert_eq!(out, definition_of(&html("- ```\n  a\nbody\n```\n")));
}

/// The fence need not be the body's first block at all. One opened on a
/// CONTINUATION line closes the definition at the same place, which is the half
/// the marker-line seed alone cannot reach.
#[test]
fn a_fence_opened_on_a_continuation_line_closes_the_definition_too() {
    let out = html(":: t\n:  a\n   ```\n   b\nbody\n   ```\n");
    assert_eq!(
        out,
        "<dl>\n  <dt>t</dt>\n  <dd>a\n<code>\nb</code></dd>\n</dl>\n<p>body\n<code></code></p>"
    );
    assert_eq!(out, definition_of(&html("- a\n  ```\n  b\nbody\n  ```\n")));
}

// ---------------------------------------------------------------------------
// Controls. These bound the rule; none of them may move.
// ---------------------------------------------------------------------------

/// AT the body's content column the body is the definition's and nothing leaves
/// it. This is the shape every other definition-list case uses, and it is what an
/// over-eager guard - one that fires on any open fence regardless of the line's
/// column - would break.
#[test]
fn control_a_body_at_the_content_column_stays_in_the_definition() {
    assert_eq!(
        html(":: t\n:  ```\n   body\n   ```\n"),
        "<dl>\n  <dt>t</dt>\n  <dd>\n    <pre><code>body\n</code></pre>\n  </dd>\n</dl>"
    );
}

/// Once the fence CLOSES at the column the guard is spent, and a flush-left line
/// after it folds into the definition exactly as it did before. A guard that
/// never cleared would strand this line at document level.
#[test]
fn control_a_closed_fence_releases_the_flush_left_fold() {
    assert_eq!(
        html(":: t\n:  ```\n   b\n   ```\nlazy\n"),
        "<dl>\n  <dt>t</dt>\n  <dd>\n    <pre><code>b\n</code></pre>\n    <p>lazy</p>\n  </dd>\n</dl>"
    );
}

/// With no fence anywhere, a flush-left line still lazily continues the open
/// paragraph. The guard reaches only bodies holding a verbatim block.
#[test]
fn control_a_plain_definition_still_folds_a_flush_left_line() {
    assert_eq!(
        html(":: t\n:  body\nlazy\n"),
        "<dl>\n  <dt>t</dt>\n  <dd>body\nlazy</dd>\n</dl>"
    );
}

/// The first-block form seeds nothing: its body is the FOLLOWING flush-left
/// block, which supplies no indentation for the rule to measure, so a fence
/// written there is an ordinary fence in that block.
#[test]
fn control_the_first_block_form_is_untouched() {
    assert_eq!(
        html(":: t\n:  +\n```\nbody\n```\n"),
        "<dl>\n  <dt>t</dt>\n  <dd>\n    <pre><code>body\n</code></pre>\n  </dd>\n</dl>"
    );
}

/// A MARKER below the body's column takes the same route as any other line: the
/// entry closes with its empty code block, and only THEN is the residue
/// classified - in the surviving context, which is the definition LIST, so the
/// marker opens the next description on the same term. The list spelling answers
/// identically (a sibling marker at the base column ends the item while its list
/// carries on), which is what makes this the rule rather than an escape from it.
#[test]
fn control_a_below_column_marker_is_classified_in_the_surviving_list() {
    let out = html(":: t\n:  ```\n:  d\n```\n");
    assert_eq!(
        out,
        "<dl>\n  <dt>t</dt>\n  <dd>\n    <pre><code>\n</code></pre>\n  </dd>\n  <dd>d\n<code></code></dd>\n</dl>"
    );
    assert_eq!(
        html("- ```\n- d\n```\n"),
        "<ul>\n  <li>\n    <pre><code>\n</code></pre>\n  </li>\n  <li>d\n<code></code></li>\n</ul>"
    );

    // A TERM marker there opens the next entry, for the same reason.
    assert_eq!(
        html(":: t\n:  ```\n:: u\n:  d\n```\n"),
        "<dl>\n  <dt>t</dt>\n  <dd>\n    <pre><code>\n</code></pre>\n  </dd>\n  <dt>u</dt>\n  <dd>d\n<code></code></dd>\n</dl>"
    );
}

/// The block-quote spelling of the shape, unanimous across the engines and
/// already correct here. It is the other end of the derivation the ruling closes
/// over, so it is asserted rather than assumed.
#[test]
fn control_the_block_quote_spelling_is_unchanged() {
    assert_eq!(
        html("> ```\nbody\n```\n"),
        "<blockquote>\n  <pre><code>\n</code></pre>\n</blockquote>\n<p>body\n<code></code></p>"
    );
}
