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
