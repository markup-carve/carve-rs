use carve::{
    from_json, from_prosemirror, from_prosemirror_with_report, parse, parse_with_options,
    render_carve, render_html, to_json, to_prosemirror, BlockNode, Options,
};
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

mod common;

const SCHEMA_MAP: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/resources/prosemirror-schema-map.json"
));

// Each entry is (ProseMirror name, why no checked-in corpus source produces it).
const NOT_IN_CORPUS: &[(&str, &str)] = &[
    (
        "carveCitation",
        "the corpus contains no parsed citation group",
    ),
    (
        "carveSection",
        "sections are rendering wrappers, not parsed source nodes",
    ),
    (
        "carveTab",
        "the corpus is bridged without applying the tabs extension",
    ),
    (
        "carveTabSet",
        "the corpus is bridged without applying the tabs extension",
    ),
    // The bridge builds all three. What no checked-in document does is REACH
    // them: Carve 0.1 source spells no block extension, ruby annotation or
    // small-caps wrapper, so one arrives over the AST-JSON wire only.
    //
    // Measured against the parsed AST rather than against this bridge's own
    // report, because the report is what was wrong the last time the same claim
    // was made: a silent substitution hid `::: footnotes` from a scan of the
    // reported causes (carve-rs#1879). Serializing each corpus document with
    // `to_json` and looking for the type finds `directive` in
    // 122-footnotes-placement.crv and none of these three anywhere.
    (
        "carveBlockExtension",
        "no corpus document parses to a block extension; Carve 0.1 source spells none",
    ),
    (
        "carveRuby",
        "no corpus document parses to a ruby annotation; Carve 0.1 source spells none",
    ),
    (
        "carveSmallCaps",
        "no corpus document parses to a small-caps wrapper; Carve 0.1 source spells none",
    ),
];

// Documents whose CANONICAL CARVE SOURCE does not survive the bridge round
// trip, grouped by cause. Every one of them renders byte-identical HTML.
//
// A declared ratchet, asserted as an exact set: a document that starts
// differing fails, and one that stops differing fails too, so a fix deletes its
// entry rather than letting the gate quietly cover less.
//
// BOTH SIDES GO THROUGH `to_carve`, which makes position-dependent source
// changes visible. `parse` leaves positions off, so this gate parses the bridge
// input with positions on as well. The bridge carries those positions through
// `carvePos`; without them the writer cannot put collected definitions back on
// emptied marker lines or preserve their relative order.
// Comparing against the SOURCE instead would report 749 documents whose `.crv`
// is simply not in canonical form; `tests/corpus_canonical_form.rs` is where
// that belongs.
const SOURCE_LOSSY: &[(&str, &[&str])] = &[
    (
        "the sanitized-away `href` attribute is not carried, so \
         `[safe](https://example.com){href=javascript:steal}` comes back without it",
        &["108-security-hardening-11.crv"],
    ),
    (
        // `-3` left this set when the comparison started applying PART 11
        // §10k's normalization list: its two spellings COMMUTE, so the trip
        // returning the other one is not a loss. `-4` stays, because there the
        // delimiters reflow around content the list says nothing about.
        "the nested emphasis delimiters reflow: `/*x*/` comes back `*/x/*`",
        &["130-bold-italic-delimiter-needs-content-4.crv"],
    ),
    (
        "the combined token is not carried, and with asterisks as its content \
         `*/*/*` would misparse, so the split marks come back braced: `/***/` \
         comes back `*{/*/}*`",
        &[
            "473-a-run-of-asterisks-inside-a-combined-token-is-content-2.crv",
            "473-a-run-of-asterisks-inside-a-combined-token-is-content.crv",
        ],
    ),
    (
        "the combined token is not carried, so punctuation can reverse or \
         split its reconstructed delimiter pair",
        &[
            "486-any-character-is-content-of-the-combined-bold-italic-token-2.crv",
            "490-a-comment-inside-a-forced-span-or-the-combined-token-ends-at-its-closer-3.crv",
        ],
    ),
];

/// Every declared source-lossy document, sorted.
fn source_lossy_names() -> Vec<String> {
    let mut names: Vec<String> = SOURCE_LOSSY
        .iter()
        .flat_map(|(_, names)| names.iter().map(|name| (*name).to_string()))
        .collect();
    names.sort();
    names
}

fn pm(source: &str) -> (Value, carve::ProseMirrorDoc) {
    let result = to_prosemirror(&parse(source));
    let value = serde_json::from_str(&result.json).expect("bridge emits JSON");
    (value, result)
}

#[test]
fn a_degraded_composite_figure_round_trips_through_its_own_bridge() {
    let source = r#"{#fig-x .columns-2}
::: figure
{#fig-x-a}
![one](a.png)
^ (a) One

{#fig-x-b}
![two](b.png)
^ (b) Two
:::
^ Figure #: Group caption
"#;
    let (value, bridge) = pm(source);

    assert!(bridge.degraded.contains_key("figure_group"));
    let children = value["content"][0]["content"]
        .as_array()
        .expect("div children");
    let caption = children.last().expect("trailing caption paragraph");
    assert_eq!(caption["type"], "paragraph");
    let caption_text: String = caption["content"]
        .as_array()
        .expect("caption inline content")
        .iter()
        .filter_map(|node| node["text"].as_str())
        .collect();
    assert_eq!(caption_text, "Figure : Group caption");
    assert!(bridge.dropped.contains_key("caption_number"));
    from_prosemirror(&bridge.json).expect("bridge accepts its degraded figure group");
}

#[test]
fn definitions_carry_their_source_positions_both_ways() {
    let source = "[^z]: note\n\n[z]: /z\n";
    let original = parse_with_options(source, &Options::default().with_positions(true));
    let bridged = to_prosemirror(&original);
    let value: Value = serde_json::from_str(&bridged.json).expect("bridge emits JSON");

    let footnote = value["content"]
        .as_array()
        .unwrap()
        .iter()
        .find(|node| node["type"] == "carveFootnoteDefinition")
        .unwrap();
    let link = value["content"]
        .as_array()
        .unwrap()
        .iter()
        .find(|node| node["type"] == "carveLinkRefDef")
        .unwrap();
    assert_eq!(footnote["attrs"]["carvePos"]["startOffset"], 6);
    assert_eq!(link["attrs"]["carvePos"]["startOffset"], 12);

    let returned = from_prosemirror(&bridged.json).expect("positioned payload imports");
    assert_eq!(render_carve(&returned).unwrap(), source);
}

#[test]
fn malformed_editor_positions_are_ignored() {
    for carve_pos in [
        json!({}),
        json!("not a position"),
        json!({
            "startLine": -1, "endLine": 1, "startColumn": 1,
            "endColumn": 2, "startOffset": 0, "endOffset": 1
        }),
    ] {
        let payload = json!({"type":"doc","content":[{
            "type":"paragraph","attrs":{"carvePos":carve_pos},
            "content":[{"type":"text","text":"ok"}]
        }]});
        let doc = from_prosemirror(&payload.to_string()).expect("bad metadata is not fatal");
        assert!(matches!(&doc.children[0], BlockNode::Paragraph(node) if node.pos.is_none()));
    }
}

#[test]
fn raw_blocks_and_comments_keep_positions() {
    let source = "```=html\n<b>x</b>\n```\n\n%% note\n";
    let original = parse_with_options(source, &Options::default().with_positions(true));
    let returned = from_prosemirror(&to_prosemirror(&original).json).unwrap();
    assert!(matches!(&returned.children[0], BlockNode::RawBlock(node) if node.pos.is_some()));
    assert!(matches!(&returned.children[1], BlockNode::Comment(node) if node.pos.is_some()));
}

#[test]
fn a_footnote_body_position_wins_over_its_definition_position() {
    let ast = r#"{"type":"document","children":[{"type":"footnote","label":"f","children":[{"type":"paragraph","children":[{"type":"text","value":"note"}],"pos":{"startLine":2,"endLine":2,"startColumn":3,"endColumn":7,"startOffset":5,"endOffset":9}}],"pos":{"startLine":2,"endLine":2,"startColumn":1,"endColumn":7,"startOffset":3,"endOffset":9}}],"srcByteLength":9}"#;
    let doc = from_json(ast).expect("the AST payload is valid");
    let value: Value = serde_json::from_str(&to_prosemirror(&doc).json).unwrap();
    assert_eq!(value["content"][0]["attrs"]["carvePos"]["startOffset"], 5);
}

#[test]
fn structural_block_positions_survive_the_bridge() {
    let source = "- item\n\n:: term\n: definition\n";
    let original = parse_with_options(source, &Options::default().with_positions(true));
    let bridged = to_prosemirror(&original);
    let value: Value = serde_json::from_str(&bridged.json).unwrap();
    assert!(value
        .pointer("/content/0/content/0/attrs/carvePos")
        .is_some());
    assert!(value
        .pointer("/content/1/content/0/attrs/carvePos")
        .is_some());
    assert!(value
        .pointer("/content/1/content/1/attrs/carvePos")
        .is_some());
    let returned = from_prosemirror(&bridged.json).unwrap();
    assert_eq!(render_carve(&returned).unwrap(), source);
}

#[test]
fn an_empty_footnote_carries_its_definition_position() {
    let source = "[^f]: {empty}\n";
    let original = parse_with_options(source, &Options::default().with_positions(true));
    let bridged = to_prosemirror(&original);
    let value: Value = serde_json::from_str(&bridged.json).unwrap();
    assert_eq!(value["content"][0]["attrs"]["carvePos"]["startOffset"], 0);
    let returned = from_prosemirror(&bridged.json).unwrap();
    assert_eq!(render_carve(&returned).unwrap(), source);
}

#[test]
fn nested_inline_elements_become_marks_on_text() {
    let (value, report) = pm("A *bold /and italic/* word.");
    assert_eq!(
        value,
        json!({"type":"doc","content":[{"type":"paragraph","content":[
            {"type":"text","text":"A "},
            {"type":"text","text":"bold ","marks":[{"type":"bold"}]},
            {"type":"text","text":"and italic","marks":[{"type":"bold"},{"type":"italic"}]},
            {"type":"text","text":" word."}
        ]}]})
    );
    assert!(report.dropped.is_empty());
}

#[test]
fn soft_break_survives_as_text_and_is_reported() {
    let (value, report) = pm("left\nright");
    assert_eq!(
        report.degraded.get("soft_break").map(String::as_str),
        Some("a soft break is whitespace in the ProseMirror model")
    );
    assert_eq!(
        value.pointer("/content/0/content/1/text"),
        Some(&json!(" "))
    );
}

#[test]
fn code_block_carries_language_and_literal_text() {
    let (value, report) = pm("``` rust\nlet x = 1;\n```");
    assert_eq!(value.pointer("/content/0/type"), Some(&json!("codeBlock")));
    assert_eq!(
        value.pointer("/content/0/attrs/language"),
        Some(&json!("rust"))
    );
    assert_eq!(
        value.pointer("/content/0/content/0/text"),
        Some(&json!("let x = 1;"))
    );
    assert!(report.dropped.is_empty());
}

#[test]
fn list_looseness_is_an_attribute() {
    let (tight, _) = pm("- one\n- two");
    assert_eq!(tight.pointer("/content/0/attrs/tight"), Some(&json!(true)));
    let (loose, _) = pm("- one\n\n- two");
    assert_eq!(loose.pointer("/content/0/attrs/tight"), Some(&json!(false)));
}

fn collect_bridge_types(json: &str, found: &mut BTreeSet<String>) {
    let mut rest = json;
    const PREFIX: &str = "\"type\":\"";
    while let Some(start) = rest.find(PREFIX) {
        rest = &rest[start + PREFIX.len()..];
        let end = rest.find('"').expect("type name is terminated");
        found.insert(rest[..end].to_owned());
        rest = &rest[end + 1..];
    }
}

fn mapped_names(map: &Value, carve_type: &str) -> Vec<String> {
    match &map["types"][carve_type]["pm"] {
        Value::String(name) => vec![name.clone()],
        Value::Array(names) => names
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect(),
        _ => Vec::new(),
    }
}

#[test]
fn every_mapped_type_is_reachable() {
    let map: Value = serde_json::from_str(SCHEMA_MAP).expect("schema map is JSON");
    let mut found = BTreeSet::new();
    let corpus = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/spec/tests/corpus");
    for entry in fs::read_dir(corpus).expect("corpus directory exists") {
        let path = entry.expect("corpus entry is readable").path();
        if path.extension().and_then(|extension| extension.to_str()) != Some("crv") {
            continue;
        }
        let source = fs::read_to_string(&path).expect("corpus source is readable");
        let report = to_prosemirror(&parse(&source));
        collect_bridge_types(&report.json, &mut found);
        for carve_type in report.dropped.keys().chain(report.degraded.keys()) {
            found.extend(mapped_names(&map, carve_type));
        }
    }

    let all: BTreeSet<String> = map["types"]
        .as_object()
        .expect("types is an object")
        .keys()
        .flat_map(|carve_type| mapped_names(&map, carve_type))
        .collect();
    let exempt: BTreeSet<String> = NOT_IN_CORPUS
        .iter()
        .map(|(name, reason)| {
            assert!(!reason.is_empty(), "{name} needs an exemption reason");
            (*name).to_owned()
        })
        .collect();
    let stale: Vec<_> = exempt.intersection(&found).cloned().collect();
    assert!(
        stale.is_empty(),
        "NOT_IN_CORPUS entries now reached: {stale:?}"
    );
    let missing: Vec<_> = all.difference(&found).cloned().collect();
    assert_eq!(missing, exempt.into_iter().collect::<Vec<_>>());
}

#[test]
fn prose_mirror_names_only_come_from_the_map() {
    let map: Value = serde_json::from_str(SCHEMA_MAP).expect("schema map is JSON");
    let mut source = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/prosemirror/to_pm.rs"
    ))
    .to_owned();
    let mut inbound = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/prosemirror/from_pm.rs"
    ))
    .to_owned();
    for carve_type in map["types"].as_object().expect("types is an object").keys() {
        source = source.replace(&format!("name(\"{carve_type}\")"), "name(LOOKUP)");
        source = source.replace(
            &format!("drop_type(\"{carve_type}\""),
            "drop_type(CARVE_TYPE",
        );
        // Same shape as `drop_type`: the argument is the CARVE type, which for
        // `link` happens to be spelled the way ProseMirror spells it too.
        source = source.replace(
            &format!("empty_mark(\"{carve_type}\""),
            "empty_mark(CARVE_TYPE",
        );
        // Same shape as `drop_type`: the argument is the CARVE type, which for
        // `link` happens to be spelled the way ProseMirror spells it too.
        source = source.replace(
            &format!("empty_mark(\"{carve_type}\""),
            "empty_mark(CARVE_TYPE",
        );
        source = source.replace("name(ty)", "name(LOOKUP)");
        // The inbound implementation necessarily names the CARVE side of the
        // mapping. Remove that vocabulary before asking whether a PM spelling
        // was restated; equal spellings (text, paragraph, link...) are not PM
        // literals merely because the two schemas agree on them.
        inbound = inbound.replace(&format!("\"{carve_type}\""), "CARVE_TYPE");
    }
    source.push_str(&inbound);
    source = source.replace("o.insert(\"text\".into()", "o.insert(TEXT_ATTRIBUTE.into()");
    let leaked: Vec<_> = map["types"]
        .as_object()
        .expect("types is an object")
        .keys()
        .flat_map(|carve_type| mapped_names(&map, carve_type))
        .filter(|name| source.contains(&format!("\"{name}\"")))
        .collect();
    assert!(leaked.is_empty(), "hardcoded ProseMirror names: {leaked:?}");
}

