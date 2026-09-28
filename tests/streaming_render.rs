use carve::{try_render_html_streaming, Options, StreamOutcome};

#[test]
fn accepted_output_arrives_in_multiple_utf8_chunks() {
    let source = "# Heading\n\nSecond paragraph.\n";
    let mut chunks = Vec::new();
    let outcome = try_render_html_streaming(source, &Options::default(), |chunk| {
        chunks.push(chunk.to_string());
    });
    assert_eq!(outcome, StreamOutcome::Complete);
    assert!(chunks.len() > 1);
    assert_eq!(chunks.concat(), carve::to_html(source));
}

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

#[test]
fn large_output_is_bounded_and_matches_the_ast_renderer() {
    for source in [
        "word & text ".repeat(20_000).trim_end().to_owned(),
        format!("```\n{}\n```\n", "<&>".repeat(30_000)),
        "- item with *strong*\n".repeat(8_000),
    ] {
        let mut output = String::new();
        let mut count = 0;
        let result = try_render_html_streaming(&source, &Options::default(), |chunk| {
            assert!(chunk.len() <= 4096);
            count += 1;
            output.push_str(chunk);
        });
        assert_eq!(result, StreamOutcome::Complete);
        assert!(count > 10);
        assert_eq!(output, carve::render_html(&carve::parse(&source)).unwrap());
    }
}

#[test]
fn late_rejection_does_not_publish_the_valid_prefix() {
    let source = format!(
        "{}\n\n{{unsupported}}\n",
        "plain paragraph\n\n".repeat(10_000)
    );
    let result = try_render_html_streaming(&source, &Options::default(), |_| {
        panic!("rejected document reached the sink");
    });
    assert_eq!(result, StreamOutcome::NeedsAst);
}

#[test]
fn unsupported_options_fall_back_without_callbacks() {
    let options = Options {
        source_lines: true,
        ..Options::default()
    };
    let result = try_render_html_streaming("text", &options, |_| panic!("unexpected output"));
    assert_eq!(result, StreamOutcome::NeedsAst);
}

#[test]
fn empty_document_calls_once_and_callback_panics_leave_no_context() {
    let mut calls = 0;
    assert_eq!(
        try_render_html_streaming("", &Options::default(), |chunk| {
            assert_eq!(chunk, "");
            calls += 1;
        }),
        StreamOutcome::Complete
    );
    assert_eq!(calls, 1);
    let source = "[label][ref]\n\n[ref]: /target\n";
    assert!(std::panic::catch_unwind(|| {
        try_render_html_streaming(source, &Options::default(), |_| panic!("sink failed"));
    })
    .is_err());
    let mut output = String::new();
    assert_eq!(
        try_render_html_streaming(source, &Options::default(), |chunk| output.push_str(chunk)),
        StreamOutcome::Complete
    );
    assert_eq!(output, carve::to_html(source));
    assert!(!carve::to_html("[label][ref]").contains("/target"));
}
