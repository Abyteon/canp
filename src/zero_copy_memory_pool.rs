//! # 零拷贝内存池 (Zero-Copy Memory Pool)
//!
//! 专门为大规模数据处理任务设计的高性能零拷贝内存池。
//! 核心功能：文件内存映射管理和解压缓冲区分配。

use anyhow::Result;
use bytes::{BufMut, Bytes, BytesMut};
use lock_pool::{LockGuard, LockPool};
use memmap2::Mmap;
use std::fs::File;
use std::path::Path;
use std::sync::Arc;
use std::sync::RwLock;

use tracing::{info, warn};

// tokio异步运行时功能
use tokio::task;

/// 内存映射块
///
/// 使用Arc<Mmap>实现多线程安全的零拷贝文件访问
#[derive(Debug, Clone)]
pub struct MemoryMappedBlock {
    /// 内存映射数据
    mmap: Arc<Mmap>,
    /// 文件路径（用于调试）
    file_path: String,
    /// 逻辑视图的起始偏移
    offset: usize,
    /// 逻辑视图的长度
    length: usize,
}

impl MemoryMappedBlock {
    /// 创建文件映射（零拷贝）
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self> {
        let file = File::open(&path)?;
        let mmap = unsafe { Mmap::map(&file)? };

        Ok(Self {
            mmap: Arc::new(mmap),
            file_path: path.as_ref().to_string_lossy().to_string(),
            offset: 0,
            length: {
                let meta_len = File::open(&path)
                    .ok()
                    .and_then(|f| f.metadata().ok())
                    .map(|m| m.len() as usize)
                    .unwrap_or(0);
                meta_len
            },
        })
    }

    /// 零拷贝数据访问
    ///
    /// 直接返回映射内存的切片，无任何数据复制
    #[inline]
    pub fn as_slice(&self) -> &[u8] {
        &self.mmap[self.offset..self.offset + self.length]
    }

    /// 零拷贝指针访问
    ///
    /// 返回原始指针和长度，用于底层操作
    #[inline]
    pub fn as_ptr_and_len(&self) -> (*const u8, usize) {
        unsafe { (self.mmap.as_ptr().add(self.offset), self.length) }
    }

    /// 零拷贝子切片访问
    ///
    /// 基于偏移量和长度访问数据，无数据复制
    #[inline]
    pub fn slice(&self, offset: usize, len: usize) -> &[u8] {
        assert!(offset <= self.length, "slice offset out of bounds");
        assert!(offset + len <= self.length, "slice length out of bounds");
        let start = self.offset + offset;
        &self.mmap[start..start + len]
    }

    /// 零拷贝子块创建
    ///
    /// 创建指向同一内存区域的新块，无数据复制 基于memmap2官方文档的最佳实践
    #[inline]
    pub fn slice_block(&self, offset: usize, len: usize) -> MemoryMappedBlock {
        assert!(offset <= self.length, "slice_block offset out of bounds");
        assert!(
            offset + len <= self.length,
            "slice_block length out of bounds"
        );
        MemoryMappedBlock {
            mmap: Arc::clone(&self.mmap), // 零拷贝引用计数
            file_path: format!("{}[{}:{}]", self.file_path, offset, offset + len),
            offset: self.offset + offset,
            length: len,
        }
    }

    /// 零拷贝视图创建
    ///
    /// 创建指向同一内存区域的新视图，无数据复制
    /// 适用于需要多个不同偏移量访问同一文件的场景
    #[inline]
    pub fn view(&self) -> MemoryMappedBlock {
        MemoryMappedBlock {
            mmap: Arc::clone(&self.mmap), // 零拷贝引用计数
            file_path: self.file_path.clone(),
            offset: self.offset,
            length: self.length,
        }
    }

    /// 文件路径
    pub fn file_path(&self) -> &str {
        &self.file_path
    }

    /// 数据长度
    #[inline]
    pub fn len(&self) -> usize {
        self.length
    }

    /// 检查是否为空
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.length == 0
    }
}

/// 零拷贝缓冲区（不可变视图）
#[derive(Debug)]
pub enum ZeroCopyBuffer<'a> {
    /// 拥有的 Bytes（来自 Owned BytesMut 冻结）
    Bytes(Bytes),
    /// 持有 Guard 的只读视图（Drop 时归还池）
    Guard(LockGuard<'a, BytesMut, 64, 512>),
}

