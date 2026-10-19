use std::collections::BTreeMap;

use serde::Deserialize;

#[derive(Deserialize)]
struct Shape {
    source: String,
    html: String,
}

#[test]
fn folded_fence_closer_shapes() {
    let shapes: BTreeMap<String, Shape> =
        serde_json::from_str(include_str!("fixtures/folded-fence-closer-shapes.json")).unwrap();
    let failures: Vec<_> = shapes
        .into_iter()
        .filter_map(|(name, shape)| {
            let actual = carve::to_html(&shape.source);
            (actual.trim_end_matches('\n') != shape.html)
                .then(|| format!("{name}:\nactual: {actual:?}\nexpected: {:?}", shape.html))
        })
        .collect();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
