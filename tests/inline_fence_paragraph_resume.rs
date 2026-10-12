use carve::to_html;
use serde_json::Value;

fn fixtures() -> Value {
    serde_json::from_str(include_str!("fixtures/inline_fence_paragraph_resume.json")).unwrap()
}

fn normalized(html: &str) -> String {
    html.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[test]
fn rejected_inline_fence_keeps_its_item_paragraph_after_later_code() {
    for row in fixtures()["ownership"].as_array().unwrap() {
        let html = to_html(row["source"].as_str().unwrap());
        assert!(
            html.rfind("after").unwrap() < html.rfind("</li>").unwrap(),
            "{}: {html}",
            row["id"]
        );
    }
}

#[test]
fn canonical_following_code_and_ordinary_code_controls_match_native() {
    for row in fixtures()["exact"].as_array().unwrap() {
        assert_eq!(
            normalized(&to_html(row["source"].as_str().unwrap())),
            normalized(row["html"].as_str().unwrap()),
            "{}",
            row["id"]
        );
    }
}
