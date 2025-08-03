use criterion::{black_box, criterion_group, criterion_main, Criterion};
use canp::memory_pool::{MemoryBlock, MemoryPoolConfig, UnifiedMemoryPool};
use std::sync::Arc;

fn memory_block_creation_benchmark(c: &mut Criterion) {
    c.bench_function("memory_block_creation", |b| {
        b.iter(|| {
            let data = vec![0u8; black_box(1024)];
            let _block = MemoryBlock::new(data);
        });
    });
}

fn memory_pool_allocation_benchmark(c: &mut Criterion) {
    let config = MemoryPoolConfig::default();
    let pool = Arc::new(UnifiedMemoryPool::new(config));
    
    c.bench_function("memory_pool_single_allocation", |b| {
        b.iter(|| {
            let _block = pool.allocate_block(black_box(1024)).unwrap();
        });
    });
}

fn memory_pool_batch_allocation_benchmark(c: &mut Criterion) {
    let config = MemoryPoolConfig::default();
    let pool = Arc::new(UnifiedMemoryPool::new(config));
    
    c.bench_function("memory_pool_batch_allocation", |b| {
        b.iter(|| {
            let _blocks = pool.allocate_blocks_batch(black_box(10), black_box(1024)).unwrap();
        });
    });
}

fn memory_pool_decompress_buffer_benchmark(c: &mut Criterion) {
    let config = MemoryPoolConfig::default();
    let pool = Arc::new(UnifiedMemoryPool::new(config));
    
    c.bench_function("memory_pool_decompress_buffer", |b| {
        b.iter(|| {
            let _buffer = pool.allocate_decompress_buffer(black_box(8192)).unwrap();
        });
    });
}

fn memory_pool_frame_buffer_benchmark(c: &mut Criterion) {
    let config = MemoryPoolConfig::default();
    let pool = Arc::new(UnifiedMemoryPool::new(config));
    
    c.bench_function("memory_pool_frame_buffer", |b| {
        b.iter(|| {
            let _buffer = pool.allocate_frame_buffer(black_box(2048)).unwrap();
        });
    });
}

fn memory_block_operations_benchmark(c: &mut Criterion) {
    c.bench_function("memory_block_slice_access", |b| {
        let data = vec![0u8; 1024];
        let block = MemoryBlock::new(data);
        
        b.iter(|| {
            let _slice = block.as_slice();
            black_box(_slice);
        });
    });
    
    c.bench_function("memory_block_ptr_len_access", |b| {
        let data = vec![0u8; 1024];
        let block = MemoryBlock::new(data);
        
        b.iter(|| {
            let (ptr, len) = block.as_ptr_and_len();
            black_box((ptr, len));
        });
    });
}

criterion_group!(
    memory_pool_benches,
    memory_block_creation_benchmark,
    memory_pool_allocation_benchmark,
    memory_pool_batch_allocation_benchmark,
    memory_pool_decompress_buffer_benchmark,
    memory_pool_frame_buffer_benchmark,
    memory_block_operations_benchmark,
);
criterion_main!(memory_pool_benches); 