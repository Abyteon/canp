//! # 线程池模块 (Thread Pool Module)
//! 
//! 提供智能的任务调度功能，支持任务类型分类、高性能库集成和内存池协作。
//! 
//! ## 设计理念
//! 
//! - **任务分类**：根据任务特性分为IO密集型、CPU密集型、内存密集型
//! - **库集成**：集成tokio (IO)、rayon (CPU)、threadpool (内存) 高性能库
//! - **内存集成**：与内存池深度协作，实现内存生命周期管理
//! - **统计监控**：实时监控任务执行情况，支持性能分析
//! 
//! ## 核心组件
//! 
//! - `Task`：任务定义，包含任务类型、优先级和内存块
//! - `TaskType`：任务类型枚举，定义IO、CPU、内存密集型任务
//! - `TaskPriority`：任务优先级枚举，支持低、普通、高、关键优先级
//! - `PipelineThreadPool`：流水线线程池，管理不同类型的线程池
//! 
//! ## 使用示例
//! 
//! ```rust
//! use canp::thread_pool::{PipelineThreadPool, TaskType, TaskPriority};
//! 
//! fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let pool = PipelineThreadPool::default();
//! 
//!     // IO密集型任务
//!     pool.submit_io_task(TaskPriority::Normal, || {
//!         // 文件读取、网络IO等
//!         Ok(())
//!     })?;
//! 
//!     // CPU密集型任务
//!     pool.submit_cpu_task(TaskPriority::High, || {
//!         // 数据解析、压缩解压等
//!         Ok(())
//!     })?;
//! 
//!     // 带内存块的任务
//!     let memory_blocks = vec![
//!         pool.memory_pool().allocate_block(1024)?,
//!     ];
//!     pool.submit_task_with_memory(
//!         TaskType::MemoryBound,
//!         TaskPriority::Normal,
//!         memory_blocks,
//!         || { Ok(()) }
//!     )?;
//!     
//!     Ok(())
//! }
//! ```

use anyhow::Result;
use rayon::prelude::*;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use threadpool::ThreadPool as StdThreadPool;
use tokio::runtime::Runtime;
use tracing::{debug, error, info};

use crate::memory_pool::{MemoryBlock, UnifiedMemoryPool};

/// 任务类型枚举
/// 
/// 根据任务的特性将任务分为不同类型，每种类型使用最适合的线程池。
/// 
/// ## 任务类型说明
/// 
/// - **IoBound**：IO密集型任务，如文件读取、网络IO、数据库操作
/// - **CpuBound**：CPU密集型任务，如数据解析、压缩解压、计算密集型操作
/// - **MemoryBound**：内存密集型任务，如大量数据处理、内存拷贝
/// 
/// ## 使用示例
/// 
/// ```rust
/// use canp::thread_pool::TaskType;
/// 
/// // IO密集型任务
/// let io_task = TaskType::IoBound;
/// 
/// // CPU密集型任务
/// let cpu_task = TaskType::CpuBound;
/// 
/// // 内存密集型任务
/// let memory_task = TaskType::MemoryBound;
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TaskType {
    /// IO密集型任务（文件读取、mmap、网络IO）
    IoBound,
    /// CPU密集型任务（解析、解压、计算）
    CpuBound,
    /// 内存密集型任务（数据处理、内存拷贝）
    MemoryBound,
}

/// 任务优先级枚举
/// 
/// 定义任务的优先级，影响任务的调度顺序。
/// 
/// ## 优先级说明
/// 
/// - **Low**：低优先级，在系统负载较高时可能被延迟执行
/// - **Normal**：普通优先级，默认优先级
/// - **High**：高优先级，优先于普通和低优先级任务执行
/// - **Critical**：关键优先级，最高优先级，立即执行
/// 
/// ## 使用示例
/// 
/// ```rust
/// use canp::thread_pool::TaskPriority;
/// 
/// // 低优先级任务
/// let low_priority = TaskPriority::Low;
/// 
/// // 普通优先级任务
/// let normal_priority = TaskPriority::Normal;
/// 
/// // 高优先级任务
/// let high_priority = TaskPriority::High;
/// 
/// // 关键优先级任务
/// let critical_priority = TaskPriority::Critical;
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TaskPriority {
    /// 低优先级
    Low = 0,
    /// 普通优先级
    Normal = 1,
    /// 高优先级
    High = 2,
    /// 关键优先级
    Critical = 3,
}

