# 内存管理学习指南

## 📚 概述

内存管理是高性能系统开发中的关键环节。CANP项目采用了零拷贝架构和智能内存池技术，本文档详细介绍相关概念、实现方法和最佳实践。

## 🏗️ 核心概念

### 1. 零拷贝 (Zero-Copy)

#### 什么是零拷贝

零拷贝是指在数据处理过程中，避免不必要的数据拷贝操作，直接通过内存映射或指针传递来访问数据。

```rust
// 传统方式：需要拷贝数据
fn process_data_traditional(data: Vec<u8>) -> Vec<u8> {
    let mut processed = Vec::new();
    processed.extend_from_slice(&data); // 拷贝数据
    processed
}

// 零拷贝方式：直接引用
fn process_data_zero_copy(data: &[u8]) -> &[u8] {
    data // 直接返回引用，无拷贝
}
```

#### 内存映射 (Memory Mapping)

```rust
use memmap2::Mmap;

// 内存映射文件
fn map_file(path: &str) -> Result<Mmap> {
    let file = std::fs::File::open(path)?;
    let mmap = unsafe { Mmap::map(&file)? };
    Ok(mmap)
}

// 零拷贝访问
fn process_mapped_file(mmap: &Mmap) -> &[u8] {
    &mmap[..] // 直接访问，无拷贝
}
```

### 2. 在CANP中的应用

```rust
// 内存映射块
pub struct MemoryMappedBlock {
    mmap: Arc<Mmap>,
    file_path: PathBuf,
}

impl MemoryMappedBlock {
    pub fn new(file_path: PathBuf) -> Result<Self> {
        let file = std::fs::File::open(&file_path)?;
        let mmap = unsafe { Mmap::map(&file)? };
        Ok(Self {
            mmap: Arc::new(mmap),
            file_path,
        })
    }
    
    pub fn as_slice(&self) -> &[u8] {
        &self.mmap[..] // 零拷贝访问
    }
    
    pub fn len(&self) -> usize {
        self.mmap.len()
    }
}
```

## 🔄 内存池 (Memory Pool)

### 1. 对象池模式

#### 基本概念

对象池是一种设计模式，通过预先分配和复用对象来减少内存分配开销。

```rust
use std::collections::VecDeque;
use std::sync::Mutex;

pub struct ObjectPool<T> {
    objects: Mutex<VecDeque<T>>,
    create_fn: Box<dyn Fn() -> T + Send + Sync>,
}

impl<T> ObjectPool<T> {
    pub fn new<F>(create_fn: F) -> Self
    where
        F: Fn() -> T + Send + Sync + 'static,
    {
        Self {
            objects: Mutex::new(VecDeque::new()),
            create_fn: Box::new(create_fn),
        }
    }
    
    pub fn acquire(&self) -> T {
        let mut objects = self.objects.lock().unwrap();
        objects.pop_front().unwrap_or_else(|| (self.create_fn)())
    }
    
    pub fn release(&self, object: T) {
        let mut objects = self.objects.lock().unwrap();
        objects.push_back(object);
    }
}
```

#### 在CANP中的应用

```rust
// 使用 lock_pool 库的对象池
use lock_pool::LockPool;
use bytes::BytesMut;

pub struct ZeroCopyMemoryPool {
    decompress_pools: Vec<Arc<LockPool<BytesMut, 64, 512>>>,
}

impl ZeroCopyMemoryPool {
    pub fn get_decompress_buffer(&self, size: usize) -> MutableMemoryBuffer {
        if let Some(pool) = self.select_decompress_pool(size) {
            if let Some(guard) = pool.try_lock() {
                let buffer = guard.clone();
                MutableMemoryBuffer { buffer }
            } else {
                self.create_new_buffer(size)
            }
        } else {
            self.create_new_buffer(size)
        }
    }
    
    fn select_decompress_pool(&self, size: usize) -> Option<&Arc<LockPool<BytesMut, 64, 512>>> {
        self.decompress_pools.iter()
            .find(|pool| pool.capacity() >= size)
    }
}
```

### 2. 分层内存池

#### 设计理念

根据数据大小分层管理，提高内存分配效率。

