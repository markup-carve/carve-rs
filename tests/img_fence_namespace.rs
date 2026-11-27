//! C2-X-059, CC-2275: safe `xmlns:xlink` declarations survive SVG sanitization.

use serde_json::{Value, json};
use std::fmt::Write as _;

fn html(svg: &str) -> String {
    let request = json!({
        "call": "renderWithDiagnostics",
        "source": format!("```img\n{svg}\n```\n"),
        "options": {
            "extensions": ["carve.imgFence.img"],
            "renderers": {"carve.imgFence.img": "standard"},
        },
    });
    let response: Value =
        serde_json::from_str(&carve::api::json::call(&request.to_string())).unwrap();
    response["ok"]["html"].as_str().unwrap().to_string()
}

fn encoded(svg: &str) -> String {
    let mut out = String::new();
    for byte in svg.bytes() {
        if byte.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&byte) {
            out.push(char::from(byte));
        } else {
            let _ = write!(out, "%{byte:02X}");
        }
    }
    format!("<img src=\"data:image/svg+xml,{out}\" alt=\"\">")
}

#[test]
fn cc_2275_keeps_a_namespace_declaration_without_whitespace() {
    assert_eq!(
        html("<svg><g xmlns:xlink=\"urn:x\"/></svg>"),
        encoded("<svg xmlns=\"http://www.w3.org/2000/svg\"><g xmlns:xlink=\"urn:x\"/></svg>")
    );
}

#[test]
fn c2_x_059_drops_a_namespace_declaration_with_whitespace() {
    for value in ["urn: x", "urn:\u{a0}x", "urn:&#160;x"] {
        assert_eq!(
            html(&format!("<svg><g xmlns:xlink=\"{value}\"/></svg>")),
            encoded("<svg xmlns=\"http://www.w3.org/2000/svg\"><g/></svg>"),
            "{value}"
        );
    }
}
