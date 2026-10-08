#[test]
fn native_escapes_and_writer_round_trips() {
    let cases: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/image-alt-escapes.json")).unwrap();
    for case in cases.as_array().unwrap() {
        let source = case["source"].as_str().unwrap();
        let expected = case["html"].as_str().unwrap();
        assert_eq!(
            carve::to_html(source)
                .trim()
                .replace("&#39;", "&#039;")
                .replace("&apos;", "&#039;"),
            expected,
            "{}",
            case["name"]
        );
        let table = case["table"].as_str().unwrap();
        let image = expected
            .strip_prefix("<p>x ")
            .unwrap()
            .strip_suffix(" y</p>")
            .unwrap();
        assert!(
            carve::to_html(table)
                .replace("&apos;", "&#039;")
                .replace("&#39;", "&#039;")
                .contains(image),
            "table {}",
            case["name"]
        );
        assert!(
            carve::to_html(&carve::to_carve(table))
                .replace("&apos;", "&#039;")
                .replace("&#39;", "&#039;")
                .contains(image),
            "table writer {}",
            case["name"]
        );
        let written = carve::to_carve(source);
        assert_eq!(
            carve::to_html(&written)
                .trim()
                .replace("&#39;", "&#039;")
                .replace("&apos;", "&#039;"),
            expected,
            "{}",
            case["name"]
        );
        assert_eq!(carve::to_carve(&written), written, "{}", case["name"]);
    }
}

#[test]
fn table_image_stays_native() {
    assert!(
        carve::to_html("| ![a\\|b](/i) |\n|---|\n| c |\n").contains("<img src=\"/i\" alt=\"a|b\">")
    );
}

#[test]
fn unresolved_image_stays_literal() {
    assert!(carve::to_html("x ![a\\|b][missing] y").contains("![a\\|b][missing]"));
}

#[test]
fn migrate_escaped_image_alt() {
    let source = "x ![a\\|b][R] y\n\n[r]: /i\n";
    assert_eq!(
        carve::migrate_case_only_references(source, &carve::Options::default()),
        "x ![a\\|b][r] y\n\n[r]: /i\n"
    );
}

#[test]
fn escaped_backtick_beside_code_cell() {
    let source = "| ![a\\`b](/i \"c\\`d\") | `x` |\n|---|---|\n";
    let expected = carve::to_html(source);
    assert!(expected.contains("alt=\"a`b\" title=\"c`d\""));
    assert!(expected.contains("<code>x</code>"));
    assert_eq!(carve::to_html(&carve::to_carve(source)), expected);
}

#[test]
fn migration_uses_the_balanced_alt_boundary() {
    let source = "x ![a\\|b `][]`][R]{title=\"] [R]\"} y\n\n[r]: /i\n";
    assert_eq!(
        carve::migrate_case_only_references(source, &carve::Options::default()),
        source.replace("`][R]", "`][r]")
    );
}
