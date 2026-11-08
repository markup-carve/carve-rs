//! A fence nested in the block a note's marker-line `+` attached is an
//! ordinary fence: its first blank is payload, as in the same block outside
//! the note (PART 0 S1, carve#5539).

fn html(source: &str) -> String {
    carve::to_html(source)
}

#[test]
fn unclosed_item_fence_keeps_one_trailing_blank() {
    let out = html("[^a]: +\n- li\n  ``` x\n  code\n\nsee[^a]\n");
    assert!(
        out.contains("<pre><code class=\"language-x\">code\n\n</code></pre>"),
        "{out}"
    );
}

#[test]
fn note_paragraph_after_the_blank_adds_no_second_blank() {
    let out = html("[^a]: +\n- li\n  ``` x\n  code\n\n  more\n\nsee[^a]\n");
    assert!(
        out.contains("<pre><code class=\"language-x\">code\n\n</code></pre>"),
        "{out}"
    );
    assert!(out.contains("<p>more<a href=\"#fnref1\""), "{out}");
}

#[test]
fn fence_the_plus_attached_still_ends_at_its_blank() {
    let out = html("[^a]: +\n{.x}\n```\nx\n\nsee[^a]\n");
    assert!(out.contains("<code>x\n</code></pre>"), "{out}");
}

#[test]
fn item_hosted_note_keeps_the_blank_too() {
    let out = html("- x\n  [^a]: +\n  - li\n    ``` x\n    code\n\nsee[^a]\n");
    assert!(
        out.contains("<pre><code class=\"language-x\">code\n\n</code></pre>"),
        "{out}"
    );
}
