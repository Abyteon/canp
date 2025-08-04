//! # 完整数据处理管道示例
//! 
//! 展示整个系统的端到端工作流程：
//! 1. 生成测试数据
//! 2. 创建DBC文件
//! 3. 运行完整处理管道
//! 4. 输出结果分析

use canp::processing_pipeline::{DataProcessingPipeline, PipelineConfig};
use canp::test_data_generator::{TestDataGenerator, TestDataConfig};
use canp::columnar_storage::{ColumnarStorageConfig, CompressionType, PartitionStrategy};
use anyhow::Result;
use std::path::PathBuf;
use tracing_subscriber;

#[tokio::main]
async fn main() -> Result<()> {
    // 初始化日志系统
    tracing_subscriber::fmt()
        .with_env_filter("info,canp=debug")
        .init();
    
    println!("🚀 欢迎使用CAN数据处理管道完整示例！");
    println!("{}", "=".repeat(60));
    
    // 第1步：准备工作环境
    println!("📋 第1步：准备工作环境");
    let workspace = prepare_workspace().await?;
    
    // 第2步：生成测试数据
    println!("\n📋 第2步：生成测试数据");
    let test_files = generate_test_data(&workspace).await?;
    
    // 第3步：创建DBC文件
    println!("\n📋 第3步：创建DBC文件");
    let dbc_files = create_dbc_files(&workspace).await?;
    
    // 第4步：配置和创建处理管道
    println!("\n📋 第4步：配置处理管道");
    let pipeline = create_processing_pipeline(&workspace).await?;
    
    // 第5步：加载DBC文件
    println!("\n📋 第5步：加载DBC文件");
    pipeline.load_dbc_files(dbc_files).await?;
    
    // 第6步：运行完整处理管道
    println!("\n📋 第6步：运行数据处理管道");
    let results = pipeline.process_files(test_files).await?;
    
    // 第7步：分析处理结果
    println!("\n📋 第7步：分析处理结果");
    analyze_results(&pipeline, &results, &workspace).await?;
    
    println!("\n🎉 完整数据处理管道示例执行完成！");
    println!("💡 您可以在以下目录查看输出结果:");
    println!("   📁 列式存储文件: {}", workspace.join("output").display());
    println!("   📊 处理日志: 控制台输出");
    
    Ok(())
}

/// 工作环境配置
struct Workspace {
    base_dir: PathBuf,
    test_data_dir: PathBuf,
    dbc_dir: PathBuf,
    output_dir: PathBuf,
}

impl Workspace {
    fn new(base_dir: PathBuf) -> Self {
        Self {
            test_data_dir: base_dir.join("test_data"),
            dbc_dir: base_dir.join("dbc_files"),
            output_dir: base_dir.join("output"),
            base_dir,
        }
    }
    
    fn join(&self, path: &str) -> PathBuf {
        self.base_dir.join(path)
    }
}

/// 准备工作环境
async fn prepare_workspace() -> Result<Workspace> {
    let base_dir = std::env::current_dir()?.join("pipeline_demo_workspace");
    let workspace = Workspace::new(base_dir);
    
    // 创建必要的目录
    for dir in [&workspace.test_data_dir, &workspace.dbc_dir, &workspace.output_dir] {
        tokio::fs::create_dir_all(dir).await?;
        println!("  ✅ 创建目录: {}", dir.display());
    }
    
    println!("🎯 工作环境准备完成: {}", workspace.base_dir.display());
    Ok(workspace)
}

/// 生成测试数据
async fn generate_test_data(workspace: &Workspace) -> Result<Vec<PathBuf>> {
    println!("  🔧 配置测试数据生成器...");
    
    let config = TestDataConfig {
        file_count: 100,  // 生成100个测试文件（模拟实际的8000个）
        target_file_size: 15 * 1024 * 1024,  // 15MB
        frames_per_file: 2000,  // 每个文件2000帧
        output_dir: workspace.test_data_dir.clone(),
    };
    
    println!("  📊 测试数据配置:");
    println!("    📄 文件数量: {}", config.file_count);
    println!("    📏 单文件大小: {} MB", config.target_file_size / 1024 / 1024);
    println!("    🎲 每文件帧数: {}", config.frames_per_file);
    
    let generator = TestDataGenerator::new(config);
    let file_paths = generator.generate_all().await?;
    
    println!("  ✅ 测试数据生成完成: {} 个文件", file_paths.len());
    
    // 验证生成的文件
    let mut total_size = 0u64;
    for path in &file_paths {
        if let Ok(metadata) = tokio::fs::metadata(path).await {
            total_size += metadata.len();
        }
    }
    
    println!("  📊 总数据量: {:.2} GB", total_size as f64 / 1024.0 / 1024.0 / 1024.0);
    
    Ok(file_paths)
}

