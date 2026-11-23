//! The glued brace run that ends an inline node (C2-I-036) is found once per brace, not once per
//! brace and enclosing node: lint of nested brace-dense inline nodes is linear in the nesting depth
//! (carve#6488), and the records the rules read stay the same.

use carve::lint::{LintOptions, lint};
use std::time::{Duration, Instant};

fn findings(source: &str) -> Vec<(&'static str, u32)> {
    lint(source, &LintOptions::default())
        .into_iter()
        .map(|finding| (finding.code, finding.pos.map_or(0, |pos| pos.start_column)))
        .collect()
}

#[test]
fn nested_inline_nodes_keep_their_attribute_runs() {
    let semantic = "semantic-attribute-outside-span";
    assert_eq!(findings("{^*b*{abbr=\"x\"}^}\n"), [(semantic, 7)]);
    assert_eq!(findings("[a]{abbr=\"x\"}{.c}\n"), []);
    assert_eq!(findings("{^[a]{kbd}{.c}^}{abbr=y}\n"), [(semantic, 18)]);
    assert_eq!(
        findings("[a {.q}]{abbr=\"x}\"}\n"),
        [("unattached-inline-attribute", 4)]
    );
}

fn nested(depth: usize) -> String {
    format!("> {}x{}\n", "{^".repeat(depth), "^}".repeat(depth))
}

fn time_of(source: &str) -> Duration {
    (0..5)
        .map(|_| {
            let t = Instant::now();
            std::hint::black_box(lint(source, &LintOptions::default()));
            t.elapsed()
        })
        .min()
        .unwrap_or_default()
}

#[test]
fn nesting_depth_is_linear() {
    // Each node used to try every brace inside it: doubling the depth took about four times as long.
    let ratios = || -> Vec<f64> {
        let times: Vec<f64> = [400, 800, 1600]
            .iter()
            .map(|&depth| time_of(&nested(depth)).as_secs_f64())
            .collect();
        times.windows(2).map(|w| w[1] / w[0]).collect()
    };
    let first = ratios();
    if first.iter().all(|&r| r < 3.0) {
        return;
    }
    let second = ratios();
    assert!(
        second.iter().all(|&r| r < 3.0),
        "time ratios per depth doubling {first:.2?} and {second:.2?}, limit 3.0"
    );
}
