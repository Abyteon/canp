"""
配置模块单元测试
"""

import pytest
import tempfile
import os
from pathlib import Path
from unittest.mock import patch

from canp.config import (
    MemoryPoolConfig,
    ExecutorConfig,
    DBCParserConfig,
    StorageConfig,
    CANPConfig,
    create_default_config,
    create_high_performance_config,
    create_memory_efficient_config,
    validate_config,
    load_config_from_file,
    save_config_to_file,
    load_config_from_env
)


class TestMemoryPoolConfig:
    """内存池配置测试"""
    
    def test_default_config(self):
        """测试默认配置"""
        config = MemoryPoolConfig()
        
        assert config.max_memory_mb == 2048
        assert config.buffer_sizes == [1024, 2048, 4096, 8192, 16384]
        assert config.mmap_cache_size == 1000
        assert config.enable_compression is True
    
    def test_custom_config(self):
        """测试自定义配置"""
        config = MemoryPoolConfig(
            max_memory_mb=4096,
            buffer_sizes=[512, 1024, 2048],
            mmap_cache_size=500,
            enable_compression=False
        )
        
        assert config.max_memory_mb == 4096
        assert config.buffer_sizes == [512, 1024, 2048]
        assert config.mmap_cache_size == 500
        assert config.enable_compression is False
    
    def test_buffer_sizes_validation(self):
        """测试缓冲区大小验证"""
        # 空列表应该失败
        with pytest.raises(ValueError, match="缓冲区大小列表不能为空"):
            MemoryPoolConfig(buffer_sizes=[])
        
        # 非升序应该失败
        with pytest.raises(ValueError, match="缓冲区大小必须按升序排列"):
            MemoryPoolConfig(buffer_sizes=[4096, 1024, 2048])
        
        # 重复值应该失败
        with pytest.raises(ValueError, match="缓冲区大小不能重复"):
            MemoryPoolConfig(buffer_sizes=[1024, 1024, 2048])
    
    def test_memory_limit_validation(self):
        """测试内存限制验证"""
        # 超过上限应该失败
        with pytest.raises(ValueError):
            MemoryPoolConfig(max_memory_mb=32769)  # 超过32GB
        
        # 负数应该失败
        with pytest.raises(ValueError):
            MemoryPoolConfig(max_memory_mb=-1)


class TestExecutorConfig:
    """执行器配置测试"""
    
    def test_default_config(self):
        """测试默认配置"""
        config = ExecutorConfig()
        
        assert config.cpu_workers is not None  # 自动检测
        assert config.io_workers == 16
        assert config.max_concurrent == 100
        assert config.task_timeout == 300.0
        assert config.enable_work_stealing is False
    
    def test_custom_config(self):
        """测试自定义配置"""
        config = ExecutorConfig(
            cpu_workers=8,
            io_workers=32,
            max_concurrent=200,
            task_timeout=600.0,
            enable_work_stealing=True
        )
        
        assert config.cpu_workers == 8
        assert config.io_workers == 32
        assert config.max_concurrent == 200
        assert config.task_timeout == 600.0
        assert config.enable_work_stealing is True
    
    @patch('multiprocessing.cpu_count')
    def test_auto_cpu_workers(self, mock_cpu_count):
        """测试自动CPU工作进程数"""
        mock_cpu_count.return_value = 16
        
        config = ExecutorConfig(cpu_workers=None)
        assert config.cpu_workers == 16


class TestDBCParserConfig:
    """DBC解析器配置测试"""
    
    def test_default_config(self):
        """测试默认配置"""
        config = DBCParserConfig()
        
        assert config.cache_enabled is True
        assert config.cache_dir == ".cache"
        assert config.max_workers == 4
        assert config.auto_reload is True
        assert config.cache_expire_seconds == 3600
    
    def test_custom_config(self):
        """测试自定义配置"""
        config = DBCParserConfig(
            cache_enabled=False,
            cache_dir="/tmp/cache",
            max_workers=8,
            auto_reload=False,
            cache_expire_seconds=7200
        )
        
        assert config.cache_enabled is False
        assert config.cache_dir == "/tmp/cache"
        assert config.max_workers == 8
        assert config.auto_reload is False
        assert config.cache_expire_seconds == 7200


