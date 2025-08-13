//! # 高性能任务执行器 (High-Performance Task Executor)
//!
//! 专门为大规模数据处理任务设计的高性能执行器，结合了社区最佳实践：
//! - Tokio异步运行时处理IO密集型任务
//! - Rayon数据并行处理CPU密集型任务  
//! - 多生产者多消费者（MPMC）模式提高并发性能
//! - 工作窃取算法提高资源利用率
//! - 背压控制防止内存溢出

use anyhow::Result;
use crossbeam::channel; // 新增：多生产者多消费者通道
use num_cpus;
use rayon::prelude::*;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};
use tokio::sync::{Semaphore, oneshot};
use tokio::task::JoinHandle;
use tracing::{debug, error, info, warn};

/// 任务类型枚举
/// 基于tokio和rayon官方文档的最佳实践
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskType {
    /// IO密集型任务（文件映射、网络IO、磁盘IO）
    IoIntensive,
    /// CPU密集型任务（解压、编码解码、数学计算）
    CpuIntensive,
    /// 混合型任务（数据解析、格式转换）
    Mixed,
    /// 高优先级任务（错误处理、监控）
    HighPriority,
    /// 自定义任务类型
    Custom(u32),
}

impl TaskType {
    /// 获取任务的建议线程池类型
    pub fn suggested_pool(&self) -> &'static str {
        match self {
            TaskType::IoIntensive => "io",
            TaskType::CpuIntensive => "cpu",
            TaskType::Mixed => "mixed",
            TaskType::HighPriority => "priority",
            TaskType::Custom(_) => "custom",
        }
    }

    /// 获取任务的权重（用于负载均衡）
    pub fn weight(&self) -> u32 {
        match self {
            TaskType::IoIntensive => 1,
            TaskType::CpuIntensive => 2,
            TaskType::Mixed => 3,
            TaskType::HighPriority => 10,
            TaskType::Custom(weight) => *weight,
        }
    }

    /// 获取任务的建议超时时间
    pub fn suggested_timeout(&self) -> Duration {
        match self {
            TaskType::HighPriority => Duration::from_secs(60), // 高优先级任务短超时
            TaskType::IoIntensive => Duration::from_secs(300), // IO任务标准超时
            TaskType::CpuIntensive => Duration::from_secs(600), // CPU任务长超时
            TaskType::Mixed => Duration::from_secs(450),       // 混合任务中等超时
            TaskType::Custom(_) => Duration::from_secs(300),   // 自定义任务默认超时
        }
    }

    /// 获取任务的建议批量大小
    pub fn suggested_batch_size(&self) -> usize {
        match self {
            TaskType::HighPriority => 5,  // 高优先级任务小批量
            TaskType::CpuIntensive => 15, // CPU密集型任务大批量
            TaskType::Mixed => 10,        // 混合任务中等批量
            TaskType::IoIntensive => 8,   // IO任务中等批量
            TaskType::Custom(_) => 10,    // 自定义任务默认批量
        }
    }
}

/// 任务优先级
#[derive(Debug, Clone, Copy, Ord, PartialOrd, Eq, PartialEq)]
pub enum Priority {
    Low = 0,
    Normal = 1,
    High = 2,
    Critical = 3,
}

/// 任务元数据
#[derive(Debug)]
pub struct TaskMetadata {
    pub id: u64,
    pub task_type: TaskType,
    pub priority: Priority,
    pub created_at: Instant,
    pub estimated_duration: Option<Duration>,
    pub description: String,
}

/// 任务执行结果
#[derive(Debug)]
pub struct TaskResult<T> {
    pub metadata: TaskMetadata,
    pub result: Result<T>,
    pub execution_time: Duration,
    pub worker_id: Option<usize>,
}

/// 执行器统计信息（MPMC优化版本）
#[derive(Debug, Clone)]
pub struct ExecutorStats {
    pub total_tasks: usize,
    pub completed_tasks: usize,
    pub failed_tasks: usize,
    // 任务类型统计
    pub io_tasks: usize,
    pub cpu_tasks: usize,
    pub mixed_tasks: usize,
    pub high_priority_tasks: usize,

