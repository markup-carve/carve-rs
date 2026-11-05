fn assert_stable(source: &str, expected: &str) {
    let written = carve::to_carve(source);
    assert_eq!(written, expected);
    assert_eq!(carve::to_html(&written), carve::to_html(source));
    assert_eq!(carve::to_carve(&written), written);
}

#[test]
fn trailing_comments_keep_the_next_list_item() {
    assert_stable(
        "1. :: t\n   : # h\n\n     %% note\n\n     %% c2\n2. b\n",
        "1. :: t\n   : # h\n\n     %% note\n\n     %% c2\n2. b\n",
    );
    assert_stable(
        "- %% c0\n   %% note\n\n  %% c2\n- b\n",
        "- %% c0\n\n  %% note\n\n  %% c2\n- b\n",
    );
}

#[test]
fn tight_item_keeps_a_delimited_comment_beside_a_block() {
    let source = "- ```\n  a\n  ```\n  {% n %}\n  > q\n- b\n";
    assert_stable(source, source);
}

#[test]
fn trailing_delimited_comment_does_not_hide_whitespace_from_a_closer() {
    for marker in ["*", "/", "_", "~", "="] {
        for space in [" ", "\n"] {
            let source = format!("{{{marker}a{space}{{% x %}}{marker}}}\n");
            assert_stable(&source, &source);
        }
    }
}

#[test]
fn delimited_comment_closer_variant_sweep() {
    let mut checked = 0;
    for marker in ["*", "/", "_", "~", "="] {
        for gap in [" ", "\n"] {
            for word in ["a", "ab", "x1", "é", "x y"] {
                for comment in ["x", "two", "tag", "0", "inner"] {
                    for host in ["", "- ", "> ", ": "] {
                        let source =
                            format!("{host}{{{marker}{word}{gap}{{% {comment} %}}{marker}}}\n");
                        let written = carve::to_carve(&source);
                        assert_eq!(
                            carve::to_html(&written),
                            carve::to_html(&source),
                            "{source:?}"
                        );
                        assert_eq!(carve::to_carve(&written), written, "{source:?}");
                        checked += 1;
                    }
                }
            }
        }
    }
    assert_eq!(checked, 1000);
}

#[test]
fn tight_item_comment_boundary_variant_sweep() {
    let mut checked = 0;
    for marker in ["-", "*", "1.", "2.", "."] {
        for payload in ["a", "ab", "x1", "hello", "é"] {
            for comment in ["x", "two", "tag", "0", "inner"] {
                for following in ["> q", "```\nb\n```", "::: note\nb\n:::", "# h"] {
                    for second in ["b", "other"] {
                        let source = format!(
                            "{marker} ```\n  {payload}\n  ```\n  {{% {comment} %}}\n  {}\n{marker} {second}\n",
                            following.replace('\n', "\n  ")
                        );
                        let written = carve::to_carve(&source);
                        assert_eq!(
                            carve::to_html(&written),
                            carve::to_html(&source),
                            "{source:?}"
                        );
                        assert_eq!(carve::to_carve(&written), written, "{source:?}");
                        checked += 1;
                    }
                }
            }
        }
    }
    assert_eq!(checked, 1000);
}

#[test]
fn trailing_line_comment_variant_sweep() {
    let mut checked = 0;
    for marker in ["-", "*", "1.", "."] {
        for lead in ["a", "ab", "x1", "hello", "é"] {
            for first in ["x", "two", "tag", "0", "inner"] {
                for second in ["y", "three", "note", "1", "outer"] {
                    for tail in ["b", "other"] {
                        let source = format!(
                            "{marker} {lead}\n\n  %% {first}\n\n  %% {second}\n{marker} {tail}\n"
                        );
                        let written = carve::to_carve(&source);
                        assert_eq!(
                            carve::to_html(&written),
                            carve::to_html(&source),
                            "{source:?}"
                        );
                        assert_eq!(carve::to_carve(&written), written, "{source:?}");
                        checked += 1;
                    }
                }
            }
        }
    }
    assert_eq!(checked, 1000);
}

#[test]
fn a_definition_in_an_item_gap_is_collected_without_a_later_region_marker() {
    let source = "- a\n+\n[r]: /a\n[r]: /b\nmore\n\nSee [r][].\n";
    let expected = "- a\n+\nmore\n\nSee [r][].\n\n[r]: /b\n";
    assert_stable(source, expected);
}

#[test]
fn collected_definition_gap_variant_sweep() {
    let mut checked = 0;
    for marker in ["-", "1."] {
        for lead in ["a", "ab", "x1", "hello", "é"] {
            for label in ["r", "ref", "x1", "link", "é"] {
                for dest in ["a", "ab", "x1", "path", "é"] {
                    for tail in ["b", "more", "x1", "other"] {
                        let source = format!(
                            "{marker} {lead}\n+\n[{label}]: /{dest}\n{tail}\n\nSee [t][{label}].\n"
                        );
                        let written = carve::to_carve(&source);
                        assert_eq!(
                            carve::to_html(&written),
                            carve::to_html(&source),
                            "{source:?}"
                        );
                        assert_eq!(carve::to_carve(&written), written, "{source:?}");
                        assert!(!written.contains(&format!("  [{label}]:")), "{source:?}");
                        checked += 1;
                    }
                }
            }
        }
    }
    assert_eq!(checked, 1000);
}
