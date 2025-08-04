# Rayon 并行计算学习指南

## 📚 概述

Rayon是Rust生态系统中最重要的并行计算库，为CANP项目提供了高性能的CPU密集型任务处理能力。本文档详细介绍Rayon的核心概念、使用方法和最佳实践。

## 🏗️ 核心概念

### 1. 并行计算基础

#### 什么是并行计算

并行计算是指同时使用多个处理器核心来执行计算任务，以提高性能。

```rust
// 串行版本
fn sum_serial(numbers: &[i32]) -> i32 {
    numbers.iter().sum()
}

// 并行版本
use rayon::prelude::*;

fn sum_parallel(numbers: &[i32]) -> i32 {
    numbers.par_iter().sum()
}
```

#### 工作窃取调度

Rayon使用工作窃取调度算法，每个线程都有自己的任务队列，当队列为空时会从其他线程"窃取"任务。

```rust
use rayon::prelude::*;

// 自动并行化
let result: i32 = (1..=1000000).par_iter().sum();

// 并行迭代
let doubled: Vec<i32> = (1..=1000).par_iter().map(|x| x * 2).collect();
```

### 2. 线程池管理

#### 创建线程池

```rust
use rayon::ThreadPoolBuilder;

// 自定义线程池
let pool = ThreadPoolBuilder::new()
    .num_threads(8)
    .stack_size(32 * 1024 * 1024) // 32MB 栈大小
    .build()
    .unwrap();

// 在线程池中执行任务
pool.install(|| {
    let result: i32 = (1..=1000000).par_iter().sum();
    println!("结果: {}", result);
});
```

#### 在CANP中的应用

```rust
// 高性能执行器中的Rayon线程池
pub struct HighPerformanceExecutor {
    cpu_pool: Arc<rayon::ThreadPool>,
    cpu_task_tx: mpsc::UnboundedSender<(TaskMetadata, BoxedCpuTask)>,
    // ... 其他字段
}

impl HighPerformanceExecutor {
    pub fn new(config: ExecutorConfig) -> Self {
        let cpu_pool = Arc::new(
            rayon::ThreadPoolBuilder::new()
                .num_threads(config.cpu_worker_threads)
                .stack_size(32 * 1024 * 1024)
                .build()
                .unwrap()
        );
        
        // ... 初始化其他字段
        Self { cpu_pool, cpu_task_tx, /* ... */ }
    }
}
```

## 🔄 并行迭代器

### 1. 基本并行迭代器

#### par_iter() - 并行迭代

```rust
use rayon::prelude::*;

// 并行求和
let numbers = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
let sum: i32 = numbers.par_iter().sum();
println!("总和: {}", sum);

// 并行映射
let doubled: Vec<i32> = numbers.par_iter().map(|&x| x * 2).collect();
println!("加倍后: {:?}", doubled);

// 并行过滤
let evens: Vec<i32> = numbers.par_iter().filter(|&&x| x % 2 == 0).cloned().collect();
println!("偶数: {:?}", evens);
```

#### par_iter_mut() - 可变并行迭代

```rust
use rayon::prelude::*;

let mut numbers = vec![1, 2, 3, 4, 5];
numbers.par_iter_mut().for_each(|x| *x *= 2);
println!("修改后: {:?}", numbers);
```

#### into_par_iter() - 消费并行迭代

```rust
use rayon::prelude::*;

let numbers = vec![1, 2, 3, 4, 5];
let sum: i32 = numbers.into_par_iter().sum();
// numbers 已经被消费，不能再使用
```

### 2. 在CANP中的应用

```rust
// 并行处理CAN帧
impl DbcManager {
    pub fn parse_can_frames_parallel(&self, frames: &[CanFrame]) -> Vec<Option<ParsedMessage>> {
        frames.par_iter()
            .map(|frame| self.parse_can_frame(frame).unwrap_or(None))
            .collect()
    }
}

// 并行数据压缩
impl DataLayerParser {
    pub fn compress_data_parallel(&self, data_chunks: Vec<Vec<u8>>) -> Vec<Vec<u8>> {
        data_chunks.par_iter()
            .map(|chunk| {
                let mut compressed = Vec::new();
                let mut encoder = flate2::write::GzEncoder::new(&mut compressed, flate2::Compression::default());
                std::io::copy(&mut std::io::Cursor::new(chunk), &mut encoder).unwrap();
                encoder.finish().unwrap();
                compressed
            })
            .collect()
    }
}
```

