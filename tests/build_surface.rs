#[path = "../build/surface.rs"]
mod surface;

#[test]
fn classifies_tags_and_operations() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/generated-surface.txt"
    );

    assert_eq!(
        surface::generated_cfgs(path),
        [
            ("generated_tag", "monitors".to_owned()),
            ("generated_op", "monitors.list".to_owned()),
            ("generated_op", "users.accounts.list".to_owned()),
        ]
    );
}

#[test]
fn missing_surface_emits_no_cfgs() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/absent-generated-surface.txt"
    );

    assert!(surface::generated_cfgs(path).is_empty());
}
