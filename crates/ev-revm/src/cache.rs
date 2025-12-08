//! Caching layer for EVM database operations.
//!
//! This module provides a bytecode cache wrapper for database operations.
//! Contract bytecode is immutable after deployment, making it ideal for caching.

use alloy_primitives::{Address, B256, U256};
use reth_revm::revm::{
    context_interface::Database,
    state::{AccountInfo, Bytecode},
};
use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

/// Thread-safe bytecode cache using LRU eviction strategy.
///
/// Contract bytecode is immutable after deployment, making it an ideal
/// candidate for caching. This cache stores bytecode by its code hash,
/// avoiding repeated database lookups for frequently-called contracts.
#[derive(Debug)]
pub struct BytecodeCache {
    /// The actual cache storage, protected by a RwLock for thread-safety.
    /// Values are Arc'd to allow cheap cloning when returning cached bytecode.
    cache: RwLock<LruCache>,
    /// Maximum number of entries before eviction
    max_entries: usize,
}

/// Simple LRU cache implementation
#[derive(Debug)]
struct LruCache {
    /// Map from code hash to (bytecode, access_order)
    entries: HashMap<B256, (Arc<Bytecode>, u64)>,
    /// Counter for tracking access order
    access_counter: u64,
}

impl LruCache {
    fn new() -> Self {
        Self {
            entries: HashMap::new(),
            access_counter: 0,
        }
    }

    fn get(&mut self, key: &B256) -> Option<Arc<Bytecode>> {
        if let Some((bytecode, order)) = self.entries.get_mut(key) {
            self.access_counter += 1;
            *order = self.access_counter;
            Some(Arc::clone(bytecode))
        } else {
            None
        }
    }

    fn insert(&mut self, key: B256, value: Bytecode, max_entries: usize) {
        // Evict oldest entries if at capacity
        if self.entries.len() >= max_entries {
            self.evict_oldest(max_entries / 2);
        }

        self.access_counter += 1;
        self.entries
            .insert(key, (Arc::new(value), self.access_counter));
    }

    fn evict_oldest(&mut self, count: usize) {
        if count == 0 || self.entries.is_empty() {
            return;
        }

        // Collect entries sorted by access order (oldest first)
        let mut entries: Vec<_> = self.entries.iter().map(|(k, (_, o))| (*k, *o)).collect();
        entries.sort_by_key(|(_, order)| *order);

        // Remove the oldest entries
        for (key, _) in entries.into_iter().take(count) {
            self.entries.remove(&key);
        }
    }

    fn len(&self) -> usize {
        self.entries.len()
    }
}

impl BytecodeCache {
    /// Creates a new bytecode cache with the specified maximum number of entries.
    ///
    /// # Arguments
    /// * `max_entries` - Maximum number of bytecode entries to cache before eviction
    ///
    /// # Panics
    /// Panics if `max_entries` is 0.
    pub fn new(max_entries: usize) -> Self {
        assert!(max_entries > 0, "max_entries must be greater than 0");
        Self {
            cache: RwLock::new(LruCache::new()),
            max_entries,
        }
    }

    /// Creates a new bytecode cache with default capacity (10,000 entries).
    ///
    /// This is suitable for most use cases, providing cache for approximately
    /// 10,000 unique contracts.
    pub fn with_default_capacity() -> Self {
        Self::new(10_000)
    }

    /// Retrieves bytecode from the cache if present.
    ///
    /// Returns `None` if the bytecode is not cached.
    pub fn get(&self, code_hash: &B256) -> Option<Bytecode> {
        let mut cache = self.cache.write().expect("cache lock poisoned");
        cache.get(code_hash).map(|arc| (*arc).clone())
    }

    /// Inserts bytecode into the cache.
    ///
    /// If the cache is at capacity, older entries will be evicted using LRU policy.
    pub fn insert(&self, code_hash: B256, bytecode: Bytecode) {
        // Don't cache empty bytecode
        if bytecode.is_empty() {
            return;
        }

        let mut cache = self.cache.write().expect("cache lock poisoned");
        cache.insert(code_hash, bytecode, self.max_entries);
    }

    /// Returns the current number of cached entries.
    pub fn len(&self) -> usize {
        self.cache.read().expect("cache lock poisoned").len()
    }

    /// Returns true if the cache is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Clears all entries from the cache.
    pub fn clear(&self) {
        let mut cache = self.cache.write().expect("cache lock poisoned");
        cache.entries.clear();
        cache.access_counter = 0;
    }
}

