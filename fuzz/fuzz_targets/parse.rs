#![no_main]
//! Parse to a tree, encode it as JSON and read the JSON back (C2-A-162).

use carve::api::{self, Options};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if data.len() > carve_fuzz::MAX_INPUT {
        return;
    }
    let Ok(tree) = api::parse(data, &Options::default()) else {
        return;
    };
    let json = api::encode_tree(&tree);
    let back = api::decode_tree(&json).expect("an encoded tree decodes");
    assert_eq!(
        api::encode_tree(&back),
        json,
        "encode(decode(encode(t))) differs"
    );
});
