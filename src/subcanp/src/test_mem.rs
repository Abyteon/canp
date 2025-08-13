use anyhow::Result;
use bytes::BufMut;
use canp::{
    MemoryPoolConfig,
    ZeroCopyMemoryPool,
    // 若需要直接使用文件窗口类型：
    // MemoryMappedBlock,
};
use flate2::{Compression, write::GzEncoder};
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::PathBuf;
use tokio::time::Duration;

#[tokio::main]
async fn main() -> Result<()> {
    // 1) 构建内存池（多层缓冲 + 预热；一次性只读场景通常关闭mmap缓存）
    let pool = ZeroCopyMemoryPool::new(MemoryPoolConfig {
        decompress_buffer_sizes: vec![16 * 1024, 64 * 1024, 256 * 1024, 1024 * 1024],
        mmap_cache_size: 128,
        max_memory_usage: 2 * 1024 * 1024 * 1024,
        enable_mmap_cache: false,
        prewarm_per_tier: 2,
    });

    // 2) 准备一个测试文件：35字节头 + 压缩数据（末4字节为压缩长度，大端）
    let tmp_path = PathBuf::from("memory_pool_example.bin");
    write_example_file(&tmp_path, b"hello zero-copy memory pool!")?;

    // 3) 映射文件（零拷贝读取），并展示窗口/切片
    let mapping = pool.create_file_mapping(&tmp_path)?;
    let whole = mapping.as_slice();
    println!(
        "mapped bytes = {}, file_path = {}",
        whole.len(),
        mapping.file_path()
    );

    // 取前35字节作为头（例子中我们明确知道格式）
    let header = &whole[0..35];
    // 末4字节为压缩体长度（大端）
    let comp_len = u32::from_be_bytes([header[31], header[32], header[33], header[34]]) as usize;

    // 子窗口：指向压缩体，零拷贝
    let compressed_block = mapping.slice_block(35, comp_len);
    let (ptr, len) = compressed_block.as_ptr_and_len();
    println!("compressed ptr={:?}, len={}", ptr, len);

    // 也可以用切片：mapping.slice(offset, len)
    let compressed_slice = compressed_block.as_slice();
    assert_eq!(compressed_slice.len(), comp_len);

    // 4) 向内存池借一个解压缓冲（等待式借出），流式解压到该缓冲
    let mut out_buf = pool.get_decompress_buffer(comp_len * 4).await;

    // 流式解压：写入 out_buf（put_slice 不会拷贝已有数据，多次写拼接）
    {
        use flate2::read::GzDecoder;
        let mut dec = GzDecoder::new(std::io::Cursor::new(compressed_slice));
        let mut tmp = [0u8; 64 * 1024];
        loop {
            let n = dec.read(&mut tmp)?;
            if n == 0 {
                break;
            }
            out_buf.put_slice(&tmp[..n]);
        }
    }
    println!("decompressed size = {}", out_buf.len());

    // 5) 冻结为只读视图（零拷贝）。Guard 分支会在 Drop 时自动归还池
    let view = out_buf.freeze();
    let (vptr, vlen) = view.as_ptr_and_len();
    println!("view ptr={:?}, len={}", vptr, vlen);

    // 可做范围只读（不复制）：as_slice_range
    let head_8 = view.as_slice_range(0..view.len().min(8));
    println!("head_8 = {:?}", head_8);

    // 6) 批量借出/归还（演示批接口与复用）
    let sizes = vec![8 * 1024, 64 * 1024, 256 * 1024];
    let mut bufs = pool.get_decompress_buffers_batch(&sizes).await;
    for (i, b) in bufs.iter_mut().enumerate() {
        b.put_slice(format!("buffer-{}", i).as_bytes());
        // 显式回收（Guard分支Drop也会自动回收，这里只是演示）
        pool.recycle_decompress_buffer(std::mem::replace(
            b,
            canp::MutableMemoryBuffer::with_capacity(0),
        ))
        .await;
    }

    // 7) 统计与维护
    let stats = pool.get_stats();
    println!(
        "stats: mapped_files={}, tiers={}, mem_usage={:.2}MB",
        stats.mapped_files, stats.decompress_buffers, stats.total_memory_usage_mb
    );

    // 可选：清理过期映射（开启 mmap 缓存时更有意义）
    pool.cleanup_expired_mappings();

    // 稍等片刻方便观察（可无）
    tokio::time::sleep(Duration::from_millis(100)).await;

    // 文件结束后自动Drop：mmap 窗口与只读视图/缓冲会自行回收
    let _ = std::fs::remove_file(&tmp_path);
    Ok(())
}

// 写一个包含“35字节头 + 压缩体”的示例文件
fn write_example_file(path: &PathBuf, payload: &[u8]) -> Result<()> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(payload)?;
    let compressed = encoder.finish()?;

    let mut header = Vec::with_capacity(35);
    // 前18字节随意放（任务里用于序列号，这里放演示字节）
    header.extend_from_slice(b"SERIAL-000000000");
    // 填满至31（剩余字节可置零）
    while header.len() < 31 {
        header.push(0);
    }
    // 末4字节写压缩长度（大端）
    header.extend_from_slice(&(compressed.len() as u32).to_be_bytes());
    assert_eq!(header.len(), 35);

    let f = File::create(path)?;
    let mut w = BufWriter::new(f);
    w.write_all(&header)?;
    w.write_all(&compressed)?;
    w.flush()?;
    Ok(())
}
