# 错误处理和日志记录学习指南

## 📚 概述

错误处理和日志记录是构建可靠系统的重要组成部分。CANP项目使用现代化的Rust错误处理库和日志系统，本文档详细介绍相关概念、使用方法和最佳实践。

## 🏗️ 错误处理

### 1. 错误类型设计

#### 使用 thiserror 定义错误类型

```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ProcessingError {
    #[error("IO错误: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("解析错误: {message}")]
    Parse { message: String, line: usize },
    
    #[error("内存不足: 需要 {needed} 字节, 可用 {available} 字节")]
    InsufficientMemory { needed: usize, available: usize },
    
    #[error("无效的数据格式: {0}")]
    InvalidFormat(String),
    
    #[error("超时错误: 操作在 {timeout:?} 后超时")]
    Timeout { timeout: std::time::Duration },
    
    #[error("配置错误: {0}")]
    Config(String),
}

// 为错误类型实现额外的方法
impl ProcessingError {
    pub fn is_retryable(&self) -> bool {
        matches!(self, 
            ProcessingError::Io(_) | 
            ProcessingError::Timeout { .. }
        )
    }
    
    pub fn get_error_code(&self) -> u32 {
        match self {
            ProcessingError::Io(_) => 1001,
            ProcessingError::Parse { .. } => 1002,
            ProcessingError::InsufficientMemory { .. } => 1003,
            ProcessingError::InvalidFormat(_) => 1004,
            ProcessingError::Timeout { .. } => 1005,
            ProcessingError::Config(_) => 1006,
        }
    }
}
```

#### 在CANP中的应用

```rust
// DBC解析错误
#[derive(Error, Debug)]
pub enum DbcError {
    #[error("DBC文件读取失败: {0}")]
    FileRead(#[from] std::io::Error),
    
    #[error("DBC语法错误: {message} at line {line}")]
    SyntaxError { message: String, line: usize },
    
    #[error("重复的消息ID: {id}")]
    DuplicateMessageId { id: u32 },
    
    #[error("无效的信号定义: {signal_name}")]
    InvalidSignal { signal_name: String },
}

// 数据解析错误
#[derive(Error, Debug)]
pub enum DataParseError {
    #[error("数据长度不足: 需要 {needed}, 实际 {actual}")]
    InsufficientData { needed: usize, actual: usize },
    
    #[error("校验和错误: 期望 {expected}, 实际 {actual}")]
    ChecksumMismatch { expected: u32, actual: u32 },
    
    #[error("无效的帧格式: {0}")]
    InvalidFrameFormat(String),
}
```

### 2. 使用 anyhow 进行错误传播

#### 基本用法

```rust
use anyhow::{Context, Result, anyhow};

// 简单的错误传播
fn read_config_file(path: &str) -> Result<String> {
    std::fs::read_to_string(path)
        .context("读取配置文件失败")
}

// 添加上下文信息
fn parse_data(data: &[u8]) -> Result<Vec<u8>> {
    if data.is_empty() {
        return Err(anyhow!("数据不能为空"));
    }
    
    // 处理逻辑
    Ok(data.to_vec())
}

// 链式错误处理
fn process_file(file_path: &str) -> Result<Vec<u8>> {
    let content = read_config_file(file_path)
        .context("无法读取文件")?;
    
    let data = content.as_bytes();
    parse_data(data)
        .context("解析文件内容失败")
}
```

#### 在CANP中的应用

```rust
// 文件处理流水线
pub async fn process_file_pipeline(
    file_path: &Path,
    memory_pool: &ZeroCopyMemoryPool,
) -> Result<ProcessedData> {
    // 1. 内存映射文件
    let mmap = memory_pool.get_mmap(file_path)
        .context("内存映射文件失败")?;
    
    // 2. 解析文件头部
    let header = FileHeader::from_bytes(&mmap[..35])
        .context("解析文件头部失败")?;
    
    // 3. 解压缩数据
    let compressed_data = &mmap[35..35+header.compressed_length as usize];
    let decompressed_data = decompress_data(compressed_data)
        .context("解压缩数据失败")?;
    
    // 4. 解析数据
    let parsed_data = parse_decompressed_data(&decompressed_data)
        .context("解析解压数据失败")?;
    
    Ok(parsed_data)
}
```

### 3. 错误恢复策略

#### 重试机制

```rust
use std::time::Duration;
use tokio::time::sleep;

async fn retry_with_backoff<F, T, E>(
    mut operation: F,
    max_retries: usize,
    initial_delay: Duration,
) -> Result<T, E>
where
    F: FnMut() -> Result<T, E>,
    E: std::fmt::Debug,
{
    let mut delay = initial_delay;
    
    for attempt in 0..=max_retries {
        match operation() {
            Ok(result) => return Ok(result),
            Err(e) if attempt == max_retries => return Err(e),
            Err(e) => {
                tracing::warn!("操作失败 (尝试 {}/{}): {:?}", attempt + 1, max_retries + 1, e);
                sleep(delay).await;
                delay *= 2; // 指数退避
            }
        }
    }
    
    unreachable!()
}

// 使用示例
async fn fetch_data_with_retry() -> Result<Vec<u8>> {
    retry_with_backoff(
        || fetch_data_from_network(),
        3,
        Duration::from_millis(100),
    ).await
}
```

