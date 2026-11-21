//! Times extension-hosted render against core render, 0.2 and 0.1 (carve-rs#4366).
//!
//! `cargo run --release --manifest-path bench/Cargo.toml --bin ext -- [--rounds N] [--profile BUNDLE]`
//!
//! Per size (2, 8, 32, 128 KiB) and bundle: the median wall time of `Carve::render`
//! with the bundle registered, core `render`, and 0.1.14 `render_html_with_options`
//! with the matching 0.1 extension, and the factor hosted / core.
#![allow(clippy::cast_precision_loss)]

use std::hint::black_box;
use std::time::Instant;

use carve::api::{self, Carve, Options, extensions};

const UNIT: &str = "## Section heading\n\nA paragraph with *emphasis*, `code`, a [link](https://example.com) and \"quotes\" in it.\n\n```mermaid\ngraph TD; A-->B\n```\n\n- first item\n- second item\n\n```math\na^2 + b^2 = c^2\n```\n\n::: code-group\n```js [NPM]\nnpm i\n```\n\n```sh\nyarn add\n```\n:::\n\n```js\nlet a = 1 <1>\n```\n<1> First\n\n";

fn doc(size: usize) -> String {
    let mut s = String::with_capacity(size + UNIT.len());
    while s.len() < size {
        s.push_str(UNIT);
    }
    s
}

fn median(rounds: usize, mut f: impl FnMut() -> usize) -> f64 {
    for _ in 0..3 {
        black_box(f());
    }
    let mut t: Vec<f64> = (0..rounds)
        .map(|_| {
            let s = Instant::now();
            black_box(f());
            s.elapsed().as_secs_f64() * 1e3
        })
        .collect();
    t.sort_by(f64::total_cmp);
    t[t.len() / 2]
}

fn host(name: &str) -> Carve {
    let mut c = Carve::new();
    let mut add = |b: extensions::Bundle| c.register(b).unwrap();
    let mermaid = || {
        extensions::fenced_render(extensions::FencedRenderOptions {
            language: vec!["mermaid".into()],
            ..Default::default()
        })
        .unwrap()
    };
    match name {
        "details" => add(extensions::details()),
        "mermaid" => add(mermaid()),
        "math" => add(extensions::math_block(
            extensions::MathBlockOptions::default(),
        )),
        "codeGroup" => add(extensions::code_group(
            extensions::CodeGroupOptions::default(),
        )),
        "callouts" => add(extensions::code_callouts()),
        "fenced4" => {
            add(mermaid());
            add(extensions::math_block(
                extensions::MathBlockOptions::default(),
            ));
            add(extensions::code_group(
                extensions::CodeGroupOptions::default(),
            ));
            add(extensions::code_callouts());
        }
        _ => unreachable!(),
    }
    c
}

fn old_render(src: &str, name: &str) -> usize {
    let details = carve_0_1::Details::new();
    let mermaid = carve_0_1::FencedRender::mermaid();
    let math = carve_0_1::MathBlock::new();
    let group = carve_0_1::CodeGroup::new();
    let callouts = carve_0_1::CodeCallouts::new();
    let mut o = carve_0_1::Options::new();
    o = match name {
        "details" => o.with_extension(&details),
        "mermaid" => o.with_extension(&mermaid),
        "math" => o.with_extension(&math),
        "codeGroup" => o.with_extension(&group),
        "callouts" => o.with_extension(&callouts),
        _ => o
            .with_extension(&mermaid)
            .with_extension(&math)
            .with_extension(&group)
            .with_extension(&callouts),
    };
    let d = carve_0_1::parse_with_options(src, &o);
    carve_0_1::render_html_with_options(&d, &o).map_or(0, |h| h.len())
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let rounds = args
        .iter()
        .position(|a| a == "--rounds")
        .and_then(|i| args.get(i + 1)?.parse().ok())
        .unwrap_or(30);
    let opts = Options::default();
    // `--dump DIR`: writes the hosted render of every spec input under each
    // standard extension to DIR, one file per extension, to diff two builds.
    if let Some(dir) = args
        .iter()
        .position(|a| a == "--dump")
        .and_then(|i| args.get(i + 1))
    {
        dump(std::path::Path::new(dir));
        return;
    }
    // `--profile NAME`: renders 128 KiB with the bundle NAME for ten seconds, for a profiler.
    if let Some(name) = args
        .iter()
        .position(|a| a == "--profile")
        .and_then(|i| args.get(i + 1))
    {
        let carve = host(name);
        let src = doc(128 * 1024);
        if std::env::var_os("EXT_T").is_some() {
            for _ in 0..5 {
                black_box(carve.render(src.as_str(), &opts).map_or(0, |h| h.len()));
            }
            return;
        }
        let start = Instant::now();
        while start.elapsed().as_secs() < 10 {
            black_box(carve.render(src.as_str(), &opts).map_or(0, |h| h.len()));
        }
        return;
    }
    println!("| bundle | size | core ms | hosted ms | factor | 0.1.14 hosted ms | hosted/0.1 |");
    for name in [
        "details",
        "mermaid",
        "math",
        "codeGroup",
        "callouts",
        "fenced4",
    ] {
        let carve = host(name);
        for kib in [2usize, 8, 32, 128] {
            let src = doc(kib * 1024);
            let r = if kib >= 128 { rounds.min(15) } else { rounds };
            let core = median(r, || {
                api::render(src.as_str(), &opts).map_or(0, |h| h.len())
            });
            let hosted = median(r, || {
                carve.render(src.as_str(), &opts).map_or(0, |h| h.len())
            });
            let old = median(r, || old_render(&src, name));
            println!(
                "| {name} | {kib}k | {core:.3} | {hosted:.3} | {:.2} | {old:.3} | {:.2} |",
                hosted / core,
                hosted / old
            );
        }
    }
}

