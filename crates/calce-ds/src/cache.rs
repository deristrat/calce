use std::hash::Hash;
use std::sync::Arc;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};

use arc_swap::ArcSwap;
use dashmap::DashMap;
use rustc_hash::FxBuildHasher;
use tokio::sync::mpsc;

use crate::pubsub::UpdateEvent;

struct Entry {
    current: AtomicU64,
    history: ArcSwap<Vec<f64>>,
}

/// A read result from the cache.
pub struct Snapshot<K> {
    pub key: K,
    pub current: f64,
    pub history: Arc<Vec<f64>>,
}

/// Errors returned by cache mutation operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheError {
    /// The key does not exist in the cache.
    KeyNotFound,
    /// The history index is out of bounds.
    IndexOutOfBounds,
}

impl std::fmt::Display for CacheError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::KeyNotFound => write!(f, "key not found in cache"),
            Self::IndexOutOfBounds => write!(f, "history index out of bounds"),
        }
    }
}

impl std::error::Error for CacheError {}

/// Concurrent time-series cache.
///
/// Each entry stores a current value (`f64`, lock-free via `AtomicU64`) and a
/// history vector (`Vec<f64>`, lock-free reads via `ArcSwap`, copy-on-write for
/// mutations). All read and write operations only acquire the `DashMap` shared
/// (read) lock, so readers never block each other or writers.
///
/// Optionally wired to a [`PubSub`](crate::pubsub::PubSub) system via a tokio
/// channel for change notifications.
pub struct TimeSeriesCache<K: Hash + Eq> {
    data: DashMap<K, Entry, FxBuildHasher>,
    notify_tx: OnceLock<mpsc::Sender<UpdateEvent<K>>>,
}

