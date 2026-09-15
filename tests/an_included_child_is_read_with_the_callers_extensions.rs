//! An include is textual composition of one document, so the same text has to
//! mean the same thing whichever file it sits in (markup-carve/carve-js#1693).

use std::collections::HashMap;

use carve::{
    expand_includes, parse_with_options, prepare_doc_with_includes, render_html_with_options,
    BlockNode, CarveExtension, Citations, Document, IncludeContext, IncludeDenial, IncludeOptions,
    IncludeResolved, IncludeResolver, Mode, Options, SmartQuotes, Wikilinks,
};

struct MapResolver(HashMap<&'static str, &'static str>);

impl MapResolver {
    fn new(files: &[(&'static str, &'static str)]) -> Self {
        Self(files.iter().copied().collect())
    }
}

impl IncludeResolver for MapResolver {
    fn resolve(
        &self,
        path: &str,
        _ctx: &IncludeContext<'_>,
    ) -> Result<IncludeResolved, IncludeDenial> {
        self.0
            .get(path)
            .map(|source| IncludeResolved::with_id(*source, path))
            .ok_or(IncludeDenial::NotFound)
    }
}

fn expanded(
    parent: &str,
    files: &[(&'static str, &'static str)],
    extensions: &[&dyn CarveExtension],
    forward: bool,
) -> Document {
    let resolver = MapResolver::new(files);
    let mut parse_options = Options::new();
    let mut include_options = IncludeOptions::new().with_resolver(&resolver);
    for extension in extensions {
        parse_options = parse_options.with_extension(*extension);
        if forward {
            include_options = include_options.with_extension(*extension);
        }
    }
    expand_includes(
        parse_with_options(parent, &parse_options),
        parent,
        &include_options,
    )
    .doc
}

fn html(doc: &Document, extensions: &[&dyn CarveExtension]) -> String {
    let mut options = Options::new();
    for extension in extensions {
        options = options.with_extension(*extension);
    }
    render_html_with_options(doc, &options).expect("renders")
}

#[test]
fn an_inline_matcher_the_caller_passed_reaches_the_child() {
    let wikilinks = Wikilinks::new();
    let doc = expanded(
        "Root [[P]].\n\n{{ c.crv }}\n",
        &[("c.crv", "Child [[P]].\n")],
        &[&wikilinks],
        true,
    );

    assert!(html(&doc, &[&wikilinks])
        .contains(r#"<p>Child <a href="p" class="wikilink" data-wikilink="P">P</a>.</p>"#));
}

#[test]
fn the_expanded_document_renders_what_the_same_lines_render_as_one_file() {
    let wikilinks = Wikilinks::new();
    let doc = expanded(
        "Root [[P]].\n\n{{ c.crv }}\n",
        &[("c.crv", "Child [[P]].\n")],
        &[&wikilinks],
        true,
    );
    let one_file = parse_with_options(
        "Root [[P]].\n\nChild [[P]].\n",
        &Options::new().with_extension(&wikilinks),
    );

    assert_eq!(html(&doc, &[&wikilinks]), html(&one_file, &[&wikilinks]));
}

#[test]
fn the_extensions_reach_a_grandchild() {
    let wikilinks = Wikilinks::new();
    let doc = expanded(
        "Root\n\n{{ a.crv }}\n",
        &[("a.crv", "{{ b.crv }}\n"), ("b.crv", "Deep [[P]].\n")],
        &[&wikilinks],
        true,
    );

    assert!(html(&doc, &[&wikilinks]).contains(r#"data-wikilink="P""#));
}

#[test]
fn the_child_is_left_without_extensions_when_the_caller_passes_none() {
    let wikilinks = Wikilinks::new();
    let doc = expanded(
        "Root [[P]].\n\n{{ c.crv }}\n",
        &[("c.crv", "Child [[P]].\n")],
        &[&wikilinks],
        false,
    );

    assert!(html(&doc, &[&wikilinks]).contains("<p>Child [[P]].</p>"));
}

#[test]
fn a_citation_definition_the_extension_made_visible_is_promoted() {
    let citations = Citations::new();
    let doc = expanded(
        "Root cites [@k].\n\n{{ c.crv }}\n",
        &[("c.crv", "[@k]: Knuth, D. TAOCP.\n")],
        &[&citations],
        true,
    );

    let definitions = doc
        .children
        .iter()
        .filter(|block| matches!(block, BlockNode::CitationDefinition(_)))
        .count();
    assert_eq!(definitions, 1, "tree: {:?}", doc.children);
}

#[test]
fn a_reference_image_with_a_caption_is_a_figure_in_a_child() {
    let doc = expanded(
        "Root\n\n{{ c.crv }}\n",
        &[("c.crv", "![alt][ref]\n^ Cap\n\n[ref]: a.png\n")],
        &[],
        true,
    );

    assert!(html(&doc, &[]).contains("<figcaption>Cap</figcaption>"));
}

/// The pipeline entry point parses the parent itself, so it forwards the set
/// without the caller naming it twice. `--quote-locale` reaches the child this
/// way, and its hook runs after parse rather than as a matcher.
#[test]
fn prepare_doc_with_includes_reads_the_child_with_the_parse_options_extensions() {
    let quotes = SmartQuotes::new("de");
    let resolver = MapResolver::new(&[("c.crv", "\"Child\" quote.\n")]);
    let source = "\"Parent\" quote.\n\n{{ c.crv }}\n";
    let options = Options::new().with_extension(&quotes);
    let prepared = prepare_doc_with_includes(
        source,
        &options,
        &IncludeOptions::new().with_resolver(&resolver),
        Mode::Interactive,
        true,
    )
    .expect("no profile");

    assert_eq!(
        html(&prepared.doc, &[&quotes]),
        "<p>„Parent“ quote.</p>\n<p>„Child“ quote.</p>"
    );
}
