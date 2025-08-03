use canp::layer_parser::{LayerParser, LayerParserConfig, BlockType};
use canp::memory_pool::{UnifiedMemoryPool, MemoryPoolConfig};
use canp::dbc_parser::{DbcParser, DbcParserConfig};
use std::sync::Arc;

/// 创建测试数据文件
fn create_test_file() -> std::path::PathBuf {
    use std::fs::File;
    use std::io::Write;
    
    let temp_dir = std::env::temp_dir();
    let test_file = temp_dir.join("layer_parser_test.bin");
    
    let mut file = File::create(&test_file).unwrap();
    
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
    let layer1_data1 = vec![1u8; 30];
    compressed_data.extend_from_slice(&layer1_data1);
    
    // 第1层数据块2：20字节头部 + 数据
    let mut layer1_block2 = vec![0u8; 20];
    layer1_block2[16] = 0; // 数据长度（大端序）：25字节
    layer1_block2[17] = 0;
    layer1_block2[18] = 0;
    layer1_block2[19] = 25;
    compressed_data.extend_from_slice(&layer1_block2);
    
    // 第1层数据块2的数据部分
    let layer1_data2 = vec![2u8; 25];
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
    
    test_file
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 初始化日志
    tracing_subscriber::fmt::init();
    
    println!("=== 分层数据解析器示例 ===");
    
    // 创建测试文件
    let test_file = create_test_file();
    println!("✓ 创建测试文件: {:?}", test_file);
    
    // 初始化内存池
    let pool_config = MemoryPoolConfig::default();
    let memory_pool = Arc::new(UnifiedMemoryPool::new(pool_config));
    println!("✓ 初始化内存池");
    
    // 初始化DBC解析器
    let dbc_config = DbcParserConfig::default();
    let dbc_parser = Arc::new(DbcParser::new(dbc_config));
    println!("✓ 初始化DBC解析器");
    
    // 初始化分层解析器
    let layer_config = LayerParserConfig::default();
    let layer_parser = LayerParser::new(layer_config)?
        .with_dbc_parser(dbc_parser.clone());
    println!("✓ 初始化分层解析器");
    
    // 第0层：解析文件头部和压缩数据
    println!("\n--- 第0层解析 ---");
    let mmap_block = memory_pool.create_file_mmap(&test_file).await?;
    let layer0_result = layer_parser.parse_layer_0(&mmap_block)?;
    
    println!("第0层解析结果:");
    println!("  层类型: {:?}", layer0_result.layer_type);
    println!("  数据块数量: {}", layer0_result.data_blocks.len());
    println!("  解析字节数: {}", layer0_result.stats.bytes_parsed);
    println!("  解析时间: {}微秒", layer0_result.stats.parse_time_us);
    
    // 提取压缩数据块
    let compressed_block = layer0_result.data_blocks
        .iter()
        .find(|block| matches!(block.block_type, BlockType::CompressedData))
        .ok_or_else(|| anyhow::anyhow!("未找到压缩数据块"))?;
    
    println!("  压缩数据: 指针={}, 长度={}", 
             compressed_block.ptr_and_len.0, compressed_block.ptr_and_len.1);
    
    // 第1层：解析压缩数据（这里假设已经解压）
    println!("\n--- 第1层解析 ---");
    let layer1_result = layer_parser.parse_layer_1(
        compressed_block.ptr_and_len.0, 
        compressed_block.ptr_and_len.1
    )?;
    
    println!("第1层解析结果:");
    println!("  层类型: {:?}", layer1_result.layer_type);
    println!("  数据块数量: {}", layer1_result.data_blocks.len());
    println!("  解析字节数: {}", layer1_result.stats.bytes_parsed);
    println!("  解析时间: {}微秒", layer1_result.stats.parse_time_us);
    
    // 显示第1层的数据块
    for (i, block) in layer1_result.data_blocks.iter().enumerate() {
        println!("  数据块{}: 类型={:?}, 指针={}, 长度={}", 
                 i, block.block_type, block.ptr_and_len.0, block.ptr_and_len.1);
    }
    
    // 第2层：解析第1层的未压缩数据块
    println!("\n--- 第2层解析 ---");
    let uncompressed_blocks: Vec<_> = layer1_result.data_blocks
        .iter()
        .filter(|block| matches!(block.block_type, BlockType::UncompressedData))
        .collect();
    
    let mut layer2_results = Vec::new();
    for block in &uncompressed_blocks {
        let result = layer_parser.parse_layer_2(block.ptr_and_len.0, block.ptr_and_len.1)?;
        layer2_results.push(result);
    }
    
    println!("第2层解析结果:");
    println!("  解析的数据块数量: {}", layer2_results.len());
    for (i, result) in layer2_results.iter().enumerate() {
        println!("  数据块{}: 层类型={:?}, 数据块数量={}, 解析字节数={}", 
                 i, result.layer_type, result.data_blocks.len(), result.stats.bytes_parsed);
    }
    
    // 最后一层：解析单帧数据
    println!("\n--- 最后一层解析 ---");
    let mut final_layer_results = Vec::new();
    
    for layer2_result in &layer2_results {
        for block in &layer2_result.data_blocks {
            if matches!(block.block_type, BlockType::UncompressedData) {
                let result = layer_parser.parse_final_layer(block.ptr_and_len.0, block.ptr_and_len.1)?;
                final_layer_results.push(result);
            }
        }
    }
    
    println!("最后一层解析结果:");
    println!("  解析的帧数量: {}", final_layer_results.len());
    for (i, result) in final_layer_results.iter().enumerate() {
        println!("  帧{}: 层类型={:?}, 数据块数量={}, 解析字节数={}", 
                 i, result.layer_type, result.data_blocks.len(), result.stats.bytes_parsed);
    }
    
    // 批量解析示例
    println!("\n--- 批量解析示例 ---");
    let batch_result = layer_parser.parse_layer_1_batch(&layer1_result.data_blocks)?;
    println!("批量解析第1层: {}个结果", batch_result.len());
    
    // 清理测试文件
    std::fs::remove_file(&test_file)?;
    println!("✓ 清理测试文件");
    
    println!("\n=== 分层解析器示例完成 ===");
    Ok(())
} 