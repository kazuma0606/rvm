use forge_stdlib::regex::{
    regex_capture, regex_find, regex_find_all, regex_match, regex_replace, regex_replace_first,
};

#[test]
fn regex_match_digit_pattern() {
    assert!(regex_match(r"^\d+$", "12345").unwrap());
    assert!(!regex_match(r"^\d+$", "12a45").unwrap());
}

#[test]
fn regex_find_all_returns_results() {
    let results = regex_find_all(r"\w+", "a1 b2").unwrap();
    assert_eq!(results, vec!["a1", "b2"]);
}

#[test]
fn regex_capture_groups() {
    let caps = regex_capture(r"(\d{4})-(\d{2})", "2026-04").unwrap();
    let caps = caps.expect("should capture groups");
    assert_eq!(caps[1], "2026");
}

#[test]
fn regex_replace_all() {
    let replaced = regex_replace(r"\d", "a1b2", "X").unwrap();
    assert_eq!(replaced, "aXbX");
    let first = regex_replace_first(r"\d", "a1b2", "Y").unwrap();
    assert_eq!(first, "aYb2");
}
