use canp::pipeline::{Pipeline, PipelineConfig};
use std::path::PathBuf;
use std::fs;
use tempfile::tempdir;

/// 创建测试文件
fn create_test_file(dir: &PathBuf, index: usize) -> PathBuf {
    use std::fs::File;
    use std::io::Write;
    
    let file_path = dir.join(format!("test_file_{}.bin", index));
    let mut file = File::create(&file_path).unwrap();
    
    // 第0层：35字节头部 + 压缩数据
    let mut layer0_data = vec![0u8; 35];
    
    // 创建压缩数据（模拟）：包含多个第1层数据块
    let mut compressed_data = Vec::new();
    
    // 第1层数据块1：20字节头部 + 数据
    let mut layer1_block1 = vec![0u8; 20];
    layer1_block1[16] = 0; // 数据长度（大端序）：30字节
    layer1_block1[17] = 0;
    layer1_block1[18] = 0;
    layer1_block1[19] = 30;
    compressed_data.extend_from_slice(&layer1_block1);
    
    // 第1层数据块1的数据部分
    let layer1_data1 = vec![index as u8; 30];
    compressed_data.extend_from_slice(&layer1_data1);
    
    // 第1层数据块2：20字节头部 + 数据
    let mut layer1_block2 = vec![0u8; 20];
    layer1_block2[16] = 0; // 数据长度（大端序）：25字节
    layer1_block2[17] = 0;
    layer1_block2[18] = 0;
    layer1_block2[19] = 25;
    compressed_data.extend_from_slice(&layer1_block2);
    
    // 第1层数据块2的数据部分
    let layer1_data2 = vec![(index * 2) as u8; 25];
    compressed_data.extend_from_slice(&layer1_data2);
    
    // 设置压缩数据长度（大端序）：实际压缩数据长度
    let compressed_length = compressed_data.len() as u32;
    layer0_data[31] = ((compressed_length >> 24) & 0xFF) as u8;
    layer0_data[32] = ((compressed_length >> 16) & 0xFF) as u8;
    layer0_data[33] = ((compressed_length >> 8) & 0xFF) as u8;
    layer0_data[34] = (compressed_length & 0xFF) as u8;
    
    // 写入第0层头部
    file.write_all(&layer0_data).unwrap();
    
    // 写入压缩数据
    file.write_all(&compressed_data).unwrap();
    
    file_path
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 初始化日志
    tracing_subscriber::fmt::init();
    
    println!("=== 高性能分层批量并发流水线示例 ===");
    
    // 创建临时目录用于测试
    let temp_dir = tempdir()?;
    let input_dir = temp_dir.path().join("input");
    let output_dir = temp_dir.path().join("output");
    
    fs::create_dir_all(&input_dir)?;
    fs::create_dir_all(&output_dir)?;
    
    println!("✓ 创建临时目录: {:?}", temp_dir.path());
    
    // 创建测试文件
    let mut file_paths = Vec::new();
    let file_count = 10;
    
    for i in 0..file_count {
        let file_path = create_test_file(&input_dir, i);
        file_paths.push(file_path);
    }
    
    println!("✓ 创建 {} 个测试文件", file_count);
    
    // 配置流水线
    let mut config = PipelineConfig::default();
    config.concurrent_files = 4; // 并发处理4个文件
    config.batch_size = 50; // 批处理大小
    config.output_dir = output_dir.clone();
    config.enable_decompression = true;
    
    // 调整内存池配置
    config.memory_pool.max_total_memory = 1024 * 1024 * 1024; // 1GB
    config.memory_pool.block_cache_size = 1000;
    config.memory_pool.mmap_cache_size = 100;
    
    // 调整线程池配置
    config.thread_pool.io_bound_threads = 4;
    config.thread_pool.cpu_bound_threads = 8;
    config.thread_pool.memory_bound_threads = 4;
    
    // 调整性能监控配置
    config.performance.enabled = true;
    config.performance.prometheus_port = 9090;
    
    println!("✓ 配置流水线参数");
    
    // 创建流水线
    let mut pipeline = Pipeline::new(config).await?;
    println!("✓ 初始化流水线");
    
    // 开始处理
    println!("\n--- 开始流水线处理 ---");
    let start_time = std::time::Instant::now();
    
    // 提交文件处理任务
    pipeline.process_files(file_paths).await?;
    
    // 启动处理
    pipeline.start_processing().await?;
    
    let total_time = start_time.elapsed();
    println!("✓ 流水线处理完成，总耗时: {:?}", total_time);
    
    // 打印统计信息
    pipeline.print_stats().await;
    
    // 检查输出文件
    let output_files: Vec<_> = fs::read_dir(&output_dir)?
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().extension().map_or(false, |ext| ext == "json"))
        .collect();
    
    println!("\n--- 输出文件统计 ---");
    println!("输出文件数量: {}", output_files.len());
    
    // 显示前几个输出文件的内容
    for (i, entry) in output_files.iter().take(3).enumerate() {
        let content = fs::read_to_string(entry.path())?;
        let json: serde_json::Value = serde_json::from_str(&content)?;
        
        println!("输出文件 {}: {}", i + 1, entry.file_name().to_string_lossy());
        println!("  任务ID: {}", json["task_id"]);
        println!("  处理时间: {} ms", json["processing_time"]);
        println!("  解析结果数量: {}", json["parse_results"].as_array().unwrap().len());
    }
    
    // 性能分析
    if output_files.len() > 0 {
        let total_size: usize = fs::read_dir(&input_dir)?
            .filter_map(|entry| entry.ok())
            .map(|entry| fs::metadata(entry.path()).map(|m| m.len() as usize).unwrap_or(0))
            .sum();
        
        let throughput = total_size as f64 / 1024.0 / 1024.0 / total_time.as_secs_f64();
        println!("\n--- 性能分析 ---");
        println!("总数据量: {:.2} MB", total_size as f64 / 1024.0 / 1024.0);
        println!("处理速度: {:.2} MB/s", throughput);
        println!("平均文件处理时间: {:?}", total_time / file_count as u32);
    }
    
    // 清理临时目录
    temp_dir.close()?;
    println!("✓ 清理临时目录");
    
    println!("\n=== 流水线示例完成 ===");
    Ok(())
} 