# CANP API 参考文档

## 📚 概述

本文档提供了CANP库的完整API参考，包括所有公共接口、数据结构、配置选项和使用示例。

## 🏗️ 核心模块

### 零拷贝内存池 (Zero-Copy Memory Pool)

#### 结构体

##### `ZeroCopyMemoryPool`

零拷贝内存池的主要结构体，提供高效的内存管理。

```rust
pub struct ZeroCopyMemoryPool {
    decompress_pools: Vec<Arc<LockPool<BytesMut, 64, 512>>>,
    mmap_cache: Arc<RwLock<lru::LruCache<String, Arc<Mmap>>>>,
    current_memory_usage: Arc<RwLock<usize>>,
}
```

**方法**:

###### `new(config: MemoryPoolConfig) -> Self`

创建新的内存池实例。

```rust
let config = MemoryPoolConfig::default();
let pool = ZeroCopyMemoryPool::new(config);
```

###### `get_decompress_buffer(&self, size: usize) -> MutableMemoryBuffer`

获取解压缩缓冲区。

```rust
let buffer = pool.get_decompress_buffer(1024)?;
assert!(buffer.buffer.capacity() >= 1024);
```

###### `get_decompress_buffers_batch(&self, sizes: &[usize]) -> Vec<MutableMemoryBuffer>`

批量获取解压缩缓冲区。

```rust
let sizes = vec![1024, 2048, 4096];
let buffers = pool.get_decompress_buffers_batch(&sizes);
assert_eq!(buffers.len(), 3);
```

###### `map_file<P: AsRef<Path>>(&self, file_path: P) -> Result<MemoryMappedBlock>`

内存映射文件。

```rust
let mapped_file = pool.map_file("data.bin")?;
let data = mapped_file.as_slice();
```

###### `get_stats(&self) -> MemoryPoolStats`

获取内存池统计信息。

```rust
let stats = pool.get_stats();
println!("当前内存使用: {}MB", stats.total_memory_usage_mb);
```

##### `MemoryPoolConfig`

内存池配置结构体。

```rust
pub struct MemoryPoolConfig {
    pub decompress_buffer_sizes: Vec<usize>,
    pub mmap_cache_size: usize,
    pub max_memory_usage: usize,
    pub memory_warning_threshold: f64,
}
```

**默认值**:
- `decompress_buffer_sizes`: `[1024, 2048, 4096, 8192, 16384]`
- `mmap_cache_size`: `1000`
- `max_memory_usage`: `1024 * 1024 * 1024` (1GB)
- `memory_warning_threshold`: `0.8`

##### `MemoryPoolStats`

内存池统计信息。

```rust
pub struct MemoryPoolStats {
    pub total_memory_usage_mb: f64,
    pub mmap_cache_hits: usize,
    pub mmap_cache_misses: usize,
    pub decompress_buffer_allocations: usize,
    pub decompress_buffer_releases: usize,
}
```

##### `MemoryMappedBlock`

内存映射块。

```rust
pub struct MemoryMappedBlock {
    mmap: Arc<Mmap>,
    file_path: PathBuf,
}
```

**方法**:

###### `as_slice(&self) -> &[u8]`

获取数据切片。

```rust
let data = mapped_block.as_slice();
```

###### `len(&self) -> usize`

获取数据长度。

```rust
let length = mapped_block.len();
```

##### `MutableMemoryBuffer`

可变内存缓冲区。

```rust
pub struct MutableMemoryBuffer {
    buffer: BytesMut,
}
```

**方法**:

###### `len(&self) -> usize`

获取缓冲区长度。

```rust
let length = buffer.len();
```

###### `capacity(&self) -> usize`

获取缓冲区容量。

```rust
let capacity = buffer.capacity();
```

###### `extend_from_slice(&mut self, data: &[u8])`

扩展缓冲区。

```rust
buffer.extend_from_slice(b"hello world");
```

### 高性能执行器 (High-Performance Executor)

#### 结构体

##### `HighPerformanceExecutor`

高性能执行器，支持混合任务类型。

