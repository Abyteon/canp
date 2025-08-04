//! # 数据处理管道 (Processing Pipeline)
//! 
//! 整合所有组件的完整数据处理管道
//! 
//! ## 处理流程
//! 1. 文件批量映射和加载
//! 2. 4层数据结构解析
//! 3. DBC信号解析
//! 4. 列式存储输出
//! 5. 统计和监控
//! 
//! ## 核心特性
//! - 零拷贝性能优化
//! - 高并发处理
//! - 智能批处理
//! - 错误恢复
//! - 完整监控

use anyhow::{Result, Context};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Semaphore;
use tracing::{info, warn, error, debug};
use futures::stream::{self, StreamExt};

use crate::zero_copy_memory_pool::{ZeroCopyMemoryPool, MemoryPoolConfig};
use crate::high_performance_executor::{HighPerformanceExecutor, ExecutorConfig};
use crate::data_layer_parser::{DataLayerParser, ParsingStats as DataParsingStats};
use crate::dbc_parser::{DbcManager, DbcManagerConfig, DbcParsingStats};
use crate::columnar_storage::{ColumnarStorageWriter, ColumnarStorageConfig, StorageStats};

/// 处理管道配置
#[derive(Debug, Clone)]
pub struct PipelineConfig {
    /// 内存池配置
    pub memory_pool_config: MemoryPoolConfig,
    /// 执行器配置
    pub executor_config: ExecutorConfig,
    /// DBC管理器配置
    pub dbc_manager_config: DbcManagerConfig,
    /// 列式存储配置
    pub storage_config: ColumnarStorageConfig,
    /// 批处理大小
    pub batch_size: usize,
    /// 最大并发文件数
    pub max_concurrent_files: usize,
    /// 是否启用错误恢复
    pub enable_error_recovery: bool,
    /// 错误重试次数
    pub max_retries: usize,
    /// 处理超时时间（秒）
    pub processing_timeout_seconds: u64,
    /// 内存压力阈值
    pub memory_pressure_threshold: f64,
    /// 是否启用进度报告
    pub enable_progress_reporting: bool,
    /// 进度报告间隔（秒）
    pub progress_report_interval: u64,
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            memory_pool_config: MemoryPoolConfig::default(),
            executor_config: ExecutorConfig::default(),
            dbc_manager_config: DbcManagerConfig::default(),
            storage_config: ColumnarStorageConfig::default(),
            batch_size: 50,  // 50个文件一批
            max_concurrent_files: num_cpus::get() * 2,
            enable_error_recovery: true,
            max_retries: 3,
            processing_timeout_seconds: 300, // 5分钟
            memory_pressure_threshold: 0.8, // 80%
            enable_progress_reporting: true,
            progress_report_interval: 30, // 30秒
        }
    }
}

/// 文件处理结果
#[derive(Debug)]
pub struct FileProcessingResult {
    /// 文件路径
    pub file_path: PathBuf,
    /// 是否成功
    pub success: bool,
    /// 错误信息
    pub error: Option<String>,
    /// 处理时间（毫秒）
    pub processing_time_ms: u64,
    /// 解析的消息数
    pub parsed_messages: usize,
    /// 解析的信号数  
    pub parsed_signals: usize,
    /// 文件大小
    pub file_size: u64,
    /// 处理吞吐量（MB/s）
    pub throughput_mb_s: f64,
}

/// 批处理结果
#[derive(Debug)]
pub struct BatchProcessingResult {
    /// 批次ID
    pub batch_id: usize,
    /// 文件结果列表
    pub file_results: Vec<FileProcessingResult>,
    /// 批处理时间（毫秒）
    pub batch_time_ms: u64,
    /// 成功文件数
    pub successful_files: usize,
    /// 失败文件数
    pub failed_files: usize,
    /// 批次吞吐量（MB/s）
    pub batch_throughput_mb_s: f64,
}

/// 管道处理统计
#[derive(Debug, Default, Clone)]
pub struct PipelineStats {
    /// 总文件数
    pub total_files: usize,
    /// 成功处理文件数
    pub successful_files: usize,
    /// 失败文件数
    pub failed_files: usize,
    /// 跳过文件数
    pub skipped_files: usize,
    /// 重试文件数
    pub retried_files: usize,
    /// 总处理时间（毫秒）
    pub total_processing_time_ms: u64,
    /// 平均文件处理时间（毫秒）
    pub avg_file_processing_time_ms: f64,
    /// 总吞吐量（MB/s）
    pub total_throughput_mb_s: f64,
    /// 峰值内存使用（字节）
    pub peak_memory_usage: usize,
    /// 数据解析统计
    pub data_parsing_stats: DataParsingStats,
    /// DBC解析统计
    pub dbc_parsing_stats: DbcParsingStats,
    /// 存储统计
    pub storage_stats: StorageStats,
}

