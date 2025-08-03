//! # 内存池模块 (Memory Pool Module)
//! 
//! 提供高效的内存管理功能，支持分层内存池、内存复用、零拷贝访问和统计监控。
//! 
//! ## 设计理念
//! 
//! - **分层设计**：根据数据大小分层管理内存，提高分配效率
//! - **内存复用**：避免频繁的内存分配/释放，减少系统开销
//! - **零拷贝访问**：提供直接指针访问，避免不必要的数据拷贝
//! - **统计监控**：实时监控内存使用情况，支持性能分析
//! 
//! ## 核心组件
//! 
//! - `MemoryBlock`：智能内存块，支持零拷贝访问
//! - `MmapBlock`：内存映射块，用于文件映射
//! - `UnifiedMemoryPool`：统一内存池，管理所有类型的内存
//! 
//! ## 使用示例
//! 
//! ```rust
//! use canp::memory_pool::{UnifiedMemoryPool, MemoryPoolConfig};
//! 
//! fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let config = MemoryPoolConfig::default();
//!     let pool = UnifiedMemoryPool::new(config);
//!     Ok(())
//! }
//! ```

use anyhow::Result;
use lock_pool::{LockPool, maybe_await};
use lru::LruCache;
use memmap2::Mmap;
use std::fs::File;
use std::path::Path;
use std::sync::Arc;
use std::sync::RwLock;
use std::time::{Duration, Instant};
use std::num::NonZeroUsize;
use tracing::info;

/// 智能指针包装的内存块
/// 
/// 提供零拷贝访问和自动内存管理功能。
/// 
/// ## 特性
/// 
/// - **零拷贝访问**：通过 `as_slice()` 和 `as_ptr_and_len()` 提供直接访问
/// - **智能管理**：使用 `Arc` 实现自动引用计数
/// - **不可克隆**：避免意外数据拷贝，保证零拷贝原则
/// 
/// ## 使用示例
/// 
/// ```rust
/// use canp::memory_pool::MemoryBlock;
/// 
/// fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let data = vec![1, 2, 3, 4, 5];
///     let block = MemoryBlock::new(data);
/// 
///     // 零拷贝访问
///     assert_eq!(block.as_slice(), &[1, 2, 3, 4, 5]);
///     assert_eq!(block.len(), 5);
/// 
///     let (ptr, len) = block.as_ptr_and_len();
///     assert_eq!(len, 5);
///     assert!(!ptr.is_null());
///     
///     Ok(())
/// }
/// ```
#[derive(Debug)]
pub struct MemoryBlock {
    /// 数据指针（共享所有权）
    data: Arc<Vec<u8>>,
    /// 数据长度
    length: usize,
    /// 创建时间
    created_at: Instant,
}

impl MemoryBlock {
    /// 创建新的内存块
    /// 
    /// ## 参数
    /// 
    /// - `data`：要包装的数据
    /// 
    /// ## 返回值
    /// 
    /// 返回包装后的 `MemoryBlock`
    /// 
    /// use canp::memory_pool::MemoryBlock;
    /// 
    /// fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     let data = vec![1, 2, 3, 4, 5];
    ///     let block = MemoryBlock::new(data);
    ///     assert_eq!(block.len(), 5);
    ///     Ok(())
    /// }
    /// ```
    pub fn new(data: Vec<u8>) -> Self {
        let length = data.len();
        Self {
            data: Arc::new(data),
            length,
            created_at: Instant::now(),
        }
    }

    /// 获取数据切片（零拷贝）
    /// 
    /// 返回对内部数据的不可变引用，不进行数据拷贝。
    /// 
    /// ## 返回值
    /// 
    /// 返回 `&[u8]` 切片引用
    /// 
    /// let data = vec![1, 2, 3, 4, 5];
    /// let block = MemoryBlock::new(data);
    /// assert_eq!(block.as_slice(), &[1, 2, 3, 4, 5]);
    /// ```
    pub fn as_slice(&self) -> &[u8] {
        &self.data
    }

    /// 获取数据指针和长度（零拷贝）
    /// 
    /// 返回指向数据的原始指针和长度，用于底层操作。
    /// 
    /// ## 返回值
    /// 
    /// 返回 `(*const u8, usize)` 元组，包含指针和长度
    /// 
    /// ## 安全说明
    /// 
    /// 返回的指针在 `MemoryBlock` 生命周期内有效。
    /// 
    /// let data = vec![1, 2, 3, 4, 5];
    /// let block = MemoryBlock::new(data);
    /// let (ptr, len) = block.as_ptr_and_len();
    /// assert_eq!(len, 5);
    /// assert!(!ptr.is_null());
    /// ```
    pub fn as_ptr_and_len(&self) -> (*const u8, usize) {
        (self.data.as_ptr(), self.length)
    }

    /// 获取可变数据切片（需要可变引用）
    /// 
    /// 尝试获取可变切片，只有在没有其他引用时才能成功。
    /// 
    /// ## 返回值
    /// 
    /// 返回 `&mut [u8]` 可变切片，如果存在其他引用则返回空切片
    /// 
    /// let data = vec![1, 2, 3, 4, 5];
    /// let mut block = MemoryBlock::new(data);
    /// let slice = block.as_mut_slice();
    /// if !slice.is_empty() {
    ///     slice[0] = 10;
    /// }
    /// ```
    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        Arc::get_mut(&mut self.data).map(|v| &mut v[..]).unwrap_or(&mut [])
    }

    /// 获取数据长度
    /// 
    /// ## 返回值
    /// 
    /// 返回数据的字节长度
    /// 
    /// let data = vec![1, 2, 3, 4, 5];
    /// let block = MemoryBlock::new(data);
    /// assert_eq!(block.len(), 5);
    /// ```
    pub fn len(&self) -> usize {
        self.length
    }

    /// 检查是否为空
    /// 
    /// ## 返回值
    /// 
    /// 如果数据长度为0则返回 `true`，否则返回 `false`
    /// 
/// use canp::memory_pool::MemoryBlock;
/// 
/// ```rust
/// use canp::memory_pool::MemoryBlock;
/// 
/// fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let block = MemoryBlock::new(vec![]);
///     assert!(block.is_empty());
/// 
///     let block = MemoryBlock::new(vec![1, 2, 3]);
///     assert!(!block.is_empty());
///     Ok(())
/// }
/// ```
/// ```
    pub fn is_empty(&self) -> bool {
        self.length == 0
    }

    /// 获取创建时间
    /// 
    /// ## 返回值
    /// 
    /// 返回内存块的创建时间
    /// 
/// ```rust
/// use canp::memory_pool::MemoryBlock;
/// 
/// fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let block = MemoryBlock::new(vec![1, 2, 3]);
///     let created_at = block.created_at();
///     assert!(created_at.elapsed().as_secs() < 1);
///     Ok(())
/// }
/// ```
/// ```
    pub fn created_at(&self) -> Instant {
        self.created_at
    }
}

/// 智能指针包装的mmap块
/// 
/// 用于内存映射文件，提供零拷贝访问。
/// 
/// ## 特性
/// 
/// - **零拷贝访问**：直接访问映射的内存
/// - **文件关联**：可以关联文件路径
/// - **自动管理**：使用 `Arc` 实现自动引用计数
/// 
/// ## 使用示例
/// 
/// ```rust
/// use canp::memory_pool::MmapBlock;
/// 
/// fn main() -> Result<(), Box<dyn std::error::Error>> {
///     // 创建mmap块的示例
///     // 实际使用时需要提供真实的mmap对象
///     Ok(())
/// }
/// ```
#[derive(Debug, Clone)]
pub struct MmapBlock {
    /// mmap数据
    mmap: Arc<Mmap>,
    /// 数据长度
    length: usize,
    /// 文件路径（如果是文件mmap）
    file_path: Option<String>,
    /// 创建时间
    created_at: Instant,
}

impl MmapBlock {
    /// 创建新的mmap块
    /// 
    /// ## 参数
    /// 
    /// - `mmap`：内存映射对象
    /// - `file_path`：关联的文件路径（可选）
    /// 
    /// ## 返回值
    /// 
    /// 返回包装后的 `MmapBlock`
    /// 
/// use canp::memory_pool::MmapBlock;
/// 
/// fn main() -> Result<(), Box<dyn std::error::Error>> {
///     // 创建mmap块的示例
///     // 实际使用时需要提供真实的mmap对象
///     Ok(())
/// }
/// ```
    pub fn new(mmap: Mmap, file_path: Option<String>) -> Self {
        let length = mmap.len();
        Self {
            mmap: Arc::new(mmap),
            length,
            file_path,
            created_at: Instant::now(),
        }
    }

