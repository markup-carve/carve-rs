//! Markdown and GFM import of line endings (carve-rs#4640), math flow
//! (carve-rs#4639) and a raw block after text in a tight item. Expected
//! sources are those carve-js `next2` writes.

use carve::api::json::call;
use serde_json::{Value, json};

fn import(from: &str, source: &str) -> (String, Value) {
    let out: Value = serde_json::from_str(&call(
        &json!({"call": "importDocument", "from": from, "source": source}).to_string(),
    ))
    .unwrap();
    let ok = out
        .get("ok")
        .cloned()
        .unwrap_or_else(|| panic!("{from} {source:?}: {out}"));
    (
        ok["source"].as_str().unwrap().to_string(),
        ok["diagnostics"].clone(),
    )
}

fn carve(source: &str) -> String {
    let (markdown, _) = import("markdown", source);
    let (gfm, _) = import("gfm", source);
    assert_eq!(markdown, gfm, "{source:?}");
    markdown
}

#[test]
fn a_cr_line_ending_is_a_line_ending_in_a_raw_block() {
    for source in [
        "x\r<div>x</div>\r",
        "x\r\n<div>x</div>\r\n",
        "x\n<div>x</div>\n",
    ] {
        let (written, diagnostics) = import("markdown", source);
        assert_eq!(written, "x\n\n```=html\n<div>x</div>\n```\n", "{source:?}");
        assert_eq!(diagnostics, json!([]), "{source:?}");
    }
    assert_eq!(carve("```\rcode\r```\r"), carve("```\ncode\n```\n"),);
}

#[test]
fn an_unclosed_math_fence_runs_to_the_end_of_its_container() {
    let cases = [
        ("$$\n# H\n", "$$`# H`\n"),
        ("$$\na\n", "$$`a`\n"),
        ("> $$\n> a\n\nx\n", "> $$`a`\n\nx\n"),
        ("- $$\n  a\n- y\n", "- $$`a`\n\n- y\n"),
    ];
    for (source, expected) in cases {
        assert_eq!(carve(source), expected, "{source:?}");
    }
}

#[test]
fn a_math_fence_pairs_as_a_code_fence_does() {
    let cases = [
        ("x\n$$\na\n$$\n", "x\n\n$$`a`\n"),
        ("$$ meta\na\n$$\n", "$$`a`\n"),
        ("$$\na\n$$$\n", "$$`a`\n"),
        ("$$$\na\n$$\nb\n", "$$`a\n$$\nb`\n"),
        ("  $$\n  a\n   b\n  $$\n", "$$`a\n b`\n"),
        ("$$\na\n$$ x\nb\n", "$$`a\n$$ x\nb`\n"),
        ("$$\n\n$$\n", "$$``\n"),
        ("> $$\n> a\n>\n> b\n> $$\n", "> $$`a\n>\n> b`\n"),
        ("$$\na\n$$\n$$\nb\n$$\n", "$$`a`\n\n$$`b`\n"),
        ("- x\n  $$\n  a\n  $$\n- y\n", "- x\n\n  $$`a`\n\n- y\n"),
    ];
    for (source, expected) in cases {
        assert_eq!(carve(source), expected, "{source:?}");
    }
}

#[test]
fn dollars_in_code_raw_html_and_text_stay_as_they_are() {
    let cases = [
        ("```\n$$\n```\n", "```\n$$\n```\n"),
        ("<div>\n$$\n</div>\n", "```=html\n<div>\n$$\n</div>\n```\n"),
        ("a $$\nb\n", "a $$\nb\n"),
        ("    $$\n", "```\n$$\n```\n"),
        ("a\n    $$\n", "a\n$$\n"),
    ];
    for (source, expected) in cases {
        assert_eq!(carve(source), expected, "{source:?}");
    }
}

#[test]
fn double_dollars_in_text_are_inline_math() {
    assert_eq!(carve("$$x$$\n"), "$`x`\n");
    assert_eq!(carve("a $$x$$ b\n"), "a $`x` b\n");
}

#[test]
fn a_raw_block_after_text_in_a_tight_item_is_inline_raw() {
    assert_eq!(
        carve("- x\n  <div>\n  a\n  </div>\n- y\n"),
        "- x\n  `<div>\n  a\n  </div>`{=html}\n- y\n"
    );
}

#[test]
fn a_comment_of_the_input_is_never_taken_for_a_raw_placeholder() {
    let (written, _) = import("markdown", "x <!--carve-hub-raw-0-->\n\n<div>y</div>\n");
    assert_eq!(
        written,
        "x `<!--carve-hub-raw-0-->`{=html}\n\n```=html\n<div>y</div>\n```\n"
    );
}

#[test]
fn a_long_backtick_fence_and_many_math_fences_stay_linear() {
    let source = format!(
        "{}\n\n{}",
        "`".repeat(20_000),
        "$$\na\n$$\n\n".repeat(5_000)
    );
    let (written, _) = import("markdown", &source);
    assert!(written.len() < source.len() * 2, "{}", written.len());
    assert!(
        !written.contains("`````"),
        "a rewritten fence reached the output"
    );
}

#[test]
fn a_dollar_line_in_code_does_not_change_a_later_math_fence() {
    assert_eq!(carve("```\n$$\n```\n$$\na\n"), "```\n$$\n```\n\n$$`a`\n");
    assert_eq!(
        carve("<div>\n$$\n</div>\n\n$$\na\n$$\n"),
        "```=html\n<div>\n$$\n</div>\n```\n\n$$`a`\n"
    );
}

#[test]
fn inline_math_strips_one_padding_character_from_each_side() {
    assert_eq!(carve("x $$\nx $$\n"), "x $`x`\n");
    assert_eq!(carve("a $$ b\nc $$ d\n"), "a $`b\nc` d\n");
}

#[test]
fn a_fence_line_inside_a_math_body_does_not_hide_its_closer() {
    assert_eq!(carve("$$\n```\n$$\nz\n"), "$$` ``` `\n\nz\n");
    assert_eq!(carve("$$\n<div>\n$$\nz\n"), "$$`<div>`\n\nz\n");
    assert_eq!(carve("> $$\n> ```\n> $$\nz\n"), "> $$` ``` `\n\nz\n");
}

#[test]
fn a_dollar_line_that_cannot_close_stays_math_content() {
    assert_eq!(carve("$$\n- $$\nx\n"), "$$`- $$\nx`\n");
    assert_eq!(carve("$$\n    $$\nx\n$$\n"), "$$`    $$\nx`\n");
}
