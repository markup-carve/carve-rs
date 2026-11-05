// djot.js ends an unclosed fence with the blockquote that holds it, so the lines
// after the quote are outside the fence and a blank before a sub-list still goes.
#[test]
fn an_unclosed_fence_ends_with_its_quote() {
    for (djot, carve) in [
        (
            "> ```\n> code\n\n- a\n\n  - b\n",
            "> ```\n> code\n\n- a\n  - b\n",
        ),
        (
            "> > ~~~\n> > code\n>\n> - a\n>\n>   - b\n",
            "> > ~~~\n> > code\n>\n> - a\n>   - b\n",
        ),
    ] {
        assert_eq!(carve::djot_to_carve(djot), carve, "{djot}");
    }
}

// Every import pass reads the lines after a quote's unclosed fence as it reads
// them with no fence above: the tail imports the same either way.
#[test]
fn every_import_pass_ends_a_fence_with_its_quote() {
    let bodies = [
        "- a\n\n  - b\n",
        "- a\n\n\n  - b\n",
        "* a\n\n  * b\n",
        "1. a\n\n   1. b\n",
        "- [ ] a\n\n  - b\n",
        "- a\n  more\n\n  - b\n",
        "- a\n\n  {.c}\n  - b\n",
        "- {.c}\n\n  para\n",
        "1. {#x}\n\n   para\n",
        "-\n  text\n",
        "- [ ]\n  text\n",
        "-\n\n- b\n",
        "[^n]: - [ ]\ntext\n\n[^n]\n",
        "a [^n]\n\n[^n]: - [ ]\n  text\n",
    ];
    let fences = ["```", "~~~~ rust"];
    let mut checked = 0;
    let mut failed = Vec::new();
    for body in bodies {
        for fence in fences {
            for code in ["code", "- a\n\n  - b"] {
                for depth in 1..=2 {
                    for outer in 0..depth {
                        let quote = "> ".repeat(depth);
                        let outer_quote = "> ".repeat(outer);
                        let mut head = format!("{quote}{fence}\n");
                        for line in code.lines() {
                            head.push_str(&format!("{quote}{line}\n"));
                        }
                        head.push_str(&format!("{}\n", outer_quote.trim_end()));
                        let tail: String = body
                            .lines()
                            .map(|line| format!("{outer_quote}{line}\n").replace("> \n", ">\n"))
                            .collect();
                        let alone = carve::djot_to_carve(&tail);
                        let after = carve::djot_to_carve(&format!("{head}{tail}"));
                        let fenced = carve::djot_to_carve(&head);
                        if without(&after, &fenced) != without(&alone, "") {
                            failed.push(format!("{head}{tail}"));
                        }
                        checked += 1;
                    }
                }
            }
        }
    }
    assert!(
        failed.is_empty(),
        "{} of {checked}:\n{}",
        failed.len(),
        failed.join("\n====\n")
    );
}

// Drops the fenced head's own lines, then reads any run of empty lines as one.
fn without(carve: &str, head: &str) -> Vec<String> {
    let mut head = head.lines().filter(|line| !empty(line)).peekable();
    let mut out: Vec<String> = Vec::new();
    for line in carve.lines() {
        if head.peek() == Some(&line) {
            head.next();
            continue;
        }
        if empty(line) && out.last().map_or(true, String::is_empty) {
            continue;
        }
        out.push(if empty(line) {
            String::new()
        } else {
            line.to_owned()
        });
    }
    while out.last().is_some_and(String::is_empty) {
        out.pop();
    }
    out
}

fn empty(line: &str) -> bool {
    line.trim_matches(['>', ' ']).is_empty()
}