#[test]
fn unknown_inbound_name_is_an_error() {
    let error = from_prosemirror(r#"{"type":"doc","content":[{"type":"somethingNobodyMapped"}]}"#)
        .expect_err("unknown nodes must not be skipped");
    assert!(error.to_string().contains("somethingNobodyMapped"));
}

#[test]
fn stock_mention_is_accepted_but_never_emitted() {
    let doc = from_prosemirror(r#"{"type":"doc","content":[{"type":"paragraph","content":[{"type":"mention","attrs":{"id":"alice"}}]}]}"#)
        .expect("the map's accepts spelling is inbound");
    let output = to_prosemirror(&doc);
    assert!(!output.json.contains(r#""type":"mention""#));
}

#[test]
fn inbound_mention_flavor_comes_from_the_arriving_name() {
    let map: Value = serde_json::from_str(SCHEMA_MAP).expect("schema map is JSON");
    let mention = mapped_names(&map, "mention");
    let tag = mapped_names(&map, "tag");
    let input = json!({"type":"doc","content":[{"type":"paragraph","content":[
        {"type":mention[0],"attrs":{"id":"alice"}}, {"type":"text","text":" "},
        {"type":tag[0],"attrs":{"id":"topic"}}
    ]}]});
    let doc = from_prosemirror(&input.to_string()).expect("mapped mention flavors import");
    let html = render_html(&doc).unwrap();
    assert!(html.contains("class=\"mention\""));
    assert!(html.contains("class=\"tag\""));
    assert!(html.contains("@alice"));
    assert!(html.contains("#topic"));
}

/// A mention node after `ping `, read back and written as Carve, with the
/// import report.
fn written_mention(ty: &str, attrs: Value) -> (String, Value, Value) {
    let input = json!({"type":"doc","content":[{"type":"paragraph","content":[
        {"type":"text","text":"ping "}, {"type":ty,"attrs":attrs}
    ]}]});
    let import = from_prosemirror_with_report(&input.to_string()).expect("the mention imports");
    let carve = render_carve(&import.document).expect("the mention writes");
    (carve, json!(import.dropped), json!(import.degraded))
}

const DIFFERENT_LABEL: &str =
    "the mention name is its id, so a different display label is not carried";

#[test]
fn a_stock_tiptap_mention_is_named_by_its_id_then_its_label() {
    let tag = mapped_names(&serde_json::from_str(SCHEMA_MAP).unwrap(), "tag")[0].clone();
    let cases = [
        (
            "mention",
            json!({"id":"alice","label":null,"mentionSuggestionChar":"@"}),
            "ping @alice\n",
            json!({}),
        ),
        (
            "mention",
            json!({"id":"alice","label":"alice","mentionSuggestionChar":"@"}),
            "ping @alice\n",
            json!({}),
        ),
        (
            "mention",
            json!({"id":"u123","label":"Alice","mentionSuggestionChar":"@"}),
            "ping @u123\n",
            json!({"label": DIFFERENT_LABEL}),
        ),
        (
            "mention",
            json!({"id":null,"label":"Alice","mentionSuggestionChar":"@"}),
            "ping @Alice\n",
            json!({}),
        ),
        ("mention", json!({"id":"alice"}), "ping @alice\n", json!({})),
        (
            "mention",
            json!({"label":"Alice"}),
            "ping @Alice\n",
            json!({}),
        ),
        (
            tag.as_str(),
            json!({"id":"release","label":null,"mentionSuggestionChar":"#"}),
            "ping #release\n",
            json!({}),
        ),
        (
            tag.as_str(),
            json!({"id":"release","label":"Release","mentionSuggestionChar":"#"}),
            "ping #release\n",
            json!({"label": DIFFERENT_LABEL}),
        ),
        (
            "mention",
            json!({"id":"alice","label":["Alice"]}),
            "ping @alice\n",
            json!({"label": "a Carve attribute holds a string, and this value is of type array"}),
        ),
        (
            "mention",
            json!({"id":"@alice","label":null}),
            "ping @alice\n",
            json!({}),
        ),
    ];
    for (ty, attrs, expected, degraded) in cases {
        let (carve, dropped, got_degraded) = written_mention(ty, attrs.clone());
        assert_eq!(carve, expected, "{ty} {attrs}");
        assert_eq!(dropped, json!({}), "{ty} {attrs}");
        assert_eq!(got_degraded, degraded, "{ty} {attrs}");
        let html = render_html(&parse(&carve)).unwrap();
        assert!(
            html.contains("<strong>"),
            "{ty} {attrs} reads back as a mention: {html}"
        );
    }
}

#[test]
fn a_mention_name_with_no_carve_spelling_is_written_as_text() {
    let tag = mapped_names(&serde_json::from_str(SCHEMA_MAP).unwrap(), "tag")[0].clone();
    let as_text = "the name has no Carve mention spelling, so it is written as literal text";
    let tag_as_text = "the name has no Carve tag spelling, so it is written as literal text";
    let cases = [
        (
            "mention",
            json!({"id":"Lea Thompson","label":null,"mentionSuggestionChar":"@"}),
            "ping \\@Lea Thompson\n",
            json!({"id": as_text}),
            "<p>ping @Lea Thompson</p>",
        ),
        (
            tag.as_str(),
            json!({"id":"two words","label":null}),
            "ping \\#two words\n",
            json!({"id": tag_as_text}),
            "<p>ping #two words</p>",
        ),
        // The name lives in `label`, so the report is keyed by `label`.
        (
            "mention",
            json!({"id":null,"label":"Lea Thompson"}),
            "ping \\@Lea Thompson\n",
            json!({"label": as_text}),
            "<p>ping @Lea Thompson</p>",
        ),
        // The text is what the editor showed, which is the label.
        (
            "mention",
            json!({"id":"u 1","label":"Lea"}),
            "ping \\@Lea\n",
            json!({"id": as_text}),
            "<p>ping @Lea</p>",
        ),
        // A name of nothing but a sigil is not a name, and not doubled either.
        (
            "mention",
            json!({"id":"@","label":null}),
            "ping @\n",
            json!({"id": as_text}),
            "<p>ping @</p>",
        ),
    ];
    for (ty, attrs, expected, degraded, html) in cases {
        let (carve, dropped, got_degraded) = written_mention(ty, attrs.clone());
        assert_eq!(carve, expected, "{ty} {attrs}");
        assert_eq!(dropped, json!({}), "{ty} {attrs}");
        assert_eq!(got_degraded, degraded, "{ty} {attrs}");
        assert_eq!(
            render_html(&parse(&carve)).unwrap().trim(),
            html,
            "{ty} {attrs}"
        );
    }
}

/// A node carrying neither an id nor a label has no name, so the bridge leaves
/// it out and keys the report on the node kind, no field having held a name
/// (ruling markup-carve/carve-php#2176). It used to write a bare `@`, which is
/// a character the payload never carried.
#[test]
fn a_mention_with_no_name_at_all_is_dropped_and_reported() {
    let tag = mapped_names(&serde_json::from_str(SCHEMA_MAP).unwrap(), "tag")[0].clone();
    let no_name = "a mention with no name has nothing to write";
    let tag_no_name = "a tag with no name has nothing to write";
    let cases = [
        ("mention", json!({}), json!({"mention": no_name})),
        ("mention", json!({"id":null}), json!({"mention": no_name})),
        (
            "mention",
            json!({"label":null}),
            json!({"mention": no_name}),
        ),
        (
            "mention",
            json!({"id":null,"label":null}),
            json!({"mention": no_name}),
        ),
        (
            "mention",
            json!({"id":null,"label":null,"mentionSuggestionChar":"@"}),
            json!({"mention": no_name}),
        ),
        (tag.as_str(), json!({}), json!({"tag": tag_no_name})),
        (
            tag.as_str(),
            json!({"id":null}),
            json!({"tag": tag_no_name}),
        ),
        (
            tag.as_str(),
            json!({"label":null}),
            json!({"tag": tag_no_name}),
        ),
        (
            tag.as_str(),
            json!({"id":null,"label":null}),
            json!({"tag": tag_no_name}),
        ),
        (
            tag.as_str(),
            json!({"id":null,"label":null,"mentionSuggestionChar":"#"}),
            json!({"tag": tag_no_name}),
        ),
        // The attribute goes with the node, and is named beside it.
        (
            "mention",
            json!({"id":null,"label":null,"data-team":"core"}),
            json!({"mention": no_name, "data-team": "a mention has no Carve spelling for an attribute"}),
        ),
        (
            tag.as_str(),
            json!({"id":null,"label":null,"data-team":"core"}),
            json!({"tag": tag_no_name, "data-team": "a tag has no Carve spelling for an attribute"}),
        ),
    ];
    for (ty, attrs, dropped) in cases {
        let (carve, got_dropped, got_degraded) = written_mention(ty, attrs.clone());
        assert_eq!(carve, "ping\n", "{ty} {attrs}");
        assert_eq!(got_dropped, dropped, "{ty} {attrs}");
        assert_eq!(got_degraded, json!({}), "{ty} {attrs}");
    }
}

/// CONTROL: the bridge drops the node because it can report the loss; the
/// writer has no channel, so it refuses (ruling markup-carve/carve-php#2176).
#[test]
fn the_writer_still_refuses_a_nameless_mention_a_caller_builds() {
    let doc = from_json(
        "{\"type\":\"document\",\"srcByteLength\":0,\"children\":[{\"type\":\"paragraph\",\
         \"children\":[{\"type\":\"mention\",\"user\":\"\"}]}]}",
    )
    .expect("a nameless mention decodes");

    assert!(render_carve(&doc).is_err());
}

/// A real attribute on a mention is dropped and reported, and the mention is
/// still written: the bridge has a report channel, so only the writer refuses
/// such a tree (ruling markup-carve/carve-php#2167). It used to vanish with an
/// empty report, and then reach a refusal that lost the whole document.
#[test]
fn a_mention_attribute_is_dropped_and_reported() {
    let tag = mapped_names(&serde_json::from_str(SCHEMA_MAP).unwrap(), "tag")[0].clone();
    let no_spelling = "a mention has no Carve spelling for an attribute";
    let cases = [
        (
            "mention",
            json!({"id":"alice","label":null,"mentionSuggestionChar":"@","data-team":"core"}),
            "ping @alice\n",
            json!({"data-team": no_spelling}),
        ),
        (
            "mention",
            json!({"id":"alice","class":"x"}),
            "ping @alice\n",
            json!({"class": no_spelling}),
        ),
        (
            tag.as_str(),
            json!({"id":"release","label":null,"data-team":"core"}),
            "ping #release\n",
            json!({"data-team": "a tag has no Carve spelling for an attribute"}),
        ),
        (
            "mention",
            json!({"id":"u123","label":"Alice","data-team":"core"}),
            "ping @u123\n",
            json!({"data-team": no_spelling}),
        ),
    ];
    for (ty, attrs, expected, dropped) in cases {
        let (carve, got_dropped, _) = written_mention(ty, attrs.clone());
        assert_eq!(carve, expected, "{ty} {attrs}");
        assert_eq!(got_dropped, dropped, "{ty} {attrs}");
        let html = render_html(&parse(&carve)).unwrap();
        assert!(
            html.contains("<strong>"),
            "{ty} {attrs} reads back as a mention: {html}"
        );
    }
}

/// A mention written as text cannot hold an attribute, so the report names it.
#[test]
fn a_mention_written_as_text_reports_its_attribute() {
    let (carve, dropped, degraded) = written_mention(
        "mention",
        json!({"id":"Lea Thompson","label":null,"data-team":"core"}),
    );
    assert_eq!(carve, "ping \\@Lea Thompson\n");
    assert_eq!(
        dropped,
        json!({"data-team": "the mention is written as text, which holds no attribute"})
    );
    assert_eq!(
        degraded,
        json!({"id": "the name has no Carve mention spelling, so it is written as literal text"})
    );

    let (carve, dropped, degraded) = written_mention(
        mapped_names(&serde_json::from_str(SCHEMA_MAP).unwrap(), "tag")[0].as_str(),
        json!({"id":"big release","label":null,"data-team":"core"}),
    );
    assert_eq!(carve, "ping \\#big release\n");
    assert_eq!(
        dropped,
        json!({"data-team": "the tag is written as text, which holds no attribute"})
    );
    assert_eq!(
        degraded,
        json!({"id": "the name has no Carve tag spelling, so it is written as literal text"})
    );
}

#[test]
fn adjacent_equal_marks_merge_on_import() {
    let doc = from_prosemirror(
        r#"{"type":"doc","content":[{"type":"paragraph","content":[
        {"type":"text","text":"bold ","marks":[{"type":"bold"}]},
        {"type":"text","text":"and ","marks":[{"type":"bold"},{"type":"italic"}]},
        {"type":"text","text":"bold","marks":[{"type":"bold"}]}
    ]}]}"#,
    )
    .expect("marked text imports");
    let html = render_html(&doc).unwrap();
    assert_eq!(html.matches("<strong>").count(), 1);
    assert_eq!(html, "<p><strong>bold <em>and </em>bold</strong></p>");
}