```rust
pub struct HighPerformanceExecutor {
    io_task_tx: mpsc::UnboundedSender<(TaskMetadata, BoxedTask)>,
    cpu_task_tx: mpsc::UnboundedSender<(TaskMetadata, BoxedCpuTask)>,
    priority_task_tx: mpsc::UnboundedSender<(TaskMetadata, BoxedTask)>,
    backpressure_semaphore: Arc<Semaphore>,
}
```

**方法**:

###### `new(config: ExecutorConfig) -> Self`

创建新的执行器实例。

```rust
let config = ExecutorConfig::default();
let executor = HighPerformanceExecutor::new(config);
```

###### `submit_io_task<F>(&self, priority: Priority, task: F) -> Result<()>`

提交IO密集型任务。

```rust
executor.submit_io_task(Priority::Normal, || async {
    // 文件读取任务
    Ok(())
})?;
```

###### `submit_cpu_task<F>(&self, priority: Priority, task: F) -> Result<()>`

提交CPU密集型任务。

```rust
executor.submit_cpu_task(Priority::High, || {
    // 数据解析任务
    Ok(())
})?;
```

###### `submit_priority_task<F>(&self, task: F) -> Result<()>`

提交高优先级任务。

```rust
executor.submit_priority_task(|| async {
    // 错误处理任务
    Ok(())
})?;
```

###### `get_stats(&self) -> ExecutorStats`

获取执行器统计信息。

```rust
let stats = executor.get_stats();
println!("完成任务: {}", stats.completed_tasks);
```

###### `shutdown(&self) -> Result<()>`

关闭执行器。

```rust
executor.shutdown()?;
```

##### `ExecutorConfig`

执行器配置结构体。

```rust
pub struct ExecutorConfig {
    pub io_worker_threads: usize,
    pub cpu_worker_threads: usize,
    pub max_queue_length: usize,
    pub task_timeout: Duration,
    pub enable_work_stealing: bool,
}
```

**默认值**:
- `io_worker_threads`: `num_cpus::get() / 2`
- `cpu_worker_threads`: `num_cpus::get()`
- `max_queue_length`: `10000`
- `task_timeout`: `Duration::from_secs(300)`
- `enable_work_stealing`: `true`

##### `ExecutorStats`

执行器统计信息。

```rust
pub struct ExecutorStats {
    pub total_tasks: usize,
    pub completed_tasks: usize,
    pub failed_tasks: usize,
    pub io_tasks: usize,
    pub cpu_tasks: usize,
    pub priority_tasks: usize,
    pub avg_execution_time_ms: f64,
}
```

##### `TaskType`

任务类型枚举。

```rust
pub enum TaskType {
    IoIntensive,
    CpuIntensive,
    Mixed,
    HighPriority,
    Custom(u32),
}
```

**方法**:

###### `suggested_pool(&self) -> &'static str`

获取建议的执行池。

```rust
let pool = TaskType::IoIntensive.suggested_pool(); // "io"
```

###### `weight(&self) -> u32`

获取任务权重。

```rust
let weight = TaskType::HighPriority.weight(); // 10
```

###### `suggested_batch_size(&self) -> usize`

获取建议的批处理大小。

```rust
let batch_size = TaskType::CpuIntensive.suggested_batch_size(); // 15
```

##### `Priority`

任务优先级枚举。

```rust
pub enum Priority {
    Low = 0,
    Normal = 1,
    High = 2,
    Critical = 3,
}
```

##### `TaskMetadata`

任务元数据。

```rust
pub struct TaskMetadata {
    pub id: u64,
    pub task_type: TaskType,
    pub priority: Priority,
    pub created_at: Instant,
}
```

### DBC解析器 (DBC Parser)

#### 结构体

##### `DbcManager`

DBC文件管理器。

```rust
pub struct DbcManager {
    dbc_cache: Arc<RwLock<HashMap<PathBuf, DbcCacheEntry>>>,
    stats: Arc<RwLock<DbcParsingStats>>,
}
```

**方法**:

###### `new(config: DbcManagerConfig) -> Self`

创建新的DBC管理器。