class TestStorageConfig:
    """存储配置测试"""
    
    def test_default_config(self):
        """测试默认配置"""
        config = StorageConfig()
        
        assert config.output_dir == "output"
        assert config.compression == "snappy"
        assert config.partition_strategy == "time"
        assert config.batch_size == 10000
        assert config.enable_metadata is True
    
    def test_custom_config(self):
        """测试自定义配置"""
        config = StorageConfig(
            output_dir="/data/output",
            compression="zstd",
            partition_strategy="file",
            batch_size=50000,
            enable_metadata=False
        )
        
        assert config.output_dir == "/data/output"
        assert config.compression == "zstd"
        assert config.partition_strategy == "file"
        assert config.batch_size == 50000
        assert config.enable_metadata is False
    
    def test_compression_validation(self):
        """测试压缩算法验证"""
        # 无效的压缩算法应该失败
        with pytest.raises(ValueError):
            StorageConfig(compression="invalid")
        
        # 有效的压缩算法应该通过
        valid_compressions = ["snappy", "gzip", "brotli", "zstd", "lz4"]
        for compression in valid_compressions:
            config = StorageConfig(compression=compression)
            assert config.compression == compression
    
    def test_partition_strategy_validation(self):
        """测试分区策略验证"""
        # 无效的分区策略应该失败
        with pytest.raises(ValueError):
            StorageConfig(partition_strategy="invalid")
        
        # 有效的分区策略应该通过
        valid_strategies = ["time", "file", "id", "custom"]
        for strategy in valid_strategies:
            config = StorageConfig(partition_strategy=strategy)
            assert config.partition_strategy == strategy


class TestCANPConfig:
    """CANP主配置测试"""
    
    def test_default_config(self):
        """测试默认配置"""
        config = CANPConfig()
        
        assert isinstance(config.memory_pool, MemoryPoolConfig)
        assert isinstance(config.executor, ExecutorConfig)
        assert isinstance(config.dbc_parser, DBCParserConfig)
        assert isinstance(config.storage, StorageConfig)
        assert config.input_dir is None
        assert config.dbc_file is None
        assert config.log_level == "INFO"
        assert config.enable_profiling is False
    
    def test_custom_config(self):
        """测试自定义配置"""
        config = CANPConfig(
            input_dir="/data/input",
            dbc_file="/data/sample.dbc",
            log_level="DEBUG",
            enable_profiling=True
        )
        
        assert config.input_dir == "/data/input"
        assert config.dbc_file == "/data/sample.dbc"
        assert config.log_level == "DEBUG"
        assert config.enable_profiling is True
    
    def test_input_dir_validation(self):
        """测试输入目录验证"""
        # 不存在的目录应该失败
        with pytest.raises(ValueError, match="输入目录不存在"):
            CANPConfig(input_dir="/nonexistent/directory")
        
        # 文件路径应该失败
        with tempfile.NamedTemporaryFile() as f:
            with pytest.raises(ValueError, match="输入路径不是目录"):
                CANPConfig(input_dir=f.name)
    
    def test_dbc_file_validation(self):
        """测试DBC文件验证"""
        # 不存在的文件应该失败
        with pytest.raises(ValueError, match="DBC文件不存在"):
            CANPConfig(dbc_file="/nonexistent/file.dbc")
        
        # 目录路径应该失败
        with tempfile.TemporaryDirectory() as temp_dir:
            with pytest.raises(ValueError, match="DBC路径不是文件"):
                CANPConfig(dbc_file=temp_dir)
        
        # 错误的扩展名应该失败
        with tempfile.NamedTemporaryFile(suffix='.txt') as f:
            with pytest.raises(ValueError, match="文件扩展名必须是.dbc"):
                CANPConfig(dbc_file=f.name)
    
    def test_log_level_validation(self):
        """测试日志级别验证"""
        # 无效的日志级别应该失败
        with pytest.raises(ValueError):
            CANPConfig(log_level="INVALID")
        
        # 有效的日志级别应该通过
        valid_levels = ["DEBUG", "INFO", "WARNING", "ERROR", "CRITICAL"]
        for level in valid_levels:
            config = CANPConfig(log_level=level)
            assert config.log_level == level


