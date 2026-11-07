use super::*;

#[test]
fn no_link_definition_fast_path_matches_the_prepass_on_generated_documents() {
    let siblings = ["plain /text/", "# heading", "> quote", "- child"];
    let tails = ["", "\n", "\n\n", "\n\nlast"];
    for n in 0..1000 {
        let depth = n % 5;
        let indent = " ".repeat(n % 4);
        let mut source = String::from("- item\n");
        for level in 0..depth {
            source.push_str(&format!("{}- nested {level}\n", "  ".repeat(level + 1)));
        }
        source.push_str(&indent);
        source.push_str(siblings[(n / 5) % siblings.len()]);
        source.push_str(tails[(n / 20) % tails.len()]);
        let options = Options::default().with_positions(n % 2 == 0);
        let fast = parse_with_options_mode_and_index(&source, &options, ParseMode::Html).0;
        let original = FORCE_LINK_PREPASS.with(|flag| {
            flag.set(true);
            let document = parse_with_options_mode_and_index(&source, &options, ParseMode::Html).0;
            flag.set(false);
            document
        });
        assert_eq!(fast, original, "variant {n}: {source:?}");
    }
}