/// 创建DBC文件
async fn create_dbc_files(workspace: &Workspace) -> Result<Vec<PathBuf>> {
    println!("  🔧 创建示例DBC文件...");
    
    // 创建多个DBC文件来演示多DBC场景
    let dbc_configs = vec![
        ("engine.dbc", create_engine_dbc_content()),
        ("chassis.dbc", create_chassis_dbc_content()),
        ("body.dbc", create_body_dbc_content()),
    ];
    
    let mut dbc_files = Vec::new();
    
    for (filename, content) in dbc_configs {
        let dbc_path = workspace.dbc_dir.join(filename);
        tokio::fs::write(&dbc_path, content).await?;
        dbc_files.push(dbc_path.clone());
        println!("  ✅ 创建DBC文件: {}", dbc_path.display());
    }
    
    println!("  📊 DBC文件创建完成: {} 个文件", dbc_files.len());
    Ok(dbc_files)
}

/// 创建引擎相关的DBC内容
fn create_engine_dbc_content() -> String {
    r#"
VERSION ""

NS_ : 
	NS_DESC_
	CM_
	BA_DEF_
	BA_
	VAL_
	CAT_DEF_
	CAT_
	FILTER
	BA_DEF_DEF_
	EV_DATA_
	ENVVAR_DATA_
	SGTYPE_
	SGTYPE_VAL_
	BA_DEF_SGTYPE_
	SIG_VALTYPE_
	SIGTYPE_VALTYPE_
	BO_TX_BU_
	BA_DEF_REL_
	BA_REL_
	BA_DEF_DEF_REL_
	BU_SG_REL_
	BU_EV_REL_
	BU_BO_REL_
	SG_MUL_VAL_

BS_:

BU_: ECU_Engine ECU_Transmission

BO_ 256 Engine_RPM: 8 ECU_Engine
 SG_ Engine_Speed : 0|16@1+ (0.25,0) [0|16383.75] "rpm" ECU_Transmission
 SG_ Engine_Load : 16|8@1+ (0.4,0) [0|102] "%" ECU_Transmission
 SG_ Coolant_Temp : 24|8@1+ (1,-40) [-40|215] "°C" ECU_Transmission
 SG_ Fuel_Level : 32|8@1+ (0.4,0) [0|102] "%" ECU_Transmission

BO_ 512 Engine_Status: 8 ECU_Engine
 SG_ Engine_Running : 0|1@1+ (1,0) [0|1] "" ECU_Transmission
 SG_ Check_Engine : 1|1@1+ (1,0) [0|1] "" ECU_Transmission
 SG_ Oil_Pressure : 8|16@1+ (0.1,0) [0|6553.5] "kPa" ECU_Transmission

CM_ SG_ 256 Engine_Speed "Engine rotational speed";
CM_ SG_ 256 Engine_Load "Engine load percentage";
CM_ SG_ 512 Engine_Running "Engine running status";

VAL_ 512 Engine_Running 0 "Stopped" 1 "Running";
VAL_ 512 Check_Engine 0 "OK" 1 "Warning";
"#.to_string()
}

