use criterion::{black_box, criterion_group, criterion_main, Criterion};
use canp::thread_pool::{PipelineThreadPool, ThreadPoolConfig, Task, TaskType, TaskPriority};
use std::sync::Arc;

fn thread_pool_creation_benchmark(c: &mut Criterion) {
    c.bench_function("thread_pool_creation", |b| {
        b.iter(|| {
            let config = ThreadPoolConfig::default();
            let _pool = PipelineThreadPool::new(config).unwrap();
        });
    });
}

fn thread_pool_task_submission_benchmark(c: &mut Criterion) {
    let config = ThreadPoolConfig::default();
    let pool = Arc::new(PipelineThreadPool::new(config).unwrap());
    
    c.bench_function("thread_pool_task_submission", |b| {
        b.iter(|| {
            let task = Task::new(
                "benchmark_task".to_string(),
                TaskType::CpuBound,
                TaskPriority::Normal,
                Box::new(|| {
                    // 模拟CPU密集型任务
                    let mut sum = 0.0;
                    for i in 0..1000 {
                        sum += (i as f64).sqrt();
                    }
                    sum
                }),
            );
            
            let _result = pool.submit_task(task);
        });
    });
}

fn thread_pool_batch_submission_benchmark(c: &mut Criterion) {
    let config = ThreadPoolConfig::default();
    let pool = Arc::new(PipelineThreadPool::new(config).unwrap());
    
    c.bench_function("thread_pool_batch_submission", |b| {
        b.iter(|| {
            let mut tasks = Vec::new();
            for i in 0..10 {
                let task = Task::new(
                    format!("batch_task_{}", i),
                    TaskType::CpuBound,
                    TaskPriority::Normal,
                    Box::new(move || {
                        // 模拟CPU密集型任务
                        let mut sum = 0.0;
                        for j in 0..100 {
                            sum += (j as f64).sqrt();
                        }
                        sum
                    }),
                );
                tasks.push(task);
            }
            
            let _results = pool.submit_tasks_batch(tasks);
        });
    });
}

fn thread_pool_mixed_task_types_benchmark(c: &mut Criterion) {
    let config = ThreadPoolConfig::default();
    let pool = Arc::new(PipelineThreadPool::new(config).unwrap());
    
    c.bench_function("thread_pool_mixed_task_types", |b| {
        b.iter(|| {
            let mut tasks = Vec::new();
            
            // CPU密集型任务
            for i in 0..5 {
                let task = Task::new(
                    format!("cpu_task_{}", i),
                    TaskType::CpuBound,
                    TaskPriority::Normal,
                    Box::new(move || {
                        let mut sum = 0.0;
                        for j in 0..500 {
                            sum += (j as f64).sqrt();
                        }
                        sum
                    }),
                );
                tasks.push(task);
            }
            
            // IO密集型任务
            for i in 0..5 {
                let task = Task::new(
                    format!("io_task_{}", i),
                    TaskType::IoBound,
                    TaskPriority::Normal,
                    Box::new(move || {
                        // 模拟IO操作
                        std::thread::sleep(std::time::Duration::from_micros(100));
                        i as f64
                    }),
                );
                tasks.push(task);
            }
            
            // 内存密集型任务
            for i in 0..5 {
                let task = Task::new(
                    format!("memory_task_{}", i),
                    TaskType::MemoryBound,
                    TaskPriority::Normal,
                    Box::new(move || {
                        // 模拟内存操作
                        let mut data = vec![0u8; 1024];
                        for j in 0..data.len() {
                            data[j] = (j % 256) as u8;
                        }
                        data.len() as f64
                    }),
                );
                tasks.push(task);
            }
            
            let _results = pool.submit_tasks_batch(tasks);
        });
    });
}

