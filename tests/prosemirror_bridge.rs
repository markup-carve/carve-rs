use carve::{from_prosemirror, parse, render_html, to_prosemirror};
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

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
];

// Wire types the published map deliberately does not name, and the entry each
// resolves through. `tag` is a real PART 12 type, but the spec classifies it
// under `mention` for profile purposes - profiles.md says the vocabulary "does
// not list" it - so carve-grammars has no `tag` key and should not grow one.
// Restating it as a local entry is how a vendored copy stops being a copy;
// carve-php did exactly that and the entry turned out to be dead.
const ALIASED_TYPES: &[(&str, &str)] = &[("tag", "mention")];

fn pm(source: &str) -> (Value, carve::ProseMirrorDoc) {
    let result = to_prosemirror(&parse(source));
    let value = serde_json::from_str(&result.json).expect("bridge emits JSON");
    (value, result)
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
    let names = mapped_names(&map, "mention");
    let input = json!({"type":"doc","content":[{"type":"paragraph","content":[
        {"type":names[0],"attrs":{"id":"alice"}}, {"type":"text","text":" "},
        {"type":names[1],"attrs":{"id":"topic"}}
    ]}]});
    let doc = from_prosemirror(&input.to_string()).expect("mapped mention flavors import");
    let html = render_html(&doc).unwrap();
    assert!(html.contains("class=\"mention\""));
    assert!(html.contains("class=\"tag\""));
    assert!(html.contains("@alice"));
    assert!(html.contains("#topic"));
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

#[test]
fn fully_covered_corpus_documents_round_trip_through_prosemirror() {
    let corpus = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/spec/tests/corpus");
    let mut covered = 0usize;
    let mut lossy = 0usize;
    for entry in fs::read_dir(corpus).expect("corpus directory exists") {
        let path = entry.expect("corpus entry is readable").path();
        if path.extension().and_then(|v| v.to_str()) != Some("crv") {
            continue;
        }
        let original = parse(&fs::read_to_string(&path).expect("corpus source is readable"));
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
            covered += 1;
        } else {
            lossy += 1;
        }
    }
    eprintln!("ProseMirror corpus: {covered} strict, {lossy} reported lossy");
    // A ratchet, not a floor of one. `covered > 0` passes with a single
    // document, so a change that quietly moved hundreds of documents out of
    // the strict set - by reporting a type as dropped rather than carrying it -
    // would not fail anything. Raise these when the numbers improve.
    const STRICT: usize = 791;
    const LOSSY: usize = 215;
    assert!(
        covered >= STRICT,
        "strict round trips fell from {STRICT} to {covered}"
    );
    assert!(
        lossy <= LOSSY,
        "reported-lossy documents rose from {LOSSY} to {lossy}"
    );
    assert_eq!(covered + lossy, STRICT + LOSSY, "the corpus size moved");
}

#[test]
fn a_tag_keeps_its_flavor() {
    let map: Value = serde_json::from_str(SCHEMA_MAP).expect("schema map is JSON");
    let names = mapped_names(&map, "mention");
    let (value, report) = pm("@alice #topic");
    assert_eq!(
        value.pointer("/content/0/content/0/type"),
        Some(&json!(names[0]))
    );
    assert_eq!(
        value.pointer("/content/0/content/2/type"),
        Some(&json!(names[1]))
    );
    assert!(report.dropped.is_empty());
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
        .filter(|ty| {
            !named.contains_key(*ty)
                && !unmapped.contains_key(*ty)
                && !ALIASED_TYPES.iter().any(|(alias, _)| alias == ty)
        })
        .collect();

    assert!(
        undecided.is_empty(),
        "wire types with no decision in resources/prosemirror-schema-map.json: {undecided:?}"
    );
}

/// An alias stops being an alias the moment the map names the type itself.
///
/// Without this the list only ever grows: upstream could add a `tag` entry and
/// the local indirection would sit there forever, describing nothing.
#[test]
fn no_aliased_type_is_named_by_the_map() {
    let map: Value = serde_json::from_str(SCHEMA_MAP).expect("the vendored map is valid JSON");
    let named = map["types"].as_object().expect("the map names types");

    for (alias, through) in ALIASED_TYPES {
        assert!(
            !named.contains_key(*alias),
            "the map now names `{alias}` itself - drop it from ALIASED_TYPES and read it directly"
        );
        assert!(
            named.contains_key(*through),
            "`{alias}` resolves through `{through}`, which the map does not name"
        );
    }
}

/// No ProseMirror name is left to be resolved by map iteration order.
///
/// Two Carve types can claim one ProseMirror name, and in both current cases
/// the second claimant is a profile-vocabulary alias of the first: an
/// admonition is a div with a type class, an autolink is a link whose text is
/// its destination. carve-php resolves these correctly by accident - PHP walks
/// the map in the file's key order, where the owner happens to come first.
/// This engine walks a sorted map, where the ALIAS wins both times.
///
/// That is not a cosmetic difference. `carveDiv` resolving to `admonition`
/// routed every labelled div down a path that does not carry the label, so
/// `:::[First]` came back as a bare div with the word gone and nothing
/// reported dropped. `from_pm.rs` names the owner explicitly; this test fails
/// if the map grows a collision that list has not been told about.
#[test]
fn no_prose_mirror_name_resolves_by_alphabet() {
    const OWNED: &[(&str, &str)] = &[("carveDiv", "div"), ("link", "link")];

    let map: Value = serde_json::from_str(SCHEMA_MAP).expect("the vendored map is valid JSON");
    let types = map["types"].as_object().expect("the map names types");

    let mut claims: std::collections::BTreeMap<String, Vec<String>> = Default::default();
    for (carve_type, entry) in types {
        let accepts = entry["accepts"]
            .as_array()
            .map(|v| {
                v.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_else(Vec::new);
        for name in mapped_names(&map, carve_type).into_iter().chain(accepts) {
            claims.entry(name).or_default().push(carve_type.clone());
        }
    }

    for (name, mut owners) in claims {
        if owners.len() < 2 {
            continue;
        }
        owners.sort();
        let declared = OWNED.iter().find(|(pm, _)| *pm == name);
        let (_, owner) = declared.unwrap_or_else(|| {
            panic!(
                "`{name}` is claimed by {owners:?} and nothing says which owns it - \
                 the sorted map would hand it to `{}`. Name the owner in from_pm.rs.",
                owners[0]
            )
        });
        assert!(
            owners.iter().any(|t| t == owner),
            "`{name}` is declared owned by `{owner}`, which no longer claims it: {owners:?}"
        );
    }
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
