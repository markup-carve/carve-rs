//! PART 9 §17 L1: a blank line a sub-list consumes is that sub-list's, also
//! when the sub-list FOLLOWS a sibling sub-list in the item. Each line is
//! measured against the content column of the sub-list item it sits in, not
//! the first sub-list's (markup-carve/carve-rs#1846, carve-js#1951).

fn html(source: &str) -> String {
    carve::to_html(source)
}

fn outer_is_tight(out: &str) -> bool {
    out.starts_with("<ul>\n  <li>e\n")
}

#[test]
fn a_blank_inside_a_fence_in_the_second_sub_list() {
    assert_eq!(
        html("- e\n  1. x\n  * ```\n\n    ```\n"),
        "<ul>\n  <li>e\n    <ol>\n      <li>x</li>\n    </ol>\n    <ul>\n      <li>\n        <pre><code>\n</code></pre>\n      </li>\n    </ul>\n  </li>\n</ul>",
    );
}

#[test]
fn sibling_sub_list_shapes_keep_the_outer_item_tight() {
    let loose = [
        ("a tilde fence", "- e\n  1. x\n  * ~~~\n\n    ~~~\n"),
        (
            "a fence with body lines",
            "- e\n  1. x\n  * ```\n    a\n\n    b\n    ```\n",
        ),
        ("an unclosed fence", "- e\n  1. x\n  * ```\n\n    code\n"),
        (
            "a fence in a quote",
            "- e\n  1. x\n  * > ```\n    >\n    > c\n    > ```\n",
        ),
        (
            "a third sub-list",
            "- e\n  1. x\n  * y\n  1. ```\n\n     ```\n",
        ),
        (
            "a second paragraph of the sibling sub-item",
            "- e\n  1. x\n  * y\n\n    z\n",
        ),
        (
            "a folded marker below the sub-item column",
            "- e\n  1. x\n    * y\n\n     z\n",
        ),
    ]
    .into_iter()
    .filter_map(|(name, src)| {
        let out = html(src);
        (!outer_is_tight(&out)).then(|| format!("{name}: {out}"))
    })
    .collect::<Vec<_>>();
    assert!(loose.is_empty(), "{}", loose.join("\n"));
}

#[test]
fn three_levels_deep_keeps_the_middle_item_tight() {
    let out = html("- e\n  - f\n    1. x\n    * ```\n\n      ```\n");
    assert!(out.contains("<li>f\n"), "{out}");
}

#[test]
fn a_sibling_after_a_marker_lead_keeps_the_next_outer_item_tight() {
    let out = html("- 1. x\n  * ```\n\n    ```\n- g\n");
    assert!(out.contains("<li>g</li>"), "{out}");
}

#[test]
fn the_sibling_sub_item_still_reads_its_own_second_paragraph_loose() {
    let out = html("- e\n  1. x\n  * y\n\n    z\n");
    assert!(out.contains("<li><p>y</p>\n        <p>z</p>"), "{out}");
}

#[test]
fn the_outer_items_own_second_paragraph_still_loosens_it() {
    let out = html("- e\n  1. x\n  * y\n\n  text\n");
    assert!(out.starts_with("<ul>\n  <li><p>e</p>\n"), "{out}");
}

#[test]
fn a_blank_between_the_outer_items_still_loosens_the_list() {
    let out = html("- e\n  1. x\n  * y\n\n- f\n");
    assert!(out.starts_with("<ul>\n  <li><p>e</p>\n"), "{out}");
}
