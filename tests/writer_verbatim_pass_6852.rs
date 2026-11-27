//! C2-W-075, C2-W-088: a kind pass that stops at a tree-spelled reference may already be the final
//! form once the reference's verbatim text is restored. These four braces are carve-js's 8 KB output.

use carve::api::{Carve, WriteOptions};

#[test]
fn cc_2323_kind_round_preserves_the_js_spelling() -> Result<(), Box<dyn std::error::Error>> {
    let source = "[a *[a][]*[x]: /u\n".repeat(455);
    let result = Carve::new().format_with_diagnostics(&source, &WriteOptions::default())?;
    let braces: Vec<(usize, char)> = result
        .output
        .char_indices()
        .filter(|&(_, ch)| matches!(ch, '{' | '}'))
        .collect();
    assert_eq!(braces, [(4815, '{'), (5861, '}'), (7607, '{'), (8167, '}')]);
    assert_eq!(
        result
            .output
            .chars()
            .filter(|&ch| !matches!(ch, '{' | '}'))
            .collect::<String>(),
        source
    );
    assert!(
        result
            .diagnostics
            .iter()
            .all(|loss| loss.code != "writer-refused")
    );
    Ok(())
}

#[test]
fn nested_quote_pass_keeps_its_authored_spelling() -> Result<(), Box<dyn std::error::Error>> {
    for line in ["> > [a[**[a][]**[x]: /u\n", "> > [a}**[a][]**[x]: /u\n"] {
        let source = line.repeat(910);
        let result = Carve::new().format_with_diagnostics(&source, &WriteOptions::default())?;
        assert_eq!(result.output, source);
    }
    Ok(())
}
