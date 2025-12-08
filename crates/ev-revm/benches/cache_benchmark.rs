//! Benchmarks for EVM caching layers.
//!
//! This benchmark compares performance of:
//! - Direct database access vs bytecode caching
//! - Direct database access vs pinned storage caching
//!
//! Run with: cargo bench -p ev-revm

use alloy_primitives::{Address, B256, U256};
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use ev_revm::cache::{BytecodeCache, CachedDatabase, PinnedStorageCache};
use rand::{Rng, SeedableRng};
use reth_revm::revm::{
    context_interface::Database,
    state::{AccountInfo, Bytecode},
};
use std::{collections::HashMap, sync::Arc};

/// Mock database with configurable latency simulation.
/// In real usage, database access involves disk I/O which is orders of magnitude slower.
#[derive(Debug)]
struct MockDatabase {
    bytecodes: HashMap<B256, Bytecode>,
    storage: HashMap<(Address, U256), U256>,
    /// Simulated latency per operation (in nanoseconds worth of work)
    latency_factor: usize,
}

impl MockDatabase {
    fn new(latency_factor: usize) -> Self {
        Self {
            bytecodes: HashMap::new(),
            storage: HashMap::new(),
            latency_factor,
        }
    }

    fn with_bytecodes(mut self, count: usize) -> Self {
        for i in 0..count {
            let code_hash = B256::repeat_byte((i % 256) as u8);
            // Create realistic bytecode (average contract ~5KB)
            let bytecode_size = 5000 + (i % 1000);
            let mut code = vec![0x60u8; bytecode_size]; // PUSH1 opcodes
            code[0] = 0x60;
            code[1] = (i % 256) as u8;
            self.bytecodes.insert(code_hash, Bytecode::new_raw(code.into()));
        }
        self
    }

    fn with_storage(mut self, address: Address, slot_count: usize) -> Self {
        for i in 0..slot_count {
            let slot = U256::from(i);
            let value = U256::from(i * 1000);
            self.storage.insert((address, slot), value);
        }
        self
    }

    /// Simulate work to represent disk I/O latency
    fn simulate_latency(&self) {
        // Do some work proportional to latency_factor
        // This simulates the overhead of disk access
        let mut dummy: u64 = 1;
        for _ in 0..self.latency_factor {
            dummy = dummy.wrapping_mul(7).wrapping_add(11);
        }
        // Prevent optimization
        black_box(dummy);
    }
}

impl Database for MockDatabase {
    type Error = std::convert::Infallible;

    fn basic(&mut self, _address: Address) -> Result<Option<AccountInfo>, Self::Error> {
        self.simulate_latency();
        Ok(None)
    }

    fn code_by_hash(&mut self, code_hash: B256) -> Result<Bytecode, Self::Error> {
        self.simulate_latency();
        Ok(self.bytecodes.get(&code_hash).cloned().unwrap_or_default())
    }

    fn storage(&mut self, address: Address, index: U256) -> Result<U256, Self::Error> {
        self.simulate_latency();
        Ok(self.storage.get(&(address, index)).copied().unwrap_or(U256::ZERO))
    }

    fn block_hash(&mut self, _number: u64) -> Result<B256, Self::Error> {
        self.simulate_latency();
        Ok(B256::ZERO)
    }
}

/// Benchmark bytecode cache hit performance
fn bench_bytecode_cache_hit(c: &mut Criterion) {
    let mut group = c.benchmark_group("bytecode_cache");

    // Different cache hit scenarios
    for &num_contracts in &[10, 100, 1000] {
        group.throughput(Throughput::Elements(num_contracts as u64));

        // Setup: pre-populate cache
        let cache = Arc::new(BytecodeCache::new(10_000));
        let mock_db = MockDatabase::new(100).with_bytecodes(num_contracts);

        // Pre-warm cache
        let code_hashes: Vec<_> = (0..num_contracts)
            .map(|i| B256::repeat_byte((i % 256) as u8))
            .collect();

        for hash in &code_hashes {
            if let Some(bytecode) = mock_db.bytecodes.get(hash) {
                cache.insert(*hash, bytecode.clone());
            }
        }

        let mut cached_db = CachedDatabase::new(mock_db, cache);

        group.bench_with_input(
            BenchmarkId::new("cache_hit", num_contracts),
            &code_hashes,
            |b, hashes| {
                b.iter(|| {
                    for hash in hashes {
                        let _ = black_box(cached_db.code_by_hash(*hash));
                    }
                })
            },
        );
    }

    group.finish();
}

