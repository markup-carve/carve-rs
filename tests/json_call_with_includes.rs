use carve::api::json::{call, call_with_includes};
use carve::include::{Cause, Includes, Request};
use serde_json::Value;
use std::sync::Arc;

fn parts() -> Includes {
    Includes::new(Arc::new(|request: &Request<'_>| match request.path {
        "part.crv" => Ok((b"Included *text*.".to_vec(), "part.crv".to_string())),
        "loop.crv" => Ok((b"{{ loop.crv }}".to_vec(), "loop.crv".to_string())),
        _ => Err(Cause::NotFound),
    }))
}

fn ok(response: &str) -> Value {
    let value: Value = serde_json::from_str(response).unwrap();
    assert!(value.get("error").is_none(), "{response}");
    value["ok"].clone()
}

#[test]
fn a_host_resolver_expands_includes_in_render_and_parse() {
    let html = ok(&call_with_includes(
        r#"{"call":"render","source":"{{ part.crv }}\n"}"#,
        parts(),
    ));
    assert_eq!(html["html"], "<p>Included <strong>text</strong>.</p>");

    let tree = ok(&call_with_includes(
        r#"{"call":"parse","source":"{{ part.crv }}\n"}"#,
        parts(),
    ));
    assert_eq!(tree["tree"]["children"][0]["type"], "include");
    assert_eq!(tree["tree"]["children"][0]["file"], "part.crv");
}

#[test]
fn without_a_resolver_the_directive_is_not_expanded() {
    let html = ok(&call(r#"{"call":"render","source":"{{ part.crv }}\n"}"#));
    assert_ne!(html["html"], "<p>Included <strong>text</strong>.</p>");
}

#[test]
fn resolver_causes_and_cycles_become_diagnostics() {
    let missing = ok(&call_with_includes(
        r#"{"call":"renderWithDiagnostics","source":"{{ gone.crv }}\n"}"#,
        parts(),
    ));
    assert!(
        missing["diagnostics"]
            .to_string()
            .contains("include-not-found"),
        "{missing}"
    );

    let cycle = ok(&call_with_includes(
        r#"{"call":"renderWithDiagnostics","source":"{{ loop.crv }}\n"}"#,
        parts(),
    ));
    assert!(
        cycle["diagnostics"]
            .as_array()
            .is_some_and(|d| !d.is_empty())
    );
}

#[test]
fn includes_in_the_request_and_from_the_host_are_refused_together() {
    let response = call_with_includes(
        r#"{"call":"render","source":"x","options":{"includes":null}}"#,
        parts(),
    );
    let value: Value = serde_json::from_str(&response).unwrap();
    assert_eq!(value["error"]["path"], "/includes", "{response}");
}
