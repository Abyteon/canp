use crate::memory_pool::{UnifiedMemoryPool, MemoryPoolConfig, MemoryBlock};
use crate::thread_pool::{PipelineThreadPool, ThreadPoolConfig};
use crate::layer_parser::{LayerParser, LayerParserConfig, ParseResult, DataBlock, LayerType};
use crate::performance::{PerformanceMonitor, PerformanceConfig};
use anyhow::{Result, anyhow};
use std::sync::Arc;
use std::path::PathBuf;
use std::collections::VecDeque;
use tokio::sync::{mpsc, RwLock};
use tracing::{info, error, debug};

/// 流水线配置
#[derive(Debug, Clone)]
pub struct PipelineConfig {
    /// 内存池配置
    pub memory_pool: MemoryPoolConfig,
    /// 线程池配置
    pub thread_pool: ThreadPoolConfig,
    /// 分层解析器配置
    pub layer_parser: LayerParserConfig,
    /// 性能监控配置
    pub performance: PerformanceConfig,
    /// 批处理大小
    pub batch_size: usize,
    /// 并发文件处理数量
    pub concurrent_files: usize,
    /// 输出目录
    pub output_dir: PathBuf,
    /// 是否启用解压缩
    pub enable_decompression: bool,
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            memory_pool: MemoryPoolConfig::default(),
            thread_pool: ThreadPoolConfig::default(),
            layer_parser: LayerParserConfig::default(),
            performance: PerformanceConfig::default(),
            batch_size: 100,
            concurrent_files: 4,
            output_dir: PathBuf::from("./output"),
            enable_decompression: true,
        }
    }
}

/// 流水线阶段
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PipelineStage {
    /// 文件映射阶段
    FileMapping,
    /// 第0层解析阶段
    Layer0Parsing,
    /// 解压缩阶段
    Decompression,
    /// 第1层解析阶段
    Layer1Parsing,
    /// 第2层解析阶段
    Layer2Parsing,
    /// 最终层解析阶段
    FinalLayerParsing,
    /// 输出阶段
    Output,
}

/// 流水线任务
#[derive(Debug)]
pub struct PipelineTask {
    /// 任务ID
    pub id: String,
    /// 文件路径
    pub file_path: PathBuf,
    /// 当前阶段
    pub stage: PipelineStage,
    /// 数据块
    pub data_blocks: Vec<DataBlock>,
    /// 解析结果
    pub parse_results: Vec<ParseResult>,
    /// 内存块
    pub memory_blocks: Vec<MemoryBlock>,
    /// 创建时间
    pub created_at: std::time::Instant,
}

impl PipelineTask {
    pub fn new(file_path: PathBuf) -> Self {
        Self {
            id: format!("task_{}", std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()),
            file_path,
            stage: PipelineStage::FileMapping,
            data_blocks: Vec::new(),
            parse_results: Vec::new(),
            memory_blocks: Vec::new(),
            created_at: std::time::Instant::now(),
        }
    }
}

/// 流水线统计信息
#[derive(Debug, Clone)]
pub struct PipelineStats {
    /// 处理的文件数量
    pub files_processed: usize,
    /// 处理的字节数
    pub bytes_processed: usize,
    /// 各阶段处理时间（微秒）
    pub stage_times: std::collections::HashMap<PipelineStage, u64>,
    /// 错误数量
    pub error_count: usize,
    /// 开始时间
    pub start_time: std::time::Instant,
}

impl Default for PipelineStats {
    fn default() -> Self {
        Self {
            files_processed: 0,
            bytes_processed: 0,
            stage_times: std::collections::HashMap::new(),
            error_count: 0,
            start_time: std::time::Instant::now(),
        }
    }
}

/// 高性能分层批量并发流水线
pub struct Pipeline {
    /// 配置
    config: PipelineConfig,
    /// 内存池
    memory_pool: Arc<UnifiedMemoryPool>,
    /// 线程池
    thread_pool: Arc<PipelineThreadPool>,
    /// 分层解析器
    layer_parser: Arc<LayerParser>,
    /// 性能监控器
    performance_monitor: Arc<PerformanceMonitor>,
    /// 任务队列
    task_queue: Arc<RwLock<VecDeque<PipelineTask>>>,
    /// 统计信息
    stats: Arc<RwLock<PipelineStats>>,
    /// 任务发送器
    task_sender: mpsc::Sender<PipelineTask>,
    /// 任务接收器
    task_receiver: mpsc::Receiver<PipelineTask>,
}

