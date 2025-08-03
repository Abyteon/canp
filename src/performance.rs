//! # 性能监测模块 (Performance Monitoring Module)
//! 
//! 提供全面的性能监测功能，包括指标收集、Prometheus导出和性能追踪。
//! 
//! ## 设计理念
//! 
//! - **实时监测**：收集关键性能指标
//! - **可视化**：支持Prometheus和Grafana集成
//! - **低开销**：最小化性能监测对系统性能的影响
//! - **可扩展**：支持自定义指标和追踪

use metrics::{counter, gauge, histogram};
use metrics_exporter_prometheus::{Matcher, PrometheusBuilder, PrometheusHandle};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};
use std::sync::Once;

static PROMETHEUS_INSTALL_ONCE: Once = Once::new();

/// 性能监测器配置
#[derive(Debug, Clone)]
pub struct PerformanceConfig {
    /// 是否启用性能监测
    pub enabled: bool,
    /// Prometheus导出端口
    pub prometheus_port: u16,
    /// 指标收集间隔（秒）
    pub collection_interval: u64,
    /// 是否启用详细追踪
    pub verbose_tracing: bool,
    /// 自定义标签
    pub labels: HashMap<String, String>,
}

impl Default for PerformanceConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            prometheus_port: 9090,
            collection_interval: 5,
            verbose_tracing: false,
            labels: HashMap::new(),
        }
    }
}

/// 性能监测器
/// 
/// 提供统一的性能监测接口，支持指标收集、Prometheus导出和性能追踪
pub struct PerformanceMonitor {
    config: PerformanceConfig,
    prometheus_handle: Option<PrometheusHandle>,
    start_time: Instant,
    custom_metrics: Arc<RwLock<HashMap<String, f64>>>,
}

impl PerformanceMonitor {
    /// 创建新的性能监测器
    /// 
    /// ## 参数
    /// 
    /// - `config`：性能监测配置
    /// 
    /// ## 返回值
    /// 
    /// 返回配置好的性能监测器
    pub fn new(config: PerformanceConfig) -> anyhow::Result<Self> {
        let start_time = Instant::now();
        let custom_metrics = Arc::new(RwLock::new(HashMap::new()));
        
        let mut monitor = Self {
            config,
            prometheus_handle: None,
            start_time,
            custom_metrics,
        };
        
        if monitor.config.enabled {
            let mut install_result = Ok(());
            PROMETHEUS_INSTALL_ONCE.call_once(|| {
                install_result = monitor.initialize_prometheus();
            });
            if let Err(e) = install_result {
                // 只在第一次失败时报错，后续直接跳过
                return Err(e);
            }
            monitor.initialize_metrics();
        }
        
        info!("性能监测器初始化完成");
        Ok(monitor)
    }
    
    /// 初始化Prometheus导出器
    fn initialize_prometheus(&mut self) -> anyhow::Result<()> {
        let builder = PrometheusBuilder::new();
        let builder = builder
            .set_buckets_for_metric(
                Matcher::Full("canp_operation_duration_seconds".to_string()),
                &[0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0],
            )?
            .set_buckets_for_metric(
                Matcher::Full("canp_memory_allocation_bytes".to_string()),
                &[1024.0, 4096.0, 16384.0, 65536.0, 262144.0, 1048576.0, 4194304.0],
            )?;
        
        let handle = builder
            .install_recorder()
            .map_err(|e| anyhow::anyhow!("Failed to install Prometheus recorder: {}", e))?;
        
        self.prometheus_handle = Some(handle);
        info!("Prometheus导出器初始化完成，端口: {}", self.config.prometheus_port);
        Ok(())
    }
    
