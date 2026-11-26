//! carve#6452, C2-B-197: the block image line pairs brackets as the inline
//! scan does around `[^` and `^[` (C2-I-087, C2-I-089) and nested tails.

use serde_json::Value;

fn html(src: &str) -> String {
    carve::render(src, &carve::Options::default())
}

#[test]
fn block_image_line_pairs_brackets_like_the_inline_scan() {
    let cases = [
        ("![a [^](u)\n", "<img src=\"u\" alt=\"a [^\">"),
        (
            "![a ^[f][a](b])](u)\n",
            "<img src=\"u\" alt=\"a ^[f][a](b])\">",
        ),
        ("![aa^[^[x]]](u)\n", "<img src=\"u\" alt=\"aa^[^[x]]\">"),
        ("![a[[^]](u)\n", "<img src=\"u\" alt=\"a[[^]\">"),
        ("![^[f][^](u)\n", "<img src=\"u\" alt=\"^[f][^\">"),
        ("![a [a](b])](u)\n", "<img src=\"u\" alt=\"a [a](b])\">"),
        ("![a[^]](u)\n", "<p>![a[^]](u)</p>"),
        ("![{^a^}](u)\n", "<img src=\"u\" alt=\"{^a^}\">"),
        // A nested label the inline scan rejects leaves the pair as text (C2-I-078, C2-I-079).
        ("![[a][@[]]](u)\n", "<img src=\"u\" alt=\"[a][@[]]\">"),
        ("![[ ][][[x]](u)\n", "<img src=\"u\" alt=\"[ ][][[x]\">"),
        ("![[a][ ][[x]](u)\n", "<img src=\"u\" alt=\"[a][ ][[x]\">"),
    ];
    for (src, want) in cases {
        assert_eq!(html(src), want, "{src:?}");
    }
}

/// The role is `image` exactly where the paragraph's inline content is one image.
#[test]
fn block_image_role_matches_the_inline_parse() {
    let lines = [
        "![^[[^]](u)",
        "![^[a[^]](u)",
        "![^[^[[^]]][k]",
        "![a ^[b [^ ]](u)",
        "![aa[^][k]",
        "![a ^[f [^n] g] h](u)",
        "![[^ ]](u)",
        "![[^[k]]](u)",
        "![a[`c]`](u)",
        "![a[[k]][k]",
        "![a [^n]](u)",
        "![a[<http://x>]](u)",
        "![[a][@[]](u)",
        "![[a][@x]](u)",
        "![[ ][]](u)",
        "![[a][ ](u)",
        "![a](u){}",
        "![a](u){.c}{}",
        "![a][k]{}",
    ];
    for line in lines {
        let src = format!("{line}\n\n[k]: /u\n\n[^n]: x\n");
        let tree: Value =
            serde_json::from_str(&carve::tree_json(&src, &carve::Options::default())).unwrap();
        let para = &tree["children"][0];
        assert_eq!(para["type"], "paragraph", "{line:?}");
        let kids = para["children"].as_array().unwrap();
        let only = kids.len() == 1 && kids[0]["type"] == "image";
        let role = para.get("role").and_then(Value::as_str) == Some("image");
        assert_eq!(only, role, "{line:?}: {para}");
    }
}

/// An empty `{}` behind the line is text, so the paragraph keeps no role.
#[test]
fn empty_attribute_block_takes_the_role_away() {
    for src in ["![a](u){}\n", "$$`x`{}\n"] {
        let tree: Value =
            serde_json::from_str(&carve::tree_json(src, &carve::Options::default())).unwrap();
        assert!(tree["children"][0].get("role").is_none(), "{src:?}");
    }
}
