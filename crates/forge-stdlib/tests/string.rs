use forge_stdlib::string::{
    char_count, contains, ends_with, index_of, is_empty, join, pad_left, pad_right, repeat,
    replace, replace_first, split, starts_with, to_lower, to_upper, trim, trim_end, trim_start,
};

#[test]
fn trim_variants_trim_whitespace() {
    assert_eq!(trim("  a  "), "a");
    assert_eq!(trim_start("  a"), "a");
    assert_eq!(trim_end("a  "), "a");
}

#[test]
fn split_and_join_roundtrip() {
    let parts = split("a,b,c", ",");
    assert_eq!(
        parts,
        vec!["a".to_string(), "b".to_string(), "c".to_string()]
    );
    assert_eq!(join(parts, ","), "a,b,c");
}

#[test]
fn replace_helpers() {
    assert_eq!(replace("hello", "l", "r"), "herro");
    assert_eq!(replace_first("hello", "l", "r"), "herlo");
}

#[test]
fn casing_and_contains() {
    assert!(starts_with("rust", "ru"));
    assert!(ends_with("rust", "st"));
    assert!(contains("forge", "org"));
    assert_eq!(to_upper("forge"), "FORGE");
    assert_eq!(to_lower("FORGE"), "forge");
}

#[test]
fn index_and_length() {
    assert_eq!(index_of("forge", "rg"), Some(2));
    assert_eq!(index_of("forge", "x"), None);
    assert_eq!(char_count("𝄞"), 1);
    assert!(is_empty(""));
}

#[test]
fn repeat_and_pad_length() {
    assert_eq!(repeat("ab", 3).unwrap(), "ababab");
    assert_eq!(pad_left("fo", 4, "0").unwrap(), "00fo");
    assert_eq!(pad_right("fo", 4, "0").unwrap(), "fo00");
}
