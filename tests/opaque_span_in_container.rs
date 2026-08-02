//! A fence-shaped line inside an opaque span must not close the container the
//! span sits in (carve#450).
//!
//! The container body collector copies opaque spans through verbatim. It used
//! to test the span's own opener line against the closer pattern, so an opener
//! that carries no info string - a bare ``` , a bare ~~~ , a `%%%` - ended the
//! span on its own line. Everything after it went back to the block parser,
//! where the `:::` inside the span closed the container.

fn html(src: &str) -> String {
    carve::to_html(src).trim().to_string()
}

const CLOSED_NOTE_WITH_CODE: &str = concat!(
    "<aside class=\"admonition note\">\n",
    "  <pre><code>:::\n",
    "</code></pre>\n",
    "  <p>body</p>\n",
    "</aside>\n",
    "<p>after</p>"
);

#[test]
fn a_bare_backtick_fence_holding_a_closer_stays_inside_the_container() {
    assert_eq!(
        html("::: note\n```\n:::\n```\nbody\n:::\nafter\n"),
        CLOSED_NOTE_WITH_CODE
    );
}

#[test]
fn a_bare_tilde_fence_holding_a_closer_stays_inside_the_container() {
    assert_eq!(
        html("::: note\n~~~\n:::\n~~~\nbody\n:::\nafter\n"),
        CLOSED_NOTE_WITH_CODE
    );
}

#[test]
fn an_info_string_fence_holding_a_closer_stays_inside_the_container() {
    assert_eq!(
        html("::: note\n```text\n:::\n```\nbody\n:::\nafter\n"),
        concat!(
            "<aside class=\"admonition note\">\n",
            "  <pre><code class=\"language-text\">:::\n",
            "</code></pre>\n",
            "  <p>body</p>\n",
            "</aside>\n",
            "<p>after</p>"
        )
    );
}

#[test]
fn a_comment_block_holding_a_closer_stays_inside_the_container() {
    // The comment block renders nothing, so what this pins is where the
    // container ends: `body` inside the aside, `after` outside it.
    let out = html("::: note\n%%%\n:::\n%%%\nbody\n:::\nafter\n");
    assert!(
        out.starts_with("<aside class=\"admonition note\">"),
        "{out}"
    );
    assert!(out.contains("<p>body</p>\n</aside>"), "{out}");
    assert!(out.ends_with("<p>after</p>"), "{out}");
    assert!(!out.contains("<div>"), "{out}");
}

#[test]
fn a_span_opener_inside_a_fence_is_not_a_container_opener() {
    // The variant that becomes load-bearing under exact-length closers: a
    // non-bare opener inside the span must not push a nesting level either.
    assert_eq!(
        html("::: note\n```text\n::: tip\n```\nbody\n:::\nafter\n"),
        concat!(
            "<aside class=\"admonition note\">\n",
            "  <pre><code class=\"language-text\">::: tip\n",
            "</code></pre>\n",
            "  <p>body</p>\n",
            "</aside>\n",
            "<p>after</p>"
        )
    );
}

#[test]
fn a_well_formed_comment_block_still_ends_where_it_did() {
    // The opener-first change must not swallow a line past the closer.
    let out = html("::: note\n%%%\nx\n%%%\nbody\n:::\nafter\n");
    assert!(out.contains("<p>body</p>"), "{out}");
    assert!(!out.contains("<p>x</p>"), "{out}");
    assert!(out.ends_with("<p>after</p>"), "{out}");
}
