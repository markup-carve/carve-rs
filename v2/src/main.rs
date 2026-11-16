//! `carve2`: reads Carve source on stdin, writes HTML, or the tree with `--tree`.

use std::io::{Read, Write};

fn main() {
    let mut opts = carve2::Options::default();
    let Some(tree) = parse_args(&mut opts) else {
        return;
    };
    let mut bytes = Vec::new();
    if std::io::stdin().read_to_end(&mut bytes).is_err() {
        std::process::exit(1);
    }
    let out = if tree {
        match carve2::try_parse_resolved_bytes_in(&bytes, &opts, &carve2::Arena::new()) {
            Ok(doc) => carve2::to_json(&doc),
            Err(e) => {
                eprintln!("carve2: {e}");
                std::process::exit(1);
            }
        }
    } else {
        match carve2::try_render_bytes(&bytes, &opts) {
            Ok(html) => html,
            Err(e) => {
                eprintln!("carve2: {e}");
                std::process::exit(1);
            }
        }
    };
    let mut stdout = std::io::stdout();
    let _ = stdout.write_all(out.as_bytes());
    let _ = stdout.write_all(b"\n");
}

fn parse_args(opts: &mut carve2::Options) -> Option<bool> {
    let mut tree = false;
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--tree" => tree = true,
            "--batch" => {
                batch(it.next(), it.next());
                return None;
            }
            "--roundtrip" => opts.roundtrip = true,
            "--raw-html" => opts.raw_html = true,
            "--no-sections" => opts.sections = false,
            "--no-smart-typography" => opts.smart_typography = false,
            "--source-lines" => opts.source_lines = true,
            "--strict-losses" => opts.strict_losses = true,
            "--mention-url" | "--tag-url" | "--heading-ids" | "--profile" => {
                option_value(opts, a, it.next());
            }
            _ => {
                eprintln!(
                    "{}",
                    concat!(
                        "usage: carve2 [--tree] [--roundtrip] [--raw-html] [--no-sections] ",
                        "[--no-smart-typography] [--source-lines] [--strict-losses] ",
                        "[--mention-url URL] [--tag-url URL] [--heading-ids off|keep-case|lower|ascii] ",
                        "[--profile full|article|comment|minimal] < input"
                    )
                );
                std::process::exit(2);
            }
        }
    }
    Some(tree)
}

fn option_value(opts: &mut carve2::Options, name: &str, value: Option<&String>) {
    match name {
        "--mention-url" | "--tag-url" => {
            let social = opts.social.get_or_insert_with(Default::default);
            let v = value.cloned();
            if name == "--mention-url" {
                social.mention_url = v;
            } else {
                social.tag_url = v;
            }
        }
        "--heading-ids" => {
            if let Some(v) = value.and_then(|v| carve2::options::HeadingIds::from_name(v)) {
                opts.heading_ids = v;
            }
        }
        "--profile" => {
            if let Some(v) = value.and_then(|v| carve2::options::Profile::from_name(v)) {
                opts.profile = v;
            }
        }
        _ => {}
    }
}

/// `--batch IN OUT`: IN is a JSON list of `{"input", "opts"}`, OUT gets a
/// list of `{"html", "tree", "ns"}`, for the differential runs.
fn batch(input: Option<&String>, output: Option<&String>) {
    let (Some(input), Some(output)) = (input, output) else {
        std::process::exit(2)
    };
    let text = std::fs::read_to_string(input).unwrap_or_default();
    let cases: serde_json::Value = serde_json::from_str(&text).unwrap_or(serde_json::Value::Null);
    let mut out = Vec::new();
    for c in cases.as_array().into_iter().flatten() {
        let src = c
            .get("input")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("");
        let opts = options_from_json(c.get("opts"));
        let t0 = std::time::Instant::now();
        let html = carve2::try_render(src, &opts);
        let ns = t0.elapsed().as_nanos();
        let tree = carve2::parse_resolved_in(src, &opts, &carve2::Arena::new())
            .map_or_else(|e| e.to_string(), |doc| carve2::to_json(&doc));
        let ns = u64::try_from(ns).unwrap_or(u64::MAX);
        out.push(match html {
            Ok(html) => serde_json::json!({ "html": html, "tree": tree, "ns": ns }),
            Err(e) => serde_json::json!({ "htmlError": e.to_string(), "tree": tree, "ns": ns }),
        });
    }
    let _ = std::fs::write(output, serde_json::Value::Array(out).to_string());
}

/// Render options in the oracle's camelCase spelling.
fn options_from_json(v: Option<&serde_json::Value>) -> carve2::Options {
    let mut o = carve2::Options::default();
    let Some(v) = v else { return o };
    let flag = |k: &str, d: bool| v.get(k).and_then(serde_json::Value::as_bool).unwrap_or(d);
    o.roundtrip = flag("roundtrip", o.roundtrip);
    o.raw_html = flag("rawHtml", o.raw_html);
    o.sections = flag("sections", o.sections);
    o.smart_typography = flag("smartTypography", o.smart_typography);
    o.source_lines = flag("sourceLines", o.source_lines);
    o.strict_losses = flag("strictLosses", o.strict_losses);
    if let Some(s) = v.get("social") {
        let url = |k: &str| {
            s.get(k)
                .and_then(serde_json::Value::as_str)
                .map(String::from)
        };
        o.social = Some(carve2::options::Social {
            mention_url: url("mentionUrl"),
            tag_url: url("tagUrl"),
        });
    }
    if let Some(h) = v
        .get("headingIds")
        .and_then(serde_json::Value::as_str)
        .and_then(carve2::options::HeadingIds::from_name)
    {
        o.heading_ids = h;
    }
    if let Some(p) = v
        .get("profile")
        .and_then(serde_json::Value::as_str)
        .and_then(carve2::options::Profile::from_name)
    {
        o.profile = p;
    }
    if v.get("mode").and_then(serde_json::Value::as_str) == Some("static") {
        o.mode = carve2::options::Mode::Static;
    }
    for (key, map) in [("labels", &mut o.labels), ("symbols", &mut o.symbols)] {
        if let Some(m) = v.get(key).and_then(serde_json::Value::as_object) {
            for (k, val) in m {
                if let Some(t) = val.as_str() {
                    map.insert(k.clone(), t.to_string());
                }
            }
        }
    }
    if let Some(p) = v.get("urlPolicy") {
        let list = |k: &str| {
            p.get(k).and_then(serde_json::Value::as_array).map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(String::from))
                    .collect::<Vec<_>>()
            })
        };
        if let Some(a) = list("allow") {
            o.url_policy = carve2::options::UrlPolicy::Allow(a);
        } else if let Some(d) = list("deny") {
            o.url_policy = carve2::options::UrlPolicy::Deny(d);
        }
    }
    o
}
