//! An attribute line already parts two sibling sub-lists, so a tight item writes
//! no PART 9 §11 N1a boundary before an attribute-led list: the blanks would
//! loosen the item (markup-carve/carve#6005).

const CASES: &[(&str, &str)] = &[
    (
        "> - x\n>   - +\n>   {#i}\n>   - q\n",
        "> - x\n>   - {empty}\n>   {#i}\n>   - q\n",
    ),
    ("- x\n  - a\n  {#i}\n  - q\n", "- x\n  - a\n  {#i}\n  - q\n"),
    (
        "- x\n  1. a\n  {.c}\n  1. q\n",
        "- x\n  1. a\n  {.c}\n  1. q\n",
    ),
    (
        "- x\n  - y\n    * a\n    {k=v}\n    * q\n",
        "- x\n  - y\n    * a\n    {k=v}\n    * q\n",
    ),
];

#[test]
fn no_boundary_before_an_attribute_led_sub_list() {
    for (source, expected) in CASES {
        let formatted = carve::to_carve(source);
        assert_eq!(&formatted, expected, "fmt of {source:?}");
        assert_eq!(
            carve::to_html(source),
            carve::to_html(&formatted),
            "html of {source:?}"
        );
        assert_eq!(
            carve::to_carve(&formatted),
            formatted,
            "idempotence of {source:?}"
        );
    }
}

#[test]
fn the_boundary_stays_where_nothing_else_parts_the_lists() {
    let source = "- x\n  - a\n\n\n\n  - q\n";
    let formatted = carve::to_carve(source);
    assert_eq!(carve::to_html(source), carve::to_html(&formatted));
    assert_eq!(carve::to_carve(&formatted), formatted);
    assert!(formatted.contains("\n\n\n\n  - q"), "{formatted:?}");
}

#[test]
fn the_root_keeps_its_boundary_before_an_attribute_led_list() {
    let formatted = carve::to_carve("- a\n{#i}\n- q\n");
    assert_eq!(carve::to_carve(&formatted), formatted);
    assert_eq!(
        carve::to_html("- a\n{#i}\n- q\n"),
        carve::to_html(&formatted)
    );
}