```rust
pub struct LayeredMemoryPool {
    small_pool: ObjectPool<Vec<u8>>,    // 1KB 以下
    medium_pool: ObjectPool<Vec<u8>>,   // 1KB - 4KB
    large_pool: ObjectPool<Vec<u8>>,    // 4KB - 16KB
    huge_pool: ObjectPool<Vec<u8>>,     // 16KB 以上
}

impl LayeredMemoryPool {
    pub fn new() -> Self {
        Self {
            small_pool: ObjectPool::new(|| Vec::with_capacity(1024)),
            medium_pool: ObjectPool::new(|| Vec::with_capacity(4096)),
            large_pool: ObjectPool::new(|| Vec::with_capacity(16384)),
            huge_pool: ObjectPool::new(|| Vec::with_capacity(65536)),
        }
    }
    
    pub fn allocate(&self, size: usize) -> Vec<u8> {
        match size {
            0..=1024 => self.small_pool.acquire(),
            1025..=4096 => self.medium_pool.acquire(),
            4097..=16384 => self.large_pool.acquire(),
            _ => self.huge_pool.acquire(),
        }
    }
    
    pub fn deallocate(&self, buffer: Vec<u8>) {
        let size = buffer.capacity();
        match size {
            0..=1024 => self.small_pool.release(buffer),
            1025..=4096 => self.medium_pool.release(buffer),
            4097..=16384 => self.large_pool.release(buffer),
            _ => self.huge_pool.release(buffer),
        }
    }
}
```

## 📦 Bytes 库

### 1. Bytes 和 BytesMut

#### 基本用法

```rust
use bytes::{Bytes, BytesMut, Buf, BufMut};

// 创建 BytesMut
let mut buf = BytesMut::with_capacity(1024);
buf.put_u8(1);
buf.put_u16(1234);
buf.put_slice(b"hello");

// 转换为 Bytes（不可变）
let bytes = buf.freeze();

// 读取数据
let mut reader = bytes.clone();
let first_byte = reader.get_u8();
let number = reader.get_u16();
let text = reader.copy_to_bytes(5);
```

#### 零拷贝特性

```rust
use bytes::{Bytes, BytesMut};

// Bytes 支持零拷贝分割
let original = Bytes::from("Hello, World!");
let hello = original.slice(0..5);      // 零拷贝分割
let world = original.slice(7..12);     // 零拷贝分割

// 引用计数
let shared = original.clone();         // 共享底层数据
```

### 2. 在CANP中的应用

```rust
// 可变内存缓冲区
pub struct MutableMemoryBuffer {
    buffer: BytesMut,
}

impl MutableMemoryBuffer {
    pub fn new(capacity: usize) -> Self {
        Self {
            buffer: BytesMut::with_capacity(capacity),
        }
    }
    
    pub fn extend_from_slice(&mut self, data: &[u8]) {
        self.buffer.extend_from_slice(data);
    }
    
    pub fn len(&self) -> usize {
        self.buffer.len()
    }
    
    pub fn capacity(&self) -> usize {
        self.buffer.capacity()
    }
    
    pub fn freeze(self) -> Bytes {
        self.buffer.freeze()
    }
}
```

## 🔄 智能指针

### 1. Arc<T> - 原子引用计数

#### 基本用法

```rust
use std::sync::Arc;
use std::thread;

// 共享数据
let data = Arc::new(vec![1, 2, 3, 4, 5]);
let mut handles = vec![];

for i in 0..3 {
    let data = Arc::clone(&data);
    let handle = thread::spawn(move || {
        println!("线程 {}: {:?}", i, data);
    });
    handles.push(handle);
}

for handle in handles {
    handle.join().unwrap();
}
```

#### 在CANP中的应用

```rust
// 共享内存池
pub struct ZeroCopyMemoryPool {
    decompress_pools: Vec<Arc<LockPool<BytesMut, 64, 512>>>,
    mmap_cache: Arc<RwLock<lru::LruCache<String, Arc<Mmap>>>>,
    current_memory_usage: Arc<RwLock<usize>>,
}

// 共享DBC管理器
pub struct DbcManager {
    dbc_cache: Arc<RwLock<HashMap<PathBuf, DbcCacheEntry>>>,
    stats: Arc<RwLock<DbcParsingStats>>,
}
```