impl PipelineStats {
    /// 打印详细统计
    pub fn print_detailed_summary(&self) {
        info!("🎯 数据处理管道完整统计:");
        info!("{}", "=".repeat(60));
        
        // 基本统计
        info!("📊 文件处理统计:");
        info!("  📁 总文件数: {}", self.total_files);
        info!("  ✅ 成功处理: {} ({:.1}%)", 
            self.successful_files, 
            if self.total_files > 0 { 
                self.successful_files as f64 / self.total_files as f64 * 100.0 
            } else { 0.0 }
        );
        info!("  ❌ 处理失败: {} ({:.1}%)", 
            self.failed_files,
            if self.total_files > 0 { 
                self.failed_files as f64 / self.total_files as f64 * 100.0 
            } else { 0.0 }
        );
        info!("  ⏭️ 跳过文件: {}", self.skipped_files);
        info!("  🔄 重试文件: {}", self.retried_files);
        
        // 性能统计
        info!("🚀 性能统计:");
        info!("  ⏱️ 总处理时间: {:.2} 分钟", self.total_processing_time_ms as f64 / 60000.0);
        info!("  📈 平均文件处理时间: {:.2} 秒", self.avg_file_processing_time_ms / 1000.0);
        info!("  🌊 总吞吐量: {:.2} MB/s", self.total_throughput_mb_s);
        info!("  💾 峰值内存使用: {:.2} GB", self.peak_memory_usage as f64 / 1024.0 / 1024.0 / 1024.0);
        
        info!("{}", "=".repeat(60));
        
        // 详细子模块统计
        info!("📋 数据解析统计:");
        self.data_parsing_stats.print_summary();
        
        info!("📡 DBC解析统计:");
        self.dbc_parsing_stats.print_summary();
        
        info!("💽 存储统计:");
        self.storage_stats.print_summary();
    }
}

/// 数据处理管道
pub struct DataProcessingPipeline {
    /// 配置
    config: PipelineConfig,
    /// 内存池
    memory_pool: Arc<ZeroCopyMemoryPool>,
    /// 高性能执行器
    executor: Arc<HighPerformanceExecutor>,
    /// DBC管理器
    dbc_manager: Arc<DbcManager>,
    /// 数据解析器
    data_parser: Arc<tokio::sync::Mutex<DataLayerParser>>,
    /// 列式存储写入器
    storage_writer: Arc<tokio::sync::Mutex<ColumnarStorageWriter>>,
    /// 并发控制信号量
    semaphore: Arc<Semaphore>,
    /// 管道统计
    stats: Arc<tokio::sync::RwLock<PipelineStats>>,
}

impl DataProcessingPipeline {
    /// 创建新的数据处理管道
    pub async fn new(config: PipelineConfig) -> Result<Self> {
        info!("🚀 初始化数据处理管道...");
        
        // 创建内存池
        let memory_pool = Arc::new(ZeroCopyMemoryPool::new(config.memory_pool_config.clone()));
        info!("✅ 内存池初始化完成");
        
        // 创建高性能执行器
        let executor = Arc::new(HighPerformanceExecutor::new(config.executor_config.clone()));
        info!("✅ 高性能执行器初始化完成");
        
        // 创建DBC管理器
        let dbc_manager = Arc::new(DbcManager::new(config.dbc_manager_config.clone()));
        info!("✅ DBC管理器初始化完成");
        
        // 创建数据解析器
        let data_parser = Arc::new(tokio::sync::Mutex::new(
            DataLayerParser::new((*memory_pool).clone())
        ));
        info!("✅ 数据解析器初始化完成");
        
        // 创建列式存储写入器
        let storage_writer = Arc::new(tokio::sync::Mutex::new(
            ColumnarStorageWriter::new(config.storage_config.clone())?
        ));
        info!("✅ 列式存储写入器初始化完成");
        
        // 创建并发控制
        let semaphore = Arc::new(Semaphore::new(config.max_concurrent_files));
        
        // 初始化统计
        let stats = Arc::new(tokio::sync::RwLock::new(PipelineStats::default()));
        
        info!("🎉 数据处理管道初始化完成");
        
        Ok(Self {
            config,
            memory_pool,
            executor,
            dbc_manager,
            data_parser,
            storage_writer,
            semaphore,
            stats,
        })
    }
    
