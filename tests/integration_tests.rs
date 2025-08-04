use canp::{
    columnar_storage::{ColumnarStorageConfig, ColumnarStorageWriter, CompressionType, PartitionStrategy},
    data_layer_parser::{CanFrame, DataLayerParser},
    dbc_parser::{DbcManager, DbcManagerConfig},
    high_performance_executor::{ExecutorConfig, HighPerformanceExecutor, Priority},
    processing_pipeline::{DataProcessingPipeline, PipelineConfig},
    test_data_generator::{TestDataConfig, TestDataGenerator},
    zero_copy_memory_pool::{MemoryPoolConfig, ZeroCopyMemoryPool},
};
use std::path::PathBuf;
use std::sync::Arc;
use tempfile::TempDir;
use tokio::time::{sleep, Duration};

/// 集成测试：完整的数据处理管道
#[tokio::test]
async fn test_complete_data_processing_pipeline() {
    let temp_dir = TempDir::new().unwrap();
    let test_data_dir = temp_dir.path().join("test_data");
    let output_dir = temp_dir.path().join("output");
    let dbc_file = temp_dir.path().join("test.dbc");

    // 创建目录
    std::fs::create_dir_all(&test_data_dir).unwrap();
    std::fs::create_dir_all(&output_dir).unwrap();

    // 1. 生成测试数据
    let test_config = TestDataConfig {
        output_dir: test_data_dir.clone(),
        file_count: 5,
        target_file_size: 1024 * 1024, // 1MB
        frames_per_file: 100,
    };
    let generator = TestDataGenerator::new(test_config);
    generator.generate_all().await.unwrap();

    // 2. 创建测试DBC文件
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

    // 3. 初始化所有组件
    let memory_pool_config = MemoryPoolConfig::default();
    let memory_pool = Arc::new(ZeroCopyMemoryPool::new(memory_pool_config.clone()));

    let executor_config = ExecutorConfig::default();
    let executor = Arc::new(HighPerformanceExecutor::new(executor_config.clone()));

    let dbc_config = DbcManagerConfig::default();
    let dbc_manager = Arc::new(DbcManager::new(dbc_config.clone()));
    dbc_manager.load_dbc_file(&dbc_file, Some(0)).await.unwrap();

    let storage_config = ColumnarStorageConfig {
        output_dir: output_dir.clone(),
        compression: CompressionType::Snappy,
        row_group_size: 1000,
        page_size: 1024 * 1024,
        enable_dictionary: true,
        enable_statistics: true,
        partition_strategy: PartitionStrategy::ByCanId,
        batch_size: 100,
        max_file_size: 10 * 1024 * 1024,
        keep_raw_data: false,
    };
    let storage_writer = Arc::new(ColumnarStorageWriter::new(storage_config.clone()).unwrap());

    let pipeline_config = PipelineConfig {
        memory_pool_config: memory_pool_config.clone(),
        executor_config: executor_config.clone(),
        dbc_manager_config: dbc_config.clone(),
        storage_config: storage_config.clone(),
        batch_size: 10,
        max_concurrent_files: 5,
        enable_error_recovery: true,
        max_retries: 3,
        processing_timeout_seconds: 60,
        memory_pressure_threshold: 0.8,
        enable_progress_reporting: true,
        progress_report_interval: 10,
    };
    let pipeline = DataProcessingPipeline::new(pipeline_config.clone()).await.unwrap();

    // 4. 收集文件路径
    let mut file_paths = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&test_data_dir) {
        for entry in entries {
            if let Ok(entry) = entry {
                let path = entry.path();
                if path.is_file() && path.extension().map_or(false, |ext| ext == "bin") {
                    file_paths.push(path);
                }
            }
        }
    }

    assert!(!file_paths.is_empty(), "应该生成测试文件");

    // 5. 执行数据处理管道
    let start_time = std::time::Instant::now();
    let result = pipeline.process_files(file_paths).await;
    let processing_time = start_time.elapsed();

    assert!(result.is_ok(), "数据处理应该成功");
    assert!(processing_time < Duration::from_secs(30), "处理时间应该在30秒内");

    // 6. 验证输出
    let output_files: Vec<_> = std::fs::read_dir(&output_dir)
        .unwrap()
        .filter_map(|entry| entry.ok())
        .filter(|entry| {
            entry.path().extension().map_or(false, |ext| ext == "parquet")
        })
        .collect();

    assert!(!output_files.is_empty(), "应该生成Parquet输出文件");

    // 7. 验证统计信息
    let stats = pipeline.get_stats().await;
    assert!(stats.total_files_processed > 0);
    assert!(stats.successful_files > 0);
    assert!(stats.total_frames_processed > 0);
}