    /// 初始化基础指标
    fn initialize_metrics(&self) {
        // 系统启动时间
        gauge!("canp_system_uptime_seconds").set(self.start_time.elapsed().as_secs_f64());
        
        // 内存池指标
        gauge!("canp_memory_pool_total_bytes").set(0.0);
        gauge!("canp_memory_pool_used_bytes").set(0.0);
        gauge!("canp_memory_pool_available_bytes").set(0.0);
        counter!("canp_memory_pool_allocation_count").increment(0);
        counter!("canp_memory_pool_deallocation_count").increment(0);
        
        // 线程池指标
        counter!("canp_thread_pool_active_tasks").increment(0);
        counter!("canp_thread_pool_queued_tasks").increment(0);
        counter!("canp_thread_pool_completed_tasks").increment(0);
        counter!("canp_thread_pool_failed_tasks").increment(0);
        
        // DBC解析指标
        counter!("canp_dbc_parser_parse_count").increment(0);
        counter!("canp_dbc_parser_error_count").increment(0);
        gauge!("canp_dbc_parser_messages_parsed").set(0.0);
        gauge!("canp_dbc_parser_signals_parsed").set(0.0);
        
        info!("基础性能指标初始化完成");
    }
    
    /// 记录操作持续时间
    /// 
    /// ## 参数
    /// 
    /// - `operation`：操作名称
    /// - `duration`：操作持续时间
    /// - `labels`：额外标签
    pub fn record_operation_duration(
        &self,
        operation: &str,
        duration: Duration,
        labels: Option<HashMap<String, String>>,
    ) {
        if !self.config.enabled {
            return;
        }
        
        let duration_seconds = duration.as_secs_f64();
        histogram!("canp_operation_duration_seconds", "operation" => operation.to_string()).record(duration_seconds);
        
        if let Some(labels) = labels {
            for (key, value) in labels {
                histogram!("canp_operation_duration_seconds", "operation" => operation.to_string(), key => value).record(duration_seconds);
            }
        }
        
        if self.config.verbose_tracing {
            debug!("操作 {} 耗时: {:.3}ms", operation, duration.as_millis());
        }
    }
    
    /// 记录内存分配
    /// 
    /// ## 参数
    /// 
    /// - `size`：分配的内存大小（字节）
    /// - `pool_type`：内存池类型
    pub fn record_memory_allocation(&self, size: usize, pool_type: &str) {
        if !self.config.enabled {
            return;
        }
        
        let size_f64 = size as f64;
        histogram!("canp_memory_allocation_bytes", "pool_type" => pool_type.to_string()).record(size_f64);
        counter!("canp_memory_pool_allocation_count", "pool_type" => pool_type.to_string()).increment(1);
        
        if self.config.verbose_tracing {
            debug!("内存分配: {} 字节, 池类型: {}", size, pool_type);
        }
    }
    
    /// 记录内存释放
    /// 
    /// ## 参数
    /// 
    /// - `size`：释放的内存大小（字节）
    /// - `pool_type`：内存池类型
    pub fn record_memory_deallocation(&self, size: usize, pool_type: &str) {
        if !self.config.enabled {
            return;
        }
        
        counter!("canp_memory_pool_deallocation_count", "pool_type" => pool_type.to_string()).increment(1);
        
        if self.config.verbose_tracing {
            debug!("内存释放: {} 字节, 池类型: {}", size, pool_type);
        }
    }
    
    /// 更新内存池状态
    /// 
    /// ## 参数
    /// 
    /// - `total_bytes`：总内存字节数
    /// - `used_bytes`：已使用内存字节数
    /// - `available_bytes`：可用内存字节数
    pub fn update_memory_pool_status(&self, total_bytes: usize, used_bytes: usize, available_bytes: usize) {
        if !self.config.enabled {
            return;
        }
        
        gauge!("canp_memory_pool_total_bytes").set(total_bytes as f64);
        gauge!("canp_memory_pool_used_bytes").set(used_bytes as f64);
        gauge!("canp_memory_pool_available_bytes").set(available_bytes as f64);
        
        let usage_percentage = if total_bytes > 0 {
            (used_bytes as f64 / total_bytes as f64) * 100.0
        } else {
            0.0
        };
        
        gauge!("canp_memory_pool_usage_percentage").set(usage_percentage);
    }
    