/// The corpus round trip, compared as CANONICAL CARVE SOURCE.
///
/// It used to compare HTML. An HTML comparison cannot fail for the whole class
/// of defect this corpus exists to catch: anything that renders to nothing, or
/// renders the same from a different node, is invisible to it. A comment is the
/// clean example - `{% c %}` and `%% c` both render to nothing, so the bridge
/// could swap one spelling for the other, delete the rest of the author's line
/// on the next parse, and this gate stayed green. carve-php compares canonical
/// source for exactly this reason; this is the same comparison.
///
/// The HTML comparison is kept as well. It is not redundant: two documents can
/// write the same source and still render differently, because resolution
/// results (footnote and caption numbers) are not spelled in the source.
///
/// The canonical source comes from `to_carve` on both sides. The comment beside
/// that comparison explains what `render_carve(parse(x))` hid.
#[test]
fn fully_covered_corpus_documents_round_trip_through_prosemirror() {
    let corpus = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/spec/tests/corpus");
    let declared = source_lossy_names();
    let mut covered = 0usize;
    let mut lossy = 0usize;
    let mut source_lossy: Vec<String> = Vec::new();
    let mut nbsp_only_degraded = Vec::new();
    let mut undeclared: Vec<String> = Vec::new();
    for entry in fs::read_dir(corpus).expect("corpus directory exists") {
        let path = entry.expect("corpus entry is readable").path();
        if path.extension().and_then(|v| v.to_str()) != Some("crv") {
            continue;
        }
        let source = fs::read_to_string(&path).expect("corpus source is readable");
        let original = parse_with_options(&source, &Options::default().with_positions(true));
        let pm = to_prosemirror(&original);
        if pm.dropped.is_empty() && pm.degraded.is_empty() {
            let returned =
                from_prosemirror(&pm.json).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            assert_eq!(
                render_html(&returned).unwrap(),
                render_html(&original).unwrap(),
                "{}",
                path.display()
            );
            let name = path
                .file_name()
                .expect("a corpus entry has a file name")
                .to_string_lossy()
                .into_owned();
            // BOTH SIDES THROUGH `to_carve`, which is the entry point a Carve
            // target uses. `render_carve(parse(x))` is not: it leaves positions
            // off, so the ORIGINAL writes its definitions in label order at
            // document level too and the bridge's loss cancels against it.
            let before = normalized_for_comparison(&carve::to_carve(&source));
            let after = normalized_for_comparison(&carve::to_carve(
                &render_carve(&returned).expect("the returned document writes"),
            ));
            if before != after {
                source_lossy.push(name.clone());
                if !declared.contains(&name) {
                    undeclared.push(format!("{name}\n  before: {before:?}\n  after : {after:?}"));
                }
            }
            covered += 1;
        } else {
            if pm.dropped.is_empty()
                && pm.degraded.len() == 1
                && pm.degraded.contains_key("non_breaking_space")
            {
                nbsp_only_degraded.push(path.file_name().unwrap().to_string_lossy().into_owned());
            }
            lossy += 1;
        }
    }
    // Reported ahead of the set comparison, because a name alone does not say
    // what moved and a new divergence is the case that needs the bytes.
    assert!(
        undeclared.is_empty(),
        "the round trip changed the canonical source of {} undeclared document(s):\n{}",
        undeclared.len(),
        undeclared.join("\n"),
    );
    source_lossy.sort();
    assert_eq!(
        source_lossy, declared,
        "the declared source-lossy set moved - add or delete the entry, do not widen a count"
    );
    eprintln!("ProseMirror corpus: {covered} strict, {lossy} reported lossy");
    // A ratchet, not a floor of one. `covered > 0` passes with a single
    // document, so a change that quietly moved hundreds of documents out of
    // the strict set - by reporting a type as dropped rather than carrying it -
    // would not fail anything. Raise these when the numbers improve.
    //
    // 791/215 to 793/224 is the eleven composite-figure documents arriving with
    // the spec pin, and nothing else: the corpus went from 1006 pairs to 1017,
    // the eleven added pairs are all `318-composite-figures*`, and no existing
    // pair changed content. Two of them hold no node the editor schema lacks
    // and round-trip strictly; the other nine carry a `figure_group`, which
    // degrades to the generic container and is reported.
    // 793/224 to 799/224 is the six `319-cell-attributes-bind-after-the-kind-
    // and-alignment-markers` documents arriving with the spec pin, and nothing
    // else: the corpus went from 1017 pairs to 1023, the six added pairs are
    // all `319-*`, and no existing pair changed content. All six are tables the
    // editor schema covers whole, so all six land in the strict set and the
    // reported-lossy count does not move.
    // 799/224 to 801/224 is the two `320-the-canonical-writer-glues-a-code-
    // fence-to-its-info-string` documents arriving with the spec pin, and
    // nothing else: the corpus went from 1023 pairs to 1025, the two added
    // pairs are both `320-*`, and no existing pair changed content. Both are
    // code blocks the editor schema covers whole.
    // 801/224 to 827/226 is the twenty-eight documents arriving with the spec
    // pin, and nothing else: the corpus went from 1025 pairs to 1053, the
    // twenty-eight added pairs are the eight `321-delimited-comments`, the ten
    // `322-an-attribute-block-reaches-the-nested-list-it-precedes`, the five
    // `323-a-block-attached-after-an-invisible-line-leaves-the-item-tight` and
    // the five `324-an-abbreviation-definition-in-an-item-body-is-paragraph-
    // text`, and no existing pair changed content. Exactly two of them report:
    // `321-delimited-comments-7` degrades `escaped_text`, and
    // `324-an-abbreviation-definition-in-an-item-body-is-paragraph-text-5`
    // drops `abbreviation_def`. The other twenty-six land in the strict set,
    // and the declared source-lossy set above does not move.
    // 827/226 to 829/224 is the two documents whose only unbridgeable node was
    // a mark with no content, and nothing else: the corpus is the same 1053
    // pairs. `03-links-8` writes an empty-label link and
    // `307-an-empty-inline-note-is-literal-3` an empty span; both reported
    // themselves dropped, which kept them out of the strict set entirely, and
    // both now ride the carrier `markCarrierNodes` declares. Both write back
    // byte-identical source, so neither joins the set above.
    // 829/224 to 893/231 is the seventy-one documents arriving with the
    // b6917ab spec pin, and nothing else: the corpus went from 1053 pairs to
    // 1124, no existing pair was removed or changed content, and the 224
    // documents that reported at the old pin are the identical 224 that report
    // here - the count rose only by new documents, so nothing regressed out of
    // the strict set. Sixty-four of the seventy-one land in the strict set. The
    // seven that report all DEGRADE and none drops: `326-a-column-0-line-...`
    // 12 and 13, `327-a-continuation-marker-...` 3 and 4, and `330-a-tab-after-
    // a-fence-...-3` carry a `soft_break`, that `330-*` one also
    // `smart_punctuation`, and the two `333-a-continuation-row-s-open-run-and-
    // an-escaped-closing-pipe` carry `escaped_text`. All three types are
    // already declared unmapped, so no new cause appears here. Nineteen of the
    // sixty-four join the source-lossy set above, every one of them the
    // generated-heading-id cause the first bullet describes.
    // 893/231 to 900/231 is the seven documents arriving with the 8b80822 spec
    // pin, and nothing else: the corpus went from 1124 pairs to 1131, and the
    // seven added pairs are the `335-a-comment-fence-at-an-item-s-content-
    // column-registers-nothing-either` through `341-a-comment-fence-inside-a-
    // colon-container-registers-nothing` documents that pin a comment fence
    // hiding its body at every column. The raise is attributed rather than
    // fitted: every document was classified at BOTH pins and the two dumps
    // diffed, and no pre-existing document appears on either side of that diff.
    // So the 1124 documents carried over still contribute exactly 893 and 231,
    // document for document, and the whole delta is the seven new ones. All
    // seven hold no node the editor schema lacks, so all seven land in the
    // strict set, none reports, and none joins the source-lossy set above -
    // which is why LOSSY does not move and the declared set is untouched.
    // 900/231 to 910/231 is the ten documents arriving with the 483bcea spec
    // pin, and nothing else: the corpus went from 1131 pairs to 1141, and the
    // ten added pairs are the `342-url-list-attributes-are-probed-token-wise`
    // documents that pin PART 9 §25's token-wise probe. Attributed rather than
    // fitted, and here the attribution is structural instead of a diff of two
    // classification dumps: the clause this pin carries lives entirely in
    // `sanitize_attr_value`, which no path this test walks ever calls - the
    // bridge maps nodes and the canonical writer writes source, and neither
    // hardens an attribute value. So no pre-existing document CAN move, and the
    // whole delta is the ten new ones. All ten are a link or an image with an
    // attribute run, which the editor schema covers whole, so all ten land in
    // the strict set and none reports. One of them joins the declared
    // source-lossy set above: document 10 is the only corpus link that spells
    // `title` as an authored attribute, and it comes back in the structural
    // title slot.
    // 910/231 to 912/231 is the two documents arriving with the 5951e6d spec
    // pin, and nothing else: the corpus went from 1141 pairs to 1143, and the
    // two added pairs are `342-url-list-attributes-are-probed-token-wise` 11
    // and 12, which pin that PART 9 §25's token pass runs IN ADDITION TO the
    // value-wide probe rather than instead of it. Same structural attribution
    // as the bullet above - the clause lives in `sanitize_attr_value`, which no
    // path this test walks calls - so no pre-existing document can move. Both
    // are a link and an image with an attribute run, which the editor schema
    // covers whole, so both land in the strict set, neither reports, and
    // neither joins the declared source-lossy set.
    // 912/231 to 912/233 is the two documents arriving with the 5866bd0 spec
    // pin, and nothing else: the corpus went from 1143 pairs to 1145, and the
    // two added pairs are the `343-an-escaped-hash-keeps-its-escape-at-a-
    // container-s-content-position` documents that pin PART 11 §8b's content
    // position. Attributed by DIFFING THE REPORTING SETS at both pins under one
    // build: the set grows by exactly these two names and no pre-existing
    // document appears on either side, so the strict count does not move at all.
    // Both report for a reason already declared above - `escaped_text` degrades,
    // the same node `321-delimited-comments-7` reports - and one also degrades a
    // `soft_break`. Neither can join the strict set while an authored escape is
    // what the document exists to pin, so this raise is LOSSY only.
    // 912/233 to 923/233 is the eleven documents arriving with the 9015c3b spec
    // pin, and nothing else: the corpus went from 1145 pairs to 1156, and the
    // eleven added pairs are the four `344-a-comment-only-line-in-a-line-block-
    // is-removed-before-any-inline-run`, the three `345-a-line-block-s-hard-
    // break-keeps-its-backslash` and the four `346-a-line-block-s-last-body-
    // line-keeps-its-backslash` documents that pin PART 9 section 23's
    // block-layer removal and PART 11 section 7c's writer property. Attributed
    // by DIFFING THE CLASSIFICATION at both pins under ONE build: the diff is
    // exactly these eleven names on the new side and empty on the old, so no
    // pre-existing document moved in either direction. All eleven are a line
    // block of paragraphs, breaks, text and comments - every one of them a node
    // the editor schema covers - so all eleven land in the strict set, none
    // reports, and none writes back a different source, which is why LOSSY does
    // not move and the declared source-lossy set is untouched.
    // 923/233 to 933/242 is the nineteen documents arriving with the 7666027
    // spec pin, and nothing else: the corpus went from 1156 pairs to 1175, and
    // the added pairs are sixteen more `326-a-column-0-line-after-a-container-
    // s-last-block-when-that-block-left-no-paragraph-open` (14 through 29),
    // which pin that no open paragraph means no lazy line at every depth and
    // after an interrupter, plus the three `347-a-comment-fence-reached-
    // through-a-quote-registers-nothing-either` documents this branch exists
    // for. Attributed by DIFFING THE CLASSIFICATION at both pins under ONE
    // build: the diff is exactly these nineteen names on the new side and empty
    // on the old, and no pre-existing document changed class in either
    // direction.
    //
    // Ten land in the strict set and nine report. All nine report for one
    // reason already declared above - `soft_break` degrades, the same node
    // `343-an-escaped-hash-...` reports - because each is a container whose last
    // block left no paragraph open, so the column-0 line below it folds in and
    // the boundary survives as a soft break. None of the nine can join the
    // strict set while that fold is what the document exists to pin.
    //
    // Three of the ten strict ones also join the declared source-lossy set
    // above: `326-...-14`, `-16` and `-17` are the only ones of the nineteen
    // that write a heading, and a heading with no attribute run comes back
    // carrying its generated id. The three `347` documents write a quote, a
    // comment fence and a definition - all covered whole - so they land strict
    // and write back unchanged.
    // 933/233 to 938/242 was the previous bump; 933/242 to 938/242 is the five
    // documents arriving with the 7c7f1e3 spec pin, and nothing else: the corpus
    // went from 1175 pairs to 1180, and the five added pairs are the
    // `348-a-closed-inline-construct-spanning-a-verse-boundary` documents that
    // pin PART 9 section 23 hardening a soft break at every depth.
    //
    // Attributed with the ENGINE CHANGE HELD SEPARATE, because this branch
    // changes the tree that the bridge maps. Three classifications under one
    // build each: the old engine at `7666027`, the new engine at `7666027`, and
    // the new engine at `7c7f1e3`. The first two are IDENTICAL - hardening a
    // break changes `soft_break` to `hard_break`, and the bridge covers both, so
    // no pre-existing document moved in either direction. The third differs from
    // the second by exactly the five new names and nothing else.
    //
    // All five land in the strict set and none reports: a line block of
    // paragraphs, breaks, text and emphasis is entirely inside the editor
    // schema. LOSSY therefore does not move.
    //
    // One of them writes back a different source and still does NOT join the
    // declared set above, which is worth stating because it looks like it
    // should. `348-...-4` is authored with a backslash break inside the
    // emphasis, and the canonical writer now drops it - a bare newline hardens
    // at that depth too, so the backslash is no longer what gives the break
    // back (PART 11 section 7c, amended by the same ruling). The spec ships a
    // `.fmt` sidecar for that document spelling it exactly that way. The
    // ROUND TRIP is unaffected: both sides of the comparison drop it.
    // 938/242 to 978/263 is the sixty-one documents arriving with the 0490ae5
    // spec pin, and nothing else: the corpus went from 1180 pairs to 1241, and
    // the added pairs are categories 349 through 359 - the table continuation
    // row inside a container, the definition and the block at a container's
    // content column, the bracketed constructs spanning a line, a verse and an
    // identifier boundary, and the quote inside a quote.
    //
    // Attributed with the ENGINE CHANGE HELD SEPARATE, because this branch
    // changes which line a container takes. Both pins were classified under ONE
    // build - the build carrying this branch - and every one of the 1180
    // documents common to the two pins holds its class: zero moved in either
    // direction, none was removed, and at the OLD pin that build still reports
    // 938/242, which are the numbers this file already declared before any of
    // these fixes existed. So the whole delta is the sixty-one new documents.
    //
    // Forty land in the strict set and twenty-one report. All twenty-one report
    // for one cause already declared above - `soft_break` degrades, the same
    // node `343-an-escaped-hash-...` reports - and none DROPS anything, so no
    // new cause appears. They are the documents that exist to pin a construct or
    // a fold spanning a line boundary, so each holds a soft break by
    // construction; one of the twenty-one also carries `smart_punctuation`,
    // which is declared above as well.
    //
    // FOUR of the forty strict ones join the declared source-lossy set
    // above: `356`, `-2`, `-6` and `-7` are the only ones of the sixty-one that
    // write a HEADING, and a heading with no attribute run comes back carrying
    // its generated id - the first bullet's cause, unchanged. That set is
    // asserted by name rather than counted, so the four are declared there and
    // nothing else moved.
    // 978/263 to 982/263 is the FOUR documents arriving with the 33bf24d spec
    // pin, and nothing else: the corpus went from 1241 pairs to 1245, and the
    // added pairs are all of category 360 - a definition behind an alternating
    // container prefix, its footnote kind, the heading control at the same
    // column, and the peeled control.
    //
    // Attributed with the ENGINE CHANGE HELD SEPARATE, because this branch
    // changes which column the definition pre-pass reaches. Both pins were
    // classified under ONE build - the build carrying this branch - and at the
    // OLD pin that build still reports 978/263 with the declared source-lossy
    // set unchanged, which are the numbers this file already carried before
    // this branch existed. So the whole delta is the four new documents.
    //
    // All four land in the strict set and none reports: three of them are a
    // list, a quote and a definition, and the fourth is a heading, all inside
    // the editor schema. LOSSY therefore does not move.
    //
    // ONE of the four joins the declared source-lossy set above. `360-...-3` is
    // the heading control, and a heading with no attribute run comes back
    // carrying its generated id - the first bullet's cause, unchanged. That set
    // is asserted by name rather than counted, so the one is declared there and
    // nothing else moved.
    // 982/263 to 984/266 is the FIVE documents arriving with the 662e861 spec
    // pin, and nothing else: the corpus went from 1245 pairs to 1250, and the
    // added pairs are all of category 361 - a paragraph opened after a block in
    // an item, its fence and quote-held spellings, and the two controls that
    // must NOT fold.
    //
    // Attributed with the ENGINE CHANGE HELD SEPARATE, because this branch
    // changes which lines an item keeps. Both pins were classified under ONE
    // build - the build carrying this branch - and at the OLD pin that build
    // still reports 982/263 with the declared source-lossy set unchanged, which
    // are the numbers this file already carried before this branch existed. So
    // the whole delta is the five new documents.
    //
    // Three of the five report and two are strict, and the three report for one
    // cause already declared above: `soft_break` degrades, and a document whose
    // whole point is a line FOLDING into an open paragraph holds a soft break by
    // construction. The two that do not fold are the controls, and they carry no
    // break. Nothing DROPS, so no new cause appears, and none of the five joins
    // the declared source-lossy set.
    // 984/266 to 986/267 is the THREE documents arriving with the 275d99d spec
    // pin, and nothing else: the corpus went from 1250 pairs to 1253, the added
    // pairs are all of category 362 - an unterminated container does not extend
    // the item past a blank line, together with its two controls - and no
    // existing pair changed content.
    //
    // Nothing is held separate for an engine change, because this branch
    // carries none. The ruling behind the category (carve#1379) is one this
    // engine already followed; the executable spec was the reader that made
    // the missing closer decide. So the whole delta is the three new documents.
    //
    // Two are strict and one reports, for a cause already declared above. The
    // reporting one is the control with NO blank line, whose last line folds
    // into the item's open paragraph and therefore holds a `soft_break`. The
    // other two put a blank in front of that line, which ends the paragraph and
    // so ends the item, leaving a block of its own with no break in it. Nothing
    // DROPS, so no new cause appears, and none of the three joins the declared
    // source-lossy set.
    // 986/267 to 991/268 is the SIX documents arriving with the 22f7f47 spec
    // pin, and nothing else: the corpus went from 1253 pairs to 1259, the added
    // pairs are all of categories 363, 364 and 365, and no existing pair was
    // removed or changed content.
    //
    // Five are strict and one reports, and the one that reports does so for a
    // cause already declared above: `364-...-2` degrades a `soft_break`, which
    // a document about a line FOLDING into an open paragraph holds by
    // construction. Its base pair `364-...` does not fold and carries no break,
    // so it is strict. All three `365-...` pairs are strict and write back
    // byte-identical source. Nothing DROPS, so no new cause appears.
    //
    // `363-...` is strict but joins the declared source-lossy set above, for
    // the generated-heading-id cause already listed there and for no reason
    // connected to this branch's engine change - see that bullet for the
    // measurement separating the heading from the checkbox.
    // carve#1377 adds two heading/item documents. One is strict; the nested
    // control reports the existing container-boundary loss, so no new loss
    // cause is introduced.
    // Categories 366-368 and 370-371 add eleven documents at the table-column
    // spec pin:
    // seven stay strict and four report already-declared source-layout losses.
    // Category 369 then adds the four quote-reachability documents from
    // carve#1384. One stays strict and three report the existing container
    // boundary/source-layout loss; the bridge introduces no new loss cause.
    // Categories 372, 373 and 375 each add one strict document: the blank raw
    // payload preserves its source, and both table alignment cases round-trip
    // without introducing a new loss cause. Category 374's four definition
    // boundary documents report already-declared source-layout losses.
    // Both numbers moved with the corpus the spec pin now carries (the bump
    // that brings PART 9 §16's backlink name also brings every ruling landed
    // since the last one). The new lossy documents are the tables that STATE
    // their head and foot row counts: ProseMirror's table node has no row-group
    // concept, and this bridge now DECLARES that loss rather than returning a
    // differently-shaped table while claiming nothing was dropped.
    // markup-carve/carve#1436 moves ONE document from strict to reported, and
    // the cause is the ruling rather than the bridge. A continuation marker
    // that attaches nothing renders nothing - it is consumed exactly as a
    // comment line is - so `384-...-6` cannot be written back byte for byte:
    //
    //     - a / `  - b` / `  +` / `  c`   ->   - a / `  - b` / `    c`
    //
    // The marker is gone and `c` is written at the content column it folded
    // into. PART 11 §1 still holds - re-reading that output gives the same
    // tree - so the loss is SOURCE, which is the category this count tracks and
    // the one the bridge already declares for comments.
    // markup-carve/carve#1259 adds category 390's five documents, and nothing
    // else: the corpus went from 1325 pairs to 1330 and no existing pair
    // changed content, because the pairs the clause moves are RESPELLED with
    // the run's terminating space rather than re-expected.
    //
    // Four are strict and one reports, for a cause already declared above:
    // `390-...-5` is the escape (`|\= a |`), and the bridge degrades
    // `escaped_text` because escaping is a source-level concern the editor
    // holds as text. Nothing DROPS, so no new loss cause appears.
    // The spec bump to carve d164b12 adds eleven documents, and every one of
    // them round trips STRICTLY: the corpus goes 1330 -> 1341 and the lossy
    // count does not move. The clause those documents pin is about accessible
    // NAMES - an `aria-label` on a rendered element - which is a render-side
    // attribute the bridge never had to carry, so nothing new is dropped.
    // The bump to carve e88d6e3 adds seventeen documents and changes none:
    // the seven `05-lists-2x` pairs the hard list boundary pins (carve#1513),
    // 394-396 from the escape narrowing (carve#1516), 397's three null-byte
    // documents (carve#1525) and the four container/definition-list extent
    // documents (carve#1526, carve#1542). Sixteen round trip STRICTLY.
    //
    // The one that reports is `394-a-leading-escaped-caret-keeps-its-escape`,
    // and it reports two causes ALREADY DECLARED above: `escaped_text`, because
    // escaping is a source-level concern the editor holds as text - the same
    // cause `390-...-5` carries - and `soft_break`, whitespace in the
    // ProseMirror model, the cause `364-...-2` carries. Nothing DROPS and no
    // new loss cause appears; the counts move because the corpus grew.
    // The bump to carve 7cb4769 (markup-carve/carve#1554) adds FOUR documents:
    // 400 from the container's opening markup (carve#1247) and 401's three from
    // the marker at an item's content column (carve#1517). Three round trip
    // STRICTLY. `401-...-3` reports, and for a cause ALREADY DECLARED above -
    // `soft_break`, whitespace in the ProseMirror model, the same cause
    // `364-...-2` carries. Nothing DROPS and no new loss cause appears; the
    // counts move because the corpus grew.
    // The bump to carve d0b6c92 adds EIGHT documents, categories 402-406.
    // Six round trip STRICTLY. The two that report are
    // `403-an-idle-escape-...` and `404-a-caption-s-marker-separator-...-2`,
    // and each reports `soft_break` alone - whitespace in the ProseMirror
    // model, the cause `364-...-2` and `401-...-3` already carry. Both are
    // documents whose last paragraph spans two source lines, so they hold a
    // soft break by construction. Measured at both pins: `dropped` is EMPTY on
    // both, no document left the lossy set, and no new loss cause appears. The
    // counts move because the corpus grew.
    // The bump to carve 3fdfd6e adds ONE document,
    // `362-an-unterminated-container-does-not-extend-the-item-past-a-blank-line-4`
    // (markup-carve/carve#1610). It round trips STRICTLY, so the lossy side does
    // not move at all: `lossy` measured 301 at both pins, `dropped` is EMPTY,
    // no document left the lossy set and no new loss cause appears. The count
    // moves because the corpus grew.
    // The bump to carve aac95ba adds THIRTEEN documents (corpus 405-408 and the
    // extra variants carve#1610, #1623, #1633 and #1639 brought with them).
    // Every one of them round trips STRICTLY, so the lossy side does not move at
    // all: `lossy` measured 301 at BOTH pins, `dropped` is empty, no document
    // left the lossy set, and no new loss cause appears. The strict count moves
    // because the corpus grew, and only because of that.
    //
    // `407-...-2` needed engine work to reach that state rather than
    // bookkeeping: a `{loose}` definition list lost its wrapper on the way back
    // until PART 9 §17 L7's field was carried across the bridge, which is what
    // the `loose` attribute in `to_pm`/`from_pm` does.
    // The bump to carve 6d08658f adds SIX documents: corpus 411's two indented
    // lone images (markup-carve/carve#1662) and corpus 412's four column-0
    // reference spellings (markup-carve/carve#1666). Every one of them round
    // trips STRICTLY, so the lossy side does not move at all.
    //
    // MEASURED BY INSTRUMENTING THE COUNTS, not by solving the equation: the
    // test was run with `covered` and `lossy` printed, and read 1089 / 301
    // against 1083 / 301 at the old pin. `lossy` holding at 301 is the half
    // that matters - it says no document LEFT the strict set, which a total
    // that merely adds up cannot distinguish from six arriving and some number
    // leaving. The declared source-lossy set is unchanged at 3 entries, so no
    // new loss cause appears either.
    // The bump to carve 2b7920a2 adds FOUR documents: corpus 411's container-
    // level lone images (markup-carve/carve#1682), two of which also gained the
    // `.fmt` canonical form their top-level siblings already had
    // (markup-carve/carve#1693). Every one of them round trips STRICTLY, so the
    // lossy side does not move at all.
    //
    // MEASURED THE SAME WAY as every bump above - `covered` and `lossy` printed
    // from the running test, reading 1093 / 301 against 1089 / 301 at the old
    // pin. `lossy` holding at 301 is the half that matters: it says no document
    // LEFT the strict set, which a total that merely adds up cannot distinguish
    // from four arriving and some number leaving. The declared source-lossy set
    // is unchanged, so no new loss cause appears either.
    // The four #1701 category-413 additions split two/two: the structural
    // cases remain strict, while the two intentional legacy-demotion cases are
    // reported lossy by the bridge. Measured at this spec pin: 1101 / 303.
    // Categories 414 and 415 add eight documents, all strict. The corrected
    // floating-list ownership also lets six existing documents round-trip
    // strictly instead of taking their previously reported source-loss path.
    // Measured over all 1412 documents: 1115 / 297. The exact total below and
    // the independent source-lossy assertion above keep this an evidence-based
    // ratchet rather than allowing either side to drift unnoticed.
    // Categories 416-418 add eight documents. Carrying the fenced block-quote
    // spelling makes all new covered cases strict and also promotes three
    // existing fenced-quote documents out of the reported-loss path. Measured
    // over all 1420 documents: 1126 / 294.
    // Categories 419-426 add 54 documents. Fifty round-trip strictly. Four
    // report an already-declared editor loss (structural content the bridge
    // degrades): 422-4, 422-6, 424-7 and 426-4. Measured over all 1474
    // documents: 1176 / 298; no pre-existing document changed bucket and the
    // exact source-lossy set above remains unchanged.
    // Categories 429-434 add 22 documents, and nothing else moves. Measured by
    // dumping BOTH buckets at 607e7915 and at e539184 and diffing them: 1180/298
    // over 1478 documents becomes 1192/308 over 1500, NO document leaves either
    // bucket in either direction, and all 22 joiners are files the base corpus
    // does not contain at all. (The recorded 1176 predates 607e7915 by four
    // documents, all of them strict - a floor cannot see arrivals on its own
    // side, which is what the exact-total assertion below is for.)
    // Twelve of the 22 round-trip strictly. The other ten report an
    // already-declared editor loss, every one of them with `dropped` empty and
    // the single cause `soft_break`: an invisible line folding as text and a
    // caption slot handed back as paragraph text both produce a MULTI-LINE
    // PARAGRAPH, which is what a soft break is. The five 430 documents and the
    // two 432 documents are the invisible-line band; the three 434 documents are
    // PART 9R R7's give-back paths.
    // The reported-cause table is otherwise identical across the two pins - the
    // same twelve causes, the soft-break row 191 to 201 and every other row
    // unchanged - so no new KIND of loss appeared, and the exact source-lossy
    // set above remains unchanged.
    // 434 is the only category that splits, and instructively: its fourth
    // document is the resolved reference image inside a list item, which is one
    // figure and no soft break, so it is the one of the four that lands strict.
    // Categories 435-439 add 38 documents, and nothing else moves. Measured the
    // same way as the bump above - dumping BOTH buckets at ac2493c0 and at
    // e539184 and diffing them: 1192/308 over 1500 documents becomes 1220/318
    // over 1538, NO document leaves either bucket in either direction, and all
    // 38 joiners are files the base corpus does not contain at all.
    // Twenty-eight round-trip strictly. The other ten report an
    // already-declared editor loss: 435-13, four 437 documents and five 439
    // documents. Every one is the same shape as the ten before them - a refused
    // continuation marker or a marker that opens no description leaves the line
    // folding into the paragraph above it, and a MULTI-LINE PARAGRAPH is what a
    // soft break is.
    // Two 439 documents join, one per bucket, and nothing leaves either -
    // measured the same way, dumping both buckets at 95e0a2de and at c9ff9245
    // and diffing them. `-8` writes `: <tab>text`, which opens a description
    // and round-trips strictly; `-7` writes `: <tab>` with nothing after it,
    // which opens none, so the line folds into the paragraph above it. That is
    // the same soft-break shape as the five 439 documents already here.
    // The bump to carve 2775b6df adds THIRTEEN documents - corpus 441's four
    // (markup-carve/carve#1897) and corpus 442's nine (markup-carve/carve#1910) -
    // and changes none: the pin diff over `tests/corpus` is 26 files, every one
    // of them an ADDITION.
    //
    // MEASURED THE SAME WAY as every bump above - dumping BOTH buckets at
    // 86569bd9 and at 2775b6df with the same engine and diffing them by name:
    // 1222/319 over 1541 documents becomes 1231/323 over 1554, NO document
    // leaves either bucket in either direction, and all 13 joiners are files
    // the base corpus does not contain at all. (The recorded 1221 sat one below
    // the measured 1222 - a floor cannot see arrivals on its own side, which is
    // what the exact-total assertion below is for.)
    //
    // Nine round-trip strictly. The other four report an already-declared
    // editor loss, every one with `dropped` EMPTY and the single cause
    // `soft_break`. They are exactly 442's folding cases - `-2`, `-5`, `-6` and
    // `-9`, the four whose marker column is strictly BETWEEN the item's base
    // and content column - and a folded marker leaves its line running on into
    // the lead text above it, which is a MULTI-LINE PARAGRAPH and so a soft
    // break by construction. The other five 442 documents put the marker AT the
    // base column (a sibling item) or at the content column (a nested item), so
    // no line folds and they land strict; all four 441 documents are strict.
    // The reported-cause table is otherwise identical across the two pins - the
    // same twelve causes, the soft-break row 212 to 216 and every other row
    // unchanged - so no new KIND of loss appeared, and the exact source-lossy
    // set above remains unchanged.
    // The pin moves on to carve 95fc3a04 (markup-carve/carve#1915), which adds
    // corpus 443's TEN documents - the unterminated comment fence band inside a
    // list item - and changes nothing else. Measured the same way against the
    // d4f278b0 dump with the same engine: 1231/323 over 1554 documents becomes
    // 1241/323 over 1564. Every one of the ten round-trips STRICTLY, so the
    // lossy side does not move at all; no document leaves either bucket in
    // either direction and all ten joiners are files the base corpus does not
    // contain. `lossy` holding EXACTLY at 323 is the half that matters - a
    // ceiling that merely fails to rise cannot distinguish ten arrivals from
    // some number leaving, which is what the by-name diff is for.
    // The pin moves on to carve 1b27b68, which adds the 121 documents of the
    // description-body and marker-folding families (corpus 444-452). Measured
    // the same way - dumping BOTH buckets at 95fc3a04 and at 1b27b68 with the
    // same engine and diffing by name: 1241/323 over 1564 becomes 1344/341 over
    // 1685. No document leaves either bucket in either direction; the 103 strict
    // and 18 lossy joiners are exactly the 121 files the base corpus does not
    // contain. All 18 lossy joiners report the same single cause `soft_break` -
    // the 442 pattern, a folded marker leaving a multi-line paragraph - so no
    // new KIND of loss appeared.
    // The include-directive core pin is strict: without a resolver, the bridge
    // sees ordinary literal text. It raises the previous 1349 strict floor by one.
    // The pin moves on to carve 9c84524, which adds 18 documents: corpus 463
    // to 468 plus `12-inline-code-7` and the five `84` heading sidecars.
    // Measured the same way, both buckets dumped by name at each pin and
    // diffed: 1350/345 over 1695 becomes 1366/347 over 1713, and NO document
    // changes bucket in either direction. The two lossy joiners report kinds
    // this set already holds - `italic`, the adjacent-span merge, and
    // `soft_break` - so no new KIND of loss appeared.
    // The pin moves on to carve 9d093d3, which adds ten documents: corpus 469
    // and 470 (markup-carve/carve#2070) and `12-inline-code-8` and `-9`
    // (markup-carve/carve#2074). Measured the same way, both buckets dumped by
    // name at each pin and diffed: 1366/347 over 1713 becomes 1375/348 over
    // 1723, and NO document changes bucket in either direction. The one lossy
    // joiner, 469-3, is `[x]( "t")`, whose quotes degrade as
    // `smart_punctuation` - a kind this set already holds.
    // The pin moves on to carve 2f99a109, which adds fourteen documents and
    // changes none: 12-inline-code-11 and -12 (#2088), the eight 471 rows
    // (#2090, #2103), the two 152 rows (#2085, #2099) and the two 472 rows
    // (#2092). Diffed by name against 77b2308 with this engine, no existing
    // document changes bucket; measured here, 1384/354. Six of the joiners are
    // lossy: the 152 rows degrade `smart_punctuation`, the 472 rows the NEW kind
    // `substitution` (the editor schema still carries each half as a string,
    // markup-carve/carve-grammars#466), and 471-5 and -7 the NEW `strong`
    // report, since a strong inside a strong cannot be two marks.
    // The pin moves on to carve 3a5bebd9, which adds the two 47 caption rows
    // (markup-carve/carve#2114) and changes none. Diffed by name against
    // 57ff3a78 with this engine, 1384/354 becomes 1385/355 and no existing
    // document changes bucket. `-10` is strict; `-11` drops `caption_number`,
    // a cause fourteen documents already carry, so no new kind appeared.
    // The pin at 1cbd2c3f adds 115 documents. Fifty-nine are strict and 56
    // report an existing loss kind, mostly soft breaks and delimiter reflow.
    // Spec 493 adds one ordinary strict document and one document whose
    // comment-only containers report existing bridge losses.
    // The pin moves on to carve e2b7087f, which adds corpus 494 and 495
    // (markup-carve/carve#2224) and changes no existing document. Measured the
    // same way - both buckets dumped by name at 41a070b1 and at e2b7087f with
    // the same engine and diffed: 1445/413 over 1858 documents becomes 1445/415
    // over 1860. NO document changes bucket in either direction, there are no
    // leavers, and the two joiners are files the base corpus does not contain.
    //
    // Both joiners are lossy, both with `dropped` EMPTY and the single cause
    // `table_row_groups` - the degrade `to_pm` reports for a table carrying
    // explicit row groups. That is not a new kind and not a consequence of the
    // three types this bump adds to the vocabulary: no corpus document reaches
    // `block_extension`, `directive` or `ruby`, because Carve 0.1 source spells
    // none of them. 494 is `{header-rows=1}` and 495 is `{footer-rows=1}`, and
    // 376-pipe-tables-can-state-head-and-foot-row-counts already carried the
    // same cause at the old pin - it was the only such document until now.
    //
    // ONE DOCUMENT CHANGED BUCKET TWICE, AND THE READING ABOVE IS WHY IT COULD.
    // "No corpus document reaches `block_extension`, `directive` or `ruby`" was
    // measured off this report, and the report was the thing that was wrong:
    // `to_pm` wrote a directive as an admonition without a degradation row, so a
    // scan of the reported causes could not see the `::: footnotes` in
    // 122-footnotes-placement.crv (carve-rs#1879). The claim held for
    // `block_extension` and `ruby` and was false for `directive`.
    //
    // Reporting that substitution moved 122 out of the strict set - 1445/415 to
    // 1444/416, the only document to move - because the bucket is defined by an
    // EMPTY report and a report that stops lying reclassifies the document.
    //
    // Emitting the node upstream named puts it back: carve-rs#1876 takes
    // `carveDirective` from carve-grammars#562, so the substitution is gone and
    // there is nothing left to report for it. 1444/416 returns to 1445/415, 122
    // is again the only document that moves, and `directive` leaves the reported
    // causes entirely. It regains both the HTML-equality and the
    // canonical-source assertion a lossy document does not get here.
    //
    // The claim about the other two is now measured a way this report cannot
    // mislead - see the note on `NOT_IN_CORPUS`.
    // At spec 238b8059, corpus 384-7 and 384-8 join the reported-lossy set:
    // each contains a folded line and reports only `soft_break`, with nothing
    // dropped. Corpus 498-5 joins the strict set. Classifying every document
    // at this pin and at 34e93335 with the same engine found no existing
    // document moving between sets: 1454/415 becomes 1455/417.
    nbsp_only_degraded.sort();
    assert_eq!(nbsp_only_degraded, vec![
        "268-trailing-whitespace-on-a-content-line-is-dropped-12.crv",
        "29-non-breaking-space.crv",
        "345-a-line-block-s-hard-break-keeps-its-backslash-3.crv",
        "346-a-line-block-s-last-body-line-keeps-its-backslash-2.crv",
        "348-a-closed-inline-construct-spanning-a-verse-boundary-5.crv",
        "400-a-container-starts-at-its-opening-markup-even-where-its-first-child-is-unplaced.crv",
        "402-a-container-ends-at-the-markup-that-closes-it-even-where-its-last-child-is-unplaced.crv",
        "41-line-blocks-2.crv",
        "41-line-blocks-3.crv",
        "41-line-blocks-9.crv",
        "421-a-sigil-fence-takes-its-attribute-line.crv",
        "486-any-character-is-content-of-the-combined-bold-italic-token-3.crv",
    ]);
    // Twelve documents now report the generated-space spelling distinction.
    // Their exact names are pinned above; the current corpus has 1872 files.
    const STRICT: usize = 1443;
    const LOSSY: usize = 429;
    assert!(
        covered >= STRICT,
        "strict round trips fell from {STRICT} to {covered}"
    );
    assert!(
        lossy <= LOSSY,
        "reported-lossy documents rose from {LOSSY} to {lossy}"
    );
    // THE RELATIONSHIP, NOT THE MAGNITUDE. Every corpus document lands in
    // exactly one of the two buckets, which is what this assertion is for -
    // a strict count that merely adds up cannot distinguish documents arriving
    // from documents LEAVING the strict set, so the two sides must still cover
    // the corpus exactly. Written as `STRICT + LOSSY` it also froze the corpus
    // SIZE, so it failed by construction the moment the spec grew: the pin at
    // carve 70e46b55 carries 1422 documents against the 1420 that total was
    // written at, and the assertion broke on a pull request that touches none
    // of this (carve-rs#1416).
    //
    // `common::expected_corpus_size()` counts the `::: compare` blocks in the
    // spec's own examples - a route to the number that does not read the
    // corpus directory this sweep reads - so a truncated checkout still fails
    // here rather than passing by not existing. The other corpus sweeps in
    // this suite already derive it that way; this was the last hardcoded total.
    assert_eq!(
        covered + lossy,
        common::expected_corpus_size(),
        "the sweep bucketed a different number of documents than the spec examples define"
    );
}

