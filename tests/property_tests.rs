use canp::{
    data_layer_parser::CanFrame,
    zero_copy_memory_pool::{MemoryPoolConfig, ZeroCopyMemoryPool, ZeroCopyBuffer, MutableMemoryBuffer},
    dbc_parser::DbcManager,
};
use proptest::prelude::*;
use std::sync::Arc;
use tempfile::TempDir;

/// 属性测试：CAN帧数据一致性
proptest! {
    #[test]
    fn test_can_frame_data_consistency(
        timestamp in 0u64..u64::MAX,
        can_id in 0u32..0x1FFFFFFF, // 标准CAN ID范围
        dlc in 0u8..9, // CAN DLC范围
        data_len in 0usize..9,
        data in prop::collection::vec(any::<u8>(), data_len)
    ) {
        // 确保数据长度与DLC一致
        let actual_data = if data.len() > dlc as usize {
            data[..dlc as usize].to_vec()
        } else {
            let mut padded_data = data.clone();
            padded_data.resize(dlc as usize, 0);
            padded_data
        };

        let frame = CanFrame {
            timestamp,
            can_id,
            dlc,
            reserved: [0; 3],
            data: actual_data.clone(),
        };

        // 验证数据一致性
        assert_eq!(frame.timestamp, timestamp);
        assert_eq!(frame.can_id, can_id);
        assert_eq!(frame.dlc, dlc);
        assert_eq!(frame.data.len(), dlc as usize);
        assert_eq!(frame.data, actual_data);
    }
}

/// 属性测试：零拷贝缓冲区操作
proptest! {
    #[test]
    fn test_zero_copy_buffer_operations(
        data in prop::collection::vec(any::<u8>(), 0..1000),
        start in 0usize..1000,
        len in 0usize..1000
    ) {
        if data.is_empty() {
            return Ok(());
        }

        let buffer = ZeroCopyBuffer::from_vec(data.clone());
        
        // 测试切片操作
        if start < data.len() && start + len <= data.len() {
            let slice = buffer.slice(start..start + len);
            assert_eq!(slice.as_slice(), &data[start..start + len]);
        }

        // 测试分割操作
        if start < data.len() {
            let mut buffer_clone = buffer.clone();
            let split = buffer_clone.split_to(start);
            assert_eq!(split.as_slice(), &data[..start]);
            assert_eq!(buffer_clone.as_slice(), &data[start..]);
        }
    }
}

/// 属性测试：内存池配置有效性
proptest! {
    #[test]
    fn test_memory_pool_config_validity(
        buffer_sizes in prop::collection::vec(1usize..1024*1024, 1..10),
        cache_size in 1usize..10000,
        max_memory in 1024*1024usize..1024*1024*1024
    ) {
        let config = MemoryPoolConfig {
            decompress_buffer_sizes: buffer_sizes.clone(),
            mmap_cache_size: cache_size,
            max_memory_usage: max_memory,
        };

        // 验证配置有效性
        assert!(!config.decompress_buffer_sizes.is_empty());
        assert!(config.mmap_cache_size > 0);
        assert!(config.max_memory_usage > 0);
        
        // 验证缓冲区大小排序
        let mut sorted_sizes = buffer_sizes.clone();
        sorted_sizes.sort();
        assert_eq!(config.decompress_buffer_sizes, sorted_sizes);
    }
}

/// 属性测试：可变缓冲区操作
proptest! {
    #[test]
    fn test_mutable_buffer_operations(
        initial_capacity in 1usize..10000,
        data in prop::collection::vec(any::<u8>(), 0..1000),
        additional_capacity in 0usize..10000
    ) {
        let mut buffer = MutableMemoryBuffer::with_capacity(initial_capacity);
        
        // 初始状态验证
        assert_eq!(buffer.len(), 0);
        assert!(buffer.is_empty());
        assert!(buffer.buffer.capacity() >= initial_capacity);

        // 写入数据
        if !data.is_empty() {
            buffer.put_slice(&data);
            assert_eq!(buffer.as_slice(), data.as_slice());
            assert_eq!(buffer.len(), data.len());
            assert!(!buffer.is_empty());
        }

        // 容量扩展
        let initial_cap = buffer.buffer.capacity();
        buffer.reserve(additional_capacity);
        assert!(buffer.buffer.capacity() >= initial_cap + additional_capacity);

        // 清空操作
        buffer.clear();
        assert_eq!(buffer.len(), 0);
        assert!(buffer.is_empty());

        // 冻结操作
        buffer.put_slice(&data);
        let frozen = buffer.freeze();
        assert_eq!(frozen.as_slice(), data.as_slice());
    }
}

