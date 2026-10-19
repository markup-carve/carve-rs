//! CARVE-P11-073 (carve#3330): fmt keeps a region marker on its line. The
//! fixture is a copy of the spec's tests/fixtures/region-marker-fmt.json.

use carve::{to_carve, to_html};

#[test]
fn fmt_writes_the_region_marker_fixture_bytes() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/region-marker-fmt.json")).unwrap();
    let rows = fixture["rows"].as_array().unwrap();
    assert!(!rows.is_empty());
    for row in rows {
        let id = row["id"].as_str().unwrap();
        let source = row["source"].as_str().unwrap();
        let fmt = row["fmt"].as_str().unwrap();
        assert_eq!(to_carve(source), fmt, "{id}");
        assert_eq!(to_carve(fmt), fmt, "{id}: not idempotent");
    }
}

#[test]
fn a_region_marker_keeps_the_reading_it_had() {
    let cases = [
        // A comment that is not a marker clears X1 retention at the content
        // column, so the marker under it stays there too.
        (
            "- a\n%% note\n%% #endregion r\n",
            "- a\n  %% note\n  %% #endregion r\n",
        ),
        // A marker retained in a nested item goes back to that item's marker column.
        (
            "- x\n  - y\n%% #endregion r\n",
            "- x\n  - y\n  %% #endregion r\n",
        ),
        // Below a list the closer cannot be written adjacent: it would join the item.
        ("- a\n\n%% #endregion r\n", "- a\n\n%% #endregion r\n"),
        // A definition above a marker is not hoisted past it, so the item keeps it.
        (
            "- [ref]: /url\n%% #region r\n\nSee [it][ref].\n",
            "- [ref]: /url\n\n%% #region r\n\nSee [it][ref].\n",
        ),
        // A paragraph holding only an image is written as a block image.
        (
            " ![Apollo](a.jpg)\n%% #region r\n",
            "![Apollo](a.jpg)\n%% #region r\n",
        ),
    ];
    for (source, expected) in cases {
        let written = to_carve(source);
        assert_eq!(written, expected, "{source:?}");
        assert_eq!(to_carve(&written), written, "{source:?}: not idempotent");
        assert_eq!(
            to_html(&written),
            to_html(source),
            "{source:?}: reading changed"
        );
    }
}
