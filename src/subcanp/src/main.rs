use bytes::BufMut;
use flate2::{Compression, write::GzEncoder};
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::PathBuf;
use subcanp::high_performance_executor::{ExecutorConfig, HighPerformanceExecutor};
use subcanp::zero_copy_memory_pool::{
    // 若需要直接使用文件窗口类型：
    MemoryMappedBlock,
    MemoryPoolConfig,
    ZeroCopyMemoryPool,
};
// use tokio::time::Duration;

use anyhow::Result;
use std::sync::Arc;
use std::time::Instant;
use tracing::{error, info, warn};
use tracing_subscriber;

struct Block {
    vid: String,
    region: MemoryMappedBlock,
}

#[tokio::main]
async fn main() -> () {
    // 创建执行器配置
    let config = ExecutorConfig {
        io_worker_count: 8,     // 8个IO工作线程
        cpu_worker_count: 16,   // 16个CPU工作线程
        max_queue_length: 5000, // 背压控制
        bounded_queue: false,   // 使用无界队列
        cpu_batch_size: 50,     // CPU任务批量大小
        ..ExecutorConfig::default()
    };

    // 创建高性能执行器
    let executor = Arc::new(HighPerformanceExecutor::new(config));

    let pool = ZeroCopyMemoryPool::new(MemoryPoolConfig {
        decompress_buffer_sizes: vec![16 * 1024, 64 * 1024, 256 * 1024, 1024 * 1024],
        max_memory_usage: 2 * 1024 * 1024 * 1024,
        prewarm_per_tier: 2,
    });

    let tmp_path = PathBuf::from("./test_5mb_blocks.bin");
    let mapping = match pool.create_file_mapping(&tmp_path) {
        Ok(mapping) => mapping,
        Err(e) => {
            eprintln!("Failed to create file mapping: {}", e);
            return;
        }
    };
    println!(
        "Memory pool created with mapping: {:?} and mapping length is {}.",
        mapping,
        mapping.len()
    );

    let mut offset = 0;
    while offset + 35 <= mapping.len() {
        let header = &mapping.mmap[offset..offset + 35];
        let vid_bytes = &header[0..18];
        let vid = std::str::from_utf8(vid_bytes)
            .unwrap_or("Invalid UTF-8")
            .to_string();
        let len_bytes = header[31..35].try_into().unwrap();
        let comp_len = u32::from_be_bytes(len_bytes) as usize;
        let start_data = offset + 35;
        let end_data = start_data + comp_len;
        if end_data > mapping.len() {
            break;
        }
        let comp_data = &mapping.mmap[start_data..end_data];
        let block = mapping.slice_block(start_data, comp_len);
        offset = end_data;
        // println!(
        //     "Block: vid = {}, comp_len = {}, data = {:?}",
        //     vid,
        //     comp_len,
        //     &comp_data[0..10] // 仅打印前10字节
        // );
        println!("Block: vid = {}, region = {:?}", vid, block,);
    }
}
// let whole = mapping.as_slice();
// println!(
//     "mapped bytes = {}, file_path = {}",
//     whole.len(),
//     mapping.file_path()
// );

// 取前35字节作为头（例子中我们明确知道格式）
// while true {
//     let header = &whole[0..35];
//     // 末4字节为压缩体长度（大端）
//     let comp_len = u32::from_be_bytes([
//         header[offset + 31],
//         header[offset + 32],
//         header[offset + 33],
//         header[offset + 34],
//     ]) as usize;
// }
//
// /// 文件头部信息（第1层）
// #[derive(Debug, Clone)]
// pub struct Block {
//     /// 文件标识（8字节）
//     pub magic: [u8; 8],
//     /// 版本号
//     pub version: u32,
//     /// 文件索引
//     pub file_index: u32,
//     /// 时间戳
//     pub timestamp: u64,
//     /// CRC32校验
//     pub crc32: u32,
//     /// 压缩数据长度
//     pub compressed_length: u32,
//     /// 保留字节
//     pub reserved: [u8; 3],
// }
//
// impl FileHeader {
//     /// 从任务说明格式（35字节）解析：仅严格提取“前18字节序列号”和“后四字节长度”，其余字段按0填充
//     pub fn from_task_spec_bytes(data: &[u8]) -> Result<([u8; 18], Self)> {
//         let mut offset = 0;
//         while offset < data.len() {
//             offset += 1; // 跳过前导零
//         }
//         if data.len() < 35 {
//             return Err(anyhow::anyhow!(
//                 "文件头部数据不足：需要35字节，实际{}字节",
//                 data.len()
//             ));
//         }
//         let mut serial = [0u8; 18];
//         serial.copy_from_slice(&data[0..18]);
//         // 后四字节为压缩数据长度（大端）
//         let len_be = u32::from_be_bytes([data[31], data[32], data[33], data[34]]);
//         let header = FileHeader {
//             magic: [0u8; 8],
//             version: 0,
//             file_index: 0,
//             timestamp: 0,
//             crc32: 0,
//             compressed_length: len_be,
//             reserved: [0u8; 3],
//         };
//         Ok((serial, header))
//     }
//
//     /// 验证文件头部有效性
//     pub fn validate(&self) -> Result<()> {
//         Ok(())
//     }
// }
// 取前35字节作为头（例子中我们明确知道格式）

// println!("Hello, world!");
// println!("Memory pool created with config: {}", pool);