/// 属性测试：DBC管理器配置
proptest! {
    #[test]
    fn test_dbc_manager_config_validity(
        max_cached_files in 1usize..1000,
        cache_expire_seconds in 1u64..86400, // 1秒到1天
        reload_check_interval in 1u64..3600, // 1秒到1小时
        max_load_threads in 1usize..100
    ) {
        let config = canp::dbc_parser::DbcManagerConfig {
            max_cached_files,
            cache_expire_seconds,
            auto_reload: true,
            reload_check_interval,
            default_priority: 0,
            parallel_loading: true,
            max_load_threads,
        };

        // 验证配置有效性
        assert!(config.max_cached_files > 0);
        assert!(config.cache_expire_seconds > 0);
        assert!(config.reload_check_interval > 0);
        assert!(config.max_load_threads > 0);
        assert!(config.reload_check_interval <= config.cache_expire_seconds);
    }
}

/// 属性测试：执行器配置
proptest! {
    #[test]
    fn test_executor_config_validity(
        io_workers in 1usize..100,
        cpu_workers in 1usize..100,
        max_queue_length in 1usize..100000,
        cpu_batch_size in 1usize..1000
    ) {
        let config = canp::high_performance_executor::ExecutorConfig {
            io_worker_threads: io_workers,
            cpu_worker_threads: cpu_workers,
            max_queue_length,
            task_timeout: std::time::Duration::from_secs(30),
            stats_update_interval: std::time::Duration::from_secs(5),
            enable_work_stealing: true,
            cpu_batch_size,
        };

        // 验证配置有效性
        assert!(config.io_worker_threads > 0);
        assert!(config.cpu_worker_threads > 0);
        assert!(config.max_queue_length > 0);
        assert!(config.cpu_batch_size > 0);
        assert!(config.task_timeout > std::time::Duration::from_secs(0));
        assert!(config.stats_update_interval > std::time::Duration::from_secs(0));
    }
}

/// 属性测试：列式存储配置
proptest! {
    #[test]
    fn test_columnar_storage_config_validity(
        row_group_size in 1usize..100000,
        page_size in 1024usize..1024*1024*100,
        batch_size in 1usize..10000,
        max_file_size in 1024*1024usize..1024*1024*1024*10
    ) {
        let config = canp::columnar_storage::ColumnarStorageConfig {
            output_dir: std::path::PathBuf::from("/tmp"),
            compression: canp::columnar_storage::CompressionType::Snappy,
            row_group_size,
            page_size,
            enable_dictionary: true,
            enable_statistics: true,
            partition_strategy: canp::columnar_storage::PartitionStrategy::ByCanId,
            batch_size,
            max_file_size,
            keep_raw_data: false,
        };

        // 验证配置有效性
        assert!(config.row_group_size > 0);
        assert!(config.page_size > 0);
        assert!(config.batch_size > 0);
        assert!(config.max_file_size > 0);
        assert!(config.page_size >= config.row_group_size);
    }
}

/// 属性测试：文件头解析一致性
proptest! {
    #[test]
    fn test_file_header_parsing_consistency(
        file_size in 100u32..1024*1024,
        frame_count in 1u32..10000,
        compressed_data_length in 10u32..1024*1024
    ) {
        // 创建模拟文件头
        let mut header = vec![0u8; 64];
        
        // 写入文件大小 (bytes 0-3)
        header[0..4].copy_from_slice(&file_size.to_le_bytes());
        
        // 写入帧数量 (bytes 4-7)
        header[4..8].copy_from_slice(&frame_count.to_le_bytes());
        
        // 写入压缩数据长度 (bytes 31-34)
        header[31..35].copy_from_slice(&compressed_data_length.to_le_bytes());

        // 验证解析一致性
        let parsed_file_size = u32::from_le_bytes([header[0], header[1], header[2], header[3]]);
        let parsed_frame_count = u32::from_le_bytes([header[4], header[5], header[6], header[7]]);
        let parsed_compressed_length = u32::from_le_bytes([header[31], header[32], header[33], header[34]]);

        assert_eq!(parsed_file_size, file_size);
        assert_eq!(parsed_frame_count, frame_count);
        assert_eq!(parsed_compressed_length, compressed_data_length);
    }
}