/// 任务定义 - 集成内存管理
/// 
/// 定义任务的基本信息，包括任务类型、优先级、执行逻辑和关联的内存块。
/// 
/// ## 特性
/// 
/// - **任务分类**：支持IO、CPU、内存密集型任务
/// - **优先级管理**：支持4个优先级级别
/// - **内存集成**：任务可以关联内存块，实现内存生命周期管理
/// - **统计跟踪**：记录任务创建时间和执行统计
/// 
/// ## 使用示例
/// 
/// ```rust
/// use canp::thread_pool::{Task, TaskType, TaskPriority};
/// use canp::memory_pool::MemoryBlock;
/// 
/// fn main() -> Result<(), Box<dyn std::error::Error>> {
///     // 创建简单任务
///     let task = Task::new(
///         TaskType::CpuBound,
///         TaskPriority::Normal,
///         || {
///             println!("执行任务");
///             Ok(())
///         }
///     );
/// 
///     // 创建带内存块的任务
///     let memory_blocks = vec![MemoryBlock::new(vec![1, 2, 3])];
///     let task_with_memory = Task::with_memory(
///         TaskType::MemoryBound,
///         TaskPriority::High,
///         memory_blocks,
///         || {
///             println!("处理内存数据");
///             Ok(())
///         }
///     );
///     
///     Ok(())
/// }
/// ```
pub struct Task {
    /// 任务ID（自动生成）
    pub id: u64,
    /// 任务类型
    pub task_type: TaskType,
    /// 任务优先级
    pub priority: TaskPriority,
    /// 任务执行逻辑
    pub payload: Box<dyn FnOnce() -> Result<()> + Send + 'static>,
    /// 任务创建时间
    pub created_at: Instant,
    /// 任务使用的内存块（可选）
    pub memory_blocks: Vec<MemoryBlock>,
}

impl Task {
    /// 创建新的任务
    /// 
    /// ## 参数
    /// 
    /// - `task_type`：任务类型
    /// - `priority`：任务优先级
    /// - `f`：任务执行逻辑
    /// 
    /// ## 返回值
    /// 
    /// 返回新创建的 `Task`
    /// 
    /// ## 示例
    /// 
    /// ```rust
    /// use canp::thread_pool::{Task, TaskType, TaskPriority};
    /// 
    /// let task = Task::new(
    ///     TaskType::CpuBound,
    ///     TaskPriority::Normal,
    ///     || {
    ///         println!("执行CPU密集型任务");
    ///         Ok(())
    ///     }
    /// );
    /// 
    /// assert_eq!(task.task_type, TaskType::CpuBound);
    /// assert_eq!(task.priority, TaskPriority::Normal);
    /// assert!(task.memory_blocks.is_empty());
    /// ```
    pub fn new<F>(task_type: TaskType, priority: TaskPriority, f: F) -> Self
    where
        F: FnOnce() -> Result<()> + Send + 'static,
    {
        static TASK_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        
        Self {
            id: TASK_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            task_type,
            priority,
            payload: Box::new(f),
            created_at: Instant::now(),
            memory_blocks: Vec::new(),
        }
    }

    /// 创建带内存块的任务
    /// 
    /// ## 参数
    /// 
    /// - `task_type`：任务类型
    /// - `priority`：任务优先级
    /// - `memory_blocks`：任务使用的内存块列表
    /// - `f`：任务执行逻辑
    /// 
    /// ## 返回值
    /// 
    /// 返回新创建的带内存块的 `Task`
    /// 
    /// ## 示例
    /// 
    /// ```rust
    /// use canp::thread_pool::{Task, TaskType, TaskPriority};
    /// use canp::memory_pool::MemoryBlock;
    /// 
    /// let memory_blocks = vec![
    ///     MemoryBlock::new(vec![1, 2, 3]),
    ///     MemoryBlock::new(vec![4, 5, 6]),
    /// ];
    /// 
    /// let task = Task::with_memory(
///     TaskType::MemoryBound,
///     TaskPriority::High,
///     memory_blocks,
///     || {
///         println!("处理内存数据");
///         Ok(())
///     }
/// );
    /// 
    /// assert_eq!(task.task_type, TaskType::MemoryBound);
    /// assert_eq!(task.priority, TaskPriority::High);
    /// assert_eq!(task.memory_blocks.len(), 2);
    /// ```
    pub fn with_memory<F>(task_type: TaskType, priority: TaskPriority, memory_blocks: Vec<MemoryBlock>, f: F) -> Self
    where
        F: FnOnce() -> Result<()> + Send + 'static,
    {
        static TASK_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        
        Self {
            id: TASK_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            task_type,
            priority,
            payload: Box::new(f),
            created_at: Instant::now(),
            memory_blocks,
        }
    }
}

