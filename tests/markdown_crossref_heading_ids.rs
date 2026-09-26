//! The Markdown target links a heading by its GFM slug and writes no `{#id}`
//! suffix (PART 11 §11). A reference is still recognized by the id the core
//! assigned: re-slugging every heading once lost a disambiguated `-2` id, and
//! the reference degraded to bare text (carve#352).

fn md(src: &str) -> String {
    carve::to_markdown(src)
}

#[test]
fn a_duplicate_heading_is_linked_by_its_deduplicated_slug() {
    // The second `Setup` has the Carve id `Setup-2` and the GFM slug `setup-1`.
    let out = md("## Setup\n\n## Setup\n\nSee </#setup-2>.\n");

    assert!(out.contains("[Setup](#setup-1)"), "{out}");
}

#[test]
fn no_heading_gains_a_suffix() {
    let out = md("## Setup\n\n## Setup\n\nSee </#setup-2>.\n");

    assert!(out.contains("## Setup\n\n## Setup\n"), "{out}");
    assert!(!out.contains("{#"), "{out}");
}

#[test]
fn a_heading_referenced_only_from_a_footnote_body_is_linked() {
    let out = md("# H\n\nBody[^n]\n\n[^n]: see </#h>\n");

    assert!(out.contains("# H\n"), "{out}");
    assert!(out.contains("[H](#h)"), "{out}");
}

#[test]
fn a_self_referencing_heading_does_not_slug_its_own_expansion() {
    // `</#a>` resolves to a link carrying the heading's own text, so counting it
    // would slug `# A </#a>` as `a-a`.
    let out = md("# A </#a>\n\nSee </#a>.\n");

    assert!(out.contains("](#a)."), "{out}");
    assert!(!out.contains("a-a"), "{out}");
}

#[test]
fn a_heading_in_a_table_cell_is_not_a_target() {
    // A block cell is flattened, so the heading inside it is not written as one
    // and the link to it keeps its authored destination (PART 11 §11a).
    let doc = carve::from_json(
        r##"{"type":"document","srcByteLength":0,"children":[
          {"type":"table","rows":[{"type":"table_row","cells":[{"type":"table_cell","header":false,"blocks":[
            {"type":"heading","level":2,"attrs":{"id":"in-cell"},"children":[{"type":"text","value":"Cell"}]}]}]}]},
          {"type":"paragraph","children":[{"type":"link","href":"#in-cell","children":[{"type":"text","value":"x"}]}]}]}"##,
    )
    .unwrap();
    let out = carve::render_markdown(&doc).unwrap();

    assert!(out.contains("[x](#in-cell)"), "{out}");
}