#### 降级策略

```rust
// 降级处理器
pub struct FallbackProcessor {
    primary_processor: Box<dyn DataProcessor>,
    fallback_processor: Box<dyn DataProcessor>,
}

impl FallbackProcessor {
    pub async fn process(&self, data: &[u8]) -> Result<Vec<u8>> {
        // 尝试主处理器
        match self.primary_processor.process(data).await {
            Ok(result) => Ok(result),
            Err(e) => {
                tracing::warn!("主处理器失败，使用降级处理器: {:?}", e);
                self.fallback_processor.process(data).await
            }
        }
    }
}
```

## 📝 日志记录

### 1. 使用 tracing 进行结构化日志

#### 基本设置

```rust
use tracing::{info, warn, error, debug, trace};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

// 初始化日志系统
fn init_logging() {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "canp=info,tokio=warn".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();
}

// 在 main 函数中调用
fn main() {
    init_logging();
    
    info!("CANP 系统启动");
    // ... 其他代码
}
```

#### 结构化日志

```rust
use tracing::{info, warn, error, instrument};

// 使用字段记录结构化信息
#[instrument(skip(data), fields(data_size = data.len()))]
async fn process_data(data: &[u8]) -> Result<Vec<u8>> {
    info!("开始处理数据");
    
    if data.is_empty() {
        warn!("收到空数据");
        return Ok(vec![]);
    }
    
    let result = perform_processing(data)
        .await
        .map_err(|e| {
            error!(error = %e, "数据处理失败");
            e
        })?;
    
    info!(result_size = result.len(), "数据处理完成");
    Ok(result)
}

// 使用 span 进行上下文跟踪
#[instrument(skip(file_path), fields(file = %file_path.display()))]
async fn process_file(file_path: &Path) -> Result<()> {
    let _span = tracing::info_span!("file_processing", 
        file_size = file_path.metadata()?.len()
    ).entered();
    
    info!("开始处理文件");
    
    // 处理逻辑
    let result = perform_file_processing(file_path).await?;
    
    info!(processed_bytes = result.len(), "文件处理完成");
    Ok(())
}
```

### 2. 日志级别和过滤

#### 日志级别

```rust
// 不同级别的日志
trace!("详细的调试信息: {:?}", internal_state);
debug!("调试信息: 处理了 {} 个字节", bytes_processed);
info!("一般信息: 文件 {} 处理完成", file_name);
warn!("警告: 内存使用率达到 {}%", memory_usage);
error!("错误: 无法打开文件: {}", error);
```

#### 环境变量配置

```bash
# 设置日志级别
export RUST_LOG=canp=debug,tokio=warn

# 更详细的配置
export RUST_LOG=canp::memory_pool=trace,canp::parser=debug,tokio=warn
```

### 3. 性能监控日志

```rust
use tracing::{info, instrument};
use std::time::Instant;

// 性能监控
#[instrument(skip(data))]
async fn process_with_metrics(data: &[u8]) -> Result<Vec<u8>> {
    let start = Instant::now();
    
    let result = perform_processing(data).await?;
    
    let duration = start.elapsed();
    info!(
        input_size = data.len(),
        output_size = result.len(),
        processing_time_ms = duration.as_millis(),
        throughput_mbps = (data.len() as f64 / duration.as_secs_f64()) / 1_000_000.0,
        "数据处理完成"
    );
    
    Ok(result)
}

// 内存使用监控
pub struct MemoryMonitor {
    current_usage: Arc<RwLock<usize>>,
}

impl MemoryMonitor {
    pub fn log_memory_usage(&self) {
        let usage = self.current_usage.read().unwrap();
        info!(
            memory_usage_mb = *usage / 1024 / 1024,
            "当前内存使用情况"
        );
    }
}
```

## 🔧 错误处理最佳实践

### 1. 错误类型设计原则

```rust
// 1. 使用具体的错误类型
#[derive(Error, Debug)]
pub enum SpecificError {
    #[error("网络连接失败: {reason}")]
    NetworkError { reason: String },
    
    #[error("数据验证失败: 字段 {field} 无效")]
    ValidationError { field: String, value: String },
}

// 2. 提供有用的错误信息
impl SpecificError {
    pub fn network_error(reason: impl Into<String>) -> Self {
        Self::NetworkError { reason: reason.into() }
    }
    
    pub fn validation_error(field: impl Into<String>, value: impl Into<String>) -> Self {
        Self::ValidationError { 
            field: field.into(), 
            value: value.into() 
        }
    }
}
```

### 2. 错误传播模式

