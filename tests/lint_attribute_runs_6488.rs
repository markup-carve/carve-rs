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

/// Openers glued inside one unquoted value, which takes `{`: each opener reads the same value.
fn glued_values(count: usize, close: char) -> String {
    format!("{{^{}{close}^}}\n", "{a=x".repeat(count))
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

fn assert_linear(name: &str, sizes: [usize; 3], shape: impl Fn(usize) -> String) {
    let ratios = || -> Vec<f64> {
        let times: Vec<f64> = sizes
            .iter()
            .map(|&size| time_of(&shape(size)).as_secs_f64())
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
        "{name}: time ratios per doubling {first:.2?} and {second:.2?}, limit 3.0"
    );
}

#[test]
fn nesting_depth_is_linear() {
    // Each node used to try every brace inside it: doubling the depth took about four times as long.
    assert_linear("nested", [400, 800, 1600], nested);
}

#[test]
fn glued_openers_in_one_value_are_linear() {
    // Each opener used to scan the shared value to its end, and copy it when the block closed.
    assert_linear("failing", [2000, 4000, 8000], |count| {
        glued_values(count, '|')
    });
    assert_linear("closing", [2000, 4000, 8000], |count| {
        glued_values(count, '}')
    });
}

#[test]
fn many_keys_in_one_block_are_linear() {
    // Each key used to look through every later key of its owner for one of the same name.
    let keys = |count: usize| {
        let keys: Vec<String> = (0..count).map(|at| format!("k{at}")).collect();
        format!("{{{}}}\nfoo\n", keys.join(" "))
    };
    assert_linear("keys", [2000, 4000, 8000], keys);
}
