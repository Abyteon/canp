# CANP 技术架构文档

## 🎯 设计理念

CANP (CAN Processing) 是一个专为大规模CAN总线数据处理设计的高性能系统。我们的设计理念基于以下几个核心原则：

### 1. 零拷贝优先 (Zero-Copy First)
- **内存映射**: 使用`memmap2`直接映射文件到内存
- **缓冲区复用**: 基于`bytes`库的零拷贝缓冲区管理
- **指针传递**: 避免不必要的数据拷贝

### 2. 分层处理 (Layered Processing)
- **4层数据结构**: 文件头部 → 解压头部 → 帧序列 → 单帧数据
- **批量处理**: 每层都支持批量操作以提高效率
- **流水线化**: 各层可以并行处理

### 3. 混合并发 (Hybrid Concurrency)
- **IO密集型**: 使用Tokio异步运行时
- **CPU密集型**: 使用Rayon并行计算
- **智能调度**: 根据任务类型自动选择最优执行器

### 4. 列式存储 (Columnar Storage)
- **Apache Arrow**: 内存中的列式数据格式
- **Parquet**: 磁盘上的压缩列式存储
- **分区策略**: 支持按时间和ID分区

## 🏗️ 系统架构

### 整体架构图

```
┌─────────────────────────────────────────────────────────────────┐
│                        应用层 (Application Layer)                │
├─────────────────────────────────────────────────────────────────┤
│                    处理流水线 (Processing Pipeline)              │
├─────────────────────────────────────────────────────────────────┤
│  数据层解析器  │  DBC解析器  │  列式存储  │  测试数据生成器        │
├─────────────────────────────────────────────────────────────────┤
│                    并发调度层 (Concurrency Layer)                │
│              Tokio (IO)  │  Rayon (CPU)  │  优先级队列           │
├─────────────────────────────────────────────────────────────────┤
│                    内存管理层 (Memory Layer)                     │
│              零拷贝内存池  │  文件映射缓存  │  对象池              │
├─────────────────────────────────────────────────────────────────┤
│                        系统层 (System Layer)                    │
│              文件系统  │  网络  │  内存  │  多核CPU              │
└─────────────────────────────────────────────────────────────────┘
```

### 数据流架构

```
输入文件 → 内存映射 → 解压缩 → 数据解析 → DBC解析 → 列式存储 → 输出文件
    │         │         │         │         │         │         │
    ▼         ▼         ▼         ▼         ▼         ▼         ▼
  文件池    映射缓存   解压池    解析池    DBC缓存    存储池    分区文件
```

## 📦 核心组件详解

### 1. 零拷贝内存池 (Zero-Copy Memory Pool)

#### 设计目标
- 最小化内存分配开销
- 最大化内存复用率
- 提供零拷贝数据访问
- 支持大规模并发访问

#### 技术实现

```rust
pub struct ZeroCopyMemoryPool {
    // 分层解压缓冲区池 - 使用lock_pool库
    decompress_pools: Vec<Arc<LockPool<BytesMut, 64, 512>>>,
    
    // 文件映射缓存 - 使用LRU缓存
    mmap_cache: Arc<RwLock<lru::LruCache<String, Arc<Mmap>>>>,
    
    // 内存使用统计
    current_memory_usage: Arc<RwLock<usize>>,
}
```

#### 关键特性

1. **分层管理**
   - 根据数据大小选择最优池
   - 减少内存碎片
   - 提高分配效率

2. **对象池模式**
   - 复用已分配的对象
   - 减少GC压力
   - 提高性能

3. **LRU缓存**
   - 智能缓存管理
   - 自动淘汰策略
   - 内存使用控制

4. **零拷贝访问**
   - 直接内存映射
   - 无数据拷贝
   - 高性能访问

#### 性能优化

