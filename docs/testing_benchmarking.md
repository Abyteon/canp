# 测试和基准测试学习指南

## 📚 概述

测试是确保代码质量和可靠性的关键环节。CANP项目采用全面的测试策略，包括单元测试、集成测试、属性测试和基准测试。本文档详细介绍各种测试方法和最佳实践。

## 🧪 单元测试

### 1. 基本单元测试

#### 测试结构

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_functionality() {
        let input = vec![1, 2, 3, 4, 5];
        let expected = vec![2, 4, 6, 8, 10];
        let result = double_elements(&input);
        assert_eq!(result, expected);
    }

    #[test]
    fn test_empty_input() {
        let input = vec![];
        let result = double_elements(&input);
        assert_eq!(result, vec![]);
    }

    #[test]
    fn test_large_input() {
        let input: Vec<i32> = (1..=1000).collect();
        let result = double_elements(&input);
        assert_eq!(result.len(), 1000);
        assert_eq!(result[0], 2);
        assert_eq!(result[999], 2000);
    }
}

fn double_elements(input: &[i32]) -> Vec<i32> {
    input.iter().map(|x| x * 2).collect()
}
```

#### 在CANP中的应用

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_header_parsing() {
        let header_data = [
            0x01, 0x02, 0x03, 0x04, // 压缩数据长度
            0x05, 0x06, 0x07, 0x08, // 其他字段
            // ... 更多测试数据
        ];
        
        let header = FileHeader::from_bytes(&header_data).unwrap();
        assert_eq!(header.compressed_length, 0x04030201);
    }

    #[test]
    fn test_invalid_header() {
        let invalid_data = [0x01, 0x02]; // 数据不足
        let result = FileHeader::from_bytes(&invalid_data);
        assert!(result.is_err());
    }
}
```

### 2. 测试辅助函数

#### 测试数据生成

```rust
#[cfg(test)]
mod test_helpers {
    use super::*;

    pub fn create_test_file_header() -> FileHeader {
        FileHeader {
            compressed_length: 1024,
            original_length: 2048,
            timestamp: 1234567890,
            checksum: 0xABCDEF01,
        }
    }

    pub fn create_test_data(size: usize) -> Vec<u8> {
        (0..size).map(|i| (i % 256) as u8).collect()
    }

    pub fn create_test_mmap(data: &[u8]) -> Mmap {
        let temp_file = tempfile::tempfile().unwrap();
        std::fs::write(&temp_file, data).unwrap();
        unsafe { Mmap::map(&temp_file).unwrap() }
    }
}
```

#### 测试夹具 (Fixtures)

```rust
#[cfg(test)]
mod fixtures {
    use super::*;

    pub struct TestFixture {
        pub memory_pool: ZeroCopyMemoryPool,
        pub test_data: Vec<u8>,
    }

    impl TestFixture {
        pub fn new() -> Self {
            Self {
                memory_pool: ZeroCopyMemoryPool::new(1024 * 1024),
                test_data: create_test_data(1000),
            }
        }

        pub fn with_large_data(self, size: usize) -> Self {
            Self {
                test_data: create_test_data(size),
                ..self
            }
        }
    }

    impl Drop for TestFixture {
        fn drop(&mut self) {
            // 清理测试资源
        }
    }
}
```

## 🔄 集成测试

### 1. 端到端测试

#### 测试文件结构

```rust
// tests/integration_test.rs
use canp::{ProcessingPipeline, Config};

#[tokio::test]
async fn test_full_processing_pipeline() {
    // 设置测试环境
    let config = Config {
        max_memory_usage: 1024 * 1024 * 1024,
        worker_threads: 4,
        batch_size: 1000,
    };

    let pipeline = ProcessingPipeline::new(config).await.unwrap();
    
    // 创建测试文件
    let test_file = create_test_can_file("test_data.bin").await.unwrap();
    
    // 执行处理
    let result = pipeline.process_file(&test_file).await.unwrap();
    
    // 验证结果
    assert!(result.frames_processed > 0);
    assert!(result.processing_time.as_millis() > 0);
}

async fn create_test_can_file(path: &str) -> Result<PathBuf> {
    // 创建测试CAN数据文件
    let test_data = generate_test_can_data(1000);
    tokio::fs::write(path, test_data).await?;
    Ok(PathBuf::from(path))
}
```

### 2. 组件交互测试