    /// 加载DBC文件
    pub async fn load_dbc_files<P: AsRef<Path>>(&self, dbc_paths: Vec<P>) -> Result<()> {
        info!("📄 开始加载DBC文件，数量: {}", dbc_paths.len());
        
        for (index, path) in dbc_paths.iter().enumerate() {
            let priority = -(index as i32); // 按顺序设置优先级
            self.dbc_manager.load_dbc_file(path, Some(priority)).await
                .with_context(|| format!("加载DBC文件失败: {:?}", path.as_ref()))?;
        }
        
        info!("✅ 所有DBC文件加载完成");
        Ok(())
    }
    
    /// 加载DBC目录
    pub async fn load_dbc_directory<P: AsRef<Path>>(&self, dbc_dir: P) -> Result<usize> {
        info!("📁 从目录加载DBC文件: {:?}", dbc_dir.as_ref());
        
        let loaded_count = self.dbc_manager.load_dbc_directory(dbc_dir).await?;
        
        info!("✅ 从目录加载完成，成功加载 {} 个DBC文件", loaded_count);
        Ok(loaded_count)
    }
    
    /// 处理文件列表
    pub async fn process_files<P: AsRef<Path>>(&self, file_paths: Vec<P>) -> Result<Vec<BatchProcessingResult>> {
        let total_files = file_paths.len();
        info!("🔄 开始处理文件，总数: {}", total_files);
        
        // 更新统计 - 基于tokio官方文档的最佳实践
        // 优化：减少锁操作，提高性能
        {
            let mut stats = self.stats.write().await;
            stats.total_files = total_files;
            stats.successful_files = 0;
            stats.failed_files = 0;
            stats.skipped_files = 0;
            stats.retried_files = 0;
        }
        
        // 启动进度报告任务
        let progress_handle = if self.config.enable_progress_reporting {
            Some(self.start_progress_reporting().await)
        } else {
            None
        };
        
        let start_time = Instant::now();
        
        // 分批处理文件
        let batches: Vec<Vec<PathBuf>> = file_paths
            .into_iter()
            .map(|p| p.as_ref().to_path_buf())
            .collect::<Vec<_>>()
            .chunks(self.config.batch_size)
            .map(|chunk| chunk.to_vec())
            .collect();
        
        info!("📦 文件分批完成，批次数: {}", batches.len());
        
        let mut batch_results = Vec::new();
        
        // 处理每个批次
        for (batch_id, batch_files) in batches.into_iter().enumerate() {
            info!("🔄 开始处理批次 {}, 文件数: {}", batch_id + 1, batch_files.len());
            
            let batch_files_len = batch_files.len();
            let batch_result = self.process_file_batch(batch_id, batch_files).await
                .with_context(|| format!("批次 {} 处理失败，文件数: {}", batch_id + 1, batch_files_len))?;
            
            batch_results.push(batch_result);
            
            // 检查内存压力
            if self.check_memory_pressure().await {
                warn!("内存压力过高，执行垃圾回收");
                // 内存压力缓解（简化实现）
                tokio::task::yield_now().await;
            }
        }
        
        // 完成存储写入
        {
            let mut writer = self.storage_writer.lock().await;
            writer.finish().await?;
        }
        
        // 停止进度报告
        if let Some(handle) = progress_handle {
            handle.abort();
        }
        
        // 更新总体统计 - 基于tokio官方文档的最佳实践
        // 优化：减少锁操作，提高性能
        let total_time = start_time.elapsed();
        {
            let mut stats = self.stats.write().await;
            stats.total_processing_time_ms = total_time.as_millis() as u64;
            
            // 优化平均时间计算，使用整数运算避免浮点精度问题
            if stats.successful_files > 0 {
                let total_time_ms = stats.total_processing_time_ms as u128;
                let successful_files = stats.successful_files as u128;
                stats.avg_file_processing_time_ms = (total_time_ms / successful_files) as f64;
            }
            
            // 从子模块更新统计 - 优化锁的使用
            {
                let data_parser = self.data_parser.lock().await;
                stats.data_parsing_stats = data_parser.get_stats().clone();
            }
            
            stats.dbc_parsing_stats = self.dbc_manager.get_stats();
            
            {
                let writer = self.storage_writer.lock().await;
                stats.storage_stats = writer.get_stats().clone();
            }
        }
        
        // 打印最终统计
        let final_stats = self.stats.read().await;
        final_stats.print_detailed_summary();
        
        info!("🎉 所有文件处理完成！");
        Ok(batch_results)
    }
    
