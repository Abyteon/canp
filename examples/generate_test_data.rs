//! # 测试数据生成示例
//! 
//! 生成符合项目需求的4层数据结构测试文件

use canp::test_data_generator::{TestDataGenerator, TestDataConfig, TestDataValidator};
use anyhow::Result;
use std::path::PathBuf;
use tracing_subscriber;

#[tokio::main]
async fn main() -> Result<()> {
    // 初始化日志
    tracing_subscriber::fmt::init();
    
    println!("🚀 开始生成测试数据...");
    
    // 配置测试数据生成
    let config = TestDataConfig {
        file_count: 20,  // 生成20个测试文件
        target_file_size: 15 * 1024 * 1024,  // 15MB
        frames_per_file: 2000,  // 每个文件2000帧
        output_dir: "test_data".into(),
    };
    
    println!("📊 配置信息:");
    println!("  📁 输出目录: {:?}", config.output_dir);
    println!("  📄 文件数量: {}", config.file_count);
    println!("  📏 单文件大小: {} MB", config.target_file_size / 1024 / 1024);
    println!("  🎲 每文件帧数: {}", config.frames_per_file);
    
    // 创建生成器并生成数据
    let generator = TestDataGenerator::new(config);
    let file_paths = generator.generate_all().await?;
    
    println!("\n✅ 数据生成完成！");
    println!("📁 生成的文件:");
    for (i, path) in file_paths.iter().enumerate() {
        if i < 5 || i >= file_paths.len() - 2 {
            println!("  {} - {:?}", i + 1, path);
        } else if i == 5 {
            println!("  ... (显示前5个和后2个文件)");
        }
    }
    
    // 验证生成的数据
    println!("\n🔍 验证测试数据...");
    let stats = TestDataValidator::analyze_test_data(&PathBuf::from("test_data"))?;
    stats.print_summary();
    
    if stats.invalid_files == 0 {
        println!("\n🎉 所有测试文件生成成功并通过验证！");
        println!("💡 现在可以运行示例程序:");
        println!("   cargo run --example task_processing_example");
    } else {
        println!("\n⚠️ 有 {} 个文件验证失败，请检查生成过程", stats.invalid_files);
    }
    
    Ok(())
}