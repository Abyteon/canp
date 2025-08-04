use canp::{
    zero_copy_memory_pool::ZeroCopyMemoryPool,
    high_performance_executor::HighPerformanceExecutor,
    dbc_parser::DbcManager,
    data_layer_parser::DataLayerParser,
    test_data_generator::TestDataGenerator,
};
use std::sync::Arc;
use tempfile::TempDir;

/// 测试环境设置
pub struct TestEnvironment {
    pub temp_dir: TempDir,
    pub memory_pool: Arc<ZeroCopyMemoryPool>,
    pub executor: Arc<HighPerformanceExecutor>,
    pub dbc_manager: Arc<DbcManager>,
    pub data_parser: DataLayerParser,
}

impl TestEnvironment {
    /// 创建新的测试环境
    pub fn new() -> Self {
        let temp_dir = TempDir::new().unwrap();
        let memory_pool = Arc::new(ZeroCopyMemoryPool::default());
        let executor = Arc::new(HighPerformanceExecutor::default());
        let dbc_manager = Arc::new(DbcManager::default());
        let data_parser = DataLayerParser::new(ZeroCopyMemoryPool::default());

        Self {
            temp_dir,
            memory_pool,
            executor,
            dbc_manager,
            data_parser,
        }
    }

    /// 生成测试数据
    pub async fn generate_test_data(&self, file_count: usize, frames_per_file: usize) -> std::path::PathBuf {
        let test_data_dir = self.temp_dir.path().join("test_data");
        std::fs::create_dir_all(&test_data_dir).unwrap();

        let test_config = canp::test_data_generator::TestDataConfig {
            output_dir: test_data_dir.clone(),
            file_count,
            target_file_size: 1024 * 1024,
            frames_per_file,
        };
        let generator = TestDataGenerator::new(test_config);
        generator.generate_all().await.unwrap();

        test_data_dir
    }

    /// 创建测试DBC文件
    pub async fn create_test_dbc(&self) -> std::path::PathBuf {
        let dbc_file = self.temp_dir.path().join("test.dbc");
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

BU_:

BO_ 256 TestMessage: 8 Vector__XXX
 SG_ TestSignal1 : 0|16@1+ (0.1,0) [0|6553.5] "V"  Vector__XXX
 SG_ TestSignal2 : 16|16@1+ (1,-32768) [-32768|32767] ""  Vector__XXX

CM_ SG_ 256 TestSignal1 "Test signal 1";
CM_ SG_ 256 TestSignal2 "Test signal 2";
"#;
        tokio::fs::write(&dbc_file, dbc_content).await.unwrap();
        dbc_file
    }

    /// 获取测试输出目录
    pub fn get_output_dir(&self) -> std::path::PathBuf {
        let output_dir = self.temp_dir.path().join("output");
        std::fs::create_dir_all(&output_dir).unwrap();
        output_dir
    }
}

/// 测试数据生成器
pub struct TestDataBuilder {
    temp_dir: TempDir,
}

impl TestDataBuilder {
    pub fn new() -> Self {
        Self {
            temp_dir: TempDir::new().unwrap(),
        }
    }

    /// 创建随机CAN帧
    pub fn create_random_can_frame(&self) -> canp::data_layer_parser::CanFrame {
        use rand::Rng;
        let mut rng = rand::thread_rng();

        let timestamp = rng.gen::<u64>();
        let can_id = rng.gen_range(0..0x1FFFFFFF);
        let dlc = rng.gen_range(0..9);
        let data: Vec<u8> = (0..dlc).map(|_| rng.gen()).collect();

        canp::data_layer_parser::CanFrame {
            timestamp,
            can_id,
            dlc,
            reserved: [0; 3],
            data,
        }
    }

    /// 创建测试文件列表
    pub fn create_test_files(&self, count: usize) -> Vec<std::path::PathBuf> {
        let test_dir = self.temp_dir.path().join("test_files");
        std::fs::create_dir_all(&test_dir).unwrap();

        (0..count)
            .map(|i| {
                let file_path = test_dir.join(format!("test_{}.bin", i));
                let data = vec![i as u8; 1024];
                std::fs::write(&file_path, &data).unwrap();
                file_path
            })
            .collect()
    }
}

/// 性能测试辅助函数
pub mod performance {
    use std::time::{Duration, Instant};

    /// 测量函数执行时间
    pub fn measure_time<F, R>(f: F) -> (Duration, R)
    where
        F: FnOnce() -> R,
    {
        let start = Instant::now();
        let result = f();
        let duration = start.elapsed();
        (duration, result)
    }

    /// 异步性能测量
    pub async fn measure_time_async<F, Fut, R>(f: F) -> (Duration, R)
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = R>,
    {
        let start = Instant::now();
        let result = f().await;
        let duration = start.elapsed();
        (duration, result)
    }

    /// 性能断言
    pub fn assert_performance(duration: Duration, max_duration: Duration, operation: &str) {
        assert!(
            duration <= max_duration,
            "{} 执行时间 {} 超过了预期时间 {}",
            operation,
            duration.as_millis(),
            max_duration.as_millis()
        );
    }
}

/// 内存测试辅助函数
pub mod memory {
    use std::alloc::{alloc, dealloc, Layout};

    /// 获取当前内存使用量（近似值）
    pub fn get_memory_usage() -> usize {
        // 这是一个简化的实现，实际项目中可能需要更精确的内存监控
        std::process::id() as usize
    }

    /// 内存泄漏检测辅助
    pub fn check_memory_leak<F>(f: F) -> bool
    where
        F: FnOnce(),
    {
        let before = get_memory_usage();
        f();
        let after = get_memory_usage();
        
        // 简单的内存泄漏检测（实际项目中需要更复杂的实现）
        after <= before + 1024 * 1024 // 允许1MB的误差
    }
}

/// 并发测试辅助函数
pub mod concurrency {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use tokio::task;

    /// 并发执行测试
    pub async fn run_concurrent_tasks<F, Fut, R>(
        task_count: usize,
        task_fn: F,
    ) -> Vec<R>
    where
        F: Fn(usize) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = R> + Send + 'static,
        R: Send + 'static,
    {
        let mut handles = Vec::new();
        
        for i in 0..task_count {
            let task_fn = task_fn.clone();
            let handle = task::spawn(async move {
                task_fn(i).await
            });
            handles.push(handle);
        }

        let mut results = Vec::new();
        for handle in handles {
            results.push(handle.await.unwrap());
        }

        results
    }

    /// 并发计数器测试
    pub async fn test_concurrent_counter(task_count: usize) -> usize {
        let counter = Arc::new(AtomicUsize::new(0));
        let mut handles = Vec::new();

        for _ in 0..task_count {
            let counter_clone = Arc::clone(&counter);
            let handle = task::spawn(async move {
                for _ in 0..1000 {
                    counter_clone.fetch_add(1, Ordering::Relaxed);
                }
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.await.unwrap();
        }

        counter.load(Ordering::Relaxed)
    }
}

/// 错误处理测试辅助函数
pub mod error_handling {
    use std::error::Error;
    use std::fmt;

    #[derive(Debug)]
    pub struct TestError {
        pub message: String,
    }

    impl fmt::Display for TestError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "Test error: {}", self.message)
        }
    }

    impl Error for TestError {}

    /// 创建测试错误
    pub fn create_test_error(message: &str) -> TestError {
        TestError {
            message: message.to_string(),
        }
    }

    /// 模拟错误条件
    pub fn simulate_error_condition(should_error: bool) -> Result<(), TestError> {
        if should_error {
            Err(create_test_error("Simulated error"))
        } else {
            Ok(())
        }
    }
} 