#[test]
fn a_tag_keeps_its_flavor() {
    let map: Value = serde_json::from_str(SCHEMA_MAP).expect("schema map is JSON");
    let (value, report) = pm("@alice #topic");
    assert_eq!(
        value.pointer("/content/0/content/0/type"),
        Some(&json!(mapped_names(&map, "mention")[0]))
    );
    assert_eq!(
        value.pointer("/content/0/content/2/type"),
        Some(&json!(mapped_names(&map, "tag")[0]))
    );
    assert!(report.dropped.is_empty());
}

#[test]
fn a_tag_and_a_mention_round_trip_through_their_own_entries() {
    let map: Value = serde_json::from_str(SCHEMA_MAP).expect("schema map is JSON");
    assert_eq!(mapped_names(&map, "tag"), ["carveTag"]);
    assert_eq!(mapped_names(&map, "mention"), ["carveMention"]);

    let source = "ping @alice about #topic\n";
    let bridged = to_prosemirror(&parse(source));
    assert!(bridged.dropped.is_empty() && bridged.degraded.is_empty());
    let import = from_prosemirror_with_report(&bridged.json).expect("the bridge output imports");
    assert!(import.dropped.is_empty() && import.degraded.is_empty());
    assert_eq!(render_carve(&import.document).unwrap(), source);
}

