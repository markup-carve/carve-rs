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
