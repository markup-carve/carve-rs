fn html(source: &str) -> String {
    carve::to_html(source)
}

#[test]
fn a_note_below_the_description_ends_its_fence_closer_search() {
    let source = ":: term\n: detail\n  ```\n  code\n[^n]: note\npayload\n  ```\nafter\n";
    assert_eq!(
        html(source),
        "<dl>\n  <dt>term</dt>\n  <dd>detail\n<code>\ncode</code></dd>\n</dl>\n<p>payload\n<code>\nafter</code></p>"
    );
}

#[test]
fn a_closer_before_the_note_still_opens_a_fenced_block() {
    let source = ":: term\n: detail\n  ```\n  code\n  ```\n[^n]: note\npayload\n";
    assert_eq!(
        html(source),
        "<dl>\n  <dt>term</dt>\n  <dd>\n    <p>detail</p>\n    <pre><code>code\n</code></pre>\n  </dd>\n</dl>"
    );
}

#[test]
fn a_wrong_column_run_before_the_note_is_payload() {
    let source = ":: term\n: detail\n  ```\n  code\n   ```\n[^n]: note\npayload\n";
    assert_eq!(
        html(source),
        "<dl>\n  <dt>term</dt>\n  <dd>detail\n<code>\ncode\n</code></dd>\n</dl>"
    );
}
