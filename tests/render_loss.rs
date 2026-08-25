use carve::{CheckedRenderOptions, RenderTarget};

const SOURCE: &str = "`one`{=latex} and `two`{=typst}\n";

#[test]
fn checked_html_keeps_output_and_reports_every_drop_in_source_order() {
    let result = carve::to_html_with_report(SOURCE, CheckedRenderOptions::default()).unwrap();
    assert_eq!(result.value, carve::to_html(SOURCE));
    assert_eq!(result.total_losses, 2);
    assert!(!result.truncated);
    assert_eq!(result.losses[0].format, "latex");
    assert_eq!(result.losses[0].target, RenderTarget::Html);
    assert_eq!(result.losses[0].pos.unwrap().start_column, 1);
    assert_eq!(result.losses[1].format, "typst");
}

#[test]
fn matching_html_is_not_a_loss() {
    let result =
        carve::to_html_with_report("`<b>x</b>`{=html}\n", CheckedRenderOptions::default()).unwrap();
    assert_eq!(result.total_losses, 0);
    assert!(result.value.contains("<b>x</b>"));
}

#[test]
fn strict_error_contains_the_complete_bounded_report() {
    let error = carve::to_html_with_report(
        SOURCE,
        CheckedRenderOptions {
            strict: true,
            max_losses: 1,
        },
    )
    .unwrap_err();
    assert_eq!(error.total_losses, 2);
    assert_eq!(error.losses.len(), 1);
    assert!(error.truncated);
}

#[test]
fn target_specific_visible_fallbacks_are_not_losses() {
    let html = "`<b>x</b>`{=html}\n";
    assert_eq!(
        carve::to_markdown_with_report(html, CheckedRenderOptions::default())
            .unwrap()
            .total_losses,
        0
    );
    let raw_block = "``` =latex\nx
```\n";
    assert_eq!(
        carve::to_ansi_with_report(raw_block, CheckedRenderOptions::default())
            .unwrap()
            .total_losses,
        0
    );
    assert_eq!(
        carve::to_plain_text_with_report(raw_block, CheckedRenderOptions::default())
            .unwrap()
            .total_losses,
        1
    );
    assert_eq!(
        carve::to_carve_with_report(raw_block, CheckedRenderOptions::default())
            .unwrap()
            .total_losses,
        0
    );
}