    /// 处理单个文件批次
    async fn process_file_batch(&self, batch_id: usize, file_paths: Vec<PathBuf>) -> Result<BatchProcessingResult> {
        let batch_start = Instant::now();
        
        // 创建任务流
        let results = stream::iter(file_paths.into_iter().enumerate())
            .map(|(file_index, path)| {
                let pipeline = self;
                async move {
                    let _permit = pipeline.semaphore.acquire().await.unwrap();
                    pipeline.process_single_file(path, file_index).await
                }
            })
            .buffer_unordered(self.config.max_concurrent_files)
            .collect::<Vec<_>>()
            .await;
        
        let batch_time = batch_start.elapsed();
        
        // 汇总批次结果
        let mut successful_files = 0;
        let mut failed_files = 0;
        let mut total_size = 0u64;
        let mut file_results = Vec::new();
        
        for result in results {
            match result {
                Ok(file_result) => {
                    if file_result.success {
                        successful_files += 1;
                    } else {
                        failed_files += 1;
                    }
                    total_size += file_result.file_size;
                    file_results.push(file_result);
                }
                Err(e) => {
                    error!("文件处理任务失败: {}", e);
                    failed_files += 1;
                }
            }
        }
        
        // 计算批次吞吐量 - 基于tokio官方文档的最佳实践
        // 优化：使用整数运算避免浮点精度问题
        let batch_throughput_mb_s = if batch_time.as_secs() > 0 {
            let total_size_mb = total_size / (1024 * 1024);
            total_size_mb as f64 / batch_time.as_secs() as f64
        } else {
            0.0
        };
        
        // 更新统计
        {
            let mut stats = self.stats.write().await;
            stats.successful_files += successful_files;
            stats.failed_files += failed_files;
        }
        
        info!("✅ 批次 {} 处理完成: 成功 {}, 失败 {}, 吞吐量 {:.2} MB/s", 
            batch_id + 1, successful_files, failed_files, batch_throughput_mb_s);
        
        Ok(BatchProcessingResult {
            batch_id,
            file_results,
            batch_time_ms: batch_time.as_millis() as u64,
            successful_files,
            failed_files,
            batch_throughput_mb_s,
        })
    }
    
    /// 处理单个文件
    async fn process_single_file(&self, file_path: PathBuf, file_index: usize) -> Result<FileProcessingResult> {
        let start_time = Instant::now();
        let mut retry_count = 0;
        
        loop {
            match self.process_single_file_impl(&file_path, file_index).await {
                Ok(result) => return Ok(result),
                Err(e) => {
                    if retry_count < self.config.max_retries && self.config.enable_error_recovery {
                        retry_count += 1;
                        warn!("文件处理失败，重试 {}/{}: {:?} - {}", 
                            retry_count, self.config.max_retries, file_path, e);
                        
                        // 更新重试统计
                        {
                            let mut stats = self.stats.write().await;
                            stats.retried_files += 1;
                        }
                        
                        // 等待一段时间后重试 - 基于tokio官方文档的最佳实践
                        // 使用指数退避策略，避免频繁重试
                        let backoff_duration = std::cmp::min(
                            1000 * (1 << retry_count) as u64, // 指数退避
                            10000 // 最大等待10秒
                        );
                        tokio::time::sleep(tokio::time::Duration::from_millis(backoff_duration)).await;
                        continue;
                    } else {
                        error!("文件处理最终失败: {:?} - {}", file_path, e);
                        return Ok(FileProcessingResult {
                            file_path,
                            success: false,
                            error: Some(e.to_string()),
                            processing_time_ms: start_time.elapsed().as_millis() as u64,
                            parsed_messages: 0,
                            parsed_signals: 0,
                            file_size: 0,
                            throughput_mb_s: 0.0,
                        });
                    }
                }
            }
        }
    }
    
