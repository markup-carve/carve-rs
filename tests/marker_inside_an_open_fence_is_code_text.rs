//! A list marker at the content column, inside a fence the item opened, is CODE
//! TEXT (corpus category 278, markup-carve/carve#975; ruling on
//! markup-carve/carve-php#1007).
//!
//! PART 9 §24's S1 MATCH PREFIXES and S2 FENCED BODY place a line by the COLUMN
//! it reaches. Neither reads the line's first character. So a marker at an
//! item's content column inside its own fence is the same continuation a plain
//! `x` is - which corpus row
//! `276-a-fence-opened-on-a-list-marker-line-body-below-the-content-column-3`
//! already pins, and which this category differs from by two characters.
//!
//! Without the guard the marker SEVERED the verbatim body: the fence closed
//! empty, the marker opened a sub-list, and the fence's own closer came back as
//! an empty code span - three wrong nodes from one missing condition.

fn html(src: &str) -> String {
    carve::to_html(src)
}

const FENCED_MARKER: &str = "<ul>\n  <li>\n    <pre><code>- x\n</code></pre>\n  </li>\n</ul>";

#[test]
fn the_fence_is_opened_on_the_marker_line() {
    // Category 278 row 1. Already correct on this engine before the guard was
    // added, because the marker-line opener seeds the fence state on a
    // different route - measured rather than assumed, since the per-engine
    // attributions on this rule have been wrong repeatedly.
    assert_eq!(html("- ```\n  - x\n  ```\n"), FENCED_MARKER);
}

#[test]
fn the_fence_is_opened_on_a_line_of_its_own() {
    // Category 278 row 2, and the row this engine owed. The fence state comes
    // from the previous content-column line rather than from the marker line,
    // and the marker test that consumes it had no fence guard.
    assert_eq!(
        html("- a\n+\n```\n- x\n```\n"),
        "<ul>\n  <li>a\n    <pre><code>- x\n</code></pre>\n  </li>\n</ul>"
    );
}

#[test]
fn a_plain_line_at_the_same_column_was_always_code_text() {
    // CONTROL, and the row the rule is derived from (corpus 276 row 3). Two
    // characters separate it from row 1 above, and the placement rule reads
    // neither of them.
    assert_eq!(
        html("- ```\n  x\n  ```\n"),
        "<ul>\n  <li>\n    <pre><code>x\n</code></pre>\n  </li>\n</ul>"
    );
}

#[test]
fn a_marker_with_no_fence_open_still_opens_a_sub_list() {
    // CONTROL, and the boundary the guard must not cross. The split the marker
    // test performs exists so an indented sub-list nests instead of folding
    // into the lead paragraph, and that reason is untouched by this rule.
    assert_eq!(
        html("- a\n  - x\n"),
        "<ul>\n  <li>a\n    <ul>\n      <li>x</li>\n    </ul>\n  </li>\n</ul>"
    );
}

#[test]
fn a_marker_after_a_closed_fence_still_opens_a_sub_list() {
    // CONTROL, and the one that separates "a fence was opened here" from "a
    // fence is OPEN here". A guard keyed on the former leaves this document
    // with its marker swallowed into the code block.
    assert_eq!(
        html("- a\n+\n```\ny\n```\n  - x\n"),
        "<ul>\n  <li>a\n    <pre><code>y\n</code></pre>\n    <ul>\n      <li>x</li>\n    </ul>\n  </li>\n</ul>"
    );
}

#[test]
fn the_continuation_marker_paths_were_already_right() {
    // CONTROLS. The `+` marker attaches a FLUSH-LEFT block through loops of
    // their own, and the corpus reaches neither. Measured on this engine before
    // the guard and unchanged by it - pinned so a later change to those loops
    // cannot regress a rule no corpus document covers.
    assert_eq!(html("- ```\n  - x\n  ```\n"), FENCED_MARKER);
    assert_eq!(
        html("- a\n+\n```\n- x\n```\n"),
        "<ul>\n  <li>a\n    <pre><code>- x\n</code></pre>\n  </li>\n</ul>"
    );
}

#[test]
fn the_other_container_bodies_were_already_right() {
    // CONTROLS. A definition body and a block quote collect their fenced bodies
    // by different routes and neither consults the item collector's marker
    // test, so neither was ever exposed. Pinned for the same reason.
    assert_eq!(
        html(":: t\n:  ```\n   - x\n   ```\n"),
        "<dl>\n  <dt>t</dt>\n  <dd>\n    <pre><code>- x\n</code></pre>\n  </dd>\n</dl>"
    );
    assert_eq!(
        html("> ```\n> - x\n> ```\n"),
        "<blockquote>\n  <pre><code>- x\n</code></pre>\n</blockquote>"
    );
}

#[test]
fn the_round_trip_holds() {
    // PART 11 §1. A writer that reproduced the severed shape would bake the
    // wrong parse into the source.
    for src in [
        "- ```\n  - x\n  ```\n",
        "- a\n  ```\n  - x\n  ```\n",
        "- a\n  ```\n  y\n  ```\n  - x\n",
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