### 2. Rc<T> - 引用计数

```rust
use std::rc::Rc;

// 单线程引用计数
let data = Rc::new(vec![1, 2, 3, 4, 5]);
let data1 = Rc::clone(&data);
let data2 = Rc::clone(&data);

println!("引用计数: {}", Rc::strong_count(&data));
```

## 🎯 缓存策略

### 1. LRU 缓存

#### 基本实现

```rust
use std::collections::HashMap;
use std::collections::VecDeque;

pub struct LRUCache<K, V> {
    capacity: usize,
    cache: HashMap<K, V>,
    order: VecDeque<K>,
}

impl<K, V> LRUCache<K, V>
where
    K: Clone + Eq + std::hash::Hash,
{
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            cache: HashMap::new(),
            order: VecDeque::new(),
        }
    }
    
    pub fn get(&mut self, key: &K) -> Option<&V> {
        if let Some(value) = self.cache.get(key) {
            // 移动到最近使用
            if let Some(pos) = self.order.iter().position(|k| k == key) {
                self.order.remove(pos);
            }
            self.order.push_back(key.clone());
            Some(value)
        } else {
            None
        }
    }
    
    pub fn put(&mut self, key: K, value: V) {
        if self.cache.contains_key(&key) {
            // 更新现有项
            self.cache.insert(key.clone(), value);
            if let Some(pos) = self.order.iter().position(|k| k == &key) {
                self.order.remove(pos);
            }
            self.order.push_back(key);
        } else {
            // 添加新项
            if self.cache.len() >= self.capacity {
                if let Some(oldest) = self.order.pop_front() {
                    self.cache.remove(&oldest);
                }
            }
            self.cache.insert(key.clone(), value);
            self.order.push_back(key);
        }
    }
}
```

#### 在CANP中的应用

```rust
// 文件映射缓存
pub struct ZeroCopyMemoryPool {
    mmap_cache: Arc<RwLock<lru::LruCache<String, Arc<Mmap>>>>,
}

impl ZeroCopyMemoryPool {
    pub fn map_file<P: AsRef<Path>>(&self, file_path: P) -> Result<MemoryMappedBlock> {
        let path_str = file_path.as_ref().to_string_lossy().to_string();
        
        // 检查缓存
        {
            let cache = self.mmap_cache.read().unwrap();
            if let Some(mmap) = cache.get(&path_str) {
                return Ok(MemoryMappedBlock {
                    mmap: Arc::clone(mmap),
                    file_path: file_path.as_ref().to_path_buf(),
                });
            }
        }
        
        // 创建新的内存映射
        let file = std::fs::File::open(file_path.as_ref())?;
        let mmap = Arc::new(unsafe { Mmap::map(&file)? });
        
        // 更新缓存
        {
            let mut cache = self.mmap_cache.write().unwrap();
            cache.put(path_str, Arc::clone(&mmap));
        }
        
        Ok(MemoryMappedBlock {
            mmap,
            file_path: file_path.as_ref().to_path_buf(),
        })
    }
}
```

### 2. 缓存统计

```rust
pub struct CacheStats {
    hits: AtomicUsize,
    misses: AtomicUsize,
}

impl CacheStats {
    pub fn new() -> Self {
        Self {
            hits: AtomicUsize::new(0),
            misses: AtomicUsize::new(0),
        }
    }
    
    pub fn record_hit(&self) {
        self.hits.fetch_add(1, Ordering::Relaxed);
    }
    
    pub fn record_miss(&self) {
        self.misses.fetch_add(1, Ordering::Relaxed);
    }
    
    pub fn hit_rate(&self) -> f64 {
        let hits = self.hits.load(Ordering::Relaxed);
        let misses = self.misses.load(Ordering::Relaxed);
        let total = hits + misses;
        
        if total == 0 {
            0.0
        } else {
            hits as f64 / total as f64
        }
    }
}
```

## 🔧 内存监控

### 1. 内存使用统计

