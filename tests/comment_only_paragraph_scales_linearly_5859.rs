//! An item paragraph of repeated `{%` opener lines parses in linear time
//! (carve#5859, PART 9 §9 LINEAR TIME).

use std::time::{Duration, Instant};

fn best_time(source: &str) -> Duration {
    (0..3)
        .map(|_| {
            let start = Instant::now();
            assert!(!carve::to_html(source).is_empty());
            start.elapsed()
        })
        .min()
        .unwrap()
}

fn item(n: usize) -> String {
    format!("- x\n\n{}   z %}}\n- s\n", "  {% a\n".repeat(n))
}

fn nested_item(n: usize) -> String {
    format!("- x\n  - y\n\n{}     z %}}\n- s\n", "    {% a\n".repeat(n))
}

#[test]
fn repeated_openers_scale_linearly() {
    for shape in [item as fn(usize) -> String, nested_item] {
        let small = best_time(&shape(250));
        let large = best_time(&shape(1000));
        // 4x the lines: linear is about 4x, the quadratic path was 16x.
        assert!(
            large <= small * 9 + Duration::from_millis(50),
            "250 lines {small:?}, 1000 lines {large:?}"
        );
    }
}

#[test]
fn repeated_openers_keep_their_output() {
    assert_eq!(
        carve::to_html(&item(3)).trim(),
        "<ul>\n  <li><p>x</p>\n    <p>{% a\n{% a\n</p>\n  </li>\n  <li><p>s</p></li>\n</ul>"
    );
}