```rust
// 使用 ? 操作符进行简洁的错误传播
fn process_data_chain(data: &[u8]) -> Result<ProcessedData> {
    let parsed = parse_data(data)?;
    let validated = validate_data(&parsed)?;
    let transformed = transform_data(validated)?;
    Ok(transformed)
}

// 使用 map_err 转换错误类型
fn process_with_error_mapping(data: &[u8]) -> Result<Vec<u8>> {
    std::fs::read(data)
        .map_err(|e| ProcessingError::Io(e))
}
```

### 3. 错误恢复和降级

```rust
// 优雅的错误恢复
async fn robust_data_processing(data: &[u8]) -> Result<Vec<u8>> {
    // 尝试主处理路径
    match primary_processing(data).await {
        Ok(result) => Ok(result),
        Err(e) => {
            // 记录错误
            error!(error = %e, "主处理路径失败");
            
            // 尝试降级处理
            match fallback_processing(data).await {
                Ok(result) => {
                    warn!("使用降级处理成功");
                    Ok(result)
                }
                Err(fallback_error) => {
                    error!(error = %fallback_error, "降级处理也失败");
                    Err(e) // 返回原始错误
                }
            }
        }
    }
}
```

## 📊 日志最佳实践

### 1. 结构化日志设计

```rust
// 使用一致的字段名
#[instrument(skip(data), fields(
    data_size = data.len(),
    processing_type = "can_data",
    timestamp = %chrono::Utc::now()
))]
async fn process_can_data(data: &[u8]) -> Result<Vec<u8>> {
    info!("开始处理CAN数据");
    
    let result = perform_can_processing(data).await?;
    
    info!(
        input_frames = data.len() / 8, // 假设每帧8字节
        output_frames = result.len() / 8,
        processing_success = true,
        "CAN数据处理完成"
    );
    
    Ok(result)
}
```

### 2. 性能敏感日志

```rust
// 使用条件日志避免性能影响
use tracing::enabled;

fn process_high_volume_data(data: &[u8]) -> Result<Vec<u8>> {
    // 只在启用 trace 级别时记录详细信息
    if enabled!(tracing::Level::TRACE) {
        trace!("处理高容量数据: {:?}", data);
    }
    
    let result = perform_processing(data)?;
    
    // 只在启用 debug 级别时记录统计信息
    if enabled!(tracing::Level::DEBUG) {
        debug!("处理完成: {} -> {} 字节", data.len(), result.len());
    }
    
    Ok(result)
}
```

### 3. 错误日志完整性

```rust
// 记录完整的错误上下文
async fn process_with_full_context(data: &[u8]) -> Result<Vec<u8>> {
    let context = ProcessingContext {
        data_size: data.len(),
        timestamp: chrono::Utc::now(),
        source: "file_processor".to_string(),
    };
    
    match perform_processing(data).await {
        Ok(result) => {
            info!(
                context = ?context,
                result_size = result.len(),
                "处理成功"
            );
            Ok(result)
        }
        Err(e) => {
            error!(
                context = ?context,
                error = %e,
                error_type = std::any::type_name::<std::io::Error>(),
                "处理失败"
            );
            Err(e)
        }
    }
}
```

## 🚀 监控和告警

### 1. 指标收集

```rust
use metrics::{counter, gauge, histogram};

// 收集性能指标
pub struct MetricsCollector;

impl MetricsCollector {
    pub fn record_processing_time(duration: Duration) {
        histogram!("processing_time_seconds", duration.as_secs_f64());
    }
    
    pub fn increment_processed_files() {
        counter!("files_processed_total", 1);
    }
    
    pub fn set_memory_usage(bytes: usize) {
        gauge!("memory_usage_bytes", bytes as f64);
    }
    
    pub fn record_error(error_type: &str) {
        counter!("errors_total", 1, "type" => error_type.to_string());
    }
}
```

### 2. 健康检查

```rust
use std::sync::atomic::{AtomicBool, Ordering};

pub struct HealthChecker {
    is_healthy: AtomicBool,
    last_check: Arc<RwLock<Instant>>,
}

impl HealthChecker {
    pub fn check_health(&self) -> bool {
        let healthy = self.is_healthy.load(Ordering::Relaxed);
        
        if !healthy {
            error!("系统健康检查失败");
        } else {
            debug!("系统健康检查通过");
        }
        
        healthy
    }
    
    pub fn mark_unhealthy(&self) {
        self.is_healthy.store(false, Ordering::Relaxed);
        error!("系统标记为不健康");
    }
}
```

## 📚 总结

错误处理和日志记录是构建可靠系统的关键组件。通过使用现代化的Rust库和最佳实践，我们可以：

- 设计清晰、可恢复的错误类型
- 提供有用的错误信息和上下文
- 实现结构化的日志记录
- 监控系统性能和健康状况
- 快速定位和解决问题

关键要点：
- 使用 `thiserror` 定义具体的错误类型
- 使用 `anyhow` 进行简洁的错误传播
- 使用 `tracing` 进行结构化日志记录
- 实现适当的错误恢复和降级策略
- 收集有用的监控指标 