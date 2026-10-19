use serde::Deserialize;

#[derive(Deserialize)]
struct Case {
    name: String,
    canonical: String,
    aligned: String,
}

#[test]
fn shared_alignment_fixtures() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/spec/resources/spec/table-alignment-fixtures.json"
    );
    let data = std::fs::read_to_string(path)
        .expect("missing spec fixtures: run git submodule update --init --recursive");
    let cases: Vec<Case> = serde_json::from_str(&data).unwrap();
    assert!(!cases.is_empty());
    let options = carve::Options::default().with_align_tables(true);
    for case in cases {
        assert_eq!(
            carve::try_to_carve_with_options(&case.canonical, &options).unwrap(),
            case.aligned,
            "{}",
            case.name
        );
        assert_eq!(
            carve::try_to_carve_with_options(&case.aligned, &options).unwrap(),
            case.aligned,
            "{} fixed point",
            case.name
        );
        assert_eq!(
            carve::to_carve(&case.aligned),
            case.canonical,
            "{} default",
            case.name
        );
        assert_eq!(
            carve::to_html(&case.aligned),
            carve::to_html(&case.canonical),
            "{} HTML",
            case.name
        );
    }
}