## 🎯 并行算法

### 1. 并行排序

```rust
use rayon::prelude::*;

// 并行排序
let mut numbers = vec![3, 1, 4, 1, 5, 9, 2, 6, 5, 3, 5];
numbers.par_sort();
println!("排序后: {:?}", numbers);

// 并行排序（不稳定）
numbers.par_sort_unstable();

// 自定义比较函数
numbers.par_sort_by(|a, b| b.cmp(a)); // 降序排序
```

### 2. 并行归约

```rust
use rayon::prelude::*;

// 并行归约
let numbers = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10];

// 求和
let sum: i32 = numbers.par_iter().sum();

// 求最大值
let max: Option<&i32> = numbers.par_iter().max();

// 求最小值
let min: Option<&i32> = numbers.par_iter().min();

// 自定义归约
let product: i32 = numbers.par_iter().product();
```

### 3. 在CANP中的应用

```rust
// 并行统计计算
impl ProcessingStats {
    pub fn calculate_stats_parallel(&self, data: &[ProcessedFrame]) -> Statistics {
        let (total_frames, total_bytes, avg_processing_time) = data.par_iter()
            .fold(
                || (0usize, 0usize, 0.0f64),
                |(frames, bytes, time), frame| {
                    (frames + 1, bytes + frame.data.len(), time + frame.processing_time)
                }
            )
            .reduce(
                || (0, 0, 0.0),
                |(f1, b1, t1), (f2, b2, t2)| (f1 + f2, b1 + b2, t1 + t2)
            );
        
        Statistics {
            total_frames,
            total_bytes,
            avg_processing_time: if total_frames > 0 { avg_processing_time / total_frames as f64 } else { 0.0 },
        }
    }
}
```

## 🔧 自定义并行任务

### 1. join() - 并行执行两个任务

```rust
use rayon::prelude::*;

fn fibonacci(n: u32) -> u32 {
    match n {
        0 => 0,
        1 => 1,
        n => {
            let (a, b) = rayon::join(
                || fibonacci(n - 1),
                || fibonacci(n - 2)
            );
            a + b
        }
    }
}

// 并行处理两个独立的任务
let (result1, result2) = rayon::join(
    || expensive_computation_1(),
    || expensive_computation_2()
);
```

### 2. scope() - 并行作用域

```rust
use rayon::prelude::*;

let mut numbers = vec![1, 2, 3, 4, 5];

rayon::scope(|s| {
    // 在作用域中生成并行任务
    for num in &mut numbers {
        s.spawn(move |_| {
            *num *= 2;
        });
    }
});

println!("修改后: {:?}", numbers);
```

### 3. 在CANP中的应用

```rust
// 并行文件处理
impl DataProcessingPipeline {
    pub fn process_files_parallel(&self, files: Vec<PathBuf>) -> Vec<Result<ProcessedFile>> {
        files.par_iter()
            .map(|file_path| {
                let mut parser = DataLayerParser::new(self.memory_pool.clone());
                parser.parse_file(file_path)
            })
            .collect()
    }
}

// 并行DBC解析
impl DbcManager {
    pub fn parse_signals_parallel(&self, signals: &[Signal], data: &[u8]) -> Vec<ParsedSignal> {
        signals.par_iter()
            .map(|signal| {
                self.parse_signal(signal, data, &self.current_dbc_path)
                    .unwrap_or_else(|_| ParsedSignal::default())
            })
            .collect()
    }
}
```

## 📊 性能优化

### 1. 数据局部性

```rust
use rayon::prelude::*;

// 好的做法：保持数据局部性
fn process_data_good(data: &[u32]) -> Vec<u32> {
    data.par_iter()
        .map(|&x| expensive_computation(x))
        .collect()
}

// 避免的做法：频繁的内存分配
fn process_data_bad(data: &[u32]) -> Vec<u32> {
    data.par_iter()
        .flat_map(|&x| {
            let mut result = Vec::new();
            for i in 0..x {
                result.push(expensive_computation(i));
            }
            result
        })
        .collect()
}
```

### 2. 负载均衡

