//! carve#4680: a reference label that crosses a line and fails stays source
//! text in every slot, with no rescan, and a destination after it stays text.

use carve::to_html;

#[test]
fn an_explicit_label_keeps_the_whole_pair_verbatim() {
    assert_eq!(to_html("[*t*][a\nb]\n").trim(), "<p>[*t*][a\nb]</p>");
    assert_eq!(to_html("![*t*][a\nb]\n").trim(), "<p>![*t*][a\nb]</p>");
}

#[test]
fn a_destination_or_attribute_block_after_the_label_is_text() {
    assert_eq!(to_html("[x][a\nb](u)\n").trim(), "<p>[x][a\nb](u)</p>");
    assert_eq!(to_html("[x][a\nb]{.c}\n").trim(), "<p>[x][a\nb]{.c}</p>");
}

#[test]
fn a_definition_never_resolves_it_and_no_abbreviation_reaches_it() {
    let defined = to_html("[x][a\nb]\n\n[a b]: /u\n");
    assert!(!defined.contains("<a "), "{defined}");
    let reserved = to_html("[x][@a\nb]\n\n*[b]: z\n");
    assert!(!reserved.contains("<abbr"), "{reserved}");
}
