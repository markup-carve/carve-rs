//! An unclosed fence run at the parent item's column, below a nested child's
//! open paragraph, does not interrupt it (CARVE-P0-014): the run is lazy text
//! in the child, and a flush-left line after it folds into the same paragraph.

fn compact(source: &str) -> String {
    carve::to_html(source)
        .lines()
        .map(str::trim)
        .collect::<Vec<_>>()
        .join("|")
}

#[test]
fn the_run_and_the_line_after_it_stay_in_the_child() {
    assert_eq!(
        compact("- a\n    - child\n  ~~~\nafter\n"),
        "<ul>|<li>a|<ul>|<li>child|~~~|after</li>|</ul>|</li>|</ul>"
    );
    assert_eq!(
        compact("- a\n    - child\n  ```\nafter\n"),
        "<ul>|<li>a|<ul>|<li>child|<code>|after</code></li>|</ul>|</li>|</ul>"
    );
}

#[test]
fn the_same_holds_under_an_ordered_parent_and_in_a_quote() {
    assert_eq!(
        compact("1. a\n     - child\n   ~~~\nafter\n"),
        "<ol>|<li>a|<ul>|<li>child|~~~|after</li>|</ul>|</li>|</ol>"
    );
    assert_eq!(
        compact("> - a\n>     - child\n>   ~~~\n> after\n"),
        "<blockquote>|<ul>|<li>a|<ul>|<li>child|~~~|after</li>|</ul>|</li>|</ul>|</blockquote>"
    );
}

#[test]
fn a_closer_outside_the_item_does_not_close_the_run() {
    assert_eq!(
        compact("- a\n    - child\n  ~~~\nafter\n\n~~~\n"),
        "<ul>|<li>a|<ul>|<li>child|~~~|after</li>|</ul>|</li>|</ul>|<pre><code></code></pre>"
    );
}

#[test]
fn a_closed_run_still_interrupts_the_child() {
    assert_eq!(
        compact("- a\n    - child\n  ~~~\n  ~~~\nafter\n"),
        "<ul>|<li>a|<ul>|<li>child</li>|</ul>|<pre><code></code></pre>|</li>|</ul>|<p>after</p>"
    );
}