    // 时间统计
    pub average_execution_time: Duration,
    pub total_io_time: Duration,
    pub total_cpu_time: Duration,

    // 工作线程统计
    pub active_io_workers: usize,
    pub active_cpu_workers: usize,
    pub queue_length: usize,

    // 错误统计
    pub timeout_tasks: usize,
    pub queue_full_rejections: usize,
    pub worker_restarts: usize,
}

impl ExecutorStats {
    /// 计算任务成功率
    pub fn success_rate(&self) -> f64 {
        if self.total_tasks == 0 {
            0.0
        } else {
            self.completed_tasks as f64 / self.total_tasks as f64
        }
    }

    /// 计算平均IO任务时间
    pub fn average_io_time(&self) -> Duration {
        if self.io_tasks == 0 {
            Duration::from_nanos(0)
        } else {
            Duration::from_nanos(self.total_io_time.as_nanos() as u64 / self.io_tasks as u64)
        }
    }

    /// 计算平均CPU任务时间
    pub fn average_cpu_time(&self) -> Duration {
        if self.cpu_tasks == 0 {
            Duration::from_nanos(0)
        } else {
            Duration::from_nanos(self.total_cpu_time.as_nanos() as u64 / self.cpu_tasks as u64)
        }
    }
}

/// 工作窃取队列中的任务
type BoxedTask = Pin<Box<dyn Future<Output = ()> + Send + 'static>>;
type BoxedCpuTask = Box<dyn FnOnce() -> Result<()> + Send + 'static>;

/// 高性能任务执行器（MPMC版本）
///
/// 结合了多种优秀的并发模式：
/// - Tokio的异步运行时用于IO任务
/// - Rayon的工作窃取用于CPU任务
/// - Crossbeam MPMC通道实现真正的多生产者多消费者
/// - 自定义优先级队列用于任务调度
/// - 背压控制防止内存溢出
pub struct HighPerformanceExecutor {
    /// 配置参数
    config: ExecutorConfig,

    /// IO任务队列（多生产者多消费者）
    io_task_tx: channel::Sender<(TaskMetadata, BoxedTask)>,

    /// CPU任务队列（多生产者多消费者）
    cpu_task_tx: channel::Sender<(TaskMetadata, BoxedCpuTask)>,

    /// 高优先级任务队列（多生产者多消费者）
    priority_task_tx: channel::Sender<(TaskMetadata, BoxedTask)>,

    /// 任务ID生成器
    task_id_counter: Arc<AtomicUsize>,

    /// 执行器统计
    stats: Arc<RwLock<ExecutorStats>>,

    /// 背压控制信号量
    backpressure_semaphore: Arc<Semaphore>,

    /// 工作线程句柄（用于优雅关闭）
    worker_handles: Arc<RwLock<Vec<JoinHandle<()>>>>,

    /// 关闭信号
    shutdown_tx: Option<oneshot::Sender<()>>,
}

/// 执行器配置（MPMC优化版本）
#[derive(Debug, Clone)]
pub struct ExecutorConfig {
    /// IO工作线程数量（多消费者）
    pub io_worker_count: usize,
    /// CPU工作线程数量（多消费者）
    pub cpu_worker_count: usize,
    /// 最大队列长度（背压控制）
    pub max_queue_length: usize,
    /// 任务超时时间
    pub task_timeout: Duration,
    /// 统计更新间隔
    pub stats_update_interval: Duration,
    /// 是否启用工作窃取
    pub enable_work_stealing: bool,
    /// CPU任务批量大小
    pub cpu_batch_size: usize,
    /// 是否使用有界队列
    pub bounded_queue: bool,
    /// 队列容量（有界队列时生效）
    pub queue_capacity: usize,
}

