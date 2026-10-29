use carve::to_html;
use serde_json::Value;

fn fixtures() -> Value {
    serde_json::from_str(include_str!("fixtures/inline_fence_paragraph_resume.json")).unwrap()
}

fn normalized(html: &str) -> String {
    regex::Regex::new(r">\s+<")
        .unwrap()
        .replace_all(html.trim(), "><")
        .into_owned()
}

#[test]
#[ignore = "rule A ledger: carve-rs#3523"]
fn closed_code_does_not_resume_a_rejected_inline_fence_paragraph() {
    for row in fixtures()["ownership"].as_array().unwrap() {
        let html = to_html(row["source"].as_str().unwrap());
        assert_eq!(
            normalized(&html),
            normalized(row["html"].as_str().unwrap()),
            "{}: {html}",
            row["id"]
        );
    }
}

#[test]
#[ignore = "rule A ledger: carve-rs#3525"]
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