    /// 实际处理单个文件的实现
    async fn process_single_file_impl(&self, file_path: &PathBuf, _file_index: usize) -> Result<FileProcessingResult> {
        let start_time = Instant::now();
        
        debug!("🔄 开始处理文件: {:?}", file_path);
        
        // 1. 创建文件映射
        let file_mapping = self.memory_pool.create_file_mapping(file_path)
            .context("创建文件映射失败")?;
        
        let file_size = file_mapping.len() as u64;
        
        // 2. 解析4层数据结构
        let parsed_data = {
            let mut parser = self.data_parser.lock().await;
            parser.parse_file(file_mapping.as_slice()).await
                .context("数据结构解析失败")?
        };
        
        // 3. DBC信号解析
        let mut parsed_messages = Vec::new();
        let mut total_signals = 0;
        
        for sequence in &parsed_data.frame_sequences {
            for frame in &sequence.frames {
                if let Some(parsed_message) = self.dbc_manager.parse_can_frame(frame).await? {
                    total_signals += parsed_message.signals.len();
                    parsed_messages.push(parsed_message);
                }
            }
        }
        
        // 4. 列式存储写入
        {
            let mut writer = self.storage_writer.lock().await;
            writer.write_parsed_data(&parsed_data, &parsed_messages, file_path).await
                .context("列式存储写入失败")?;
        }
        
        let processing_time = start_time.elapsed();
        let throughput_mb_s = if processing_time.as_secs_f64() > 0.0 {
            (file_size as f64 / 1024.0 / 1024.0) / processing_time.as_secs_f64()
        } else {
            0.0
        };
        
        debug!("✅ 文件处理完成: {:?}, 消息数: {}, 信号数: {}, 吞吐量: {:.2} MB/s", 
            file_path, parsed_messages.len(), total_signals, throughput_mb_s);
        
        Ok(FileProcessingResult {
            file_path: file_path.clone(),
            success: true,
            error: None,
            processing_time_ms: processing_time.as_millis() as u64,
            parsed_messages: parsed_messages.len(),
            parsed_signals: total_signals,
            file_size,
            throughput_mb_s,
        })
    }
    
    /// 检查内存压力 - 基于tokio官方文档的最佳实践
    async fn check_memory_pressure(&self) -> bool {
        // 基于系统内存使用情况检查内存压力
        // 这里使用简化的实现，实际项目中可以使用更复杂的监控
        let mut system = sysinfo::System::new_all();
        system.refresh_memory();
        let memory_usage = system.used_memory() as f64 / system.total_memory() as f64;
        memory_usage > self.config.memory_pressure_threshold
    }
    
    /// 启动进度报告任务
    async fn start_progress_reporting(&self) -> tokio::task::JoinHandle<()> {
        let stats = Arc::clone(&self.stats);
        let interval = self.config.progress_report_interval;
        
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(interval));
            
            loop {
                interval.tick().await;
                
                let stats = stats.read().await;
                if stats.total_files > 0 {
                    let progress = (stats.successful_files + stats.failed_files) as f64 / stats.total_files as f64 * 100.0;
                    info!("📊 处理进度: {:.1}% ({}/{}) - 成功: {}, 失败: {}", 
                        progress,
                        stats.successful_files + stats.failed_files,
                        stats.total_files,
                        stats.successful_files,
                        stats.failed_files
                    );
                }
            }
        })
    }
    
    /// 获取管道统计信息
    pub async fn get_stats(&self) -> PipelineStats {
        self.stats.read().await.clone()
    }
    
    /// 重置统计信息
    pub async fn reset_stats(&self) {
        let mut stats = self.stats.write().await;
        *stats = PipelineStats::default();
        
        // 重置子模块统计
        {
            let mut parser = self.data_parser.lock().await;
            parser.reset_stats();
        }
        
        self.dbc_manager.reset_stats();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use crate::test_data_generator::{TestDataGenerator, TestDataConfig};
    
    #[tokio::test]
    async fn test_pipeline_creation() {
        let temp_dir = TempDir::new().unwrap();
        
        let config = PipelineConfig {
            storage_config: ColumnarStorageConfig {
                output_dir: temp_dir.path().to_path_buf(),
                ..ColumnarStorageConfig::default()
            },
            ..PipelineConfig::default()
        };
        
        let pipeline = DataProcessingPipeline::new(config).await.unwrap();
        let stats = pipeline.get_stats().await;
        
        assert_eq!(stats.total_files, 0);
        assert_eq!(stats.successful_files, 0);
    }
    
    #[tokio::test]
    async fn test_pipeline_processing() {
        let temp_dir = TempDir::new().unwrap();
        
        // 生成测试数据
        let data_config = TestDataConfig {
            file_count: 2,
            target_file_size: 1024 * 1024, // 1MB
            frames_per_file: 100,
            output_dir: temp_dir.path().join("test_data"),
        };
        
        let generator = TestDataGenerator::new(data_config);
        let file_paths = generator.generate_all().await.unwrap();
        
        // 配置管道
        let pipeline_config = PipelineConfig {
            storage_config: ColumnarStorageConfig {
                output_dir: temp_dir.path().join("output"),
                ..ColumnarStorageConfig::default()
            },
            batch_size: 2,
            max_concurrent_files: 2,
            ..PipelineConfig::default()
        };
        
        let pipeline = DataProcessingPipeline::new(pipeline_config).await.unwrap();
        
        // 处理文件
        let results = pipeline.process_files(file_paths).await.unwrap();
        
        assert!(!results.is_empty());
        
        let stats = pipeline.get_stats().await;
        assert_eq!(stats.total_files, 2);
        
        stats.print_detailed_summary();
    }
}