#[test]
fn a_table_keeps_its_header_and_caption() {
    let map: Value = serde_json::from_str(SCHEMA_MAP).expect("schema map is JSON");
    let cells = mapped_names(&map, "table_cell");
    let caption = &mapped_names(&map, "caption")[0];
    let (value, report) = pm("|= H |\n| body |\n^ Table caption\n");
    assert_eq!(
        value.pointer("/content/0/content/0/type"),
        Some(&json!(caption))
    );
    assert_eq!(
        value.pointer("/content/0/content/1/content/0/type"),
        Some(&json!(cells[1]))
    );
    assert!(report.dropped.is_empty(), "{:?}", report.dropped);
}

#[test]
fn a_dropped_type_is_reported_and_absent() {
    let (value, report) = pm("*[HTML]: Hypertext Markup Language\n");
    let reason = report
        .dropped
        .get("abbreviation_def")
        .expect("abbreviation definition is reported");
    assert!(!reason.is_empty());
    assert!(!value.to_string().contains("Hypertext Markup Language"));
}

/// Every wire type the spec defines has a decision in the vendored map.
///
/// The expectation comes from the spec's own `resources/ast-schema.json`, NOT
/// from the map. That is the whole point: a test that asks the map which types
/// exist and then checks those same types against the map cannot fail when an
/// entry is deleted - both sides shrink together. Deleting `math` from the map
/// silently drops every math node out of every editor payload, and the
/// reachability sweep above stays green through it.
///
/// An independent oracle makes that a failure. The schema is the same file the
/// `WIRE_FIELDS` generator reads, so it moves when the spec pin moves.
#[test]
fn every_wire_type_the_spec_defines_has_a_decision() {
    let schema_path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/spec/resources/ast-schema.json");
    let schema: Value = serde_json::from_str(
        &fs::read_to_string(&schema_path).expect("the pinned spec ships its AST schema"),
    )
    .expect("the AST schema is valid JSON");

    let defs = schema["$defs"]
        .as_object()
        .expect("the AST schema defines $defs");
    let wire_types: BTreeSet<&str> = defs
        .values()
        .filter_map(|def| def["properties"]["type"]["const"].as_str())
        .collect();
    assert!(
        wire_types.len() > 40,
        "expected the spec's node vocabulary, found {} types - the schema shape moved",
        wire_types.len()
    );

    let map: Value = serde_json::from_str(SCHEMA_MAP).expect("the vendored map is valid JSON");
    let named = map["types"].as_object().expect("the map names types");
    let unmapped = map["unmapped"]
        .as_object()
        .expect("the map records what it cannot hold");

    let undecided: Vec<&str> = wire_types
        .iter()
        .copied()
        .filter(|ty| !named.contains_key(*ty) && !unmapped.contains_key(*ty))
        .collect();

    assert!(
        undecided.is_empty(),
        "wire types with no decision in resources/prosemirror-schema-map.json: {undecided:?}"
    );
}

/// A name two Carve types claim resolves by the payload, not by map order.
///
/// `carveDiv` is claimed by both `div` and `admonition`, and `link` by both
/// `link` and `autolink`. Which one a reverse lookup returns depends on the
/// order the map is walked in - carve-php preserves the file's key order and
/// gets the owner, this engine sorts and gets the alias - so nothing may
/// depend on the answer. Both pairs are handled by one match arm that reads
/// the node's own state.
///
/// These assert the behavior rather than the arbitration. An earlier version
/// picked the owner by sniffing the entry's notes for the phrase "profile
/// vocabulary only", which a copy-edit upstream would have silently undone;
/// what a reader cares about is that a labelled div is still a labelled div.
#[test]
fn a_carve_div_with_no_kind_is_a_div_and_keeps_its_label() {
    let payload = json!({"type": "doc", "content": [{
        "type": "carveDiv",
        "attrs": {"label": "First"},
        "content": [{"type": "paragraph", "content": [{"type": "text", "text": "Body."}]}]
    }]});
    let doc = from_prosemirror(&payload.to_string()).expect("the payload converts");
    let html = render_html(&doc).unwrap();

    assert!(html.contains("First"), "the label survives: {html}");
    assert!(
        !html.contains("<aside"),
        "a div is not an admonition: {html}"
    );
}

#[test]
fn a_carve_div_with_a_kind_is_an_admonition() {
    let payload = json!({"type": "doc", "content": [{
        "type": "carveDiv",
        "attrs": {"class": "note"},
        "content": [{"type": "paragraph", "content": [{"type": "text", "text": "Body."}]}]
    }]});
    let doc = from_prosemirror(&payload.to_string()).expect("the payload converts");
    let html = render_html(&doc).unwrap();

    assert!(
        html.contains("<aside"),
        "a kind makes it an admonition: {html}"
    );
}

#[test]
fn an_autolink_mark_stays_an_autolink_and_a_link_stays_a_link() {
    let both = json!({"type": "doc", "content": [{"type": "paragraph", "content": [
        {"type": "text", "text": "https://example.com",
         "marks": [{"type": "link", "attrs": {"href": "https://example.com", "carveAutolink": true}}]},
        {"type": "text", "text": " and "},
        {"type": "text", "text": "words",
         "marks": [{"type": "link", "attrs": {"href": "https://example.com"}}]}
    ]}]});
    let doc = from_prosemirror(&both.to_string()).expect("the payload converts");
    let source = render_carve(&doc).expect("the document writes back");

    assert!(
        source.contains("<https://example.com>"),
        "autolink spelling: {source}"
    );
    assert!(
        source.contains("[words](https://example.com)"),
        "link spelling: {source}"
    );
}

/// An unstamped admonition title is the opener, not an authored attribute.
///
/// The outbound side stamps the opener title under its own key so an authored
/// `title` attribute can keep `title`. A payload from an editor that does not
/// stamp it still has to work, and there `title` is all there is - but then it
/// is the opener, and letting it through the attribute pass as well renders the
/// words twice: once as the visible title, once as `title="..."`.
#[test]
fn an_unstamped_admonition_title_does_not_become_an_attribute() {
    let payload = json!({"type": "doc", "content": [{
        "type": "carveDiv",
        "attrs": {"class": "note", "title": "Heads up"},
        "content": [{"type": "paragraph", "content": [{"type": "text", "text": "Body."}]}]
    }]});
    let doc = from_prosemirror(&payload.to_string()).expect("the payload converts");
    let html = render_html(&doc).unwrap();

    assert!(
        html.contains("Heads up"),
        "the opener title is rendered: {html}"
    );
    assert!(
        !html.contains("title=\"Heads up\""),
        "the opener must not also be an authored attribute: {html}"
    );
}

