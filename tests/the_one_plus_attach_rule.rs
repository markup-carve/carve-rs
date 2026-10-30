//! The one attach rule and the refused `+` (carve#4409, carve#4396, carve#4408).

use carve::to_html;
use serde_json::Value;

fn squeeze(html: &str) -> String {
    let mut out = String::new();
    for line in html.lines() {
        out.push_str(line.trim_start());
    }
    out
}

fn assert_html(source: &str, want: &str) {
    assert_eq!(squeeze(&to_html(source)), want, "{source:?}");
}

#[test]
fn the_ticket_expecteds() {
    for (source, want) in [
        // carve#4409
        (
            ":: t\n: +\n- l\n",
            "<dl><dt>t</dt><dd><ul><li>l</li></ul></dd></dl>",
        ),
        (
            ":: t\n: - +\np\n",
            "<dl><dt>t</dt><dd><ul><li>+p</li></ul></dd></dl>",
        ),
        (
            "- a\n  * +\n[r]: /u\n  after\n",
            "<ul><li>a<ul><li>+</li></ul></li></ul><p>after</p>",
        ),
        // The sub-rulings.
        (
            "> +\n> x\n",
            "<blockquote><blockquote><p>x</p></blockquote></blockquote>",
        ),
        ("- +\n- b\n", "<ul><li>+</li><li>b</li></ul>"),
        ("- +\n  para\n", "<ul><li>+para</li></ul>"),
        // carve#4408
        (
            ":: t\n: +\nl\n : b\n",
            "<dl><dt>t</dt><dd>l</dd><dd>b</dd></dl>",
        ),
        (
            ":: t\n: +\n# H\n : b\n",
            "<dl><dt>t</dt><dd><h1 id=\"H\">H</h1></dd><dd>b</dd></dl>",
        ),
        (
            ":: t\n : +\nl\n : b\n",
            "<dl><dt>t</dt><dd>+l</dd><dd>b</dd></dl>",
        ),
    ] {
        assert_html(source, want);
    }
}

#[test]
fn a_column_zero_block_attaches_to_a_note_at_column_zero() {
    let html = squeeze(&to_html("[^a]: +\n- l\n\nsee[^a]\n"));
    assert!(
        html.contains("<li id=\"fn1\"><ul><li>l</li></ul>"),
        "{html}"
    );
}

#[test]
fn every_appendix_a_row_flips_as_the_report_says() {
    let rows: Vec<Value> =
        serde_json::from_str(include_str!("fixtures/plus-appendix-a.json")).unwrap();
    assert_eq!(rows.len(), 107);
    let failed: Vec<&str> = rows
        .iter()
        .filter(|row| {
            to_html(row["source"].as_str().unwrap()).trim() != row["html"].as_str().unwrap().trim()
        })
        .map(|row| row["name"].as_str().unwrap())
        .collect();
    assert!(failed.is_empty(), "{failed:#?}");
}

/// Replacing a refused `+` with a letter changes the output only in that character.
#[test]
fn a_refused_plus_is_a_plain_letter() {
    let hosts: [&[&str]; 12] = [
        &["- +"],
        &["1. +"],
        &["- [ ] +"],
        &["- - +"],
        &["> +"],
        &["> > +"],
        &["> - +"],
        &[":: t", ": +"],
        &[":: t", ": - +"],
        &["[^a]: +"],
        &["> [^a]: +"],
        &["- a", "  - +"],
    ];
    let separate: [&[&str]; 6] = [
        &["- a", "+"],
        &["> a", "+"],
        &[":: t", ": a", "+"],
        &["[^a]: a", "+"],
        &["- ```", "  x", "+"],
        &["- a", "  - b", "  +"],
    ];
    let followers: [&[&str]; 11] = [
        &[],
        &[""],
        &["para"],
        &["# H"],
        &["- l"],
        &["> q"],
        &["```", "x", "```"],
        &["::: d", "y", ":::"],
        &[": b"],
        &["| c |"],
        &["{.k}", "para"],
    ];
    let mut kept = 0;
    for head in hosts.iter().chain(separate.iter()) {
        for follower in followers {
            for shift in 0..4 {
                let mut lines: Vec<String> = head.iter().map(|l| (*l).to_string()).collect();
                let pad = " ".repeat(shift);
                lines.extend(follower.iter().map(|l| format!("{pad}{l}")));
                lines.push(String::new());
                lines.push("see[^a]".to_string());
                let source = lines.join("\n") + "\n";
                let plain = to_html(&source);
                if !plain.contains('+') {
                    continue;
                }
                kept += 1;
                let plus_line = head.last().unwrap();
                let at = source.find(plus_line).unwrap() + plus_line.rfind('+').unwrap();
                let mut letter = source.clone();
                letter.replace_range(at..at + 1, "w");
                assert_eq!(plain, to_html(&letter).replace('w', "+"), "{source:?}");
            }
        }
    }
    assert!(kept > 300, "{kept}");
}

#[test]
fn a_source_holding_control_bytes_keeps_them() {
    let source = "\u{1}\u{2}\u{3}\u{4}\u{5}\u{6}\n\n- +\n- b\n";
    let html = to_html(source);
    assert!(html.contains('\u{1}'), "{html:?}");
    assert!(html.contains("<li>+</li>"), "{html:?}");
}

#[test]
fn a_source_holding_every_control_byte_reads_every_plus_alike() {
    let controls: String = (0x01u8..0x20)
        .filter(|byte| !matches!(byte, b'\t' | b'\n' | b'\r'))
        .map(char::from)
        .collect();
    let heads: [&[&str]; 12] = [
        &["- +"],
        &["* * +"],
        &["> +"],
        &["1. +"],
        &[":: t", ": +"],
        &["[^a]: +"],
        &["- a", "+"],
        &["> a", "+"],
        &[":: t", ": a", "+"],
        &["[^a]: a", "+"],
        &["- ```", "  x", "+"],
        &["- a", "  - b", "  +"],
    ];
    let followers: [&[&str]; 11] = [
        &[],
        &[""],
        &["para"],
        &["# H"],
        &["- l"],
        &["> q"],
        &["```", "x", "```"],
        &["::: d", "y", ":::"],
        &[": b"],
        &["| c |"],
        &["{.k}", "para"],
    ];
    let mut failed = Vec::new();
    for head in heads {
        for follower in followers {
            for shift in 0..4 {
                let pad = " ".repeat(shift);
                let mut lines: Vec<String> = head.iter().map(|l| (*l).to_string()).collect();
                lines.extend(follower.iter().map(|l| format!("{pad}{l}")));
                let source = lines.join("\n") + "\n";
                let want = to_html(&format!("zz\n\n{source}")).replace("zz", &controls);
                let got = to_html(&format!("{controls}\n\n{source}"));
                if got != want {
                    failed.push(source);
                }
            }
        }
    }
    assert!(failed.is_empty(), "{} {failed:#?}", failed.len());
}