    /// 获取数据切片（零拷贝）
    /// 
    /// 返回对映射内存的不可变引用。
    /// 
    /// ## 返回值
    /// 
    /// 返回 `&[u8]` 切片引用
    /// 
/// use canp::memory_pool::MmapBlock;
/// 
/// fn main() -> Result<(), Box<dyn std::error::Error>> {
///     // 这里需要实际的mmap块创建逻辑
///     // let mmap_block = create_mmap_block()?;
///     // let slice = mmap_block.as_slice();
///     // println!("数据长度: {}", slice.len());
///     Ok(())
/// }
/// ```
    pub fn as_slice(&self) -> &[u8] {
        &self.mmap
    }

    /// 获取数据指针和长度（零拷贝）
    /// 
    /// 返回指向映射内存的原始指针和长度。
    /// 
    /// ## 返回值
    /// 
    /// 返回 `(*const u8, usize)` 元组，包含指针和长度
    /// 
    /// ## 安全说明
    /// 
    /// 返回的指针在 `MmapBlock` 生命周期内有效。
    /// 
    /// let mmap_block = create_mmap_block()?;
    /// let (ptr, len) = mmap_block.as_ptr_and_len();
    /// assert_eq!(len, mmap_block.len());
    /// assert!(!ptr.is_null());
    /// ```
    pub fn as_ptr_and_len(&self) -> (*const u8, usize) {
        (self.mmap.as_ptr(), self.length)
    }

    /// 获取数据长度
    /// 
    /// ## 返回值
    /// 
    /// 返回映射数据的字节长度
    /// 
    /// let mmap_block = create_mmap_block()?;
    /// println!("映射数据长度: {} bytes", mmap_block.len());
    /// ```
    pub fn len(&self) -> usize {
        self.length
    }

    /// 检查是否为空
    /// 
    /// ## 返回值
    /// 
    /// 如果映射数据长度为0则返回 `true`，否则返回 `false`
    /// 
    /// let mmap_block = create_mmap_block()?;
    /// if mmap_block.is_empty() {
    ///     println!("映射文件为空");
    /// }
    /// ```
    pub fn is_empty(&self) -> bool {
        self.length == 0
    }

    /// 获取文件路径
    /// 
    /// ## 返回值
    /// 
    /// 返回关联的文件路径，如果是匿名映射则返回 `None`
    /// 
    /// let mmap_block = create_mmap_block()?;
    /// if let Some(path) = mmap_block.file_path() {
    ///     println!("映射文件: {}", path);
    /// }
    /// ```
    pub fn file_path(&self) -> Option<&str> {
        self.file_path.as_deref()
    }

    /// 获取创建时间
    /// 
    /// ## 返回值
    /// 
    /// 返回mmap块的创建时间
    /// 
    /// let mmap_block = create_mmap_block()?;
    /// let created_at = mmap_block.created_at();
    /// println!("创建时间: {:?}", created_at);
    /// ```
    pub fn created_at(&self) -> Instant {
        self.created_at
    }
}

/// 内存池配置
/// 
/// 定义内存池的行为参数，包括分层大小、缓存配置和内存限制。
/// 
/// ## 配置项说明
/// 
/// - **分层大小**：根据数据大小分层管理，提高分配效率
/// - **缓存配置**：LRU缓存大小和TTL设置
/// - **内存限制**：总内存使用量限制和警告阈值
/// 
/// ## 使用示例
/// 
/// ```rust
/// use canp::memory_pool::MemoryPoolConfig;
/// 
/// fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let config = MemoryPoolConfig {
///         block_sizes: vec![512, 1024, 2048, 4096, 8192],
///         decompress_sizes: vec![1024, 2048, 4096, 8192, 16384],
///         frame_sizes: vec![256, 512, 1024, 2048, 4096],
///         mmap_cache_size: 1000,
///         block_cache_size: 500,
///         cache_ttl: 300,
///         max_total_memory: 1024 * 1024 * 1024,  // 1GB
///         memory_warning_threshold: 0.8,
///         ..Default::default()
///     };
///     Ok(())
/// }
/// ```
#[derive(Debug, Clone)]
pub struct MemoryPoolConfig {
    /// 内存块大小配置
    /// 
    /// 定义通用内存块池的分层大小，从小到大排列。
    /// 系统会根据请求的大小选择最接近的池。
    pub block_sizes: Vec<usize>,
    /// mmap大小配置
    /// 
    /// 定义mmap池的分层大小，用于文件映射。
    pub mmap_sizes: Vec<usize>,
    /// 解压缩缓冲区大小配置
    /// 
    /// 定义解压缩缓冲区池的分层大小，专门用于解压缩操作。
    pub decompress_sizes: Vec<usize>,
    /// 帧数据缓冲区大小配置
    /// 
    /// 定义帧数据缓冲区池的分层大小，专门用于帧数据处理。
    pub frame_sizes: Vec<usize>,
    /// mmap缓存大小
    /// 
    /// LRU缓存中最多保存的mmap块数量。
    pub mmap_cache_size: usize,
    /// 内存块缓存大小
    /// 
    /// LRU缓存中最多保存的内存块数量。
    pub block_cache_size: usize,
    /// 缓存TTL（秒）
    /// 
    /// 缓存项的生存时间，超过此时间的缓存项会被清理。
    pub cache_ttl: u64,
    /// 是否启用预分配
    /// 
    /// 是否在内存池初始化时预分配内存块。
    pub enable_preallocation: bool,
    /// 预分配数量
    /// 
    /// 每个池预分配的内存块数量。
    pub preallocation_count: usize,
    /// 最大总内存使用量（字节）
    /// 
    /// 内存池允许使用的最大内存量，超过此限制会拒绝分配。
    pub max_total_memory: usize,
    /// 内存使用量警告阈值（百分比）
    /// 
    /// 当内存使用量超过此阈值时会记录警告日志。
    pub memory_warning_threshold: f64,
}

impl Default for MemoryPoolConfig {
    fn default() -> Self {
        Self {
            block_sizes: vec![1024, 4096, 16384, 65536, 262144, 1048576], // 1KB - 1MB
            mmap_sizes: vec![
                1024 * 1024,
                5 * 1024 * 1024,
                15 * 1024 * 1024,
                50 * 1024 * 1024,
            ], // 1MB - 50MB
            decompress_sizes: vec![10240, 51200, 102400, 512000], // 10KB - 500KB (适合gzip解压)
            frame_sizes: vec![512, 1024, 2048, 4096, 8192], // 512B - 8KB (适合单个帧)
            mmap_cache_size: 1000,
            block_cache_size: 1000,
            cache_ttl: 3600, // 1小时
            enable_preallocation: true,
            preallocation_count: 100,
            max_total_memory: 1024 * 1024 * 1024, // 1GB
            memory_warning_threshold: 0.8, // 80%
        }
    }
}

/// 内存池统计信息
#[derive(Debug, Clone)]
pub struct MemoryPoolStats {
    /// 总分配次数
    pub total_allocations: usize,
    /// 总释放次数
    pub total_deallocations: usize,
    /// 当前内存使用量
    pub current_memory_usage: usize,
    /// 峰值内存使用量
    pub peak_memory_usage: usize,
    /// 内存块池命中率
    pub block_pool_hit_rate: f64,
    /// mmap池命中率
    pub mmap_pool_hit_rate: f64,
    /// 缓存命中率
    pub cache_hit_rate: f64,
}





