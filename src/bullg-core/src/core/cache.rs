use std::collections::HashMap;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use std::sync::Arc;

/// A cached value with timestamp for TTL
#[derive(Debug, Clone)]
struct CacheEntry<T> {
    value: T,
    expires_at: Option<Instant>, // None = never expires
}

#[derive(Debug)]
pub struct Cache<K, V> {
    store: RwLock<HashMap<K, CacheEntry<V>>>,
    ttl: Option<Duration>, // default TTL for all entries
}

impl<K, V> Cache<K, V>
where
    K: std::cmp::Eq + std::hash::Hash + Clone,
    V: Clone,
{
    /// Create new cache with optional TTL
    pub fn new(ttl: Option<Duration>) -> Arc<Self> {
        Arc::new(Self {
            store: RwLock::new(HashMap::new()),
            ttl,
        })
    }

    /// Insert value into cache
    pub async fn insert(&self, key: K, value: V) {
        let expires_at = self.ttl.map(|t| Instant::now() + t);
        let entry = CacheEntry { value, expires_at };

        let mut store = self.store.write().await;
        store.insert(key, entry);
    }

    /// Get value if not expired
    pub async fn get(&self, key: &K) -> Option<V> {
        let mut store = self.store.write().await;

        if let Some(entry) = store.get(key) {
            if let Some(expiry) = entry.expires_at {
                if Instant::now() > expiry {
                    // Expired, remove entry
                    store.remove(key);
                    return None;
                }
            }
            return Some(entry.value.clone());
        }
        None
    }

    /// Remove specific key
    pub async fn remove(&self, key: &K) {
        let mut store = self.store.write().await;
        store.remove(key);
    }

    /// Clear entire cache
    pub async fn clear(&self) {
        let mut store = self.store.write().await;
        store.clear();
    }
}
