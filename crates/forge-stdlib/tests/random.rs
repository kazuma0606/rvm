use forge_stdlib::random::{random_choice, random_float, random_int, seed_random, shuffle};

#[test]
fn random_int_in_range() {
    let value = random_int(1, 5).expect("should be valid");
    assert!(value >= 1 && value <= 5);
}

#[test]
fn random_choice_from_list() {
    let choice = random_choice(vec!["a", "b", "c"]).expect("should choose");
    assert!(["a", "b", "c"].contains(&choice));
}

#[test]
fn shuffle_preserves_elements() {
    let original = vec![1, 2, 3];
    let shuffled = shuffle(original.clone());
    assert_eq!(shuffled.len(), original.len());
}

#[test]
fn seed_random_reproducible() {
    seed_random(42);
    let a = random_int(1, 100).unwrap();
    seed_random(42);
    let b = random_int(1, 100).unwrap();
    assert_eq!(a, b);
}