/// One bridge round trip, reported as canonical Carve source before and after.
///
/// Source, not HTML: a comment renders to nothing, so an HTML comparison agrees
/// with itself whether or not the comment came back the way it went in.
fn round_trip(source: &str) -> (String, String) {
    let original = parse(source);
    let pm = to_prosemirror(&original);
    assert!(pm.dropped.is_empty(), "dropped: {:?}", pm.dropped);
    assert!(pm.degraded.is_empty(), "degraded: {:?}", pm.degraded);
    let returned = from_prosemirror(&pm.json).expect("the payload converts back");
    (
        render_carve(&original).expect("the document writes back"),
        render_carve(&returned).expect("the returned document writes back"),
    )
}

#[test]
fn interchange_section_and_block_cell_survive_editor_bridge() {
    let original = from_json(r#"{"type":"document","srcByteLength":0,"children":[{"type":"section","level":2,"children":[{"type":"table","rows":[{"type":"table_row","cells":[{"type":"table_cell","header":false,"blocks":[{"type":"paragraph","children":[{"type":"text","value":"one"}]},{"type":"paragraph","children":[{"type":"text","value":"two"}]}]}]}]}]}]}"#).unwrap();
    let pm = to_prosemirror(&original);
    assert!(pm.dropped.is_empty(), "{:?}", pm.dropped);
    assert!(pm.degraded.is_empty(), "{:?}", pm.degraded);
    let wire: Value = serde_json::from_str(&pm.json).unwrap();
    assert_eq!(
        wire.pointer("/content/0/type"),
        Some(&json!("carveSection"))
    );
    assert_eq!(wire.pointer("/content/0/attrs/level"), Some(&json!(2)));
    assert_eq!(
        wire.pointer("/content/0/content/0/content/0/content/0/attrs/carveCellBlocks"),
        Some(&json!(true))
    );
    let returned = from_prosemirror(&pm.json).unwrap();
    assert_eq!(to_json(&returned), to_json(&original));
}

/// PART 9 §21a: a delimited inline comment ends at `%}`, a `%%` comment ends at
/// the end of the line. Losing the distinction is not a spelling change, it is
/// a deletion: everything the author wrote after the comment on that line is
/// inside the comment on the next parse.
#[test]
fn a_delimited_inline_comment_keeps_its_delimiters() {
    let source = "foo {% bar %} baz\n";
    let (before, after) = round_trip(source);

    assert_eq!(
        after, before,
        "the delimited spelling is the node's identity"
    );
    assert!(
        !after.contains("%%"),
        "a delimited comment must not return as a line comment: {after:?}"
    );
    // The user-visible symptom, stated as itself: the text after the comment is
    // still in the document once the written-back source is read again.
    let reparsed = render_html(&parse(&after)).expect("the written source parses");
    assert!(
        reparsed.contains("baz"),
        "the trailing text survives: {reparsed:?}"
    );
    assert_eq!(
        reparsed,
        render_html(&parse(&before)).expect("the original source parses")
    );
}

/// The same loss in the other direction: a delimited comment's body is hidden,
/// and a `%%` comment hides only its own line, so a multi-line body degraded to
/// `%%` publishes everything from its second line on.
#[test]
fn a_delimited_block_comment_does_not_leak_its_body() {
    let source = "{%\nhidden\n%}\n\nafter\n";
    let (before, after) = round_trip(source);

    assert_eq!(after, before);
    let reparsed = render_html(&parse(&after)).expect("the written source parses");
    assert!(
        !reparsed.contains("hidden"),
        "a hidden body must not become visible: {reparsed:?}"
    );
    assert!(reparsed.contains("after"));
}

/// The flag is carried, not invented: a `%%` comment stays a `%%` comment.
#[test]
fn a_line_comment_is_not_promoted_to_a_delimited_one() {
    for source in ["foo %% bar\n", "%% a whole line\n\nafter\n"] {
        let (before, after) = round_trip(source);
        assert_eq!(after, before, "{source:?}");
        assert!(!after.contains("{%"), "{after:?}");
    }
}

/// carve-grammars#240, part 1: the run comes back in the order it was WRITTEN.
///
/// The AST records the run as a list of slots - `#id`, `.class`, and each key
/// by its own name - and the bridge carries that list verbatim as
/// `carveAttrOrder`. Writing the canonical `#id .class key="val"` instead is a
/// respelling of the author's line, and an HTML comparison cannot see it: both
/// spellings render the same element.
///
/// The source is asserted, not the HTML, for exactly that reason.
#[test]
fn an_attribute_run_comes_back_in_the_order_it_was_written() {
    // carve-grammars wire fixture `attribute-run-in-authored-order`. Its order
    // is none of `#id .class key`, `.class #id key` or any other fixed
    // sequence, so a bridge that sorts cannot pass it by accident.
    let (before, after) = round_trip("[x]{key=c .a #b}\n");
    assert_eq!(after, before);
    assert_eq!(after, "[x]{key=c .a #b}\n");

    // Every slot in one run, in three different orders.
    for source in ["[x]{#i .a k=v}\n", "[x]{.a k=v #i}\n", "[x]{k=v #i .a}\n"] {
        let (before, after) = round_trip(source);
        assert_eq!(after, before, "{source:?}");
    }

    // The order rides on the wire under the name the map gives it.
    let (value, _) = pm("[x]{key=c .a #b}\n");
    assert_eq!(
        value.pointer("/content/0/content/0/marks/0/attrs/carveAttrOrder"),
        Some(&json!(["key", ".class", "#id"]))
    );
}

/// The replay is against the document as it stands, not as it was written.
///
/// An editor may have added a class, assigned an id, or removed a key since.
/// A slot the run names that is gone is skipped; an attribute the run does not
/// name is still an attribute and goes after the ones it does. Dropping it was
/// a deletion: a class an editor toggled on never reached the source.
#[test]
fn a_run_that_no_longer_matches_the_document_keeps_every_attribute() {
    let span = |attrs: Value| {
        json!({"type": "doc", "content": [{"type": "paragraph", "content": [
            {"type": "text", "text": "x", "marks": [{"type": "carveSpan", "attrs": attrs}]}
        ]}]})
    };
    let source = |payload: &Value| {
        render_carve(&from_prosemirror(&payload.to_string()).expect("the payload converts"))
            .expect("the document writes back")
    };

    // A class the run does not name survives, after the slots it does name.
    assert_eq!(
        source(&span(
            json!({"carveAttrOrder": ["k", "#id"], "id": "i", "class": "added", "k": "v"})
        )),
        "[x]{k=v #i .added}\n"
    );
    // An id the run does not name survives too.
    assert_eq!(
        source(&span(
            json!({"carveAttrOrder": [".class"], "class": "a", "id": "i", "later": "v"})
        )),
        "[x]{.a #i later=v}\n"
    );
    // A slot the run names that the document no longer has is skipped.
    assert_eq!(
        source(&span(
            json!({"carveAttrOrder": ["#id", ".class", "gone"], "class": "a"})
        )),
        "[x]{.a}\n"
    );
    // With no run at all, the canonical spelling: `#id .class key="val"`.
    assert_eq!(
        source(&span(json!({"class": "a", "id": "i", "k": "v"}))),
        "[x]{#i .a k=v}\n"
    );
}

/// A recorded run that does not name `#id` proves the id was not authored.
///
/// The only ids Carve synthesizes are heading ids, and they are a resolution
/// artifact - regenerated whenever the document renders. Slotting one into the
/// wire's `id`, which the map defines as the AUTHORED id, wrote it back as an
/// attribute line the author never typed.
#[test]
fn a_generated_heading_id_is_not_an_authored_one() {
    let (before, after) = round_trip("# H\n");
    assert_eq!(after, before);
    assert!(!after.contains("#H"), "{after:?}");
    // The wire too, not only what comes back off it.
    let (value, _) = pm("# H\n");
    assert_eq!(value.pointer("/content/0/attrs/level"), Some(&json!(1)));
    assert_eq!(value.pointer("/content/0/attrs/id"), None);
}

/// A DEDUPLICATED generated id is the case a per-heading test cannot reach.
///
/// `h-2` is a function of the whole document rather than of the heading, so a
/// redundancy test that slugged the heading text alone would call it authored
/// and write it out.
#[test]
fn a_deduplicated_generated_heading_id_is_not_an_authored_one_either() {
    let (before, after) = round_trip("# h\n\n# h\n");
    assert_eq!(after, before);
    assert!(!after.contains('{'), "{after:?}");
}

/// The guard on the recorded run, which the derivation test alone cannot cover:
/// `# h` derives `h`, and this document ALSO writes `{#h}`. The two agree, so
/// deciding on the derivation by itself would delete the author's line.
///
/// THE FIRST HEADING IS LOAD-BEARING. `redundant_heading_ids` returns the empty
/// set for a document in which no heading carries an unslotted id, and `{#h}`
/// alone is such a document - so without a heading whose id IS generated,
/// nothing is ever in the set, the guard is never reached, and this reads as a
/// check that cannot fail. Measured: with the run guard deleted and only the
/// one-heading document, this stayed green and the corpus caught it instead.
#[test]
fn an_authored_heading_id_a_fresh_parse_would_also_derive_stays_authored() {
    let source = "# other\n\n{#h}\n# h\n";
    let (value, _) = pm(source);
    assert_eq!(value.pointer("/content/1/attrs/id"), Some(&json!("h")));
    assert_eq!(
        value.pointer("/content/1/attrs/carveAttrOrder"),
        Some(&json!(["#id"]))
    );
    // The first heading's id is generated and gone, which is what puts `h` into
    // the redundancy set in the first place.
    assert_eq!(value.pointer("/content/0/attrs/id"), None);
    let (before, after) = round_trip(source);
    assert_eq!(after, before);
    assert!(after.contains("{#h}"), "{after:?}");
}

/// The other direction of minimal form, which stops this from being "drop every
/// unslotted id": an AST that reached the bridge from somewhere other than a
/// parse, holding a heading whose text was edited after the id was assigned. A
/// fresh parse derives `new`, so `old` is the only place that lives.
#[test]
fn an_ingested_heading_id_the_text_no_longer_derives_still_travels() {
    let doc = carve::from_json(
        r#"{"type":"document","children":[{"type":"heading","level":1,
        "children":[{"type":"text","value":"new"}],"attrs":{"id":"old"}}],
        "srcByteLength":5}"#,
    )
    .expect("the AST JSON ingests");
    let bridged = to_prosemirror(&doc);
    let value: Value = serde_json::from_str(&bridged.json).expect("the bridge emits JSON");
    assert_eq!(value.pointer("/content/0/attrs/id"), Some(&json!("old")));
    let returned = from_prosemirror(&bridged.json).expect("it imports");
    assert_eq!(render_carve(&returned).unwrap(), "{#old}\n# new\n");
}

/// A structural title uses its namespaced wire slot and does not come back as
/// an authored attribute as well.
#[test]
fn a_structural_title_does_not_come_back_as_an_authored_attribute() {
    for source in [
        "[a]: /u \"T\"\n",
        "[z](safe.html \"T\")\n",
        "![i](s.png \"T\")\n",
    ] {
        let (before, after) = round_trip(source);
        assert_eq!(after, before);
        assert_eq!(after, source);
        assert!(!after.contains("title="), "{source:?}: {after:?}");
    }
}

/// The inverse. The run names `title`, so the field is the ATTRIBUTE, and the
/// structural slot stays empty.
#[test]
fn an_authored_title_attribute_does_not_move_into_the_structural_slot() {
    for (source, structural) in [
        ("[a]: /u {title=T}\n", "/u \"T\""),
        ("[z](safe.html){title=T}\n", "safe.html \"T\""),
        ("![i](s.png){title=T}\n", "s.png \"T\""),
    ] {
        let (before, after) = round_trip(source);
        assert_eq!(after, before);
        assert_eq!(after, source);
        assert!(!after.contains(structural), "{source:?}: {after:?}");
    }
}

/// The namespaced structural slot lets both authored title spellings survive.
#[test]
fn structural_and_authored_titles_both_survive() {
    let (wire, _) = pm("[z](safe.html \"S\"){title=A}\n");
    let wire_attrs = wire
        .pointer("/content/0/content/0/marks/0/attrs")
        .expect("the link mark carries attributes");
    assert_eq!(wire_attrs.pointer("/carveLinkTitle"), Some(&json!("S")));
    assert_eq!(wire_attrs.pointer("/title"), Some(&json!("A")));
    assert_eq!(
        wire_attrs.pointer("/carveAttrOrder"),
        Some(&json!(["title"]))
    );

    for source in [
        "[z](safe.html \"S\"){title=A}\n",
        "![i](s.png \"S\"){title=A}\n",
        "[a]: /u \"S\" {title=A}\n",
    ] {
        let (before, after) = round_trip(source);
        assert_eq!(after, before);
        assert_eq!(after, source);
    }

    let (value, _) = pm("[z](safe.html \"S\"){#i title=A}\n");
    let attrs = value
        .pointer("/content/0/content/0/marks/0/attrs")
        .expect("the link mark carries attributes");
    assert_eq!(attrs.pointer("/carveLinkTitle"), Some(&json!("S")));
    assert_eq!(attrs.pointer("/title"), Some(&json!("A")));
    assert_eq!(
        attrs.pointer("/carveAttrOrder"),
        Some(&json!(["#id", "title"]))
    );
}

/// Stored payloads from before `carveLinkTitle` used `title` for either slot.
/// `carveAttrOrder` remains enough to read both legacy meanings correctly.
#[test]
fn legacy_overloaded_title_payloads_remain_readable() {
    let structural = json!({"type":"doc","content":[{
        "type":"paragraph","content":[{"type":"text","text":"z","marks":[{
            "type":"link","attrs":{"href":"/u","title":"S"}
        }]}]
    }]});
    assert_eq!(
        render_carve(&from_prosemirror(&structural.to_string()).expect("legacy payload imports"))
            .unwrap(),
        "[z](/u \"S\")\n"
    );

    let authored = json!({"type":"doc","content":[{
        "type":"paragraph","content":[{"type":"text","text":"z","marks":[{
            "type":"link","attrs":{
                "href":"/u","title":"A","carveAttrOrder":["title"]
            }
        }]}]
    }]});
    assert_eq!(
        render_carve(&from_prosemirror(&authored.to_string()).expect("legacy payload imports"))
            .unwrap(),
        "[z](/u){title=A}\n"
    );
}

/// carve-grammars#240, part 2: an attribute run on inline code is the code
/// mark's, and the mark carries all four slots.
#[test]
fn an_attribute_run_on_inline_code_survives() {
    // carve-grammars wire fixture `inline-code-with-attributes`.
    let (before, after) = round_trip("A `code`{#i .cls k=v} span.\n");
    assert_eq!(after, before);
    assert_eq!(after, "A `code`{#i .cls k=v} span.\n");

    let (value, _) = pm("A `code`{#i .cls k=v} span.\n");
    let mark = value
        .pointer("/content/0/content/1/marks/0")
        .expect("the code mark is on the text");
    assert_eq!(mark.pointer("/attrs/id"), Some(&json!("i")));
    assert_eq!(mark.pointer("/attrs/class"), Some(&json!("cls")));
    assert_eq!(
        mark.pointer("/attrs/carveAttrOrder"),
        Some(&json!(["#id", ".class", "k"]))
    );
}

