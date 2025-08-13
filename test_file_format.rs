// 测试生成文件格式的Rust验证程序

use std::fs::File;
use std::io::{Read, BufReader};
use std::path::Path;

fn main() -> std::io::Result<()> {
    let filename = "test_5mb_blocks.bin";
    
    println!("🔍 使用Rust验证文件格式: {}", filename);
    
    let file = File::open(filename)?;
    let mut reader = BufReader::new(file);
    let mut block_count = 0;
    let mut total_data_size = 0u64;
    
    loop {
        // 读取35字节头部
        let mut header = [0u8; 35];
        match reader.read_exact(&mut header) {
            Ok(_) => {},
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                println!("📄 文件读取完成，到达文件末尾");
                break;
            },
            Err(e) => return Err(e),
        }
        
        // 解析序列号（前18字节）
        let serial_bytes = &header[0..18];
        let serial = String::from_utf8_lossy(serial_bytes).trim_end_matches('\0');
        
        // 解析数据长度（后4字节，大端序）
        let data_length = u32::from_be_bytes([
            header[31], header[32], header[33], header[34]
        ]) as usize;
        
        // 读取数据段
        let mut data_buffer = vec![0u8; data_length];
        reader.read_exact(&mut data_buffer)?;
        
        // 验证数据段开头
        let data_start = String::from_utf8_lossy(&data_buffer[0..std::cmp::min(20, data_length)]);
        
        if block_count < 10 {
            println!(
                "块 {}: 序列号='{}', 数据长度={}, 数据开头='{}'",
                block_count, serial, data_length, data_start
            );
        }
        
        total_data_size += data_length as u64;
        block_count += 1;
        
        // 每100个块显示进度
        if block_count % 100 == 0 {
            println!("已处理 {} 个块...", block_count);
        }
    }
    
    let total_file_size = block_count * 35 + total_data_size;
    
    println!("\n📊 文件统计信息:");
    println!("总块数: {}", block_count);
    println!("数据总大小: {:.2} MB", total_data_size as f64 / 1024.0 / 1024.0);
    println!("文件总大小: {:.2} MB ({} 字节)", total_file_size as f64 / 1024.0 / 1024.0, total_file_size);
    println!("平均数据块大小: {:.0} 字节", total_data_size as f64 / block_count as f64);
    
    // 验证实际文件大小
    let actual_size = std::fs::metadata(filename)?.len();
    println!("实际文件大小: {:.2} MB ({} 字节)", actual_size as f64 / 1024.0 / 1024.0, actual_size);
    
    if actual_size == total_file_size {
        println!("✅ 文件格式验证成功！");
    } else {
        println!("❌ 文件大小不匹配，可能有格式问题");
    }
    
    Ok(())
}
