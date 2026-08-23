//! A CONTAINER THE RENDERER WROTE COMES BACK AS THAT CONTAINER
//! (markup-carve/carve-rs#1240, markup-carve/carve#1502).
//!
//! `render_admonition` sends an `Admonition` to one of two shapes: a Tier-1
//! kind becomes `<aside class="admonition {kind}">`, every other kind becomes
//! `<div class="{kind}">`. The import is that mapping read backwards, so a tab
//! set, a code group, a panel and a callout all survive - and so does the next
//! container an extension invents, which is the half a list of names would go on
//! losing.
//!
//! WHY THE ASSERTION IS ON NODE KINDS AND NOT ON BYTES
//! (markup-carve/carve-js#1295). Every input below re-renders to byte-identical
//! HTML with the defect present: an unwrapped `<aside>` gives back the same
//! `<p>` it went in as, and a `<div class="tabs">` kept as a `Div` carrying a
//! `.tabs` class renders `<div class="tabs">` again. An HTML-to-HTML check
//! therefore reports success while the callout has stopped being a callout. The
//! node is the only place the loss is visible, so the node is what is measured.

use carve::{
    html_to_ast, html_to_carve, parse, render_html, BlockNode, Document, HtmlImportOptions,
};

fn imported(html: &str) -> String {
    html_to_carve(html, &HtmlImportOptions::default())
        .expect("import")
        .value
}

fn imported_ast(html: &str) -> Document {
    html_to_ast(html, &HtmlImportOptions::default())
        .expect("import")
        .value
}

/// Every block node's type name, in document order - the unit #1295 requires.
fn kinds(blocks: &[BlockNode], out: &mut Vec<String>) {
    for block in blocks {
        match block {
            BlockNode::Admonition(a) => {
                out.push(format!("admonition:{}", a.kind));
                kinds(&a.children, out);
            }
            BlockNode::Div(d) => {
                out.push("div".to_string());
                kinds(&d.children, out);
            }
            BlockNode::Paragraph(_) => out.push("paragraph".to_string()),
            other => out.push(
                format!("{other:?}")
                    .split('(')
                    .next()
                    .unwrap()
                    .to_lowercase(),
            ),
        }
    }
}

fn block_kinds(doc: &Document) -> Vec<String> {
    let mut out = Vec::new();
    kinds(&doc.children, &mut out);
    out
}

/// A Tier-1 callout: the construct that did not degrade, it LEFT. The `<aside>`
/// was unwrapped, so the admonition node was gone and the body was all that
/// came back.
#[test]
fn a_tier1_callout_survives_its_own_html() {
    let html = render_html(&parse("::: note\nbody\n:::\n")).unwrap();
    assert!(html.contains("<aside class=\"admonition note\""), "{html}");

    assert_eq!(
        block_kinds(&imported_ast(&html)),
        vec!["admonition:note", "paragraph"]
    );
    assert_eq!(imported(&html), "::: note\nbody\n:::\n");
}

/// A tab set and its panel: these DID come back, as generic divs carrying the
/// structural class. Byte-identical HTML on a re-render, and the wrong node.
#[test]
fn a_tab_set_and_its_panel_survive_as_containers() {
    let html = "<div class=\"tabs\"><div class=\"tabs-panel\"><p>a</p></div></div>";
    assert_eq!(
        block_kinds(&imported_ast(html)),
        vec!["admonition:tabs", "admonition:tabs-panel", "paragraph"]
    );
    assert_eq!(imported(html), "::: tabs\n:::: tabs-panel\na\n::::\n:::\n");
}

#[test]
fn a_code_group_survives_as_a_container() {
    let html = "<div class=\"code-group\"><p>x</p></div>";
    assert_eq!(
        block_kinds(&imported_ast(html)),
        vec!["admonition:code-group", "paragraph"]
    );
}

