//! Deterministic byte mutation smoke test for malformed source and nesting limits.

fn next(state: &mut u64) -> u64 {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    *state
}

fn next_index(state: &mut u64, bound: usize) -> usize {
    usize::try_from(next(state)).unwrap_or_default() % bound
}

fn next_byte(state: &mut u64) -> u8 {
    u8::try_from(next(state) & 0xff).unwrap_or_default()
}

fn exercise(bytes: &[u8]) {
    let opts = carve2::Options::default();
    let _ = carve2::try_render_bytes(bytes, &opts).unwrap();
    let arena = carve2::Arena::new();
    let tree = carve2::try_parse_resolved_bytes_in(bytes, &opts, &arena).unwrap();
    let encoded = carve2::to_json(&tree);
    let mut parser = serde_json::Deserializer::from_str(&encoded);
    parser.disable_recursion_limit();
    let _: serde_json::Value = serde::Deserialize::deserialize(&mut parser).unwrap();
}

#[test]
fn mutated_bytes_and_deep_generated_shapes_do_not_panic() {
    let bases = [
        include_bytes!("data/bench/commonmark-core.crv").as_slice(),
        include_bytes!("data/bench/lists-tables.crv").as_slice(),
        b"::: |\n\t a\n::: \n\n|= a | b |\n+ c | d |\n".as_slice(),
    ];
    let mut state = 0x14b3_9a71_4c2d_81e5_u64;
    for i in 0..512 {
        let mut bytes = bases[i % bases.len()].to_vec();
        for _ in 0..8 {
            let at = next_index(&mut state, bytes.len() + 1);
            match next(&mut state) % 3 {
                0 if at < bytes.len() => bytes[at] = next_byte(&mut state),
                1 => bytes.insert(at, next_byte(&mut state)),
                _ if at < bytes.len() => {
                    bytes.remove(at);
                }
                _ => {}
            }
        }
        exercise(&bytes);
    }
    for i in 0..512 {
        let len = next_index(&mut state, 512);
        let bytes: Vec<u8> = (0..len).map(|_| next_byte(&mut state)).collect();
        exercise(&bytes);
        if i % 512 == 0 {
            exercise(format!("{}*a*", "> ".repeat(210)).as_bytes());
        }
    }
}

#[test]
fn c2_b_218_degraded_inline_uses_decoded_source_ranges() {
    let opts = carve2::Options {
        profile: carve2::options::Profile::Comment,
        ..carve2::Options::default()
    };
    let bytes = b"\xff\xff\xff\xff ab </#abc> @bob :smile: `c`\n";
    let decoded = "\u{FFFD}\u{FFFD}\u{FFFD}\u{FFFD} ab </#abc> @bob :smile: `c`\n";
    assert_eq!(
        carve2::try_render_bytes(bytes, &opts).unwrap(),
        carve2::try_render(decoded, &opts).unwrap()
    );
    let minimal = carve2::Options {
        profile: carve2::options::Profile::Minimal,
        ..carve2::Options::default()
    };
    let nul_source = "x $`a\0b`";
    let nul_arena = carve2::Arena::new();
    let nul_tree = carve2::parse_resolved_in(nul_source, &minimal, &nul_arena).unwrap();
    let expected_nul = carve2::try_render("x $`a\u{FFFD}b`", &minimal).unwrap();
    assert!(expected_nul.contains('\u{FFFD}'));
    assert_eq!(
        carve2::try_render(nul_source, &minimal).unwrap(),
        expected_nul
    );
    let (nul_html, nul_losses) =
        carve2::html::render_with_losses_source(&nul_tree, &minimal, nul_source);
    assert!(nul_losses.contains(&"profile-construct-degraded"));
    assert_eq!(nul_html, expected_nul);
    assert_eq!(
        carve2::try_render_bytes(b"a\xe2\x82 b\0 :smile: `c`", &opts).unwrap(),
        carve2::try_render("a\u{FFFD} b\u{FFFD} :smile: `c`", &opts).unwrap()
    );
    let malformed = b"x $`a\xffb`";
    let arena = carve2::Arena::new();
    let tree = carve2::try_parse_resolved_bytes_in(malformed, &minimal, &arena).unwrap();
    let (html, losses) = carve2::html::render_with_losses_source_bytes(&tree, &minimal, malformed);
    assert!(losses.contains(&"profile-construct-degraded"));
    assert_eq!(html, carve2::try_render_bytes(malformed, &minimal).unwrap());
}

#[test]
fn c2_h_163_byte_source_limit_counts_input_bytes() {
    let opts = carve2::Options {
        profile: carve2::options::Profile::Minimal,
        ..carve2::Options::default()
    };
    let bytes = vec![0xff; 5_000];
    assert!(carve2::try_render_bytes(&bytes, &opts).is_ok());
    assert!(matches!(
        carve2::try_render_bytes(&vec![0xff; 10_001], &opts),
        Err(carve2::RenderError::TooLong { .. })
    ));
}
