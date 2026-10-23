#[test]
fn a_band_line_after_a_flush_lazy_line_stays_in_the_nested_paragraph() {
    let cases = [
        ("- x\n  - y\nlazy\n lazy2\n", "y\nlazy\nlazy2"),
        ("- x\n  - y\nlazy\n # h\n", "y\nlazy\n# h"),
        ("- x\n  - y\nlazy\n - z\n", "y\nlazy\n- z"),
        ("- x\n  - y\n: e\n :: t\n : zq\n", "y\n: e\n:: t\n: zq"),
        ("1. x\n   - y\nlazy\n  # h\n", "y\nlazy\n# h"),
    ];
    for (source, text) in cases {
        let html = carve::to_html(source);
        assert!(
            html.contains(&format!("<li>{text}</li>")),
            "{source:?}\n{html}"
        );
    }
}