/// 创建底盘相关的DBC内容
fn create_chassis_dbc_content() -> String {
    r#"
VERSION ""

NS_ : 
	NS_DESC_
	CM_
	BA_DEF_
	BA_
	VAL_
	CAT_DEF_
	CAT_
	FILTER
	BA_DEF_DEF_
	EV_DATA_
	ENVVAR_DATA_
	SGTYPE_
	SGTYPE_VAL_
	BA_DEF_SGTYPE_
	SIG_VALTYPE_
	SIGTYPE_VALTYPE_
	BO_TX_BU_
	BA_DEF_REL_
	BA_REL_
	BA_DEF_DEF_REL_
	BU_SG_REL_
	BU_EV_REL_
	BU_BO_REL_
	SG_MUL_VAL_

BS_:

BU_: ECU_ABS ECU_ESP

BO_ 768 Vehicle_Speed: 8 ECU_ABS
 SG_ Vehicle_Speed : 0|16@1+ (0.0625,0) [0|4095.9375] "km/h" ECU_ESP
 SG_ Wheel_Speed_FL : 16|16@1+ (0.0625,0) [0|4095.9375] "km/h" ECU_ESP
 SG_ Wheel_Speed_FR : 32|16@1+ (0.0625,0) [0|4095.9375] "km/h" ECU_ESP
 SG_ Wheel_Speed_RL : 48|16@1+ (0.0625,0) [0|4095.9375] "km/h" ECU_ESP

BO_ 1024 Brake_Status: 8 ECU_ABS
 SG_ Brake_Pedal_Pos : 0|8@1+ (0.4,0) [0|102] "%" ECU_ESP
 SG_ ABS_Active : 8|1@1+ (1,0) [0|1] "" ECU_ESP
 SG_ ESP_Active : 9|1@1+ (1,0) [0|1] "" ECU_ESP

CM_ SG_ 768 Vehicle_Speed "Vehicle speed from ABS sensor";
CM_ SG_ 1024 ABS_Active "ABS system activation status";

VAL_ 1024 ABS_Active 0 "Inactive" 1 "Active";
VAL_ 1024 ESP_Active 0 "Inactive" 1 "Active";
"#.to_string()
}

/// 创建车身相关的DBC内容
fn create_body_dbc_content() -> String {
    r#"
VERSION ""

NS_ : 
	NS_DESC_
	CM_
	BA_DEF_
	BA_
	VAL_
	CAT_DEF_
	CAT_
	FILTER
	BA_DEF_DEF_
	EV_DATA_
	ENVVAR_DATA_
	SGTYPE_
	SGTYPE_VAL_
	BA_DEF_SGTYPE_
	SIG_VALTYPE_
	SIGTYPE_VALTYPE_
	BO_TX_BU_
	BA_DEF_REL_
	BA_REL_
	BA_DEF_DEF_REL_
	BU_SG_REL_
	BU_EV_REL_
	BU_BO_REL_
	SG_MUL_VAL_

BS_:

BU_: ECU_BCM ECU_Climate

BO_ 1280 Door_Status: 8 ECU_BCM
 SG_ Door_FL_Open : 0|1@1+ (1,0) [0|1] "" ECU_Climate
 SG_ Door_FR_Open : 1|1@1+ (1,0) [0|1] "" ECU_Climate
 SG_ Door_RL_Open : 2|1@1+ (1,0) [0|1] "" ECU_Climate
 SG_ Door_RR_Open : 3|1@1+ (1,0) [0|1] "" ECU_Climate
 SG_ Trunk_Open : 4|1@1+ (1,0) [0|1] "" ECU_Climate

BO_ 1536 Climate_Control: 8 ECU_Climate
 SG_ AC_Status : 0|1@1+ (1,0) [0|1] "" ECU_BCM
 SG_ Fan_Speed : 8|4@1+ (1,0) [0|15] "" ECU_BCM
 SG_ Target_Temp : 16|8@1+ (0.5,0) [0|127.5] "°C" ECU_BCM
 SG_ Ambient_Temp : 24|8@1+ (1,-40) [-40|215] "°C" ECU_BCM

CM_ SG_ 1280 Door_FL_Open "Front left door status";
CM_ SG_ 1536 AC_Status "Air conditioning status";

VAL_ 1280 Door_FL_Open 0 "Closed" 1 "Open";
VAL_ 1536 AC_Status 0 "Off" 1 "On";
"#.to_string()
}