const STANDARD: &[&str] = &[
    "carve.fencedRender.mermaid",
    "carve.glossary",
    "carve.index",
    "carve.references",
    "carve.mathBlock",
    "carve.codeGroup",
    "carve.codeCallouts",
    "carve.details",
    "carve.spoiler",
    "carve.listTable",
    "carve.colorSwatch",
    "carve.tabNormalize",
    "carve.inlineFootnotesToParentheses",
    "carve.semanticSpan",
    "carve.smartQuotes",
    "carve.externalLinks",
    "carve.headingLevelShift",
    "carve.headingPermalinks",
    "carve.headingNumbers",
    "carve.headingReference",
    "carve.toc",
    "carve.wiki",
    "carve.tabs",
];

fn inputs() -> Vec<String> {
    // One JSON string per line: the spec inputs, which nest past serde_json's
    // recursion limit as whole files.
    let path = std::env::var("EXT_INPUTS").expect("EXT_INPUTS names the inputs file");
    let mut out: Vec<String> = std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    let ext: Vec<String> = out[out.len() - 94..].to_vec();
    // Neighbors: each extension case after a heading and before a list, and pairs.
    for (i, a) in ext.iter().enumerate() {
        out.push(format!("# Top\n\n{a}\n- item [^n]\n\n[^n]: note\n"));
        out.push(format!("> {}\n", a.replace('\n', "\n> ")));
        out.push(format!("- {}\n", a.replace('\n', "\n  ")));
        out.push(format!("{a}\n{}", ext[(i + 1) % ext.len()]));
    }
    out.push(doc(8 * 1024));
    out
}

fn dump(dir: &std::path::Path) {
    std::fs::create_dir_all(dir).unwrap();
    let inputs = inputs();
    let opts = Options::default();
    let mut configs: Vec<(String, Carve)> = STANDARD
        .iter()
        .map(|name| {
            let mut c = Carve::new();
            let _ = c.enable_standard(name, true);
            ((*name).to_string(), c)
        })
        .collect();
    let mut all = Carve::new();
    for name in STANDARD {
        let _ = all.enable_standard(name, true);
    }
    configs.push(("all".into(), all));
    configs.push(("fenced4".into(), host("fenced4")));
    for (name, carve) in &configs {
        let mut text = String::new();
        for (i, input) in inputs.iter().enumerate() {
            let rendered = carve.render_with_diagnostics(input.as_str(), &opts);
            let strict = carve.render(input.as_str(), &opts);
            let line = match rendered {
                Ok(r) => serde_json::json!([
                    i,
                    r.html,
                    serde_json::to_value(&r.diagnostics).unwrap(),
                    r.suppressed_diagnostics,
                    strict.map_err(|e| e.to_string())
                ]),
                Err(e) => serde_json::json!([i, e.to_string()]),
            };
            text.push_str(&line.to_string());
            text.push('\n');
        }
        std::fs::write(dir.join(format!("{name}.jsonl")), text).unwrap();
    }
    println!("{} inputs, {} configs", inputs.len(), configs.len());
}
