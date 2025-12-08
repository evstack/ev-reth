//! Benchmarks for bytecode caching layer.
//!
//! This benchmark compares performance of direct database access vs bytecode caching.
//! Bytecode is immutable after deployment, making it ideal for caching.
//!
//! Run with: cargo bench -p ev-revm

use alloy_primitives::{Address, B256, U256};
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use ev_revm::cache::{BytecodeCache, CachedDatabase};
use rand::{Rng, SeedableRng};
use reth_revm::revm::{
    context_interface::Database,
    state::{AccountInfo, Bytecode},
};
use std::{collections::HashMap, sync::Arc};

/// Mock database with configurable latency simulation.
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
            self.bytecodes
                .insert(code_hash, Bytecode::new_raw(code.into()));
        }
        self
    }

    /// Simulate database latency by doing busy work
    fn simulate_latency(&self) {
        if self.latency_factor > 0 {
            let mut sum = 0u64;
            for i in 0..self.latency_factor {
                sum = sum.wrapping_add(i as u64);
            }
            black_box(sum);
        }
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
        Ok(self
            .storage
            .get(&(address, index))
            .copied()
            .unwrap_or(U256::ZERO))
    }

    fn block_hash(&mut self, _number: u64) -> Result<B256, Self::Error> {
        self.simulate_latency();
        Ok(B256::ZERO)
    }
}

/// Benchmark bytecode cache hits - all requests hit the cache
fn bench_bytecode_cache_hit(c: &mut Criterion) {
    let mut group = c.benchmark_group("bytecode_cache_hit");

    // Test different cache sizes
    for &num_contracts in &[10, 100, 1000] {
        group.throughput(Throughput::Elements(num_contracts as u64));

        // Pre-populate cache
        let cache = Arc::new(BytecodeCache::new(num_contracts * 2));
        for i in 0..num_contracts {
            let code_hash = B256::repeat_byte((i % 256) as u8);
            let bytecode = Bytecode::new_raw(vec![0x60, (i % 256) as u8].into());
            cache.insert(code_hash, bytecode);
        }

        group.bench_with_input(
            BenchmarkId::new("cache_hit", num_contracts),
            &num_contracts,
            |b, &n| {
                b.iter(|| {
                    for i in 0..n {
                        let code_hash = B256::repeat_byte((i % 256) as u8);
                        let result = cache.get(&code_hash);
                        black_box(result);
                    }
                })
            },
        );
    }

    group.finish();
}

/// Benchmark cache miss vs hit - demonstrates the benefit of caching
fn bench_bytecode_cache_miss_vs_hit(c: &mut Criterion) {
    let mut group = c.benchmark_group("bytecode_cache_miss_vs_hit");

    let num_contracts = 100;
    let latency_factor = 1000; // Simulate some database latency

    group.throughput(Throughput::Elements(num_contracts as u64));

    // Without cache (all misses go to database)
    group.bench_function("no_cache", |b| {
        let mut db = MockDatabase::new(latency_factor).with_bytecodes(num_contracts);
        b.iter(|| {
            for i in 0..num_contracts {
                let code_hash = B256::repeat_byte((i % 256) as u8);
                let _result = db.code_by_hash(code_hash);
            }
        })
    });

    // With cache (first pass misses, subsequent passes hit)
    group.bench_function("with_cache_warm", |b| {
        let db = MockDatabase::new(latency_factor).with_bytecodes(num_contracts);
        let cache = Arc::new(BytecodeCache::new(num_contracts * 2));
        let mut cached_db = CachedDatabase::new(db, cache);

        // Warm up the cache
        for i in 0..num_contracts {
            let code_hash = B256::repeat_byte((i % 256) as u8);
            let _ = cached_db.code_by_hash(code_hash);
        }

        b.iter(|| {
            for i in 0..num_contracts {
                let code_hash = B256::repeat_byte((i % 256) as u8);
                let _result = cached_db.code_by_hash(code_hash);
            }
        })
    });

    group.finish();
}

/// Benchmark LRU eviction behavior
fn bench_bytecode_cache_eviction(c: &mut Criterion) {
    let mut group = c.benchmark_group("bytecode_cache_eviction");

    let cache_size = 100;
    let num_contracts = 200; // More contracts than cache can hold

    group.throughput(Throughput::Elements(num_contracts as u64));

    // Random access pattern (will cause evictions)
    group.bench_function("random_access", |b| {
        let db = MockDatabase::new(100).with_bytecodes(num_contracts);
        let cache = Arc::new(BytecodeCache::new(cache_size));
        let mut cached_db = CachedDatabase::new(db, cache);
        let mut rng = rand::rngs::StdRng::seed_from_u64(42);

        b.iter(|| {
            for _ in 0..num_contracts {
                let i = rng.gen_range(0..num_contracts);
                let code_hash = B256::repeat_byte((i % 256) as u8);
                let _result = cached_db.code_by_hash(code_hash);
            }
        })
    });

    // Sequential access pattern (better cache locality)
    group.bench_function("sequential_access", |b| {
        let db = MockDatabase::new(100).with_bytecodes(num_contracts);
        let cache = Arc::new(BytecodeCache::new(cache_size));
        let mut cached_db = CachedDatabase::new(db, cache);

        b.iter(|| {
            for i in 0..num_contracts {
                let code_hash = B256::repeat_byte((i % 256) as u8);
                let _result = cached_db.code_by_hash(code_hash);
            }
        })
    });

    group.finish();
}

/// Benchmark realistic workload with mixed operations
fn bench_realistic_workload(c: &mut Criterion) {
    let mut group = c.benchmark_group("realistic_workload");

    let num_contracts = 50;
    let ops_per_iteration = 1000;
    let latency_factor = 500;

    group.throughput(Throughput::Elements(ops_per_iteration as u64));

    // Simulate a realistic workload where some contracts are called frequently
    // (hot contracts) and others are called rarely (cold contracts)
    group.bench_function("hot_cold_distribution", |b| {
        let db = MockDatabase::new(latency_factor).with_bytecodes(num_contracts);
        let cache = Arc::new(BytecodeCache::new(num_contracts));
        let mut cached_db = CachedDatabase::new(db, cache);
        let mut rng = rand::rngs::StdRng::seed_from_u64(42);

        // 80% of calls go to 20% of contracts (hot contracts: 0-9)
        // 20% of calls go to 80% of contracts (cold contracts: 10-49)
        b.iter(|| {
            for _ in 0..ops_per_iteration {
                let i = if rng.gen_bool(0.8) {
                    rng.gen_range(0..10) // Hot contract
                } else {
                    rng.gen_range(10..num_contracts) // Cold contract
                };
                let code_hash = B256::repeat_byte((i % 256) as u8);
                let _result = cached_db.code_by_hash(code_hash);
            }
        })
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_bytecode_cache_hit,
    bench_bytecode_cache_miss_vs_hit,
    bench_bytecode_cache_eviction,
    bench_realistic_workload,
);
criterion_main!(benches);
