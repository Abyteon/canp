//! # 数据处理任务示例
//! 
//! 展示如何使用ZeroCopyMemoryPool处理大规模文件的完整流程

use canp::zero_copy_memory_pool::{ZeroCopyMemoryPool, MemoryPoolConfig};
use anyhow::Result;
use std::path::PathBuf;

use flate2::read::GzDecoder;
use std::io::Read;
use tracing::{info, error, warn};

/// 模拟的数据处理任务
struct DataProcessingTask {
    /// 内存池
    pool: ZeroCopyMemoryPool,
    /// 文件路径列表
    file_paths: Vec<PathBuf>,
}

impl DataProcessingTask {
    /// 创建新的数据处理任务
    pub fn new(file_paths: Vec<PathBuf>) -> Self {
        let config = MemoryPoolConfig {
            // 针对您的数据特征优化
            decompress_buffer_sizes: vec![
                16 * 1024,   // 16KB - 对应~10KB压缩数据
                64 * 1024,   // 64KB - 中等解压结果
                256 * 1024,  // 256KB - 大型解压结果
                1024 * 1024, // 1MB - 超大解压结果
            ],
            mmap_cache_size: 500, // 缓存500个15MB文件
            max_memory_usage: 4 * 1024 * 1024 * 1024, // 4GB限制
        };

        Self {
            pool: ZeroCopyMemoryPool::new(config),
            file_paths,
        }
    }

    /// 处理单个文件的完整流程
    pub async fn process_single_file(&self, file_path: &PathBuf) -> Result<ProcessingResult> {
        info!("📁 开始处理文件: {:?}", file_path);

        // 1. 零拷贝文件映射 (mmap)
        let file_mapping = self.pool.create_file_mapping(file_path)
            .map_err(|e| anyhow::anyhow!("文件映射失败: {}", e))?;

        info!("🗺️ 文件映射完成: {} bytes", file_mapping.len());

        // 2. 解析文件头部（35字节）
        if file_mapping.len() < 35 {
            return Err(anyhow::anyhow!("文件太小，无法包含完整头部"));
        }

        let header = file_mapping.slice(0, 35);
        // 根据任务要求：35字节头部的"后四个字节"（位置31-34）为压缩数据长度
        let compressed_data_length = u32::from_be_bytes([
            header[31], header[32], header[33], header[34]
        ]) as usize;

        info!("📋 头部解析完成，压缩数据长度: {} bytes", compressed_data_length);

        // 3. 提取压缩数据（零拷贝）
        if file_mapping.len() < 35 + compressed_data_length {
            return Err(anyhow::anyhow!("文件长度不足，无法包含完整压缩数据"));
        }

        let compressed_data = file_mapping.slice(35, compressed_data_length);
        info!("📦 压缩数据提取完成: {} bytes", compressed_data.len());

        // 4. 解压数据（需要内存分配）
        let decompressed_data = self.decompress_data(compressed_data).await?;
        info!("🔓 数据解压完成: {} bytes", decompressed_data.len());

        // 5. 解析解压后的数据头部（20字节）
        let decompressed_slice = decompressed_data.as_slice();
        if decompressed_slice.len() < 20 {
            return Err(anyhow::anyhow!("解压数据太小，无法包含完整头部"));
        }

        let decompressed_header = &decompressed_slice[0..20];
        let frame_data_length = u32::from_be_bytes([
            decompressed_header[16], decompressed_header[17], 
            decompressed_header[18], decompressed_header[19]
        ]) as usize;

        info!("📊 解压头部解析完成，帧数据长度: {} bytes", frame_data_length);

        // 6. 处理帧序列数据（零拷贝）
        let frame_results = self.process_frame_sequences(
            &decompressed_data, 
            20, 
            frame_data_length
        ).await?;

        info!("🎯 文件处理完成: {} 个帧序列", frame_results.len());

        Ok(ProcessingResult {
            file_path: file_path.clone(),
            total_frames: frame_results.iter().map(|r| r.frame_count).sum(),
            frame_sequences: frame_results,
            original_size: file_mapping.len(),
            compressed_size: compressed_data_length,
            decompressed_size: decompressed_data.len(),
        })
    }

