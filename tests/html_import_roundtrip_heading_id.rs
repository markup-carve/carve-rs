//! `roundtrip` mode reads a heading id the renderer generated as generated, and
//! one an author wrote as authored (carve-rs#1354).
//!
//! The two are ONE test on purpose. A generated id and an authored id whose
//! value is the same slug render to different bytes - `render_heading` puts a
//! generated id after every authored attribute and an authored one in the slot
//! it was written in - so a fixture holding only one of them cannot see the
//! placement difference that is the whole defect. Either half alone passes
//! under a wrong rule: dropping every heading id passes the generated case,
//! keeping every heading id passes the authored one.

use carve::{html_to_carve, to_html, HtmlImportMode, HtmlImportOptions};

fn roundtrip(source: &str) -> String {
    let html = to_html(source);
    let options = HtmlImportOptions {
        mode: HtmlImportMode::Roundtrip,
        ..HtmlImportOptions::default()
    };
    let back = html_to_carve(&html, &options).unwrap().value;
    // The source coming back is the contract the corpus round trip measures;
    // the render coming back is what the ticket says the re-emitted id broke.
    assert_eq!(
        to_html(&back),
        html,
        "re-rendering the imported source did not reproduce the HTML it was imported from"
    );
    back
}

#[test]
fn a_generated_heading_id_and_an_authored_one_round_trip_to_their_own_source() {
    // No attribute block: the id in the HTML is the slug the renderer derived,
    // and it is written after the class.
    let generated = "- a\n  {.k}\n  # H\n";
    // An authored id whose value EQUALS that slug, written before the class -
    // which is where the renderer puts it back.
    let authored = "- a\n  {#H .k}\n  # H\n";

    assert_ne!(
        to_html(generated),
        to_html(authored),
        "the two documents must render differently, or this fixture cannot discriminate"
    );

    assert_eq!(roundtrip(generated), generated);
    assert_eq!(roundtrip(authored), authored);
}

#[test]
fn an_authored_heading_id_keeps_its_slot_wherever_it_was_written() {
    // Written LAST, so it sits where a generated id would - only its value
    // says otherwise. Position alone would eat this one.
    assert_eq!(
        roundtrip("- a\n  {.k #Other}\n  # H\n"),
        "- a\n  {.k #Other}\n  # H\n"
    );
    assert_eq!(
        roundtrip("- a\n  {#Other .k}\n  # H\n"),
        "- a\n  {#Other .k}\n  # H\n"
    );
}

#[test]
fn a_deduplicated_generated_heading_id_is_still_generated() {
    // `next_heading_id` numbers a repeated slug `H`, `H-2`, ... so the second
    // heading's id is generated too, and must not come back as a slot.
    let html = r#"<ul><li>a<h1 class="k" id="H-2">H</h1></li></ul>"#;
    let options = HtmlImportOptions {
        mode: HtmlImportMode::Roundtrip,
        ..HtmlImportOptions::default()
    };
    assert_eq!(
        html_to_carve(html, &options).unwrap().value,
        "- a\n  {.k}\n  # H\n"
    );
}

#[test]
fn outside_roundtrip_mode_a_heading_id_stays_authored() {
    // carve-rs#1324's ruling, unchanged: input that is not this engine's own
    // output carries no promise that the id was generated, so it is kept.
    let html = r#"<ul><li>a<h1 class="k" id="H">H</h1></li></ul>"#;
    for mode in [HtmlImportMode::Safe, HtmlImportMode::Semantic] {
        let options = HtmlImportOptions {
            mode,
            ..HtmlImportOptions::default()
        };
        assert_eq!(
            html_to_carve(html, &options).unwrap().value,
            "- a\n  {.k #H}\n  # H\n",
            "mode {mode:?} dropped an id it cannot know is generated"
        );
    }
}