impl<'a> ZeroCopyBuffer<'a> {
    /// 从BytesMut创建（转移所有权，零拷贝）
    pub fn from_bytes_mut(buffer: BytesMut) -> Self {
        ZeroCopyBuffer::Bytes(buffer.freeze())
    }

    /// 从Vec创建（最后一次拷贝）
    pub fn from_vec(data: Vec<u8>) -> Self {
        ZeroCopyBuffer::Bytes(Bytes::from(data))
    }

    /// 零拷贝数据访问
    #[inline]
    pub fn as_slice(&self) -> &[u8] {
        match self {
            ZeroCopyBuffer::Bytes(b) => &b[..],
            ZeroCopyBuffer::Guard(g) => &g[..],
        }
    }

    /// 零拷贝指针访问
    #[inline]
    pub fn as_ptr_and_len(&self) -> (*const u8, usize) {
        match self {
            ZeroCopyBuffer::Bytes(b) => (b.as_ptr(), b.len()),
            ZeroCopyBuffer::Guard(g) => (g.as_ptr(), g.len()),
        }
    }

    /// 数据长度
    #[inline]
    pub fn len(&self) -> usize {
        self.as_slice().len()
    }

    /// 检查是否为空
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// 引用范围切片（零拷贝引用，不创建新缓冲）
    #[inline]
    pub fn as_slice_range(&self, range: std::ops::Range<usize>) -> &[u8] {
        &self.as_slice()[range]
    }
}

/// 可变内存缓冲区
///
/// 用于写入数据，支持高效的零拷贝转换
#[derive(Debug)]
pub enum BufferInner<'a> {
    /// 来自对象池的缓冲区，持有Guard，Drop时自动归还
    Guarded(LockGuard<'a, BytesMut, 64, 512>),
    /// 临时分配的缓冲区
    Owned(BytesMut),
}

#[derive(Debug)]
pub struct MutableMemoryBuffer<'a> {
    inner: BufferInner<'a>,
}

impl<'a> MutableMemoryBuffer<'a> {
    /// 创建指定容量的缓冲区
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            inner: BufferInner::Owned(BytesMut::with_capacity(capacity)),
        }
    }

    /// 写入数据
    #[inline]
    pub fn put_slice(&mut self, src: &[u8]) {
        match &mut self.inner {
            BufferInner::Guarded(g) => g.put_slice(src),
            BufferInner::Owned(b) => b.put_slice(src),
        }
    }

    /// 扩展容量
    #[inline]
    pub fn reserve(&mut self, additional: usize) {
        match &mut self.inner {
            BufferInner::Guarded(g) => g.reserve(additional),
            BufferInner::Owned(b) => b.reserve(additional),
        }
    }

    /// 清空缓冲区
    #[inline]
    pub fn clear(&mut self) {
        match &mut self.inner {
            BufferInner::Guarded(g) => g.clear(),
            BufferInner::Owned(b) => b.clear(),
        }
    }

    /// 冻结为不可变缓冲区（零拷贝）
    pub fn freeze(self) -> ZeroCopyBuffer<'a> {
        match self.inner {
            BufferInner::Guarded(g) => ZeroCopyBuffer::Guard(g),
            BufferInner::Owned(b) => ZeroCopyBuffer::Bytes(b.freeze()),
        }
    }

    /// 获取可变切片
    #[inline]
    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        match &mut self.inner {
            BufferInner::Guarded(g) => &mut g[..],
            BufferInner::Owned(b) => &mut b[..],
        }
    }

    /// 获取不可变切片
    #[inline]
    pub fn as_slice(&self) -> &[u8] {
        match &self.inner {
            BufferInner::Guarded(g) => &g[..],
            BufferInner::Owned(b) => &b[..],
        }
    }

    /// 当前长度
    #[inline]
    pub fn len(&self) -> usize {
        match &self.inner {
            BufferInner::Guarded(g) => g.len(),
            BufferInner::Owned(b) => b.len(),
        }
    }

    /// 剩余容量
    #[inline]
    pub fn remaining_mut(&self) -> usize {
        match &self.inner {
            BufferInner::Guarded(g) => g.remaining_mut(),
            BufferInner::Owned(b) => b.remaining_mut(),
        }
    }

    /// 检查是否为空
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// 容量（便于测试和监控）
    #[inline]
    pub fn capacity(&self) -> usize {
        match &self.inner {
            BufferInner::Guarded(g) => g.capacity(),
            BufferInner::Owned(b) => b.capacity(),
        }
    }
}

