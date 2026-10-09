//! A block-attribute line before a raw block attaches to it, as it does to a
//! code block (`raw_block.attrs` in the PART 12 schema).

use std::collections::HashMap;

use carve::{
    expand_includes, from_json, from_prosemirror, parse, render_carve, to_json, to_prosemirror,
    BlockNode, IncludeDenial, IncludeOptions, IncludeResolved, IncludeResolver, Options,
};

const SOURCE: &str = "{#r .x}\n```=html\n<p>raw</p>\n```\n";

fn html(source: &str, safe: bool) -> String {
    carve::to_html_with_options(source, &Options::default().with_raw_html(!safe))
}

#[test]
fn the_parser_attaches_the_line() {
    let doc = parse(SOURCE);
    let BlockNode::RawBlock(raw) = &doc.children[0] else {
        panic!("expected a raw block, got {:?}", doc.children[0]);
    };
    let attrs = raw.attrs.as_ref().expect("the attribute line attaches");
    assert_eq!(attrs.id.as_deref(), Some("r"));
    assert_eq!(attrs.classes, vec!["x".to_string()]);
}

#[test]
fn ast_json_carries_and_reads_back_the_attrs() {
    let json = to_json(&parse(SOURCE));
    assert_eq!(
        json,
        r##"{"type":"document","children":[{"type":"raw_block","format":"html","content":"<p>raw</p>","attrs":{"id":"r","classes":["x"],"order":["#id",".class"]}}],"srcByteLength":32}"##
    );
    let decoded = from_json(&json).expect("the encoded tree decodes");
    assert_eq!(to_json(&decoded), json);
}

#[test]
fn safe_html_places_them_on_the_pre_like_carve_js() {
    assert_eq!(
        html(SOURCE, true),
        "<pre id=\"r\" class=\"x\"><code class=\"language-html\">&lt;p&gt;raw&lt;/p&gt;\n</code></pre>"
    );
}

/// The spec gives a passed-through raw block no attribute rule, so the
/// payload reaches the output exactly as written.
#[test]
fn raw_allowed_html_passes_the_payload_through_unchanged() {
    assert_eq!(html(SOURCE, false), "<p>raw</p>");
}

#[test]
fn the_carve_writer_writes_the_line_back() {
    assert_eq!(render_carve(&parse(SOURCE)).unwrap(), SOURCE);
}

#[test]
fn the_prosemirror_bridge_keeps_them() {
    let bridged = to_prosemirror(&parse(SOURCE));
    let returned = from_prosemirror(&bridged.json).expect("the bridge reads its own output");
    assert_eq!(render_carve(&returned).unwrap(), SOURCE);
}

#[test]
fn the_id_is_reserved_against_a_derived_heading_id() {
    let source = "{#Intro}\n```=html\nx\n```\n\n# Intro\n";
    for safe in [true, false] {
        assert!(
            html(source, safe).contains("<section id=\"Intro-2\">"),
            "safe={safe}"
        );
    }
}

struct Files(HashMap<String, String>);

impl IncludeResolver for Files {
    fn resolve(
        &self,
        path: &str,
        _ctx: &carve::IncludeContext<'_>,
    ) -> Result<IncludeResolved, IncludeDenial> {
        self.0
            .get(path)
            .map(|source| IncludeResolved::with_id(source.clone(), path))
            .ok_or(IncludeDenial::NotFound)
    }
}

#[test]
fn an_include_selector_finds_the_raw_block_by_id() {
    let part = format!("before\n\n{SOURCE}\nafter\n");
    let resolver = Files(HashMap::from([("part.crv".to_string(), part)]));
    let source = "{{ part.crv#r }}\n";
    let expanded = expand_includes(
        parse(source),
        source,
        &IncludeOptions::new().with_resolver(&resolver),
    );
    assert_eq!(render_carve(&expanded.doc).unwrap(), SOURCE);
}

#[test]
fn a_source_line_is_stamped_like_a_code_blocks() {
    let options = Options::default()
        .with_raw_html(false)
        .with_source_lines(true);
    let raw = carve::to_html_with_options("a\n\n```=html\n<b>x</b>\n```\n", &options);
    let code = carve::to_html_with_options("a\n\n```html\n<b>x</b>\n```\n", &options);
    assert!(raw.contains("<pre data-source-line=\"3\">"), "{raw}");
    assert_eq!(raw, code);
}