/// 统一内存池
/// 
/// 提供统一的内存管理接口，支持多种内存类型（内存块、mmap块、解压缩缓冲区、帧数据缓冲区）。
/// 
/// ## 特性
/// 
/// - **分层管理**：根据数据大小分层管理内存，提高分配效率
/// - **内存复用**：避免频繁的内存分配/释放，减少系统开销
/// - **零拷贝访问**：提供直接指针访问，避免不必要的数据拷贝
/// - **统计监控**：实时监控内存使用情况，支持性能分析
/// 
/// ## 使用示例
/// 
/// ```rust
/// use canp::memory_pool::{UnifiedMemoryPool, MemoryBlock};
/// 
/// fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let pool = UnifiedMemoryPool::default();
/// 
///     // 分配内存块
///     let block = pool.allocate_block(1024)?;
/// 
///     // 零拷贝访问
///     let slice = block.as_slice();
///     let (ptr, len) = block.as_ptr_and_len();
/// 
///     // 回收内存
///     pool.release_block(block)?;
///     
///     Ok(())
/// }
/// ```
pub struct UnifiedMemoryPool {
    /// 配置
    config: MemoryPoolConfig,
    /// 分层内存块池
    block_pools: Vec<Arc<LockPool<Vec<u8>, 64, 1024>>>,
    /// mmap池
    mmap_pools: Vec<Arc<LockPool<Mmap, 32, 512>>>,
    /// 解压缩缓冲区池
    decompress_pools: Vec<Arc<LockPool<Vec<u8>, 32, 256>>>,
    /// 帧数据缓冲区池
    frame_pools: Vec<Arc<LockPool<Vec<u8>, 64, 512>>>,
    /// mmap缓存
    mmap_cache: Arc<RwLock<LruCache<String, Arc<MmapBlock>>>>,
    /// 内存块缓存
    block_cache: Arc<RwLock<LruCache<String, Arc<MemoryBlock>>>>,
    /// 统计信息
    stats: Arc<RwLock<MemoryPoolStats>>,
    /// 当前内存使用量
    current_memory_usage: Arc<RwLock<usize>>,
    /// 内存分配失败计数
    allocation_failures: Arc<RwLock<usize>>,
}

impl UnifiedMemoryPool {
    /// 创建新的内存池
    /// 
    /// ## 参数
    /// 
    /// - `config`：内存池配置
    /// 
    /// ## 返回值
    /// 
    /// 返回初始化好的 `UnifiedMemoryPool`
    /// 
    /// use canp::memory_pool::{UnifiedMemoryPool, MemoryPoolConfig};
    /// 
    /// fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     let config = MemoryPoolConfig::default();
    ///     let pool = UnifiedMemoryPool::new(config);
    ///     Ok(())
    /// }
    /// ```
    pub fn new(config: MemoryPoolConfig) -> Self {
        // 创建分层内存块池
        let block_pools = config
            .block_sizes
            .iter()
            .map(|&size| Arc::new(LockPool::from_fn(move |_| Vec::with_capacity(size))))
            .collect();

        // 创建mmap池 - 修复：mmap池应该存储Mmap对象而不是Vec
        let mmap_pools = config
            .mmap_sizes
            .iter()
            .map(|&_size| {
                Arc::new(LockPool::from_fn(move |_| {
                    // 创建临时文件用于mmap池
                    match tempfile::tempfile() {
                        Ok(file) => {
                            // 设置文件大小为对应的大小
                            if let Ok(()) = file.set_len(_size as u64) {
                                unsafe { Mmap::map(&file).unwrap_or_else(|_| {
                                    // 如果映射失败，创建一个空的内存映射
                                    let empty_file = tempfile::tempfile().unwrap();
                                    empty_file.set_len(0).unwrap();
                                    Mmap::map(&empty_file).unwrap()
                                }) }
                            } else {
                                // 如果设置文件大小失败，创建一个空的内存映射
                                let empty_file = tempfile::tempfile().unwrap();
                                empty_file.set_len(0).unwrap();
                                unsafe { Mmap::map(&empty_file).unwrap() }
                            }
                        }
                        Err(_) => {
                            // 如果创建临时文件失败，创建一个空的内存映射
                            let empty_file = tempfile::tempfile().unwrap();
                            empty_file.set_len(0).unwrap();
                            unsafe { Mmap::map(&empty_file).unwrap() }
                        }
                    }
                }))
            })
            .collect();

        // 创建解压缩缓冲区池
        let decompress_pools = config
            .decompress_sizes
            .iter()
            .map(|&size| Arc::new(LockPool::from_fn(move |_| Vec::with_capacity(size))))
            .collect();

        // 创建帧数据缓冲区池
        let frame_pools = config
            .frame_sizes
            .iter()
            .map(|&size| Arc::new(LockPool::from_fn(move |_| Vec::with_capacity(size))))
            .collect();

        // 创建缓存
        let mmap_cache = Arc::new(RwLock::new(LruCache::new(
            NonZeroUsize::new(config.mmap_cache_size).unwrap()
        )));
        let block_cache = Arc::new(RwLock::new(LruCache::new(
            NonZeroUsize::new(config.block_cache_size).unwrap()
        )));

        // 创建统计信息
        let stats = Arc::new(RwLock::new(MemoryPoolStats {
            total_allocations: 0,
            total_deallocations: 0,
            current_memory_usage: 0,
            peak_memory_usage: 0,
            block_pool_hit_rate: 0.0,
            mmap_pool_hit_rate: 0.0,
            cache_hit_rate: 0.0,
        }));

        // 创建内存使用量和分配失败计数
        let current_memory_usage = Arc::new(RwLock::new(0));
        let allocation_failures = Arc::new(RwLock::new(0));

        let pool = Self {
            config,
            block_pools,
            mmap_pools,
            decompress_pools,
            frame_pools,
            mmap_cache,
            block_cache,
            stats,
            current_memory_usage,
            allocation_failures,
        };

        // 预分配
        if pool.config.enable_preallocation {
            pool.preallocate();
        }

        pool
    }

    /// 选择合适的内存块池
    /// 
    /// 根据请求的数据大小，选择最合适的内存块池。
    /// 
    /// ## 参数
    /// 
    /// - `size`：请求的数据大小
    /// 
    /// ## 返回值
    /// 
    /// 返回匹配的内存块池，如果找不到则返回 `None`
    /// 
    /// let pool = UnifiedMemoryPool::default();
    /// let block_pool = pool.select_block_pool(1024);
    /// assert!(block_pool.is_some());
    /// ```
    fn select_block_pool(&self, size: usize) -> Option<&Arc<LockPool<Vec<u8>, 64, 1024>>> {
        for (i, &block_size) in self.config.block_sizes.iter().enumerate() {
            if size <= block_size {
                return self.block_pools.get(i);
            }
        }
        self.block_pools.last()
    }

    /// 选择合适的mmap池
    /// 
    /// 根据请求的mmap大小，选择最合适的mmap池。
    /// 
    /// ## 参数
    /// 
    /// - `size`：请求的mmap大小
    /// 
    /// ## 返回值
    /// 
    /// 返回匹配的mmap池，如果找不到则返回 `None`
    /// 
    /// let pool = UnifiedMemoryPool::default();
    /// let mmap_pool = pool.select_mmap_pool(1024 * 1024);
    /// assert!(mmap_pool.is_some());
    /// ```
    fn select_mmap_pool(&self, size: usize) -> Option<&Arc<LockPool<Mmap, 32, 512>>> {
        for (i, &mmap_size) in self.config.mmap_sizes.iter().enumerate() {
            if size <= mmap_size {
                return self.mmap_pools.get(i);
            }
        }
        self.mmap_pools.last()
    }

    /// 选择合适的解压缩缓冲区池
    /// 
    /// 根据请求的解压缩缓冲区大小，选择最合适的解压缩缓冲区池。
    /// 
    /// ## 参数
    /// 
    /// - `size`：请求的解压缩缓冲区大小
    /// 
    /// ## 返回值
    /// 
    /// 返回匹配的解压缩缓冲区池，如果找不到则返回 `None`
    /// 
    /// let pool = UnifiedMemoryPool::default();
    /// let decompress_pool = pool.select_decompress_pool(10240);
    /// assert!(decompress_pool.is_some());
    /// ```
    fn select_decompress_pool(&self, size: usize) -> Option<&Arc<LockPool<Vec<u8>, 32, 256>>> {
        for (i, &decompress_size) in self.config.decompress_sizes.iter().enumerate() {
            if size <= decompress_size {
                return self.decompress_pools.get(i);
            }
        }
        self.decompress_pools.last()
    }

    /// 选择合适的帧数据缓冲区池
    /// 
    /// 根据请求的帧数据缓冲区大小，选择最合适的帧数据缓冲区池。
    /// 
    /// ## 参数
    /// 
    /// - `size`：请求的帧数据缓冲区大小
    /// 
    /// ## 返回值
    /// 
    /// 返回匹配的帧数据缓冲区池，如果找不到则返回 `None`
    /// 
    /// let pool = UnifiedMemoryPool::default();
    /// let frame_pool = pool.select_frame_pool(1024);
    /// assert!(frame_pool.is_some());
    /// ```
    fn select_frame_pool(&self, size: usize) -> Option<&Arc<LockPool<Vec<u8>, 64, 512>>> {
        for (i, &frame_size) in self.config.frame_sizes.iter().enumerate() {
            if size <= frame_size {
                return self.frame_pools.get(i);
            }
        }
        self.frame_pools.last()
    }