impl Pipeline {
    /// 创建新的流水线
    pub async fn new(config: PipelineConfig) -> Result<Self> {
        // 创建输出目录
        std::fs::create_dir_all(&config.output_dir)?;

        // 初始化组件
        let memory_pool = Arc::new(UnifiedMemoryPool::new(config.memory_pool.clone()));
        let thread_pool = Arc::new(PipelineThreadPool::new(config.thread_pool.clone()));
        let layer_parser = Arc::new(LayerParser::new(config.layer_parser.clone())?);
        let performance_monitor = Arc::new(PerformanceMonitor::new(config.performance.clone())?);

        // 创建任务通道
        let (task_sender, task_receiver) = mpsc::channel(config.concurrent_files * 2);

        Ok(Self {
            config,
            memory_pool,
            thread_pool,
            layer_parser,
            performance_monitor,
            task_queue: Arc::new(RwLock::new(VecDeque::new())),
            stats: Arc::new(RwLock::new(PipelineStats::default())),
            task_sender,
            task_receiver,
        })
    }

    /// 处理单个文件
    pub async fn process_file(&self, file_path: PathBuf) -> Result<()> {
        let task = PipelineTask::new(file_path);
        self.task_sender.send(task).await?;
        Ok(())
    }

    /// 处理多个文件
    pub async fn process_files(&self, file_paths: Vec<PathBuf>) -> Result<()> {
        for file_path in file_paths {
            self.process_file(file_path).await?;
        }
        Ok(())
    }

    /// 启动流水线处理
    pub async fn start_processing(&mut self) -> Result<()> {
        info!("启动流水线处理...");

        // 启动工作线程
        let mut handles = Vec::new();
        
        for _ in 0..self.config.concurrent_files {
            let memory_pool = self.memory_pool.clone();
            let thread_pool = self.thread_pool.clone();
            let layer_parser = self.layer_parser.clone();
            let performance_monitor = self.performance_monitor.clone();
            let stats = self.stats.clone();
            let task_sender = self.task_sender.clone();
            let config = self.config.clone();

            let handle = tokio::spawn(async move {
                // 这里应该从某个共享队列中获取任务
                // 暂时简化处理，直接返回
                debug!("工作线程启动");
            });
            handles.push(handle);
        }

        // 等待所有工作线程完成
        for handle in handles {
            handle.await?;
        }

        info!("流水线处理完成");
        Ok(())
    }

    /// 处理单个任务
    async fn process_task(
        memory_pool: &Arc<UnifiedMemoryPool>,
        thread_pool: &Arc<PipelineThreadPool>,
        layer_parser: &Arc<LayerParser>,
        performance_monitor: &Arc<PerformanceMonitor>,
        stats: &Arc<RwLock<PipelineStats>>,
        config: &PipelineConfig,
        task: &mut PipelineTask,
    ) -> Result<()> {
        debug!("开始处理任务: {}", task.id);

        // 阶段1: 文件映射
        task.stage = PipelineStage::FileMapping;
        let stage_start = std::time::Instant::now();
        
        let mmap_block = memory_pool.create_file_mmap(&task.file_path).await?;
        performance_monitor.record_memory_allocation(mmap_block.len(), "file_mmap");
        
        let stage_time = stage_start.elapsed().as_micros() as u64;
        {
            let mut stats = stats.write().await;
            stats.stage_times.insert(PipelineStage::FileMapping, stage_time);
        }

        // 阶段2: 第0层解析
        task.stage = PipelineStage::Layer0Parsing;
        let stage_start = std::time::Instant::now();
        
        let layer0_result = layer_parser.parse_layer_0(&mmap_block)?;
        task.parse_results.push(layer0_result.clone());
        
        // 提取压缩数据块
        let compressed_block = layer0_result.data_blocks
            .iter()
            .find(|block| matches!(block.block_type, crate::layer_parser::BlockType::CompressedData))
            .ok_or_else(|| anyhow!("未找到压缩数据块"))?;
        
        let stage_time = stage_start.elapsed().as_micros() as u64;
        {
            let mut stats = stats.write().await;
            stats.stage_times.insert(PipelineStage::Layer0Parsing, stage_time);
        }

        // 阶段3: 解压缩（如果需要）
        if config.enable_decompression {
            task.stage = PipelineStage::Decompression;
            let stage_start = std::time::Instant::now();
            
            // 分配解压缩缓冲区
            let _decompress_buffer = memory_pool.allocate_decompress_buffer(compressed_block.ptr_and_len.1)?;
            
            // 这里应该实现实际的解压缩逻辑
            // 暂时跳过解压缩，直接使用原始数据
            let _decompressed_data = compressed_block.ptr_and_len;
            
            let stage_time = stage_start.elapsed().as_micros() as u64;
            {
                let mut stats = stats.write().await;
                stats.stage_times.insert(PipelineStage::Decompression, stage_time);
            }
        }

        // 阶段4: 第1层解析
        task.stage = PipelineStage::Layer1Parsing;
        let stage_start = std::time::Instant::now();
        
        let layer1_result = layer_parser.parse_layer_1(
            compressed_block.ptr_and_len.0,
            compressed_block.ptr_and_len.1,
        )?;
        task.parse_results.push(layer1_result.clone());
        
        let stage_time = stage_start.elapsed().as_micros() as u64;
        {
            let mut stats = stats.write().await;
            stats.stage_times.insert(PipelineStage::Layer1Parsing, stage_time);
        }

        // 阶段5: 第2层解析
        task.stage = PipelineStage::Layer2Parsing;
        let stage_start = std::time::Instant::now();
        
        let uncompressed_blocks: Vec<_> = layer1_result.data_blocks
            .iter()
            .filter(|block| matches!(block.block_type, crate::layer_parser::BlockType::UncompressedData))
            .collect();
        
        for block in &uncompressed_blocks {
            let layer2_result = layer_parser.parse_layer_2(block.ptr_and_len.0, block.ptr_and_len.1)?;
            task.parse_results.push(layer2_result);
        }
        
        let stage_time = stage_start.elapsed().as_micros() as u64;
        {
            let mut stats = stats.write().await;
            stats.stage_times.insert(PipelineStage::Layer2Parsing, stage_time);
        }

        // 阶段6: 最终层解析
        task.stage = PipelineStage::FinalLayerParsing;
        let stage_start = std::time::Instant::now();
        
        let mut final_results = Vec::new();
        for layer2_result in &task.parse_results {
            if layer2_result.layer_type == LayerType::Layer2 {
                for block in &layer2_result.data_blocks {
                    if matches!(block.block_type, crate::layer_parser::BlockType::UncompressedData) {
                        let final_result = layer_parser.parse_final_layer(block.ptr_and_len.0, block.ptr_and_len.1)?;
                        final_results.push(final_result);
                    }
                }
            }
        }
        
        // 将最终结果添加到任务中
        task.parse_results.extend(final_results);
        
        let stage_time = stage_start.elapsed().as_micros() as u64;
        {
            let mut stats = stats.write().await;
            stats.stage_times.insert(PipelineStage::FinalLayerParsing, stage_time);
        }

        // 阶段7: 输出到文件
        task.stage = PipelineStage::Output;
        let stage_start = std::time::Instant::now();
        
        Self::output_results(config, task).await?;
        
        let stage_time = stage_start.elapsed().as_micros() as u64;
        {
            let mut stats = stats.write().await;
            stats.stage_times.insert(PipelineStage::Output, stage_time);
            stats.files_processed += 1;
            stats.bytes_processed += mmap_block.len();
        }

        debug!("任务处理完成: {}", task.id);
        Ok(())
    }