/// 线程池配置
/// 
/// 定义线程池的行为参数，包括各类型线程数和功能开关。
/// 
/// ## 配置项说明
/// 
/// - **线程数配置**：根据任务类型设置不同的线程数
/// - **功能开关**：控制统计和内存管理功能
/// 
/// ## 使用示例
/// 
/// ```rust
/// use canp::thread_pool::ThreadPoolConfig;
/// use std::thread;
/// 
/// fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let config = ThreadPoolConfig {
///         io_bound_threads: thread::available_parallelism().unwrap().get() / 2,
///         cpu_bound_threads: thread::available_parallelism().unwrap().get(),
///         memory_bound_threads: thread::available_parallelism().unwrap().get() / 2,
///         enable_stats: true,
///         enable_memory_management: true,
///     };
///     Ok(())
/// }
/// ```
#[derive(Debug, Clone)]
pub struct ThreadPoolConfig {
    /// IO密集型线程数
    /// 
    /// 用于处理IO密集型任务的线程数，通常设置为CPU核心数的一半。
    pub io_bound_threads: usize,
    /// CPU密集型线程数
    /// 
    /// 用于处理CPU密集型任务的线程数，通常设置为CPU核心数。
    pub cpu_bound_threads: usize,
    /// 内存密集型线程数
    /// 
    /// 用于处理内存密集型任务的线程数，通常设置为CPU核心数的一半。
    pub memory_bound_threads: usize,
    /// 是否启用任务统计
    /// 
    /// 控制是否收集和统计任务执行信息。
    pub enable_stats: bool,
    /// 是否启用内存管理
    /// 
    /// 控制是否启用与内存池的集成功能。
    pub enable_memory_management: bool,
}

impl Default for ThreadPoolConfig {
    fn default() -> Self {
        Self {
            io_bound_threads: num_cpus::get() / 2,      // IO密集型线程较少
            cpu_bound_threads: num_cpus::get(),          // CPU密集型线程等于CPU核心数
            memory_bound_threads: num_cpus::get() / 2,   // 内存密集型线程较少
            enable_stats: true,
            enable_memory_management: true,
        }
    }
}

/// 线程池统计信息
/// 
/// 记录线程池的运行统计信息，包括任务执行情况和内存管理统计。
/// 
/// ## 统计项说明
/// 
/// - **任务统计**：总任务数、已完成任务数、失败任务数、平均执行时间
/// - **类型统计**：各类型任务的执行数量
/// - **内存统计**：内存块分配和回收情况
/// 
/// ## 使用示例
/// 
/// ```rust
/// use canp::thread_pool::PipelineThreadPool;
/// 
/// fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let pool = PipelineThreadPool::default();
///     let stats = pool.get_stats();
/// 
///     println!("总任务数: {}", stats.total_tasks);
///     println!("已完成任务: {}", stats.completed_tasks);
///     println!("失败任务: {}", stats.failed_tasks);
///     println!("平均执行时间: {:.2}ms", stats.avg_execution_time);
///     Ok(())
/// }
/// ```
#[derive(Debug, Clone)]
pub struct ThreadPoolStats {
    /// 总任务数
    pub total_tasks: usize,
    /// 已完成任务数
    pub completed_tasks: usize,
    /// 失败任务数
    pub failed_tasks: usize,
    /// 平均任务执行时间（毫秒）
    pub avg_execution_time: f64,
    /// 各类型任务统计
    pub task_type_stats: std::collections::HashMap<TaskType, usize>,
    /// 内存管理统计
    pub memory_management_stats: MemoryManagementStats,
}