```rust
use std::sync::atomic::{AtomicUsize, Ordering};

pub struct MemoryStats {
    total_allocated: AtomicUsize,
    total_freed: AtomicUsize,
    peak_usage: AtomicUsize,
    current_usage: AtomicUsize,
}

impl MemoryStats {
    pub fn new() -> Self {
        Self {
            total_allocated: AtomicUsize::new(0),
            total_freed: AtomicUsize::new(0),
            peak_usage: AtomicUsize::new(0),
            current_usage: AtomicUsize::new(0),
        }
    }
    
    pub fn record_allocation(&self, size: usize) {
        self.total_allocated.fetch_add(size, Ordering::Relaxed);
        let current = self.current_usage.fetch_add(size, Ordering::Relaxed) + size;
        
        // 更新峰值
        let mut peak = self.peak_usage.load(Ordering::Relaxed);
        while current > peak {
            match self.peak_usage.compare_exchange_weak(
                peak, current, Ordering::Relaxed, Ordering::Relaxed
            ) {
                Ok(_) => break,
                Err(new_peak) => peak = new_peak,
            }
        }
    }
    
    pub fn record_deallocation(&self, size: usize) {
        self.total_freed.fetch_add(size, Ordering::Relaxed);
        self.current_usage.fetch_sub(size, Ordering::Relaxed);
    }
    
    pub fn get_stats(&self) -> MemoryUsageStats {
        MemoryUsageStats {
            total_allocated: self.total_allocated.load(Ordering::Relaxed),
            total_freed: self.total_freed.load(Ordering::Relaxed),
            peak_usage: self.peak_usage.load(Ordering::Relaxed),
            current_usage: self.current_usage.load(Ordering::Relaxed),
        }
    }
}
```

### 2. 内存泄漏检测

```rust
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Instant, Duration};

pub struct LeakDetector {
    allocations: Mutex<HashMap<usize, AllocationInfo>>,
    next_id: AtomicUsize,
}

#[derive(Debug)]
struct AllocationInfo {
    id: usize,
    size: usize,
    timestamp: Instant,
    stack_trace: String,
}

impl LeakDetector {
    pub fn new() -> Self {
        Self {
            allocations: Mutex::new(HashMap::new()),
            next_id: AtomicUsize::new(0),
        }
    }
    
    pub fn track_allocation(&self, ptr: usize, size: usize) {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let info = AllocationInfo {
            id,
            size,
            timestamp: Instant::now(),
            stack_trace: self.get_stack_trace(),
        };
        
        let mut allocations = self.allocations.lock().unwrap();
        allocations.insert(ptr, info);
    }
    
    pub fn track_deallocation(&self, ptr: usize) {
        let mut allocations = self.allocations.lock().unwrap();
        allocations.remove(&ptr);
    }
    
    pub fn check_leaks(&self, threshold: Duration) -> Vec<AllocationInfo> {
        let now = Instant::now();
        let mut allocations = self.allocations.lock().unwrap();
        
        allocations.values()
            .filter(|info| now.duration_since(info.timestamp) > threshold)
            .cloned()
            .collect()
    }
    
    fn get_stack_trace(&self) -> String {
        // 简化的栈跟踪实现
        "stack_trace_placeholder".to_string()
    }
}
```

## 🎯 最佳实践

### 1. 内存分配优化

```rust
// 预分配容量
fn efficient_vector_creation() -> Vec<i32> {
    let mut vec = Vec::with_capacity(1000);
    for i in 0..1000 {
        vec.push(i);
    }
    vec
}

// 避免频繁分配
fn avoid_frequent_allocation() {
    let mut buffer = Vec::with_capacity(1024);
    
    for _ in 0..100 {
        buffer.clear(); // 重用缓冲区
        // 填充数据
        for i in 0..100 {
            buffer.push(i);
        }
        // 处理数据
        process_buffer(&buffer);
    }
}

// 使用对象池
fn use_object_pool() {
    let pool = ObjectPool::new(|| Vec::with_capacity(1024));
    
    for _ in 0..100 {
        let mut buffer = pool.acquire();
        // 使用缓冲区
        buffer.push(42);
        // 归还到池中
        pool.release(buffer);
    }
}
```

