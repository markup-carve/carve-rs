//! A decoded tree's positions are not checked against its source: a forged end offset must not
//! size an allocation (fuzz target `decode`, an `endOffset` of 2,888,888,884 asked for 5.7 GB).

use carve::api::{self, Carve, Options, WriteOptions};

#[test]
fn a_forged_end_offset_renders_and_writes() {
    let json = r#"{"type":"document","astVersion":"2.0","children":[{"type":"code_block","value":"a\n  b\nc\n","pos":{"startLine":1,"endLine":5,"startColumn":3,"endColumn":6,"startOffset":2,"endOffset":4294967295}}],"srcByteLength":24,"stages":["referenceResolution","tableAlignment"]}"#;
    let tree = api::decode_tree(json).expect("the tree decodes");
    let rendered =
        api::render_tree_with_diagnostics(&tree, &Options::default()).expect("the tree renders");
    assert_eq!(rendered.html, "<pre><code>a\n  b\nc\n</code></pre>");
    assert!(
        Carve::new()
            .write_with_diagnostics(&tree, &WriteOptions::default())
            .is_ok()
    );
}
