use carve::api::json::call;
use serde_json::{Value, json};

fn imported(source: &str) -> String {
    let out: Value = serde_json::from_str(&call(
        &json!({"call":"importDocument","from":"markdown","source":source}).to_string(),
    ))
    .unwrap();
    out["ok"]["source"].as_str().unwrap().to_string()
}

#[test]
fn unclosed_math_ends_at_its_container_boundary() {
    for (input, expected) in [
        ("> $$\n> a\n>\n\nx\n", "> $$`a`\n\nx\n"),
        ("$$\na\n\n\n", "$$`a\n\n`\n"),
        ("- $$\n  a\n  \n", "- $$`a\n  `\n"),
        ("- $$\n  a\n  \n  \n", "- $$`a\n\n  `\n"),
    ] {
        assert_eq!(imported(input), expected, "{input:?}");
    }
}

#[test]
fn unclosed_math_quote_family_drops_its_boundary_blank() {
    let mut checked = 0;
    for first in b'a'..=b'p' {
        for last in b'a'..=b'p' {
            let word = format!("{}{}", char::from(first), char::from(last));
            for blanks in 1..=4 {
                let input = format!("> $$\n> {word}\n>\n{}x\n", "\n".repeat(blanks));
                let output = imported(&input);
                assert!(
                    output.starts_with(&format!("> $$`{word}`\n")),
                    "{input:?}: {output:?}"
                );
                checked += 1;
            }
        }
    }
    assert_eq!(checked, 1024);
}