/// carve-grammars#240, part 3: a mark with no content rides the carrier node
/// `markCarrierNodes` declares.
///
/// A ProseMirror mark cannot span zero characters, so walking the children of
/// `[](/u)` or `[]{.a}` produces nothing and the construct left the document.
/// The wire shape is asserted against the published fixture, not just the
/// round trip, because the vocabulary is the half that drifts between bridges.
#[test]
fn an_empty_label_link_comes_back() {
    // carve-grammars wire fixture `empty-label-link`.
    let source = "[](https://example.com \"T\"){.a #i}\n";
    let (before, after) = round_trip(source);
    assert_eq!(after, before);
    assert_eq!(after, source);

    let (value, _) = pm(source);
    assert_eq!(
        value.pointer("/content/0/content/0"),
        Some(&json!({
            "type": "carveEmptyMark",
            "attrs": {
                "markType": "link",
                "markAttrs": {
                    "href": "https://example.com",
                    "carveLinkTitle": "T",
                    "id": "i",
                    "class": "a",
                    "carveAttrOrder": [".class", "#id"]
                }
            }
        }))
    );
}

/// The same carrier for an empty span and for an empty link.
///
/// Neither used to report itself: the outbound side walked the children, found
/// none, and returned - so the mark was deleted from the source and the report
/// was empty.
///
/// THE EDITORIAL PAIR USED TO BE HERE, spelled `{++}` and `{--}`.
/// markup-carve/carve#1447 made an empty brace pair text, so no SOURCE spells
/// an empty insertion or deletion any more. The carrier still names both marks
/// and an editor can still hold one; they became interchange-only shapes, and a
/// round trip that starts from Carve has nothing to start from.
#[test]
fn an_empty_span_and_an_empty_link_come_back() {
    // carve-grammars wire fixture `empty-span-and-editorial-marks`.
    let source = "a []{.x} b [](/u) c\n";
    let (before, after) = round_trip(source);
    assert_eq!(after, before);
    assert_eq!(after, source);

    let (value, report) = pm(source);
    assert!(report.dropped.is_empty(), "{:?}", report.dropped);
    assert_eq!(
        value.pointer("/content/0/content"),
        Some(&json!([
            {"type": "text", "text": "a "},
            {"type": "carveEmptyMark", "attrs": {"markType": "carveSpan", "markAttrs": {
                "class": "x", "carveAttrOrder": [".class"]
            }}},
            {"type": "text", "text": " b "},
            {"type": "carveEmptyMark", "attrs": {"markType": "link", "markAttrs": {
                "href": "/u"
            }}},
            {"type": "text", "text": " c"}
        ]))
    );
}

/// Two empty marks side by side are two constructs.
///
/// Import merges adjacent runs of equal marks, which is right for text an
/// editor split in two and wrong for these: neither has text, so merging them
/// leaves one and deletes the other.
///
/// SIDE BY SIDE MEANS NO TEXT BETWEEN THEM. `a []{.x} []{.x} b` reads as two
/// adjacent marks and is not: the space is a text node, so the two carriers
/// never meet in the child list and the merge that would delete one is never
/// reached. Written that way the case passed with the guard removed, which
/// made it a check that could not fail. The rows below are the shapes that do
/// meet - one per carrier the map declares, because the merge compares `type`
/// and `attrs` and each carrier spells those differently.
#[test]
fn two_adjacent_empty_marks_stay_two() {
    // The span, at both ends of a paragraph and in the middle of one.
    for source in ["a []{.x}[]{.x} b\n", "[]{.x}[]{.x}\n"] {
        let (before, after) = round_trip(source);
        assert_eq!(after, before);
        assert_eq!(after.matches("[]{.x}").count(), 2, "{after:?}");
    }

    // The critic pair used to be a row here, spelled `{++}{++}` and
    // `{--}{--}`. markup-carve/carve#1447 made an empty brace pair text, so no
    // source spells those; the merge guard they exercised is on the wire and is
    // still reached by the carriers above.

    // The empty-label link, whose carrier attributes are the mark's own
    // `href`/`title` rather than an attribute run.
    let (before, after) = round_trip("a [](/u)[](/u) b\n");
    assert_eq!(after, before);
    assert_eq!(after.matches("[](/u)").count(), 2, "{after:?}");

    // A space between them is NOT this case, and is kept as the control that
    // says so: it round trips for a different reason.
    let (before, after) = round_trip("a []{.x} []{.x} b\n");
    assert_eq!(after, before);
}

/// An empty mark next to an EQUAL NON-EMPTY one is still two constructs.
///
/// The merge compares `type` and `attrs`, so `[]{.x}` and `[a]{.x}` match on
/// both and only the child lists tell them apart. Refusing the merge for two
/// empty marks left this half: one side carried text, so the guard did not
/// fire and the empty construct was folded into its neighbour and deleted.
///
/// It is silent. The carrier means the outbound side no longer reports the
/// mark dropped, so the loss happens on the way back with an empty report -
/// which is why the round trip has to compare the SOURCE to see it at all.
#[test]
fn an_empty_mark_is_not_absorbed_by_an_equal_neighbour() {
    // Both orders, because the merge folds `b` into `a` and the empty one may
    // be either.
    for (source, construct, count) in [
        ("[]{.x}[a]{.x}\n", "{.x}", 2),
        ("[a]{.x}[]{.x}\n", "{.x}", 2),
        ("[](/u)[a](/u)\n", "](/u)", 2),
        ("[a](/u)[](/u)\n", "](/u)", 2),
        // The critic rows are gone with markup-carve/carve#1447: an empty brace
        // pair is text, so `{++}` and `{--}` are no longer empty marks a source
        // can put next to a non-empty neighbour.
    ] {
        let (before, after) = round_trip(source);
        assert_eq!(after, before, "{source:?}");
        assert_eq!(after.matches(construct).count(), count, "{after:?}");
    }

    // CONTROL. Two NON-EMPTY runs of the same mark still merge, so the guard
    // narrowed the merge rather than switching it off. That the merged form is
    // not the source is the merge's own pre-existing lossiness, not this
    // guard's - asserting it here is what stops a later `return false` for
    // every neighbour passing as a fix.
    let original = parse("[a]{.x}[b]{.x}\n");
    let pm = to_prosemirror(&original);
    let returned = from_prosemirror(&pm.json).expect("the payload converts back");
    assert_eq!(render_carve(&returned).unwrap(), "[ab]{.x}\n");
    // And that merge is now REPORTED (markup-carve/carve-rs#1672), which is why
    // this control cannot go through `round_trip` any more.
    assert!(pm.degraded.contains_key("carveSpan"), "{:?}", pm.degraded);
}

/// An admonition's kind is the opener word, not an authored class.
///
/// The outbound side appends the kind to the classes, because that is where it
/// lives once the element is rendered. Reading it back as authored wrote it
/// twice - `{.note}` above `::: note` - which is the same defect as the
/// generated heading id, in the other slot.
#[test]
fn an_admonition_kind_does_not_come_back_as_an_authored_class() {
    let (before, after) = round_trip("::: note\nBody.\n:::\n");
    assert_eq!(after, before);
    assert!(!after.contains("{.note}"), "{after:?}");

    // An authored class next to the kind keeps its own copy.
    let (before, after) = round_trip("{.extra}\n::: note\nBody.\n:::\n");
    assert_eq!(after, before);
    assert!(after.contains(".extra"), "{after:?}");
}

/// The carrier name is read from the map, like every other ProseMirror name.
///
/// `prose_mirror_names_only_come_from_the_map` sweeps `types`; the carrier
/// lives in `markCarrierNodes`, which is a section of its own, so a bridge that
/// reads only `types` cannot name it and one that hardcodes it is not reading
/// the map at all.
#[test]
fn the_empty_mark_carrier_is_named_by_the_map_and_not_by_the_source() {
    let map: Value = serde_json::from_str(SCHEMA_MAP).expect("schema map is JSON");
    let carriers = map["markCarrierNodes"]
        .as_object()
        .expect("the map declares its mark carriers");
    let declared: Vec<&String> = carriers
        .iter()
        .filter(|(_, entry)| entry["attrs"]["markType"].is_string())
        .map(|(name, _)| name)
        .collect();
    assert_eq!(declared.len(), 1, "one carrier stands in for a mark");

    let source = format!(
        "{}{}",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/prosemirror/to_pm.rs"
        )),
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/prosemirror/from_pm.rs"
        ))
    );
    for name in declared {
        assert!(
            !source.contains(&format!("\"{name}\"")),
            "the carrier name `{name}` is hardcoded rather than read from the map"
        );
        // And it really is what the bridge emits for a mark with no content.
        assert!(
            pm("[](/u)\n").1.json.contains(name.as_str()),
            "an empty-label link does not ride `{name}`"
        );
    }
}

/// A preservation node is part of the wire, not an unknown name.
///
/// This engine writes no `carveUnsupported` and cannot yet read one back, so a
/// payload carrying one has to say which of the two it is. Reporting it the
/// way a typo is reported sends the reader looking for a schema mismatch that
/// is not there.
#[test]
fn a_preservation_node_is_answered_as_what_it_is() {
    let map: Value = serde_json::from_str(SCHEMA_MAP).expect("schema map is JSON");
    let name = map["preservationNodes"]
        .as_object()
        .expect("the map declares its preservation nodes")
        .iter()
        .find(|(_, entry)| entry["attrs"]["carveSource"].is_string())
        .map(|(name, _)| name.clone())
        .expect("a preservation node carries carveSource");

    let payload = json!({"type": "doc", "content": [
        {"type": name, "attrs": {"carveSource": "::: x\n:::\n", "carveType": "div"}}
    ]});
    let error = from_prosemirror(&payload.to_string()).expect_err("it is not readable yet");
    assert!(error.to_string().contains(&name), "{error}");
    assert!(error.to_string().contains("preserves"), "{error}");
}

/// A mark is a property of the characters, so two adjacent spans of one kind
/// come back as a single run. The bridge reports that rather than claiming the
/// trip was lossless (markup-carve/carve-rs#1663).
#[test]
fn two_adjacent_spans_of_one_kind_report_a_degrade() {
    let pm = to_prosemirror(&parse("~{/x/}{/y~/}\n"));
    assert!(
        pm.dropped.is_empty(),
        "nothing is dropped: {:?}",
        pm.dropped
    );
    assert!(
        pm.degraded.contains_key("italic"),
        "the merge is reported: {:?}",
        pm.degraded
    );
    // And the report is not pessimism: the trip really does change the render.
    let returned = from_prosemirror(&pm.json).expect("the document returns");
    assert_ne!(
        render_html(&returned).unwrap(),
        render_html(&parse("~{/x/}{/y~/}\n")).unwrap()
    );
}

#[test]
fn two_adjacent_spans_of_different_kinds_report_nothing() {
    let pm = to_prosemirror(&parse("{/x/}{*y*}\n"));
    assert!(pm.degraded.is_empty(), "{:?}", pm.degraded);
}

#[test]
fn two_spans_of_one_kind_with_text_between_report_nothing() {
    let pm = to_prosemirror(&parse("{/x/} and {/y/}\n"));
    assert!(pm.degraded.is_empty(), "{:?}", pm.degraded);
}

#[test]
fn two_adjacent_spans_whose_attributes_differ_report_nothing() {
    // Marks with different attributes do not merge, so nothing is lost.
    let pm = to_prosemirror(&parse("{/x/}{/y/}{.c}\n"));
    assert!(pm.degraded.is_empty(), "{:?}", pm.degraded);
}

/// The other four kinds that merge the same way. They reported nothing until
/// now, so a host reading `dropped` and `degraded` was told the trip was
/// lossless while the rendered HTML changed (markup-carve/carve-rs#1672).
#[test]
fn two_adjacent_insertions_report_a_degrade() {
    let pm = to_prosemirror(&parse("{+a+}{+b+}\n"));
    assert!(pm.degraded.contains_key("carveInsert"), "{:?}", pm.degraded);
}

#[test]
fn two_adjacent_deletions_report_a_degrade() {
    let pm = to_prosemirror(&parse("{-a-}{-b-}\n"));
    assert!(pm.degraded.contains_key("carveDelete"), "{:?}", pm.degraded);
}

#[test]
fn two_adjacent_attribute_spans_report_a_degrade() {
    let pm = to_prosemirror(&parse("[a]{.c}[b]{.c}\n"));
    assert!(pm.degraded.contains_key("carveSpan"), "{:?}", pm.degraded);
}

#[test]
fn two_adjacent_links_to_one_destination_report_a_degrade() {
    let pm = to_prosemirror(&parse("[a](u)[b](u)\n"));
    assert!(pm.degraded.contains_key("link"), "{:?}", pm.degraded);
}

/// Content loss, not a boundary loss: the second destination was gone, and
/// nothing said so. Two links that differ are two links on the way back.
#[test]
fn two_adjacent_links_to_different_destinations_keep_both() {
    let src = "[a](u)[b](v)\n";
    let pm = to_prosemirror(&parse(src));
    let returned = from_prosemirror(&pm.json).expect("the document returns");
    assert_eq!(
        render_html(&returned).unwrap(),
        render_html(&parse(src)).unwrap()
    );
    assert!(pm.degraded.is_empty(), "{:?}", pm.degraded);
    assert!(pm.dropped.is_empty(), "{:?}", pm.dropped);
}

/// A title is identity too, and it lives in a field rather than in `attrs`.
#[test]
fn two_adjacent_links_with_different_titles_keep_both() {
    let src = "[a](u \"t\")[b](u \"z\")\n";
    let pm = to_prosemirror(&parse(src));
    let returned = from_prosemirror(&pm.json).expect("the document returns");
    assert_eq!(
        render_html(&returned).unwrap(),
        render_html(&parse(src)).unwrap()
    );
    assert!(pm.degraded.is_empty(), "{:?}", pm.degraded);
}

#[test]
fn two_insertions_with_text_between_report_nothing() {
    // CONTROL. The pair has to be adjacent to merge at all.
    let pm = to_prosemirror(&parse("{+a+} and {+b+}\n"));
    assert!(pm.degraded.is_empty(), "{:?}", pm.degraded);
}

/// PART 11 §10k's normalization list, applied to the COMPARISON rather than to
/// either engine: a nested emphasis of two DIFFERENT strengths commutes, so
/// `/*x*/` and `*/x/*` are one document written two ways and the bridge
/// returning the other spelling is not a loss. Corpus 466 pins it.
///
/// Nothing else on the list touches the source: the `<del>` and `<s>` entry is
/// a rendering equivalence, and both spell `~x~` here.
fn normalized_for_comparison(source: &str) -> String {
    let mut out = String::with_capacity(source.len());
    let mut rest = source;
    while let Some(open) = rest.find("/*") {
        let after_open = &rest[open + 2..];
        let Some(close) = after_open.find("*/") else {
            break;
        };
        out.push_str(&rest[..open]);
        out.push_str("*/");
        out.push_str(&after_open[..close]);
        out.push_str("/*");
        rest = &after_open[close + 2..];
    }
    out.push_str(rest);
    out
}

