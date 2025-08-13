use anyhow::Result;
use canp::{
    DataProcessingPipeline, PipelineConfig,
    TestDataGenerator, TestDataConfig,
    DbcManager, DbcManagerConfig,
    ColumnarStorageWriter, ColumnarStorageConfig,
    DataLayerParser, ParsedFileData,
    HighPerformanceExecutor, ExecutorConfig,
    ZeroCopyMemoryPool, MemoryPoolConfig
};
use std::path::PathBuf;
use std::time::Instant;
use std::sync::Arc;
use tracing::{info, warn, error};
use tracing_subscriber;

#[tokio::main]
async fn main() -> Result<()> {
    // 初始化日志系统
    tracing_subscriber::fmt()
        .with_env_filter("info")
        .init();

    info!("🚀 启动CAN数据处理管道");

    // 配置参数
    let input_dir = PathBuf::from("test_data");
    let output_dir = PathBuf::from("output");
    let dbc_file = PathBuf::from("can-dbc/example.dbc");

    // 创建输出目录
    std::fs::create_dir_all(&output_dir)?;

    // 1. 初始化内存池 - 零拷贝文件映射
    info!("📦 初始化零拷贝内存池");
    let memory_pool_config = MemoryPoolConfig {
        decompress_buffer_sizes: vec![
            16 * 1024,   // 16KB - 小压缩块
            64 * 1024,   // 64KB - 中等压缩块  
            256 * 1024,  // 256KB - 大压缩块
            1024 * 1024, // 1MB - 超大压缩块
        ],
        mmap_cache_size: 1000, // 缓存1000个文件映射
        max_memory_usage: 2 * 1024 * 1024 * 1024, // 2GB内存限制
        enable_mmap_cache: false,
        prewarm_per_tier: 0,
    };
    let memory_pool = Arc::new(ZeroCopyMemoryPool::new(memory_pool_config.clone()));

    // 2. 初始化高性能执行器
    info!("⚡ 初始化高性能执行器");
    let executor_config = ExecutorConfig {
        io_worker_threads: 8,           // 8个IO工作线程
        cpu_worker_threads: num_cpus::get(), // CPU核心数
        max_queue_length: 10000,   // 最大队列大小
        task_timeout: std::time::Duration::from_secs(300), // 5分钟超时
        stats_update_interval: std::time::Duration::from_secs(10),
        enable_work_stealing: true,
        cpu_batch_size: 100,
    };
    let executor = Arc::new(HighPerformanceExecutor::new(executor_config.clone()));

    // 3. 初始化DBC解析器
    info!("📋 初始化DBC解析器");
    let dbc_config = DbcManagerConfig {
        max_cached_files: 100,         // 缓存100个DBC文件
        cache_expire_seconds: 3600,    // 1小时过期
        auto_reload: true,
        reload_check_interval: 300,    // 5分钟检查间隔
        default_priority: 0,
        parallel_loading: true,
        max_load_threads: 4,
    };
    let dbc_manager = Arc::new(DbcManager::new(dbc_config.clone()));
    
    // 加载DBC文件
    if dbc_file.exists() {
        dbc_manager.load_dbc_file(&dbc_file, Some(0)).await?;
        info!("✅ 成功加载DBC文件: {:?}", dbc_file);
    } else {
        warn!("⚠️  DBC文件不存在，将使用默认解析: {:?}", dbc_file);
    }

    // 4. 初始化列式存储写入器
    info!("💾 初始化列式存储写入器");
    let storage_config = ColumnarStorageConfig {
        output_dir: output_dir.clone(),
        compression: canp::columnar_storage::CompressionType::Snappy,
        row_group_size: 10000,       // 每批10000条记录
        page_size: 1024 * 1024,      // 1MB页面
        enable_dictionary: true,
        enable_statistics: true,
        partition_strategy: canp::columnar_storage::PartitionStrategy::ByCanId,
        batch_size: 1000,
        max_file_size: 100 * 1024 * 1024, // 100MB
        keep_raw_data: false,
    };
    let storage_writer = Arc::new(ColumnarStorageWriter::new(storage_config.clone())?);

    // 5. 初始化数据处理管道
    info!("🔧 初始化数据处理管道");
    let pipeline_config = PipelineConfig {
        memory_pool_config: memory_pool_config.clone(),
        executor_config: executor_config.clone(),
        dbc_manager_config: dbc_config.clone(),
        storage_config: storage_config.clone(),
        batch_size: 100,             // 每批100个文件
        max_concurrent_files: 50,    // 50个并发文件处理
        enable_error_recovery: true,
        max_retries: 3,
        processing_timeout_seconds: 300, // 5分钟超时
        memory_pressure_threshold: 0.8, // 80%内存压力阈值
        enable_progress_reporting: true,
        progress_report_interval: 30, // 30秒报告间隔
    };

    let pipeline = DataProcessingPipeline::new(pipeline_config.clone()).await?;

    // 6. 检查输入数据
    let start_time = Instant::now();
    
    if !input_dir.exists() {
        info!("📝 输入目录不存在，生成测试数据");
        let test_config = TestDataConfig {
            output_dir: input_dir.clone(),
            file_count: 20,           // 生成20个测试文件
            target_file_size: 15 * 1024 * 1024, // 15MB目标文件大小
            frames_per_file: 2000,    // 每个文件2000帧
        };
        
        let generator = TestDataGenerator::new(test_config);
        generator.generate_all().await?;
        info!("✅ 测试数据生成完成");
    }

    // 7. 执行数据处理管道
    info!("🔄 开始执行数据处理管道");
    info!("📊 处理配置:");
    info!("   - 输入目录: {:?}", input_dir);
    info!("   - 输出目录: {:?}", output_dir);
    info!("   - 最大并发文件: {}", pipeline_config.max_concurrent_files);
    info!("   - 批处理大小: {}", pipeline_config.batch_size);
    info!("   - 内存压力阈值: {}%", (pipeline_config.memory_pressure_threshold * 100.0) as u32);

    // 获取输入文件列表
    let mut file_paths = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&input_dir) {
        for entry in entries {
            if let Ok(entry) = entry {
                let path = entry.path();
                if path.is_file() && path.extension().map_or(false, |ext| ext == "bin") {
                    file_paths.push(path);
                }
            }
        }
    }
    
    info!("📁 找到 {} 个文件待处理", file_paths.len());

    match pipeline.process_files(file_paths).await {
        Ok(batch_results) => {
            let total_time = start_time.elapsed();
            let stats = pipeline.get_stats().await;
            
            info!("🎉 数据处理完成!");
            info!("📈 处理统计:");
            info!("   - 总处理时间: {:.2}秒", total_time.as_secs_f64());
            info!("   - 成功文件: {}", stats.successful_files);
            info!("   - 失败文件: {}", stats.failed_files);
            info!("   - 跳过文件: {}", stats.skipped_files);
            info!("   - 重试文件: {}", stats.retried_files);
            info!("   - 平均文件处理时间: {:.2}ms", stats.avg_file_processing_time_ms);
            info!("   - 总吞吐量: {:.2} MB/s", 
                (stats.successful_files as f64 * 15.0) / total_time.as_secs_f64());

            // 显示子模块统计
            info!("🔍 子模块统计:");
            info!("   - 数据解析统计: {:?}", stats.data_parsing_stats);
            info!("   - DBC解析统计: {:?}", stats.dbc_parsing_stats);
            info!("   - 存储统计: {:?}", stats.storage_stats);

            // 显示内存池统计
            let pool_stats = memory_pool.get_stats();
            info!("💾 内存池统计:");
            info!("   - 映射文件数: {}", pool_stats.mapped_files);
            info!("   - 解压缓冲区数: {}", pool_stats.decompress_buffers);
            info!("   - 总内存使用: {:.2} MB", pool_stats.total_memory_usage_mb);

            // 显示执行器统计
            let executor_stats = executor.get_stats();
            info!("⚡ 执行器统计:");
            info!("   - 总任务数: {}", executor_stats.total_tasks);
            info!("   - 完成任务数: {}", executor_stats.completed_tasks);
            info!("   - IO任务数: {}", executor_stats.io_tasks);
            info!("   - CPU任务数: {}", executor_stats.cpu_tasks);
            info!("   - 平均执行时间: {:.2}ms", executor_stats.average_execution_time.as_millis());

        },
        Err(e) => {
            error!("❌ 数据处理失败: {}", e);
            return Err(e);
        }
    }

    info!("🏁 程序执行完成");
    Ok(())
}

// 辅助函数：显示系统信息
fn display_system_info() {
    info!("🖥️  系统信息:");
    info!("   - CPU核心数: {}", num_cpus::get());
    info!("   - 可用内存: {:.2} GB", 
        sysinfo::System::new_all().total_memory() as f64 / 1024.0 / 1024.0 / 1024.0);
}

// 辅助函数：验证输出结果
async fn verify_output(output_dir: &PathBuf) -> Result<()> {
    info!("🔍 验证输出结果");
    
    if !output_dir.exists() {
        warn!("⚠️  输出目录不存在");
        return Ok(());
    }

    let entries = std::fs::read_dir(output_dir)?;
    let mut file_count = 0;
    let mut total_size = 0;

    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() && path.extension().map_or(false, |ext| ext == "parquet") {
            file_count += 1;
            total_size += entry.metadata()?.len();
        }
    }

    info!("📊 输出验证结果:");
    info!("   - Parquet文件数: {}", file_count);
    info!("   - 总输出大小: {:.2} MB", total_size as f64 / 1024.0 / 1024.0);

    Ok(())
}