class TestConfigFactories:
    """配置工厂函数测试"""
    
    def test_create_default_config(self):
        """测试创建默认配置"""
        config = create_default_config()
        
        assert isinstance(config, CANPConfig)
        assert config.memory_pool.max_memory_mb == 2048
        assert config.executor.io_workers == 16
    
    def test_create_high_performance_config(self):
        """测试创建高性能配置"""
        config = create_high_performance_config()
        
        assert isinstance(config, CANPConfig)
        assert config.memory_pool.max_memory_mb == 4096
        assert config.storage.compression == "zstd"
        assert config.storage.batch_size == 50000
    
    def test_create_memory_efficient_config(self):
        """测试创建内存高效配置"""
        config = create_memory_efficient_config()
        
        assert isinstance(config, CANPConfig)
        assert config.memory_pool.max_memory_mb == 1024
        assert config.storage.compression == "gzip"
        assert config.storage.batch_size == 5000


class TestConfigValidation:
    """配置验证测试"""
    
    def test_valid_config(self):
        """测试有效配置"""
        config = create_default_config()
        assert validate_config(config) is True
    
    def test_invalid_memory_config(self):
        """测试无效内存配置"""
        config = create_default_config()
        config.memory_pool.max_memory_mb = 256  # 小于512MB
        
        assert validate_config(config) is False
    
    def test_invalid_cpu_workers(self):
        """测试无效CPU工作进程数"""
        config = create_default_config()
        config.executor.cpu_workers = 1000  # 超过限制
        
        assert validate_config(config) is False


class TestConfigFileOperations:
    """配置文件操作测试"""
    
    def test_save_and_load_config(self):
        """测试保存和加载配置"""
        config = create_default_config()
        config.input_dir = "/test/input"
        config.dbc_file = "/test/sample.dbc"
        
        with tempfile.NamedTemporaryFile(mode='w', suffix='.json', delete=False) as f:
            config_file = f.name
        
        try:
            # 保存配置
            save_config_to_file(config, config_file)
            
            # 加载配置
            loaded_config = load_config_from_file(config_file)
            
            # 验证配置
            assert loaded_config.input_dir == "/test/input"
            assert loaded_config.dbc_file == "/test/sample.dbc"
            assert loaded_config.memory_pool.max_memory_mb == 2048
            
        finally:
            # 清理临时文件
            os.unlink(config_file)


class TestEnvironmentConfig:
    """环境变量配置测试"""
    
    @patch.dict(os.environ, {
        'CANP_MAX_MEMORY_MB': '4096',
        'CANP_CPU_WORKERS': '8',
        'CANP_IO_WORKERS': '32',
        'CANP_OUTPUT_DIR': '/data/output',
        'CANP_COMPRESSION': 'zstd',
        'CANP_INPUT_DIR': '/data/input',
        'CANP_DBC_FILE': '/data/sample.dbc',
        'CANP_LOG_LEVEL': 'DEBUG'
    })
    def test_load_config_from_env(self):
        """测试从环境变量加载配置"""
        config = load_config_from_env()
        
        assert config.memory_pool.max_memory_mb == 4096
        assert config.executor.cpu_workers == 8
        assert config.executor.io_workers == 32
        assert config.storage.output_dir == "/data/output"
        assert config.storage.compression == "zstd"
        assert config.input_dir == "/data/input"
        assert config.dbc_file == "/data/sample.dbc"
        assert config.log_level == "DEBUG"
    
    def test_load_config_from_env_empty(self):
        """测试从空环境变量加载配置"""
        with patch.dict(os.environ, {}, clear=True):
            config = load_config_from_env()
            
            # 应该使用默认值
            assert config.memory_pool.max_memory_mb == 2048
            assert config.executor.io_workers == 16
            assert config.storage.output_dir == "output" 