//! A FENCED COMMENT FOLDED INLINE KEEPS ITS SPELLING over the ProseMirror
//! bridge (PART 9 §28; markup-carve/carve-rs#2067).
//!
//! The two spellings are different documents: `%%` runs to the end of its inline
//! run, the `%%%` form ends at its closer. So a fenced comment that comes back
//! spelled `%%` swallows whatever followed it on the line - here the `tail` word
//! in the term. Only the `block` attribute tells the two apart, and both bridge
//! arms dropped it.
//!
//! `CarveCommentInline` in carve-grammars declares the attribute as of
//! markup-carve/carve-grammars#572, so the editor schema keeps what this writes.
//!
//! The corpus round trip cannot see any of it:
//! `fully_covered_corpus_documents_round_trip_through_prosemirror` skips every
//! document that reports a loss, and the folded form reports `soft_break`. The
//! document is written out here for that reason.

use carve::{
    from_prosemirror, parse_with_options, render_html, to_html, to_html_with_options,
    to_prosemirror, Options,
};

fn both_paths(src: &str) -> String {
    let html = to_html(src);
    assert_eq!(
        html,
        to_html_with_options(src, &Options::default().with_positions(true)),
        "{src:?}"
    );
    html
}

/// The comment node inside the term, as both bridge directions spell it.
fn folded_comment(json: &str) -> serde_json::Value {
    let payload: serde_json::Value = serde_json::from_str(json).unwrap();
    payload["content"][0]["content"][0]["content"][2].clone()
}

#[test]
fn folded_fenced_comment_survives_prosemirror() {
    // A fenced comment indented under a term folds into the term
    // (markup-carve/carve#2458), which is the only source spelling that reaches
    // an inline comment carrying `block`.
    let source = ":: term\n   %%%\n   hidden\n   %%%\n   tail\n:  description\n";
    assert_eq!(
        both_paths(source),
        "<dl>\n  <dt>term\n\n   tail</dt>\n  <dd>description</dd>\n</dl>"
    );

    for positions in [false, true] {
        let original = parse_with_options(source, &Options::default().with_positions(positions));
        let bridge = to_prosemirror(&original);
        assert_eq!(
            folded_comment(&bridge.json)["attrs"]["block"],
            true,
            "{}",
            bridge.json
        );

        let returned = from_prosemirror(&bridge.json).unwrap();
        assert_eq!(
            folded_comment(&to_prosemirror(&returned).json)["attrs"]["block"],
            true
        );
        // The comment still ends at its closer, so the term keeps what followed
        // it and hides what it contains.
        let html = render_html(&returned).unwrap();
        assert!(html.contains("tail"), "{html}");
        assert!(!html.contains("hidden"), "{html}");

        // A payload without the flag is the `%%` spelling an editor that never
        // saw a delimited comment would have produced.
        let mut legacy: serde_json::Value = serde_json::from_str(&bridge.json).unwrap();
        legacy["content"][0]["content"][0]["content"][2]["attrs"]
            .as_object_mut()
            .unwrap()
            .remove("block");
        let returned = from_prosemirror(&legacy.to_string()).unwrap();
        assert_eq!(
            folded_comment(&to_prosemirror(&returned).json)["attrs"]["block"],
            false
        );
    }
}
