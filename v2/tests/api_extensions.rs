//! PROVISIONAL extension host of `carve2::api` (C2-X-050 to C2-X-055).

use carve2::api::{
    self, Carve, Channel, DocumentTransform, Error, Extension, ExtensionDeclaration, Mode, Options,
    RenderContext, Renderer, Tree, extension::InsertTransform,
};
use serde_json::{Value, json};

fn declaration(name: &str) -> ExtensionDeclaration {
    ExtensionDeclaration {
        name: name.into(),
        version: "1".into(),
        required: false,
        slots: Vec::new(),
    }
}

fn declared_tree(names: &[&str], children: &Value) -> Tree {
    let extensions: Vec<Value> = names
        .iter()
        .map(|n| json!({"name": n, "version": "1", "required": false, "slots": []}))
        .collect();
    api::decode_tree(
        &json!({"type": "document", "astVersion": "2.0", "extensions": extensions, "children": children})
            .to_string(),
    )
    .unwrap()
}

fn para(text: &str) -> Value {
    json!({"type": "paragraph", "children": [{"type": "text", "value": text}]})
}

fn block(name: &str, fallback: &str) -> Value {
    json!({"type": "block_extension", "name": name, "version": "1", "children": [],
        "fallback": [para(fallback)]})
}

fn insert(name: &str, after: &str, node: Value) -> Extension {
    Extension {
        declaration: declaration(name),
        renderers: Vec::new(),
        transform: Some(Box::new(InsertTransform {
            after: after.into(),
            node,
        })),
    }
}

fn codes(r: &api::Rendered) -> Vec<&str> {
    r.diagnostics.iter().map(|e| e.code.as_str()).collect()
}

#[test]
fn c2_x_053_a_transform_inserts_a_declared_node() {
    let tree = declared_tree(&["sample.insert"], &json!([para("A")]));
    let mut carve = Carve::new();
    carve
        .register_extension(insert(
            "sample.insert",
            "/children/0",
            block("sample.insert", "B"),
        ))
        .unwrap();
    let html = carve.render_tree(&tree, &Options::default()).unwrap();
    assert_eq!(html, "<p>A</p>\n<p>B</p>");
    assert_eq!(
        api::render_tree(&tree, &Options::default()).unwrap(),
        "<p>A</p>"
    );
}

#[test]
fn c2_x_053_a_later_transform_sees_the_earlier_result() {
    let tree = declared_tree(&["sample.first", "sample.second"], &json!([para("A")]));
    let mut carve = Carve::new();
    carve
        .register_extension(insert(
            "sample.first",
            "/children/0",
            block("sample.first", "B"),
        ))
        .unwrap();
    carve
        .register_extension(insert(
            "sample.second",
            "/children/1",
            block("sample.second", "C"),
        ))
        .unwrap();
    let html = carve.render_tree(&tree, &Options::default()).unwrap();
    assert_eq!(html, "<p>A</p>\n<p>B</p>\n<p>C</p>");
}

#[test]
fn c2_x_055_a_duplicate_authored_id_fails_the_transform() {
    let mut first = para("A");
    first["attrs"] = json!({"id": "a"});
    let tree = declared_tree(&["sample.first"], &json!([first]));
    let mut node = block("sample.first", "B");
    node["attrs"] = json!({"id": "a"});
    let mut carve = Carve::new();
    carve
        .register_extension(insert("sample.first", "/children/0", node))
        .unwrap();
    let r = carve
        .render_tree_with_diagnostics(&tree, &Options::default())
        .unwrap();
    assert_eq!(r.html, "<p id=\"a\">A</p>");
    assert_eq!(codes(&r), ["extension-transform-failed"]);
    assert!(r.diagnostics[0].message.contains("sample.first"));
}

struct Failing;

impl DocumentTransform for Failing {
    fn transform(&self, _view: Tree) -> Result<Tree, Error> {
        Err(Error::Unsupported { call: "test" })
    }
}

#[test]
fn c2_x_050_a_failing_transform_keeps_its_input() {
    let mut carve = Carve::new();
    carve
        .register_extension(Extension {
            declaration: declaration("ex.fail"),
            renderers: Vec::new(),
            transform: Some(Box::new(Failing)),
        })
        .unwrap();
    let r = carve
        .render_with_diagnostics("x", &Options::default())
        .unwrap();
    assert_eq!(r.html, "<p>x</p>");
    assert_eq!(codes(&r), ["extension-transform-failed"]);
}

struct Boxed;

impl Renderer for Boxed {
    fn render(&self, node: &Value, cx: &RenderContext<'_>) -> Result<String, Error> {
        let body = cx.render_blocks(&node["slots"]["body"])?;
        let tag = if cx.mode() == Mode::Static {
            "aside"
        } else {
            "div"
        };
        Ok(format!(
            "<{tag} class=\"{}\">{body}</{tag}>",
            cx.escape_attribute("b&x")
        ))
    }
}

struct Broken;

impl Renderer for Broken {
    fn render(&self, _node: &Value, _cx: &RenderContext<'_>) -> Result<String, Error> {
        Err(Error::Unsupported { call: "test" })
    }
}

fn boxed_tree() -> Tree {
    let mut node = block("ex.box", "fallback");
    node["slots"] = json!({"body": [para("*inside*")]});
    let tree = json!({"type": "document", "astVersion": "2.0",
        "extensions": [{"name": "ex.box", "version": "1", "required": false, "slots": ["body"]}],
        "children": [node]});
    api::decode_tree(&tree.to_string()).unwrap()
}