```rust
#[tokio::test]
async fn test_memory_pool_and_executor_integration() {
    let memory_pool = Arc::new(ZeroCopyMemoryPool::new(1024 * 1024));
    let executor = Arc::new(HighPerformanceExecutor::new(ExecutorConfig::default()));
    
    // 测试内存池和执行器的交互
    let test_data = create_test_data(10000);
    let mmap = memory_pool.get_mmap("test.bin").unwrap();
    
    let task = async move {
        // 模拟数据处理任务
        let result = process_data(&mmap).await.unwrap();
        assert_eq!(result.len(), test_data.len());
    };
    
    executor.submit_io_task(task).await.unwrap();
    executor.shutdown().await;
}
```

## 🎲 属性测试

### 1. 使用 proptest

#### 基本属性测试

```rust
use proptest::prelude::*;

proptest! {
    #[test]
    fn test_data_parsing_roundtrip(data in prop::collection::vec(any::<u8>(), 0..1000)) {
        // 测试数据解析的往返性
        let parsed = parse_data(&data).unwrap();
        let serialized = serialize_data(&parsed).unwrap();
        prop_assert_eq!(data, serialized);
    }

    #[test]
    fn test_memory_pool_allocation(size in 1..10000usize) {
        let pool = ZeroCopyMemoryPool::new(1024 * 1024);
        let buffer = pool.get_decompress_buffer(size).unwrap();
        prop_assert_eq!(buffer.capacity(), size);
    }

    #[test]
    fn test_can_frame_validation(
        id in 0..0x1FFFFFFFu32,
        data in prop::collection::vec(any::<u8>(), 0..8)
    ) {
        let frame = CanFrame { id, data: data.clone() };
        let validation_result = validate_can_frame(&frame);
        
        // 验证CAN帧的有效性
        if data.len() <= 8 {
            prop_assert!(validation_result.is_ok());
        } else {
            prop_assert!(validation_result.is_err());
        }
    }
}
```

#### 复杂属性测试

```rust
proptest! {
    #[test]
    fn test_processing_pipeline_invariants(
        file_count in 1..10usize,
        file_size in 1000..100000usize
    ) {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        
        runtime.block_on(async {
            let pipeline = ProcessingPipeline::new(Config::default()).await.unwrap();
            
            // 创建多个测试文件
            let test_files = (0..file_count)
                .map(|i| create_test_file(i, file_size))
                .collect::<Vec<_>>();
            
            // 处理所有文件
            let results = futures::future::join_all(
                test_files.iter().map(|file| pipeline.process_file(file))
            ).await;
            
            // 验证不变性
            let success_count = results.iter().filter(|r| r.is_ok()).count();
            prop_assert!(success_count > 0);
            
            // 验证总处理时间合理
            let total_time: Duration = results.iter()
                .filter_map(|r| r.as_ref().ok().map(|r| r.processing_time))
                .sum();
            
            prop_assert!(total_time.as_secs() < 60); // 不应超过60秒
        });
    }
}
```

### 2. 自定义策略

```rust
// 自定义测试数据生成策略
fn can_frame_strategy() -> impl Strategy<Value = CanFrame> {
    (0..0x1FFFFFFFu32, prop::collection::vec(any::<u8>(), 0..8))
        .prop_map(|(id, data)| CanFrame { id, data })
}

fn dbc_message_strategy() -> impl Strategy<Value = DbcMessage> {
    (
        any::<String>(),
        0..0x1FFFFFFFu32,
        prop::collection::vec(any::<DbcSignal>(), 0..10)
    ).prop_map(|(name, id, signals)| DbcMessage { name, id, signals })
}

proptest! {
    #[test]
    fn test_dbc_parsing_properties(message in dbc_message_strategy()) {
        let dbc_content = format_dbc_message(&message);
        let parsed = parse_dbc_message(&dbc_content).unwrap();
        
        prop_assert_eq!(message.name, parsed.name);
        prop_assert_eq!(message.id, parsed.id);
        prop_assert_eq!(message.signals.len(), parsed.signals.len());
    }
}
```

## ⚡ 基准测试

### 1. 使用 criterion

#### 基本基准测试

