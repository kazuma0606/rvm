use once_cell::sync::Lazy;
use rand::prelude::{SliceRandom, StdRng};
use rand::{Rng, SeedableRng};
use std::sync::Mutex;

static RNG: Lazy<Mutex<StdRng>> = Lazy::new(|| Mutex::new(StdRng::from_entropy()));

fn with_rng<T>(f: impl FnOnce(&mut StdRng) -> T) -> T {
    let mut guard = RNG.lock().expect("rng lock");
    f(&mut guard)
}

pub fn random_int(min: i64, max: i64) -> Result<i64, String> {
    if min > max {
        return Err("min must be less than or equal to max".to_string());
    }
    Ok(with_rng(|rng| rng.gen_range(min..=max)))
}

pub fn random_float() -> f64 {
    with_rng(|rng| rng.gen())
}

pub fn random_choice<T: Clone>(list: Vec<T>) -> Result<T, String> {
    if list.is_empty() {
        return Err("list cannot be empty".to_string());
    }
    let item = with_rng(|rng| {
        let idx = rng.gen_range(0..list.len());
        list[idx].clone()
    });
    Ok(item)
}

pub fn shuffle<T: Clone>(mut list: Vec<T>) -> Vec<T> {
    with_rng(|rng| list.shuffle(rng));
    list
}

pub fn seed_random(seed: u64) {
    let mut guard = RNG.lock().expect("rng lock");
    *guard = StdRng::seed_from_u64(seed);
}