/// 创建处理管道
async fn create_processing_pipeline(workspace: &Workspace) -> Result<DataProcessingPipeline> {
    println!("  🔧 配置处理管道参数...");
    
    let config = PipelineConfig {
        storage_config: ColumnarStorageConfig {
            output_dir: workspace.output_dir.clone(),
            compression: CompressionType::Zstd, // 使用Zstd压缩获得更好的压缩比
            partition_strategy: PartitionStrategy::Daily, // 按天分区
            batch_size: 1000, // 1000行一批
            max_file_size: 50 * 1024 * 1024, // 50MB一个文件
            keep_raw_data: false, // 不保留原始数据以节省空间
            ..ColumnarStorageConfig::default()
        },
        batch_size: 20, // 20个文件一批处理
        max_concurrent_files: num_cpus::get().min(8), // 限制并发数
        enable_error_recovery: true,
        max_retries: 2,
        enable_progress_reporting: true,
        progress_report_interval: 10, // 每10秒报告一次进度
        ..PipelineConfig::default()
    };
    
    println!("  📊 管道配置:");
    println!("    🔄 批处理大小: {} 文件/批", config.batch_size);
    println!("    🚀 最大并发数: {} 文件", config.max_concurrent_files);
    println!("    📦 存储压缩: {:?}", config.storage_config.compression);
    println!("    📁 分区策略: {:?}", config.storage_config.partition_strategy);
    println!("    🔄 错误重试: {} 次", config.max_retries);
    
    let pipeline = DataProcessingPipeline::new(config).await?;
    println!("  ✅ 处理管道创建完成");
    
    Ok(pipeline)
}

/// 分析处理结果
async fn analyze_results(
    pipeline: &DataProcessingPipeline, 
    batch_results: &[canp::processing_pipeline::BatchProcessingResult],
    workspace: &Workspace
) -> Result<()> {
    
    println!("  📊 分析批处理结果...");
    
    // 汇总批次统计
    let mut total_files = 0;
    let mut successful_files = 0;
    let mut failed_files = 0;
    let mut total_processing_time = 0u64;
    let mut total_throughput = 0.0;
    
    for (batch_index, batch_result) in batch_results.iter().enumerate() {
        println!("    📦 批次 {}: 成功 {}, 失败 {}, 吞吐量 {:.2} MB/s",
            batch_index + 1,
            batch_result.successful_files,
            batch_result.failed_files,
            batch_result.batch_throughput_mb_s
        );
        
        total_files += batch_result.successful_files + batch_result.failed_files;
        successful_files += batch_result.successful_files;
        failed_files += batch_result.failed_files;
        total_processing_time += batch_result.batch_time_ms;
        total_throughput += batch_result.batch_throughput_mb_s;
    }
    
    let avg_throughput = if batch_results.len() > 0 {
        total_throughput / batch_results.len() as f64
    } else {
        0.0
    };
    
    println!("\n  🎯 批处理汇总:");
    println!("    📄 总文件数: {}", total_files);
    println!("    ✅ 成功处理: {} ({:.1}%)", 
        successful_files, 
        if total_files > 0 { successful_files as f64 / total_files as f64 * 100.0 } else { 0.0 }
    );
    println!("    ❌ 处理失败: {} ({:.1}%)", 
        failed_files,
        if total_files > 0 { failed_files as f64 / total_files as f64 * 100.0 } else { 0.0 }
    );
    println!("    ⏱️ 总处理时间: {:.2} 分钟", total_processing_time as f64 / 60000.0);
    println!("    🚀 平均吞吐量: {:.2} MB/s", avg_throughput);
    
    // 获取详细管道统计
    println!("\n  📈 获取详细统计信息...");
    let stats = pipeline.get_stats().await;
    stats.print_detailed_summary();
    
    // 检查输出文件
    println!("\n  📁 检查输出文件...");
    analyze_output_files(&workspace.output_dir).await?;
    
    // 性能建议
    println!("\n  💡 性能分析和建议:");
    provide_performance_recommendations(&stats, avg_throughput);
    
    Ok(())
}