impl<K: Hash + Eq + Clone + Send + Sync + 'static> TimeSeriesCache<K> {
    #[must_use]
    pub fn new() -> Self {
        Self {
            data: DashMap::with_hasher(FxBuildHasher),
            notify_tx: OnceLock::new(),
        }
    }

    #[must_use]
    pub fn with_notifier(tx: mpsc::Sender<UpdateEvent<K>>) -> Self {
        let lock = OnceLock::new();
        // Fresh OnceLock, set always succeeds.
        let _ = lock.set(tx);
        Self {
            data: DashMap::with_hasher(FxBuildHasher),
            notify_tx: lock,
        }
    }

    /// Wire a notification sender after construction.
    ///
    /// # Errors
    ///
    /// Returns `Err(tx)` if a notifier was already set.
    pub fn set_notifier(
        &self,
        tx: mpsc::Sender<UpdateEvent<K>>,
    ) -> Result<(), mpsc::Sender<UpdateEvent<K>>> {
        self.notify_tx.set(tx)
    }

    /// Insert or replace an entry.
    pub fn insert(&self, key: K, current: f64, history: Vec<f64>) {
        self.data.insert(
            key,
            Entry {
                current: AtomicU64::new(current.to_bits()),
                history: ArcSwap::from(Arc::new(history)),
            },
        );
    }

    /// Bulk-insert from an iterator. Optimised for startup loading.
    pub fn bulk_insert(&self, entries: impl IntoIterator<Item = (K, f64, Vec<f64>)>) {
        for (key, current, history) in entries {
            self.insert(key, current, history);
        }
    }

    /// Update the current value (lock-free).
    ///
    /// # Errors
    ///
    /// Returns `CacheError::KeyNotFound` if the key does not exist.
    pub fn update_current(&self, key: &K, value: f64) -> Result<(), CacheError> {
        let entry = self.data.get(key).ok_or(CacheError::KeyNotFound)?;
        entry.current.store(value.to_bits(), Ordering::Relaxed);
        if let Some(tx) = self.notify_tx.get() {
            let _ = tx.try_send(UpdateEvent::CurrentChanged { key: key.clone() });
        }
        Ok(())
    }

    /// Update a single history entry at `index` (copy-on-write).
    ///
    /// # Errors
    ///
    /// Returns `CacheError::KeyNotFound` if the key does not exist, or
    /// `CacheError::IndexOutOfBounds` if `index >= history.len()`.
    pub fn update_history(&self, key: &K, index: usize, value: f64) -> Result<(), CacheError> {
        let entry = self.data.get(key).ok_or(CacheError::KeyNotFound)?;
        let hist = entry.history.load_full();
        if index >= hist.len() {
            return Err(CacheError::IndexOutOfBounds);
        }
        // Safe: history only grows (append-only), so if `index` is in bounds
        // on this snapshot, it stays in bounds for all future snapshots seen
        // by `rcu`.
        entry.history.rcu(|old| {
            let mut new = old.as_ref().clone();
            new[index] = value;
            Arc::new(new)
        });
        if let Some(tx) = self.notify_tx.get() {
            let _ = tx.try_send(UpdateEvent::HistoryChanged { key: key.clone() });
        }
        Ok(())
    }

    /// Append a value to the history (copy-on-write).
    ///
    /// # Errors
    ///
    /// Returns `CacheError::KeyNotFound` if the key does not exist.
    pub fn append_history(&self, key: &K, value: f64) -> Result<(), CacheError> {
        let entry = self.data.get(key).ok_or(CacheError::KeyNotFound)?;
        entry.history.rcu(|old| {
            let mut new = old.as_ref().clone();
            new.push(value);
            Arc::new(new)
        });
        if let Some(tx) = self.notify_tx.get() {
            let _ = tx.try_send(UpdateEvent::HistoryChanged { key: key.clone() });
        }
        Ok(())
    }

    /// Read the current value for one key.
    #[must_use]
    pub fn get_current(&self, key: &K) -> Option<f64> {
        self.data
            .get(key)
            .map(|e| f64::from_bits(e.current.load(Ordering::Relaxed)))
    }

    /// Read the full history for one key (Arc clone, no memcpy).
    #[must_use]
    pub fn get_history(&self, key: &K) -> Option<Arc<Vec<f64>>> {
        self.data.get(key).map(|e| e.history.load_full())
    }

    /// Read a slice of history `[from..to)` for one key.
    ///
    /// Returns `None` if the key is missing. Clamps indices to actual length.
    #[must_use]
    pub fn get_history_range(&self, key: &K, from: usize, to: usize) -> Option<Vec<f64>> {
        self.data.get(key).map(|e| {
            let hist = e.history.load_full();
            let start = from.min(hist.len());
            let end = to.min(hist.len());
            hist[start..end].to_vec()
        })
    }

    /// Read snapshots for a batch of keys.
    #[must_use]
    pub fn read_batch(&self, keys: &[K]) -> Vec<Snapshot<K>> {
        let mut out = Vec::with_capacity(keys.len());
        for key in keys {
            if let Some(e) = self.data.get(key) {
                out.push(Snapshot {
                    key: key.clone(),
                    current: f64::from_bits(e.current.load(Ordering::Relaxed)),
                    history: e.history.load_full(),
                });
            }
        }
        out
    }

    /// Iterate over all keys, calling `f` for each.
    pub fn for_each_key(&self, mut f: impl FnMut(&K)) {
        for entry in &self.data {
            f(entry.key());
        }
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.data.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    #[must_use]
    pub fn contains_key(&self, key: &K) -> bool {
        self.data.contains_key(key)
    }
}