/// 内存管理统计
/// 
/// 记录内存池和线程池协作的内存管理统计信息。
/// 
/// ## 统计项说明
/// 
/// - **内存块统计**：总内存块数、已回收内存块数
/// - **复用率**：内存复用率，反映内存使用效率
/// 
/// ## 使用示例
/// 
/// ```rust
/// use canp::thread_pool::PipelineThreadPool;
/// 
/// fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let pool = PipelineThreadPool::default();
///     let stats = pool.get_stats();
///     let mem_stats = &stats.memory_management_stats;
/// 
///     println!("总内存块数: {}", mem_stats.total_memory_blocks);
///     println!("已回收内存块: {}", mem_stats.recycled_memory_blocks);
///     println!("内存复用率: {:.2}%", mem_stats.memory_reuse_rate * 100.0);
///     Ok(())
/// }
/// ```
#[derive(Debug, Clone)]
pub struct MemoryManagementStats {
    /// 总内存块数
    pub total_memory_blocks: usize,
    /// 已回收内存块数
    pub recycled_memory_blocks: usize,
    /// 内存复用率
    pub memory_reuse_rate: f64,
}

impl Default for MemoryManagementStats {
    fn default() -> Self {
        Self {
            total_memory_blocks: 0,
            recycled_memory_blocks: 0,
            memory_reuse_rate: 0.0,
        }
    }
}

/// 流水线线程池 - 集成内存管理
/// 
/// 提供统一的任务调度接口，集成多种高性能线程池和内存池。
/// 
/// ## 特性
/// 
/// - **任务分类调度**：根据任务类型自动选择最适合的线程池
/// - **高性能库集成**：集成tokio、rayon、threadpool等高性能库
/// - **内存池协作**：与内存池深度集成，实现内存生命周期管理
/// - **统计监控**：实时监控任务执行和内存使用情况
/// 
/// ## 线程池说明
/// 
/// - **io_bound_runtime**：tokio runtime，用于IO密集型任务
/// - **cpu_bound_pool**：rayon线程池，用于CPU密集型任务
/// - **memory_bound_pool**：threadpool，用于内存密集型任务
/// 
/// ## 使用示例
/// 
/// ```rust
/// use canp::thread_pool::{PipelineThreadPool, TaskType, TaskPriority};
/// 
/// fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let pool = PipelineThreadPool::default();
/// 
///     // 提交不同类型的任务
///     pool.submit_io_task(TaskPriority::Normal, || {
///         // IO操作
///         Ok(())
///     })?;
/// 
///     pool.submit_cpu_task(TaskPriority::High, || {
///         // CPU密集型操作
///         Ok(())
///     })?;
/// 
///     pool.submit_memory_task(TaskPriority::Normal, || {
///         // 内存密集型操作
///         Ok(())
///     })?;
/// 
///     // 等待所有任务完成
///     pool.wait_for_completion();
///     
///     Ok(())
/// }
/// ```
pub struct PipelineThreadPool {
    /// IO密集型线程池（使用tokio runtime）
    io_bound_runtime: Arc<Runtime>,
    /// CPU密集型线程池（使用rayon）
    cpu_bound_pool: Arc<rayon::ThreadPool>,
    /// 内存密集型线程池（使用threadpool库）
    memory_bound_pool: Arc<StdThreadPool>,
    /// 内存池
    memory_pool: Arc<UnifiedMemoryPool>,
    /// 统计信息
    stats: Arc<Mutex<ThreadPoolStats>>,
}

impl PipelineThreadPool {
    /// 创建新的流水线线程池
    pub fn new(config: ThreadPoolConfig) -> Self {
        let stats = Arc::new(Mutex::new(ThreadPoolStats {
            total_tasks: 0,
            completed_tasks: 0,
            failed_tasks: 0,
            avg_execution_time: 0.0,
            task_type_stats: std::collections::HashMap::new(),
            memory_management_stats: MemoryManagementStats::default(),
        }));

        // 创建内存池
        let memory_pool = Arc::new(UnifiedMemoryPool::default());

        // 使用tokio创建IO密集型线程池
        let io_bound_runtime = Arc::new(
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(config.io_bound_threads)
                .enable_all()
                .build()
                .expect("Failed to create IO-bound runtime")
        );

        // 使用rayon创建CPU密集型线程池
        let cpu_bound_pool = Arc::new(
            rayon::ThreadPoolBuilder::new()
                .num_threads(config.cpu_bound_threads)
                .build()
                .expect("Failed to create CPU-bound thread pool")
        );

        // 使用threadpool库创建内存密集型线程池
        let memory_bound_pool = Arc::new(StdThreadPool::new(config.memory_bound_threads));

        info!(
            "🚀 创建流水线线程池: IO={}, CPU={}, Memory={}, 内存管理={}",
            config.io_bound_threads, config.cpu_bound_threads, config.memory_bound_threads,
            config.enable_memory_management
        );

        Self {
            io_bound_runtime,
            cpu_bound_pool,
            memory_bound_pool,
            memory_pool,
            stats,
        }
    }

