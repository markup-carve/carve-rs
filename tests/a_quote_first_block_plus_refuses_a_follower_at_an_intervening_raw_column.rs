//! A quote's first-block `+` reads its follower at the raw column, before rule A
//! moves the line. Off column 0, the opener line's column and the owning quote's
//! column it is refused and reads like a letter (CARVE-P9-067, carve#3421).

use carve::to_html;

fn flat(source: &str) -> String {
    to_html(source).split_whitespace().collect()
}

#[test]
fn a_follower_at_an_intervening_column_refuses_the_plus() {
    for (source, expected) in [
        (
            "> +\n # h\n",
            "<blockquote><p>+</p></blockquote><sectionid=\"h\"><h1>h</h1></section>",
        ),
        (
            "> +\n | a |\n",
            "<blockquote><p>+</p></blockquote><table><tbody><tr><td>a</td></tr></tbody></table>",
        ),
        (
            "> > +\n > q\n",
            "<blockquote><blockquote><p>+q</p></blockquote></blockquote>",
        ),
        (
            "- > +\n   # h\n",
            "<ul><li><blockquote><p>+</p></blockquote><h1id=\"h\">h</h1></li></ul>",
        ),
    ] {
        assert_eq!(flat(source), expected, "{source:?}");
    }
}

#[test]
fn every_refused_follower_reads_like_a_letter() {
    let hosts: [(&[&str], &str, bool); 6] = [
        (&[], "", false),
        (&["- a"], "  ", false),
        (&["- a", "  - b"], "    ", false),
        (&["1. a"], "   ", false),
        (&[], "- ", true),
        (&[], "- - ", true),
    ];
    let mut count = 0;
    for (pre, lead, marker) in hosts {
        for stack in ["> +", "> > +", "> > > +"] {
            let opener = format!("{lead}{stack}");
            let line_column = if marker { lead.len() - lead.trim_start().len() } else { lead.len() };
            let eligible = [0, line_column, opener.rfind('>').unwrap()];
            for col in 0..=8 {
                if eligible.contains(&col) {
                    continue;
                }
                for depth in 0..3 {
                    for block in ["q", "# h", "| a |", "***", "::: note", "{.c}"] {
                        let follower = format!("{}{}{block}", " ".repeat(col), "> ".repeat(depth));
                        let doc = |end: &str| {
                            let mut lines: Vec<String> = pre.iter().map(|l| l.to_string()).collect();
                            lines.push(format!("{}{end}", &opener[..opener.len() - 1]));
                            lines.push(follower.clone());
                            lines.join("\n") + "\n"
                        };
                        assert_eq!(to_html(&doc("+")), to_html(&doc("z")).replacen('z', "+", 1), "{:?}", doc("+"));
                        count += 1;
                    }
                }
            }
        }
    }
    assert!(count > 2000, "{count}");
}