impl Default for BytecodeCache {
    fn default() -> Self {
        Self::with_default_capacity()
    }
}

// ============================================================================
// Cached Database
// ============================================================================

/// A database wrapper that adds bytecode caching to any underlying database.
///
/// Contract bytecode is immutable after deployment, so caching provides
/// significant performance benefits for frequently-called contracts.
///
/// # Example
///
/// ```ignore
/// use ev_revm::cache::{BytecodeCache, CachedDatabase};
/// use std::sync::Arc;
///
/// let inner_db = StateProviderDatabase::new(&state_provider);
/// let bytecode_cache = Arc::new(BytecodeCache::with_default_capacity());
/// let cached_db = CachedDatabase::new(inner_db, bytecode_cache);
/// ```
#[derive(Debug)]
pub struct CachedDatabase<DB> {
    /// The underlying database
    inner: DB,
    /// Shared bytecode cache
    bytecode_cache: Arc<BytecodeCache>,
}

impl<DB> CachedDatabase<DB> {
    /// Creates a new cached database wrapper.
    ///
    /// # Arguments
    /// * `inner` - The underlying database to wrap
    /// * `bytecode_cache` - Shared bytecode cache (can be shared across multiple databases)
    pub fn new(inner: DB, bytecode_cache: Arc<BytecodeCache>) -> Self {
        Self {
            inner,
            bytecode_cache,
        }
    }

    /// Returns a reference to the underlying database.
    pub fn inner(&self) -> &DB {
        &self.inner
    }

    /// Returns a mutable reference to the underlying database.
    pub fn inner_mut(&mut self) -> &mut DB {
        &mut self.inner
    }

    /// Consumes the wrapper and returns the underlying database.
    pub fn into_inner(self) -> DB {
        self.inner
    }

    /// Returns a reference to the bytecode cache.
    pub fn bytecode_cache(&self) -> &Arc<BytecodeCache> {
        &self.bytecode_cache
    }

    /// Returns a reference to the bytecode cache (alias for backwards compatibility).
    pub fn cache(&self) -> &Arc<BytecodeCache> {
        &self.bytecode_cache
    }
}

impl<DB: Database> Database for CachedDatabase<DB> {
    type Error = DB::Error;

    fn basic(&mut self, address: Address) -> Result<Option<AccountInfo>, Self::Error> {
        self.inner.basic(address)
    }

    fn code_by_hash(&mut self, code_hash: B256) -> Result<Bytecode, Self::Error> {
        // Check bytecode cache first
        if let Some(cached) = self.bytecode_cache.get(&code_hash) {
            return Ok(cached);
        }

        // Cache miss - fetch from underlying database
        let bytecode = self.inner.code_by_hash(code_hash)?;

        // Cache for future use
        self.bytecode_cache.insert(code_hash, bytecode.clone());

        Ok(bytecode)
    }

    fn storage(&mut self, address: Address, index: U256) -> Result<U256, Self::Error> {
        self.inner.storage(address, index)
    }