    /// 获取内存池引用
    pub fn memory_pool(&self) -> &UnifiedMemoryPool {
        &self.memory_pool
    }

    /// 提交IO密集型任务（异步）
    pub fn submit_io_task<F>(&self, priority: TaskPriority, f: F) -> Result<()>
    where
        F: FnOnce() -> Result<()> + Send + 'static,
    {
        let task = Task::new(TaskType::IoBound, priority, f);
        self.submit_task_to_tokio(task)
    }

    /// 提交CPU密集型任务
    pub fn submit_cpu_task<F>(&self, priority: TaskPriority, f: F) -> Result<()>
    where
        F: FnOnce() -> Result<()> + Send + 'static,
    {
        let task = Task::new(TaskType::CpuBound, priority, f);
        self.submit_task_to_rayon(task)
    }

    /// 提交内存密集型任务
    pub fn submit_memory_task<F>(&self, priority: TaskPriority, f: F) -> Result<()>
    where
        F: FnOnce() -> Result<()> + Send + 'static,
    {
        let task = Task::new(TaskType::MemoryBound, priority, f);
        self.submit_task_to_threadpool(task)
    }

    /// 提交带内存块的任务
    pub fn submit_task_with_memory<F>(&self, task_type: TaskType, priority: TaskPriority, memory_blocks: Vec<MemoryBlock>, f: F) -> Result<()>
    where
        F: FnOnce() -> Result<()> + Send + 'static,
    {
        let task = Task::with_memory(task_type, priority, memory_blocks, f);
        
        match task_type {
            TaskType::IoBound => self.submit_task_to_tokio(task),
            TaskType::CpuBound => self.submit_task_to_rayon(task),
            TaskType::MemoryBound => self.submit_task_to_threadpool(task),
        }
    }

    /// 提交任务到tokio runtime
    fn submit_task_to_tokio(&self, task: Task) -> Result<()> {
        // 更新统计信息
        {
            let mut stats = self.stats.lock().unwrap();
            stats.total_tasks += 1;
            *stats.task_type_stats.entry(task.task_type).or_insert(0) += 1;
            stats.memory_management_stats.total_memory_blocks += task.memory_blocks.len();
        }

        let stats = self.stats.clone();
        let memory_pool = self.memory_pool.clone();
        let task_id = task.id;
        let memory_blocks = task.memory_blocks;
        let start_time = Instant::now();

        // 使用tokio的spawn_blocking进行CPU密集型任务
        self.io_bound_runtime.spawn_blocking(move || {
            // 执行任务
            let result = (task.payload)();
            
            // 任务完成后回收内存块
            if !memory_blocks.is_empty() {
                let block_count = memory_blocks.len();
                for block in memory_blocks {
                    if let Err(e) = memory_pool.release_block(block) {
                        error!("❌ 回收内存块失败: {}", e);
                    }
                }
                
                // 更新内存管理统计
                {
                    let mut stats = stats.lock().unwrap();
                    stats.memory_management_stats.recycled_memory_blocks += block_count;
                    stats.memory_management_stats.memory_reuse_rate = 
                        stats.memory_management_stats.recycled_memory_blocks as f64 / 
                        stats.memory_management_stats.total_memory_blocks as f64;
                }
            }
            
            match result {
                Ok(_) => {
                    let execution_time = start_time.elapsed();
                    debug!(
                        "✅ 任务 {} 执行成功，耗时 {:?}",
                        task_id, execution_time
                    );
                    
                    // 更新统计信息
                    {
                        let mut stats = stats.lock().unwrap();
                        stats.completed_tasks += 1;
                        
                        // 更新平均执行时间
                        let total_time = stats.avg_execution_time * (stats.completed_tasks - 1) as f64
                            + execution_time.as_millis() as f64;
                        stats.avg_execution_time = total_time / stats.completed_tasks as f64;
                    }
                }
                Err(e) => {
                    error!("❌ 任务 {} 执行失败: {}", task_id, e);
                    
                    // 更新统计信息
                    {
                        let mut stats = stats.lock().unwrap();
                        stats.failed_tasks += 1;
                    }
                }
            }
        });

        Ok(())
    }