impl Default for ExecutorConfig {
    fn default() -> Self {
        let cpu_cores = num_cpus::get();
        Self {
            io_worker_count: cpu_cores * 2, // IO密集型用更多线程
            cpu_worker_count: cpu_cores,    // CPU密集型用CPU核心数
            max_queue_length: 10000,
            task_timeout: Duration::from_secs(300), // 5分钟超时
            stats_update_interval: Duration::from_secs(10),
            enable_work_stealing: true,
            cpu_batch_size: 100,
            bounded_queue: false, // 默认使用无界队列
            queue_capacity: 1000, // 有界队列容量
        }
    }
}

impl HighPerformanceExecutor {
    /// 创建新的高性能执行器（MPMC模式）
    pub fn new(config: ExecutorConfig) -> Self {
        info!("🚀 初始化高性能执行器 (MPMC模式)");

        // 创建MPMC通道 - 基于crossbeam最佳实践
        let (io_task_tx, io_task_rx) = if config.bounded_queue {
            channel::bounded(config.queue_capacity)
        } else {
            channel::unbounded()
        };

        let (cpu_task_tx, cpu_task_rx) = if config.bounded_queue {
            channel::bounded(config.queue_capacity)
        } else {
            channel::unbounded()
        };

        let (priority_task_tx, priority_task_rx) = if config.bounded_queue {
            channel::bounded(config.queue_capacity / 4) // 优先级队列较小
        } else {
            channel::unbounded()
        };

        let (shutdown_tx, shutdown_rx) = oneshot::channel();

        let backpressure_semaphore = Arc::new(Semaphore::new(config.max_queue_length));
        let stats = Arc::new(RwLock::new(ExecutorStats {
            total_tasks: 0,
            completed_tasks: 0,
            failed_tasks: 0,
            io_tasks: 0,
            cpu_tasks: 0,
            mixed_tasks: 0,
            high_priority_tasks: 0,
            average_execution_time: Duration::from_millis(0),
            total_io_time: Duration::from_nanos(0),
            total_cpu_time: Duration::from_nanos(0),
            active_io_workers: config.io_worker_count,
            active_cpu_workers: config.cpu_worker_count,
            queue_length: 0,
            timeout_tasks: 0,
            queue_full_rejections: 0,
            worker_restarts: 0,
        }));

        let executor = Self {
            config: config.clone(),
            io_task_tx,
            cpu_task_tx,
            priority_task_tx,
            task_id_counter: Arc::new(AtomicUsize::new(1)),
            stats: Arc::clone(&stats),
            backpressure_semaphore,
            worker_handles: Arc::new(RwLock::new(Vec::new())),
            shutdown_tx: Some(shutdown_tx),
        };

        // 启动MPMC工作线程
        executor.start_mpmc_workers(io_task_rx, cpu_task_rx, priority_task_rx, shutdown_rx);

        info!(
            "🚀 高性能执行器已启动 (MPMC) - IO工作线程: {}, CPU工作线程: {}",
            config.io_worker_count, config.cpu_worker_count
        );

        executor
    }

    /// 启动所有MPMC工作线程
    fn start_mpmc_workers(
        &self,
        io_task_rx: channel::Receiver<(TaskMetadata, BoxedTask)>,
        cpu_task_rx: channel::Receiver<(TaskMetadata, BoxedCpuTask)>,
        priority_task_rx: channel::Receiver<(TaskMetadata, BoxedTask)>,
        shutdown_rx: oneshot::Receiver<()>,
    ) {
        let mut handles = Vec::new();

        // 启动多个IO工作消费者
        for worker_id in 0..self.config.io_worker_count {
            let handle = self.start_io_worker(worker_id, io_task_rx.clone());
            handles.push(handle);
        }

        // 启动多个CPU工作消费者
        for worker_id in 0..self.config.cpu_worker_count {
            let handle = self.start_cpu_worker(worker_id, cpu_task_rx.clone());
            handles.push(handle);
        }

        // 启动高优先级任务处理器
        let priority_handle = self.start_priority_worker(priority_task_rx);
        handles.push(priority_handle);

        // 启动统计收集器
        let stats_handle = self.start_stats_collector();
        handles.push(stats_handle);

        // 启动关闭监听器
        let shutdown_handle = self.start_shutdown_listener(shutdown_rx);
        handles.push(shutdown_handle);

        // 保存工作线程句柄
        *self.worker_handles.write().unwrap() = handles;

        info!(
            "✅ 已启动 {} 个IO工作线程和 {} 个CPU工作线程",
            self.config.io_worker_count, self.config.cpu_worker_count
        );
    }