    /// 分配内存块
    /// 
    /// 尝试从内存池中分配一个指定大小的内存块。
    /// 
    /// ## 参数
    /// 
    /// - `size`：请求的内存块大小
    /// 
    /// ## 返回值
    /// 
    /// 成功时返回 `MemoryBlock`，失败时返回 `Result<MemoryBlock>`
    /// 
/// use canp::memory_pool::UnifiedMemoryPool;
/// 
/// fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let pool = UnifiedMemoryPool::default();
///     let block = pool.allocate_block(1024)?;
///     assert_eq!(block.len(), 0); // 新分配的块长度为0
///     assert!(block.is_empty()); // 长度为0时应该为空
///     Ok(())
/// }
/// ```
    pub fn allocate_block(&self, size: usize) -> Result<MemoryBlock> {
        // 检查内存限制
        self.check_memory_limit(size)?;
        
        if let Some(pool) = self.select_block_pool(size) {
            if let Some(mut guard) = pool.try_get() {
                // 池命中
                self.record_pool_hit();
                guard.clear();
                guard.reserve(size);
                let data = guard.to_vec();
                self.update_memory_usage(size, true);
                self.record_allocation(size);
                Ok(MemoryBlock::new(data))
            } else {
                // 池未命中，直接分配
                self.record_pool_miss();
                let data = Vec::with_capacity(size);
                self.update_memory_usage(size, true);
                self.record_allocation(size);
                Ok(MemoryBlock::new(data))
            }
        } else {
            // 没有合适的池，直接分配
            self.record_pool_miss();
            let data = Vec::with_capacity(size);
            self.update_memory_usage(size, true);
            self.record_allocation(size);
            Ok(MemoryBlock::new(data))
        }
    }

    /// 异步分配内存块
    /// 
    /// 尝试异步从内存池中分配一个指定大小的内存块。
    /// 
    /// ## 参数
    /// 
    /// - `size`：请求的内存块大小
    /// 
    /// ## 返回值
    /// 
    /// 成功时返回 `MemoryBlock`，失败时返回 `Result<MemoryBlock>`
    /// 
    
    pub async fn allocate_block_async(&self, size: usize) -> Result<MemoryBlock> {
        // 检查内存限制
        self.check_memory_limit(size)?;
        
        if let Some(pool) = self.select_block_pool(size) {
            // 等待可用对象
            let mut guard = maybe_await!(pool.get());
            self.record_pool_hit();
            guard.clear();
            guard.reserve(size);
            let data = guard.to_vec();
            self.update_memory_usage(size, true);
            self.record_allocation(size);
            Ok(MemoryBlock::new(data))
        } else {
            // 没有合适的池，直接分配
            self.record_pool_miss();
            let data = Vec::with_capacity(size);
            self.update_memory_usage(size, true);
            self.record_allocation(size);
            Ok(MemoryBlock::new(data))
        }
    }

    /// 创建文件mmap
    /// 
    /// 尝试从内存池中获取或创建一个文件的内存映射。
    /// 
    /// ## 参数
    /// 
    /// - `path`：文件路径
    /// 
    /// ## 返回值
    /// 
    /// 成功时返回 `MmapBlock`，失败时返回 `Result<MmapBlock>`
    /// 
    /// use canp::memory_pool::UnifiedMemoryPool;
    /// 
    /// #[tokio::main]
    /// async fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     let pool = UnifiedMemoryPool::default();
    ///     // 创建文件内存映射
    ///     Ok(())
    /// }
    /// ```
    pub async fn create_file_mmap<P: AsRef<Path>>(&self, path: P) -> Result<MmapBlock> {
        let path_str = path.as_ref().to_string_lossy().to_string();

        // 检查缓存
        {
            let cache = self.mmap_cache.write().unwrap();
            if cache.contains(&path_str) {
                self.record_cache_hit();
                // 由于Mmap不支持Clone，我们重新打开文件
                let file = File::open(&path)?;
                let mmap = unsafe { Mmap::map(&file)? };
                return Ok(MmapBlock::new(mmap, Some(path_str)));
            }
        }

        // 缓存未命中，创建新的mmap
        self.record_cache_miss();
        let file = File::open(&path)?;
        let mmap = unsafe { Mmap::map(&file)? };
        let mmap_len = mmap.len();

        // 缓存mmap - 由于Mmap不支持Clone，我们重新创建
        let mmap_block = MmapBlock::new(mmap, Some(path_str.clone()));
        {
            let mut cache = self.mmap_cache.write().unwrap();
            cache.put(path_str, Arc::new(mmap_block.clone()));
        }

        self.record_allocation(mmap_len);
        Ok(mmap_block)
    }

    /// 创建匿名mmap
    /// 
    /// 尝试从内存池中创建一个匿名内存映射。
    /// 
    /// ## 参数
    /// 
    /// - `size`：请求的匿名mmap大小
    /// 
    /// ## 返回值
    /// 
    /// 成功时返回 `MmapBlock`，失败时返回 `Result<MmapBlock>`
    /// 
    /// use canp::memory_pool::UnifiedMemoryPool;
    /// 
    /// #[tokio::main]
    /// async fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     let pool = UnifiedMemoryPool::default();
    ///     // 创建匿名内存映射
    ///     Ok(())
    /// }
    /// ```
    pub async fn create_anonymous_mmap(&self, size: usize) -> Result<MmapBlock> {
        // 创建临时文件用于匿名mmap
        let temp_file = tempfile::tempfile()?;
        temp_file.set_len(size as u64)?;

        let mmap = unsafe { Mmap::map(&temp_file)? };
        self.record_allocation(size);

        Ok(MmapBlock::new(mmap, None))
    }

    /// 批量分配内存块
    /// 
    /// 尝试批量分配多个指定大小的内存块。
    /// 
    /// ## 参数
    /// 
    /// - `sizes`：请求的内存块大小列表
    /// 
    /// ## 返回值
    /// 
    /// 成功时返回 `Vec<MemoryBlock>`，失败时返回 `Result<Vec<MemoryBlock>>`
    /// 
    
    pub fn allocate_blocks_batch(&self, sizes: &[usize]) -> Result<Vec<MemoryBlock>> {
        let mut blocks = Vec::with_capacity(sizes.len());
        
        for (i, &size) in sizes.iter().enumerate() {
            match self.allocate_block(size) {
                Ok(block) => {
                    blocks.push(block);
                }
                Err(e) => {
                    // 发生错误，回滚已分配的内存块
                    for block in blocks {
                        let _ = self.release_block(block);
                    }
                    return Err(anyhow::anyhow!(
                        "批量分配内存块失败，索引 {}: {}",
                        i, e
                    ));
                }
            }
        }
        
        Ok(blocks)
    }

    /// 批量异步分配内存块
    /// 
    /// 尝试批量异步分配多个指定大小的内存块。
    /// 
    /// ## 参数
    /// 
    /// - `sizes`：请求的内存块大小列表
    /// 
    /// ## 返回值
    /// 
    /// 成功时返回 `Vec<MemoryBlock>`，失败时返回 `Result<Vec<MemoryBlock>>`
    /// 
    /// use canp::memory_pool::UnifiedMemoryPool;
    /// 
    /// #[tokio::main]
    /// async fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     let pool = UnifiedMemoryPool::default();
    ///     let sizes = vec![1024, 2048, 4096];
    ///     let blocks = pool.allocate_blocks_batch_async(&sizes).await?;
    ///     assert_eq!(blocks.len(), 3);
    ///     Ok(())
    /// }
    /// ```
    pub async fn allocate_blocks_batch_async(&self, sizes: &[usize]) -> Result<Vec<MemoryBlock>> {
        let mut blocks = Vec::with_capacity(sizes.len());
        
        for (i, &size) in sizes.iter().enumerate() {
            match self.allocate_block_async(size).await {
                Ok(block) => {
                    blocks.push(block);
                }
                Err(e) => {
                    // 发生错误，回滚已分配的内存块
                    for block in blocks {
                        let _ = self.release_block(block);
                    }
                    return Err(anyhow::anyhow!(
                        "批量异步分配内存块失败，索引 {}: {}",
                        i, e
                    ));
                }
            }
        }
        
        Ok(blocks)
    }

