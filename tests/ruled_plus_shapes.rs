use serde::Deserialize;

#[derive(Deserialize)]
struct Shape {
    source: String,
    html: String,
}

#[test]
fn ruled_plus_shapes() {
    let mut failures = Vec::new();
    for (name, data) in [
        (
            "refused-plus-open-fence-shapes",
            include_str!("fixtures/refused-plus-open-fence-shapes.json"),
        ),
        (
            "plus-below-open-div-shapes",
            include_str!("fixtures/plus-below-open-div-shapes.json"),
        ),
        (
            "plus-below-fence-host-followers",
            include_str!("fixtures/plus-below-fence-host-followers.json"),
        ),
    ] {
        let shapes: Vec<Shape> = serde_json::from_str(data).unwrap();
        let file_failures: Vec<_> = shapes
            .into_iter()
            .enumerate()
            .filter_map(|(index, shape)| {
                let actual = carve::to_html(&shape.source);
                (actual.trim_end_matches('\n') != shape.html).then(|| {
                    format!(
                        "{name}[{index}]:\nactual: {actual:?}\nexpected: {:?}",
                        shape.html
                    )
                })
            })
            .take(8)
            .collect();
        failures.extend(file_failures);
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