/// 属性测试：位提取操作
proptest! {
    #[test]
    fn test_bit_extraction_operations(
        data in prop::collection::vec(any::<u8>(), 1..100),
        start_bit in 0usize..800,
        length in 1usize..64
    ) {
        if data.is_empty() || start_bit + length > data.len() * 8 {
            return Ok(());
        }

        let manager = DbcManager::default();
        
        // 测试小端序位提取
        let little_endian_result = manager.extract_little_endian_bits(&data, start_bit, length);
        assert!(little_endian_result.is_ok());
        
        // 测试大端序位提取
        let big_endian_result = manager.extract_big_endian_bits(&data, start_bit, length);
        assert!(big_endian_result.is_ok());
        
        // 验证提取的值在合理范围内
        let le_value = little_endian_result.unwrap();
        let be_value = big_endian_result.unwrap();
        
        let max_value = (1u64 << length) - 1;
        assert!(le_value <= max_value);
        assert!(be_value <= max_value);
    }
}

/// 属性测试：内存映射操作
proptest! {
    #[test]
    fn test_memory_mapping_operations(
        data_size in 1usize..1024*1024,
        offset in 0usize..1024*1024,
        slice_length in 1usize..1024*1024
    ) {
        if data_size == 0 || offset >= data_size || offset + slice_length > data_size {
            return Ok(());
        }

        let temp_dir = TempDir::new().unwrap();
        let test_file = temp_dir.path().join("test.bin");
        
        // 创建测试数据
        let test_data: Vec<u8> = (0..data_size).map(|i| (i % 256) as u8).collect();
        std::fs::write(&test_file, &test_data).unwrap();

        let pool = ZeroCopyMemoryPool::default();
        let mapping = pool.create_file_mapping(&test_file).unwrap();

        // 验证基本属性
        assert_eq!(mapping.len(), data_size);
        assert!(!mapping.is_empty());
        assert_eq!(mapping.as_slice(), test_data.as_slice());

        // 测试切片操作
        let slice = mapping.slice(offset, slice_length);
        assert_eq!(slice.len(), slice_length);
        assert_eq!(slice, &test_data[offset..offset + slice_length]);

        // 测试指针和长度
        let (ptr, len) = mapping.as_ptr_and_len();
        assert_eq!(len, data_size);
        assert!(!ptr.is_null());
    }
}

/// 属性测试：任务优先级排序
proptest! {
    #[test]
    fn test_task_priority_ordering(
        priorities in prop::collection::vec(0u32..4, 10..100)
    ) {
        use canp::high_performance_executor::Priority;
        
        let mut priority_tasks: Vec<(u32, Priority)> = priorities.iter()
            .map(|&p| (p, match p {
                0 => Priority::Low,
                1 => Priority::Normal,
                2 => Priority::High,
                3 => Priority::Critical,
                _ => Priority::Normal,
            }))
            .collect();

        // 按优先级排序
        priority_tasks.sort_by(|a, b| b.1.cmp(&a.1)); // 高优先级在前

        // 验证排序正确性
        for i in 1..priority_tasks.len() {
            let prev_priority = priority_tasks[i-1].1 as u32;
            let curr_priority = priority_tasks[i].1 as u32;
            assert!(prev_priority >= curr_priority);
        }
    }
}

/// 属性测试：数据压缩一致性
proptest! {
    #[test]
    fn test_data_compression_consistency(
        original_data in prop::collection::vec(any::<u8>(), 100..10000)
    ) {
        use flate2::write::GzEncoder;
        use flate2::read::GzDecoder;
        use std::io::{Read, Write};

        // 压缩数据
        let mut encoder = GzEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(&original_data).unwrap();
        let compressed_data = encoder.finish().unwrap();

        // 解压数据
        let mut decoder = GzDecoder::new(&compressed_data[..]);
        let mut decompressed_data = Vec::new();
        decoder.read_to_end(&mut decompressed_data).unwrap();

        // 验证数据一致性
        assert_eq!(decompressed_data, original_data);
        assert!(compressed_data.len() > 0);
    }
} 