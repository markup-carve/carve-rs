//! An admonition's TITLE comes before its grouping LABEL, in every target.
//!
//! `::: tip "Pro Tip" [Build]` writes them in that order and the HTML renderer
//! emits them in that order. The non-HTML targets prepended the title to the body
//! and then prepended the label on top of that, which put the label ABOVE the
//! title -- in plain, Markdown and ANSI alike, since all three shared the shape
//! (carve#352, corpus 42-admonitions-4).

const SOURCE: &str = "::: tip \"Pro Tip\" [Build]\nSave early, save often.\n:::\n";

#[test]
fn plain_puts_the_title_first() {
    assert_eq!(
        carve::to_plain_text(SOURCE),
        "Pro Tip\n\nBuild\n\nSave early, save often.\n"
    );
}

#[test]
fn markdown_puts_the_title_first() {
    assert_eq!(
        carve::to_markdown(SOURCE),
        "**Pro Tip**\n\n**Build**\n\nSave early, save often.\n"
    );
}

#[test]
fn ansi_puts_the_title_first() {
    let out = carve::to_ansi(SOURCE);
    let title = out.find("Pro Tip").expect("the title");
    let label = out.find("Build").expect("the label");
    assert!(title < label, "label came before the title: {out:?}");
}

#[test]
fn the_html_target_agrees() {
    let html = carve::to_html(SOURCE);
    let title = html.find("Pro Tip").expect("the title");
    let label = html.find("Build").expect("the label");
    assert!(title < label, "html disagrees: {html:?}");
}

#[test]
fn a_label_without_a_title_still_renders() {
    // The label must not be dropped when there is no title to sit above it.
    let src = "::: tip [Build]\nBody.\n:::\n";
    assert_eq!(carve::to_plain_text(src), "Build\n\nBody.\n");
}

#[test]
fn a_title_without_a_label_still_renders() {
    let src = "::: tip \"Pro Tip\"\nBody.\n:::\n";
    assert_eq!(carve::to_plain_text(src), "Pro Tip\n\nBody.\n");
}
