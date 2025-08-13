"""
CANP Python - 高性能CAN总线数据处理流水线

一个基于Python的高性能CAN总线数据处理流水线系统，
专为大规模汽车数据分析和处理设计。
"""

__version__ = "0.1.0"
__author__ = "CANP Team"
__email__ = "team@canp-project.org"

from .config import (
    CANPConfig,
    MemoryPoolConfig,
    ExecutorConfig,
    DBCParserConfig,
    StorageConfig,
)
from .memory import HighPerformanceMemoryPool
from .executor import AsyncHighPerformanceExecutor
from .dbc import DBCParser
from .storage import ColumnarStorage
from .pipeline import AsyncProcessingPipeline

__all__ = [
    # 配置类
    "CANPConfig",
    "MemoryPoolConfig", 
    "ExecutorConfig",
    "DBCParserConfig",
    "StorageConfig",
    
    # 核心组件
    "HighPerformanceMemoryPool",
    "AsyncHighPerformanceExecutor", 
    "DBCParser",
    "ColumnarStorage",
    "AsyncProcessingPipeline",
] 