### 2. 零拷贝优化

```rust
// 使用切片而不是克隆
fn process_data_efficient(data: &[u8]) -> &[u8] {
    // 直接处理，无拷贝
    &data[10..20]
}

// 使用引用传递
fn process_large_data(data: &[u8]) -> Vec<u8> {
    // 只在必要时创建新数据
    if data.len() > 1000 {
        data.to_vec() // 只在需要所有权时克隆
    } else {
        data.iter().map(|&b| b * 2).collect()
    }
}

// 使用内存映射
fn process_file_efficient(path: &str) -> Result<&[u8]> {
    let file = std::fs::File::open(path)?;
    let mmap = unsafe { Mmap::map(&file)? };
    Ok(&mmap[..])
}
```

### 3. 缓存优化

```rust
// 使用合适的缓存大小
fn create_optimized_cache() -> LRUCache<String, Vec<u8>> {
    LRUCache::new(1000) // 根据内存限制调整
}

// 定期清理缓存
fn maintain_cache(cache: &mut LRUCache<String, Vec<u8>>) {
    // 定期清理过期项
    if cache.len() > 800 {
        // 清理最旧的20%项
        let to_remove = cache.len() - 800;
        for _ in 0..to_remove {
            cache.remove_oldest();
        }
    }
}

// 监控缓存性能
fn monitor_cache_performance(stats: &CacheStats) {
    let hit_rate = stats.hit_rate();
    if hit_rate < 0.8 {
        println!("缓存命中率低: {:.2}%", hit_rate * 100.0);
    }
}
```

## 🔧 调试和监控

### 1. 内存使用监控

```rust
use std::time::{Instant, Duration};

pub struct MemoryMonitor {
    stats: MemoryStats,
    last_report: Instant,
    report_interval: Duration,
}

impl MemoryMonitor {
    pub fn new(report_interval: Duration) -> Self {
        Self {
            stats: MemoryStats::new(),
            last_report: Instant::now(),
            report_interval,
        }
    }
    
    pub fn check_and_report(&mut self) {
        if self.last_report.elapsed() >= self.report_interval {
            let stats = self.stats.get_stats();
            println!("内存使用报告:");
            println!("  当前使用: {} MB", stats.current_usage / 1024 / 1024);
            println!("  峰值使用: {} MB", stats.peak_usage / 1024 / 1024);
            println!("  总分配: {} MB", stats.total_allocated / 1024 / 1024);
            println!("  总释放: {} MB", stats.total_freed / 1024 / 1024);
            
            self.last_report = Instant::now();
        }
    }
}
```

### 2. 性能分析

```rust
// 内存分配性能分析
fn profile_memory_allocation<F, T>(name: &str, operation: F) -> T
where
    F: FnOnce() -> T,
{
    let start = Instant::now();
    let result = operation();
    let duration = start.elapsed();
    
    println!("{} 执行时间: {:?}", name, duration);
    result
}

// 使用示例
let result = profile_memory_allocation("大向量分配", || {
    Vec::<i32>::with_capacity(1000000)
});
```

## 📚 学习资源

### 官方文档
- [Rust Memory Management](https://doc.rust-lang.org/book/ch04-00-understanding-ownership.html)
- [Bytes Documentation](https://docs.rs/bytes/)
- [memmap2 Documentation](https://docs.rs/memmap2/)

### 社区资源
- [Rust Memory Safety](https://doc.rust-lang.org/nomicon/)
- [Zero-Copy Programming](https://en.wikipedia.org/wiki/Zero-copy)
- [Memory Pool Patterns](https://en.wikipedia.org/wiki/Object_pool_pattern)

### 进阶主题
- [Memory Layout](https://doc.rust-lang.org/reference/type-layout.html)
- [Unsafe Rust](https://doc.rust-lang.org/book/ch19-01-unsafe-rust.html)
- [FFI](https://doc.rust-lang.org/nomicon/ffi.html)

---

这个文档详细介绍了CANP项目中的内存管理技术。建议结合实际代码进行学习，并在实践中不断优化内存使用效率。 