/// An EMPTY code span has no characters for its mark to sit on, so walking its
/// children produced nothing and the span left the document in silence. It
/// takes the same carrier the other empty marks use (corpus 12-inline-code-7,
/// markup-carve/carve-rs#1674).
#[test]
fn an_empty_code_span_survives_the_trip() {
    let doc = parse("{~` ~}\n");
    let pm = to_prosemirror(&doc);
    assert!(pm.dropped.is_empty(), "{:?}", pm.dropped);
    let returned = from_prosemirror(&pm.json).expect("the document returns");
    assert_eq!(
        render_html(&returned).unwrap(),
        render_html(&doc).unwrap(),
        "the empty code span comes back"
    );
    // CONTROL. A filled span was never at risk and still round trips.
    let filled = parse("{~`a`~}\n");
    let pm = to_prosemirror(&filled);
    let returned = from_prosemirror(&pm.json).expect("the document returns");
    assert_eq!(
        render_html(&returned).unwrap(),
        render_html(&filled).unwrap()
    );
}

/// A directive crossing the wire either carries its OWN ProseMirror name or is
/// reported, and the assertion is written that way on purpose: pinning the
/// emitted name to `carveDirective` would pass once carve-rs#1876 takes
/// carve-grammars#562's node and say nothing about the interim, while pinning
/// the degradation row alone would have to be deleted when the real node lands.
/// This form holds under both, and fails on a bridge that substitutes silently.
///
/// It reads the CORPUS rather than a hand-built AST, because the premise of
/// carve-rs#1879 is that the type is reachable from an ordinary document - a
/// constructed `BlockNode::Directive` would prove the arm exists without
/// proving anything about what a corpus run reports.
#[test]
fn a_directive_either_carries_its_own_name_or_is_reported() {
    let map: Value = serde_json::from_str(SCHEMA_MAP).expect("schema map is JSON");
    let own_names = mapped_names(&map, "directive");
    let corpus = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/spec/tests/corpus");
    let mut checked = 0usize;
    for entry in fs::read_dir(corpus).expect("corpus directory exists") {
        let path = entry.expect("corpus entry is readable").path();
        if path.extension().and_then(|extension| extension.to_str()) != Some("crv") {
            continue;
        }
        let source = fs::read_to_string(&path).expect("corpus source is readable");
        let document = parse(&source);
        if !document
            .children
            .iter()
            .any(|block| matches!(block, BlockNode::Directive(_)))
        {
            continue;
        }
        checked += 1;
        let report = to_prosemirror(&document);
        let mut emitted = BTreeSet::new();
        collect_bridge_types(&report.json, &mut emitted);
        let named = own_names.iter().any(|name| emitted.contains(name));
        let told =
            report.degraded.contains_key("directive") || report.dropped.contains_key("directive");
        assert!(
            named || told,
            "{}: a directive crossed the wire as {:?} with no `directive` row in degraded {:?} or dropped {:?}",
            path.display(),
            emitted,
            report.degraded.keys().collect::<Vec<_>>(),
            report.dropped.keys().collect::<Vec<_>>(),
        );
    }
    assert!(
        checked > 0,
        "no corpus document parsed to a directive, so this gate asserted nothing"
    );
}

// The four types carve-grammars#562 named at once (carve-rs#1876). Each one is
// asserted in BOTH directions: a conforming payload of that type decodes, and
// what this engine writes reads back. One direction alone passes on a bridge
// that can write a name nothing reads, which is the gap these four were the
// standing example of.
//
// Three of them have no Carve 0.1 spelling, so their AST side is built through
// `from_json` rather than parsed. That is the only route a document carrying one
// takes to this bridge either.

/// The AST-JSON wrapper a single block needs, so a test states only its node.
fn document_of(block: &str) -> String {
    format!(r#"{{"type":"document","srcByteLength":0,"children":[{block}]}}"#)
}

/// The AST-JSON wrapper a single inline needs.
fn paragraph_of(inline: &str) -> String {
    document_of(&format!(r#"{{"type":"paragraph","children":[{inline}]}}"#))
}

fn pm_name(carve_type: &str) -> String {
    let map: Value = serde_json::from_str(SCHEMA_MAP).expect("schema map is JSON");
    mapped_names(&map, carve_type)
        .into_iter()
        .next()
        .unwrap_or_else(|| panic!("the map names no node for `{carve_type}`"))
}

#[test]
fn a_directive_travels_as_its_own_node_in_both_directions() {
    let (value, bridge) = pm("::: toc\n:::\n");
    let node = &value["content"][0];
    assert_eq!(node["type"], Value::String(pm_name("directive")));
    assert_eq!(node["attrs"]["kind"], "toc");
    // Not the admonition's spelling, and not a class either: the kind word is
    // what the schema refuses on an admonition, so it may not ride as one.
    assert!(node["attrs"].get("carveAdmonitionKind").is_none());
    assert!(node["attrs"].get("class").is_none());
    assert!(
        bridge.degraded.is_empty() && bridge.dropped.is_empty(),
        "a mapped node is not a loss: {:?} {:?}",
        bridge.degraded,
        bridge.dropped
    );

    let back = from_prosemirror(&bridge.json).expect("the node it wrote reads back");
    let BlockNode::Directive(directive) = &back.children[0] else {
        panic!("expected a directive, got {:?}", back.children[0]);
    };
    assert_eq!(directive.kind, "toc");
}

#[test]
fn an_arriving_directive_node_decodes() {
    let payload = json!({"type":"doc","content":[
        {"type":pm_name("directive"),"attrs":{"kind":"footnotes","label":"main"}}
    ]});
    let document = from_prosemirror(&payload.to_string()).expect("a conforming directive decodes");
    let BlockNode::Directive(directive) = &document.children[0] else {
        panic!("expected a directive, got {:?}", document.children[0]);
    };
    assert_eq!(directive.kind, "footnotes");
    assert_eq!(directive.label.as_deref(), Some("main"));
    // The structural kind is not ALSO an authored attribute: writing it into the
    // run would render `{kind=footnotes}` the author never typed, which is the
    // mistake the admonition's class already had to be unwound from.
    assert!(directive.attrs.is_none(), "{:?}", directive.attrs);
}

#[test]
fn a_block_extension_keeps_its_fallback_as_content_both_ways() {
    let source = document_of(
        r#"{"type":"block_extension","name":"org.example.diagram","version":"2",
            "payload":{"format":"application/json","value":{"shape":"box"}},
            "fallback":{"type":"paragraph","children":[{"type":"text","value":"a box"}]}}"#,
    );
    let document = from_json(&source).expect("the AST-JSON wire carries one");
    let bridge = to_prosemirror(&document);
    let value: Value = serde_json::from_str(&bridge.json).expect("bridge emits JSON");
    let node = &value["content"][0];
    assert_eq!(node["type"], Value::String(pm_name("block_extension")));
    assert_eq!(node["attrs"]["name"], "org.example.diagram");
    assert_eq!(node["attrs"]["version"], "2");
    assert_eq!(node["attrs"]["payload"]["format"], "application/json");
    assert_eq!(node["attrs"]["payload"]["value"]["shape"], "box");
    // The fallback is the node's CONTENT, so it is still a block the editor
    // holds rather than a value beside it.
    assert_eq!(
        node["content"][0]["type"],
        Value::String(pm_name("paragraph"))
    );
    assert!(
        bridge.degraded.is_empty() && bridge.dropped.is_empty(),
        "name, version and payload all have somewhere to go now: {:?} {:?}",
        bridge.degraded,
        bridge.dropped
    );

    let back = from_prosemirror(&bridge.json).expect("the node it wrote reads back");
    let BlockNode::BlockExtension(extension) = &back.children[0] else {
        panic!("expected a block extension, got {:?}", back.children[0]);
    };
    assert_eq!(extension.name, "org.example.diagram");
    assert_eq!(extension.version.as_deref(), Some("2"));
    assert_eq!(
        extension.payload.as_ref().map(|p| p.format.as_str()),
        Some("application/json")
    );
    assert!(matches!(*extension.fallback, BlockNode::Paragraph(_)));
}

#[test]
fn a_block_extension_payload_with_no_fallback_is_refused() {
    // The fallback is REQUIRED, and a payload without one cannot become a block
    // extension. Inventing an empty block for it would put a document on the
    // Carve side that says the extension degrades to nothing.
    let payload = json!({"type":"doc","content":[
        {"type":pm_name("block_extension"),"attrs":{"name":"org.example.diagram"}}
    ]});
    let error = from_prosemirror(&payload.to_string())
        .expect_err("a block extension without its fallback is not a document");
    assert!(error.to_string().contains("fallback"), "{error}");
}

#[test]
fn a_ruby_annotation_travels_as_an_inline_atom_both_ways() {
    let source = paragraph_of(
        r#"{"type":"ruby","pairs":[
            {"base":[{"type":"text","value":"漢"}],
             "annotation":[{"type":"text","value":"kan"}]},
            {"base":[{"type":"text","value":"字"}],"annotation":[]}]}"#,
    );
    let document = from_json(&source).expect("the AST-JSON wire carries one");
    let bridge = to_prosemirror(&document);
    let value: Value = serde_json::from_str(&bridge.json).expect("bridge emits JSON");
    let node = &value["content"][0]["content"][0];
    assert_eq!(node["type"], Value::String(pm_name("ruby")));
    // An ATOM: the pairs are the association, so they ride in the attribute and
    // the node holds no content of its own.
    assert!(node.get("content").is_none(), "{node}");
    assert_eq!(node["attrs"]["pairs"][0]["base"][0]["text"], "\u{6f22}");
    assert_eq!(node["attrs"]["pairs"][0]["annotation"][0]["text"], "kan");
    // An annotation may be empty; a base may not.
    assert_eq!(
        node["attrs"]["pairs"][1]["annotation"],
        Value::Array(Vec::new())
    );
    assert!(
        bridge.degraded.is_empty() && bridge.dropped.is_empty(),
        "the pairs are carried, not flattened to parentheses: {:?} {:?}",
        bridge.degraded,
        bridge.dropped
    );

    let back = from_prosemirror(&bridge.json).expect("the node it wrote reads back");
    assert_eq!(
        to_json(&back),
        to_json(&document),
        "a ruby annotation survives the trip unchanged"
    );
}

#[test]
fn an_arriving_ruby_node_decodes() {
    let payload = json!({"type":"doc","content":[{"type":pm_name("paragraph"),"content":[
        {"type":pm_name("ruby"),"attrs":{"pairs":[
            {"base":[{"type":pm_name("text"),"text":"NY"}],
             "annotation":[{"type":pm_name("text"),"text":"New York"}]}]}}
    ]}]});
    let document = from_prosemirror(&payload.to_string()).expect("a conforming ruby decodes");
    let BlockNode::Paragraph(paragraph) = &document.children[0] else {
        panic!("expected a paragraph, got {:?}", document.children[0]);
    };
    let carve::InlineNode::Ruby(ruby) = &paragraph.children[0] else {
        panic!(
            "expected a ruby annotation, got {:?}",
            paragraph.children[0]
        );
    };
    assert_eq!(ruby.pairs.len(), 1);
    assert!(render_html(&document).unwrap().contains("New York"));
}

#[test]
fn small_caps_travels_as_a_mark_in_both_directions() {
    let source =
        paragraph_of(r#"{"type":"small_caps","children":[{"type":"text","value":"nato"}]}"#);
    let document = from_json(&source).expect("the AST-JSON wire carries one");
    let bridge = to_prosemirror(&document);
    let value: Value = serde_json::from_str(&bridge.json).expect("bridge emits JSON");
    let text = &value["content"][0]["content"][0];
    // A MARK, so the text stays text and stays editable - a node would make it
    // an atom and take the letters out of the editor's reach.
    assert_eq!(text["type"], Value::String(pm_name("text")));
    assert_eq!(text["text"], "nato");
    assert_eq!(
        text["marks"][0]["type"],
        Value::String(pm_name("small_caps"))
    );
    assert!(
        bridge.degraded.is_empty() && bridge.dropped.is_empty(),
        "an unattributed wrapper loses nothing: {:?} {:?}",
        bridge.degraded,
        bridge.dropped
    );

    let back = from_prosemirror(&bridge.json).expect("the mark it wrote reads back");
    assert_eq!(
        to_json(&back),
        to_json(&document),
        "a small-caps wrapper survives the trip unchanged"
    );
}

#[test]
fn an_arriving_small_caps_mark_decodes() {
    let payload = json!({"type":"doc","content":[{"type":pm_name("paragraph"),"content":[
        {"type":pm_name("text"),"text":"nato","marks":[{"type":pm_name("small_caps")}]}
    ]}]});
    let document = from_prosemirror(&payload.to_string()).expect("a conforming mark decodes");
    let BlockNode::Paragraph(paragraph) = &document.children[0] else {
        panic!("expected a paragraph, got {:?}", document.children[0]);
    };
    let carve::InlineNode::Emphasis(emphasis) = &paragraph.children[0] else {
        panic!("expected a wrapper, got {:?}", paragraph.children[0]);
    };
    assert_eq!(emphasis.kind, carve::EmphasisKind::SmallCaps);
}

/// An ATTRIBUTED small-caps wrapper reports the reshaping, because there is one.
///
/// CARVE-P12-050 keeps the run on an ordinary attributed span around the same
/// children rather than on the wrapper, which is what the map's entry says the
/// mark carries alone. Nothing is lost, but the wrapper comes back nested
/// inside a span instead of carrying the run, and a shape change a consumer can
/// see is a shape change the report names.
#[test]
fn an_attributed_small_caps_wrapper_puts_its_run_on_a_span_and_says_so() {
    let source = paragraph_of(
        r#"{"type":"small_caps","attrs":{"classes":["x"],"order":[".class"]},
            "children":[{"type":"text","value":"nato"}]}"#,
    );
    let document = from_json(&source).expect("the AST-JSON wire carries one");
    let bridge = to_prosemirror(&document);
    let value: Value = serde_json::from_str(&bridge.json).expect("bridge emits JSON");
    let marks = &value["content"][0]["content"][0]["marks"];
    assert_eq!(marks[0]["type"], Value::String(pm_name("span")));
    assert_eq!(marks[0]["attrs"]["class"], "x");
    assert_eq!(marks[1]["type"], Value::String(pm_name("small_caps")));
    assert!(marks[1].get("attrs").is_none(), "{marks}");
    assert!(
        bridge.degraded.contains_key("small_caps"),
        "the reshaping is reported: {:?}",
        bridge.degraded
    );

    let back = from_prosemirror(&bridge.json).expect("the marks it wrote read back");
    assert!(
        render_html(&back).unwrap().contains("x"),
        "the class survives: {}",
        render_html(&back).unwrap()
    );
}

/// An authored attribute whose name is the structural one loses, and is told.
///
/// Upstream spells a directive's kind `kind` rather than under a `carve` prefix
/// the way an admonition's is, so an author who wrote `{kind=...}` collides with
/// it. The shared map's spelling is not this bridge's to re-choose, but the
/// collision costs the authored value and a silent overwrite is the failure
/// carve-rs#1879 named.
#[test]
fn an_authored_attribute_colliding_with_a_structural_one_is_reported() {
    let (value, bridge) = pm("{kind=custom}\n::: toc\n:::\n");
    assert_eq!(value["content"][0]["attrs"]["kind"], "toc");
    assert!(
        bridge.degraded.contains_key("directive"),
        "the authored value is gone and nothing said so: {:?}",
        bridge.degraded
    );
}