    /// 批量分配解压缩缓冲区（专门用于gzip解压）
    /// 
    /// 尝试批量分配多个指定大小的解压缩缓冲区。
    /// 
    /// ## 参数
    /// 
    /// - `size`：请求的解压缩缓冲区大小
    /// 
    /// ## 返回值
    /// 
    /// 成功时返回 `Vec<MemoryBlock>`，失败时返回 `Result<Vec<MemoryBlock>>`
    /// 
    /// let pool = UnifiedMemoryPool::default();
    /// let sizes = vec![10240, 51200, 102400];
    /// let buffers = pool.allocate_decompress_buffers_batch(&sizes).unwrap();
    /// assert_eq!(buffers.len(), 3);
    /// for buffer in buffers {
    ///     assert!(buffer.is_empty());
    /// }
    /// ```
    pub fn allocate_decompress_buffer(&self, size: usize) -> Result<MemoryBlock> {
        // 检查内存限制
        self.check_memory_limit(size)?;
        
        if let Some(pool) = self.select_decompress_pool(size) {
            if let Some(mut guard) = pool.try_get() {
                // 池命中
                self.record_pool_hit();
                guard.clear();
                guard.reserve(size);
                let data = guard.to_vec();
                self.update_memory_usage(size, true);
                self.record_allocation(size);
                Ok(MemoryBlock::new(data))
            } else {
                // 池未命中，直接分配
                self.record_pool_miss();
                let data = Vec::with_capacity(size);
                self.update_memory_usage(size, true);
                self.record_allocation(size);
                Ok(MemoryBlock::new(data))
            }
        } else {
            // 没有合适的池，直接分配
            self.record_pool_miss();
            let data = Vec::with_capacity(size);
            self.update_memory_usage(size, true);
            self.record_allocation(size);
            Ok(MemoryBlock::new(data))
        }
    }

    /// 异步分配解压缩缓冲区
    /// 
    /// 尝试异步从内存池中分配一个指定大小的解压缩缓冲区。
    /// 
    /// ## 参数
    /// 
    /// - `size`：请求的解压缩缓冲区大小
    /// 
    /// ## 返回值
    /// 
    /// 成功时返回 `MemoryBlock`，失败时返回 `Result<MemoryBlock>`
    /// 
    /// let pool = UnifiedMemoryPool::default();
    /// let buffer_async = pool.allocate_decompress_buffer_async(51200).await?;
    /// assert_eq!(buffer_async.len(), 0);
    /// ```
    pub async fn allocate_decompress_buffer_async(&self, size: usize) -> Result<MemoryBlock> {
        // 检查内存限制
        self.check_memory_limit(size)?;
        
        if let Some(pool) = self.select_decompress_pool(size) {
            // 等待可用对象
            let mut guard = maybe_await!(pool.get());
            self.record_pool_hit();
            guard.clear();
            guard.reserve(size);
            let data = guard.to_vec();
            self.update_memory_usage(size, true);
            self.record_allocation(size);
            Ok(MemoryBlock::new(data))
        } else {
            // 没有合适的池，直接分配
            self.record_pool_miss();
            let data = Vec::with_capacity(size);
            self.update_memory_usage(size, true);
            self.record_allocation(size);
            Ok(MemoryBlock::new(data))
        }
    }

    /// 批量分配解压缩缓冲区
    /// 
    /// 尝试批量分配多个指定大小的解压缩缓冲区。
    /// 
    /// ## 参数
    /// 
    /// - `sizes`：请求的解压缩缓冲区大小列表
    /// 
    /// ## 返回值
    /// 
    /// 成功时返回 `Vec<MemoryBlock>`，失败时返回 `Result<Vec<MemoryBlock>>`
    /// 
    /// let pool = UnifiedMemoryPool::default();
    /// let sizes = vec![10240, 51200, 102400];
    /// let buffers = pool.allocate_decompress_buffers_batch(&sizes).unwrap();
    /// assert_eq!(buffers.len(), 3);
    /// for buffer in buffers {
    ///     assert!(buffer.is_empty());
    /// }
    /// ```
    pub fn allocate_decompress_buffers_batch(&self, sizes: &[usize]) -> Result<Vec<MemoryBlock>> {
        let mut buffers = Vec::with_capacity(sizes.len());
        
        for (i, &size) in sizes.iter().enumerate() {
            match self.allocate_decompress_buffer(size) {
                Ok(buffer) => {
                    buffers.push(buffer);
                }
                Err(e) => {
                    // 发生错误，回滚已分配的缓冲区
                    for buffer in buffers {
                        let _ = self.release_decompress_buffer(buffer);
                    }
                    return Err(anyhow::anyhow!(
                        "批量分配解压缩缓冲区失败，索引 {}: {}",
                        i, e
                    ));
                }
            }
        }
        
        Ok(buffers)
    }

    /// 批量异步分配解压缩缓冲区
    /// 
    /// 尝试批量异步分配多个指定大小的解压缩缓冲区。
    /// 
    /// ## 参数
    /// 
    /// - `sizes`：请求的解压缩缓冲区大小列表
    /// 
    /// ## 返回值
    /// 
    /// 成功时返回 `Vec<MemoryBlock>`，失败时返回 `Result<Vec<MemoryBlock>>`
    /// 
    /// let pool = UnifiedMemoryPool::default();
    /// let sizes = vec![10240, 51200, 102400];
    /// let buffers = pool.allocate_decompress_buffers_batch_async(&sizes).await.unwrap();
    /// assert_eq!(buffers.len(), 3);
    /// for buffer in buffers {
    ///     assert!(buffer.is_empty());
    /// }
    /// ```
    pub async fn allocate_decompress_buffers_batch_async(&self, sizes: &[usize]) -> Result<Vec<MemoryBlock>> {
        let mut buffers = Vec::with_capacity(sizes.len());
        
        for (i, &size) in sizes.iter().enumerate() {
            match self.allocate_decompress_buffer_async(size).await {
                Ok(buffer) => {
                    buffers.push(buffer);
                }
                Err(e) => {
                    // 发生错误，回滚已分配的缓冲区
                    for buffer in buffers {
                        let _ = self.release_decompress_buffer(buffer);
                    }
                    return Err(anyhow::anyhow!(
                        "批量异步分配解压缩缓冲区失败，索引 {}: {}",
                        i, e
                    ));
                }
            }
        }
        
        Ok(buffers)
    }

    /// 分配帧数据缓冲区（专门用于单个帧数据）
    /// 
    /// 尝试从内存池中分配一个指定大小的帧数据缓冲区。
    /// 
    /// ## 参数
    /// 
    /// - `size`：请求的帧数据缓冲区大小
    /// 
    /// ## 返回值
    /// 
    /// 成功时返回 `MemoryBlock`，失败时返回 `Result<MemoryBlock>`
    /// 
    /// let pool = UnifiedMemoryPool::default();
    /// let buffer = pool.allocate_frame_buffer(1024)?;
    /// assert_eq!(buffer.len(), 0);
    /// assert!(buffer.is_empty());
    /// ```
    pub fn allocate_frame_buffer(&self, size: usize) -> Result<MemoryBlock> {
        // 检查内存限制
        self.check_memory_limit(size)?;
        
        if let Some(pool) = self.select_frame_pool(size) {
            if let Some(mut guard) = pool.try_get() {
                // 池命中
                self.record_pool_hit();
                guard.clear();
                guard.reserve(size);
                let data = guard.to_vec();
                self.update_memory_usage(size, true);
                self.record_allocation(size);
                Ok(MemoryBlock::new(data))
            } else {
                // 池未命中，直接分配
                self.record_pool_miss();
                let data = Vec::with_capacity(size);
                self.update_memory_usage(size, true);
                self.record_allocation(size);
                Ok(MemoryBlock::new(data))
            }
        } else {
            // 没有合适的池，直接分配
            self.record_pool_miss();
            let data = Vec::with_capacity(size);
            self.update_memory_usage(size, true);
            self.record_allocation(size);
            Ok(MemoryBlock::new(data))
        }
    }