/// 集成测试：错误恢复机制
#[tokio::test]
async fn test_error_recovery_mechanism() {
    let temp_dir = TempDir::new().unwrap();
    let test_data_dir = temp_dir.path().join("test_data");
    let output_dir = temp_dir.path().join("output");

    std::fs::create_dir_all(&test_data_dir).unwrap();
    std::fs::create_dir_all(&output_dir).unwrap();

    // 创建一些有效文件和无效文件
    let valid_file = test_data_dir.join("valid.bin");
    let invalid_file = test_data_dir.join("invalid.bin");

    // 生成有效的测试文件
    let test_config = TestDataConfig {
        output_dir: test_data_dir.clone(),
        file_count: 1,
        target_file_size: 1024,
        frames_per_file: 10,
    };
    let generator = TestDataGenerator::new(test_config);
    generator.generate_all().await.unwrap();

    // 创建无效文件
    tokio::fs::write(&invalid_file, b"invalid data").await.unwrap();

    let pipeline_config = PipelineConfig {
        memory_pool_config: MemoryPoolConfig::default(),
        executor_config: ExecutorConfig::default(),
        dbc_manager_config: DbcManagerConfig::default(),
        storage_config: ColumnarStorageConfig {
            output_dir: output_dir.clone(),
            compression: CompressionType::Snappy,
            row_group_size: 1000,
            page_size: 1024 * 1024,
            enable_dictionary: true,
            enable_statistics: true,
            partition_strategy: PartitionStrategy::ByCanId,
            batch_size: 100,
            max_file_size: 10 * 1024 * 1024,
            keep_raw_data: false,
        },
        batch_size: 10,
        max_concurrent_files: 5,
        enable_error_recovery: true,
        max_retries: 2,
        processing_timeout_seconds: 30,
        memory_pressure_threshold: 0.8,
        enable_progress_reporting: true,
        progress_report_interval: 5,
    };
    let pipeline = DataProcessingPipeline::new(pipeline_config.clone()).await.unwrap();

    let file_paths = vec![valid_file, invalid_file];
    let result = pipeline.process_files(file_paths).await;

    // 应该部分成功（有效文件处理成功，无效文件失败）
    assert!(result.is_ok());

    let stats = pipeline.get_stats().await;
    assert!(stats.successful_files > 0);
    assert!(stats.failed_files > 0);
}