/// Benchmark bytecode cache miss vs hit comparison
fn bench_bytecode_cache_miss_vs_hit(c: &mut Criterion) {
    let mut group = c.benchmark_group("bytecode_cache_comparison");

    let num_lookups = 100;
    group.throughput(Throughput::Elements(num_lookups as u64));

    // Setup databases with simulated latency
    let latency_factor = 1000; // Simulate ~1000 cycles of work per DB access

    // Pre-generate code hashes
    let code_hashes: Vec<_> = (0..num_lookups)
        .map(|i| B256::repeat_byte((i % 256) as u8))
        .collect();

    // Benchmark: No caching (direct DB access)
    {
        let mock_db = MockDatabase::new(latency_factor).with_bytecodes(num_lookups);

        // Wrap in CachedDatabase but don't pre-warm (always miss)
        let cache = Arc::new(BytecodeCache::new(10_000));
        let mut cached_db = CachedDatabase::new(mock_db, cache);

        group.bench_function("always_miss", |b| {
            b.iter(|| {
                // Clear cache to ensure misses
                cached_db.cache().clear();
                for hash in &code_hashes {
                    let _ = black_box(cached_db.code_by_hash(*hash));
                }
            })
        });
    }

    // Benchmark: With caching (all hits after first pass)
    {
        let mock_db = MockDatabase::new(latency_factor).with_bytecodes(num_lookups);
        let cache = Arc::new(BytecodeCache::new(10_000));
        let mut cached_db = CachedDatabase::new(mock_db, cache);

        // Pre-warm cache
        for hash in &code_hashes {
            let _ = cached_db.code_by_hash(*hash);
        }

        group.bench_function("always_hit", |b| {
            b.iter(|| {
                for hash in &code_hashes {
                    let _ = black_box(cached_db.code_by_hash(*hash));
                }
            })
        });
    }

    group.finish();
}

/// Benchmark pinned storage cache performance
fn bench_pinned_storage_cache(c: &mut Criterion) {
    let mut group = c.benchmark_group("pinned_storage_cache");

    let pinned_contract = Address::repeat_byte(0x42);
    let num_slots = 100;
    group.throughput(Throughput::Elements(num_slots as u64));

    let slots: Vec<_> = (0..num_slots).map(U256::from).collect();
    let latency_factor = 1000;

    // Benchmark: Non-pinned storage (no caching)
    {
        let non_pinned = Address::repeat_byte(0x01);
        let mock_db = MockDatabase::new(latency_factor).with_storage(non_pinned, num_slots);
        let bytecode_cache = Arc::new(BytecodeCache::new(100));
        let pinned_storage = Arc::new(PinnedStorageCache::new(vec![pinned_contract])); // Different address
        let mut cached_db =
            CachedDatabase::with_pinned_storage(mock_db, bytecode_cache, pinned_storage);

        group.bench_function("non_pinned", |b| {
            b.iter(|| {
                for slot in &slots {
                    let _ = black_box(cached_db.storage(non_pinned, *slot));
                }
            })
        });
    }

    // Benchmark: Pinned storage (cache hit)
    {
        let mock_db = MockDatabase::new(latency_factor).with_storage(pinned_contract, num_slots);
        let bytecode_cache = Arc::new(BytecodeCache::new(100));
        let pinned_storage = Arc::new(PinnedStorageCache::new(vec![pinned_contract]));
        let mut cached_db =
            CachedDatabase::with_pinned_storage(mock_db, bytecode_cache, pinned_storage);

        // Pre-warm cache
        for slot in &slots {
            let _ = cached_db.storage(pinned_contract, *slot);
        }

        group.bench_function("pinned_hit", |b| {
            b.iter(|| {
                for slot in &slots {
                    let _ = black_box(cached_db.storage(pinned_contract, *slot));
                }
            })
        });
    }

    group.finish();
}