    /// 记录线程池任务
    /// 
    /// ## 参数
    /// 
    /// - `task_type`：任务类型
    /// - `status`：任务状态（queued, active, completed, failed）
    pub fn record_thread_pool_task(&self, task_type: &str, status: &str) {
        if !self.config.enabled {
            return;
        }
        
        match status {
            "queued" => {
                counter!("canp_thread_pool_queued_tasks", "task_type" => task_type.to_string()).increment(1);
            }
            "active" => {
                counter!("canp_thread_pool_active_tasks", "task_type" => task_type.to_string()).increment(1);
            }
            "completed" => {
                counter!("canp_thread_pool_completed_tasks", "task_type" => task_type.to_string()).increment(1);
            }
            "failed" => {
                counter!("canp_thread_pool_failed_tasks", "task_type" => task_type.to_string()).increment(1);
            }
            _ => warn!("未知的任务状态: {}", status),
        }
    }
    
    /// 记录DBC解析结果
    /// 
    /// ## 参数
    /// 
    /// - `success`：是否解析成功
    /// - `message_count`：解析的消息数量
    /// - `signal_count`：解析的信号数量
    /// - `parse_time_ms`：解析时间（毫秒）
    pub fn record_dbc_parse_result(
        &self,
        success: bool,
        message_count: usize,
        signal_count: usize,
        parse_time_ms: u64,
    ) {
        if !self.config.enabled {
            return;
        }
        
        if success {
            counter!("canp_dbc_parser_parse_count").increment(1);
            gauge!("canp_dbc_parser_messages_parsed").set(message_count as f64);
            gauge!("canp_dbc_parser_signals_parsed").set(signal_count as f64);
            histogram!("canp_dbc_parser_duration_ms").record(parse_time_ms as f64);
        } else {
            counter!("canp_dbc_parser_error_count").increment(1);
        }
    }
    
    /// 设置自定义指标
    /// 
    /// ## 参数
    /// 
    /// - `name`：指标名称
    /// - `value`：指标值
    pub async fn set_custom_metric(&self, name: String, value: f64) {
        if !self.config.enabled {
            return;
        }
        
        let mut metrics = self.custom_metrics.write().await;
        metrics.insert(name.clone(), value);
        gauge!("canp_custom_metric", "name" => name).set(value);
    }
    
    /// 获取自定义指标
    /// 
    /// ## 参数
    /// 
    /// - `name`：指标名称
    /// 
    /// ## 返回值
    /// 
    /// 返回指标值，如果不存在则返回None
    pub async fn get_custom_metric(&self, name: &str) -> Option<f64> {
        let metrics = self.custom_metrics.read().await;
        metrics.get(name).copied()
    }
    
    /// 获取Prometheus指标
    /// 
    /// ## 返回值
    /// 
    /// 返回Prometheus格式的指标数据
    pub fn get_prometheus_metrics(&self) -> Option<String> {
        self.prometheus_handle.as_ref().map(|handle| handle.render())
    }
    
    /// 获取系统运行时间
    /// 
    /// ## 返回值
    /// 
    /// 返回系统运行时间
    pub fn get_uptime(&self) -> Duration {
        self.start_time.elapsed()
    }
    
    /// 获取性能统计信息
    /// 
    /// ## 返回值
    /// 
    /// 返回性能统计信息
    pub async fn get_performance_stats(&self) -> PerformanceStats {
        let uptime = self.get_uptime();
        let custom_metrics = self.custom_metrics.read().await.clone();
        
        PerformanceStats {
            uptime_seconds: uptime.as_secs_f64(),
            custom_metrics,
            config: self.config.clone(),
        }
    }
    