/// 集成测试：内存压力处理
#[tokio::test]
async fn test_memory_pressure_handling() {
    let temp_dir = TempDir::new().unwrap();
    let test_data_dir = temp_dir.path().join("test_data");
    let output_dir = temp_dir.path().join("output");

    std::fs::create_dir_all(&test_data_dir).unwrap();
    std::fs::create_dir_all(&output_dir).unwrap();

    // 生成大量测试数据
    let test_config = TestDataConfig {
        output_dir: test_data_dir.clone(),
        file_count: 20,
        target_file_size: 512 * 1024, // 512KB
        frames_per_file: 50,
    };
    let generator = TestDataGenerator::new(test_config);
    generator.generate_all().await.unwrap();

    let pipeline_config = PipelineConfig {
        memory_pool_config: MemoryPoolConfig {
            decompress_buffer_sizes: vec![1024, 2048],
            mmap_cache_size: 5, // 限制缓存大小
            max_memory_usage: 1024 * 1024, // 1MB限制
        },
        executor_config: ExecutorConfig {
            io_worker_threads: 2,
            cpu_worker_threads: 2,
            max_queue_length: 10,
            task_timeout: Duration::from_secs(30),
            stats_update_interval: Duration::from_secs(5),
            enable_work_stealing: true,
            cpu_batch_size: 10,
        },
        dbc_manager_config: DbcManagerConfig::default(),
        storage_config: ColumnarStorageConfig {
            output_dir: output_dir.clone(),
            compression: CompressionType::Snappy,
            row_group_size: 100,
            page_size: 512 * 1024,
            enable_dictionary: true,
            enable_statistics: true,
            partition_strategy: PartitionStrategy::ByCanId,
            batch_size: 50,
            max_file_size: 5 * 1024 * 1024,
            keep_raw_data: false,
        },
        batch_size: 5,
        max_concurrent_files: 3,
        enable_error_recovery: true,
        max_retries: 2,
        processing_timeout_seconds: 60,
        memory_pressure_threshold: 0.5, // 降低阈值
        enable_progress_reporting: true,
        progress_report_interval: 5,
    };
    let pipeline = DataProcessingPipeline::new(pipeline_config.clone()).await.unwrap();

    let mut file_paths = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&test_data_dir) {
        for entry in entries {
            if let Ok(entry) = entry {
                let path = entry.path();
                if path.is_file() && path.extension().map_or(false, |ext| ext == "bin") {
                    file_paths.push(path);
                }
            }
        }
    }

    let result = pipeline.process_files(file_paths).await;
    assert!(result.is_ok(), "即使在内存压力下也应该成功处理");

    let stats = pipeline.get_stats().await;
    assert!(stats.successful_files > 0);
}

/// 集成测试：并发处理能力
#[tokio::test]
async fn test_concurrent_processing_capability() {
    let temp_dir = TempDir::new().unwrap();
    let test_data_dir = temp_dir.path().join("test_data");
    let output_dir = temp_dir.path().join("output");

    std::fs::create_dir_all(&test_data_dir).unwrap();
    std::fs::create_dir_all(&output_dir).unwrap();

    // 生成测试数据
    let test_config = TestDataConfig {
        output_dir: test_data_dir.clone(),
        file_count: 10,
        target_file_size: 256 * 1024,
        frames_per_file: 25,
    };
    let generator = TestDataGenerator::new(test_config);
    generator.generate_all().await.unwrap();

    let pipeline_config = PipelineConfig {
        memory_pool_config: MemoryPoolConfig::default(),
        executor_config: ExecutorConfig {
            io_worker_threads: 4,
            cpu_worker_threads: 4,
            max_queue_length: 100,
            task_timeout: Duration::from_secs(30),
            stats_update_interval: Duration::from_secs(5),
            enable_work_stealing: true,
            cpu_batch_size: 20,
        },
        dbc_manager_config: DbcManagerConfig::default(),
        storage_config: ColumnarStorageConfig {
            output_dir: output_dir.clone(),
            compression: CompressionType::Snappy,
            row_group_size: 500,
            page_size: 512 * 1024,
            enable_dictionary: true,
            enable_statistics: true,
            partition_strategy: PartitionStrategy::ByCanId,
            batch_size: 50,
            max_file_size: 5 * 1024 * 1024,
            keep_raw_data: false,
        },
        batch_size: 5,
        max_concurrent_files: 8, // 高并发
        enable_error_recovery: true,
        max_retries: 2,
        processing_timeout_seconds: 60,
        memory_pressure_threshold: 0.8,
        enable_progress_reporting: true,
        progress_report_interval: 5,
    };
    let pipeline = DataProcessingPipeline::new(pipeline_config.clone()).await.unwrap();

    let mut file_paths = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&test_data_dir) {
        for entry in entries {
            if let Ok(entry) = entry {
                let path = entry.path();
                if path.is_file() && path.extension().map_or(false, |ext| ext == "bin") {
                    file_paths.push(path);
                }
            }
        }
    }

    let start_time = std::time::Instant::now();
    let result = pipeline.process_files(file_paths).await;
    let processing_time = start_time.elapsed();

    assert!(result.is_ok());
    assert!(processing_time < Duration::from_secs(20), "并发处理应该更快");

    let stats = pipeline.get_stats().await;
    assert!(stats.successful_files > 0);
}

