//! An unclosed fence-shaped line under an item's open paragraph is paragraph
//! text (CARVE-P0-014), so a box closer below it still closes the box and a
//! flush-left line after the box leaves the list.

fn compact(source: &str) -> String {
    carve::to_html(source)
        .lines()
        .map(str::trim)
        .collect::<Vec<_>>()
        .join("|")
}

#[test]
fn a_line_after_the_closed_box_leaves_the_list() {
    assert_eq!(
        compact("- ::: box\n  - item\n   ```\n   :::\n  :::\nafter\n"),
        "<ul>|<li>|<div class=\"box\">|<ul>|<li>item|<code></code></li>|</ul>|</div>|<div>||</div>|</li>|</ul>|<p>after</p>"
    );
    assert_eq!(
        compact("- ::: box\n  - item\n   ~~~\n  :::\nafter\n"),
        "<ul>|<li>|<div class=\"box\">|<ul>|<li>item|~~~</li>|</ul>|</div>|</li>|</ul>|<p>after</p>"
    );
}

#[test]
fn a_closed_fence_still_hides_the_colon_run() {
    assert_eq!(
        compact("- ::: box\n  ```\n  :::\n  ```\n  :::\nafter\n"),
        "<ul>|<li>|<div class=\"box\">|<pre><code>:::|</code></pre>|</div>|</li>|</ul>|<p>after</p>"
    );
}