impl<K: Hash + Eq + Clone + Send + Sync + 'static> Default for TimeSeriesCache<K> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_and_read() {
        let cache = TimeSeriesCache::<u32>::new();
        cache.insert(1, 100.0, vec![10.0, 20.0, 30.0]);

        assert_eq!(cache.get_current(&1), Some(100.0));
        let hist = cache.get_history(&1);
        assert!(hist.is_some());
        assert_eq!(hist.as_deref(), Some(&vec![10.0, 20.0, 30.0]));
    }

    #[test]
    fn missing_key_returns_none() {
        let cache = TimeSeriesCache::<u32>::new();
        assert_eq!(cache.get_current(&42), None);
        assert!(cache.get_history(&42).is_none());
    }

    #[test]
    fn update_current() {
        let cache = TimeSeriesCache::<u32>::new();
        cache.insert(1, 50.0, vec![]);
        assert!(cache.update_current(&1, 75.0).is_ok());
        assert_eq!(cache.get_current(&1), Some(75.0));
    }

    #[test]
    fn update_current_missing_key() {
        let cache = TimeSeriesCache::<u32>::new();
        assert_eq!(cache.update_current(&1, 75.0), Err(CacheError::KeyNotFound));
    }

    #[test]
    fn update_history_cow() {
        let cache = TimeSeriesCache::<u32>::new();
        cache.insert(1, 0.0, vec![1.0, 2.0, 3.0]);

        // Grab a reference before the update
        let before = cache.get_history(&1);

        assert!(cache.update_history(&1, 1, 99.0).is_ok());

        let after = cache.get_history(&1);

        // Old snapshot is unmodified (CoW)
        assert_eq!(before.as_deref(), Some(&vec![1.0, 2.0, 3.0]));
        // New snapshot has the update
        assert_eq!(after.as_deref(), Some(&vec![1.0, 99.0, 3.0]));
    }

    #[test]
    fn update_history_out_of_bounds() {
        let cache = TimeSeriesCache::<u32>::new();
        cache.insert(1, 0.0, vec![1.0, 2.0]);
        assert_eq!(
            cache.update_history(&1, 100, 99.0),
            Err(CacheError::IndexOutOfBounds)
        );
        assert_eq!(cache.get_history(&1).as_deref(), Some(&vec![1.0, 2.0]));
    }

    #[test]
    fn update_history_missing_key() {
        let cache = TimeSeriesCache::<u32>::new();
        assert_eq!(
            cache.update_history(&42, 0, 99.0),
            Err(CacheError::KeyNotFound)
        );
    }

    #[test]
    fn append_history() {
        let cache = TimeSeriesCache::<u32>::new();
        cache.insert(1, 0.0, vec![1.0, 2.0]);
        assert!(cache.append_history(&1, 3.0).is_ok());
        assert_eq!(cache.get_history(&1).as_deref(), Some(&vec![1.0, 2.0, 3.0]));
    }

    #[test]
    fn append_history_missing_key() {
        let cache = TimeSeriesCache::<u32>::new();
        assert_eq!(cache.append_history(&42, 3.0), Err(CacheError::KeyNotFound));
    }

    #[test]
    fn get_history_range() {
        let cache = TimeSeriesCache::<u32>::new();
        cache.insert(1, 0.0, vec![10.0, 20.0, 30.0, 40.0, 50.0]);
        assert_eq!(
            cache.get_history_range(&1, 1, 4),
            Some(vec![20.0, 30.0, 40.0])
        );
        // Clamped range
        assert_eq!(cache.get_history_range(&1, 3, 100), Some(vec![40.0, 50.0]));
    }

    #[test]
    fn bulk_insert() {
        let cache = TimeSeriesCache::<u32>::new();
        let entries: Vec<_> = (0..100)
            .map(|i| (i, f64::from(i), vec![f64::from(i)]))
            .collect();
        cache.bulk_insert(entries);
        assert_eq!(cache.len(), 100);
        assert_eq!(cache.get_current(&50), Some(50.0));
    }

    #[test]
    fn read_batch() {
        let cache = TimeSeriesCache::<u32>::new();
        cache.insert(1, 10.0, vec![1.0]);
        cache.insert(2, 20.0, vec![2.0]);
        cache.insert(3, 30.0, vec![3.0]);

        let snapshots = cache.read_batch(&[1, 3, 99]);
        assert_eq!(snapshots.len(), 2);
        assert_eq!(snapshots[0].key, 1);
        assert_eq!(snapshots[1].key, 3);
    }

    #[test]
    fn len_and_contains() {
        let cache = TimeSeriesCache::<u32>::new();
        assert!(cache.is_empty());
        cache.insert(1, 0.0, vec![]);
        assert_eq!(cache.len(), 1);
        assert!(cache.contains_key(&1));
        assert!(!cache.contains_key(&2));
    }

    #[test]
    fn string_key() {
        let cache = TimeSeriesCache::<String>::new();
        cache.insert("AAPL".to_owned(), 150.0, vec![148.0, 149.0, 150.0]);
        assert_eq!(cache.get_current(&"AAPL".to_owned()), Some(150.0));
    }

    #[test]
    fn tuple_key() {
        let cache = TimeSeriesCache::<(u8, u8)>::new();
        cache.insert((1, 2), 1.05, vec![1.0, 1.02, 1.05]);
        assert_eq!(cache.get_current(&(1, 2)), Some(1.05));
        assert_eq!(cache.get_current(&(2, 1)), None);
    }
}