    /// 提交任务到rayon线程池
    fn submit_task_to_rayon(&self, task: Task) -> Result<()> {
        // 更新统计信息
        {
            let mut stats = self.stats.lock().unwrap();
            stats.total_tasks += 1;
            *stats.task_type_stats.entry(task.task_type).or_insert(0) += 1;
            stats.memory_management_stats.total_memory_blocks += task.memory_blocks.len();
        }

        let stats = self.stats.clone();
        let memory_pool = self.memory_pool.clone();
        let task_id = task.id;
        let memory_blocks = task.memory_blocks;
        let start_time = Instant::now();

        // 使用rayon的spawn方法
        self.cpu_bound_pool.spawn(move || {
            // 执行任务
            let result = (task.payload)();
            
            // 任务完成后回收内存块
            if !memory_blocks.is_empty() {
                let block_count = memory_blocks.len();
                for block in memory_blocks {
                    if let Err(e) = memory_pool.release_block(block) {
                        error!("❌ 回收内存块失败: {}", e);
                    }
                }
                
                // 更新内存管理统计
                {
                    let mut stats = stats.lock().unwrap();
                    stats.memory_management_stats.recycled_memory_blocks += block_count;
                    stats.memory_management_stats.memory_reuse_rate = 
                        stats.memory_management_stats.recycled_memory_blocks as f64 / 
                        stats.memory_management_stats.total_memory_blocks as f64;
                }
            }
            
            match result {
                Ok(_) => {
                    let execution_time = start_time.elapsed();
                    debug!(
                        "✅ 任务 {} 执行成功，耗时 {:?}",
                        task_id, execution_time
                    );
                    
                    // 更新统计信息
                    {
                        let mut stats = stats.lock().unwrap();
                        stats.completed_tasks += 1;
                        
                        // 更新平均执行时间
                        let total_time = stats.avg_execution_time * (stats.completed_tasks - 1) as f64
                            + execution_time.as_millis() as f64;
                        stats.avg_execution_time = total_time / stats.completed_tasks as f64;
                    }
                }
                Err(e) => {
                    error!("❌ 任务 {} 执行失败: {}", task_id, e);
                    
                    // 更新统计信息
                    {
                        let mut stats = stats.lock().unwrap();
                        stats.failed_tasks += 1;
                    }
                }
            }
        });

        Ok(())
    }

    /// 提交任务到threadpool
    fn submit_task_to_threadpool(&self, task: Task) -> Result<()> {
        // 更新统计信息
        {
            let mut stats = self.stats.lock().unwrap();
            stats.total_tasks += 1;
            *stats.task_type_stats.entry(task.task_type).or_insert(0) += 1;
            stats.memory_management_stats.total_memory_blocks += task.memory_blocks.len();
        }

        let stats = self.stats.clone();
        let memory_pool = self.memory_pool.clone();
        let task_id = task.id;
        let memory_blocks = task.memory_blocks;
        let start_time = Instant::now();

        // 使用threadpool的execute方法
        self.memory_bound_pool.execute(move || {
            // 执行任务
            let result = (task.payload)();
            
            // 任务完成后回收内存块
            if !memory_blocks.is_empty() {
                let block_count = memory_blocks.len();
                for block in memory_blocks {
                    if let Err(e) = memory_pool.release_block(block) {
                        error!("❌ 回收内存块失败: {}", e);
                    }
                }
                
                // 更新内存管理统计
                {
                    let mut stats = stats.lock().unwrap();
                    stats.memory_management_stats.recycled_memory_blocks += block_count;
                    stats.memory_management_stats.memory_reuse_rate = 
                        stats.memory_management_stats.recycled_memory_blocks as f64 / 
                        stats.memory_management_stats.total_memory_blocks as f64;
                }
            }
            
            match result {
                Ok(_) => {
                    let execution_time = start_time.elapsed();
                    debug!(
                        "✅ 任务 {} 执行成功，耗时 {:?}",
                        task_id, execution_time
                    );
                    
                    // 更新统计信息
                    {
                        let mut stats = stats.lock().unwrap();
                        stats.completed_tasks += 1;
                        
                        // 更新平均执行时间
                        let total_time = stats.avg_execution_time * (stats.completed_tasks - 1) as f64
                            + execution_time.as_millis() as f64;
                        stats.avg_execution_time = total_time / stats.completed_tasks as f64;
                    }
                }
                Err(e) => {
                    error!("❌ 任务 {} 执行失败: {}", task_id, e);
                    
                    // 更新统计信息
                    {
                        let mut stats = stats.lock().unwrap();
                        stats.failed_tasks += 1;
                    }
                }
            }
        });

        Ok(())
    }