```rust
// benches/benchmarks.rs
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use canp::{ZeroCopyMemoryPool, ProcessingPipeline};

fn benchmark_memory_pool_allocation(c: &mut Criterion) {
    let pool = ZeroCopyMemoryPool::new(1024 * 1024 * 1024);
    
    c.bench_function("allocate_small_buffer", |b| {
        b.iter(|| {
            let buffer = pool.get_decompress_buffer(black_box(1024)).unwrap();
            black_box(buffer);
        });
    });
    
    c.bench_function("allocate_large_buffer", |b| {
        b.iter(|| {
            let buffer = pool.get_decompress_buffer(black_box(1024 * 1024)).unwrap();
            black_box(buffer);
        });
    });
}

fn benchmark_data_parsing(c: &mut Criterion) {
    let test_data = create_test_data(10000);
    
    c.bench_function("parse_small_data", |b| {
        b.iter(|| {
            let result = parse_data(black_box(&test_data[..1000])).unwrap();
            black_box(result);
        });
    });
    
    c.bench_function("parse_large_data", |b| {
        b.iter(|| {
            let result = parse_data(black_box(&test_data)).unwrap();
            black_box(result);
        });
    });
}
```

#### 复杂基准测试

```rust
fn benchmark_processing_pipeline(c: &mut Criterion) {
    let mut group = c.benchmark_group("pipeline");
    
    // 测试不同文件大小的处理性能
    for size in [1000, 10000, 100000] {
        group.bench_function(&format!("process_{}_bytes", size), |b| {
            b.iter_custom(|iters| {
                let runtime = tokio::runtime::Runtime::new().unwrap();
                let start = std::time::Instant::now();
                
                runtime.block_on(async {
                    let pipeline = ProcessingPipeline::new(Config::default()).await.unwrap();
                    let test_file = create_test_file(size).await.unwrap();
                    
                    for _ in 0..iters {
                        let _result = pipeline.process_file(&test_file).await.unwrap();
                    }
                });
                
                start.elapsed()
            });
        });
    }
    
    group.finish();
}

fn benchmark_concurrent_processing(c: &mut Criterion) {
    c.bench_function("concurrent_file_processing", |b| {
        b.iter_custom(|iters| {
            let runtime = tokio::runtime::Runtime::new().unwrap();
            let start = std::time::Instant::now();
            
            runtime.block_on(async {
                let pipeline = ProcessingPipeline::new(Config::default()).await.unwrap();
                let test_files = create_test_files(10, 10000).await.unwrap();
                
                for _ in 0..iters {
                    let _results = futures::future::join_all(
                        test_files.iter().map(|file| pipeline.process_file(file))
                    ).await;
                }
            });
            
            start.elapsed()
        });
    });
}
```

### 2. 内存基准测试

```rust
fn benchmark_memory_usage(c: &mut Criterion) {
    c.bench_function("memory_pool_usage", |b| {
        b.iter_custom(|iters| {
            let start_memory = get_memory_usage();
            let start = std::time::Instant::now();
            
            {
                let pool = ZeroCopyMemoryPool::new(1024 * 1024 * 1024);
                
                for _ in 0..iters {
                    let buffers: Vec<_> = (0..100)
                        .map(|i| pool.get_decompress_buffer(1024 * (i + 1)).unwrap())
                        .collect();
                    black_box(buffers);
                }
            }
            
            let end_memory = get_memory_usage();
            let memory_used = end_memory - start_memory;
            
            println!("内存使用: {} MB", memory_used / 1024 / 1024);
            start.elapsed()
        });
    });
}

fn get_memory_usage() -> usize {
    // 获取当前进程内存使用量
    #[cfg(target_os = "linux")]
    {
        use std::fs;
        let status = fs::read_to_string("/proc/self/status").unwrap();
        for line in status.lines() {
            if line.starts_with("VmRSS:") {
                let parts: Vec<&str> = line.split_whitespace().collect();
                return parts[1].parse::<usize>().unwrap() * 1024; // 转换为字节
            }
        }
    }
    0
}
```

## 🔍 模糊测试

### 1. 使用 cargo-fuzz

#### 设置模糊测试

```rust
// fuzz/fuzz_targets/parse_data.rs
#![no_main]

use libfuzzer_sys::fuzz_target;
use canp::data_layer_parser::DataLayerParser;

fuzz_target!(|data: &[u8]| {
    // 模糊测试数据解析
    let mut parser = DataLayerParser::new();
    let _result = parser.parse_data(data);
    // 不检查结果，只确保不会崩溃
});

// fuzz/fuzz_targets/dbc_parser.rs
#![no_main]

use libfuzzer_sys::fuzz_target;
use canp::dbc_parser::DbcParser;

fuzz_target!(|data: &[u8]| {
    // 模糊测试DBC解析
    let mut parser = DbcParser::new();
    let _result = parser.parse_dbc_content(data);
});
```

### 2. 自定义模糊测试

