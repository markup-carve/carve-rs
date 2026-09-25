//! CARVE-P10-010: a placed element's own tags carry ONE column. They may take
//! the nesting indentation or sit at column 0, and an element whose opener and
//! closer disagree conforms under neither choice.
//!
//! `render_extension_carrier` indents the FIRST line of what an extension
//! returns, so a renderer whose parts all self-pad - which is what every line
//! after the first needs - pays that pad twice on whichever part comes first.
//! carve-rs#1906 fixed the `<dl>` when it led; carve-rs#1969 is the same defect
//! at the parts that can lead instead, plus the `toc` fragment the clause
//! anchors at column 0 whatever the nesting.
//!
//! EVERY CASE SPELLS THE MARKER OFF COLUMN 0. At column 0 the three engines
//! already agree, which is why the suites could not see this class.
//! The goldens are carve-js `c46d0cb08` output, measured directly.

use carve::{Glossary, Index, Options, TocPlacement};

/// The column each line carrying `needle` is written at.
fn columns(html: &str, needle: &str) -> Vec<usize> {
    html.lines()
        .filter(|line| line.trim_start().starts_with(needle))
        .map(|line| line.len() - line.trim_start().len())
        .collect()
}

fn glossary_html(source: &str) -> String {
    let ext = Glossary::new();
    carve::to_html_with_options(source, &Options::new().with_extension(&ext))
        .trim()
        .to_string()
}

fn index_html(source: &str) -> String {
    let ext = Index::new();
    carve::to_html_with_options(source, &Options::new().with_extension(&ext))
        .trim()
        .to_string()
}

fn toc_html(source: &str) -> String {
    let ext = TocPlacement::new();
    carve::to_html_with_options(source, &Options::new().with_extension(&ext))
        .trim()
        .to_string()
}

#[test]
fn a_glossary_title_sits_at_the_element_it_titles() {
    let out = glossary_html("> ::: glossary \"G\"\n> :: API\n> : Interface\n> :::\n");
    assert_eq!(
        out,
        "<blockquote>\n  <p class=\"admonition-title\">G</p>\n  <dl class=\"glossary\">\n    \
         <dt id=\"gloss-api\">API</dt>\n    <dd>Interface</dd>\n  </dl>\n</blockquote>"
    );
    assert_eq!(columns(&out, "<p class=\"admonition-title\""), vec![2]);
    assert_eq!(columns(&out, "<dl"), vec![2]);
    assert_eq!(columns(&out, "</dl>"), vec![2]);
}

#[test]
fn a_block_authored_above_a_glossarys_rows_sits_with_it() {
    // The part that leads changes with the document, and the pad must come off
    // whichever one it is - a title above, or an authored block above that.
    let out = glossary_html("# H\n\n::: glossary\nlead\n\n:: a\n: 1\n:::\n");
    assert_eq!(columns(&out, "<p>lead</p>"), vec![2]);
    assert_eq!(columns(&out, "<dl"), vec![2]);
    assert_eq!(columns(&out, "</dl>"), vec![2]);
}

#[test]
fn an_index_title_sits_at_the_list_it_titles() {
    let out = index_html("> A :index[parser] here.\n>\n> ::: index \"I\"\n> :::\n");
    assert_eq!(columns(&out, "<p class=\"admonition-title\""), vec![2]);
    assert_eq!(columns(&out, "<ul"), vec![2]);
    assert_eq!(columns(&out, "</ul>"), vec![2]);
    // The control for the pad coming off ONCE: without a title the list leads,
    // and it must still be at 2 rather than at 0.
    let bare = index_html("> A :index[parser] here.\n>\n> ::: index\n> :::\n");
    assert_eq!(columns(&bare, "<ul"), vec![2]);
    assert_eq!(columns(&bare, "</ul>"), vec![2]);
}

#[test]
fn a_placed_toc_keeps_its_whole_fragment_at_column_zero() {
    // Extensions section 8b.3 makes this fragment the cross-implementation
    // contract byte for byte, INCLUDING its leading whitespace, so the clause's
    // other choice is the one it takes wherever the marker was written.
    for source in [
        "> ::: toc \"C\"\n> :::\n\n# One\n",
        "- a\n  ::: toc\n  :::\n\n# One\n",
        "::: note\n::: toc\n:::\n:::\n\n# One\n",
    ] {
        let out = toc_html(source);
        assert_eq!(columns(&out, "<nav"), vec![0], "{source:?}\n{out}");
        assert_eq!(columns(&out, "</nav>"), vec![0], "{source:?}\n{out}");
    }
    // Written flush left the answer is the same, which is why no existing case
    // could see the divergence.
    let flat = toc_html("::: toc\n:::\n\n# One\n");
    assert_eq!(columns(&flat, "<nav"), vec![0]);
    assert_eq!(columns(&flat, "</nav>"), vec![0]);
}

#[test]
fn a_block_authored_inside_a_toc_marker_keeps_the_ambient_column() {
    // Only the bare nav is anchored at 0. An authored block comes first and is
    // ordinary content of the container holding it.
    let out = toc_html("> ::: toc\n> lead\n> :::\n\n# One\n");
    assert_eq!(columns(&out, "<p>lead</p>"), vec![2], "{out}");
    assert_eq!(columns(&out, "<nav"), vec![0], "{out}");
    assert_eq!(columns(&out, "</nav>"), vec![0], "{out}");
}