```rust
let config = DbcManagerConfig::default();
let manager = DbcManager::new(config);
```

###### `load_dbc_file<P: AsRef<Path>>(&self, file_path: P, priority: Option<i32>) -> Result<()>`

加载DBC文件。

```rust
manager.load_dbc_file("vehicle.dbc", Some(0))?;
```

###### `load_dbc_directory<P: AsRef<Path>>(&self, dir_path: P) -> Result<usize>`

加载DBC目录中的所有文件。

```rust
let count = manager.load_dbc_directory("dbc_files")?;
println!("加载了 {} 个DBC文件", count);
```

###### `parse_can_frame(&self, frame: &CanFrame) -> Result<Option<ParsedMessage>>`

解析CAN帧。

```rust
let parsed = manager.parse_can_frame(&can_frame)?;
if let Some(message) = parsed {
    println!("解析到消息: {}", message.name);
}
```

###### `get_stats(&self) -> DbcParsingStats`

获取解析统计信息。

```rust
let stats = manager.get_stats();
println!("解析帧数: {}", stats.parsed_frames);
```

###### `reset_stats(&self)`

重置统计信息。

```rust
manager.reset_stats();
```

##### `DbcManagerConfig`

DBC管理器配置。

```rust
pub struct DbcManagerConfig {
    pub max_cached_files: usize,
    pub cache_expire_seconds: u64,
    pub auto_reload: bool,
    pub parallel_loading: bool,
    pub max_load_threads: usize,
}
```

**默认值**:
- `max_cached_files`: `100`
- `cache_expire_seconds`: `3600`
- `auto_reload`: `true`
- `parallel_loading`: `true`
- `max_load_threads`: `4`

##### `DbcParsingStats`

DBC解析统计信息。

```rust
pub struct DbcParsingStats {
    pub parsed_frames: usize,
    pub unknown_messages: usize,
    pub cache_hits: usize,
    pub cache_misses: usize,
    pub total_parse_time_ms: u64,
}
```

##### `CanFrame`

CAN帧结构体。

```rust
pub struct CanFrame {
    pub id: u32,
    pub dlc: u8,
    pub data: Vec<u8>,
    pub timestamp: u64,
    pub frame_type: CanFrameType,
    pub is_remote: bool,
}
```

##### `CanFrameType`

CAN帧类型枚举。

```rust
pub enum CanFrameType {
    Standard,  // 标准帧 (11位ID)
    Extended,  // 扩展帧 (29位ID)
}
```

##### `ParsedMessage`

解析后的消息。

```rust
pub struct ParsedMessage {
    pub name: String,
    pub id: u32,
    pub signals: Vec<ParsedSignal>,
    pub source_dbc: PathBuf,
}
```

##### `ParsedSignal`

解析后的信号。

```rust
pub struct ParsedSignal {
    pub name: String,
    pub raw_value: u64,
    pub physical_value: f64,
    pub unit: Option<String>,
    pub value_description: Option<String>,
}
```

### 数据层解析器 (Data Layer Parser)

#### 结构体

##### `DataLayerParser`

数据层解析器。

```rust
pub struct DataLayerParser {
    memory_pool: ZeroCopyMemoryPool,
    stats: ParsingStats,
}
```

**方法**:

###### `new(memory_pool: ZeroCopyMemoryPool) -> Self`

创建新的数据层解析器。

```rust
let memory_pool = ZeroCopyMemoryPool::default();
let parser = DataLayerParser::new(memory_pool);
```

###### `parse_file(&mut self, file_data: &[u8]) -> Result<ParsedFileData>`

解析文件数据。

```rust
let parsed_data = parser.parse_file(&file_data).await?;
println!("解析了 {} 个帧序列", parsed_data.frame_sequences.len());
```

###### `get_stats(&self) -> ParsingStats`

获取解析统计信息。

```rust
let stats = parser.get_stats();
println!("解析文件数: {}", stats.files_parsed);
```

##### `ParsingStats`

解析统计信息。