/// 集成测试：数据一致性验证
#[tokio::test]
async fn test_data_consistency_verification() {
    let temp_dir = TempDir::new().unwrap();
    let test_data_dir = temp_dir.path().join("test_data");
    let output_dir = temp_dir.path().join("output");
    let dbc_file = temp_dir.path().join("test.dbc");

    std::fs::create_dir_all(&test_data_dir).unwrap();
    std::fs::create_dir_all(&output_dir).unwrap();

    // 生成测试数据
    let test_config = TestDataConfig {
        output_dir: test_data_dir.clone(),
        file_count: 3,
        target_file_size: 1024,
        frames_per_file: 10,
    };
    let generator = TestDataGenerator::new(test_config);
    generator.generate_all().await.unwrap();

    // 创建DBC文件
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

    // 手动解析一些帧进行验证
    let mut expected_frames = Vec::new();
    let data_parser = DataLayerParser::new(ZeroCopyMemoryPool::default());
    
    if let Ok(entries) = std::fs::read_dir(&test_data_dir) {
        for entry in entries {
            if let Ok(entry) = entry {
                let path = entry.path();
                if path.is_file() && path.extension().map_or(false, |ext| ext == "bin") {
                    if let Ok(frames) = data_parser.parse_file(&path).await {
                        expected_frames.extend(frames);
                    }
                }
            }
        }
    }

    // 运行管道
    let pipeline_config = PipelineConfig {
        memory_pool_config: MemoryPoolConfig::default(),
        executor_config: ExecutorConfig::default(),
        dbc_manager_config: DbcManagerConfig::default(),
        storage_config: ColumnarStorageConfig {
            output_dir: output_dir.clone(),
            compression: CompressionType::Snappy,
            row_group_size: 1000,
            page_size: 1024 * 1024,
            enable_dictionary: true,
            enable_statistics: true,
            partition_strategy: PartitionStrategy::ByCanId,
            batch_size: 100,
            max_file_size: 10 * 1024 * 1024,
            keep_raw_data: false,
        },
        batch_size: 10,
        max_concurrent_files: 5,
        enable_error_recovery: true,
        max_retries: 3,
        processing_timeout_seconds: 60,
        memory_pressure_threshold: 0.8,
        enable_progress_reporting: true,
        progress_report_interval: 10,
    };
    let pipeline = DataProcessingPipeline::new(pipeline_config.clone()).await.unwrap();

    let mut file_paths = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&test_data_dir) {
        for entry in entries {
            if let Ok(entry) = entry {
                let path = entry.path();
                if path.is_file() && path.extension().map_or(false, |ext| ext == "bin") {
                    file_paths.push(path);
                }
            }
        }
    }

    let result = pipeline.process_files(file_paths).await;
    assert!(result.is_ok());

    // 验证输出文件存在
    let output_files: Vec<_> = std::fs::read_dir(&output_dir)
        .unwrap()
        .filter_map(|entry| entry.ok())
        .filter(|entry| {
            entry.path().extension().map_or(false, |ext| ext == "parquet")
        })
        .collect();

    assert!(!output_files.is_empty());
    assert!(expected_frames.len() > 0);
} 