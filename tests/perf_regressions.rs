use std::fmt::Write as _;
use std::time::Instant;

#[test]
fn many_abbreviations_do_not_scan_every_definition_at_every_position() {
    let mut source = String::new();
    for i in 0..1500 {
        writeln!(source, "[A{i}]: expansion {i}").unwrap();
    }
    source.push('\n');
    source.push_str(&"z".repeat(12_000));

    let start = Instant::now();
    let html = carve::to_html(&source);

    assert!(html.contains(&"z".repeat(80)), "{html}");
    assert!(
        start.elapsed().as_secs_f32() < 2.0,
        "abbreviation parse took {:?}",
        start.elapsed()
    );
}

#[test]
fn many_unterminated_colon_fence_openers_do_not_rescan_document() {
    let mut source = String::new();
    source.push_str("intro\n");
    for _ in 0..8_000 {
        source.push_str("::: note\n");
    }

    let start = Instant::now();
    let html = carve::to_html(&source);

    assert!(html.contains("::: note"), "{html}");
    assert!(
        start.elapsed().as_secs_f32() < 2.0,
        "unterminated colon-fence parse took {:?}",
        start.elapsed()
    );
}
