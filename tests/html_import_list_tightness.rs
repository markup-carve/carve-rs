//! HTML import keeps the tightness the source spelled.
//!
//! carve#1210: "a bare-text `<li>` imports as a TIGHT list item;
//! `<li><p>...</p></li>` stays loose. HTML draws the tight/loose distinction the
//! same way Carve does, and import preserves source structure rather than
//! normalizing." Corpus-convert `27-html-a-bare-text-list-item-imports-tight`
//! and `28-html-a-mixed-list-stays-loose` are the two halves.
//!
//! The importer used to hardwire `tight: false`, so `<ul><li>one</li></ul>`
//! came back as `- one\n\n` and rendered `<li><p>one</p></li>` - a paragraph the
//! source never wrote, on every bare-text list any HTML document holds.
//!
//! ## The predicate, and why it is not "every item is bare text"
//!
//! Carve spells tightness per LIST, not per item, so a MIXED list has to
//! resolve one way. It resolves LOOSE, the way CommonMark resolves it: one
//! paragraph item loosens the whole list. Resolving tight instead would drop the
//! paragraph that item actually spelled, which is the loss the rule exists to
//! prevent.
//!
//! That makes the whole question "does any item spell a paragraph", so ONLY A
//! DIRECT `<p>` VOTES:
//!
//! ```text
//! tight = !items.any(li => li has a direct <p> child)
//! ```
//!
//! Asking instead whether every item is BARE TEXT is the intermediate shape
//! markup-carve/carve-js#1106 shipped and markup-carve/carve-js#1110 corrected.
//! It loosens four shapes that spell no paragraph at all - an item holding only
//! a block quote, only a code block, only a sublist, or nothing - and each then
//! re-renders with a `<p>` its source never wrote. Those four are the controls
//! below, and they are what separates this rule from the one that over-applies.

use carve::{html_to_carve, to_html, HtmlImportOptions};

fn migrated(html: &str) -> String {
    html_to_carve(html, &HtmlImportOptions::default())
        .expect("import")
        .value
}

#[test]
fn a_bare_text_item_imports_tight() {
    // corpus-convert 27. The rendered document is the case's expected.html.
    let carve = migrated("<ul><li>one</li><li>two</li></ul>");
    assert_eq!(carve, "- one\n- two\n");
    assert_eq!(
        to_html(&carve),
        "<ul>\n  <li>one</li>\n  <li>two</li>\n</ul>"
    );
}

#[test]
fn a_paragraph_wrapped_item_stays_loose() {
    let carve = migrated("<ul><li><p>one</p></li><li><p>two</p></li></ul>");
    assert_eq!(carve, "- one\n\n- two\n");
    assert_eq!(
        to_html(&carve),
        "<ul>\n  <li><p>one</p></li>\n  <li><p>two</p></li>\n</ul>"
    );
}

#[test]
fn a_mixed_list_stays_loose() {
    // corpus-convert 28, and the half markup-carve/carve-js#1110 had to correct.
    // Tightness is per LIST, so the paragraph item decides for both, and it
    // decides LOOSE - normalizing to tight would drop the paragraph item two
    // actually spelled.
    let carve = migrated("<ul><li>one</li><li><p>two</p></li></ul>");
    assert_eq!(carve, "- one\n\n- two\n");
    assert_eq!(
        to_html(&carve),
        "<ul>\n  <li><p>one</p></li>\n  <li><p>two</p></li>\n</ul>"
    );
}

#[test]
fn an_item_that_spells_no_paragraph_does_not_loosen_the_list() {
    // THE FOUR CONTROLS. None of these holds a `<p>`, so none may loosen its
    // list - and each would, under "tight only when every item is bare text",
    // coming back with a paragraph its source never wrote.
    for (html, carve, rendered) in [
        // Only a block quote.
        (
            "<ul><li><blockquote>q</blockquote></li></ul>",
            "- > q\n",
            "<ul>\n  <li>\n    <blockquote><p>q</p></blockquote>\n  </li>\n</ul>",
        ),
        // Only a sublist. A nested `<ul>` is structure, not a paragraph
        // wrapper.
        (
            "<ul><li><ul><li>a</li></ul></li></ul>",
            "- - a\n",
            "<ul>\n  <li>\n    <ul>\n      <li>a</li>\n    </ul>\n  </li>\n</ul>",
        ),
        // Bare text beside a sublist: the HTML of a TIGHT item with a sublist.
        (
            "<ul><li>one<ul><li>a</li></ul></li></ul>",
            "- one\n  - a\n",
            "<ul>\n  <li>one\n    <ul>\n      <li>a</li>\n    </ul>\n  </li>\n</ul>",
        ),
    ] {
        assert_eq!(migrated(html), carve, "for {html}");
        assert_eq!(to_html(&migrated(html)), rendered, "for {html}");
    }
    // Only a code block, and an empty item: both spell no paragraph either, and
    // both write a marker whose exact bytes are their own question, so only the
    // tightness is asserted - the list must hold no blank line between items.
    for html in [
        "<ul><li><pre><code>x</code></pre></li><li>two</li></ul>",
        "<ul><li></li><li>two</li></ul>",
    ] {
        let carve = migrated(html);
        assert!(
            !carve.contains("\n\n"),
            "loosened a list no item spelled a paragraph in: {html} -> {carve:?}"
        );
    }
}

#[test]
fn a_task_list_checkbox_does_not_loosen_the_list() {
    // The `<input>` is consumed into the `[x]` marker rather than imported, so
    // it is not a paragraph and does not vote.
    let carve = migrated(
        "<ul><li><input type=\"checkbox\" checked>done</li><li><input type=\"checkbox\">todo</li></ul>",
    );
    assert!(!carve.contains("\n\n"), "loosened a task list: {carve:?}");
}
