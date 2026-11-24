//! markup-carve/carve#6512: a description list opened on a task item's marker line.

use carve::api::{Carve, Options, WriteOptions};

#[test]
fn a_term_behind_a_task_box_keeps_its_entries_at_its_own_column() {
    let cases = [
        "- [ ] :: A long term\n         continued\n      : The description.\n",
        "- [ ] :: t\n      : d\n",
        "- [x] :: t\n      : d\n      : e\n",
        ". [ ] :: t\n      : d\n",
        "- [ ] :: t\n      : d\n      :: u\n      : e\n",
        "- [ ] :: t\n      : - x\n",
        "> - [ ] :: t\n>       : d\n",
        "- :: t\n  : d\n",
    ];
    let carve = Carve::new();
    for input in cases {
        let options = WriteOptions::default();
        assert_eq!(carve.format(input, &options).unwrap(), input, "{input:?}");
        let tree = carve.parse(input, &Options::default()).unwrap();
        assert_eq!(carve.write(&tree, &options).unwrap(), input, "{input:?}");
    }
}
