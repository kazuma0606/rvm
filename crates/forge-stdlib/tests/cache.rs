use std::cell::Cell;
use std::thread::sleep;
use std::time::Duration;

use forge_stdlib::cache::{memoize, Cache};

#[test]
fn test_cache_get_or_set_caches_result() {
    let mut cache = Cache::new(60, 2);
    let counter = Cell::new(0);
    let result = cache
        .get_or_set("value", || {
            counter.set(counter.get() + 1);
            Ok(counter.get())
        })
        .expect("should load");
    assert_eq!(result, 1);
    let cached = cache
        .get_or_set("value", || panic!("should not run again"))
        .expect("should reuse");
    assert_eq!(cached, 1);
}

#[test]
fn test_cache_ttl_expires_entry() {
    let mut cache = Cache::new(1, 1);
    cache.set("temp", 42);
    sleep(Duration::from_millis(1100));
    assert!(cache.get("temp").is_none());
}

#[test]
fn test_cache_invalidate_removes_entry() {
    let mut cache = Cache::new(60, 1);
    cache.set("key", 10);
    assert!(cache.invalidate("key"));
    assert!(cache.get("key").is_none());
}

#[test]
fn test_memoize_does_not_call_fn_twice() {
    let mut cache = Cache::new(60, 2);
    let counter = Cell::new(0);
    let first = memoize(&mut cache, "once", || {
        counter.set(counter.get() + 1);
        Ok(counter.get())
    })
    .expect("memoize");
    assert_eq!(first, 1);
    let second = memoize(&mut cache, "once", || panic!("should be cached")).unwrap();
    assert_eq!(second, 1);
}

#[test]
fn test_cache_key_fn_custom_key() {
    let mut cache = Cache::new(60, 2);
    let user_id = "user42";
    let key = format!("user:{}:profile", user_id);
    cache.set(&key, "profile_data".to_string());
    assert_eq!(cache.get(&key).unwrap(), "profile_data".to_string());
}
