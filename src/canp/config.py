"""
配置管理模块

使用Pydantic进行配置验证和管理，提供类型安全的配置系统。
"""

from typing import List, Optional, Dict, Any
from pathlib import Path
from dataclasses import dataclass
from pydantic import BaseModel, Field, validator
import multiprocessing as mp


class MemoryPoolConfig(BaseModel):
    """内存池配置"""
    
    max_memory_mb: int = Field(
        default=2048,
        description="最大内存使用量(MB)",
        gt=0,
        le=32768  # 32GB限制
    )
    
    buffer_sizes: List[int] = Field(
        default=[1024, 2048, 4096, 8192, 16384],
        description="NumPy数组池的大小列表"
    )
    
    mmap_cache_size: int = Field(
        default=1000,
        description="内存映射缓存大小",
        gt=0,
        le=10000
    )
    
    enable_compression: bool = Field(
        default=True,
        description="是否启用压缩"
    )
    
    @validator('buffer_sizes')
    def validate_buffer_sizes(cls, v):
        """验证缓冲区大小列表"""
        if not v:
            raise ValueError("缓冲区大小列表不能为空")
        
        # 检查是否按升序排列
        if v != sorted(v):
            raise ValueError("缓冲区大小必须按升序排列")
        
        # 检查是否有重复
        if len(v) != len(set(v)):
            raise ValueError("缓冲区大小不能重复")
        
        return v


class ExecutorConfig(BaseModel):
    """执行器配置"""
    
    cpu_workers: Optional[int] = Field(
        default=None,
        description="CPU工作进程数，None表示自动检测"
    )
    
    io_workers: int = Field(
        default=16,
        description="IO工作线程数",
        gt=0,
        le=100
    )
    
    max_concurrent: int = Field(
        default=100,
        description="最大并发任务数",
        gt=0,
        le=1000
    )
    
    task_timeout: float = Field(
        default=300.0,
        description="任务超时时间(秒)",
        gt=0
    )
    
    enable_work_stealing: bool = Field(
        default=False,
        description="是否启用工作窃取(实验性)"
    )
    
    @validator('cpu_workers', pre=True, always=True)
    def set_cpu_workers(cls, v):
        """设置CPU工作进程数"""
        if v is None:
            return mp.cpu_count()
        return v


class DBCParserConfig(BaseModel):
    """DBC解析器配置"""
    
    cache_enabled: bool = Field(
        default=True,
        description="是否启用缓存"
    )
    
    cache_dir: str = Field(
        default=".cache",
        description="缓存目录"
    )
    
    max_workers: int = Field(
        default=4,
        description="最大工作线程数",
        gt=0,
        le=16
    )
    
    auto_reload: bool = Field(
        default=True,
        description="是否自动重新加载DBC文件"
    )
    
    cache_expire_seconds: int = Field(
        default=3600,
        description="缓存过期时间(秒)",
        gt=0
    )


class StorageConfig(BaseModel):
    """存储配置"""
    
    output_dir: str = Field(
        default="output",
        description="输出目录"
    )
    
    compression: str = Field(
        default="snappy",
        description="压缩算法",
        regex="^(snappy|gzip|brotli|zstd|lz4)$"
    )
    
    partition_strategy: str = Field(
        default="time",
        description="分区策略",
        regex="^(time|file|id|custom)$"
    )
    
    batch_size: int = Field(
        default=10000,
        description="批处理大小",
        gt=0,
        le=100000
    )
    
    enable_metadata: bool = Field(
        default=True,
        description="是否启用元数据"
    )


class CANPConfig(BaseModel):
    """CANP主配置"""
    
    # 子配置
    memory_pool: MemoryPoolConfig = Field(
        default_factory=MemoryPoolConfig,
        description="内存池配置"
    )
    
    executor: ExecutorConfig = Field(
        default_factory=ExecutorConfig,
        description="执行器配置"
    )
    
    dbc_parser: DBCParserConfig = Field(
        default_factory=DBCParserConfig,
        description="DBC解析器配置"
    )
    
    storage: StorageConfig = Field(
        default_factory=StorageConfig,
        description="存储配置"
    )
    
    # 全局配置
    input_dir: Optional[str] = Field(
        default=None,
        description="输入目录"
    )
    
    dbc_file: Optional[str] = Field(
        default=None,
        description="DBC文件路径"
    )
    
    log_level: str = Field(
        default="INFO",
        description="日志级别",
        regex="^(DEBUG|INFO|WARNING|ERROR|CRITICAL)$"
    )
    
    enable_profiling: bool = Field(
        default=False,
        description="是否启用性能分析"
    )
    
    @validator('input_dir')
    def validate_input_dir(cls, v):
        """验证输入目录"""
        if v is not None:
            path = Path(v)
            if not path.exists():
                raise ValueError(f"输入目录不存在: {v}")
            if not path.is_dir():
                raise ValueError(f"输入路径不是目录: {v}")
        return v
    
    @validator('dbc_file')
    def validate_dbc_file(cls, v):
        """验证DBC文件"""
        if v is not None:
            path = Path(v)
            if not path.exists():
                raise ValueError(f"DBC文件不存在: {v}")
            if not path.is_file():
                raise ValueError(f"DBC路径不是文件: {v}")
            if path.suffix.lower() != '.dbc':
                raise ValueError(f"文件扩展名必须是.dbc: {v}")
        return v
    
    class Config:
        """Pydantic配置"""
        validate_assignment = True
        extra = "forbid"
        use_enum_values = True