```rust
#[cfg(test)]
mod fuzz_tests {
    use super::*;

    #[test]
    fn fuzz_test_memory_pool() {
        // 模拟模糊测试
        for _ in 0..1000 {
            let size = rand::random::<usize>() % 100000;
            let pool = ZeroCopyMemoryPool::new(1024 * 1024);
            
            // 随机分配和释放缓冲区
            let buffers: Vec<_> = (0..10)
                .map(|_| pool.get_decompress_buffer(size).ok())
                .collect();
            
            // 确保不会崩溃
            assert!(buffers.iter().any(|b| b.is_some()));
        }
    }
}
```

## 🧪 测试最佳实践

### 1. 测试组织

```rust
#[cfg(test)]
mod tests {
    use super::*;

    // 单元测试
    mod unit {
        use super::*;

        #[test]
        fn test_basic_functionality() {
            // 基本功能测试
        }

        #[test]
        fn test_edge_cases() {
            // 边界情况测试
        }
    }

    // 集成测试
    mod integration {
        use super::*;

        #[tokio::test]
        async fn test_component_interaction() {
            // 组件交互测试
        }
    }

    // 性能测试
    mod performance {
        use super::*;

        #[test]
        fn test_memory_efficiency() {
            // 内存效率测试
        }
    }
}
```

### 2. 测试数据管理

```rust
#[cfg(test)]
mod test_data {
    use super::*;

    pub struct TestDataManager {
        temp_dir: tempfile::TempDir,
    }

    impl TestDataManager {
        pub fn new() -> Self {
            Self {
                temp_dir: tempfile::tempdir().unwrap(),
            }
        }

        pub fn create_test_file(&self, name: &str, data: &[u8]) -> PathBuf {
            let path = self.temp_dir.path().join(name);
            std::fs::write(&path, data).unwrap();
            path
        }

        pub fn create_large_test_file(&self, name: &str, size: usize) -> PathBuf {
            let data = create_test_data(size);
            self.create_test_file(name, &data)
        }
    }

    impl Drop for TestDataManager {
        fn drop(&mut self) {
            // 自动清理临时文件
        }
    }
}
```

### 3. 异步测试

```rust
#[cfg(test)]
mod async_tests {
    use super::*;

    #[tokio::test]
    async fn test_async_processing() {
        let pipeline = ProcessingPipeline::new(Config::default()).await.unwrap();
        let test_file = create_test_file(1000).await.unwrap();
        
        let result = pipeline.process_file(&test_file).await.unwrap();
        assert!(result.frames_processed > 0);
    }

    #[tokio::test]
    async fn test_concurrent_processing() {
        let pipeline = Arc::new(ProcessingPipeline::new(Config::default()).await.unwrap());
        let test_files = create_test_files(10, 1000).await.unwrap();
        
        let handles: Vec<_> = test_files.iter()
            .map(|file| {
                let pipeline = Arc::clone(&pipeline);
                let file = file.clone();
                tokio::spawn(async move {
                    pipeline.process_file(&file).await
                })
            })
            .collect();
        
        let results = futures::future::join_all(handles).await;
        assert!(results.iter().all(|r| r.is_ok()));
    }
}
```

## 📊 测试覆盖率

### 1. 覆盖率工具

```bash
# 安装覆盖率工具
cargo install cargo-tarpaulin

# 运行覆盖率测试
cargo tarpaulin --out Html --output-dir coverage

# 查看覆盖率报告
open coverage/tarpaulin-report.html
```

### 2. 覆盖率目标

```rust
// 在代码中添加覆盖率标记
#[cfg(test)]
mod coverage_tests {
    use super::*;

    #[test]
    fn test_all_error_paths() {
        // 测试所有错误路径以提高覆盖率
        let result = parse_invalid_data();
        assert!(result.is_err());
        
        let result = process_empty_data();
        assert!(result.is_ok());
    }
}
```

## 📚 总结

全面的测试策略是确保CANP项目质量的关键。通过结合单元测试、集成测试、属性测试和基准测试，我们可以：

- 验证代码的正确性和健壮性
- 发现性能瓶颈和优化机会
- 确保系统在各种条件下的稳定性
- 提供回归测试保护

关键要点：
- 使用 `#[cfg(test)]` 组织测试代码
- 编写全面的单元测试覆盖核心功能
- 使用 `proptest` 进行属性测试
- 使用 `criterion` 进行性能基准测试
- 实现适当的测试数据管理
- 监控测试覆盖率 