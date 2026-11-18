//! C2-W-138: duplicate IDs identify the authored attribute token.

use carve2::Options;

#[test]
fn duplicate_id_range_skips_equal_text_and_prefixes() {
    for (input, line, start, end) in [
        ("{#a}\n= A\n\n[#a]{#a}\n", 4, 6, 8),
        ("{#a}\n= A\n\n[x]{#ab #a}\n", 4, 9, 11),
        ("{#a}\n= A\n\n[x]{id=a}\n", 4, 5, 9),
    ] {
        let out = carve2::render_with_diagnostics(input, &Options::default());
        let finding = out
            .diagnostics
            .iter()
            .find(|d| d.code == "duplicate-id")
            .unwrap_or_else(|| panic!("missing duplicate-id for {input:?}"));
        let pos = finding.pos.unwrap();
        assert_eq!(
            (
                pos.start_line,
                pos.start_column,
                pos.end_line,
                pos.end_column
            ),
            (line, start, line, end),
            "{input:?}"
        );
    }
}