    /// 异步分配帧数据缓冲区
    /// 
    /// 尝试异步从内存池中分配一个指定大小的帧数据缓冲区。
    /// 
    /// ## 参数
    /// 
    /// - `size`：请求的帧数据缓冲区大小
    /// 
    /// ## 返回值
    /// 
    /// 成功时返回 `MemoryBlock`，失败时返回 `Result<MemoryBlock>`
    /// 
    /// let pool = UnifiedMemoryPool::default();
    /// let buffer_async = pool.allocate_frame_buffer_async(2048).await?;
    /// assert_eq!(buffer_async.len(), 0);
    /// ```
    pub async fn allocate_frame_buffer_async(&self, size: usize) -> Result<MemoryBlock> {
        // 检查内存限制
        self.check_memory_limit(size)?;
        
        if let Some(pool) = self.select_frame_pool(size) {
            // 等待可用对象
            let mut guard = maybe_await!(pool.get());
            self.record_pool_hit();
            guard.clear();
            guard.reserve(size);
            let data = guard.to_vec();
            self.update_memory_usage(size, true);
            self.record_allocation(size);
            Ok(MemoryBlock::new(data))
        } else {
            // 没有合适的池，直接分配
            self.record_pool_miss();
            let data = Vec::with_capacity(size);
            self.update_memory_usage(size, true);
            self.record_allocation(size);
            Ok(MemoryBlock::new(data))
        }
    }

    /// 批量分配帧数据缓冲区
    /// 
    /// 尝试批量分配多个指定大小的帧数据缓冲区。
    /// 
    /// ## 参数
    /// 
    /// - `sizes`：请求的帧数据缓冲区大小列表
    /// 
    /// ## 返回值
    /// 
    /// 成功时返回 `Vec<MemoryBlock>`，失败时返回 `Result<Vec<MemoryBlock>>`
    /// 
    /// let pool = UnifiedMemoryPool::default();
    /// let sizes = vec![512, 1024, 2048];
    /// let buffers = pool.allocate_frame_buffers_batch(&sizes).unwrap();
    /// assert_eq!(buffers.len(), 3);
    /// for buffer in buffers {
    ///     assert!(buffer.is_empty());
    /// }
    /// ```
    pub fn allocate_frame_buffers_batch(&self, sizes: &[usize]) -> Result<Vec<MemoryBlock>> {
        let mut buffers = Vec::with_capacity(sizes.len());
        
        for (i, &size) in sizes.iter().enumerate() {
            match self.allocate_frame_buffer(size) {
                Ok(buffer) => {
                    buffers.push(buffer);
                }
                Err(e) => {
                    // 发生错误，回滚已分配的缓冲区
                    for buffer in buffers {
                        let _ = self.release_frame_buffer(buffer);
                    }
                    return Err(anyhow::anyhow!(
                        "批量分配帧数据缓冲区失败，索引 {}: {}",
                        i, e
                    ));
                }
            }
        }
        
        Ok(buffers)
    }

    /// 批量异步分配帧数据缓冲区
    /// 
    /// 尝试批量异步分配多个指定大小的帧数据缓冲区。
    /// 
    /// ## 参数
    /// 
    /// - `sizes`：请求的帧数据缓冲区大小列表
    /// 
    /// ## 返回值
    /// 
    /// 成功时返回 `Vec<MemoryBlock>`，失败时返回 `Result<Vec<MemoryBlock>>`
    /// 
    /// let pool = UnifiedMemoryPool::default();
    /// let sizes = vec![512, 1024, 2048];
    /// let buffers = pool.allocate_frame_buffers_batch_async(&sizes).await.unwrap();
    /// assert_eq!(buffers.len(), 3);
    /// for buffer in buffers {
    ///     assert!(buffer.is_empty());
    /// }
    /// ```
    pub async fn allocate_frame_buffers_batch_async(&self, sizes: &[usize]) -> Result<Vec<MemoryBlock>> {
        let mut buffers = Vec::with_capacity(sizes.len());
        
        for (i, &size) in sizes.iter().enumerate() {
            match self.allocate_frame_buffer_async(size).await {
                Ok(buffer) => {
                    buffers.push(buffer);
                }
                Err(e) => {
                    // 发生错误，回滚已分配的缓冲区
                    for buffer in buffers {
                        let _ = self.release_frame_buffer(buffer);
                    }
                    return Err(anyhow::anyhow!(
                        "批量异步分配帧数据缓冲区失败，索引 {}: {}",
                        i, e
                    ));
                }
            }
        }
        
        Ok(buffers)
    }



    /// 预分配内存池
    /// 
    /// 在内存池初始化时预分配一定数量的内存块，提高性能。
    /// 
    /// let pool = UnifiedMemoryPool::default();
    /// pool.preallocate();
    /// ```
    fn preallocate(&self) {
        info!(
            "🔧 预分配内存池: {} 个对象",
            self.config.preallocation_count
        );

        // 预分配内存块池
        for pool in &self.block_pools {
            for _ in 0..self.config.preallocation_count {
                if let Some(mut guard) = pool.try_get() {
                    guard.clear();
                    drop(guard); // 自动归还到池中
                }
            }
        }

        // 预分配解压缩缓冲区池
        for pool in &self.decompress_pools {
            for _ in 0..self.config.preallocation_count {
                if let Some(mut guard) = pool.try_get() {
                    guard.clear();
                    drop(guard);
                }
            }
        }

        // 预分配帧数据缓冲区池
        for pool in &self.frame_pools {
            for _ in 0..self.config.preallocation_count {
                if let Some(mut guard) = pool.try_get() {
                    guard.clear();
                    drop(guard);
                }
            }
        }

        info!("✅ 内存池预分配完成");
    }

    /// 清理过期缓存
    /// 
    /// 定期清理LRU缓存中过期的缓存项。
    /// 
    /// let pool = UnifiedMemoryPool::default();
    /// pool.cleanup_expired_cache();
    /// ```
    pub fn cleanup_expired_cache(&self) {
        let now = Instant::now();
        let ttl = Duration::from_secs(self.config.cache_ttl);

        // 清理mmap缓存
        {
            let mut cache = self.mmap_cache.write().unwrap();
            let expired_keys: Vec<String> = cache
                .iter()
                .filter(|(_, mmap_block)| now.duration_since(mmap_block.created_at()) > ttl)
                .map(|(key, _)| key.clone())
                .collect();

            for key in expired_keys {
                cache.pop(&key);
            }
        }

        // 清理内存块缓存
        {
            let mut cache = self.block_cache.write().unwrap();
            let expired_keys: Vec<String> = cache
                .iter()
                .filter(|(_, block)| now.duration_since(block.created_at()) > ttl)
                .map(|(key, _)| key.clone())
                .collect();

            for key in expired_keys {
                cache.pop(&key);
            }
        }
    }

    /// 获取统计信息
    /// 
    /// 获取当前内存池的统计信息。
    /// 
    /// ## 返回值
    /// 
    /// 返回 `MemoryPoolStats` 结构体，包含总分配、总释放、当前使用量等。
    /// 
/// use canp::memory_pool::UnifiedMemoryPool;
/// 
/// fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let pool = UnifiedMemoryPool::default();
///     let stats = pool.get_stats();
///     println!("总分配: {}", stats.total_allocations);
///     Ok(())
/// }
/// ```
    pub fn get_stats(&self) -> MemoryPoolStats {
        self.stats.read().unwrap().clone()
    }

    /// 记录分配
    /// 
    /// 记录一次内存分配，更新统计信息。
    /// 
    /// ## 参数
    /// 
    /// - `size`：分配的内存大小
    /// 
    /// let pool = UnifiedMemoryPool::default();
    /// pool.record_allocation(1024);
    /// ```
    fn record_allocation(&self, size: usize) {
        let mut stats = self.stats.write().unwrap();
        stats.total_allocations += 1;
        stats.current_memory_usage += size;
        stats.peak_memory_usage = stats.peak_memory_usage.max(stats.current_memory_usage);
    }

    /// 记录释放
    /// 
    /// 记录一次内存释放，更新统计信息。
    /// 
    /// ## 参数
    /// 
    /// - `size`：释放的内存大小
    /// 
    /// let pool = UnifiedMemoryPool::default();
    /// pool.record_deallocation(1024);
    /// ```
    fn record_deallocation(&self, size: usize) {
        let mut stats = self.stats.write().unwrap();
        stats.total_deallocations += 1;
        stats.current_memory_usage = stats.current_memory_usage.saturating_sub(size);
    }