/// 内存池配置
#[derive(Debug, Clone)]
pub struct MemoryPoolConfig {
    /// 解压缓冲区的预设大小（基于您的数据特征）
    pub decompress_buffer_sizes: Vec<usize>,
    /// 最大内存使用量
    pub max_memory_usage: usize,
    /// 每层预热个数（可选），用于稳定初期吞吐
    pub prewarm_per_tier: usize,
}

impl Default for MemoryPoolConfig {
    fn default() -> Self {
        Self {
            // 基于~10KB压缩数据，解压后可能的大小
            decompress_buffer_sizes: vec![
                16 * 1024,   // 16KB - 小压缩块
                64 * 1024,   // 64KB - 中等压缩块
                256 * 1024,  // 256KB - 大压缩块
                1024 * 1024, // 1MB - 超大压缩块
            ],
            max_memory_usage: 2 * 1024 * 1024 * 1024, // 2GB内存限制
            prewarm_per_tier: 0,
        }
    }
}

/// 零拷贝内存池
///
/// 专门为数据处理任务设计的高性能内存池
pub struct ZeroCopyMemoryPool {
    /// 配置
    config: MemoryPoolConfig,
    /// 解压缓冲区池（分层管理）
    decompress_pools: Vec<Arc<LockPool<BytesMut, 64, 512>>>,
    /// 当前内存使用量
    current_memory_usage: Arc<RwLock<usize>>,
}

impl ZeroCopyMemoryPool {
    /// 创建新的零拷贝内存池
    pub fn new(config: MemoryPoolConfig) -> Self {
        info!("🚀 初始化零拷贝内存池");

        // 创建分层解压缓冲区池
        let decompress_pools: Vec<Arc<LockPool<BytesMut, 64, 512>>> = config
            .decompress_buffer_sizes
            .iter()
            .map(|&size| Arc::new(LockPool::from_fn(move |_| BytesMut::with_capacity(size))))
            .collect();

        let current_memory_usage = Arc::new(RwLock::new(0));

        // 预热：为每个 tier 尝试借出/归还一次，触发底层分配，降低首包抖动（同步构造阶段仅使用非阻塞try_get）
        if config.prewarm_per_tier > 0 {
            for pool in &decompress_pools {
                for _ in 0..config.prewarm_per_tier {
                    if let Some(mut g) = pool.try_get() {
                        g.clear();
                    }
                }
            }
        }

        info!(
            "✅ 零拷贝内存池初始化完成 - 缓冲区池: {} 层",
            decompress_pools.len()
        );

        Self {
            config,
            decompress_pools,
            current_memory_usage,
        }
    }

    /// 创建文件映射（零拷贝）
    ///
    /// ## 设计说明
    /// - 直接创建mmap，不涉及缓存（单次处理场景）
    /// - 统计内存使用量，支持监控和限制
    /// - 返回MemoryMappedBlock包装，提供安全的访问接口
    pub fn create_file_mapping<P: AsRef<Path>>(&self, path: P) -> Result<MemoryMappedBlock> {
        let path_str = path.as_ref().to_string_lossy().to_string();

        // 创建文件映射 - 基于memmap2最佳实践
        let file = File::open(&path)?;
        let mmap = unsafe { 
            // 使用memmap2的默认设置，它会自动处理大部分平台特定的优化
            Mmap::map(&file)?
        };
        let mmap_arc = Arc::new(mmap);

        // 更新内存使用量统计
        {
            let mut usage = self.current_memory_usage.write().unwrap();
            *usage += mmap_arc.len();

            if *usage > self.config.max_memory_usage {
                warn!(
                    "⚠️ 内存使用量超过限制: {:.2} MB (当前: {:.2} MB)", 
                    self.config.max_memory_usage as f64 / 1024.0 / 1024.0,
                    *usage as f64 / 1024.0 / 1024.0
                );
            }
        }

        let len = mmap_arc.len();
        Ok(MemoryMappedBlock {
            mmap: mmap_arc,
            file_path: path_str,
            offset: 0,
            length: len,
        })
    }