    /// 启动IO工作线程（MPMC消费者）
    fn start_io_worker(
        &self,
        worker_id: usize,
        io_task_rx: channel::Receiver<(TaskMetadata, BoxedTask)>,
    ) -> JoinHandle<()> {
        let stats = Arc::clone(&self.stats);
        let semaphore = Arc::clone(&self.backpressure_semaphore);

        tokio::spawn(async move {
            info!("🔧 IO工作线程 {} 启动", worker_id);

            // MPMC消费者循环 - 基于crossbeam官方文档的最佳实践
            while let Ok((metadata, task)) = io_task_rx.recv() {
                let start_time = Instant::now();
                let task_id = metadata.id;

                debug!("📥 IO工作线程 {} 开始执行任务: {}", worker_id, task_id);

                // 执行异步任务，带超时控制
                let timeout_duration = metadata.task_type.suggested_timeout();
                let result = tokio::time::timeout(timeout_duration, task).await;

                let execution_time = start_time.elapsed();

                // 更新统计信息
                {
                    let mut stats = stats.write().unwrap();
                    stats.io_tasks += 1;
                    stats.total_io_time += execution_time;

                    // match result {
                    //     Ok(Ok(_)) => {
                    //         stats.completed_tasks += 1;
                    //         debug!("✅ IO工作线程 {} 完成任务 {}", worker_id, task_id);
                    //     }
                    //     Ok(Err(_)) => {
                    //         stats.failed_tasks += 1;
                    //         error!("❌ IO工作线程 {} 任务 {} 失败", worker_id, task_id);
                    //     }
                    //     Err(_) => {
                    //         stats.failed_tasks += 1;
                    //         stats.timeout_tasks += 1;
                    //         error!("⏰ IO工作线程 {} 任务 {} 超时", worker_id, task_id);
                    //     }
                    // }

                    // 更新平均执行时间
                    let total_completed = stats.completed_tasks + stats.failed_tasks;
                    if total_completed > 0 {
                        let current_avg_nanos = stats.average_execution_time.as_nanos() as u128;
                        let new_avg_nanos = (current_avg_nanos * (total_completed - 1) as u128
                            + execution_time.as_nanos() as u128)
                            / total_completed as u128;
                        stats.average_execution_time = Duration::from_nanos(new_avg_nanos as u64);
                    }
                }

                // 释放背压信号量
                semaphore.add_permits(1);
            }

            warn!("🔧 IO工作线程 {} 退出", worker_id);
        })
    }

