use uuid::Uuid;

pub fn uuid_v4() -> String {
    Uuid::new_v4().to_string()
}

pub fn is_uuid(value: impl AsRef<str>) -> bool {
    Uuid::parse_str(value.as_ref()).is_ok()
}
