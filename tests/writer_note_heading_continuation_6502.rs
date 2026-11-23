//! C2-B-154, C2-W-024: a heading continuation stands past the heading's marker, not at the container's body column (carve#6502).
use carve::api::{Carve, Options, WriteOptions};

#[test]
fn heading_continuations_stand_past_the_marker_in_every_container() {
    let mut cases = vec![
        "- = Head\n    cont\n".to_string(),
        ":: T\n: = Head\n    cont\n".to_string(),
        "> = Head\n>   cont\n".to_string(),
        "[^1]: Para.\n\n  = Head\n    cont\n".to_string(),
    ];
    for key in ["1", "12", "12345"] {
        for level in 1..=3 {
            let indent = " ".repeat(key.len() + 5 + level + 1);
            cases.push(format!(
                "Text.[^{key}]\n\n[^{key}]: {} Head\n{indent}cont\n{indent}more\n",
                "=".repeat(level)
            ));
        }
    }
    let carve = Carve::new();
    for input in &cases {
        for normalize in [false, true] {
            let options = WriteOptions {
                normalize,
                ..WriteOptions::default()
            };
            assert_eq!(&carve.format(input, &options).unwrap(), input);
            let tree = carve.parse(input, &Options::default()).unwrap();
            assert_eq!(&carve.write(&tree, &options).unwrap(), input);
        }
    }
}
