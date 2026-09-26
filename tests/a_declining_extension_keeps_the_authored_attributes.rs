//! An extension that renders NOTHING for a container must leave the container
//! exactly as the core renderer would write it (markup-carve/carve-rs#1979).
//!
//! `Tabs` and `CodeGroup` rewrite their container to an `ExtensionCarrier` in a
//! before-render pass, then decline at render time when no child is an item.
//! The core renderer's carrier fallback writes the SENTINEL NAME as the class
//! and no attributes at all, so `{#tb data-x=1}` on a tab set with no tabs
//! reached nothing: the id, the key-values and the author's own classes were
//! all absent, and nothing reported it.
//!
//! The decline is structural - `as_tab` reads a kind and a class, and a code
//! group's panel is any code block - so it is decided BEFORE the rewrite. A
//! container that would decline is never rewritten, which is what carve-js does
//! too: it asks the same question at render time and hands back the ORIGINAL
//! node rather than a carrier.
//!
//! Measured against carve-js `8e427ade` with its own `tabs` and `codeGroup`
//! extensions enabled, so both sides answer with the extension loaded.

use carve::{CodeGroup, Options, Tabs};

fn with_tabs(src: &str) -> String {
    let ext = Tabs::new();
    carve::to_html_with_options(src, &Options::new().with_extension(&ext))
}

fn with_code_group(src: &str) -> String {
    let ext = CodeGroup::new();
    carve::to_html_with_options(src, &Options::new().with_extension(&ext))
}

/// The ticket's document.
#[test]
fn an_unrecognized_tabs_body_keeps_its_id_and_key_values() {
    assert_eq!(
        with_tabs("{#tb data-x=1}\n:::: tabs\n::: one\na\n:::\n::::\n"),
        "<div class=\"tabs\" id=\"tb\" data-x=\"1\">\n  <div class=\"one\">\n    \
         <p>a</p>\n  </div>\n</div>"
    );
}

/// The author's OWN class is part of the same loss, and the div spelling puts
/// the structural class in an authored slot rather than in front.
#[test]
fn it_keeps_the_authored_classes_too() {
    assert_eq!(
        with_tabs("{#tb .mine data-x=1}\n:::: tabs\n::: one\na\n:::\n::::\n"),
        "<div class=\"tabs mine\" id=\"tb\" data-x=\"1\">\n  <div class=\"one\">\n    \
         <p>a</p>\n  </div>\n</div>"
    );
    assert_eq!(
        with_tabs("{#tb .tabs data-x=1}\n::::\n::: one\na\n:::\n::::\n"),
        "<div id=\"tb\" class=\"tabs\" data-x=\"1\">\n  <div class=\"one\">\n    \
         <p>a</p>\n  </div>\n</div>"
    );
}

/// §13 binds both constructs, so the same loss was in both renderers.
#[test]
fn an_unrecognized_code_group_body_keeps_them_as_well() {
    assert_eq!(
        with_code_group("{#cg data-z=3}\n:::: code-group\n::: one\na\n:::\n::::\n"),
        "<div class=\"code-group\" id=\"cg\" data-z=\"3\">\n  <div class=\"one\">\n    \
         <p>a</p>\n  </div>\n</div>"
    );
}

/// THE EXTENSION STILL RUNS WHERE IT HAS SOMETHING TO RENDER. The gate is on
/// the container holding an item, not on the extension being loaded, so a tab
/// set with one tab and a code group with one block are untouched.
#[test]
fn a_recognized_body_still_renders_its_tab_set() {
    let html = with_tabs("{#tb data-x=1}\n:::: tabs\n::: tab [One]\na\n:::\n::::\n");
    assert!(html.contains("class=\"tabs-radio\""), "{html}");
    assert!(html.contains(">One</label>"), "{html}");
    assert!(html.contains("id=\"tb\""), "{html}");
    assert!(html.contains("data-x=\"1\""), "{html}");

    let group = with_code_group("{#cg}\n:::: code-group\n``` rust\nfn main() {}\n```\n::::\n");
    assert!(group.contains("id=\"cg\""), "{group}");
    assert!(!group.contains("carve-code-group"), "{group}");
}

/// A NESTED RECOGNIZED SET INSIDE A DECLINING ONE still renders.
///
/// The rewrite used to skip a rewritten container's children outright, so a real
/// tab set written inside a tab set with no tabs of its own never became one.
/// Declining leaves the walk to the ordinary recursion, which reaches it.
#[test]
fn a_tab_set_inside_a_declining_container_still_renders() {
    let html = with_tabs(
        "{#outer}\n:::::: tabs\n::::: box\n:::: tabs\n::: tab [One]\na\n:::\n::::\n:::::\n::::::\n",
    );
    assert!(html.contains("id=\"outer\""), "{html}");
    assert!(html.contains("class=\"tabs-radio\""), "{html}");
    assert!(html.contains(">One</label>"), "{html}");
}

/// THE SENTINEL NAME IS NOT A CLASS ANYWHERE in these documents. It is the
/// tell the fallback left behind, and it is what a search would find if the
/// gate were removed.
#[test]
fn no_sentinel_name_reaches_the_output() {
    for html in [
        with_tabs("{#tb}\n:::: tabs\n::: one\na\n:::\n::::\n"),
        with_tabs("{.tabs}\n::::\ntext\n::::\n"),
        with_code_group("{#cg}\n:::: code-group\n::: one\na\n:::\n::::\n"),
        with_code_group("{.code-group}\n::::\ntext\n::::\n"),
    ] {
        assert!(!html.contains("carve-tabs"), "{html}");
        assert!(!html.contains("carve-code-group"), "{html}");
    }
}
