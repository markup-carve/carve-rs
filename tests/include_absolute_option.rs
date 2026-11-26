use carve::include::{Cause, Request, fs};
use serde_json::Value;

#[test]
fn absolute_include_paths_respect_the_resolver_switch() {
    let root = std::env::temp_dir().join(format!(
        "carve-rs-absolute-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&root).unwrap();
    let file = root.join("inside.crv");
    std::fs::write(&file, "inside").unwrap();
    let path = file.to_str().unwrap();
    let request = Request {
        path,
        section: None,
        options: &Value::Null,
        from: None,
    };
    assert_eq!(fs::resolver(&root).unwrap()(&request).unwrap().0, b"inside");
    assert!(matches!(
        fs::resolver_with(&root, false).unwrap()(&request),
        Err(Cause::OutsideRoot)
    ));
}
