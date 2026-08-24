use carve::{try_render_html_streaming, Options, StreamOutcome};

#[test]
fn accepted_input_reaches_the_sink_byte_identically() {
    let source = "# Heading\n\nText with *strong*.\n";
    let mut output = String::new();
    let outcome =
        try_render_html_streaming(source, &Options::default(), |chunk| output.push_str(chunk));
    assert_eq!(outcome, StreamOutcome::Complete);
    assert_eq!(output, carve::to_html(source));
}

#[test]
fn fallback_emits_nothing() {
    let source = "[^note]: Body.\n\nText[^note].\n";
    let mut called = false;
    let outcome = try_render_html_streaming(source, &Options::default(), |_| called = true);
    assert_eq!(outcome, StreamOutcome::NeedsAst);
    assert!(!called);
}
