//! A fenced comment keeps its `block` flag over the ProseMirror bridge
//! (PART 9 §28; markup-carve/carve-rs#2067). The inline fold this file was
//! written for is gone under rule A (markup-carve/carve#3885): the fenced
//! comment under a term is now a block comment, and the round trip keeps it.

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

/// The comment node after the term, as both bridge directions spell it.
fn fenced_comment(json: &str) -> serde_json::Value {
    let payload: serde_json::Value = serde_json::from_str(json).unwrap();
    payload["content"][1].clone()
}

#[test]
fn a_fenced_comment_under_a_term_survives_prosemirror() {
    // Under rule A (markup-carve/carve#3885) a term owns nothing, so the fenced
    // comment no longer folds into it: it is a block comment after the list.
    let source = ":: term\n   %%%\n   hidden\n   %%%\n   tail\n:  description\n";
    let html = both_paths(source);
    assert_eq!(
        html,
        "<dl>\n  <dt>term</dt>\n</dl>\n<p>tail\n:  description</p>"
    );

    for positions in [false, true] {
        let original = parse_with_options(source, &Options::default().with_positions(positions));
        let bridge = to_prosemirror(&original);
        let comment = fenced_comment(&bridge.json);
        assert_eq!(comment["type"], "carveComment", "{}", bridge.json);
        assert_eq!(comment["attrs"]["block"], true, "{}", bridge.json);

        let returned = from_prosemirror(&bridge.json).unwrap();
        assert_eq!(
            fenced_comment(&to_prosemirror(&returned).json),
            comment,
            "{}",
            bridge.json
        );
        // The bridge folds the soft break to a space, so only content is compared.
        let returned_html = render_html(&returned).unwrap();
        assert!(returned_html.contains("tail"), "{returned_html}");
        assert!(!returned_html.contains("hidden"), "{returned_html}");
    }
}
