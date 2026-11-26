use carve::api::{Carve, Options, WriteOptions};
use serde_json::Value;

// C2-W-088, C2-I-047: the backslash of a hard break lets the delimiter before it open, so it takes its kind's escape.
const CASES: &[(&str, &str, &str)] = &[
    ("slash", "x(/u)\\/\\\n", "x(\\/u)\\/\\\n"),
    ("star", "x(*u)\\*\\\nfoo", "x(\\*u)\\*\\\nfoo\n"),
    (
        "slash in a list in a quote",
        "> - x(/u)\\/\\\n>   y",
        "> - x(\\/u)\\/\\\n>   y\n",
    ),
    (
        "star in the class form of hard breaks",
        "{.hardbreaks}\n:::\nx(*u)\\*\\\nfoo\n:::\n",
        "::: hardbreaks\nx(\\*u)\\*\\\nfoo\n:::\n",
    ),
    // C2-W-080: below the opener the break is the bare line ending, so the slash before it cannot open.
    (
        "slash below a hard-break opener",
        "::: \\\nx(/u)\\/\nfoo\n:::\n",
        "::: \\\nx(\\/u)/\nfoo\n:::\n",
    ),
];

fn strip(v: &mut Value) {
    match v {
        Value::Object(map) => {
            map.remove("pos");
            map.remove("srcByteLength");
            map.values_mut().for_each(strip);
        }
        Value::Array(items) => items.iter_mut().for_each(strip),
        _ => {}
    }
}

fn write(carve: &Carve, input: &str) -> (String, Value) {
    let tree = carve.parse(input, &Options::default()).unwrap();
    let written = carve
        .format_with_diagnostics(input, &WriteOptions::default())
        .unwrap();
    let mut value = serde_json::to_value(tree.document()).unwrap();
    strip(&mut value);
    (written.output, value)
}

#[test]
fn delimiter_before_hard_break_takes_its_kind_escape() {
    let carve = Carve::new();
    for (name, input, expected) in CASES {
        let (written, tree) = write(&carve, input);
        assert_eq!(&written, expected, "{name}");
        let (_, again) = write(&carve, &written);
        assert_eq!(again, tree, "{name}");
    }
}

/// A `/` or `*` right before a written hard break that the parser can open
/// (no word character before it), left bare while the same kind is escaped
/// elsewhere in the output.
fn bare_opener_before_break(written: &str) -> bool {
    written.lines().any(|line| {
        let b = line.as_bytes();
        let n = b.len();
        if n < 2 || b[n - 1] != b'\\' {
            return false;
        }
        let d = b[n - 2];
        if d != b'/' && d != b'*' {
            return false;
        }
        let prev = n.checked_sub(3).map(|i| b[i]);
        if prev.is_some_and(|p| p == b'\\' || p == d || p.is_ascii_alphanumeric()) {
            return false;
        }
        written.contains(&format!("\\{}", char::from(d)))
    })
}

#[test]
fn delimiter_before_hard_break_variants() {
    let carve = Carve::new();
    let heads = [
        "x(/u)", "x(*u)", "(/u)", "(*u)", "a /b", "a *b", "x", "\\/y", "/w/ ", "",
    ];
    let tails = ["\\/", "\\*", " \\/", "(\\/", "(\\*", "x\\/", "/", "*"];
    let nexts = ["", "foo", "foo/", "y*", "/z/"];
    let containers: &[(&str, &str)] = &[
        ("", ""),
        ("> ", "> "),
        ("- ", "  "),
        ("1. ", "   "),
        ("> - ", ">   "),
    ];
    let (mut checked, mut bare) = (0, 0);
    for head in heads {
        for tail in tails {
            for next in nexts {
                let mut inputs = Vec::new();
                for (first, rest) in containers {
                    let input = if next.is_empty() {
                        format!("{first}{head}{tail}\\\n")
                    } else {
                        format!("{first}{head}{tail}\\\n{rest}{next}\n")
                    };
                    inputs.push((input, true));
                }
                let body = if next.is_empty() {
                    format!("{head}{tail}\\\n")
                } else {
                    format!("{head}{tail}\\\n{next}\n")
                };
                inputs.push((format!("::: hardbreaks\n{body}:::\n"), true));
                inputs.push((format!("::: \\\n{head}{tail}\n{next}\n:::\n"), false));
                for (input, backslash) in inputs {
                    let (written, tree) = write(&carve, &input);
                    let (again, reread) = write(&carve, &written);
                    assert_eq!(reread, tree, "{input:?} -> {written:?}");
                    assert_eq!(again, written, "not idempotent: {input:?}");
                    // C2-W-080: below a hard-break opener a break is the bare line ending.
                    let bare_breaks = !backslash || written.starts_with("::: \\\n");
                    if !bare_breaks && bare_opener_before_break(&written) {
                        bare += 1;
                        eprintln!("bare opener: {input:?} -> {written:?}");
                    }
                    checked += 1;
                }
            }
        }
    }
    assert_eq!(checked, 2_800);
    assert_eq!(bare, 0);
}