```rust
// 批量分配优化
pub async fn get_decompress_buffers_batch(
    &self,
    sizes: &[usize],
) -> Vec<MutableMemoryBuffer> {
    sizes.iter().map(|&size| {
        self.get_decompress_buffer(size).now_or_never()
            .unwrap_or_else(|| self.create_new_buffer(size))
    }).collect()
}

// 智能池选择
fn select_decompress_pool(&self, size: usize) -> Option<&Arc<LockPool<BytesMut, 64, 512>>> {
    self.decompress_pools.iter()
        .find(|pool| pool.capacity() >= size)
}
```

### 2. 高性能执行器 (High-Performance Executor)

#### 设计目标
- 支持混合任务类型
- 提供智能调度
- 实现负载均衡
- 防止系统过载

#### 技术实现

```rust
pub struct HighPerformanceExecutor {
    // IO任务队列 - Tokio异步
    io_task_tx: mpsc::UnboundedSender<(TaskMetadata, BoxedTask)>,
    
    // CPU任务队列 - Rayon并行
    cpu_task_tx: mpsc::UnboundedSender<(TaskMetadata, BoxedCpuTask)>,
    
    // 高优先级任务队列
    priority_task_tx: mpsc::UnboundedSender<(TaskMetadata, BoxedTask)>,
    
    // 背压控制
    backpressure_semaphore: Arc<Semaphore>,
}
```

#### 任务类型分类

```rust
pub enum TaskType {
    IoIntensive,    // 文件IO、网络IO
    CpuIntensive,   // 数据解析、压缩解压
    Mixed,          // 混合型任务
    HighPriority,   // 错误处理、监控
    Custom(u32),    // 自定义任务
}
```

#### 调度策略

1. **IO密集型任务**
   - 使用Tokio异步运行时
   - 支持大量并发IO操作
   - 非阻塞执行

2. **CPU密集型任务**
   - 使用Rayon并行计算
   - 工作窃取调度
   - 充分利用多核

3. **高优先级任务**
   - 独立优先级队列
   - 抢占式调度
   - 快速响应

#### 背压控制

```rust
// 信号量控制并发数
let semaphore = Arc::new(Semaphore::new(config.max_concurrent_tasks));

// 任务提交前获取许可
let _permit = semaphore.acquire().await?;
```

### 3. DBC解析器 (DBC Parser)

#### 设计目标
- 完全兼容CAN-DBC标准
- 支持大规模DBC文件
- 提供高性能解析
- 实现智能缓存

#### 技术实现

```rust
pub struct DbcManager {
    // DBC文件缓存
    dbc_cache: Arc<RwLock<HashMap<PathBuf, DbcCacheEntry>>>,
    
    // 解析统计
    stats: Arc<RwLock<DbcParsingStats>>,
}
```

#### 缓存策略

1. **文件级缓存**
   - 缓存已加载的DBC文件
   - LRU淘汰策略
   - 自动过期机制

2. **消息级缓存**
   - 缓存消息定义
   - 快速查找
   - 内存优化

#### 位提取算法

```rust
// 小端序位提取
fn extract_little_endian_bits(&self, data: &[u8], start_bit: usize, length: usize) -> Result<u64> {
    let mut result = 0u64;
    let mut bit_pos = 0;
    
    for byte_idx in start_byte..=end_byte {
        let byte = data[byte_idx];
        let mut bits_in_this_byte = 8;
        let mut start_bit_in_byte = 0;
        
        // 处理边界情况
        if byte_idx == start_byte && start_bit % 8 != 0 {
            start_bit_in_byte = start_bit % 8;
            bits_in_this_byte = 8 - start_bit_in_byte;
        }
        
        // 提取位值
        let mask = ((1u8 << bits_in_this_byte) - 1) << start_bit_in_byte;
        let value = (byte & mask) >> start_bit_in_byte;
        
        // 防止溢出
        if bit_pos < 64 {
            result |= (value as u64) << bit_pos;
        }
        bit_pos += bits_in_this_byte;
    }
    
    Ok(result)
}
```

### 4. 数据层解析器 (Data Layer Parser)