```rust
pub struct ParsingStats {
    pub files_parsed: usize,
    pub frame_sequences_parsed: usize,
    pub total_frames: usize,
    pub total_bytes_processed: usize,
    pub avg_parse_time_ms: f64,
}
```

##### `ParsedFileData`

解析后的文件数据。

```rust
pub struct ParsedFileData {
    pub file_header: FileHeader,
    pub decompressed_header: DecompressedHeader,
    pub frame_sequences: Vec<FrameSequence>,
}
```

##### `FileHeader`

文件头部。

```rust
pub struct FileHeader {
    pub magic: [u8; 4],
    pub version: u8,
    pub flags: u8,
    pub reserved: [u8; 26],
    pub compressed_length: u32,
}
```

**方法**:

###### `validate(&self) -> Result<()>`

验证文件头部。

```rust
file_header.validate()?;
```

##### `DecompressedHeader`

解压头部。

```rust
pub struct DecompressedHeader {
    pub magic: [u8; 4],
    pub version: u8,
    pub flags: u8,
    pub reserved: [u8; 10],
    pub decompressed_length: u32,
}
```

##### `FrameSequence`

帧序列。

```rust
pub struct FrameSequence {
    pub length: u32,
    pub reserved: [u8; 12],
    pub frames: Vec<CanFrame>,
}
```

### 列式存储 (Columnar Storage)

#### 结构体

##### `ColumnarStorageWriter`

列式存储写入器。

```rust
pub struct ColumnarStorageWriter {
    partition_strategy: PartitionStrategy,
    compression: CompressionType,
    record_batches: Vec<RecordBatch>,
}
```

**方法**:

###### `new(config: ColumnarStorageConfig) -> Self`

创建新的列式存储写入器。

```rust
let config = ColumnarStorageConfig::default();
let writer = ColumnarStorageWriter::new(config);
```

###### `write_batch(&mut self, batch: RecordBatch) -> Result<()>`

写入记录批次。

```rust
writer.write_batch(record_batch)?;
```

###### `flush(&mut self) -> Result<()>`

刷新数据到磁盘。

```rust
writer.flush()?;
```

##### `ColumnarStorageConfig`

列式存储配置。

```rust
pub struct ColumnarStorageConfig {
    pub output_dir: PathBuf,
    pub partition_strategy: PartitionStrategy,
    pub compression: CompressionType,
    pub batch_size: usize,
    pub max_file_size: usize,
}
```

**默认值**:
- `output_dir`: `PathBuf::from("output")`
- `partition_strategy`: `PartitionStrategy::TimeBased { interval: Duration::from_secs(3600) }`
- `compression`: `CompressionType::Snappy`
- `batch_size`: `10000`
- `max_file_size`: `100 * 1024 * 1024` (100MB)

##### `PartitionStrategy`

分区策略枚举。

```rust
pub enum PartitionStrategy {
    TimeBased { interval: Duration },
    IdBased { bucket_count: usize },
    Custom { partition_fn: Box<dyn Fn(&RecordBatch) -> String> },
}
```

##### `CompressionType`

压缩类型枚举。

```rust
pub enum CompressionType {
    Uncompressed,
    Snappy,
    Gzip,
    Lz4,
    Zstd,
}
```

### 处理流水线 (Processing Pipeline)

#### 结构体

##### `DataProcessingPipeline`

数据处理流水线。

```rust
pub struct DataProcessingPipeline {
    config: PipelineConfig,
    memory_pool: Arc<ZeroCopyMemoryPool>,
    executor: Arc<HighPerformanceExecutor>,
    dbc_manager: Arc<DbcManager>,
    storage_writer: Arc<ColumnarStorageWriter>,
}
```

**方法**:

###### `new(config: PipelineConfig) -> Self`

创建新的处理流水线。

```rust
let config = PipelineConfig::default();
let pipeline = DataProcessingPipeline::new(config);
```

###### `process_files(&self) -> Result<ProcessingResult>`

处理文件。

```rust
let result = pipeline.process_files().await?;
println!("处理完成: {:?}", result);
```

##### `PipelineConfig`

流水线配置。