    /// 异步创建文件映射
    ///
    /// 将同步的mmap操作包装为异步，避免阻塞异步运行时
    /// 适用于需要在异步上下文中创建单个文件映射的场景
    pub async fn create_file_mapping_async<P: AsRef<Path>>(
        &self,
        path: P,
    ) -> Result<MemoryMappedBlock> {
        let path_buf = path.as_ref().to_path_buf();
        
        task::spawn_blocking(move || {
            MemoryMappedBlock::new(path_buf)
        })
        .await
        .map_err(|e| anyhow::anyhow!("异步文件映射任务失败: {}", e))?
    }

    /// 批量创建文件映射
    ///
    /// ## 设计原则
    /// - 基于tokio spawn_blocking处理同步I/O，避免阻塞异步运行时
    /// - 直接使用MemoryMappedBlock::new，简化调用链
    /// - 通过futures::future::try_join_all并发等待，提高性能
    /// 
    /// ## 参考文档
    /// - [tokio spawn_blocking](https://docs.rs/tokio/latest/tokio/task/fn.spawn_blocking.html)
    /// - [memmap2最佳实践](https://docs.rs/memmap2/latest/memmap2/)
    pub async fn create_file_mappings_batch<P: AsRef<Path>>(
        &self,
        paths: &[P],
    ) -> Vec<Result<MemoryMappedBlock>> {
        if paths.is_empty() {
            return Vec::new();
        }
        
        // 预分配结果容器
        let mut results = Vec::with_capacity(paths.len());
        
        // 将路径转换为拥有的PathBuf，避免生命周期问题
        let path_bufs: Vec<std::path::PathBuf> = paths
            .iter()
            .map(|p| p.as_ref().to_path_buf())
            .collect();

        // 创建并发任务，每个任务在blocking线程池中执行文件映射
        // 使用spawn_blocking是因为文件I/O和mmap是同步操作
        let handles: Vec<_> = path_bufs
            .into_iter()
            .map(|path| {
                task::spawn_blocking(move || {
                    // 直接调用MemoryMappedBlock::new，避免通过ZeroCopyMemoryPool包装
                    // 这样避免了Clone trait的需求和额外的间接调用
                    MemoryMappedBlock::new(path)
                })
            })
            .collect();

        // 等待所有任务完成并收集结果
        // 使用简单的for循环而不是try_join_all，因为我们需要收集所有结果（包括错误）
        for handle in handles {
            match handle.await {
                Ok(mapping_result) => results.push(mapping_result),
                Err(join_error) => {
                    // JoinError表示任务panic或被取消
                    results.push(Err(anyhow::anyhow!(
                        "异步任务执行失败: {}", 
                        join_error
                    )))
                }
            }
        }

        results
    }

    /// 获取内存缓冲区
    ///
    /// 从池中获取合适大小的缓冲区，支持零拷贝操作
    /// 基于bytes和lock_pool官方文档的最佳实践
    pub async fn get_decompress_buffer(&self, estimated_size: usize) -> MutableMemoryBuffer<'_> {
        // 选择合适的池
        let pool = self.select_decompress_pool(estimated_size);

        if let Some(pool) = pool {
            // 等待式获取，减少 Owned 回退，提升吞吐稳定性
            let mut guard = lock_pool::maybe_await!(pool.get());
            guard.clear();
            let cap = guard.capacity();
            if cap < estimated_size {
                guard.reserve(estimated_size - cap);
            }
            MutableMemoryBuffer {
                inner: BufferInner::Guarded(guard),
            }
        } else {
            // 直接创建新缓冲区（无池）
            MutableMemoryBuffer::with_capacity(estimated_size)
        }
    }

    /// 批量获取内存缓冲区
    pub async fn get_decompress_buffers_batch(&self, sizes: &[usize]) -> Vec<MutableMemoryBuffer> {
        let mut buffers = Vec::with_capacity(sizes.len());

        for &size in sizes {
            buffers.push(self.get_decompress_buffer(size).await);
        }

        buffers
    }

    /// 回收内存缓冲区
    ///
    /// 将使用完的缓冲区回收到池中
    /// 基于lock_pool官方文档的最佳实践
    pub async fn recycle_decompress_buffer<'a>(&self, mut buffer: MutableMemoryBuffer<'a>) {
        // Guard 模式下，Drop 即可自动归还；Owned 模式下，直接丢弃
        match &mut buffer.inner {
            BufferInner::Guarded(g) => {
                g.clear();
                // guard 在函数结束时被丢弃，自动归还到池
            }
            BufferInner::Owned(_b) => {
                // 直接丢弃即可
            }
        }
    }

    /// 选择合适的内存缓冲区池
    fn select_decompress_pool(&self, size: usize) -> Option<&Arc<LockPool<BytesMut, 64, 512>>> {
        for (i, &pool_size) in self.config.decompress_buffer_sizes.iter().enumerate() {
            if size <= pool_size {
                return self.decompress_pools.get(i);
            }
        }
        self.decompress_pools.last()
    }

    /// 获取内存使用统计
    pub fn get_memory_usage(&self) -> usize {
        *self.current_memory_usage.read().unwrap()
    }

    /// 兼容接口：无缓存实现，空操作
    pub fn cleanup_expired_mappings(&self) {}

    /// 获取内存池统计信息
    pub fn get_stats(&self) -> MemoryPoolStats {
        // LockPool没有len方法，我们使用池的数量作为统计
        let decompress_buffers = self.decompress_pools.len();
        let total_memory_usage_mb = self.get_memory_usage() as f64 / 1024.0 / 1024.0;

        MemoryPoolStats {
            decompress_buffers,
            total_memory_usage_mb,
        }
    }
}