    /// 解压数据
    async fn decompress_data(&self, compressed_data: &[u8]) -> Result<canp::zero_copy_memory_pool::ZeroCopyBuffer> {
        // 预估解压后大小（通常比压缩数据大3-10倍）
        let estimated_size = compressed_data.len() * 5;
        
        // 从池中获取缓冲区
        let mut buffer = self.pool.get_decompress_buffer(estimated_size).await;

        // 使用gzip解压
        let mut decoder = GzDecoder::new(compressed_data);
        let mut temp_vec = Vec::new();
        decoder.read_to_end(&mut temp_vec)
            .map_err(|e| anyhow::anyhow!("解压失败: {}", e))?;

        // 将解压结果写入缓冲区
        buffer.put_slice(&temp_vec);

        // 冻结为零拷贝缓冲区
        Ok(buffer.freeze())
    }

    /// 处理帧序列数据（零拷贝）
    async fn process_frame_sequences(
        &self,
        decompressed_data: &canp::zero_copy_memory_pool::ZeroCopyBuffer,
        offset: usize,
        total_length: usize,
    ) -> Result<Vec<FrameSequenceResult>> {
        let mut results = Vec::new();
        let mut current_offset = offset;
        let data_slice = decompressed_data.as_slice();

        while current_offset < offset + total_length {
            // 确保有足够的数据读取16字节长度信息
            if current_offset + 16 > data_slice.len() {
                break;
            }

            // 解析帧序列长度（16字节中的12-15字节）
            let length_bytes = &data_slice[current_offset + 12..current_offset + 16];
            let sequence_length = u32::from_be_bytes([
                length_bytes[0], length_bytes[1], length_bytes[2], length_bytes[3]
            ]) as usize;

            if current_offset + 16 + sequence_length > data_slice.len() {
                warn!("⚠️ 帧序列长度超出数据范围，跳过");
                break;
            }

            // 零拷贝提取帧序列数据
            let sequence_data = &data_slice[current_offset + 16..current_offset + 16 + sequence_length];
            
            // 处理单个帧序列（这里只是计数，实际应该用DBC解析）
            let frame_count = self.count_frames_in_sequence(sequence_data);

            results.push(FrameSequenceResult {
                offset: current_offset,
                length: sequence_length,
                frame_count,
            });

            current_offset += 16 + sequence_length;
        }

        Ok(results)
    }

    /// 计算帧序列中的帧数量（模拟CAN帧解析）
    fn count_frames_in_sequence(&self, sequence_data: &[u8]) -> usize {
        // 这里简化处理，假设每个CAN帧8字节
        // 实际应该使用can-dbc库进行解析
        sequence_data.len() / 8
    }

    /// 批量处理文件
    pub async fn process_batch(&self, batch_size: usize) -> Result<Vec<ProcessingResult>> {
        let mut results = Vec::new();
        
        for chunk in self.file_paths.chunks(batch_size) {
            info!("🚀 开始处理批次: {} 个文件", chunk.len());
            
            // 并发处理批次中的文件
            let batch_futures: Vec<_> = chunk
                .iter()
                .map(|path| self.process_single_file(path))
                .collect();

            for future in batch_futures {
                match future.await {
                    Ok(result) => results.push(result),
                    Err(e) => error!("❌ 文件处理失败: {}", e),
                }
            }

            // 清理过期的文件映射缓存
            self.pool.cleanup_expired_mappings();
            
            info!("✅ 批次处理完成，当前内存使用: {} MB", 
                  self.pool.get_memory_usage() / 1024 / 1024);
        }

        Ok(results)
    }