```rust
pub struct PipelineConfig {
    pub input_dir: PathBuf,
    pub output_dir: PathBuf,
    pub batch_size: usize,
    pub max_workers: usize,
    pub max_memory_usage: usize,
    pub enable_compression: bool,
}
```

**默认值**:
- `input_dir`: `PathBuf::from("input")`
- `output_dir`: `PathBuf::from("output")`
- `batch_size`: `100`
- `max_workers`: `num_cpus::get()`
- `max_memory_usage`: `1024 * 1024 * 1024` (1GB)
- `enable_compression`: `true`

##### `ProcessingResult`

处理结果。

```rust
pub struct ProcessingResult {
    pub files_processed: usize,
    pub frames_parsed: usize,
    pub bytes_processed: usize,
    pub processing_time_ms: u64,
    pub output_files: Vec<PathBuf>,
}
```

### 测试数据生成器 (Test Data Generator)

#### 结构体

##### `TestDataGenerator`

测试数据生成器。

```rust
pub struct TestDataGenerator {
    config: TestDataConfig,
}
```

**方法**:

###### `new(config: TestDataConfig) -> Self`

创建新的测试数据生成器。

```rust
let config = TestDataConfig::default();
let generator = TestDataGenerator::new(config);
```

###### `generate_all(&self) -> Result<()>`

生成所有测试数据。

```rust
generator.generate_all().await?;
```

###### `generate_single_file(&self, file_index: usize) -> Result<PathBuf>`

生成单个测试文件。

```rust
let file_path = generator.generate_single_file(0)?;
```

##### `TestDataConfig`

测试数据配置。

```rust
pub struct TestDataConfig {
    pub output_dir: PathBuf,
    pub file_count: usize,
    pub target_file_size: usize,
    pub frames_per_file: usize,
    pub enable_compression: bool,
}
```

**默认值**:
- `output_dir`: `PathBuf::from("test_data")`
- `file_count`: `10`
- `target_file_size`: `1024 * 1024` (1MB)
- `frames_per_file`: `1000`
- `enable_compression`: `true`

## 🔧 配置示例

### 基本配置

```rust
use canp::{
    MemoryPoolConfig,
    ExecutorConfig,
    DbcManagerConfig,
    ColumnarStorageConfig,
    PipelineConfig,
};

// 内存池配置
let memory_config = MemoryPoolConfig {
    decompress_buffer_sizes: vec![1024, 2048, 4096, 8192],
    mmap_cache_size: 1000,
    max_memory_usage: 1024 * 1024 * 1024, // 1GB
    memory_warning_threshold: 0.8,
};

// 执行器配置
let executor_config = ExecutorConfig {
    io_worker_threads: 4,
    cpu_worker_threads: 8,
    max_queue_length: 10000,
    task_timeout: Duration::from_secs(300),
    enable_work_stealing: true,
};

// DBC管理器配置
let dbc_config = DbcManagerConfig {
    max_cached_files: 100,
    cache_expire_seconds: 3600,
    auto_reload: true,
    parallel_loading: true,
    max_load_threads: 4,
};

// 列式存储配置
let storage_config = ColumnarStorageConfig {
    output_dir: PathBuf::from("output"),
    partition_strategy: PartitionStrategy::TimeBased {
        interval: Duration::from_secs(3600),
    },
    compression: CompressionType::Snappy,
    batch_size: 10000,
    max_file_size: 100 * 1024 * 1024, // 100MB
};

// 流水线配置
let pipeline_config = PipelineConfig {
    input_dir: PathBuf::from("input"),
    output_dir: PathBuf::from("output"),
    batch_size: 100,
    max_workers: 8,
    max_memory_usage: 1024 * 1024 * 1024, // 1GB
    enable_compression: true,
};
```

### 高性能配置

