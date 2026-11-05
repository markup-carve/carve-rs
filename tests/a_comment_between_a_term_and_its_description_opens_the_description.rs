//! carve#5167 (ruled option 1, CARVE-P12-027): a comment between a definition
//! term and its description is the first block of that description, and of
//! the description after the last term when more terms follow.

use serde_json::Value;

fn ast(source: &str) -> Value {
    serde_json::from_str(&carve::to_json(&carve::parse_with_options(
        source,
        &carve::Options::new().with_positions(true),
    )))
    .unwrap()
}

/// Each comment as (parent type, content).
fn comments(source: &str) -> Vec<(String, String)> {
    fn walk(node: &Value, owner: &str, found: &mut Vec<(String, String)>) {
        match node {
            Value::Object(fields) => {
                let kind = fields.get("type").and_then(Value::as_str).unwrap_or(owner);
                if kind == "comment" {
                    found.push((owner.to_owned(), fields["content"].as_str().unwrap().to_owned()));
                }
                for (key, value) in fields {
                    if key != "pos" {
                        walk(value, kind, found);
                    }
                }
            }
            Value::Array(values) => values.iter().for_each(|v| walk(v, owner, found)),
            _ => {}
        }
    }
    let mut found = Vec::new();
    walk(&ast(source), "document", &mut found);
    found
}

fn in_description(content: &str) -> (String, String) {
    ("definition_description".to_owned(), content.to_owned())
}

#[test]
fn every_comment_form_and_term_count_lands_in_the_description() {
    for (source, content) in [
        (":: t\n  %%%\n      a\n  %%%\n:  body\n", "      a"),
        (":: t\n%%%\na\n%%%\n: body\n", "a"),
        (":: t\n%%%\na\n%%%\n:: u\n: body\n", "a"),
        (":: t\n%% c\n: body\n", "c"),
        (":: t\n%% c\n:: u\n: body\n", "c"),
        ("> :: t\n> %% c\n> : body\n", "c"),
        ("- i\n  :: t\n  %%%\n  a\n  %%%\n  : body\n", "a"),
        ("::: note\n:: t\n%% c\n: body\n:::\n", "c"),
    ] {
        assert_eq!(comments(source), [in_description(content)], "{source:?}");
    }
}

#[test]
fn the_description_starts_at_the_comment_unless_a_term_stands_between() {
    let doc = ast(":: t\n%% c\n: body\n");
    let description = &doc["children"][0]["items"][1];
    assert_eq!(description["pos"]["startOffset"], 5);
    assert_eq!(doc["children"][0]["pos"]["endOffset"], 16);

    let doc = ast(":: t\n%% c\n:: u\n: body\n");
    let list = &doc["children"][0];
    assert!(list["items"][2].get("pos").is_none());
    assert_eq!(list["pos"]["endOffset"], 21);
}

#[test]
fn comment_lines_after_the_first_still_open_the_description() {
    assert_eq!(
        comments(":: t\n%% c\n%% d\n: body\n"),
        [in_description("c"), in_description("d")]
    );
    let doc = ast(":: t\n%% c\n%% d\n: body\n");
    assert_eq!(doc["children"][0]["items"][1]["pos"]["startOffset"], 5);
}

#[test]
fn a_list_in_the_description_is_measured_from_its_marker_line() {
    let doc = ast(":: t\n%% c\n: d\n  - x\n");
    assert_eq!(doc["children"][0]["items"][1]["children"][2]["pos"]["startOffset"], 16);
    let doc = ast("> :: t0\n> %% c\n> :: t1\n> : d\n>   - x\n");
    let list = &doc["children"][0]["children"][0]["items"][2]["children"][2];
    assert_eq!(list["pos"]["startOffset"], 33);
}

#[test]
fn fmt_writes_the_comment_on_the_description_line() {
    for (source, written) in [
        (":: t\n%% c\n:: u\n: body\n", ":: t\n:: u\n: %% c\n\n  body\n"),
        (
            "- i\n  :: t\n  %%%\n  a\n  %%%\n  : body\n",
            "- i\n  :: t\n  : %%%\n    a\n    %%%\n\n    body\n",
        ),
    ] {
        let formatted = carve::to_carve(source);
        assert_eq!(formatted, written, "{source:?}");
        assert_eq!(carve::to_carve(&formatted), formatted, "{source:?}");
        assert_eq!(carve::to_html(&formatted), carve::to_html(source), "{source:?}");
    }
}

/// An item collector reading these took the description's later blocks out
/// of it; the spec model keeps them in.
#[test]
fn a_description_in_an_item_keeps_its_blocks_past_an_opener_or_a_closed_span() {
    for (source, html) in [
        (
            "- i\n  :: t\n  : c\n    %%%\n    %%%\n    - x\n",
            "<ul>\n  <li>i\n    <dl>\n      <dt>t</dt>\n      <dd>\n        <p>c</p>\n        <ul>\n          <li>x</li>\n        </ul>\n      </dd>\n    </dl>\n  </li>\n</ul>",
        ),
        (
            "- i\n  :: t\n  : c\n    # h\n    - x\n",
            "<ul>\n  <li>i\n    <dl>\n      <dt>t</dt>\n      <dd>\n        <p>c</p>\n        <h1 id=\"h\">h</h1>\n        <ul>\n          <li>x</li>\n        </ul>\n      </dd>\n    </dl>\n  </li>\n</ul>",
        ),
        (
            "- i\n  :: t\n  : %%%\n    a\n    %%%\n\n    d\n",
            "<ul>\n  <li>i\n    <dl>\n      <dt>t</dt>\n      <dd>d</dd>\n    </dl>\n  </li>\n</ul>",
        ),
        (
            "- x\n  - %%%\n    a\n    %%%\n\n    d\n",
            "<ul>\n  <li>x\n    <ul>\n      <li>d</li>\n    </ul>\n  </li>\n</ul>",
        ),
    ] {
        assert_eq!(carve::to_html(source), html, "{source:?}");
    }
}
