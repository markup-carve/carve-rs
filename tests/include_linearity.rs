//! Include expansion is linear in the size of the texts (C2-N-067): render
//! time at sizes n, 2n, 4n and 8n for stress shapes, each doubling at most
//! 2.6 times as long, as in `linearity.rs`.
#![allow(clippy::format_push_string, clippy::format_collect)]

use carve::include::{Cause, Includes, Request};
use carve::options::Options;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const LIMIT: f64 = 2.6;
static SERIAL: Mutex<()> = Mutex::new(());
const FLOOR: Duration = Duration::from_millis(4);

/// An entry document and the files it reads.
type Shape = (String, Vec<(&'static str, String)>);

fn opts(files: Vec<(&'static str, String)>) -> Options {
    let map: BTreeMap<&'static str, String> = files.into_iter().collect();
    let resolver = Arc::new(move |r: &Request<'_>| {
        map.get(r.path)
            .map(|t| (t.as_bytes().to_vec(), r.path.to_string()))
            .ok_or(Cause::NotFound)
    });
    Options {
        includes: Some(Includes::new(resolver)),
        ..Options::default()
    }
}

fn time_of(shape: &Shape) -> Duration {
    let o = opts(shape.1.clone());
    (0..5)
        .map(|_| {
            let t = Instant::now();
            std::hint::black_box(carve::render(&shape.0, &o));
            t.elapsed()
        })
        .min()
        .unwrap_or_default()
}

fn ratios(make: &dyn Fn(usize) -> Shape) -> Vec<f64> {
    let mut base = 1;
    while time_of(&make(base)) < FLOOR && base < 1 << 14 {
        base *= 2;
    }
    let times: Vec<f64> = [1, 2, 4, 8]
        .iter()
        .map(|k| time_of(&make(base * k)).as_secs_f64())
        .collect();
    times.windows(2).map(|w| w[1] / w[0]).collect()
}

fn check(name: &str, make: &dyn Fn(usize) -> Shape) {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let first = ratios(make);
    if first.iter().all(|&r| r < LIMIT) {
        return;
    }
    let second = ratios(make);
    assert!(
        second.iter().all(|&r| r < LIMIT),
        "{name}: time ratios per doubling {first:.2?} and {second:.2?}, limit {LIMIT}"
    );
}

#[test]
fn c2_n_067_many_directives_of_one_file() {
    check("directives", &|k| {
        (
            "{{ a.crv }}\n\n".repeat(k),
            vec![("a.crv", "A *b* [c][d].\n\n[d]: /u\n".into())],
        )
    });
}

#[test]
fn c2_n_067_lines_of_a_long_file() {
    check("lines", &|k| {
        let file = "line text\n".repeat(2 * k);
        (
            format!("{{{{ a.crv @lines:{}-{} }}}}", k, 2 * k),
            vec![("a.crv", file)],
        )
    });
}

#[test]
fn c2_n_067_last_of_many_regions() {
    check("regions", &|k| {
        let mut file = String::new();
        for i in 0..k {
            file.push_str(&format!("%% #region r{i}\ntext {i}\n%% #endregion\n"));
        }
        (
            format!("{{{{ a.crv @region:r{} }}}}", k - 1),
            vec![("a.crv", file)],
        )
    });
}

#[test]
fn c2_n_067_last_of_many_sections() {
    check("sections", &|k| {
        let file: String = (0..k).map(|i| format!("= H{i}\n\nBody {i}.\n\n")).collect();
        (
            format!("{{{{ a.crv #H{} }}}}", k - 1),
            vec![("a.crv", file)],
        )
    });
}

#[test]
fn c2_n_067_rows_and_cols_of_a_large_table() {
    check("table", &|k| {
        let head: Vec<String> = (0..8).map(|i| format!("c{i}")).collect();
        let mut file = head.join(",") + "\n";
        for r in 0..k {
            file.push_str(&format!("{r},b,c,d,e,f,g,h\n"));
        }
        let entry = format!("{{table}}\n{{{{ t.csv @rows:2-{k} @cols:\"c3,c0\" @code:csv }}}}");
        (entry, vec![("t.csv", file)])
    });
}

#[test]
fn c2_n_067_references_resolved_outward() {
    check("references", &|k| {
        let file: String = (0..k)
            .map(|i| format!("[x][r{i}] and [^n{i}]\n\n"))
            .collect();
        let defs: String = (0..k)
            .map(|i| format!("[r{i}]: /u{i}\n[^n{i}]: N{i}\n"))
            .collect();
        (format!("{{{{ a.crv }}}}\n\n{defs}"), vec![("a.crv", file)])
    });
}

#[test]
fn c2_n_067_shifted_headings() {
    check("shift", &|k| {
        let file: String = (0..k).map(|i| format!("= H{i}\n\n== S{i}\n\n")).collect();
        (
            "= Top\n\n{{ a.crv @shift:auto }}".into(),
            vec![("a.crv", file)],
        )
    });
}
