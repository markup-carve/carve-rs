//! A critic comment is VISIBLE content: the HTML target renders it as
//! `<span class="critic-comment"> note </span>`. Dropping it in the text targets
//! made two targets of one engine disagree about whether the document says it --
//! the same class of inconsistency as the unresolved footnote reference
//! (carve-rs#311, carve-rs#312), and carve-php was again the engine that had it
//! right (carve#352, corpus 33-editorial-markup).

#[test]
fn plain_keeps_a_critic_comment() {
    assert_eq!(carve::to_plain_text("b{# note #}\n"), "b note\n");
}

#[test]
fn ansi_keeps_a_critic_comment() {
    assert!(carve::to_ansi("b{# note #}\n").contains("note"));
}

#[test]
fn every_target_agrees_the_content_is_there() {
    let src = "b{# note #}\n";
    assert!(carve::to_html(src).contains("note"));
    assert!(carve::to_plain_text(src).contains("note"));
    assert!(carve::to_ansi(src).contains("note"));
    // The carve target already reproduced it, which is what made the omission look
    // deliberate rather than missing.
    assert!(carve::to_carve(src).contains("{# note #}"));
}

#[test]
fn it_survives_alongside_the_other_editorial_marks() {
    let src = "a {+ins+} {-del-} {~old~>new~} b{# note #}\n";
    assert_eq!(carve::to_plain_text(src), "a ins ~del~ ~old~new b note\n");
}
