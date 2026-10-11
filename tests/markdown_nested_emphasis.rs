use carve::CheckedRenderOptions;
use serde::Deserialize;

#[derive(Deserialize)]
struct Control {
    source: String,
    markdown: Markdown,
}

#[derive(Deserialize)]
struct Markdown {
    rust: String,
}

#[test]
fn markdown_preserves_nested_emphasis() {
    let controls: Vec<Control> =
        serde_json::from_str(include_str!("fixtures/markdown-nested-emphasis.json")).unwrap();
    assert_eq!(controls.len(), 2556);
    for control in controls {
        let result =
            carve::to_markdown_with_report(&control.source, CheckedRenderOptions::default())
                .unwrap();
        assert_eq!(result.value, control.markdown.rust, "{}", control.source);
        assert!(result.losses.is_empty(), "{}", control.source);
        assert_eq!(result.total_losses, 0, "{}", control.source);
        assert!(!result.truncated, "{}", control.source);
        let ast = carve::parse(&control.source);
        assert_eq!(
            carve::render_markdown(&ast).unwrap(),
            result.value,
            "{}",
            control.source
        );
    }
}
