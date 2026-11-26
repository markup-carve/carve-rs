//! An empty `<code>` has no spelling (C2-I-015, C2-I-017), so the import drops
//! it as C2-M-069 drops an inline node that needs content, instead of refusing
//! the whole document.

use carve::api::{self, ImportOptions};

fn import(html: &str) -> (String, Vec<String>) {
    let got = api::import_document(html, &ImportOptions::default()).expect("import");
    let codes = got.diagnostics.iter().map(|d| d.code.clone()).collect();
    (got.source, codes)
}

#[test]
fn empty_code_is_dropped() {
    for (html, source) in [
        ("<p><code></code>y</p>", "y\n"),
        ("<p>a<code></code></p>", "a\n"),
        ("<p><code><b></b></code>y</p>", "y\n"),
        ("<blockquote><p><code></code>y</p></blockquote>", "> y\n"),
        ("<ul><li><code></code>y</li></ul>", "- y\n"),
    ] {
        let (src, codes) = import(html);
        assert_eq!(src, source, "{html}");
        assert!(codes.is_empty(), "{html}: {codes:?}");
    }
}

#[test]
fn empty_code_with_attributes_is_reported() {
    for html in [
        "<p><code title=\"t\"></code>y</p>",
        "<p><code class=\"language-js\"></code>y</p>",
    ] {
        let (src, codes) = import(html);
        assert_eq!(src, "y\n", "{html}");
        assert_eq!(codes, ["import-structure-repaired"], "{html}");
    }
}

#[test]
fn code_with_a_space_is_kept() {
    let (src, _) = import("<p><code> </code>y</p>");
    assert_eq!(src, "` `y\n");
}
