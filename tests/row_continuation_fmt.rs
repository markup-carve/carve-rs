use carve::api::{Carve, WriteOptions};

#[test]
fn empty_continuation_in_task_item_survives_second_format() {
    let carve = Carve::new();
    let opts = WriteOptions::default();
    let input = "- [ ] ||\n      + |\n";
    let expected = "- [ ] | |\n  + |\n";
    let once = carve.format(input, &opts).unwrap();
    assert_eq!(once, expected);
    assert_eq!(carve.format(once.as_str(), &opts).unwrap(), expected);
}

#[test]
fn continuation_facts_follow_the_marker_in_changing_prefixes() {
    let carve = Carve::new();
    let opts = WriteOptions::default();
    for input in [
        "- | a |\n  + |\n",
        "> - [ ] | a |\n>   + |\n",
        "- [ ] | a |\n  + |\n",
        "- [ ] | 0 |\n  + y |\n",
    ] {
        let once = carve.format(input, &opts).unwrap();
        assert!(once.contains("+ "), "{input:?}: {once:?}");
        let twice = carve.format(once.as_str(), &opts).unwrap();
        assert_eq!(twice, once, "{input:?}");
    }
}

#[test]
fn plus_in_footnote_label_does_not_steal_table_marker() {
    let carve = Carve::new();
    let opts = WriteOptions::default();
    let input = "[^a+b]: | x |\n        + y |\n\nRef [^a+b].\n";
    let expected = "[^a+b]: | x |\n  + y |\n\nRef [^a+b].\n";
    let once = carve.format(input, &opts).unwrap();
    assert_eq!(once, expected);
    assert_eq!(carve.format(once.as_str(), &opts).unwrap(), expected);
}

#[test]
fn pipe_in_table_attribute_does_not_steal_row_marker() {
    let carve = Carve::new();
    let opts = WriteOptions::default();
    for input in [
        "{title=\"p|q\"}\n| a |\n+ b |\n",
        "{title=\"p|q\"}\n| a |\n+ |\n",
    ] {
        let once = carve.format(input, &opts).unwrap();
        assert!(once.contains("+ "), "{input:?}: {once:?}");
        assert_eq!(carve.format(once.as_str(), &opts).unwrap(), once);
    }
}

#[test]
fn caption_text_does_not_supply_table_continuation_facts() {
    let carve = Carve::new();
    let opts = WriteOptions::default();
    for input in [
        "| a |\n^ x + y\n",
        "| a |\n^ x + | y\n",
        "| a |\n+ b |\n^ x + y\n",
        "> | a |\n> ^ x + y\n",
    ] {
        let once = carve.format(input, &opts).unwrap();
        assert_eq!(once, input, "{input:?}");
        assert_eq!(carve.format(once.as_str(), &opts).unwrap(), once);
    }
    let input = "|= H |= J |\n+ |\n^ a | b\n";
    let expected = "|= H |= J |\n+ | |\n^ a | b\n";
    let once = carve.format(input, &opts).unwrap();
    assert_eq!(once, expected);
    assert_eq!(carve.format(once.as_str(), &opts).unwrap(), expected);
}

#[test]
fn fuzz_caption_input_is_stable() {
    let carve = Carve::new();
    let opts = WriteOptions::default();
    // The original fmt fuzz input for #4550, after UTF-8 decoding.
    let input = "|= Name\0=={>}       |\n^ Table #\x01\0+\0: Drinks";
    let once = carve.format(input, &opts).unwrap();
    assert!(!once.contains("+ |"), "{once:?}");
    assert_eq!(carve.format(once.as_str(), &opts).unwrap(), once);
}
