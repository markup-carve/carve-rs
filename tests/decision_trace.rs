//! `carve::decision_trace` writes the block records of carve's
//! `docs/decision-trace.md`, record for record what the spec model writes.

fn trace(source: &str) -> Vec<String> {
    carve::decision_trace(source)
        .lines()
        .map(str::to_owned)
        .collect()
}

#[test]
fn the_documented_example_traces_as_the_spec_model_does() {
    assert_eq!(
        trace("- a\n  > q\nlazy\n"),
        [
            r#"{"trace":"carve-decision-trace","version":1,"engine":"carve-rs"}"#,
            r#"{"line":1,"step":"S5","owner":"doc/item:1","rule":"CARVE-P0-024","form":"item","cell":"B-item-start"}"#,
            r#"{"line":2,"step":"S5","owner":"doc/item:1/quote:2","rule":"CARVE-P0-024","form":"quote","cell":"B-quote-start"}"#,
            r#"{"line":3,"step":"S4","owner":"doc/item:1/quote:2","rule":"CARVE-P0-006","form":"lazy","reached":"doc"}"#,
        ]
    );
}

#[test]
fn a_fence_payload_is_an_s1_record_of_its_family() {
    assert_eq!(
        trace("> ```\n> x\n\n> ```\n")[1..],
        [
            r#"{"line":1,"step":"S5","owner":"doc/quote:1/code:1","rule":"CARVE-P0-024","form":"quote","cell":"B-quote-start"}"#,
            r#"{"line":2,"step":"S1","owner":"doc/quote:1/code:1","rule":"CARVE-P0-005","form":"payload","cell":"B-code-keep"}"#,
            r#"{"line":3,"step":"S5","owner":"doc","rule":"CARVE-P0-003","form":"blank","closed":1,"reached":"doc"}"#,
            r#"{"line":4,"step":"S5","owner":"doc/quote:4/code:4","rule":"CARVE-P0-024","form":"quote","cell":"B-quote-start"}"#,
        ]
    );
}

#[test]
fn a_div_closer_is_owned_by_the_div_it_closes() {
    assert_eq!(
        trace("::: note\ntext\n:::\n")[1..],
        [
            r#"{"line":1,"step":"S5","owner":"doc/div:1","rule":"CARVE-P0-024","form":"div","cell":"B-div-start"}"#,
            r#"{"line":2,"step":"S5","owner":"doc/div:1","rule":"CARVE-P0-024","form":"text"}"#,
            r#"{"line":3,"step":"S5","owner":"doc/div:1","rule":"CARVE-P0-024","form":"closer","cell":"B-div-end"}"#,
        ]
    );
}

#[test]
fn a_plus_and_its_follower_name_the_plus_line() {
    assert_eq!(
        trace("- a\n+\n> q\n")[1..],
        [
            r#"{"line":1,"step":"S5","owner":"doc/item:1","rule":"CARVE-P0-024","form":"item","cell":"B-item-start"}"#,
            r#"{"line":2,"step":"S2","owner":"doc/item:1","rule":"CARVE-P9-067","form":"plus","cell":"B-plus-start"}"#,
            r#"{"line":3,"step":"S5","owner":"doc/quote:3","rule":"CARVE-P0-024","form":"quote","cell":"B-quote-start","closed":1,"reached":"doc","plus":2}"#,
        ]
    );
}

#[test]
fn frontmatter_closed_on_the_last_line_is_input_throughout() {
    assert_eq!(
        trace("---\na: 1\n---")[1..],
        [
            r#"{"line":1,"step":"INPUT","owner":"doc","rule":null,"form":"frontmatter"}"#,
            r#"{"line":2,"step":"INPUT","owner":"doc","rule":null,"form":"frontmatter"}"#,
            r#"{"line":3,"step":"INPUT","owner":"doc","rule":null,"form":"frontmatter"}"#,
        ]
    );
}