    /// 输出结果到文件
    async fn output_results(config: &PipelineConfig, task: &PipelineTask) -> Result<()> {
        let output_file = config.output_dir.join(format!("{}.json", task.id));
        
        // 创建输出数据结构
        let output_data = serde_json::json!({
            "task_id": task.id,
            "file_path": task.file_path.to_string_lossy(),
            "processing_time": task.created_at.elapsed().as_millis(),
            "parse_results": task.parse_results.iter().map(|result| {
                serde_json::json!({
                    "layer_type": format!("{:?}", result.layer_type),
                    "data_blocks_count": result.data_blocks.len(),
                    "bytes_parsed": result.stats.bytes_parsed,
                    "parse_time_us": result.stats.parse_time_us,
                })
            }).collect::<Vec<_>>(),
        });
        
        // 写入文件
        let output_content = serde_json::to_string_pretty(&output_data)?;
        tokio::fs::write(&output_file, output_content).await?;
        
        info!("输出结果到文件: {:?}", output_file);
        Ok(())
    }

    /// 获取统计信息
    pub async fn get_stats(&self) -> PipelineStats {
        self.stats.read().await.clone()
    }

    /// 打印统计信息
    pub async fn print_stats(&self) {
        let stats = self.get_stats().await;
        let total_time = stats.start_time.elapsed();
        
        println!("=== 流水线统计信息 ===");
        println!("处理文件数量: {}", stats.files_processed);
        println!("处理字节数: {}", stats.bytes_processed);
        println!("总处理时间: {:?}", total_time);
        println!("错误数量: {}", stats.error_count);
        
        if stats.files_processed > 0 {
            println!("平均处理时间: {:?}", total_time / stats.files_processed as u32);
            println!("处理速度: {:.2} MB/s", 
                stats.bytes_processed as f64 / 1024.0 / 1024.0 / total_time.as_secs_f64());
        }
        
        println!("\n各阶段处理时间:");
        for (stage, time_us) in &stats.stage_times {
            println!("  {:?}: {} 微秒", stage, time_us);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_pipeline_creation() {
        let config = PipelineConfig::default();
        // 在同步上下文中测试配置创建
        assert_eq!(config.concurrent_files, 4);
        assert_eq!(config.batch_size, 100);
        assert!(config.enable_decompression);
    }

    #[tokio::test]
    async fn test_pipeline_task_creation() {
        let temp_dir = tempdir().unwrap();
        let file_path = temp_dir.path().join("test.dbc");
        
        let task = PipelineTask::new(file_path);
        assert_eq!(task.stage, PipelineStage::FileMapping);
        assert!(task.data_blocks.is_empty());
        assert!(task.parse_results.is_empty());
    }
} 