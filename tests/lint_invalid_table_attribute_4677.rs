//! An invalid table attribute key leaves the other keys' findings standing (C2-B-211, C2-B-212, C2-B-227).

fn findings(input: &str) -> Vec<(&'static str, u32, u32)> {
    let mut found: Vec<_> = carve::lint::lint(input, &carve::lint::LintOptions::default())
        .into_iter()
        .filter(|finding| finding.code.starts_with("table-"))
        .map(|finding| {
            let pos = finding.pos.expect("range");
            (finding.code, pos.start_offset, pos.end_offset)
        })
        .collect();
    found.sort_unstable();
    found
}

#[test]
fn width_total_beside_an_invalid_key() {
    assert_eq!(
        findings("{aligns=x widths=60,50}\n| a | b |\n"),
        [
            ("table-attribute-invalid", 1, 9),
            ("table-width-total", 10, 16)
        ]
    );
    assert_eq!(
        findings("{widths=60,50 header-rows=x}\n| a | b |\n"),
        [
            ("table-attribute-invalid", 14, 27),
            ("table-width-total", 1, 7)
        ]
    );
}

#[test]
fn width_total_with_the_invalid_key_on_another_line() {
    assert_eq!(
        findings("{aligns=x}\n{widths=60,50}\n| a | b |\n"),
        [
            ("table-attribute-invalid", 1, 9),
            ("table-width-total", 12, 18)
        ]
    );
}

#[test]
fn arity_beside_an_invalid_key() {
    assert_eq!(
        findings("{aligns=x widths=50}\n| a | b |\n"),
        [
            ("table-attribute-invalid", 1, 9),
            ("table-column-arity", 10, 16)
        ]
    );
    assert_eq!(
        findings("{aligns=left valigns=q}\n| a | b |\n"),
        [
            ("table-attribute-invalid", 13, 22),
            ("table-column-arity", 1, 7)
        ]
    );
}

#[test]
fn overlap_beside_an_invalid_key() {
    assert_eq!(
        findings("{aligns=x valigns=top}\n|={?^} a |\n"),
        [
            ("table-attribute-invalid", 1, 9),
            ("table-column-overlap", 10, 17)
        ]
    );
    assert_eq!(
        findings("{header-rows=x aligns=left,right}\n|={<} a |={>} b |\n"),
        [
            ("table-attribute-invalid", 1, 14),
            ("table-column-overlap", 15, 21),
            ("table-column-overlap", 15, 21)
        ]
    );
}

#[test]
fn the_invalid_list_itself_reports_only_attribute_invalid() {
    assert_eq!(
        findings("{aligns=left,left,left valigns=q}\n| a | b |\n"),
        [("table-attribute-invalid", 23, 32)]
    );
    assert_eq!(
        findings("{aligns=x}\n|={<} a |\n"),
        [("table-attribute-invalid", 1, 9)]
    );
}