    /// 处理所有文件
    pub async fn process_all(&self) -> Result<ProcessingSummary> {
        info!("🎯 开始处理所有文件: {} 个", self.file_paths.len());
        
        let start_time = std::time::Instant::now();
        let results = self.process_batch(50).await?; // 每批处理50个文件
        let duration = start_time.elapsed();

        let summary = ProcessingSummary {
            total_files: self.file_paths.len(),
            successful_files: results.len(),
            total_frames: results.iter().map(|r| r.total_frames).sum(),
            total_original_size: results.iter().map(|r| r.original_size).sum(),
            total_compressed_size: results.iter().map(|r| r.compressed_size).sum(),
            total_decompressed_size: results.iter().map(|r| r.decompressed_size).sum(),
            processing_duration: duration,
            throughput_mb_per_sec: {
                let total_mb = results.iter().map(|r| r.original_size).sum::<usize>() as f64 / 1024.0 / 1024.0;
                total_mb / duration.as_secs_f64()
            },
        };

        info!("🎉 处理完成！统计信息: {:#?}", summary);
        Ok(summary)
    }
}

/// 处理结果
#[derive(Debug)]
pub struct ProcessingResult {
    pub file_path: PathBuf,
    pub total_frames: usize,
    pub frame_sequences: Vec<FrameSequenceResult>,
    pub original_size: usize,
    pub compressed_size: usize,
    pub decompressed_size: usize,
}

/// 帧序列结果
#[derive(Debug)]
pub struct FrameSequenceResult {
    pub offset: usize,
    pub length: usize,
    pub frame_count: usize,
}

/// 处理总结
#[derive(Debug)]
pub struct ProcessingSummary {
    pub total_files: usize,
    pub successful_files: usize,
    pub total_frames: usize,
    pub total_original_size: usize,
    pub total_compressed_size: usize,
    pub total_decompressed_size: usize,
    pub processing_duration: std::time::Duration,
    pub throughput_mb_per_sec: f64,
}

#[tokio::main]
async fn main() -> Result<()> {
    // 初始化日志
    tracing_subscriber::fmt::init();

    // 模拟8000个文件路径（实际使用时从目录扫描获取）
    // 使用生成的测试数据文件
    let test_data_dir = PathBuf::from("test_data");
    let mut file_paths = Vec::new();
    
    if test_data_dir.exists() {
        for entry in std::fs::read_dir(&test_data_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_file() && path.extension().map_or(false, |ext| ext == "bin") {
                file_paths.push(path);
            }
        }
    }
    
    if file_paths.is_empty() {
        eprintln!("❌ 未找到测试数据文件！");
        eprintln!("💡 请先运行以下命令生成测试数据:");
        eprintln!("   cargo run --example generate_test_data");
        return Ok(());
    }
    
    file_paths.sort(); // 确保处理顺序一致
    println!("📁 找到 {} 个测试文件", file_paths.len());

    // 创建处理任务
    let task = DataProcessingTask::new(file_paths);

    // 处理所有文件
    match task.process_all().await {
        Ok(summary) => {
            println!("🎯 处理总结:");
            println!("  📁 总文件数: {}", summary.total_files);
            println!("  ✅ 成功处理: {}", summary.successful_files);
            println!("  🎲 总帧数: {}", summary.total_frames);
            println!("  📊 原始数据: {} MB", summary.total_original_size / 1024 / 1024);
            println!("  📦 压缩数据: {} MB", summary.total_compressed_size / 1024 / 1024);
            println!("  🔓 解压数据: {} MB", summary.total_decompressed_size / 1024 / 1024);
            println!("  ⏱️ 处理时间: {:.2}s", summary.processing_duration.as_secs_f64());
            println!("  🚀 吞吐量: {:.2} MB/s", summary.throughput_mb_per_sec);
            
            let compression_ratio = summary.total_compressed_size as f64 / summary.total_decompressed_size as f64;
            println!("  📈 压缩比: {:.2}", compression_ratio);
        }
        Err(e) => {
            error!("❌ 处理失败: {}", e);
        }
    }

    Ok(())
}