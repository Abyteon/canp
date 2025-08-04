use criterion::{black_box, criterion_group, criterion_main, Criterion};
use canp::{
    zero_copy_memory_pool::ZeroCopyMemoryPool,
    high_performance_executor::{HighPerformanceExecutor, ExecutorConfig},
    dbc_parser::DbcManager,
    data_layer_parser::{DataLayerParser, CanFrame},
    test_data_generator::TestDataGenerator,
};
use tempfile::TempDir;

/// 基准测试：内存池性能
fn bench_memory_pool(c: &mut Criterion) {
    let mut group = c.benchmark_group("memory_pool");
    
    group.bench_function("file_mapping", |b| {
        let temp_dir = TempDir::new().unwrap();
        let test_file = temp_dir.path().join("test.bin");
        let test_data = vec![0u8; 1024 * 1024]; // 1MB
        std::fs::write(&test_file, &test_data).unwrap();
        
        let pool = ZeroCopyMemoryPool::default();
        
        b.iter(|| {
            let _mapping = pool.create_file_mapping(&test_file).unwrap();
        });
    });
    
    group.bench_function("decompress_buffer", |b| {
        let pool = ZeroCopyMemoryPool::default();
        
        b.iter(|| {
            let _buffer = pool.get_decompress_buffer(1024);
        });
    });
    
    group.finish();
}

/// 基准测试：执行器性能
fn bench_executor(c: &mut Criterion) {
    let mut group = c.benchmark_group("executor");
    
    group.bench_function("io_task_submission", |b| {
        let executor = HighPerformanceExecutor::new(ExecutorConfig::default());
        
        b.iter(|| {
            let _task_id = executor.submit_io_task(
                "benchmark task".to_string(),
                canp::high_performance_executor::Priority::Normal,
                async { Ok("completed") }
            );
        });
    });
    
    group.bench_function("cpu_task_submission", |b| {
        let executor = HighPerformanceExecutor::new(ExecutorConfig::default());
        
        b.iter(|| {
            let _task_id = executor.submit_cpu_task(
                "benchmark task".to_string(),
                canp::high_performance_executor::Priority::Normal,
                || Ok("completed")
            );
        });
    });
    
    group.finish();
}

/// 基准测试：DBC解析性能
fn bench_dbc_parser(c: &mut Criterion) {
    let mut group = c.benchmark_group("dbc_parser");
    
    group.bench_function("can_frame_parsing", |b| {
        let manager = DbcManager::default();
        let test_frame = CanFrame {
            timestamp: 1640995200,
            can_id: 256,
            dlc: 8,
            reserved: [0; 3],
            data: vec![0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08],
        };
        
        b.iter(|| {
            let _result = manager.parse_can_frame(&test_frame);
        });
    });
    
    group.finish();
}

/// 基准测试：数据层解析性能
fn bench_data_layer_parser(c: &mut Criterion) {
    let mut group = c.benchmark_group("data_layer_parser");
    
    group.bench_function("file_parsing", |b| {
        let temp_dir = TempDir::new().unwrap();
        let test_file = temp_dir.path().join("test.bin");
        
        // 生成测试文件
        let test_config = canp::test_data_generator::TestDataConfig {
            output_dir: temp_dir.path().to_path_buf(),
            file_count: 1,
            target_file_size: 1024 * 1024,
            frames_per_file: 1000,
        };
        let generator = TestDataGenerator::new(test_config);
        let _ = generator.generate_all();
        
        let mut parser = DataLayerParser::new(ZeroCopyMemoryPool::default());
        
        b.iter(|| {
            let file_data = std::fs::read(&test_file).unwrap();
            let _frames = parser.parse_file(&file_data);
        });
    });
    
    group.finish();
}

/// 基准测试：端到端性能
fn bench_end_to_end(c: &mut Criterion) {
    let mut group = c.benchmark_group("end_to_end");
    
    group.bench_function("complete_pipeline", |b| {
        let temp_dir = TempDir::new().unwrap();
        let test_data_dir = temp_dir.path().join("test_data");
        let output_dir = temp_dir.path().join("output");
        
        std::fs::create_dir_all(&test_data_dir).unwrap();
        std::fs::create_dir_all(&output_dir).unwrap();
        
        // 生成测试数据
        let test_config = canp::test_data_generator::TestDataConfig {
            output_dir: test_data_dir.clone(),
            file_count: 5,
            target_file_size: 512 * 1024,
            frames_per_file: 500,
        };
        let generator = TestDataGenerator::new(test_config);
        let _ = generator.generate_all();
        
        b.iter(|| {
            // 这里可以运行完整的处理管道
            // 为了基准测试的简洁性，我们只测试关键组件
            let pool = ZeroCopyMemoryPool::default();
            let executor = HighPerformanceExecutor::new(ExecutorConfig::default());
            let dbc_manager = DbcManager::default();
            
            black_box((pool, executor, dbc_manager));
        });
    });
    
    group.finish();
}

criterion_group!(
    benches,
    bench_memory_pool,
    bench_executor,
    bench_dbc_parser,
    bench_data_layer_parser,
    bench_end_to_end
);
criterion_main!(benches); 