/// THE CATEGORY, not the list. No extension claims `sidebar`, and it rebuilds
/// for the same reason the named ones do - the renderer would have written this
/// exact HTML for it.
#[test]
fn a_container_no_extension_claims_survives_too() {
    let html = "<div class=\"sidebar\"><p>x</p></div>";
    assert_eq!(
        block_kinds(&imported_ast(html)),
        vec!["admonition:sidebar", "paragraph"]
    );
    assert_eq!(imported(html), "::: sidebar\nx\n:::\n");
}

/// THE GUARD, and it is the writer's own rule rather than a copy of it. A fence
/// opener reads its type word as `[a-zA-Z_][\w-]*`, so a class outside that
/// shape cannot be the fence word: written there it would read back as a
/// paragraph, and the element would lose both its class and its structure.
#[test]
fn a_class_no_fence_opener_can_spell_stays_a_class() {
    let html = "<div class=\"2col\"><p>x</p></div>";
    assert_eq!(block_kinds(&imported_ast(html)), vec!["div", "paragraph"]);
    assert_eq!(imported(html), "{.2col}\n:::\nx\n:::\n");
}

/// The class PAIR is what marks a rendered callout. A bare `<aside>` is somebody
/// else's sidebar and keeps the unwrap it has always had.
#[test]
fn an_aside_that_is_not_a_callout_is_still_unwrapped() {
    assert_eq!(
        block_kinds(&imported_ast("<aside><p>x</p></aside>")),
        vec!["paragraph"]
    );
}

/// The structural class becomes the fence word and is NOT kept beside it: the
/// renderer writes it back from the kind, so keeping it would emit
/// `class="tabs tabs"` on the next render.
#[test]
fn an_extra_class_rides_beside_the_name_the_fence_consumed() {
    assert_eq!(
        imported("<div class=\"tabs extra\"><p>x</p></div>"),
        "{.extra}\n::: tabs\nx\n:::\n"
    );
}

/// The written form is a fixed point of this engine's own formatter, which is
/// what the spec asks of an importer and what lets a shared fixture compare
/// byte-for-byte.
#[test]
fn the_written_container_is_a_formatter_fixed_point() {
    for source in [
        "::: note\nbody\n:::\n",
        "::: tabs\n:::: tabs-panel\na\n::::\n:::\n",
        "::: code-group\nx\n:::\n",
    ] {
        let out = imported(&render_html(&parse(source)).unwrap());
        assert_eq!(carve::render_carve(&parse(&out)).unwrap(), out, "{source}");
    }
}

/// THE ENDNOTES ROW, pinned against carve-php's answer, which loses the content.
/// Importing this to `[^1]: n` produces a definition no reference reaches, and
/// an unreferenced definition renders to the empty string - so the note's text
/// leaves the document silently. Degrading to the `<hr>` and `<ol>` the section
/// is built from keeps every byte a reader could see. A footnote that HAS a
/// `doc-noteref` reference already rebuilds here.
#[test]
fn an_unreferenced_endnotes_section_keeps_its_content_visible() {
    let out = imported(
        "<section role=\"doc-endnotes\"><hr><ol><li id=\"fn1\"><p>n</p></li></ol></section>",
    );
    assert!(out.contains('n'), "{out}");
    assert!(!render_html(&parse(&out)).unwrap().is_empty(), "{out}");
}

/// THE TITLE IS MARKED BY ITS CLASS, NOT BY THE GENERATED ID.
///
/// `render_admonition` emits `id="adm-N"` only for a Tier-1 kind with no
/// authored name, so both shapes here render a BARE
/// `<p class="admonition-title">`. Keying the lift on the id left their titles
/// in the body, written back as an ordinary paragraph carrying the renderer's
/// own class: the container came back title-less and one paragraph longer.
#[test]
fn a_title_that_renders_with_no_id_is_still_lifted() {
    for source in [
        "::: sidebar \"A\"\nx\n:::\n",
        "{aria-label=Mine}\n::: note \"A\"\nx\n:::\n",
    ] {
        let html = render_html(&parse(source)).unwrap();
        assert!(html.contains("<p class=\"admonition-title\">"), "{html}");
        assert!(!html.contains("id=\"adm-"), "{html}");
        assert_eq!(imported(&html), source);
    }
}