    /// 启动Prometheus服务器
    /// 
    /// ## 返回值
    /// 
    /// 返回服务器句柄
    pub async fn start_prometheus_server(&self) -> anyhow::Result<tokio::task::JoinHandle<()>> {
        if !self.config.enabled || self.prometheus_handle.is_none() {
            return Err(anyhow::anyhow!("Prometheus未启用"));
        }
        
        let port = self.config.prometheus_port;
        let handle = tokio::spawn(async move {
            let addr: std::net::SocketAddr = format!("0.0.0.0:{}", port).parse().unwrap();
            let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
            
            info!("Prometheus服务器启动在端口 {}", port);
            
            loop {
                let (stream, _) = listener.accept().await.unwrap();
                let _ = tokio::spawn(async move {
                    // 这里应该实现HTTP服务器来提供/metrics端点
                    // 简化实现，实际应该使用axum或warp
                });
            }
        });
        
        Ok(handle)
    }
}

/// 性能统计信息
#[derive(Debug, Clone)]
pub struct PerformanceStats {
    /// 系统运行时间（秒）
    pub uptime_seconds: f64,
    /// 自定义指标
    pub custom_metrics: HashMap<String, f64>,
    /// 性能监测配置
    pub config: PerformanceConfig,
}

/// 性能追踪器
/// 
/// 用于追踪单个操作的性能
pub struct PerformanceTracer {
    monitor: Arc<PerformanceMonitor>,
    operation: String,
    start_time: Instant,
    labels: Option<HashMap<String, String>>,
}

impl PerformanceTracer {
    /// 创建新的性能追踪器
    /// 
    /// ## 参数
    /// 
    /// - `monitor`：性能监测器
    /// - `operation`：操作名称
    /// - `labels`：额外标签
    /// 
    /// ## 返回值
    /// 
    /// 返回性能追踪器
    pub fn new(
        monitor: Arc<PerformanceMonitor>,
        operation: String,
        labels: Option<HashMap<String, String>>,
    ) -> Self {
        Self {
            monitor,
            operation,
            start_time: Instant::now(),
            labels,
        }
    }
    
    /// 完成追踪并记录指标
    pub fn finish(mut self) {
        let duration = self.start_time.elapsed();
        let labels = self.labels.take(); // 取走而不是移动
        self.monitor.record_operation_duration(&self.operation, duration, labels);
    }
}

impl Drop for PerformanceTracer {
    fn drop(&mut self) {
        // 如果没有显式调用finish，自动完成追踪
        let duration = self.start_time.elapsed();
        let labels = self.labels.take(); // 取走而不是移动
        self.monitor.record_operation_duration(&self.operation, duration, labels);
    }
}

/// 创建性能追踪器的便捷宏
#[macro_export]
macro_rules! trace_performance {
    ($monitor:expr, $operation:expr) => {
        $crate::performance::PerformanceTracer::new(
            $monitor,
            $operation.to_string(),
            None,
        )
    };
    ($monitor:expr, $operation:expr, $labels:expr) => {
        $crate::performance::PerformanceTracer::new(
            $monitor,
            $operation.to_string(),
            Some($labels),
        )
    };
}

impl Default for PerformanceMonitor {
    fn default() -> Self {
        Self::new(PerformanceConfig::default()).unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    
    #[tokio::test]
    async fn test_performance_monitor_creation() {
        let config = PerformanceConfig::default();
        let monitor = PerformanceMonitor::new(config).unwrap();
        assert!(monitor.config.enabled);
    }
    
    #[tokio::test]
    async fn test_custom_metrics() {
        let monitor = PerformanceMonitor::default();
        
        monitor.set_custom_metric("test_metric".to_string(), 42.0).await;
        let value = monitor.get_custom_metric("test_metric").await;
        assert_eq!(value, Some(42.0));
    }
    
    #[tokio::test]
    async fn test_performance_stats() {
        let monitor = PerformanceMonitor::default();
        let stats = monitor.get_performance_stats().await;
        
        assert!(stats.uptime_seconds > 0.0);
        assert!(stats.config.enabled);
    }
    
    #[test]
    fn test_performance_tracer() {
        let monitor = Arc::new(PerformanceMonitor::default());
        let mut labels = HashMap::new();
        labels.insert("test".to_string(), "value".to_string());
        
        let _tracer = PerformanceTracer::new(
            monitor.clone(),
            "test_operation".to_string(),
            Some(labels),
        );
        
        // tracer会在drop时自动记录指标
    }
} 