    /// 启动CPU工作线程（MPMC消费者）
    fn start_cpu_worker(
        &self,
        worker_id: usize,
        cpu_task_rx: channel::Receiver<(TaskMetadata, BoxedCpuTask)>,
    ) -> JoinHandle<()> {
        let stats = Arc::clone(&self.stats);
        let semaphore = Arc::clone(&self.backpressure_semaphore);
        let batch_size = self.config.cpu_batch_size;

        tokio::spawn(async move {
            info!("💪 CPU工作线程 {} 启动", worker_id);

            let mut task_batch = Vec::with_capacity(batch_size);

            // MPMC消费者循环 - 支持批量处理
            while let Ok((metadata, task)) = cpu_task_rx.recv() {
                task_batch.push((metadata, task));

                // 批量处理CPU任务以提高效率
                if task_batch.len() >= batch_size {
                    Self::process_cpu_batch_worker(worker_id, &mut task_batch, &stats, &semaphore)
                        .await;
                }

                // 检查是否有更多任务可以立即处理
                while let Ok((metadata, task)) = cpu_task_rx.try_recv() {
                    task_batch.push((metadata, task));
                    if task_batch.len() >= batch_size {
                        Self::process_cpu_batch_worker(
                            worker_id,
                            &mut task_batch,
                            &stats,
                            &semaphore,
                        )
                        .await;
                    }
                }

                // 处理剩余任务
                if !task_batch.is_empty() {
                    Self::process_cpu_batch_worker(worker_id, &mut task_batch, &stats, &semaphore)
                        .await;
                }
            }

            warn!("💪 CPU工作线程 {} 退出", worker_id);
        })
    }

    /// 处理CPU任务批次（单个工作线程版本）
    async fn process_cpu_batch_worker(
        worker_id: usize,
        task_batch: &mut Vec<(TaskMetadata, BoxedCpuTask)>,
        stats: &Arc<RwLock<ExecutorStats>>,
        semaphore: &Arc<Semaphore>,
    ) {
        let batch = std::mem::take(task_batch);
        let batch_size = batch.len();

        debug!(
            "🔧 CPU工作线程 {} 处理批次 {} 个任务",
            worker_id, batch_size
        );

        // 使用Rayon进行并行处理 - 基于rayon官方文档的最佳实践
        let results: Vec<(u64, Result<Duration>)> = batch
            .into_par_iter()
            .map(|(metadata, task)| {
                let start_time = Instant::now();
                let task_id = metadata.id;

                debug!("🔧 开始执行CPU任务: {} (工作线程 {})", task_id, worker_id);

                let result = task().map(|_| start_time.elapsed());

                match &result {
                    Ok(execution_time) => {
                        debug!(
                            "✅ CPU任务完成: {} - 耗时: {:?} (工作线程 {})",
                            task_id, execution_time, worker_id
                        );
                    }
                    Err(e) => {
                        error!(
                            "❌ CPU任务失败: {} - {} (工作线程 {})",
                            task_id, e, worker_id
                        );
                    }
                }

                (task_id, result)
            })
            .collect();

        // 收集结果并更新统计
        let mut completed = 0;
        let mut failed = 0;
        let mut total_time = Duration::from_nanos(0);

        for (_, result) in results {
            match result {
                Ok(execution_time) => {
                    completed += 1;
                    total_time += execution_time;
                }
                Err(_) => {
                    failed += 1;
                }
            }
        }

        // 更新统计信息
        {
            let mut stats = stats.write().unwrap();
            stats.completed_tasks += completed;
            stats.failed_tasks += failed;
            stats.cpu_tasks += completed + failed;
            stats.total_cpu_time += total_time;

            // 更新平均执行时间
            let total_completed = stats.completed_tasks + stats.failed_tasks;
            if total_completed > 0 {
                let current_avg_nanos = stats.average_execution_time.as_nanos() as u128;
                let new_avg_nanos = (current_avg_nanos
                    * (total_completed - (completed + failed)) as u128
                    + total_time.as_nanos() as u128)
                    / total_completed as u128;
                stats.average_execution_time = Duration::from_nanos(new_avg_nanos as u64);
            }
        }

        // 释放背压信号量
        semaphore.add_permits(batch_size);

        debug!(
            "✅ CPU工作线程 {} 完成批次处理: 成功 {}, 失败 {}",
            worker_id, completed, failed
        );
    }