#### 设计目标
- 支持4层嵌套数据结构
- 实现零拷贝解析
- 提供批量处理
- 保证数据完整性

#### 数据结构

```
文件格式:
┌─────────────┬─────────────┬─────────────┬─────────────┐
│  文件头部   │  解压头部   │  帧序列1    │  帧序列2    │
│  (35字节)   │  (20字节)   │  (16字节)   │  (16字节)   │
└─────────────┴─────────────┴─────────────┴─────────────┘
     │              │              │              │
     ▼              ▼              ▼              ▼
  压缩数据长度    解压数据长度    帧序列长度      帧序列长度
```

#### 解析流程

```rust
pub async fn parse_file(&mut self, file_data: &[u8]) -> Result<ParsedFileData> {
    // 1. 解析文件头部
    let file_header = FileHeader::from_bytes(&file_data[..35])?;
    
    // 2. 解压缩数据
    let compressed_data = &file_data[35..35+file_header.compressed_length as usize];
    let decompressed_data = self.decompress_data(compressed_data)?;
    
    // 3. 解析解压头部
    let decompressed_header = DecompressedHeader::from_bytes(&decompressed_data[..20])?;
    
    // 4. 解析帧序列
    let frame_data = &decompressed_data[20..];
    let frame_sequences = self.parse_frame_sequences(frame_data)?;
    
    Ok(ParsedFileData {
        file_header,
        decompressed_header,
        frame_sequences,
    })
}
```

### 5. 列式存储 (Columnar Storage)

#### 设计目标
- 高性能数据存储
- 支持复杂查询
- 压缩优化
- 分区管理

#### 技术实现

```rust
pub struct ColumnarStorageWriter {
    // 分区策略
    partition_strategy: PartitionStrategy,
    
    // 压缩配置
    compression: CompressionType,
    
    // Arrow记录批次
    record_batches: Vec<RecordBatch>,
}
```

#### 存储格式

1. **Apache Arrow**
   - 内存中的列式格式
   - 零拷贝序列化
   - 高性能查询

2. **Parquet**
   - 磁盘上的压缩格式
   - 列式压缩
   - 元数据管理

#### 分区策略

```rust
pub enum PartitionStrategy {
    TimeBased { interval: Duration },
    IdBased { bucket_count: usize },
    Custom { partition_fn: Box<dyn Fn(&RecordBatch) -> String> },
}
```

## 🔄 处理流水线

### 完整处理流程

```
1. 文件发现 → 2. 内存映射 → 3. 批量解压 → 4. 数据解析 → 5. DBC解析 → 6. 列式存储
     │              │              │              │              │              │
     ▼              ▼              ▼              ▼              ▼              ▼
  并发扫描        零拷贝映射      并行解压        批量解析        信号提取        分区写入
```

### 并发控制

```rust
pub struct ProcessingPipeline {
    // 配置参数
    config: PipelineConfig,
    
    // 核心组件
    memory_pool: Arc<ZeroCopyMemoryPool>,
    executor: Arc<HighPerformanceExecutor>,
    dbc_manager: Arc<DbcManager>,
    storage_writer: Arc<ColumnarStorageWriter>,
}
```

### 批处理策略

1. **文件级批处理**
   - 批量文件映射
   - 并发文件处理
   - 负载均衡

2. **数据级批处理**
   - 批量解压缩
   - 批量数据解析
   - 批量DBC解析

3. **存储级批处理**
   - 批量记录写入
   - 批量分区管理
   - 批量元数据更新

## 📊 性能优化

### 内存优化

1. **零拷贝策略**
   - 内存映射文件
   - 缓冲区复用
   - 指针传递

2. **对象池模式**
   - 减少分配开销
   - 提高缓存命中率
   - 降低GC压力

3. **智能缓存**
   - LRU淘汰策略
   - 自动过期机制
   - 内存使用控制

### CPU优化

1. **并行计算**
   - 多核并行处理
   - 工作窃取调度
   - 负载均衡

2. **批量处理**
   - 减少函数调用开销
   - 提高缓存效率
   - 优化内存访问模式

