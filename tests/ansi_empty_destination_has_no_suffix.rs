//! An empty authored destination prints no dim ` ()` suffix in ANSI, as a
//! fragment destination already does. A destination blanked by the denylist
//! keeps its ` ()`; `ansi_destination_denylist` pins that.

const LINK: &str = "\u{1b}[4m\u{1b}[34mx\u{1b}[0m";

#[test]
fn an_inline_link_with_an_empty_destination_has_no_suffix() {
    assert_eq!(carve::to_ansi("[x]()\n").trim_end(), LINK);
    assert_eq!(carve::to_ansi("[x]( \"t\")\n").trim_end(), LINK);
}

#[test]
fn a_reference_to_an_empty_definition_has_no_suffix() {
    assert_eq!(carve::to_ansi("[r]:\n\n[x][r]\n").trim_end(), LINK);
}

#[test]
fn a_non_empty_destination_keeps_its_suffix() {
    assert_eq!(
        carve::to_ansi("[x](/u)\n").trim_end(),
        format!("{LINK}\u{1b}[2m (/u)\u{1b}[0m")
    );
}
