"""
高性能内存池模块

基于NumPy和内存映射的高性能内存管理系统，
提供零拷贝文件访问和智能数组复用。
"""

import asyncio
import mmap
import weakref
from typing import Dict, List, Optional, Union, Any
from pathlib import Path
from collections import defaultdict, OrderedDict
import numpy as np
import psutil
import structlog
from dataclasses import dataclass, field

from .config import MemoryPoolConfig

logger = structlog.get_logger(__name__)


@dataclass
class MemoryStats:
    """内存统计信息"""
    
    total_memory_usage_mb: float = 0.0
    array_pool_sizes: Dict[int, int] = field(default_factory=dict)
    mmap_cache_size: int = 0
    object_pool_size: int = 0
    cache_hit_rate: float = 0.0
    cache_miss_rate: float = 0.0
    system_memory_usage_mb: float = 0.0
    system_memory_percent: float = 0.0


class LRUCache:
    """简单的LRU缓存实现"""
    
    def __init__(self, capacity: int):
        self.capacity = capacity
        self.cache: OrderedDict[str, Any] = OrderedDict()
        self.hits = 0
        self.misses = 0
    
    def get(self, key: str) -> Optional[Any]:
        """获取缓存项"""
        if key in self.cache:
            # 移动到末尾（最近使用）
            self.cache.move_to_end(key)
            self.hits += 1
            return self.cache[key]
        
        self.misses += 1
        return None
    
    def put(self, key: str, value: Any) -> None:
        """放入缓存项"""
        if key in self.cache:
            # 如果已存在，移动到末尾
            self.cache.move_to_end(key)
        else:
            # 如果缓存满了，删除最旧的项
            if len(self.cache) >= self.capacity:
                self.cache.popitem(last=False)
        
        self.cache[key] = value
    
    def remove(self, key: str) -> bool:
        """移除缓存项"""
        if key in self.cache:
            del self.cache[key]
            return True
        return False
    
    def clear(self) -> None:
        """清空缓存"""
        self.cache.clear()
        self.hits = 0
        self.misses = 0
    
    @property
    def hit_rate(self) -> float:
        """缓存命中率"""
        total = self.hits + self.misses
        return self.hits / total if total > 0 else 0.0
    
    @property
    def miss_rate(self) -> float:
        """缓存未命中率"""
        return 1.0 - self.hit_rate


class HighPerformanceMemoryPool:
    """高性能内存池"""
    
    def __init__(self, config: MemoryPoolConfig):
        self.config = config
        
        # NumPy数组池 - 按大小分层
        self._array_pools: Dict[int, List[np.ndarray]] = defaultdict(list)
        
        # 内存映射缓存 - LRU策略
        self._mmap_cache = LRUCache(config.mmap_cache_size)
        
        # 内存使用统计
        self._current_memory_usage = 0
        self._max_memory_bytes = config.max_memory_mb * 1024 * 1024
        
        # 锁保护
        self._lock = asyncio.Lock()
        
        # 对象池 - 使用weakref避免内存泄漏
        self._object_pool = weakref.WeakSet()
        
        # 统计信息
        self._stats = {
            'total_allocations': 0,
            'total_deallocations': 0,
            'cache_hits': 0,
            'cache_misses': 0,
            'memory_cleanups': 0
        }
        
        logger.info(
            "内存池初始化完成",
            max_memory_mb=config.max_memory_mb,
            buffer_sizes=config.buffer_sizes,
            mmap_cache_size=config.mmap_cache_size
        )
    
    async def get_array(self, size: int, dtype: np.dtype = np.uint8) -> np.ndarray:
        """获取NumPy数组 - 优先从池中获取"""
        async with self._lock:
            # 1. 尝试从池中获取
            if size in self._array_pools and self._array_pools[size]:
                array = self._array_pools[size].pop()
                self._stats['cache_hits'] += 1
                logger.debug("从池中获取数组", size=size, dtype=dtype)
                return array
            
            self._stats['cache_misses'] += 1
            
            # 2. 检查内存限制
            if self._current_memory_usage + size > self._max_memory_bytes:
                await self._cleanup_old_arrays()
            
            # 3. 创建新的数组
            array = np.zeros(size, dtype=dtype)
            self._current_memory_usage += array.nbytes
            self._stats['total_allocations'] += 1
            
            logger.debug("创建新数组", size=size, dtype=dtype, nbytes=array.nbytes)
            return array
    
    async def return_array(self, array: np.ndarray) -> None:
        """归还NumPy数组到池中"""
        async with self._lock:
            size = array.size
            max_pool_size = 10  # 限制池大小
            
            if len(self._array_pools[size]) < max_pool_size:
                # 清零数组
                array.fill(0)
                self._array_pools[size].append(array)
                self._stats['total_deallocations'] += 1
                logger.debug("归还数组到池中", size=size)
            else:
                # 池已满，直接释放
                self._current_memory_usage -= array.nbytes
                logger.debug("池已满，直接释放数组", size=size)
    
    async def map_file(self, file_path: str) -> np.ndarray:
        """内存映射文件 - 零拷贝访问"""
        # 检查缓存
        if cached_array := self._mmap_cache.get(file_path):
            logger.debug("从缓存获取内存映射", file_path=file_path)
            return cached_array
        
        # 创建新的内存映射
        try:
            with open(file_path, 'rb') as f:
                mm = mmap.mmap(f.fileno(), 0, access=mmap.ACCESS_READ)
                # 转换为NumPy数组视图 - 零拷贝
                array = np.frombuffer(mm, dtype=np.uint8)
                
                # 缓存管理
                self._mmap_cache.put(file_path, array)
                
                logger.debug("创建内存映射", file_path=file_path, size=len(array))
                return array
                
        except Exception as e:
            logger.error("内存映射失败", file_path=file_path, error=str(e))
            raise
    
    async def get_array_batch(self, sizes: List[int], dtype: np.dtype = np.uint8) -> List[np.ndarray]:
        """批量获取NumPy数组"""
        arrays = []
        
        for size in sizes:
            array = await self.get_array(size, dtype)
            arrays.append(array)
        
        return arrays
    
    async def return_array_batch(self, arrays: List[np.ndarray]) -> None:
        """批量归还NumPy数组"""
        for array in arrays:
            await self.return_array(array)
    
    async def _cleanup_old_arrays(self) -> None:
        """清理旧数组以释放内存"""
        target_usage = self._max_memory_bytes * 0.8  # 目标使用80%
        
        while self._current_memory_usage > target_usage:
            # 找到最大的池
            largest_size = max(self._array_pools.keys(), default=None)
            if largest_size is None or not self._array_pools[largest_size]:
                break
            
            # 移除一个数组
            array = self._array_pools[largest_size].pop()
            self._current_memory_usage -= array.nbytes
            
            self._stats['memory_cleanups'] += 1
        
        logger.info(
            "内存清理完成",
            current_usage_mb=self._current_memory_usage / (1024 * 1024),
            target_usage_mb=target_usage / (1024 * 1024)
        )
    
    async def get_stats(self) -> MemoryStats:
        """获取内存池统计信息"""
        async with self._lock:
            # 系统内存信息
            system_memory = psutil.virtual_memory()
            
            return MemoryStats(
                total_memory_usage_mb=self._current_memory_usage / (1024 * 1024),
                array_pool_sizes={
                    size: len(arrays) for size, arrays in self._array_pools.items()
                },
                mmap_cache_size=len(self._mmap_cache.cache),
                object_pool_size=len(self._object_pool),
                cache_hit_rate=self._mmap_cache.hit_rate,
                cache_miss_rate=self._mmap_cache.miss_rate,
                system_memory_usage_mb=system_memory.used / (1024 * 1024),
                system_memory_percent=system_memory.percent
            )
    
    async def clear_cache(self) -> None:
        """清空缓存"""
        async with self._lock:
            self._mmap_cache.clear()
            self._array_pools.clear()
            self._current_memory_usage = 0
            
            logger.info("缓存已清空")
    
    async def cleanup(self) -> None:
        """清理资源"""
        async with self._lock:
            # 清空所有池
            self._array_pools.clear()
            self._mmap_cache.clear()
            self._object_pool.clear()
            self._current_memory_usage = 0
            
            logger.info("内存池清理完成")
    
    def __enter__(self):
        """上下文管理器入口"""
        return self
    
    def __exit__(self, exc_type, exc_val, exc_tb):
        """上下文管理器出口"""
        # 注意：这里不能使用await，所以只是标记
        pass
    
    async def __aenter__(self):
        """异步上下文管理器入口"""
        return self
    
    async def __aexit__(self, exc_type, exc_val, exc_tb):
        """异步上下文管理器出口"""
        await self.cleanup()