    /// 启动高优先级任务处理器
    fn start_priority_worker(
        &self,
        priority_task_rx: channel::Receiver<(TaskMetadata, BoxedTask)>,
    ) -> JoinHandle<()> {
        let stats = Arc::clone(&self.stats);

        tokio::spawn(async move {
            info!("⚡ 高优先级任务处理器已启动");

            while let Ok((metadata, task)) = priority_task_rx.recv() {
                let stats = Arc::clone(&stats);
                let task_id = metadata.id;
                let start_time = Instant::now();

                debug!("🔥 开始执行高优先级任务: {}", task_id);

                // 高优先级任务立即执行，不受背压控制
                let timeout_duration = Duration::from_secs(60);
                let result = tokio::time::timeout(timeout_duration, task).await;

                let execution_time = start_time.elapsed();

                // 更新统计信息
                {
                    let mut stats = stats.write().unwrap();
                    stats.high_priority_tasks += 1;

                    // match result {
                    //     Ok(Ok(_)) => {
                    //         stats.completed_tasks += 1;
                    //         debug!(
                    //             "✅ 高优先级任务完成: {} - 耗时: {:?}",
                    //             task_id, execution_time
                    //         );
                    //     }
                    //     Ok(Err(_)) => {
                    //         stats.failed_tasks += 1;
                    //         error!("❌ 高优先级任务失败: {}", task_id);
                    //     }
                    //     Err(_) => {
                    //         stats.failed_tasks += 1;
                    //         stats.timeout_tasks += 1;
                    //         error!("⏰ 高优先级任务超时: {}", task_id);
                    //     }
                    // }
                }
            }

            warn!("⚡ 高优先级任务处理器退出");
        })
    }

    /// 启动统计收集器
    fn start_stats_collector(&self) -> JoinHandle<()> {
        let stats = Arc::clone(&self.stats);
        let interval = self.config.stats_update_interval;

        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);

