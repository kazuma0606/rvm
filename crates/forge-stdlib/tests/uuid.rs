use forge_stdlib::uuid::{is_uuid, uuid_v4};

#[test]
fn uuid_generates_v4() {
    let value = uuid_v4();
    assert!(is_uuid(&value));
}

#[test]
fn is_uuid_rejects_invalid() {
    assert!(!is_uuid("not-a-uuid"));
}