3. **算法优化**
   - 高效的位提取算法
   - 优化的解析流程
   - 智能的数据结构

### IO优化

1. **异步IO**
   - 非阻塞文件操作
   - 并发IO处理
   - 事件驱动模型

2. **批量IO**
   - 批量文件读取
   - 批量数据写入
   - 减少系统调用

3. **缓存优化**
   - 文件映射缓存
   - 数据块缓存
   - 元数据缓存

## 🧪 测试策略

### 测试层次

1. **单元测试**
   - 模块功能测试
   - 边界条件测试
   - 错误处理测试

2. **集成测试**
   - 端到端流程测试
   - 组件交互测试
   - 性能回归测试

3. **属性测试**
   - 数据一致性测试
   - 不变性验证
   - 随机性测试

4. **性能测试**
   - 基准测试
   - 压力测试
   - 内存泄漏测试

### 测试工具

- **criterion**: 性能基准测试
- **proptest**: 属性测试
- **tokio-test**: 异步测试
- **tempfile**: 临时文件管理

## 🔧 配置管理

### 配置层次

1. **系统级配置**
   - 内存限制
   - 线程数量
   - 缓存大小

2. **应用级配置**
   - 批处理大小
   - 并发数量
   - 超时设置

3. **组件级配置**
   - 内存池配置
   - 执行器配置
   - 存储配置

### 配置验证

```rust
impl PipelineConfig {
    pub fn validate(&self) -> Result<()> {
        // 验证内存限制
        if self.max_memory_usage == 0 {
            return Err(anyhow!("内存使用限制不能为0"));
        }
        
        // 验证线程数量
        if self.max_workers == 0 {
            return Err(anyhow!("工作线程数不能为0"));
        }
        
        // 验证批处理大小
        if self.batch_size == 0 {
            return Err(anyhow!("批处理大小不能为0"));
        }
        
        Ok(())
    }
}
```

## 📈 监控和统计

### 性能指标

1. **吞吐量指标**
   - 文件处理速度
   - 数据解析速度
   - 存储写入速度

2. **延迟指标**
   - 处理延迟
   - 响应时间
   - 队列延迟

3. **资源指标**
   - CPU使用率
   - 内存使用率
   - 磁盘IO

### 统计收集

```rust
pub struct ProcessingStats {
    // 处理统计
    files_processed: AtomicUsize,
    frames_parsed: AtomicUsize,
    bytes_processed: AtomicUsize,
    
    // 性能统计
    processing_time: AtomicU64,
    memory_usage: AtomicUsize,
    cpu_usage: AtomicU64,
}
```

### 监控集成

- **实时监控**: 系统资源使用情况
- **性能分析**: 瓶颈识别和优化
- **告警机制**: 异常情况通知
- **日志记录**: 详细的操作日志

## 🚀 部署和运维

### 系统要求

- **硬件要求**
  - CPU: 8核以上
  - 内存: 16GB以上
  - 存储: SSD/NVMe

- **软件要求**
  - Rust 1.70+
  - Linux/macOS/Windows
  - 足够的文件描述符限制

### 部署配置

1. **开发环境**
   - 调试模式编译
   - 详细日志输出
   - 性能分析工具

2. **生产环境**
   - 发布模式编译
   - 优化配置参数
   - 监控和告警

3. **测试环境**
   - 模拟生产负载
   - 压力测试
   - 性能基准测试

### 运维最佳实践

1. **资源监控**
   - 实时监控系统资源
   - 设置合理的告警阈值
   - 定期性能分析

2. **日志管理**
   - 结构化日志输出
   - 日志轮转和压缩
   - 日志分析和告警

3. **故障处理**
   - 自动错误恢复
   - 降级策略
   - 故障转移机制

---

这个技术架构文档描述了CANP系统的完整设计理念、技术实现和最佳实践。通过遵循这些设计原则，CANP能够提供高性能、高可靠性的CAN总线数据处理能力。 