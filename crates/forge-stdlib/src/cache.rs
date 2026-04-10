use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

struct CacheEntry<T> {
    value: T,
    stored: Instant,
}

/// Simple in-memory cache with TTL and capacity control.
pub struct Cache<T> {
    ttl: Duration,
    max_entries: usize,
    entries: HashMap<String, CacheEntry<T>>,
    order: VecDeque<String>,
}

impl<T: Clone> Cache<T> {
    pub fn new(ttl_seconds: u64, max_entries: usize) -> Self {
        Self {
            ttl: Duration::from_secs(ttl_seconds),
            max_entries: max_entries.max(1),
            entries: HashMap::new(),
            order: VecDeque::new(),
        }
    }

    pub fn get_or_set<F>(&mut self, key: impl AsRef<str>, loader: F) -> Result<T, String>
    where
        F: Fn() -> Result<T, String>,
    {
        let key = key.as_ref().to_string();
        if let Some(value) = self.get(&key) {
            return Ok(value);
        }
        let value = loader()?;
        self.set(&key, value.clone());
        Ok(value)
    }

    pub fn get(&mut self, key: impl AsRef<str>) -> Option<T> {
        self.prune_expired();
        let key_str = key.as_ref().to_string();
        let (found, expired) = match self.entries.get(&key_str) {
            Some(entry) => {
                if self.is_expired(entry) {
                    (false, true)
                } else {
                    (true, false)
                }
            }
            None => (false, false),
        };
        if found {
            let value = self.entries[&key_str].value.clone();
            self.bump_order(&key_str);
            return Some(value);
        }
        if expired {
            self.remove_key(&key_str);
        }
        None
    }

    pub fn set(&mut self, key: impl AsRef<str>, value: T) {
        self.prune_expired();
        let key = key.as_ref().to_string();
        if self.entries.contains_key(&key) {
            self.remove_key(&key);
        }
        self.entries.insert(
            key.clone(),
            CacheEntry {
                value,
                stored: Instant::now(),
            },
        );
        self.order.push_back(key.clone());
        self.trim_capacity();
    }

    pub fn invalidate(&mut self, key: impl AsRef<str>) -> bool {
        let key = key.as_ref();
        self.remove_key(key)
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.order.clear();
    }

    fn prune_expired(&mut self) {
        let expired: Vec<String> = self
            .entries
            .iter()
            .filter_map(|(key, entry)| {
                if self.is_expired(entry) {
                    Some(key.clone())
                } else {
                    None
                }
            })
            .collect();
        for key in expired {
            self.remove_key(&key);
        }
    }

    fn is_expired(&self, entry: &CacheEntry<T>) -> bool {
        self.ttl > Duration::ZERO && entry.stored.elapsed() >= self.ttl
    }

    fn remove_key(&mut self, key: impl AsRef<str>) -> bool {
        let key = key.as_ref();
        let existed = self.entries.remove(key).is_some();
        if existed {
            self.order.retain(|existing| existing != key);
        }
        existed
    }

    fn bump_order(&mut self, key: impl AsRef<str>) {
        let key = key.as_ref();
        self.order.retain(|existing| existing != key);
        self.order.push_back(key.to_string());
    }

    fn trim_capacity(&mut self) {
        while self.entries.len() > self.max_entries {
            if let Some(old_key) = self.order.pop_front() {
                self.entries.remove(&old_key);
            }
        }
    }
}

/// Helper invoked by transpiled `@memoize` decorator.
pub fn memoize<T, F>(cache: &mut Cache<T>, key: impl AsRef<str>, loader: F) -> Result<T, String>
where
    T: Clone,
    F: Fn() -> Result<T, String>,
{
    cache.get_or_set(key, loader)
}

/// Helper invoked by transpiled `@cache` decorator (TTL-aware).
pub fn cache<T, F>(cache: &mut Cache<T>, key: impl AsRef<str>, loader: F) -> Result<T, String>
where
    T: Clone,
    F: Fn() -> Result<T, String>,
{
    memoize(cache, key, loader)
}
