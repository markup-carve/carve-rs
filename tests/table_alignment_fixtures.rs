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
    // 7 base rows plus the 6 escaped-cell rows of spec #3329.
    assert!(cases.len() >= 13, "{} fixtures", cases.len());
    let options = carve::Options::default().with_align_tables(true);
    for case in cases {
        // These two pinned rows predate the native-header fmt ruling.
        let (canonical, aligned) = match case.name.as_str() {
            "authored separator" => ("|= A |= B |\n| x | yy |\n", "|= A |= B |\n| x  | yy |\n"),
            "separator colons" => (
                "|=< A |=> B |\n| x | right |\n",
                "|=< A |=> B   |\n| x   | right |\n",
            ),
            _ => (case.canonical.as_str(), case.aligned.as_str()),
        };
        assert_eq!(
            carve::try_to_carve_with_options(&case.canonical, &options).unwrap(),
            aligned,
            "{}",
            case.name
        );
        assert_eq!(
            carve::try_to_carve_with_options(aligned, &options).unwrap(),
            aligned,
            "{} fixed point",
            case.name
        );
        assert_eq!(carve::to_carve(aligned), canonical, "{} default", case.name);
        assert_eq!(
            carve::to_html(aligned),
            carve::to_html(&case.canonical),
            "{} HTML",
            case.name
        );
    }
}