# 便捷的内存管理函数
async def create_memory_pool(
    max_memory_mb: int = 2048,
    buffer_sizes: Optional[List[int]] = None,
    mmap_cache_size: int = 1000
) -> HighPerformanceMemoryPool:
    """创建内存池的便捷函数"""
    config = MemoryPoolConfig(
        max_memory_mb=max_memory_mb,
        buffer_sizes=buffer_sizes or [1024, 2048, 4096, 8192, 16384],
        mmap_cache_size=mmap_cache_size
    )
    
    return HighPerformanceMemoryPool(config)


# 内存监控装饰器
def monitor_memory(func):
    """内存监控装饰器"""
    async def wrapper(*args, **kwargs):
        import psutil
        import time
        
        process = psutil.Process()
        start_memory = process.memory_info().rss
        start_time = time.time()
        
        try:
            result = await func(*args, **kwargs)
            return result
        finally:
            end_memory = process.memory_info().rss
            end_time = time.time()
            
            memory_diff = (end_memory - start_memory) / (1024 * 1024)  # MB
            time_diff = end_time - start_time
            
            logger.info(
                "函数内存使用",
                function=func.__name__,
                memory_diff_mb=memory_diff,
                execution_time_seconds=time_diff
            )
    
    return wrapper


# 内存泄漏检测
class MemoryLeakDetector:
    """内存泄漏检测器"""
    
    def __init__(self):
        self.snapshots: List[Dict[str, Any]] = []
    
    def take_snapshot(self, name: str) -> None:
        """拍摄内存快照"""
        import psutil
        
        process = psutil.Process()
        memory_info = process.memory_info()
        
        snapshot = {
            'name': name,
            'timestamp': time.time(),
            'rss_mb': memory_info.rss / (1024 * 1024),
            'vms_mb': memory_info.vms / (1024 * 1024),
            'num_fds': process.num_fds() if hasattr(process, 'num_fds') else 0
        }
        
        self.snapshots.append(snapshot)
        logger.info("内存快照", **snapshot)
    
    def analyze_leaks(self) -> Dict[str, Any]:
        """分析内存泄漏"""
        if len(self.snapshots) < 2:
            return {'error': '需要至少两个快照'}
        
        first = self.snapshots[0]
        last = self.snapshots[-1]
        
        rss_diff = last['rss_mb'] - first['rss_mb']
        vms_diff = last['vms_mb'] - first['vms_mb']
        
        return {
            'rss_increase_mb': rss_diff,
            'vms_increase_mb': vms_diff,
            'potential_leak': rss_diff > 100,  # 100MB阈值
            'snapshots_count': len(self.snapshots)
        } 