    fn block_hash(&mut self, number: u64) -> Result<B256, Self::Error> {
        self.inner.block_hash(number)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_primitives::bytes;

    #[test]
    fn test_bytecode_cache_basic_operations() {
        let cache = BytecodeCache::new(100);

        // Create a test bytecode
        let code_hash = B256::repeat_byte(0x42);
        let bytecode = Bytecode::new_raw(bytes!("6080604052").into());

        // Initially not in cache
        assert!(cache.get(&code_hash).is_none());

        // Insert into cache
        cache.insert(code_hash, bytecode.clone());

        // Now should be retrievable
        let cached = cache.get(&code_hash).expect("should be cached");
        assert_eq!(cached.bytes(), bytecode.bytes());
    }

    #[test]
    fn test_bytecode_cache_empty_bytecode_not_cached() {
        let cache = BytecodeCache::new(100);
        let code_hash = B256::repeat_byte(0x42);
        let empty_bytecode = Bytecode::new();

        cache.insert(code_hash, empty_bytecode);

        // Empty bytecode should not be cached
        assert!(cache.get(&code_hash).is_none());
    }

    #[test]
    fn test_bytecode_cache_lru_eviction() {
        let cache = BytecodeCache::new(3);

        // Insert 3 entries
        for i in 0..3u8 {
            let code_hash = B256::repeat_byte(i);
            let bytecode = Bytecode::new_raw(vec![0x60, i].into());
            cache.insert(code_hash, bytecode);
        }

        assert_eq!(cache.len(), 3);

        // Access entry 0 to make it recently used
        cache.get(&B256::repeat_byte(0));

        // Insert a 4th entry, should evict entry 1 (least recently used)
        let code_hash_3 = B256::repeat_byte(3);
        cache.insert(code_hash_3, Bytecode::new_raw(vec![0x60, 3].into()));

        // Entry 0 should still be present (was accessed)
        assert!(cache.get(&B256::repeat_byte(0)).is_some());
        // Entry 3 should be present (just added)
        assert!(cache.get(&B256::repeat_byte(3)).is_some());
    }

    #[test]
    fn test_bytecode_cache_clear() {
        let cache = BytecodeCache::new(100);

        // Insert some entries
        for i in 0..5u8 {
            let code_hash = B256::repeat_byte(i);
            let bytecode = Bytecode::new_raw(vec![0x60, i].into());
            cache.insert(code_hash, bytecode);
        }

        assert_eq!(cache.len(), 5);

        cache.clear();

        assert!(cache.is_empty());
    }

    #[test]
    #[should_panic(expected = "max_entries must be greater than 0")]
    fn test_bytecode_cache_zero_capacity_panics() {
        BytecodeCache::new(0);
    }

    // Mock database for testing CachedDatabase
    #[derive(Debug, Default)]
    struct MockDatabase {
        bytecodes: HashMap<B256, Bytecode>,
        storage: HashMap<(Address, U256), U256>,
        code_by_hash_call_count: std::cell::Cell<usize>,
    }

    impl MockDatabase {
        fn new() -> Self {
            Self::default()
        }

        fn with_bytecode(mut self, code_hash: B256, bytecode: Bytecode) -> Self {
            self.bytecodes.insert(code_hash, bytecode);
            self
        }

        fn code_by_hash_call_count(&self) -> usize {
            self.code_by_hash_call_count.get()
        }
    }

    impl Database for MockDatabase {
        type Error = std::convert::Infallible;

        fn basic(&mut self, _address: Address) -> Result<Option<AccountInfo>, Self::Error> {
            Ok(None)
        }

        fn code_by_hash(&mut self, code_hash: B256) -> Result<Bytecode, Self::Error> {
            self.code_by_hash_call_count
                .set(self.code_by_hash_call_count.get() + 1);
            Ok(self.bytecodes.get(&code_hash).cloned().unwrap_or_default())
        }

        fn storage(&mut self, address: Address, index: U256) -> Result<U256, Self::Error> {
            Ok(self
                .storage
                .get(&(address, index))
                .copied()
                .unwrap_or(U256::ZERO))
        }

        fn block_hash(&mut self, _number: u64) -> Result<B256, Self::Error> {
            Ok(B256::ZERO)
        }
    }

    #[test]
    fn test_cached_database_cache_hit() {
        let code_hash = B256::repeat_byte(0x42);
        let bytecode = Bytecode::new_raw(bytes!("6080604052").into());

        let mock_db = MockDatabase::new().with_bytecode(code_hash, bytecode.clone());
        let cache = Arc::new(BytecodeCache::new(100));
        let mut cached_db = CachedDatabase::new(mock_db, cache);

        // First call - cache miss, should hit database
        let result1 = cached_db.code_by_hash(code_hash).unwrap();
        assert_eq!(result1.bytes(), bytecode.bytes());
        assert_eq!(cached_db.inner().code_by_hash_call_count(), 1);

        // Second call - cache hit, should NOT hit database
        let result2 = cached_db.code_by_hash(code_hash).unwrap();
        assert_eq!(result2.bytes(), bytecode.bytes());
        assert_eq!(cached_db.inner().code_by_hash_call_count(), 1); // Still 1!
    }

    #[test]
    fn test_cached_database_delegates_other_methods() {
        let mock_db = MockDatabase::new();
        let cache = Arc::new(BytecodeCache::new(100));
        let mut cached_db = CachedDatabase::new(mock_db, cache);

        // These should delegate to inner database
        assert!(cached_db.basic(Address::ZERO).unwrap().is_none());
        assert_eq!(
            cached_db.storage(Address::ZERO, U256::ZERO).unwrap(),
            U256::ZERO
        );
        assert_eq!(cached_db.block_hash(0).unwrap(), B256::ZERO);
    }
}