```rust
// 高性能内存池配置
let high_perf_memory_config = MemoryPoolConfig {
    decompress_buffer_sizes: vec![1024, 2048, 4096, 8192, 16384, 32768],
    mmap_cache_size: 2000,
    max_memory_usage: 2 * 1024 * 1024 * 1024, // 2GB
    memory_warning_threshold: 0.85,
};

// 高性能执行器配置
let high_perf_executor_config = ExecutorConfig {
    io_worker_threads: num_cpus::get() / 2,
    cpu_worker_threads: num_cpus::get(),
    max_queue_length: 50000,
    task_timeout: Duration::from_secs(600),
    enable_work_stealing: true,
};

// 高性能流水线配置
let high_perf_pipeline_config = PipelineConfig {
    input_dir: PathBuf::from("input"),
    output_dir: PathBuf::from("output"),
    batch_size: 500,
    max_workers: num_cpus::get(),
    max_memory_usage: 2 * 1024 * 1024 * 1024, // 2GB
    enable_compression: true,
};
```

### 内存受限配置

```rust
// 内存受限配置
let memory_constrained_config = MemoryPoolConfig {
    decompress_buffer_sizes: vec![512, 1024, 2048],
    mmap_cache_size: 100,
    max_memory_usage: 512 * 1024 * 1024, // 512MB
    memory_warning_threshold: 0.7,
};

// 内存受限执行器配置
let memory_constrained_executor_config = ExecutorConfig {
    io_worker_threads: 2,
    cpu_worker_threads: 4,
    max_queue_length: 1000,
    task_timeout: Duration::from_secs(300),
    enable_work_stealing: false,
};

// 内存受限流水线配置
let memory_constrained_pipeline_config = PipelineConfig {
    input_dir: PathBuf::from("input"),
    output_dir: PathBuf::from("output"),
    batch_size: 50,
    max_workers: 4,
    max_memory_usage: 512 * 1024 * 1024, // 512MB
    enable_compression: true,
};
```

## 🚀 使用示例

### 基本使用

```rust
use canp::{
    DataProcessingPipeline,
    PipelineConfig,
    MemoryPoolConfig,
    ExecutorConfig,
    DbcManagerConfig,
    ColumnarStorageConfig,
};
use std::path::PathBuf;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<()> {
    // 1. 配置各个组件
    let memory_config = MemoryPoolConfig::default();
    let executor_config = ExecutorConfig::default();
    let dbc_config = DbcManagerConfig::default();
    let storage_config = ColumnarStorageConfig::default();
    
    // 2. 配置流水线
    let pipeline_config = PipelineConfig {
        input_dir: PathBuf::from("data/input"),
        output_dir: PathBuf::from("data/output"),
        batch_size: 100,
        max_workers: 8,
        ..Default::default()
    };
    
    // 3. 创建处理流水线
    let pipeline = DataProcessingPipeline::new(pipeline_config);
    
    // 4. 处理文件
    let result = pipeline.process_files().await?;
    
    println!("处理完成:");
    println!("  文件数: {}", result.files_processed);
    println!("  帧数: {}", result.frames_parsed);
    println!("  字节数: {}", result.bytes_processed);
    println!("  处理时间: {}ms", result.processing_time_ms);
    
    Ok(())
}
```

### 高级使用

```rust
use canp::{
    ZeroCopyMemoryPool,
    HighPerformanceExecutor,
    DbcManager,
    DataLayerParser,
    ColumnarStorageWriter,
    CanFrame,
    TaskType,
    Priority,
};
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<()> {
    // 1. 创建核心组件
    let memory_pool = Arc::new(ZeroCopyMemoryPool::default());
    let executor = Arc::new(HighPerformanceExecutor::default());
    let dbc_manager = Arc::new(DbcManager::default());
    let storage_writer = Arc::new(ColumnarStorageWriter::default());
    
    // 2. 加载DBC文件
    dbc_manager.load_dbc_file("vehicle.dbc", Some(0)).await?;
    
    // 3. 创建数据解析器
    let mut parser = DataLayerParser::new(Arc::clone(&memory_pool));
    
    // 4. 处理文件
    let file_data = std::fs::read("data.bin")?;
    let parsed_data = parser.parse_file(&file_data).await?;
    
    // 5. 解析CAN帧
    for frame_sequence in &parsed_data.frame_sequences {
        for frame in &frame_sequence.frames {
            if let Some(parsed_message) = dbc_manager.parse_can_frame(frame)? {
                // 处理解析后的消息
                println!("解析到消息: {}", parsed_message.name);
                
                // 提交存储任务
                let storage_writer = Arc::clone(&storage_writer);
                let message = parsed_message.clone();
                
                executor.submit_cpu_task(Priority::Normal, move || {
                    // 存储消息到列式存储
                    Ok(())
                })?;
            }
        }
    }
    
    // 6. 等待所有任务完成
    executor.shutdown()?;
    
    Ok(())
}
```