/// 分析输出文件
async fn analyze_output_files(output_dir: &PathBuf) -> Result<()> {
    let mut output_files = Vec::new();
    let mut total_output_size = 0u64;
    
    // 递归遍历输出目录
    let mut stack = vec![output_dir.clone()];
    
    while let Some(current_dir) = stack.pop() {
        if let Ok(mut entries) = tokio::fs::read_dir(&current_dir).await {
            while let Ok(Some(entry)) = entries.next_entry().await {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                } else if path.extension().map_or(false, |ext| ext == "parquet") {
                    if let Ok(metadata) = entry.metadata().await {
                        output_files.push((path.clone(), metadata.len()));
                        total_output_size += metadata.len();
                    }
                }
            }
        }
    }
    
    println!("    📄 输出文件数: {}", output_files.len());
    println!("    💾 总输出大小: {:.2} MB", total_output_size as f64 / 1024.0 / 1024.0);
    
    if !output_files.is_empty() {
        let avg_file_size = total_output_size / output_files.len() as u64;
        println!("    📊 平均文件大小: {:.2} MB", avg_file_size as f64 / 1024.0 / 1024.0);
        
        // 显示前几个输出文件
        println!("    📁 输出文件示例:");
        for (i, (path, size)) in output_files.iter().take(5).enumerate() {
            println!("      {} - {} ({:.2} MB)", 
                i + 1, 
                path.display(), 
                *size as f64 / 1024.0 / 1024.0
            );
        }
        
        if output_files.len() > 5 {
            println!("      ... 还有 {} 个文件", output_files.len() - 5);
        }
    }
    
    // 检查元数据文件
    let metadata_path = output_dir.join("_metadata.json");
    if metadata_path.exists() {
        println!("    ✅ 元数据文件已生成: {}", metadata_path.display());
    }
    
    Ok(())
}

/// 提供性能建议
fn provide_performance_recommendations(stats: &canp::processing_pipeline::PipelineStats, avg_throughput: f64) {
    println!("    🎯 基于处理结果的优化建议:");
    
    // 成功率分析
    let success_rate = if stats.total_files > 0 {
        stats.successful_files as f64 / stats.total_files as f64
    } else {
        0.0
    };
    
    if success_rate < 0.95 {
        println!("      ⚠️ 文件处理成功率较低 ({:.1}%)，建议检查:", success_rate * 100.0);
        println!("         - DBC文件完整性");
        println!("         - 测试数据格式");
        println!("         - 错误重试机制");
    } else {
        println!("      ✅ 文件处理成功率良好 ({:.1}%)", success_rate * 100.0);
    }
    
    // 吞吐量分析
    if avg_throughput < 50.0 {
        println!("      ⚠️ 平均吞吐量较低 ({:.2} MB/s)，建议优化:", avg_throughput);
        println!("         - 增加并发处理数");
        println!("         - 调整批处理大小");
        println!("         - 使用更快的存储设备");
        println!("         - 优化DBC解析缓存");
    } else if avg_throughput < 100.0 {
        println!("      🔶 平均吞吐量中等 ({:.2} MB/s)，可以进一步优化:", avg_throughput);
        println!("         - 调优内存池配置");
        println!("         - 优化列式存储压缩设置");
    } else {
        println!("      🚀 平均吞吐量优秀 ({:.2} MB/s)", avg_throughput);
    }
    
    // 内存使用分析
    let memory_gb = stats.peak_memory_usage as f64 / 1024.0 / 1024.0 / 1024.0;
    if memory_gb > 8.0 {
        println!("      ⚠️ 峰值内存使用较高 ({:.2} GB)，建议:", memory_gb);
        println!("         - 减少批处理大小");
        println!("         - 启用更积极的垃圾回收");
        println!("         - 优化内存池配置");
    } else {
        println!("      ✅ 内存使用合理 ({:.2} GB)", memory_gb);
    }
    
    // DBC解析优化建议
    if stats.dbc_parsing_stats.cache_hit_rate < 0.8 {
        println!("      🔶 DBC缓存命中率较低 ({:.1}%)，建议:", 
            stats.dbc_parsing_stats.cache_hit_rate * 100.0);
        println!("         - 增加DBC缓存大小");
        println!("         - 检查CAN ID分布");
    }
    
    // 针对8000文件的实际场景建议
    println!("      💡 针对实际8000文件处理的建议:");
    println!("         - 批处理大小建议: 50-100 文件/批");
    println!("         - 推荐并发数: {}-{} (当前CPU核心数: {})", 
        num_cpus::get(), num_cpus::get() * 2, num_cpus::get());
    println!("         - 预计处理时间: {:.1}-{:.1} 小时", 
        8000.0 / stats.successful_files as f64 * stats.total_processing_time_ms as f64 / 3600000.0 * 0.8,
        8000.0 / stats.successful_files as f64 * stats.total_processing_time_ms as f64 / 3600000.0 * 1.2
    );
    println!("         - 建议存储空间: {:.1} GB", 
        8000.0 / stats.total_files as f64 * stats.storage_stats.compressed_size as f64 / 1024.0 / 1024.0 / 1024.0
    );
}