fn thread_pool_priority_tasks_benchmark(c: &mut Criterion) {
    let config = ThreadPoolConfig::default();
    let pool = Arc::new(PipelineThreadPool::new(config).unwrap());
    
    c.bench_function("thread_pool_priority_tasks", |b| {
        b.iter(|| {
            let mut tasks = Vec::new();
            
            // 高优先级任务
            for i in 0..3 {
                let task = Task::new(
                    format!("high_priority_task_{}", i),
                    TaskType::CpuBound,
                    TaskPriority::High,
                    Box::new(move || {
                        let mut sum = 0.0;
                        for j in 0..100 {
                            sum += (j as f64).sqrt();
                        }
                        sum
                    }),
                );
                tasks.push(task);
            }
            
            // 正常优先级任务
            for i in 0..5 {
                let task = Task::new(
                    format!("normal_priority_task_{}", i),
                    TaskType::CpuBound,
                    TaskPriority::Normal,
                    Box::new(move || {
                        let mut sum = 0.0;
                        for j in 0..200 {
                            sum += (j as f64).sqrt();
                        }
                        sum
                    }),
                );
                tasks.push(task);
            }
            
            // 低优先级任务
            for i in 0..3 {
                let task = Task::new(
                    format!("low_priority_task_{}", i),
                    TaskType::CpuBound,
                    TaskPriority::Low,
                    Box::new(move || {
                        let mut sum = 0.0;
                        for j in 0..300 {
                            sum += (j as f64).sqrt();
                        }
                        sum
                    }),
                );
                tasks.push(task);
            }
            
            let _results = pool.submit_tasks_batch(tasks);
        });
    });
}

fn thread_pool_task_with_memory_benchmark(c: &mut Criterion) {
    let config = ThreadPoolConfig::default();
    let pool = Arc::new(PipelineThreadPool::new(config).unwrap());
    
    c.bench_function("thread_pool_task_with_memory", |b| {
        b.iter(|| {
            use canp::memory_pool::{MemoryBlock, MemoryPoolConfig, UnifiedMemoryPool};
            
            let memory_pool = Arc::new(UnifiedMemoryPool::new(MemoryPoolConfig::default()));
            let memory_blocks = vec![
                memory_pool.allocate_block(1024).unwrap(),
                memory_pool.allocate_block(2048).unwrap(),
            ];
            
            let task = Task::with_memory(
                "memory_task".to_string(),
                TaskType::MemoryBound,
                TaskPriority::Normal,
                Box::new(|memory_blocks| {
                    // 使用内存块进行计算
                    let mut total_size = 0;
                    for block in memory_blocks {
                        total_size += block.len();
                    }
                    total_size as f64
                }),
                memory_blocks,
            );
            
            let _result = pool.submit_task(task);
        });
    });
}

fn thread_pool_parallel_processing_benchmark(c: &mut Criterion) {
    let config = ThreadPoolConfig::default();
    let pool = Arc::new(PipelineThreadPool::new(config).unwrap());
    
    c.bench_function("thread_pool_parallel_processing", |b| {
        b.iter(|| {
            let mut tasks = Vec::new();
            
            // 创建大量并行任务
            for i in 0..50 {
                let task = Task::new(
                    format!("parallel_task_{}", i),
                    TaskType::CpuBound,
                    TaskPriority::Normal,
                    Box::new(move || {
                        // 模拟并行计算
                        let mut result = 0.0;
                        for j in 0..100 {
                            result += (j as f64).powf(1.5);
                        }
                        result
                    }),
                );
                tasks.push(task);
            }
            
            let _results = pool.submit_tasks_batch(tasks);
        });
    });
}

criterion_group!(
    thread_pool_benches,
    thread_pool_creation_benchmark,
    thread_pool_task_submission_benchmark,
    thread_pool_batch_submission_benchmark,
    thread_pool_mixed_task_types_benchmark,
    thread_pool_priority_tasks_benchmark,
    thread_pool_task_with_memory_benchmark,
    thread_pool_parallel_processing_benchmark,
);
criterion_main!(thread_pool_benches); 