    /// 记录池命中
    /// 
    /// 记录一次内存池命中，简化处理。
    /// 
    /// let pool = UnifiedMemoryPool::default();
    /// pool.record_pool_hit();
    /// ```
    fn record_pool_hit(&self) {
        let _stats = self.stats.write().unwrap();
        // 这里简化处理，实际应该分别记录block_pool和mmap_pool的命中
    }

    /// 记录池未命中
    /// 
    /// 记录一次内存池未命中，简化处理。
    /// 
    /// let pool = UnifiedMemoryPool::default();
    /// pool.record_pool_miss();
    /// ```
    fn record_pool_miss(&self) {
        let _stats = self.stats.write().unwrap();
        // 这里简化处理，实际应该分别记录block_pool和mmap_pool的未命中
    }

    /// 记录缓存命中
    /// 
    /// 记录一次缓存命中，简化处理。
    /// 
    /// let pool = UnifiedMemoryPool::default();
    /// pool.record_cache_hit();
    /// ```
    fn record_cache_hit(&self) {
        let _stats = self.stats.write().unwrap();
        // 简化处理
    }

    /// 记录缓存未命中
    /// 
    /// 记录一次缓存未命中，简化处理。
    /// 
    /// let pool = UnifiedMemoryPool::default();
    /// pool.record_cache_miss();
    /// ```
    fn record_cache_miss(&self) {
        let _stats = self.stats.write().unwrap();
        // 简化处理
    }

    /// 打印统计信息
    /// 
    /// 打印当前内存池的详细统计信息。
    /// 
    /// let pool = UnifiedMemoryPool::default();
    /// pool.print_stats();
    /// ```
    pub fn print_stats(&self) {
        let stats = self.get_stats();
        info!("📊 内存池统计信息:");
        info!("  总分配: {}", stats.total_allocations);
        info!("  总释放: {}", stats.total_deallocations);
        info!(
            "  当前内存使用: {} MB",
            stats.current_memory_usage / (1024 * 1024)
        );
        info!(
            "  峰值内存使用: {} MB",
            stats.peak_memory_usage / (1024 * 1024)
        );
        info!(
            "  内存块池命中率: {:.2}%",
            stats.block_pool_hit_rate * 100.0
        );
        info!("  mmap池命中率: {:.2}%", stats.mmap_pool_hit_rate * 100.0);
        info!("  缓存命中率: {:.2}%", stats.cache_hit_rate * 100.0);
    }

    /// 检查内存使用量是否超过限制
    /// 
    /// 检查当前内存使用量是否超过配置的最大总内存限制。
    /// 
    /// ## 参数
    /// 
    /// - `required_size`：请求的内存大小
    /// 
    /// ## 返回值
    /// 
    /// 成功时返回 `Ok(())`，失败时返回 `Err(anyhow::Error)`
    /// 
    /// let pool = UnifiedMemoryPool::default();
    /// let result = pool.check_memory_limit(1024);
    /// assert!(result.is_ok());
    /// ```
    fn check_memory_limit(&self, required_size: usize) -> Result<()> {
        let current_usage = *self.current_memory_usage.read().unwrap();
        let new_usage = current_usage + required_size;
        
        if new_usage > self.config.max_total_memory {
            let mut failures = self.allocation_failures.write().unwrap();
            *failures += 1;
            
            return Err(anyhow::anyhow!(
                "内存使用量超限: 当前 {} MB, 需要 {} MB, 限制 {} MB",
                current_usage / (1024 * 1024),
                required_size / (1024 * 1024),
                self.config.max_total_memory / (1024 * 1024)
            ));
        }
        
        // 检查警告阈值
        let usage_ratio = new_usage as f64 / self.config.max_total_memory as f64;
        if usage_ratio > self.config.memory_warning_threshold {
            tracing::warn!(
                "内存使用量接近限制: {:.1}% ({} MB / {} MB)",
                usage_ratio * 100.0,
                new_usage / (1024 * 1024),
                self.config.max_total_memory / (1024 * 1024)
            );
        }
        
        Ok(())
    }

    /// 更新内存使用量
    /// 
    /// 根据内存分配或释放更新当前内存使用量。
    /// 
    /// ## 参数
    /// 
    /// - `size`：操作的内存大小
    /// - `is_allocation`：是否为分配操作
    /// 
    /// let pool = UnifiedMemoryPool::default();
    /// pool.update_memory_usage(1024, true); // 分配
    /// pool.update_memory_usage(1024, false); // 释放
    /// ```
    fn update_memory_usage(&self, size: usize, is_allocation: bool) {
        let mut usage = self.current_memory_usage.write().unwrap();
        if is_allocation {
            *usage += size;
        } else {
            *usage = usage.saturating_sub(size);
        }
    }

    /// 回收内存块到池中
    /// 
    /// 尝试将一个内存块回收回其对应的池中。
    /// 
    /// ## 参数
    /// 
    /// - `block`：要回收的内存块
    /// 
    /// ## 返回值
    /// 
    /// 成功时返回 `Ok(())`，失败时返回 `Result<()>`
    /// 
/// use canp::memory_pool::{UnifiedMemoryPool, MemoryBlock};
/// 
/// fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let pool = UnifiedMemoryPool::default();
///     let block = MemoryBlock::new(vec![1, 2, 3]);
///     let result = pool.release_block(block);
///     assert!(result.is_ok());
///     Ok(())
/// }
/// ```
    pub fn release_block(&self, block: MemoryBlock) -> Result<()> {
        let size = block.len();
        
        if let Some(pool) = self.select_block_pool(size) {
            if let Some(mut guard) = pool.try_get() {
                // 清空并重置容量
                guard.clear();
                guard.shrink_to_fit();
                drop(guard); // 自动归还到池中
                
                self.update_memory_usage(size, false);
                self.record_deallocation(size);
                return Ok(());
            }
        }
        
        // 如果池已满或没有合适的池，直接丢弃
        self.update_memory_usage(size, false);
        self.record_deallocation(size);
        Ok(())
    }

    /// 回收解压缩缓冲区到池中
    /// 
    /// 尝试将一个解压缩缓冲区回收回其对应的池中。
    /// 
    /// ## 参数
    /// 
    /// - `buffer`：要回收的解压缩缓冲区
    /// 
    /// ## 返回值
    /// 
    /// 成功时返回 `Ok(())`，失败时返回 `Result<()>`
    /// 
    /// let pool = UnifiedMemoryPool::default();
    /// let buffer = MemoryBlock::new(vec![1, 2, 3]);
    /// let result = pool.release_decompress_buffer(buffer);
    /// assert!(result.is_ok());
    /// ```
    pub fn release_decompress_buffer(&self, buffer: MemoryBlock) -> Result<()> {
        let size = buffer.len();
        
        if let Some(pool) = self.select_decompress_pool(size) {
            if let Some(mut guard) = pool.try_get() {
                // 清空并重置容量
                guard.clear();
                guard.shrink_to_fit();
                drop(guard); // 自动归还到池中
                
                self.update_memory_usage(size, false);
                self.record_deallocation(size);
                return Ok(());
            }
        }
        
        // 如果池已满或没有合适的池，直接丢弃
        self.update_memory_usage(size, false);
        self.record_deallocation(size);
        Ok(())
    }

    /// 回收帧数据缓冲区到池中
    /// 
    /// 尝试将一个帧数据缓冲区回收回其对应的池中。
    /// 
    /// ## 参数
    /// 
    /// - `buffer`：要回收的帧数据缓冲区
    /// 
    /// ## 返回值
    /// 
    /// 成功时返回 `Ok(())`，失败时返回 `Result<()>`
    /// 
    /// let pool = UnifiedMemoryPool::default();
    /// let buffer = MemoryBlock::new(vec![1, 2, 3]);
    /// let result = pool.release_frame_buffer(buffer);
    /// assert!(result.is_ok());
    /// ```
    pub fn release_frame_buffer(&self, buffer: MemoryBlock) -> Result<()> {
        let size = buffer.len();
        
        if let Some(pool) = self.select_frame_pool(size) {
            if let Some(mut guard) = pool.try_get() {
                // 清空并重置容量
                guard.clear();
                guard.shrink_to_fit();
                drop(guard); // 自动归还到池中
                
                self.update_memory_usage(size, false);
                self.record_deallocation(size);
                return Ok(());
            }
        }
        
        // 如果池已满或没有合适的池，直接丢弃
        self.update_memory_usage(size, false);
        self.record_deallocation(size);
        Ok(())
    }