fn with_renderers(renderers: Vec<(Channel, Box<dyn Renderer>)>) -> Carve {
    let mut carve = Carve::new();
    carve
        .register_extension(Extension {
            declaration: ExtensionDeclaration {
                slots: vec!["body".into()],
                ..declaration("ex.box")
            },
            renderers,
            transform: None,
        })
        .unwrap();
    carve
}

#[test]
fn c2_x_052_the_mode_selects_the_channel() {
    let tree = boxed_tree();
    let interactive = with_renderers(vec![(Channel::Interactive, Box::new(Boxed))]);
    let r = interactive
        .render_tree_with_diagnostics(&tree, &Options::default())
        .unwrap();
    assert_eq!(r.html, "<div class=\"b&amp;x\"><p>*inside*</p></div>");
    assert!(r.diagnostics.is_empty());
    let stat = Options {
        mode: Mode::Static,
        ..Options::default()
    };
    let r = interactive
        .render_tree_with_diagnostics(&tree, &stat)
        .unwrap();
    assert_eq!(r.html, "<p>fallback</p>");
    assert_eq!(codes(&r), ["extension-fallback-rendered"]);
    let build = with_renderers(vec![(Channel::BuildTime, Box::new(Boxed))]);
    let html = build.render_tree(&tree, &stat).unwrap();
    assert_eq!(html, "<aside class=\"b&amp;x\"><p>*inside*</p></aside>");
}

#[test]
fn c2_x_050_a_failing_renderer_writes_the_fallback() {
    let carve = with_renderers(vec![(Channel::Interactive, Box::new(Broken))]);
    let r = carve
        .render_tree_with_diagnostics(&boxed_tree(), &Options::default())
        .unwrap();
    assert_eq!(r.html, "<p>fallback</p>");
    assert_eq!(codes(&r), ["extension-renderer-failed"]);
    let strict = Options {
        strict_losses: true,
        ..Options::default()
    };
    let err = carve.render_tree(&boxed_tree(), &strict).unwrap_err();
    assert!(matches!(err, Error::Render { .. }));
}

#[test]
fn two_renderers_for_one_channel_are_refused() {
    let mut carve = Carve::new();
    let err = carve
        .register_extension(Extension {
            declaration: declaration("ex.box"),
            renderers: vec![
                (Channel::Interactive, Box::new(Boxed)),
                (Channel::Interactive, Box::new(Broken)),
            ],
            transform: None,
        })
        .unwrap_err();
    assert!(matches!(err, Error::Option { .. }));
}

#[test]
fn without_extensions_the_host_renders_as_the_free_calls() {
    let carve = Carve::new();
    for src in ["# T\n\nA [^n]\n\n[^n]: note", "x {{ y }}", ""] {
        let o = Options::default();
        assert_eq!(
            carve.render(src, &o).unwrap(),
            api::render(src, &o).unwrap()
        );
        assert_eq!(
            carve.render_with_diagnostics(src, &o).unwrap(),
            api::render_with_diagnostics(src, &o).unwrap()
        );
    }
}

fn call(request: &Value) -> Value {
    serde_json::from_str(&api::json::call(&request.to_string())).unwrap()
}

#[test]
fn json_host_document_transforms() {
    let tree = json!({"type": "document", "astVersion": "2.0",
        "extensions": [{"name": "sample.insert", "version": "1", "required": false, "slots": []}],
        "children": [para("A")]});
    let out = call(
        &json!({"call": "render", "tree": tree, "host": {"documentTransforms": [
            {"name": "sample.insert", "after": "/children/0", "insert": block("sample.insert", "B")}
        ]}}),
    );
    assert_eq!(out["ok"]["html"], "<p>A</p>\n<p>B</p>");
    let bad = call(&json!({"call": "render", "source": "x", "host": {"renderers": {}}}));
    assert_eq!(bad["error"]["category"], "usage");
    let unnamed = call(&json!({"call": "render", "source": "x",
        "host": {"documentTransforms": [{"after": "/children/0", "insert": para("B")}]}}));
    assert_eq!(unnamed["error"]["name"], "CarveOptionError");
}

struct SeesIds(std::sync::Mutex<Option<String>>);

impl DocumentTransform for SeesIds {
    fn transform(&self, view: Tree) -> Result<Tree, Error> {
        *self.0.lock().unwrap() = Some(api::encode_tree(&view));
        Ok(view)
    }
}

#[test]
fn c2_x_053_a_transform_receives_the_resolved_view() {
    let seen = std::sync::Arc::new(SeesIds(std::sync::Mutex::new(None)));
    struct Shared(std::sync::Arc<SeesIds>);
    impl DocumentTransform for Shared {
        fn transform(&self, view: Tree) -> Result<Tree, Error> {
            self.0.transform(view)
        }
    }
    let mut carve = Carve::new();
    carve
        .register_extension(Extension {
            declaration: declaration("ex.look"),
            renderers: Vec::new(),
            transform: Some(Box::new(Shared(seen.clone()))),
        })
        .unwrap();
    let tree = api::parse("# Title", &Options::default()).unwrap();
    let bare: Value = serde_json::from_str(&api::encode_tree(&tree)).unwrap();
    let mut stripped = bare.clone();
    stripped["children"][0]
        .as_object_mut()
        .unwrap()
        .remove("generatedId");
    let supplied = api::decode_tree(&stripped.to_string()).unwrap();
    carve.render_tree(&supplied, &Options::default()).unwrap();
    let view = seen.0.lock().unwrap().clone().unwrap();
    assert!(view.contains("\"generatedId\":\"Title\""), "{view}");
}