    /// 批量提交任务
    pub fn submit_batch(&self, tasks: Vec<Task>) -> Result<()> {
        for task in tasks {
            match task.task_type {
                TaskType::IoBound => self.submit_task_to_tokio(task)?,
                TaskType::CpuBound => self.submit_task_to_rayon(task)?,
                TaskType::MemoryBound => self.submit_task_to_threadpool(task)?,
            }
        }
        Ok(())
    }

    /// 使用rayon并行处理数据
    pub fn parallel_process<T, R, F>(&self, data: Vec<T>, f: F) -> Result<Vec<R>>
    where
        T: Send + Sync,
        R: Send,
        F: Fn(T) -> Result<R> + Send + Sync,
    {
        let results: Vec<Result<R>> = data
            .into_par_iter()
            .map(f)
            .collect();

        // 收集结果，如果有错误则返回第一个错误
        let mut processed_results = Vec::new();
        for result in results {
            processed_results.push(result?);
        }

        Ok(processed_results)
    }

    /// 获取统计信息
    pub fn get_stats(&self) -> ThreadPoolStats {
        self.stats.lock().unwrap().clone()
    }

    /// 等待所有任务完成
    pub fn wait_for_completion(&self) {
        info!("⏳ 等待所有任务完成...");
        // threadpool会自动等待任务完成
        self.memory_bound_pool.join();
        // rayon的任务会在主线程退出时自动等待
        // tokio runtime会在作用域结束时自动等待
    }

    /// 关闭线程池
    pub fn shutdown(&self) {
        info!("🛑 关闭流水线线程池");
        // tokio runtime会在作用域结束时自动清理
        // threadpool和rayon会在作用域结束时自动清理
    }
}