### 测试数据生成

```rust
use canp::{TestDataGenerator, TestDataConfig};
use std::path::PathBuf;

#[tokio::main]
async fn main() -> Result<()> {
    // 1. 配置测试数据生成器
    let config = TestDataConfig {
        output_dir: PathBuf::from("test_data"),
        file_count: 10,
        target_file_size: 1024 * 1024, // 1MB
        frames_per_file: 1000,
        enable_compression: true,
    };
    
    // 2. 创建生成器
    let generator = TestDataGenerator::new(config);
    
    // 3. 生成测试数据
    generator.generate_all().await?;
    
    println!("测试数据生成完成");
    
    Ok(())
}
```

## 🔍 错误处理

### 错误类型

CANP使用`anyhow::Result<T>`作为统一的错误类型，所有公共API都返回这个类型。

### 常见错误处理

```rust
use anyhow::{Result, Context};

// 处理文件读取错误
let file_data = std::fs::read("data.bin")
    .context("无法读取数据文件")?;

// 处理解析错误
let parsed_data = parser.parse_file(&file_data)
    .await
    .context("解析文件失败")?;

// 处理DBC解析错误
let parsed_message = dbc_manager.parse_can_frame(&frame)
    .context("解析CAN帧失败")?;

// 处理存储错误
storage_writer.write_batch(record_batch)
    .context("写入数据失败")?;
```

### 自定义错误处理

```rust
use anyhow::{anyhow, Result};

fn validate_config(config: &PipelineConfig) -> Result<()> {
    if config.batch_size == 0 {
        return Err(anyhow!("批处理大小不能为0"));
    }
    
    if config.max_workers == 0 {
        return Err(anyhow!("工作线程数不能为0"));
    }
    
    if !config.input_dir.exists() {
        return Err(anyhow!("输入目录不存在: {:?}", config.input_dir));
    }
    
    Ok(())
}
```

## 📊 性能监控

### 统计信息收集

```rust
// 获取内存池统计
let memory_stats = memory_pool.get_stats();
println!("内存使用: {:.2}MB", memory_stats.total_memory_usage_mb);
println!("缓存命中率: {:.2}%", 
    memory_stats.mmap_cache_hits as f64 / 
    (memory_stats.mmap_cache_hits + memory_stats.mmap_cache_misses) as f64 * 100.0);

// 获取执行器统计
let executor_stats = executor.get_stats();
println!("完成任务: {}/{}", executor_stats.completed_tasks, executor_stats.total_tasks);
println!("平均执行时间: {:.2}ms", executor_stats.avg_execution_time_ms);

// 获取DBC解析统计
let dbc_stats = dbc_manager.get_stats();
println!("解析帧数: {}", dbc_stats.parsed_frames);
println!("未知消息: {}", dbc_stats.unknown_messages);

// 获取解析统计
let parsing_stats = parser.get_stats();
println!("解析文件数: {}", parsing_stats.files_parsed);
println!("解析帧序列数: {}", parsing_stats.frame_sequences_parsed);
```

### 性能基准测试

```rust
use std::time::Instant;

// 性能基准测试
let start = Instant::now();

// 执行处理任务
let result = pipeline.process_files().await?;

let duration = start.elapsed();
println!("处理时间: {:?}", duration);
println!("吞吐量: {:.2} MB/s", 
    result.bytes_processed as f64 / duration.as_secs_f64() / 1024.0 / 1024.0);
```

---

这个API参考文档提供了CANP库的完整接口说明。通过遵循这些API设计，开发者可以构建高性能的CAN总线数据处理应用。 