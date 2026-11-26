//! Differential run of the inline pass against the oracle on generated
//! paragraphs. Ignored by default: it needs node and the TypeScript engine.
//!
//! `CARVE_JS=/path/to/carve-js INLINE_DIFF_N=3000 INLINE_DIFF_SEED=1 \
//!  cargo test -p carve --test inline_diff -- --ignored --nocapture`
mod common;
#[path = "support/inline.rs"]
mod support;

use serde_json::Value;
use std::fmt::Write as _;

const TOKENS: &[&str] = &[
    "*",
    "*",
    "/",
    "/",
    "{*",
    "*}",
    "{/",
    "/}",
    "{=",
    "=}",
    "{-",
    "-}",
    "{+",
    "+}",
    "{_",
    "_}",
    "[",
    "[",
    "]",
    "](",
    "](/u)",
    "(",
    ")",
    "`",
    "`",
    "``",
    "$",
    "$`",
    "!",
    "{.c}",
    "{#i}",
    "{}",
    "%%",
    " %% ",
    "{%",
    "%}",
    "{#",
    "#}",
    "\"",
    "\"",
    "'",
    "'",
    "-",
    "--",
    "---",
    "<",
    ">",
    "<--",
    "-->",
    "...",
    "@x",
    "#t",
    ":s:",
    ":e[",
    "^[",
    "[^n]",
    "[a]",
    "][a]",
    "][]",
    "\\",
    "\\*",
    "\\ ",
    "a",
    "b",
    "word",
    "it's",
    "HT",
    "x",
    "_",
    "é",
    "1",
    " ",
    " ",
    " ",
    " ",
    "\n",
    "\n",
    "(tm)",
    "!=",
    "<=",
    "+-",
    "<http://a.b>",
    "</#h>",
    "'n'",
    "'90s",
    "'em",
    "~",
    "\\\n",
    "\\`",
    "\\[",
    "[x](/u \"t\")",
    "![i](/p)",
    "![x]",
    "*a*",
    "/b/",
    "{^",
    "^}",
    "{~",
    "~}",
    "&",
    "ö",
    "{:en}",
    "{k=v}",
];

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: usize) -> usize {
        usize::try_from(self.next() % (n as u64)).unwrap_or(0)
    }
}

fn mix(rng: &mut Rng, len: usize, cell: bool) -> String {
    let mut s = String::new();
    for _ in 0..len {
        let t = TOKENS.get(rng.below(TOKENS.len())).copied().unwrap_or("a");
        if !(cell && t.contains('\n')) {
            s.push_str(t);
        }
    }
    s
}

fn generate(rng: &mut Rng) -> String {
    let len = 2 + rng.below(14);
    let mut s = match rng.below(7) {
        0 => format!("# a {}", mix(rng, len, false)),
        1 => format!("| a {} | {} |", mix(rng, len, true), mix(rng, 3, true)),
        _ => format!("a {}", mix(rng, len, false)),
    };
    match rng.below(4) {
        0 => s.push_str("\n\n[a]: /u\n*[HT]: Hyper"),
        1 => s.push_str("\n\n[x]: /v"),
        _ => {}
    }
    s
}

#[test]
#[ignore = "needs node and the TypeScript engine (CARVE_JS)"]
fn inline_differential() {
    let Ok(js) = std::env::var("CARVE_JS") else {
        return;
    };
    let n: usize = std::env::var("INLINE_DIFF_N")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(2000);
    let seed: u64 = std::env::var("INLINE_DIFF_SEED")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);
    let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
    let inputs: Vec<String> = (0..n).map(|_| generate(&mut rng)).collect();
    let cases: Vec<Value> = inputs
        .iter()
        .map(|i| serde_json::json!({"input": i, "opts": {}}))
        .collect();
    let dir = std::env::temp_dir().join(format!("carve-inline-diff-{seed}"));
    let _ = std::fs::create_dir_all(&dir);
    let in_path = dir.join("in.json");
    let out_path = dir.join("out.json");
    let _ = std::fs::write(&in_path, serde_json::to_string(&cases).unwrap_or_default());
    let tool = concat!(env!("CARGO_MANIFEST_DIR"), "/tools/oracle.mjs");
    let status = std::process::Command::new("node")
        .arg(tool)
        .arg(&js)
        .arg(&in_path)
        .arg(&out_path)
        .status();
    assert!(status.is_ok_and(|s| s.success()), "oracle run failed");
    let out: Vec<Value> =
        serde_json::from_str(&std::fs::read_to_string(&out_path).unwrap_or_default())
            .unwrap_or_default();
    let mut compared = 0;
    let mut diffs: Vec<(String, String, String)> = Vec::new();
    for (input, rec) in inputs.iter().zip(&out) {
        let Some(tree) = rec
            .get("tree")
            .and_then(Value::as_str)
            .and_then(|t| serde_json::from_str::<Value>(t).ok())
        else {
            continue;
        };
        for (want, got) in support::compare(input, &tree, common::sorted) {
            compared += 1;
            if want != got {
                diffs.push((input.clone(), want, got));
            }
        }
    }
    diffs.sort_by_key(|d| d.0.len());
    let mut report = format!("compared {compared}, differ {}\n", diffs.len());
    for (input, want, got) in &diffs {
        let _ = write!(report, "=== {input:?}\n  want {want}\n  got  {got}\n");
    }
    let _ = std::fs::write(dir.join("report.txt"), &report);
    println!(
        "{}report: {}",
        report.lines().next().unwrap_or(""),
        dir.join("report.txt").display()
    );
}
