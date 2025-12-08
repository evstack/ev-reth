//! Caching layer for EVM database operations.
//!
//! This module provides cache wrappers for database operations:
//! - `BytecodeCache`: Caches immutable contract bytecode
//! - `PinnedStorageCache`: Pins storage slots for specific contracts in RAM

use alloy_primitives::{Address, B256, U256};
use reth_revm::revm::{
    context_interface::Database,
    state::{AccountInfo, Bytecode},
};
use std::{
    collections::{HashMap, HashSet},
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
        self.entries.insert(key, (Arc::new(value), self.access_counter));
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
// Pinned Storage Cache
// ============================================================================

/// RAM-pinned storage cache for hot contracts.
///
/// This cache stores storage slots for explicitly configured contracts in RAM,
/// providing fast access for frequently-accessed contracts like DEXes, bridges,
/// or popular tokens.
///
/// Unlike the bytecode cache which uses LRU eviction, pinned storage is permanent
/// for the duration of the node's runtime - the configured contracts are always
/// kept in RAM.
#[derive(Debug)]
pub struct PinnedStorageCache {
    /// Set of contract addresses that should be pinned
    pinned_addresses: HashSet<Address>,
    /// Storage slots for pinned contracts: address -> (slot -> value)
    storage: RwLock<HashMap<Address, HashMap<U256, U256>>>,
}

impl PinnedStorageCache {
    /// Creates a new pinned storage cache for the given contract addresses.
    ///
    /// Only storage reads/writes for these addresses will be cached.
    pub fn new(pinned_addresses: Vec<Address>) -> Self {
        let addresses: HashSet<_> = pinned_addresses.into_iter().collect();
        let storage = addresses.iter().map(|addr| (*addr, HashMap::new())).collect();

        Self {
            pinned_addresses: addresses,
            storage: RwLock::new(storage),
        }
    }

    /// Creates an empty cache with no pinned contracts.
    pub fn empty() -> Self {
        Self {
            pinned_addresses: HashSet::new(),
            storage: RwLock::new(HashMap::new()),
        }
    }

    /// Returns true if the given address is configured for pinning.
    #[inline]
    pub fn is_pinned(&self, address: &Address) -> bool {
        self.pinned_addresses.contains(address)
    }

    /// Returns the set of pinned addresses.
    pub fn pinned_addresses(&self) -> &HashSet<Address> {
        &self.pinned_addresses
    }

    /// Retrieves a storage value from the cache.
    ///
    /// Returns `None` if:
    /// - The address is not a pinned contract
    /// - The slot has not been cached yet
    pub fn get_storage(&self, address: &Address, slot: &U256) -> Option<U256> {
        if !self.is_pinned(address) {
            return None;
        }

        let storage = self.storage.read().expect("storage lock poisoned");
        storage.get(address)?.get(slot).copied()
    }

    /// Stores a storage value in the cache.
    ///
    /// Only stores if the address is a pinned contract.
    pub fn set_storage(&self, address: Address, slot: U256, value: U256) {
        if !self.is_pinned(&address) {
            return;
        }

        let mut storage = self.storage.write().expect("storage lock poisoned");
        storage.entry(address).or_default().insert(slot, value);
    }

    /// Returns the number of cached storage slots for a given address.
    pub fn slot_count(&self, address: &Address) -> usize {
        self.storage
            .read()
            .expect("storage lock poisoned")
            .get(address)
            .map(|slots| slots.len())
            .unwrap_or(0)
    }

    /// Returns the total number of cached storage slots across all contracts.
    pub fn total_slot_count(&self) -> usize {
        self.storage
            .read()
            .expect("storage lock poisoned")
            .values()
            .map(|slots| slots.len())
            .sum()
    }

    /// Clears all cached storage for a specific address.
    pub fn clear_address(&self, address: &Address) {
        if let Some(slots) = self
            .storage
            .write()
            .expect("storage lock poisoned")
            .get_mut(address)
        {
            slots.clear();
        }
    }

    /// Clears all cached storage.
    pub fn clear(&self) {
        let mut storage = self.storage.write().expect("storage lock poisoned");
        for slots in storage.values_mut() {
            slots.clear();
        }
    }
}

impl Default for PinnedStorageCache {
    fn default() -> Self {
        Self::empty()
    }
}

// ============================================================================
// Cached Database
// ============================================================================

/// A database wrapper that adds bytecode and storage caching to any underlying database.
///
/// This wrapper provides two levels of caching:
/// - **Bytecode caching**: Caches immutable contract bytecode by code hash
/// - **Pinned storage**: RAM-pins storage slots for explicitly configured contracts
///
/// # Example
///
/// ```ignore
/// use ev_revm::cache::{BytecodeCache, PinnedStorageCache, CachedDatabase};
/// use std::sync::Arc;
///
/// let inner_db = StateProviderDatabase::new(&state_provider);
/// let bytecode_cache = Arc::new(BytecodeCache::with_default_capacity());
/// let pinned_storage = Arc::new(PinnedStorageCache::new(vec![uniswap_address, usdc_address]));
/// let cached_db = CachedDatabase::with_pinned_storage(inner_db, bytecode_cache, pinned_storage);
/// ```
#[derive(Debug)]
pub struct CachedDatabase<DB> {
    /// The underlying database
    inner: DB,
    /// Shared bytecode cache
    bytecode_cache: Arc<BytecodeCache>,
    /// Optional pinned storage cache for hot contracts
    pinned_storage: Option<Arc<PinnedStorageCache>>,
}

impl<DB> CachedDatabase<DB> {
    /// Creates a new cached database wrapper with bytecode caching only.
    ///
    /// # Arguments
    /// * `inner` - The underlying database to wrap
    /// * `bytecode_cache` - Shared bytecode cache (can be shared across multiple databases)
    pub fn new(inner: DB, bytecode_cache: Arc<BytecodeCache>) -> Self {
        Self {
            inner,
            bytecode_cache,
            pinned_storage: None,
        }
    }

    /// Creates a new cached database wrapper with both bytecode and pinned storage caching.
    ///
    /// # Arguments
    /// * `inner` - The underlying database to wrap
    /// * `bytecode_cache` - Shared bytecode cache
    /// * `pinned_storage` - Shared pinned storage cache for hot contracts
    pub fn with_pinned_storage(
        inner: DB,
        bytecode_cache: Arc<BytecodeCache>,
        pinned_storage: Arc<PinnedStorageCache>,
    ) -> Self {
        Self {
            inner,
            bytecode_cache,
            pinned_storage: Some(pinned_storage),
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

    /// Returns a reference to the pinned storage cache, if configured.
    pub fn pinned_storage(&self) -> Option<&Arc<PinnedStorageCache>> {
        self.pinned_storage.as_ref()
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
        // Check pinned storage cache first
        if let Some(pinned) = &self.pinned_storage {
            if let Some(value) = pinned.get_storage(&address, &index) {
                return Ok(value);
            }
        }

        // Cache miss or not pinned - fetch from underlying database
        let value = self.inner.storage(address, index)?;

        // Cache for future use if this is a pinned contract
        if let Some(pinned) = &self.pinned_storage {
            pinned.set_storage(address, index, value);
        }

        Ok(value)
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
        storage_call_count: std::cell::Cell<usize>,
    }

    impl MockDatabase {
        fn new() -> Self {
            Self::default()
        }

        fn with_bytecode(mut self, code_hash: B256, bytecode: Bytecode) -> Self {
            self.bytecodes.insert(code_hash, bytecode);
            self
        }

        fn with_storage(mut self, address: Address, slot: U256, value: U256) -> Self {
            self.storage.insert((address, slot), value);
            self
        }

        fn code_by_hash_call_count(&self) -> usize {
            self.code_by_hash_call_count.get()
        }

        fn storage_call_count(&self) -> usize {
            self.storage_call_count.get()
        }
    }

    impl Database for MockDatabase {
        type Error = std::convert::Infallible;

        fn basic(&mut self, _address: Address) -> Result<Option<AccountInfo>, Self::Error> {
            Ok(None)
        }

        fn code_by_hash(&mut self, code_hash: B256) -> Result<Bytecode, Self::Error> {
            self.code_by_hash_call_count.set(self.code_by_hash_call_count.get() + 1);
            Ok(self.bytecodes.get(&code_hash).cloned().unwrap_or_default())
        }

        fn storage(&mut self, address: Address, index: U256) -> Result<U256, Self::Error> {
            self.storage_call_count.set(self.storage_call_count.get() + 1);
            Ok(self.storage.get(&(address, index)).copied().unwrap_or(U256::ZERO))
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
        assert_eq!(cached_db.storage(Address::ZERO, U256::ZERO).unwrap(), U256::ZERO);
        assert_eq!(cached_db.block_hash(0).unwrap(), B256::ZERO);
    }

    // ========================================================================
    // PinnedStorageCache Tests
    // ========================================================================

    #[test]
    fn test_pinned_storage_cache_basic_operations() {
        let contract = Address::repeat_byte(0x42);
        let cache = PinnedStorageCache::new(vec![contract]);

        // Initially empty
        assert!(cache.is_pinned(&contract));
        assert_eq!(cache.slot_count(&contract), 0);
        assert!(cache.get_storage(&contract, &U256::from(1)).is_none());

        // Set a value
        cache.set_storage(contract, U256::from(1), U256::from(100));

        // Should be retrievable
        assert_eq!(cache.get_storage(&contract, &U256::from(1)), Some(U256::from(100)));
        assert_eq!(cache.slot_count(&contract), 1);
    }

    #[test]
    fn test_pinned_storage_cache_non_pinned_ignored() {
        let pinned = Address::repeat_byte(0x01);
        let not_pinned = Address::repeat_byte(0x02);
        let cache = PinnedStorageCache::new(vec![pinned]);

        assert!(cache.is_pinned(&pinned));
        assert!(!cache.is_pinned(&not_pinned));

        // Storing to non-pinned address should be ignored
        cache.set_storage(not_pinned, U256::from(1), U256::from(100));
        assert!(cache.get_storage(&not_pinned, &U256::from(1)).is_none());
        assert_eq!(cache.total_slot_count(), 0);
    }

    #[test]
    fn test_pinned_storage_cache_multiple_contracts() {
        let contract1 = Address::repeat_byte(0x01);
        let contract2 = Address::repeat_byte(0x02);
        let cache = PinnedStorageCache::new(vec![contract1, contract2]);

        cache.set_storage(contract1, U256::from(1), U256::from(100));
        cache.set_storage(contract2, U256::from(1), U256::from(200));
        cache.set_storage(contract1, U256::from(2), U256::from(300));

        assert_eq!(cache.get_storage(&contract1, &U256::from(1)), Some(U256::from(100)));
        assert_eq!(cache.get_storage(&contract2, &U256::from(1)), Some(U256::from(200)));
        assert_eq!(cache.get_storage(&contract1, &U256::from(2)), Some(U256::from(300)));
        assert_eq!(cache.total_slot_count(), 3);
    }

    #[test]
    fn test_pinned_storage_cache_clear() {
        let contract = Address::repeat_byte(0x42);
        let cache = PinnedStorageCache::new(vec![contract]);

        cache.set_storage(contract, U256::from(1), U256::from(100));
        cache.set_storage(contract, U256::from(2), U256::from(200));
        assert_eq!(cache.slot_count(&contract), 2);

        cache.clear_address(&contract);
        assert_eq!(cache.slot_count(&contract), 0);
    }

    #[test]
    fn test_pinned_storage_cache_empty() {
        let cache = PinnedStorageCache::empty();

        assert!(!cache.is_pinned(&Address::ZERO));
        assert_eq!(cache.total_slot_count(), 0);
    }

    // ========================================================================
    // CachedDatabase with Pinned Storage Tests
    // ========================================================================

    #[test]
    fn test_cached_database_pinned_storage_hit() {
        let pinned_contract = Address::repeat_byte(0x42);
        let slot = U256::from(1);
        let value = U256::from(12345);

        let mock_db = MockDatabase::new().with_storage(pinned_contract, slot, value);
        let bytecode_cache = Arc::new(BytecodeCache::new(100));
        let pinned_storage = Arc::new(PinnedStorageCache::new(vec![pinned_contract]));
        let mut cached_db = CachedDatabase::with_pinned_storage(mock_db, bytecode_cache, pinned_storage);

        // First call - cache miss, should hit database
        let result1 = cached_db.storage(pinned_contract, slot).unwrap();
        assert_eq!(result1, value);
        assert_eq!(cached_db.inner().storage_call_count(), 1);

        // Second call - cache hit, should NOT hit database
        let result2 = cached_db.storage(pinned_contract, slot).unwrap();
        assert_eq!(result2, value);
        assert_eq!(cached_db.inner().storage_call_count(), 1); // Still 1!
    }

    #[test]
    fn test_cached_database_non_pinned_not_cached() {
        let pinned_contract = Address::repeat_byte(0x01);
        let non_pinned_contract = Address::repeat_byte(0x02);
        let slot = U256::from(1);

        let mock_db = MockDatabase::new()
            .with_storage(non_pinned_contract, slot, U256::from(999));
        let bytecode_cache = Arc::new(BytecodeCache::new(100));
        let pinned_storage = Arc::new(PinnedStorageCache::new(vec![pinned_contract]));
        let mut cached_db = CachedDatabase::with_pinned_storage(mock_db, bytecode_cache, pinned_storage);

        // First call to non-pinned contract
        let result1 = cached_db.storage(non_pinned_contract, slot).unwrap();
        assert_eq!(result1, U256::from(999));
        assert_eq!(cached_db.inner().storage_call_count(), 1);

        // Second call - should still hit database (not cached)
        let result2 = cached_db.storage(non_pinned_contract, slot).unwrap();
        assert_eq!(result2, U256::from(999));
        assert_eq!(cached_db.inner().storage_call_count(), 2); // Now 2!
    }

    #[test]
    fn test_cached_database_without_pinned_storage() {
        let contract = Address::repeat_byte(0x42);
        let slot = U256::from(1);

        let mock_db = MockDatabase::new().with_storage(contract, slot, U256::from(100));
        let bytecode_cache = Arc::new(BytecodeCache::new(100));
        // No pinned storage - using new() instead of with_pinned_storage()
        let mut cached_db = CachedDatabase::new(mock_db, bytecode_cache);

        // All calls should hit database
        cached_db.storage(contract, slot).unwrap();
        assert_eq!(cached_db.inner().storage_call_count(), 1);

        cached_db.storage(contract, slot).unwrap();
        assert_eq!(cached_db.inner().storage_call_count(), 2);
    }
}
