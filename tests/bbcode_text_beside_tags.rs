//! Text beside a converted formatting tag stays text. The same cases, and the
//! same generated corpus, are pinned in carve-js and carve-php.

use carve::{bbcode_to_carve, parse, to_html, to_json, BbcodeImportError};
use serde_json::Value;

fn carve(bbcode: &str) -> String {
    bbcode_to_carve(bbcode).expect("under the input limit")
}

#[test]
fn text_beside_a_tag_stays_text() {
    for (bbcode, expected) in [
        ("q ~}[s]_ a}[/s] q", "<p>q ~}<s>_ a}</s> q</p>"),
        ("q #[u]x[/u] q", "<p>q #<u>x</u> q</p>"),
        ("q #[/i]x q", "<p>q #x q</p>"),
        ("q @[/b]x q", "<p>q @x q</p>"),
        ("q [B]{} q", "<p>q [B]{} q</p>"),
        ("q =x== q", "<p>q =x== q</p>"),
        ("q a &#8212; b q", "<p>q a &amp;#8212; b q</p>"),
        ("q [u][*x*[u]*** q", "<p>q [u][*x*[u]*** q</p>"),
        ("q [i][/u][/i] q", "<p>q  q</p>"),
        ("q [x[b](y) q", "<p>q [x[b](y) q</p>"),
        (
            "[url=http://x]a [b] b[/url]",
            "<p><a href=\"http://x\">a [b] b</a></p>",
        ),
    ] {
        assert_eq!(to_html(&carve(bbcode)).trim(), expected, "{bbcode}");
    }
}

/// The import's tree, written the way `reference` writes its answer. Smart
/// typography applies to any Carve source and reads as the characters it was
/// written with; any other construct shows up as itself and fails.
fn imported(post: &str) -> String {
    fn write(nodes: &[Value]) -> String {
        nodes
            .iter()
            .map(|n| {
                let kind = n["type"].as_str().unwrap_or("");
                let tag = match kind {
                    "strong" => "strong",
                    "emphasis" => "em",
                    "underline" => "u",
                    "strike" => "s",
                    "text" | "escaped_text" | "smart_punctuation" => {
                        return escape(n["value"].as_str().unwrap_or(""));
                    }
                    other => return format!("<?{other}>"),
                };
                let kids = write(n["children"].as_array().map(Vec::as_slice).unwrap_or(&[]));
                format!("<{tag}>{kids}</{tag}>")
            })
            .collect()
    }
    let tree: Value = serde_json::from_str(&to_json(&parse(&carve(post)))).unwrap();
    tree["children"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| {
            if b["type"] == "paragraph" {
                format!("<p>{}</p>", write(b["children"].as_array().unwrap()))
            } else {
                format!("<?{}>", b["type"])
            }
        })
        .collect()
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// An independent reading of the four formatting tags, straight to HTML: the
/// innermost close must match, an unclosed tag is literal, a stray close tag
/// goes, a tag inside its own kind adds nothing, and an empty one goes.
fn reference(post: &str) -> String {
    enum Piece {
        Text(String),
        Node(usize),
    }
    struct Node {
        kind: char,
        open: String,
        kids: Vec<Piece>,
        unclosed: bool,
    }
    let tag = regex::Regex::new(r"(?i)\[(/?)(b|i|u|s)\]").unwrap();
    let mut nodes = vec![Node {
        kind: ' ',
        open: String::new(),
        kids: Vec::new(),
        unclosed: false,
    }];
    let mut stack = vec![0];
    let mut from = 0;
    for caps in tag.captures_iter(post) {
        let whole = caps.get(0).unwrap();
        let top = *stack.last().unwrap();
        nodes[top]
            .kids
            .push(Piece::Text(post[from..whole.start()].to_string()));
        from = whole.end();
        let kind = caps[2].to_ascii_lowercase().chars().next().unwrap();
        if caps[1].is_empty() {
            nodes.push(Node {
                kind,
                open: whole.as_str().to_string(),
                kids: Vec::new(),
                unclosed: false,
            });
            let id = nodes.len() - 1;
            nodes[top].kids.push(Piece::Node(id));
            stack.push(id);
        } else if nodes[top].kind == kind {
            stack.pop();
        }
    }
    let top = *stack.last().unwrap();
    nodes[top].kids.push(Piece::Text(post[from..].to_string()));
    for &id in &stack[1..] {
        nodes[id].unclosed = true;
    }
    fn out(nodes: &[Node], kids: &[Piece], open: &[char]) -> String {
        kids.iter()
            .map(|k| match k {
                Piece::Text(t) => escape(t),
                Piece::Node(id) => {
                    let n = &nodes[*id];
                    if n.unclosed {
                        escape(&n.open) + &out(nodes, &n.kids, open)
                    } else if open.contains(&n.kind) {
                        out(nodes, &n.kids, open)
                    } else {
                        let mut inner_open = open.to_vec();
                        inner_open.push(n.kind);
                        let inner = out(nodes, &n.kids, &inner_open);
                        let tag = match n.kind {
                            'b' => "strong",
                            'i' => "em",
                            'u' => "u",
                            _ => "s",
                        };
                        if inner.is_empty() {
                            String::new()
                        } else {
                            format!("<{tag}>{inner}</{tag}>")
                        }
                    }
                }
            })
            .collect()
    }
    format!("<p>{}</p>", out(&nodes, &nodes[0].kids, &[]))
}

#[test]
fn a_generated_post_renders_exactly_as_its_tags_say() {
    let atoms = [
        "[b]", "[/b]", "[i]", "[/i]", "[u]", "[/u]", "[s]", "[/s]", "x", "ab", " ", "_", "*", "/",
        "~", "=", "{", "}", "#", "@", ":", "\\", "`", "$", "^", "+", "-", "<", ">", "[", "]", "(",
        ")", "!", "%", "|",
    ];
    let mut seed: u32 = 1;
    let mut rnd = |n: usize| {
        seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12_345) & 0x7fff_ffff;
        seed as usize % n
    };
    let mut wrong = Vec::new();
    for _ in 0..3000 {
        let mut post = String::from("q ");
        for _ in 0..1 + rnd(12) {
            post.push_str(atoms[rnd(atoms.len())]);
        }
        post.push_str(" q");
        if imported(&post) != reference(&post) {
            wrong.push(post);
        }
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
}

#[test]
fn a_post_that_leaves_the_link_mark_no_code_point_is_refused() {
    // Two code points are left free, and the literal-run stash takes them for
    // the code tag, so only the link mark has nowhere to go.
    let most: String = (0xe000..=0xf8ffu32)
        .filter(|&c| c != 0xe010 && c != 0xe011)
        .filter_map(char::from_u32)
        .collect();
    assert!(matches!(
        bbcode_to_carve(&format!("{most} [code]x[/code] [url=http://x]y[/url]")),
        Err(BbcodeImportError::SentinelSpaceExhausted)
    ));
    assert!(bbcode_to_carve(&format!("{most} [code]x[/code]")).is_ok());
}
