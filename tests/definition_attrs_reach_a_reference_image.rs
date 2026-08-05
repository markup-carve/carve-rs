//! A definition's trailing attribute block reaches a reference IMAGE, not only a
//! reference link.
//!
//! The clause is NORMATIVE and spells the case out: "AN IMAGE REFERENCE RESOLVES
//! THE SAME ENTRY - `![alt][ex]` looks the label up in the same `linkDefs` table
//! and takes the same three fields, so `[ex]: /i.png {.wide}` gives
//! `<img src="/i.png" alt="alt" class="wide">`."
//!
//! This engine took `href` and `title` from that entry and stopped. Two of three
//! fields transferring is not a rule, it is where the implementation stopped - and
//! the clause names this engine and carve-js as the two that stopped there. The
//! link branch a few lines above already did the merge; the image branch was
//! missing it (carve#697).
//!
//! THE MERGE IS §15 A3's, the same one the link branch uses: the definition's list
//! first, the use site's second - so a repeated key takes the LAST value and
//! classes ACCUMULATE in source order. Asserting only that the class is present
//! would pass for a merge that REPLACED instead of accumulating, so the order is
//! asserted exactly.
//!
//! Verified against the executable spec, carve-php and carve-js (which fixed its
//! half in carve-js#683) - all three produce these exact strings.

use carve::to_html;

fn html(src: &str) -> String {
    to_html(src).trim().to_string()
}

#[test]
fn a_definitions_attributes_reach_the_image() {
    assert_eq!(
        html("![a][ex]\n\n[ex]: /i.png {.wide}\n"),
        r#"<img src="/i.png" alt="a" class="wide">"#
    );
}

#[test]
fn the_merge_is_a3_with_the_definition_first() {
    // Classes accumulate in source order; the repeated id takes the use site's.
    assert_eq!(
        html("![a][ex]{.internal #b}\n\n[ex]: /i.png {.external #a}\n"),
        r#"<img src="/i.png" alt="a" class="external internal" id="b">"#
    );
}

#[test]
fn a_collapsed_image_reference_gets_them_too() {
    assert_eq!(
        html("![ex][]\n\n[ex]: /i.png {.wide}\n"),
        r#"<img src="/i.png" alt="ex" class="wide">"#
    );
}

#[test]
fn they_arrive_alongside_the_title_not_instead_of_it() {
    // `title` already crossed before this fix; pinned so the new merge cannot be
    // written in a way that displaces it.
    assert_eq!(
        html("![a][ex]\n\n[ex]: /i.png \"T\" {.wide}\n"),
        r#"<img src="/i.png" alt="a" title="T" class="wide">"#
    );
}

#[test]
fn a_reference_link_still_works_as_before() {
    // The branch that was already right, and the one this fix was copied from.
    assert_eq!(
        html("[t][ex]{.internal #b}\n\n[ex]: /u {.external #a}\n"),
        r#"<p><a href="/u" class="external internal" id="b">t</a></p>"#
    );
}

#[test]
fn an_unresolved_reference_image_gets_nothing() {
    // No definition, so nothing to merge, and the literal source survives whole.
    assert_eq!(html("![a][none]{.x}\n"), r#"<p>![a][none]{.x}</p>"#);
}

#[test]
fn a_direct_image_is_untouched() {
    // The boundary: a direct image never consults the table.
    assert_eq!(
        html("![a](/d.png){.x}\n"),
        r#"<img src="/d.png" alt="a" class="x">"#
    );
}

#[test]
fn an_inline_reference_image_gets_them() {
    // The resolution pass walks children; an image mid-paragraph goes through the
    // same branch and must not be missed.
    let out = html("see ![a][ex] here\n\n[ex]: /i.png {.wide}\n");
    assert!(out.contains(r#"class="wide""#), "{out}");
}
