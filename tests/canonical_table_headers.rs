#[test]
fn fmt_uses_native_headers_and_preserves_table_html() {
    let cases = [
        (
            "| Left | Center | Right | Plain |\n|:---|:---:|---:|---|\n| a | b | c | d |\n",
            "|=< Left |=~ Center |=> Right |= Plain |\n| a | b | c | d |\n",
        ),
        (
            "| Left | Center | Right |\n|:---|:---:|---:|\n",
            "|=< Left |=~ Center |=> Right |\n",
        ),
        (
            "| A | B |\n|---|---|\n| a | b | wider |\n",
            "|= A |= B |\n| a | b | wider |\n",
        ),
    ];
    for (source, expected) in cases {
        let formatted = carve::to_carve(source);
        assert_eq!(formatted, expected);
        assert_eq!(carve::to_html(&formatted), carve::to_html(source));
        assert_eq!(carve::to_carve(&formatted), formatted);
    }
}

#[test]
fn generated_table_headers_preserve_html_across_columns_and_containers() {
    let markers = ["", "<", "~", ">"];
    let mut count = 0;
    for container in 0..5 {
        for columns in 1..=4 {
            for marker in markers {
                for width in 1..=4 {
                    for extra in 0..=2 {
                        let heads: Vec<_> = (0..columns).map(|i| format!("H{i}")).collect();
                        let cells: Vec<_> = (0..columns + extra).map(|i| format!("v{i}")).collect();
                        let delimiter = (0..columns)
                            .map(|_| {
                                format!(
                                    "{}{}{}",
                                    if matches!(marker, "<" | "~") { ":" } else { "" },
                                    "-".repeat(width),
                                    if matches!(marker, ">" | "~") { ":" } else { "" }
                                )
                            })
                            .collect::<Vec<_>>()
                            .join("|");
                        let source = format!(
                            "| {} |\n|{delimiter}|\n| {} |\n",
                            heads.join(" | "),
                            cells.join(" | ")
                        );
                        let source = match container {
                            0 => source,
                            1 => source.lines().map(|line| format!("> {line}\n")).collect(),
                            2 => source.lines().map(|line| format!("> > {line}\n")).collect(),
                            3 => format!("::: note\n{source}:::\n"),
                            _ => format!("- item\n+\n{source}"),
                        };
                        let formatted = carve::to_carve(&source);
                        assert_eq!(
                            carve::to_html(&formatted),
                            carve::to_html(&source),
                            "variant {count}"
                        );
                        assert_eq!(carve::to_carve(&formatted), formatted, "variant {count}");
                        count += 1;
                    }
                }
            }
        }
    }
    assert_eq!(count, 960);
}
