//! CARVE-P11-079 (b): consecutive container-owned line comments form one run.

fn check(source: &str, expected: &str) {
    let formatted = carve::to_carve(source);
    assert_eq!(formatted, expected, "source: {source:?}");
    assert_eq!(carve::to_html(&formatted), carve::to_html(source));
    assert_eq!(carve::to_carve(&formatted), formatted);
}

#[test]
fn a_loose_description_keeps_one_run() {
    check(
        ":: t\n: a\n\n  %% c\n  %% d\n\n  b\n",
        ":: t\n: a\n\n  %% c\n  %% d\n\n  b\n",
    );
}

#[test]
fn a_loose_list_keeps_one_run_between_items() {
    check(
        "- a\n\n  %% y\n  %% z\n- c\n",
        "- a\n\n  %% y\n  %% z\n\n- c\n",
    );
}

#[test]
fn document_owned_lines_remain_separate() {
    check("a\n\n%% c\n%% d\n\nb\n", "a\n\n%% c\n\n%% d\n\nb\n");
}

#[test]
fn one_thousand_container_run_variants_read_back() {
    let words = ["a", "bold", "[link](url)", "one two", "x /em/ y"];
    let tails = [
        "b",
        "next",
        "last words",
        "[ref](url)",
        "z *strong* q",
        "a /soft/ b",
        "plain",
        "end item",
        "x and y",
        "the end",
    ];
    let mut checked = 0;
    for description in [false, true] {
        for loose in [false, true] {
            for count in 2..=6 {
                for first in words {
                    for last in tails {
                        let mut source = if description {
                            format!(":: term\n: {first}\n")
                        } else {
                            format!("- {first}\n")
                        };
                        if loose {
                            source.push('\n');
                        }
                        for n in 0..count {
                            source.push_str(&format!("  %% comment {n}\n"));
                        }
                        if loose {
                            source.push('\n');
                        }
                        source.push_str(&format!("  {last}\n"));
                        if description {
                            source.push_str(": second\n");
                        } else {
                            source.push_str("- second\n");
                        }
                        let formatted = carve::to_carve(&source);
                        assert_eq!(
                            carve::to_html(&formatted),
                            carve::to_html(&source),
                            "readback failed for {source:?} -> {formatted:?}"
                        );
                        assert_eq!(carve::to_carve(&formatted), formatted);
                        for n in 0..count - 1 {
                            assert!(
                                formatted
                                    .contains(&format!("%% comment {n}\n  %% comment {}", n + 1)),
                                "run split for {source:?} -> {formatted:?}"
                            );
                        }
                        checked += 1;
                    }
                }
            }
        }
    }
    assert_eq!(checked, 1_000);
}