```rust
use rayon::prelude::*;

// 使用 chunks 进行负载均衡
fn process_large_data(data: &[u32]) -> Vec<u32> {
    data.par_chunks(1000) // 每个块1000个元素
        .flat_map(|chunk| {
            chunk.iter().map(|&x| expensive_computation(x))
        })
        .collect()
}

// 自定义分块策略
fn process_with_custom_chunks(data: &[u32]) -> Vec<u32> {
    data.par_chunks(if data.len() > 10000 { 1000 } else { 100 })
        .flat_map(|chunk| {
            chunk.iter().map(|&x| expensive_computation(x))
        })
        .collect()
}
```

### 3. 在CANP中的应用

```rust
// 优化的并行帧处理
impl DataLayerParser {
    pub fn parse_frame_sequences_parallel(&self, sequences: &[FrameSequence]) -> Vec<ParsedSequence> {
        sequences.par_chunks(100) // 每100个序列一个块
            .flat_map(|chunk| {
                chunk.iter().map(|sequence| {
                    self.parse_single_sequence(sequence)
                        .unwrap_or_else(|_| ParsedSequence::default())
                })
            })
            .collect()
    }
    
    pub fn compress_chunks_parallel(&self, chunks: Vec<Vec<u8>>) -> Vec<Vec<u8>> {
        chunks.par_iter()
            .map(|chunk| {
                let mut compressed = Vec::with_capacity(chunk.len() / 2); // 预分配
                let mut encoder = flate2::write::GzEncoder::new(&mut compressed, flate2::Compression::fast());
                std::io::copy(&mut std::io::Cursor::new(chunk), &mut encoder).unwrap();
                encoder.finish().unwrap();
                compressed
            })
            .collect()
    }
}
```

## 🔒 线程安全

### 1. 共享状态管理

```rust
use rayon::prelude::*;
use std::sync::{Arc, Mutex};

// 使用 Arc<Mutex<T>> 共享状态
fn parallel_counter(numbers: &[i32]) -> i32 {
    let counter = Arc::new(Mutex::new(0));
    
    numbers.par_iter().for_each(|&num| {
        if num > 5 {
            let mut count = counter.lock().unwrap();
            *count += 1;
        }
    });
    
    *counter.lock().unwrap()
}

// 更好的做法：使用归约
fn parallel_counter_better(numbers: &[i32]) -> i32 {
    numbers.par_iter()
        .filter(|&&num| num > 5)
        .count() as i32
}
```

### 2. 原子操作

```rust
use rayon::prelude::*;
use std::sync::atomic::{AtomicUsize, Ordering};

// 使用原子操作
fn parallel_atomic_counter(numbers: &[i32]) -> usize {
    let counter = AtomicUsize::new(0);
    
    numbers.par_iter().for_each(|&num| {
        if num > 5 {
            counter.fetch_add(1, Ordering::Relaxed);
        }
    });
    
    counter.load(Ordering::Relaxed)
}
```

### 3. 在CANP中的应用

```rust
// 线程安全的统计收集
use std::sync::atomic::{AtomicUsize, AtomicU64, Ordering};

pub struct AtomicProcessingStats {
    files_processed: AtomicUsize,
    frames_parsed: AtomicUsize,
    total_bytes: AtomicU64,
}

impl AtomicProcessingStats {
    pub fn new() -> Self {
        Self {
            files_processed: AtomicUsize::new(0),
            frames_parsed: AtomicUsize::new(0),
            total_bytes: AtomicU64::new(0),
        }
    }
    
    pub fn increment_files(&self) {
        self.files_processed.fetch_add(1, Ordering::Relaxed);
    }
    
    pub fn add_frames(&self, count: usize) {
        self.frames_parsed.fetch_add(count, Ordering::Relaxed);
    }
    
    pub fn add_bytes(&self, bytes: u64) {
        self.total_bytes.fetch_add(bytes, Ordering::Relaxed);
    }
    
    pub fn get_stats(&self) -> ProcessingStats {
        ProcessingStats {
            files_processed: self.files_processed.load(Ordering::Relaxed),
            frames_parsed: self.frames_parsed.load(Ordering::Relaxed),
            total_bytes: self.total_bytes.load(Ordering::Relaxed),
        }
    }
}
```

## 🎯 最佳实践

### 1. 任务粒度

```rust
use rayon::prelude::*;

// 好的粒度：适中的任务大小
fn good_granularity(data: &[u32]) -> Vec<u32> {
    data.par_iter()
        .map(|&x| expensive_computation(x))
        .collect()
}

// 避免过细的粒度
fn bad_granularity(data: &[u32]) -> Vec<u32> {
    data.par_iter()
        .flat_map(|&x| {
            // 每个元素生成太多小任务
            (0..1000).map(|i| simple_computation(x, i))
        })
        .collect()
}
```