/// 内存池统计信息
#[derive(Debug, Clone)]
pub struct MemoryPoolStats {
    pub decompress_buffers: usize,
    pub total_memory_usage_mb: f64,
}

// 为了支持批量异步操作，需要实现Clone
impl Clone for ZeroCopyMemoryPool {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            decompress_pools: self.decompress_pools.clone(),
            current_memory_usage: Arc::clone(&self.current_memory_usage),
        }
    }
}

impl Default for ZeroCopyMemoryPool {
    fn default() -> Self {
        Self::new(MemoryPoolConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    /// 测试批量异步创建文件映射的功能
    #[tokio::test]
    async fn test_batch_file_mapping_async() {
        // 创建临时测试文件
        let mut temp_files = Vec::new();
        for i in 0..3 {
            let mut temp_file = NamedTempFile::new().unwrap();
            writeln!(temp_file, "测试数据 {}: Hello, World!", i).unwrap();
            temp_files.push(temp_file);
        }

        let pool = ZeroCopyMemoryPool::default();
        let paths: Vec<_> = temp_files.iter().map(|f| f.path()).collect();

        // 测试批量创建
        let results = pool.create_file_mappings_batch(&paths).await;
        
        assert_eq!(results.len(), 3);
        for (i, result) in results.iter().enumerate() {
            assert!(result.is_ok(), "文件 {} 映射失败: {:?}", i, result);
            let mapping = result.as_ref().unwrap();
            assert!(!mapping.is_empty(), "文件 {} 映射为空", i);
            
            // 验证内容
            let content = String::from_utf8_lossy(mapping.as_slice());
            assert!(content.contains(&format!("测试数据 {}", i)));
        }

        println!("✅ 批量异步文件映射测试通过");
    }

    /// 测试单个异步文件映射功能
    #[tokio::test]
    async fn test_single_file_mapping_async() {
        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(temp_file, "单个异步测试文件").unwrap();

        let pool = ZeroCopyMemoryPool::default();
        
        // 测试异步创建
        let result = pool.create_file_mapping_async(temp_file.path()).await;
        assert!(result.is_ok(), "异步文件映射失败: {:?}", result);
        
        let mapping = result.unwrap();
        assert!(!mapping.is_empty());
        
        let content = String::from_utf8_lossy(mapping.as_slice());
        assert!(content.contains("单个异步测试文件"));

        println!("✅ 单个异步文件映射测试通过");
    }

    /// 测试空路径数组的处理
    #[tokio::test]
    async fn test_empty_batch() {
        let pool = ZeroCopyMemoryPool::default();
        let empty_paths: Vec<std::path::PathBuf> = vec![];
        
        let results = pool.create_file_mappings_batch(&empty_paths).await;
        assert_eq!(results.len(), 0);
        
        println!("✅ 空批次处理测试通过");
    }

    /// 测试错误处理（不存在的文件）
    #[tokio::test]
    async fn test_nonexistent_file_handling() {
        let pool = ZeroCopyMemoryPool::default();
        let paths = vec!["nonexistent_file_1.txt", "nonexistent_file_2.txt"];
        
        let results = pool.create_file_mappings_batch(&paths).await;
        assert_eq!(results.len(), 2);
        
        for result in &results {
            assert!(result.is_err(), "应该返回错误，但得到了成功结果");
        }
        
        println!("✅ 错误处理测试通过");
    }
}
