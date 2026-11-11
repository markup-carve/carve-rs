use std::fs;
use std::path::Path;

#[test]
fn an_open_attached_colon_owns_a_later_plus_in_each_marker_host() {
    for slug in [
        "1033-a-quote-attached-colon-keeps-a-later-plus",
        "1034-an-item-attached-colon-keeps-a-later-plus",
    ] {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/spec/tests/corpus");
        let source = fs::read_to_string(dir.join(format!("{slug}.crv"))).unwrap();
        let expected = fs::read_to_string(dir.join(format!("{slug}.html"))).unwrap();
        assert_eq!(carve::to_html(&source).trim(), expected.trim(), "{slug}");
    }
}

#[test]
fn an_attached_colon_keeps_later_plus_text_across_extra_paragraph_lines() {
    for host in ["> +", "- +"] {
        for before in ["x", "x\ny", "x\ny\nz"] {
            let source = format!("{host}\n::: note\n{before}\n+\nafter");
            let html = carve::to_html(&source);
            let paragraph = format!("<p>{before}\n+\nafter</p>");
            assert!(html.contains(&paragraph), "{source:?}: {html}");
        }
    }
}