/// Benchmark mixed workload with realistic access patterns
fn bench_mixed_workload(c: &mut Criterion) {
    let mut group = c.benchmark_group("mixed_workload");

    let num_contracts = 50;
    let num_pinned = 5;
    let slots_per_contract = 20;
    let iterations = 100;

    group.throughput(Throughput::Elements(iterations as u64));

    // Setup: Multiple contracts, some pinned
    let pinned_contracts: Vec<_> = (0..num_pinned).map(|i| Address::repeat_byte(i as u8)).collect();
    let all_contracts: Vec<_> = (0..num_contracts)
        .map(|i| Address::repeat_byte(i as u8))
        .collect();
    let code_hashes: Vec<_> = (0..num_contracts)
        .map(|i| B256::repeat_byte(i as u8))
        .collect();
    let slots: Vec<_> = (0..slots_per_contract).map(U256::from).collect();

    let latency_factor = 500;

    // Create mock database with all storage
    let mut mock_db = MockDatabase::new(latency_factor).with_bytecodes(num_contracts);
    for contract in &all_contracts {
        mock_db = MockDatabase {
            bytecodes: mock_db.bytecodes,
            storage: {
                let mut s = mock_db.storage;
                for i in 0..slots_per_contract {
                    s.insert((*contract, U256::from(i)), U256::from(i * 1000));
                }
                s
            },
            latency_factor,
        };
    }

    // Benchmark: No caching
    {
        let db = MockDatabase::new(latency_factor).with_bytecodes(num_contracts);
        let db = MockDatabase {
            bytecodes: db.bytecodes,
            storage: mock_db.storage.clone(),
            latency_factor,
        };

        let bytecode_cache = Arc::new(BytecodeCache::new(100));
        let pinned_storage = Arc::new(PinnedStorageCache::empty()); // No pinning
        let mut cached_db =
            CachedDatabase::with_pinned_storage(db, bytecode_cache, pinned_storage);

        // Simulate realistic access: read bytecode, then storage slots
        let mut rng = rand::rngs::StdRng::seed_from_u64(42);

        group.bench_function("no_pinning", |b| {
            b.iter(|| {
                cached_db.cache().clear();
                if let Some(ps) = cached_db.pinned_storage() {
                    ps.clear();
                }
                for _ in 0..iterations {
                    let contract_idx = rng.gen_range(0..num_contracts);
                    let _ = black_box(cached_db.code_by_hash(code_hashes[contract_idx]));
                    for _ in 0..3 {
                        let slot_idx = rng.gen_range(0..slots_per_contract);
                        let _ = black_box(
                            cached_db.storage(all_contracts[contract_idx], slots[slot_idx]),
                        );
                    }
                }
            })
        });
    }

    // Benchmark: With bytecode + pinned storage caching
    {
        let db = MockDatabase::new(latency_factor).with_bytecodes(num_contracts);
        let db = MockDatabase {
            bytecodes: db.bytecodes,
            storage: mock_db.storage.clone(),
            latency_factor,
        };

        let bytecode_cache = Arc::new(BytecodeCache::new(100));
        let pinned_storage = Arc::new(PinnedStorageCache::new(pinned_contracts.clone()));
        let mut cached_db =
            CachedDatabase::with_pinned_storage(db, bytecode_cache, pinned_storage);

        // Pre-warm caches
        for hash in &code_hashes {
            let _ = cached_db.code_by_hash(*hash);
        }
        for contract in &pinned_contracts {
            for slot in &slots {
                let _ = cached_db.storage(*contract, *slot);
            }
        }

        let mut rng = rand::rngs::StdRng::seed_from_u64(42);

        group.bench_function("with_caching", |b| {
            b.iter(|| {
                for _ in 0..iterations {
                    let contract_idx = rng.gen_range(0..num_contracts);
                    let _ = black_box(cached_db.code_by_hash(code_hashes[contract_idx]));
                    for _ in 0..3 {
                        let slot_idx = rng.gen_range(0..slots_per_contract);
                        let _ = black_box(
                            cached_db.storage(all_contracts[contract_idx], slots[slot_idx]),
                        );
                    }
                }
            })
        });
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_bytecode_cache_hit,
    bench_bytecode_cache_miss_vs_hit,
    bench_pinned_storage_cache,
    bench_mixed_workload,
);
criterion_main!(benches);
