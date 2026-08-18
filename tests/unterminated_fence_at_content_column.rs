//! An unterminated fence after a container's lead paragraph is paragraph text.
//!
//! PART 9 §10 I4 requires a matching closer before a fenced code block can
//! interrupt an open paragraph. The container layout tracker must honor that
//! same decision: treating the rejected opener as a live fence ejects the next
//! below-column lazy continuation from the list item or definition body
//! (executable-spec corpus category 367).

#[test]
fn list_item_keeps_the_lazy_line_after_an_unterminated_fence() {
    assert_eq!(
        carve::to_html("- q\n  ```\ntail\n"),
        "<ul>\n  <li>q\n<code>\ntail</code></li>\n</ul>"
    );
}

#[test]
fn definition_keeps_the_lazy_line_after_an_unterminated_fence() {
    assert_eq!(
        carve::to_html(":: t\n:  a\n   ```\ntail\n"),
        "<dl>\n  <dt>t</dt>\n  <dd>a\n<code>\ntail</code></dd>\n</dl>"
    );
}