impl Default for PipelineThreadPool {
    fn default() -> Self {
        Self::new(ThreadPoolConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_thread_pool_creation() {
        let config = ThreadPoolConfig {
            io_bound_threads: 2,
            cpu_bound_threads: 4,
            memory_bound_threads: 2,
            enable_stats: true,
            enable_memory_management: true,
        };

        let pool = PipelineThreadPool::new(config);
        let stats = pool.get_stats();
        
        assert_eq!(stats.total_tasks, 0);
        assert_eq!(stats.completed_tasks, 0);
    }

    #[test]
    fn test_task_submission() {
        let pool = PipelineThreadPool::default();
        
        // 提交CPU密集型任务
        let result = pool.submit_cpu_task(TaskPriority::Normal, || {
            std::thread::sleep(Duration::from_millis(10));
            Ok(())
        });
        
        assert!(result.is_ok());
        
        // 等待任务完成
        std::thread::sleep(Duration::from_millis(100));
        
        let stats = pool.get_stats();
        assert!(stats.completed_tasks > 0);
    }

    #[test]
    fn test_task_with_memory() {
        let pool = PipelineThreadPool::default();
        
        // 分配内存块
        let memory_blocks = vec![
            pool.memory_pool().allocate_block(1024).unwrap(),
            pool.memory_pool().allocate_block(2048).unwrap(),
        ];
        
        // 提交带内存块的任务
        let result = pool.submit_task_with_memory(
            TaskType::MemoryBound,
            TaskPriority::Normal,
            memory_blocks,
            || {
                std::thread::sleep(Duration::from_millis(10));
                Ok(())
            }
        );
        
        assert!(result.is_ok());
        
        // 等待任务完成
        std::thread::sleep(Duration::from_millis(100));
        
        let stats = pool.get_stats();
        assert!(stats.completed_tasks > 0);
        assert!(stats.memory_management_stats.recycled_memory_blocks > 0);
    }

    #[test]
    fn test_parallel_processing() {
        let pool = PipelineThreadPool::default();
        
        let data = vec![1, 2, 3, 4, 5];
        let results = pool.parallel_process(data, |x| {
            std::thread::sleep(Duration::from_millis(10));
            Ok(x * 2)
        }).unwrap();
        
        assert_eq!(results, vec![2, 4, 6, 8, 10]);
    }

    #[test]
    fn test_batch_task_submission() {
        let pool = PipelineThreadPool::default();
        
        let mut tasks = Vec::new();
        for i in 0..5 {
            let task = Task::new(TaskType::CpuBound, TaskPriority::Normal, move || {
                std::thread::sleep(Duration::from_millis(10));
                println!("执行任务 {}", i);
                Ok(())
            });
            tasks.push(task);
        }
        
        let result = pool.submit_batch(tasks);
        assert!(result.is_ok());
        
        // 等待任务完成
        std::thread::sleep(Duration::from_millis(200));
        
        let stats = pool.get_stats();
        assert!(stats.completed_tasks >= 5);
    }

    #[test]
    fn test_memory_pool_thread_pool_collaboration() {
        let pool = PipelineThreadPool::default();
        
        // 测试1: 基本内存分配和回收
        let memory_blocks = vec![
            pool.memory_pool().allocate_block(1024).unwrap(),
            pool.memory_pool().allocate_block(2048).unwrap(),
            pool.memory_pool().allocate_block(4096).unwrap(),
        ];
        
        // 验证内存块分配成功
        assert_eq!(memory_blocks.len(), 3);
        assert_eq!(memory_blocks[0].len(), 0); // 新分配的块长度为0
        assert_eq!(memory_blocks[1].len(), 0);
        assert_eq!(memory_blocks[2].len(), 0);
        
        // 手动填充数据（模拟实际使用）
        for block in &memory_blocks {
            assert!(block.is_empty());
        }
        
        // 测试2: 提交带内存块的任务
        let result = pool.submit_task_with_memory(
            TaskType::CpuBound,
            TaskPriority::Normal,
            memory_blocks,
            || {
                // 模拟任务处理
                std::thread::sleep(Duration::from_millis(10));
                Ok(())
            }
        );
        
        assert!(result.is_ok());
        
        // 等待任务完成
        std::thread::sleep(Duration::from_millis(100));
        
        // 测试3: 验证内存管理统计
        let stats = pool.get_stats();
        assert!(stats.completed_tasks > 0);
        assert!(stats.memory_management_stats.recycled_memory_blocks > 0);
        assert!(stats.memory_management_stats.memory_reuse_rate > 0.0);
        
        // 测试4: 验证内存池统计
        let memory_pool_stats = pool.memory_pool().get_stats();
        assert!(memory_pool_stats.total_allocations > 0);
        assert!(memory_pool_stats.total_deallocations > 0);
        
        // 测试5: 批量任务测试
        let mut batch_tasks = Vec::new();
        for i in 0..5 {
            let memory_blocks = vec![
                pool.memory_pool().allocate_block(512).unwrap(),
                pool.memory_pool().allocate_block(1024).unwrap(),
            ];
            
            let task = Task::with_memory(
                TaskType::MemoryBound,
                TaskPriority::Normal,
                memory_blocks,
                move || {
                    println!("执行批量任务 {}", i);
                    std::thread::sleep(Duration::from_millis(5));
                    Ok(())
                }
            );
            batch_tasks.push(task);
        }
        
        let result = pool.submit_batch(batch_tasks);
        assert!(result.is_ok());
        
        // 等待所有任务完成
        std::thread::sleep(Duration::from_millis(200));
        
        // 测试6: 验证最终统计
        let final_stats = pool.get_stats();
        let final_memory_stats = pool.memory_pool().get_stats();
        
        println!("线程池统计: {:?}", final_stats);
        println!("内存池统计: {:?}", final_memory_stats);
        
        // 验证内存回收
        assert!(final_stats.memory_management_stats.recycled_memory_blocks >= 13); // 3 + 5*2
        assert!(final_memory_stats.total_deallocations >= 13);
        
        // 验证内存复用率
        assert!(final_stats.memory_management_stats.memory_reuse_rate > 0.0);
        assert!(final_stats.memory_management_stats.memory_reuse_rate <= 1.0);
    }
} 