### 2. 内存管理

```rust
use rayon::prelude::*;

// 预分配内存
fn efficient_memory_usage(data: &[u32]) -> Vec<u32> {
    let mut result = Vec::with_capacity(data.len());
    data.par_iter()
        .map(|&x| expensive_computation(x))
        .collect_into_vec(&mut result);
    result
}

// 避免频繁分配
fn avoid_frequent_allocation(data: &[u32]) -> Vec<u32> {
    data.par_iter()
        .fold(
            Vec::new,
            |mut acc, &x| {
                acc.push(expensive_computation(x));
                acc
            }
        )
        .reduce(
            Vec::new,
            |mut acc, mut vec| {
                acc.append(&mut vec);
                acc
            }
        )
}
```

### 3. 错误处理

```rust
use rayon::prelude::*;
use anyhow::Result;

// 并行错误处理
fn parallel_with_error_handling(data: &[u32]) -> Result<Vec<u32>> {
    let results: Vec<Result<u32>> = data.par_iter()
        .map(|&x| {
            expensive_computation_with_error(x)
        })
        .collect();
    
    // 收集所有错误
    let mut errors = Vec::new();
    let mut successes = Vec::new();
    
    for result in results {
        match result {
            Ok(value) => successes.push(value),
            Err(e) => errors.push(e),
        }
    }
    
    if errors.is_empty() {
        Ok(successes)
    } else {
        Err(anyhow!("处理过程中出现 {} 个错误", errors.len()))
    }
}
```

## 🔧 调试和监控

### 1. 性能分析

```rust
use rayon::prelude::*;
use std::time::Instant;

// 并行性能分析
fn profile_parallel_operation<F, T>(name: &str, operation: F) -> T
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
let result = profile_parallel_operation("并行排序", || {
    let mut data = vec![3, 1, 4, 1, 5, 9, 2, 6];
    data.par_sort();
    data
});
```

### 2. 线程池监控

```rust
use rayon::prelude::*;

// 监控线程池状态
fn monitor_thread_pool() {
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(8)
        .build()
        .unwrap();
    
    pool.install(|| {
        println!("当前线程数: {}", rayon::current_num_threads());
        println!("当前线程索引: {}", rayon::current_thread_index().unwrap_or(0));
        
        // 执行并行任务
        let result: i32 = (1..=1000000).par_iter().sum();
        println!("计算结果: {}", result);
    });
}
```

### 3. 在CANP中的应用

```rust
// 性能监控的并行处理
impl DataProcessingPipeline {
    pub fn process_with_monitoring(&self, files: Vec<PathBuf>) -> ProcessingResult {
        let start = Instant::now();
        
        let processed_files = files.par_iter()
            .map(|file_path| {
                let file_start = Instant::now();
                let result = self.process_single_file(file_path);
                let file_duration = file_start.elapsed();
                
                println!("文件 {:?} 处理时间: {:?}", file_path, file_duration);
                result
            })
            .collect::<Vec<_>>();
        
        let total_duration = start.elapsed();
        println!("总处理时间: {:?}", total_duration);
        
        ProcessingResult {
            files_processed: processed_files.len(),
            processing_time_ms: total_duration.as_millis() as u64,
            // ... 其他字段
        }
    }
}
```

## 📚 学习资源

### 官方文档
- [Rayon Documentation](https://docs.rs/rayon/)
- [Rayon GitHub](https://github.com/rayon-rs/rayon)
- [Rayon Examples](https://github.com/rayon-rs/rayon/tree/master/rayon-demo)

### 社区资源
- [Rayon Tutorial](https://github.com/rayon-rs/rayon/blob/master/README.md)
- [Parallel Programming in Rust](https://rust-lang.github.io/async-book/parallel.html)
- [Rayon Best Practices](https://github.com/rayon-rs/rayon/blob/master/FAQ.md)

### 进阶主题
- [Work Stealing](https://en.wikipedia.org/wiki/Work_stealing)
- [Parallel Algorithms](https://en.wikipedia.org/wiki/Parallel_algorithm)
- [Lock-free Programming](https://en.wikipedia.org/wiki/Lock-free_programming)

---

这个文档详细介绍了Rayon在CANP项目中的应用。建议结合实际代码进行学习，并在实践中不断优化并行编程技能。 