# 便捷的配置创建函数
def create_default_config() -> CANPConfig:
    """创建默认配置"""
    return CANPConfig()


def create_high_performance_config() -> CANPConfig:
    """创建高性能配置"""
    return CANPConfig(
        memory_pool=MemoryPoolConfig(
            max_memory_mb=4096,
            buffer_sizes=[1024, 2048, 4096, 8192, 16384, 32768],
            mmap_cache_size=2000,
            enable_compression=True
        ),
        executor=ExecutorConfig(
            cpu_workers=mp.cpu_count(),
            io_workers=32,
            max_concurrent=200,
            task_timeout=600.0
        ),
        dbc_parser=DBCParserConfig(
            cache_enabled=True,
            max_workers=8,
            auto_reload=True
        ),
        storage=StorageConfig(
            compression="zstd",
            batch_size=50000,
            enable_metadata=True
        )
    )


def create_memory_efficient_config() -> CANPConfig:
    """创建内存高效配置"""
    return CANPConfig(
        memory_pool=MemoryPoolConfig(
            max_memory_mb=1024,
            buffer_sizes=[512, 1024, 2048, 4096],
            mmap_cache_size=500,
            enable_compression=True
        ),
        executor=ExecutorConfig(
            cpu_workers=max(1, mp.cpu_count() // 2),
            io_workers=8,
            max_concurrent=50,
            task_timeout=300.0
        ),
        dbc_parser=DBCParserConfig(
            cache_enabled=True,
            max_workers=2,
            auto_reload=False
        ),
        storage=StorageConfig(
            compression="gzip",
            batch_size=5000,
            enable_metadata=False
        )
    )


# 配置验证函数
def validate_config(config: CANPConfig) -> bool:
    """验证配置的有效性"""
    try:
        # Pydantic会自动验证
        config_dict = config.dict()
        
        # 额外的业务逻辑验证
        if config.memory_pool.max_memory_mb < 512:
            raise ValueError("内存池大小不能小于512MB")
        
        if config.executor.cpu_workers > mp.cpu_count() * 2:
            raise ValueError("CPU工作进程数不能超过CPU核心数的2倍")
        
        return True
        
    except Exception as e:
        print(f"配置验证失败: {e}")
        return False


# 配置加载和保存
def load_config_from_file(file_path: str) -> CANPConfig:
    """从文件加载配置"""
    import json
    
    with open(file_path, 'r', encoding='utf-8') as f:
        config_dict = json.load(f)
    
    return CANPConfig(**config_dict)


def save_config_to_file(config: CANPConfig, file_path: str) -> None:
    """保存配置到文件"""
    import json
    
    config_dict = config.dict()
    
    with open(file_path, 'w', encoding='utf-8') as f:
        json.dump(config_dict, f, indent=2, ensure_ascii=False)


# 环境变量配置
def load_config_from_env() -> CANPConfig:
    """从环境变量加载配置"""
    import os
    
    config_dict = {}
    
    # 内存池配置
    if max_memory := os.getenv('CANP_MAX_MEMORY_MB'):
        config_dict.setdefault('memory_pool', {})['max_memory_mb'] = int(max_memory)
    
    # 执行器配置
    if cpu_workers := os.getenv('CANP_CPU_WORKERS'):
        config_dict.setdefault('executor', {})['cpu_workers'] = int(cpu_workers)
    
    if io_workers := os.getenv('CANP_IO_WORKERS'):
        config_dict.setdefault('executor', {})['io_workers'] = int(io_workers)
    
    # 存储配置
    if output_dir := os.getenv('CANP_OUTPUT_DIR'):
        config_dict.setdefault('storage', {})['output_dir'] = output_dir
    
    if compression := os.getenv('CANP_COMPRESSION'):
        config_dict.setdefault('storage', {})['compression'] = compression
    
    # 全局配置
    if input_dir := os.getenv('CANP_INPUT_DIR'):
        config_dict['input_dir'] = input_dir
    
    if dbc_file := os.getenv('CANP_DBC_FILE'):
        config_dict['dbc_file'] = dbc_file
    
    if log_level := os.getenv('CANP_LOG_LEVEL'):
        config_dict['log_level'] = log_level
    
    return CANPConfig(**config_dict) 