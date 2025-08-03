use canp::{trace_performance, performance::{PerformanceMonitor, PerformanceConfig}};
use canp::memory_pool::{UnifiedMemoryPool, MemoryPoolConfig};
use canp::dbc_parser::{DbcParser, DbcParserConfig};
use canp::thread_pool::{PipelineThreadPool, ThreadPoolConfig, Task, TaskType, TaskPriority};
use std::sync::Arc;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== 性能监测示例 ===\n");

    // 初始化性能监测器
    let mut config = PerformanceConfig::default();
    config.verbose_tracing = true;
    config.prometheus_port = 9090;
    
    let monitor = Arc::new(PerformanceMonitor::new(config)?);
    println!("✓ 性能监测器初始化完成");

    // 测试内存池性能监测
    println!("\n1. 测试内存池性能监测");
    test_memory_pool_monitoring(&monitor).await;

    // 测试DBC解析性能监测
    println!("\n2. 测试DBC解析性能监测");
    test_dbc_parser_monitoring(&monitor).await;

    // 测试线程池性能监测
    println!("\n3. 测试线程池性能监测");
    test_thread_pool_monitoring(&monitor).await;

    // 测试自定义指标
    println!("\n4. 测试自定义指标");
    test_custom_metrics(&monitor).await;

    // 显示性能统计
    println!("\n5. 性能统计信息");
    let stats = monitor.get_performance_stats().await;
    println!("系统运行时间: {:.2} 秒", stats.uptime_seconds);
    println!("自定义指标数量: {}", stats.custom_metrics.len());
    
    for (name, value) in &stats.custom_metrics {
        println!("  {}: {}", name, value);
    }

    // 显示Prometheus指标
    if let Some(metrics) = monitor.get_prometheus_metrics() {
        println!("\n6. Prometheus指标 (前10行):");
        for (i, line) in metrics.lines().take(10).enumerate() {
            println!("  {}: {}", i + 1, line);
        }
    }

    println!("\n=== 性能监测示例完成 ===");
    Ok(())
}

async fn test_memory_pool_monitoring(monitor: &Arc<PerformanceMonitor>) {
    let config = MemoryPoolConfig::default();
    let pool = UnifiedMemoryPool::new(config);
    
    // 使用性能追踪器
    let _tracer = trace_performance!(monitor.clone(), "memory_pool_test");
    
    // 记录内存分配
    for i in 0..5 {
        let size = 1024 * (i + 1);
        let _block = pool.allocate_block(size).unwrap(); // 用 _block 消除未使用变量警告
        monitor.record_memory_allocation(size, "block");
        
        // 模拟使用内存
        std::thread::sleep(Duration::from_millis(10));
        
        // 记录内存释放
        monitor.record_memory_deallocation(size, "block");
    }
    
    // 更新内存池状态
    let stats = pool.get_stats();
    monitor.update_memory_pool_status(
        stats.peak_memory_usage,
        stats.current_memory_usage,
        stats.peak_memory_usage - stats.current_memory_usage,
    );
    
    println!("  ✓ 内存池性能监测完成");
}

async fn test_dbc_parser_monitoring(monitor: &Arc<PerformanceMonitor>) {
    let config = DbcParserConfig::default();
    let parser = DbcParser::new(config);
    
    let dbc_content = r#"
VERSION ""

NS_ :
	NS_DESC_
	CM_
	BA_DEF_
	BA_
	VAL_
	CAT_DEF_
	CAT_
	FILTER
	BA_DEF_DEF_
	EV_DATA_
	ENVVAR_DATA_
	SGTYPE_
	SGTYPE_VAL_
	BA_DEF_SGTYPE_
	BA_SGTYPE_
	SIG_TYPE_REF_
	VAL_TABLE_
	SIG_GROUP_
	SIG_VALTYPE_
	SIGTYPE_VALTYPE_
	BO_TX_BU_
	BA_DEF_REL_
	BA_REL_
	BA_DEF_DEF_REL_
	BU_SG_REL_
	BU_EV_REL_
	BU_BO_REL_
	SG_MUL_VAL_

BS_:

BU_: Engine_ECU

BO_ 100 EngineData: 8 Engine_ECU
 SG_ EngineSpeed : 0|16@1+ (0.125,0) [0|8031.875] "rpm" Engine_ECU
 SG_ EngineTemp : 16|8@1+ (1,-40) [-40|215] "degC" Engine_ECU

BA_DEF_ "BusType" STRING ;
BA_DEF_DEF_ "BusType" "CAN" ;
BA_ "BusType" "CAN" ;
"#;
    
    let start_time = std::time::Instant::now();
    
    match parser.parse_content(dbc_content) {
        Ok(result) => {
            let parse_time = start_time.elapsed();
            monitor.record_dbc_parse_result(
                true,
                result.stats.message_count,
                result.stats.signal_count,
                parse_time.as_millis() as u64,
            );
            println!("  ✓ DBC解析成功: {} 消息, {} 信号", 
                result.stats.message_count, result.stats.signal_count);
        }
        Err(_) => {
            monitor.record_dbc_parse_result(false, 0, 0, 0);
            println!("  ✗ DBC解析失败");
        }
    }
}

async fn test_thread_pool_monitoring(monitor: &Arc<PerformanceMonitor>) {
    let config = ThreadPoolConfig::default();
    let pool = Arc::new(PipelineThreadPool::new(config));
    
    // 创建不同类型的任务
    let mut tasks = Vec::new();
    
    // CPU密集型任务
    for i in 0..3 {
        let task = Task::new(
            TaskType::CpuBound,
            TaskPriority::Normal,
            Box::new(move || {
                let mut sum = 0.0;
                for j in 0..1000 {
                    sum += (j as f64).sqrt();
                }
                Ok(())
            }),
        );
        tasks.push(task);
    }
    
    // IO密集型任务
    for i in 0..2 {
        let task = Task::new(
            TaskType::IoBound,
            TaskPriority::Normal,
            Box::new(move || {
                std::thread::sleep(Duration::from_millis(10));
                Ok(())
            }),
        );
        tasks.push(task);
    }
    
    // 记录任务提交
    for task in &tasks {
        monitor.record_thread_pool_task(&format!("{:?}", task.task_type), "queued");
    }
    
    // 提交任务
    let result = pool.submit_batch(tasks);

    // 只需判断一次整体结果
    if result.is_ok() {
        monitor.record_thread_pool_task("cpu_bound", "completed");
    } else {
        monitor.record_thread_pool_task("cpu_bound", "failed");
    }
    
    println!("  ✓ 线程池性能监测完成");
}

async fn test_custom_metrics(monitor: &Arc<PerformanceMonitor>) {
    // 设置自定义指标
    monitor.set_custom_metric("pipeline_throughput".to_string(), 1500.5).await;
    monitor.set_custom_metric("error_rate".to_string(), 0.02).await;
    monitor.set_custom_metric("active_connections".to_string(), 25.0).await;
    monitor.set_custom_metric("cache_hit_ratio".to_string(), 0.85).await;
    
    // 获取自定义指标
    let throughput = monitor.get_custom_metric("pipeline_throughput").await;
    let error_rate = monitor.get_custom_metric("error_rate").await;
    
    println!("  ✓ 自定义指标设置完成");
    println!("    流水线吞吐量: {:?}", throughput);
    println!("    错误率: {:?}", error_rate);
} 