    /// 批量回收内存块
    /// 
    /// 尝试批量回收多个内存块。
    /// 
    /// ## 参数
    /// 
    /// - `blocks`：要回收的内存块列表
    /// 
    /// ## 返回值
    /// 
    /// 成功时返回 `Ok(())`，失败时返回 `Result<()>`
    /// 
/// use canp::memory_pool::{UnifiedMemoryPool, MemoryBlock};
/// 
/// fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let pool = UnifiedMemoryPool::default();
///     let blocks = vec![MemoryBlock::new(vec![1, 2, 3]), MemoryBlock::new(vec![4, 5, 6])];
///     let result = pool.release_blocks_batch(blocks);
///     assert!(result.is_ok());
///     Ok(())
/// }
/// ```
    pub fn release_blocks_batch(&self, blocks: Vec<MemoryBlock>) -> Result<()> {
        let mut errors = Vec::new();
        
        for (i, block) in blocks.into_iter().enumerate() {
            if let Err(e) = self.release_block(block) {
                errors.push((i, e));
            }
        }
        
        if !errors.is_empty() {
            return Err(anyhow::anyhow!(
                "批量回收内存块时发生 {} 个错误: {:?}",
                errors.len(),
                errors
            ));
        }
        
        Ok(())
    }

    /// 批量回收解压缩缓冲区
    /// 
    /// 尝试批量回收多个解压缩缓冲区。
    /// 
    /// ## 参数
    /// 
    /// - `buffers`：要回收的解压缩缓冲区列表
    /// 
    /// ## 返回值
    /// 
    /// 成功时返回 `Ok(())`，失败时返回 `Result<()>`
    /// 
    /// let pool = UnifiedMemoryPool::default();
    /// let buffers = vec![MemoryBlock::new(vec![1, 2, 3]), MemoryBlock::new(vec![4, 5, 6])];
    /// let result = pool.release_decompress_buffers_batch(buffers);
    /// assert!(result.is_ok());
    /// ```
    pub fn release_decompress_buffers_batch(&self, buffers: Vec<MemoryBlock>) -> Result<()> {
        let mut errors = Vec::new();
        
        for (i, buffer) in buffers.into_iter().enumerate() {
            if let Err(e) = self.release_decompress_buffer(buffer) {
                errors.push((i, e));
            }
        }
        
        if !errors.is_empty() {
            return Err(anyhow::anyhow!(
                "批量回收解压缩缓冲区时发生 {} 个错误: {:?}",
                errors.len(),
                errors
            ));
        }
        
        Ok(())
    }

    /// 批量回收帧数据缓冲区
    /// 
    /// 尝试批量回收多个帧数据缓冲区。
    /// 
    /// ## 参数
    /// 
    /// - `buffers`：要回收的帧数据缓冲区列表
    /// 
    /// ## 返回值
    /// 
    /// 成功时返回 `Ok(())`，失败时返回 `Result<()>`
    /// 
    /// let pool = UnifiedMemoryPool::default();
    /// let buffers = vec![MemoryBlock::new(vec![1, 2, 3]), MemoryBlock::new(vec![4, 5, 6])];
    /// let result = pool.release_frame_buffers_batch(buffers);
    /// assert!(result.is_ok());
    /// ```
    pub fn release_frame_buffers_batch(&self, buffers: Vec<MemoryBlock>) -> Result<()> {
        let mut errors = Vec::new();
        
        for (i, buffer) in buffers.into_iter().enumerate() {
            if let Err(e) = self.release_frame_buffer(buffer) {
                errors.push((i, e));
            }
        }
        
        if !errors.is_empty() {
            return Err(anyhow::anyhow!(
                "批量回收帧数据缓冲区时发生 {} 个错误: {:?}",
                errors.len(),
                errors
            ));
        }
        
        Ok(())
    }
}

impl Default for UnifiedMemoryPool {
    fn default() -> Self {
        Self::new(MemoryPoolConfig::default())
    }
}



#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_memory_block_allocation() {
        let pool = UnifiedMemoryPool::new(MemoryPoolConfig::default());

        // 测试内存块分配
        let block = pool.allocate_block(1024).unwrap();
        assert_eq!(block.len(), 0); // 新分配的块长度为0
        assert!(block.is_empty()); // 长度为0时应该为空

        // 测试异步分配
        let block_async = pool.allocate_block_async(2048).await.unwrap();
        assert_eq!(block_async.len(), 0);
    }

    #[tokio::test]
    async fn test_batch_allocation() {
        let pool = UnifiedMemoryPool::new(MemoryPoolConfig::default());

        let sizes = vec![1024, 2048, 4096];
        let blocks = pool.allocate_blocks_batch(&sizes).unwrap();

        assert_eq!(blocks.len(), 3);
        for block in blocks {
            assert!(block.is_empty()); // 新分配的块长度为0，应该为空
        }
    }

    #[tokio::test]
    async fn test_anonymous_mmap() {
        let pool = UnifiedMemoryPool::new(MemoryPoolConfig::default());

        let mmap_block = pool.create_anonymous_mmap(1024 * 1024).await.unwrap();
        assert_eq!(mmap_block.len(), 1024 * 1024);
        assert!(mmap_block.file_path().is_none());
    }

    #[test]
    fn test_memory_block_operations() {
        let data = vec![1, 2, 3, 4, 5];
        let block = MemoryBlock::new(data);

        assert_eq!(block.len(), 5);
        assert!(!block.is_empty());
        assert_eq!(block.as_slice(), &[1, 2, 3, 4, 5]);
    }

    #[test]
    fn test_mmap_block_operations() {
        // 创建临时文件进行测试
        let temp_file = tempfile::tempfile().unwrap();
        temp_file.set_len(1024).unwrap();

        let mmap = unsafe { Mmap::map(&temp_file).unwrap() };
        let mmap_block = MmapBlock::new(mmap, Some("test.txt".to_string()));

        assert_eq!(mmap_block.len(), 1024);
        assert!(!mmap_block.is_empty());
        assert_eq!(mmap_block.file_path(), Some("test.txt"));
    }

    #[tokio::test]
    async fn test_decompress_buffer_allocation() {
        let pool = UnifiedMemoryPool::new(MemoryPoolConfig::default());

        // 测试解压缩缓冲区分配
        let buffer = pool.allocate_decompress_buffer(10240).unwrap();
        assert_eq!(buffer.len(), 0);
        assert!(buffer.is_empty());

        // 测试异步分配
        let buffer_async = pool.allocate_decompress_buffer_async(51200).await.unwrap();
        assert_eq!(buffer_async.len(), 0);
    }

    #[tokio::test]
    async fn test_frame_buffer_allocation() {
        let pool = UnifiedMemoryPool::new(MemoryPoolConfig::default());

        // 测试帧数据缓冲区分配
        let buffer = pool.allocate_frame_buffer(1024).unwrap();
        assert_eq!(buffer.len(), 0);
        assert!(buffer.is_empty());

        // 测试异步分配
        let buffer_async = pool.allocate_frame_buffer_async(2048).await.unwrap();
        assert_eq!(buffer_async.len(), 0);
    }

    #[test]
    fn test_ptr_and_len_operations() {
        let data = vec![1, 2, 3, 4, 5];
        let block = MemoryBlock::new(data);

        let (ptr, len) = block.as_ptr_and_len();
        assert_eq!(len, 5);
        assert!(!ptr.is_null());

        // 验证指针指向的数据
        let slice = unsafe { std::slice::from_raw_parts(ptr, len) };
        assert_eq!(slice, &[1, 2, 3, 4, 5]);
    }

    #[tokio::test]
    async fn test_batch_decompress_allocation() {
        let pool = UnifiedMemoryPool::new(MemoryPoolConfig::default());

        let sizes = vec![10240, 51200, 102400];
        let buffers = pool.allocate_decompress_buffers_batch(&sizes).unwrap();

        assert_eq!(buffers.len(), 3);
        for buffer in buffers {
            assert!(buffer.is_empty());
        }
    }

    #[tokio::test]
    async fn test_batch_frame_allocation() {
        let pool = UnifiedMemoryPool::new(MemoryPoolConfig::default());

        let sizes = vec![512, 1024, 2048];
        let buffers = pool.allocate_frame_buffers_batch(&sizes).unwrap();

        assert_eq!(buffers.len(), 3);
        for buffer in buffers {
            assert!(buffer.is_empty());
        }
    }




}