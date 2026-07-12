//! `carve fmt` (to_carve) must preserve the reference-image invariant:
//! to_html(to_carve(x)) == to_html(x). An UNRESOLVED reference image round-trips
//! via its verbatim source, exactly like an unresolved reference link - emitting
//! `![alt]()` would change the rendered text and break the invariant.

#[test]
fn unresolved_reference_image_round_trips_verbatim() {
    let src = "![a][nope]";
    assert_eq!(carve::to_carve(src).trim(), "![a][nope]");
    assert_eq!(carve::to_html(&carve::to_carve(src)), carve::to_html(src));
}

#[test]
fn unresolved_reference_image_in_text_round_trips() {
    let src = "x ![a][nope] y";
    assert_eq!(carve::to_carve(src).trim(), "x ![a][nope] y");
    assert_eq!(carve::to_html(&carve::to_carve(src)), carve::to_html(src));
}

#[test]
fn resolved_reference_image_formats_to_inline() {
    let src = "![alt][ref]\n\n[ref]: /u \"t\"";
    // A resolved reference image normalizes to the inline form.
    assert_eq!(carve::to_carve(src).trim(), "![alt](/u \"t\")");
    assert_eq!(carve::to_html(&carve::to_carve(src)), carve::to_html(src));
}