            loop {
                ticker.tick().await;

                let stats = stats.read().unwrap().clone();
                info!(
                    "📊 执行器状态 (MPMC) - 总任务: {}, 完成: {} ({:.1}%), 失败: {}, 超时: {}, 平均耗时: {:?}, IO工作线程: {}, CPU工作线程: {}",
                    stats.total_tasks,
                    stats.completed_tasks,
                    stats.success_rate() * 100.0,
                    stats.failed_tasks,
                    stats.timeout_tasks,
                    stats.average_execution_time,
                    stats.active_io_workers,
                    stats.active_cpu_workers
                );
            }
        })
    }

    /// 启动关闭监听器
    fn start_shutdown_listener(&self, shutdown_rx: oneshot::Receiver<()>) -> JoinHandle<()> {
        tokio::spawn(async move {
            if let Ok(_) = shutdown_rx.await {
                info!("🛑 收到关闭信号，执行器即将关闭");
            }
        })
    }

    /// 提交IO密集型任务（MPMC优化）
    pub async fn submit_io_task<F, T>(
        &self,
        description: String,
        priority: Priority,
        task: F,
    ) -> Result<u64>
    where
        F: Future<Output = Result<T>> + Send + 'static,
        T: Send + 'static,
    {
        let task_id = self.task_id_counter.fetch_add(1, Ordering::SeqCst) as u64;

        let metadata = TaskMetadata {
            id: task_id,
            task_type: TaskType::IoIntensive,
            priority,
            created_at: Instant::now(),
            estimated_duration: None,
            description,
        };

        // 包装任务以处理结果
        let boxed_task = Box::pin(async move {
            match task.await {
                Ok(_result) => {
                    debug!("✅ IO任务成功完成: {}", task_id);
                }
                Err(e) => {
                    error!("❌ IO任务执行失败: {} - {}", task_id, e);
                }
            }
        });

        // 提交前获取背压许可
        let _permit = self.backpressure_semaphore.acquire().await.unwrap();

        // 根据优先级选择队列
        let sender = if priority >= Priority::High {
            &self.priority_task_tx
        } else {
            &self.io_task_tx
        };

        // 使用MPMC通道发送任务
        sender
            .send((metadata, boxed_task))
            .map_err(|_| anyhow::anyhow!("Failed to submit IO task"))?;

        // 更新统计
        {
            let mut stats = self.stats.write().unwrap();
            stats.total_tasks += 1;
        }

        Ok(task_id)
    }

    /// 提交CPU密集型任务（MPMC优化）
    pub async fn submit_cpu_task<F, T>(
        &self,
        description: String,
        priority: Priority,
        task: F,
    ) -> Result<u64>
    where
        F: FnOnce() -> Result<T> + Send + 'static,
        T: Send + 'static,
    {
        let task_id = self.task_id_counter.fetch_add(1, Ordering::SeqCst) as u64;

        let metadata = TaskMetadata {
            id: task_id,
            task_type: TaskType::CpuIntensive,
            priority,
            created_at: Instant::now(),
            estimated_duration: None,
            description,
        };

        // 包装任务
        let boxed_task = Box::new(move || match task() {
            Ok(_result) => {
                debug!("✅ CPU任务成功完成: {}", task_id);
                Ok(())
            }
            Err(e) => {
                error!("❌ CPU任务执行失败: {} - {}", task_id, e);
                Err(e)
            }
        });

        // 提交前获取背压许可
        let _permit = self.backpressure_semaphore.acquire().await.unwrap();

        // 使用MPMC通道发送任务
        self.cpu_task_tx
            .send((metadata, boxed_task))
            .map_err(|_| anyhow::anyhow!("Failed to submit CPU task"))?;

        // 更新统计
        {
            let mut stats = self.stats.write().unwrap();
            stats.total_tasks += 1;
        }

        Ok(task_id)
    }

    /// 批量提交IO任务
    pub async fn submit_io_batch<F, T>(&self, tasks: Vec<(String, Priority, F)>) -> Result<Vec<u64>>
    where
        F: Future<Output = Result<T>> + Send + 'static,
        T: Send + 'static,
    {
        let mut task_ids = Vec::with_capacity(tasks.len());

        for (description, priority, task) in tasks {
            let task_id = self.submit_io_task(description, priority, task).await?;
            task_ids.push(task_id);
        }

        Ok(task_ids)
    }

    /// 批量提交CPU任务
    pub async fn submit_cpu_batch<F, T>(
        &self,
        tasks: Vec<(String, Priority, F)>,
    ) -> Result<Vec<u64>>
    where
        F: FnOnce() -> Result<T> + Send + 'static,
        T: Send + 'static,
    {
        let mut task_ids = Vec::with_capacity(tasks.len());

        for (description, priority, task) in tasks {
            let task_id = self.submit_cpu_task(description, priority, task).await?;
            task_ids.push(task_id);
        }

        Ok(task_ids)
    }

    /// 获取执行器统计信息
    pub fn get_stats(&self) -> ExecutorStats {
        self.stats.read().unwrap().clone()
    }

    /// 等待所有任务完成
    pub async fn wait_for_completion(&self) -> Result<()> {
        let timeout = Duration::from_secs(30);
        let start_time = Instant::now();

        loop {
            let stats = self.get_stats();
            if stats.total_tasks > 0
                && stats.completed_tasks + stats.failed_tasks >= stats.total_tasks
            {
                break;
            }

            if start_time.elapsed() > timeout {
                return Err(anyhow::anyhow!("等待任务完成超时"));
            }

            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        Ok(())
    }

    /// 优雅关闭执行器
    pub async fn shutdown(mut self) -> Result<()> {
        info!("🛑 开始关闭高性能执行器 (MPMC)");

        if let Some(shutdown_tx) = self.shutdown_tx.take() {
            let _ = shutdown_tx.send(());
        }

        // 等待正在执行的任务完成
        self.wait_for_completion().await?;

        // 等待工作线程退出
        let handles = {
            let mut handles = self.worker_handles.write().unwrap();
            std::mem::take(&mut *handles)
        };

        for handle in handles {
            if let Err(e) = handle.await {
                error!("工作线程退出错误: {}", e);
            }
        }

        info!("🛑 高性能执行器 (MPMC) 已关闭");
        Ok(())
    }
}
