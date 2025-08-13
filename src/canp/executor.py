"""
异步高性能执行器模块

结合asyncio和多进程的混合并发模型，提供智能的任务调度和背压控制。
"""

import asyncio
import multiprocessing as mp
from concurrent.futures import ProcessPoolExecutor, ThreadPoolExecutor
from typing import Callable, Any, Dict, List, Optional, Union
from dataclasses import dataclass, field
from enum import Enum
import time
import uuid
import structlog
from functools import wraps

from .config import ExecutorConfig

logger = structlog.get_logger(__name__)


class TaskType(Enum):
    """任务类型"""
    IO_INTENSIVE = "io_intensive"
    CPU_INTENSIVE = "cpu_intensive"
    MIXED = "mixed"
    HIGH_PRIORITY = "high_priority"


class TaskStatus(Enum):
    """任务状态"""
    PENDING = "pending"
    RUNNING = "running"
    COMPLETED = "completed"
    FAILED = "failed"
    CANCELLED = "cancelled"


@dataclass
class TaskMetadata:
    """任务元数据"""
    
    task_id: str
    task_type: TaskType
    priority: int
    created_at: float
    started_at: Optional[float] = None
    completed_at: Optional[float] = None
    timeout: float = 300.0
    status: TaskStatus = TaskStatus.PENDING
    error: Optional[str] = None
    result: Optional[Any] = None


@dataclass
class ExecutorStats:
    """执行器统计信息"""
    
    total_tasks: int = 0
    completed_tasks: int = 0
    failed_tasks: int = 0
    cancelled_tasks: int = 0
    io_tasks: int = 0
    cpu_tasks: int = 0
    priority_tasks: int = 0
    average_task_time: float = 0.0
    total_execution_time: float = 0.0
    active_tasks: int = 0
    queue_size: int = 0


class AsyncHighPerformanceExecutor:
    """异步高性能执行器"""
    
    def __init__(self, config: ExecutorConfig):
        self.config = config
        
        # 进程池 - CPU密集型任务
        self._process_pool = ProcessPoolExecutor(
            max_workers=config.cpu_workers
        )
        
        # 线程池 - IO密集型任务
        self._thread_pool = ThreadPoolExecutor(
            max_workers=config.io_workers
        )
        
        # 任务队列
        self._io_queue = asyncio.Queue(maxsize=config.max_concurrent)
        self._cpu_queue = asyncio.Queue(maxsize=config.max_concurrent)
        self._priority_queue = asyncio.Queue(maxsize=100)
        
        # 背压控制
        self._semaphore = asyncio.Semaphore(config.max_concurrent)
        
        # 任务跟踪
        self._tasks: Dict[str, TaskMetadata] = {}
        self._task_results: Dict[str, Any] = {}
        
        # 统计信息
        self._stats = ExecutorStats()
        self._start_time = time.time()
        
        # 工作协程
        self._workers: List[asyncio.Task] = []
        self._running = False
        
        logger.info(
            "执行器初始化完成",
            cpu_workers=config.cpu_workers,
            io_workers=config.io_workers,
            max_concurrent=config.max_concurrent
        )
    
    async def start(self) -> None:
        """启动执行器"""
        if self._running:
            return
        
        self._running = True
        
        # 启动工作协程
        self._workers = [
            asyncio.create_task(self._io_worker()),
            asyncio.create_task(self._cpu_worker()),
            asyncio.create_task(self._priority_worker()),
            asyncio.create_task(self._stats_worker())
        ]
        
        logger.info("执行器已启动")
    
    async def stop(self) -> None:
        """停止执行器"""
        if not self._running:
            return
        
        self._running = False
        
        # 取消所有工作协程
        for worker in self._workers:
            worker.cancel()
        
        # 等待所有工作协程完成
        await asyncio.gather(*self._workers, return_exceptions=True)
        
        # 关闭池
        self._process_pool.shutdown(wait=True)
        self._thread_pool.shutdown(wait=True)
        
        logger.info("执行器已停止")
    
    async def submit_io_task(
        self, 
        func: Callable, 
        *args, 
        priority: int = 0,
        timeout: float = 300.0,
        **kwargs
    ) -> str:
        """提交IO密集型任务"""
        task_id = f"io_{uuid.uuid4().hex[:8]}"
        
        metadata = TaskMetadata(
            task_id=task_id,
            task_type=TaskType.IO_INTENSIVE,
            priority=priority,
            created_at=time.time(),
            timeout=timeout
        )
        
        self._tasks[task_id] = metadata
        self._stats.total_tasks += 1
        self._stats.io_tasks += 1
        
        # 包装函数以支持参数传递
        wrapped_func = self._wrap_function(func, *args, **kwargs)
        
        await self._io_queue.put((metadata, wrapped_func))
        
        logger.debug("提交IO任务", task_id=task_id, priority=priority)
        return task_id
    
    async def submit_cpu_task(
        self, 
        func: Callable, 
        *args, 
        priority: int = 0,
        timeout: float = 300.0,
        **kwargs
    ) -> str:
        """提交CPU密集型任务"""
        task_id = f"cpu_{uuid.uuid4().hex[:8]}"
        
        metadata = TaskMetadata(
            task_id=task_id,
            task_type=TaskType.CPU_INTENSIVE,
            priority=priority,
            created_at=time.time(),
            timeout=timeout
        )
        
        self._tasks[task_id] = metadata
        self._stats.total_tasks += 1
        self._stats.cpu_tasks += 1
        
        # 包装函数以支持参数传递
        wrapped_func = self._wrap_function(func, *args, **kwargs)
        
        await self._cpu_queue.put((metadata, wrapped_func))
        
        logger.debug("提交CPU任务", task_id=task_id, priority=priority)
        return task_id
    
    async def submit_priority_task(
        self, 
        func: Callable, 
        *args, 
        timeout: float = 60.0,
        **kwargs
    ) -> str:
        """提交高优先级任务"""
        task_id = f"priority_{uuid.uuid4().hex[:8]}"
        
        metadata = TaskMetadata(
            task_id=task_id,
            task_type=TaskType.HIGH_PRIORITY,
            priority=1000,  # 最高优先级
            created_at=time.time(),
            timeout=timeout
        )
        
        self._tasks[task_id] = metadata
        self._stats.total_tasks += 1
        self._stats.priority_tasks += 1
        
        # 包装函数以支持参数传递
        wrapped_func = self._wrap_function(func, *args, **kwargs)
        
        await self._priority_queue.put((metadata, wrapped_func))
        
        logger.debug("提交优先级任务", task_id=task_id)
        return task_id
    
    def _wrap_function(self, func: Callable, *args, **kwargs) -> Callable:
        """包装函数以支持参数传递"""
        @wraps(func)
        def wrapped():
            return func(*args, **kwargs)
        return wrapped
    
    async def get_task_result(self, task_id: str, timeout: Optional[float] = None) -> Any:
        """获取任务结果"""
        start_time = time.time()
        
        while True:
            if task_id in self._task_results:
                return self._task_results[task_id]
            
            if task_id in self._tasks:
                task = self._tasks[task_id]
                if task.status == TaskStatus.FAILED:
                    raise Exception(f"任务失败: {task.error}")
                elif task.status == TaskStatus.CANCELLED:
                    raise Exception("任务已取消")
            
            if timeout and (time.time() - start_time) > timeout:
                raise asyncio.TimeoutError(f"等待任务结果超时: {task_id}")
            
            await asyncio.sleep(0.01)  # 短暂等待
    
    async def cancel_task(self, task_id: str) -> bool:
        """取消任务"""
        if task_id in self._tasks:
            task = self._tasks[task_id]
            if task.status == TaskStatus.PENDING:
                task.status = TaskStatus.CANCELLED
                self._stats.cancelled_tasks += 1
                logger.debug("任务已取消", task_id=task_id)
                return True
        
        return False
    
    async def _io_worker(self) -> None:
        """IO工作协程"""
        while self._running:
            try:
                async with self._semaphore:
                    metadata, func = await self._io_queue.get()
                    
                    if metadata.status == TaskStatus.CANCELLED:
                        self._io_queue.task_done()
                        continue
                    
                    await self._execute_task(metadata, func, is_io=True)
                    self._io_queue.task_done()
                    
            except asyncio.CancelledError:
                break
            except Exception as e:
                logger.error("IO工作协程错误", error=str(e))
    
    async def _cpu_worker(self) -> None:
        """CPU工作协程"""
        while self._running:
            try:
                async with self._semaphore:
                    metadata, func = await self._cpu_queue.get()
                    
                    if metadata.status == TaskStatus.CANCELLED:
                        self._cpu_queue.task_done()
                        continue
                    
                    await self._execute_task(metadata, func, is_io=False)
                    self._cpu_queue.task_done()
                    
            except asyncio.CancelledError:
                break
            except Exception as e:
                logger.error("CPU工作协程错误", error=str(e))
    
    async def _priority_worker(self) -> None:
        """优先级工作协程"""
        while self._running:
            try:
                metadata, func = await self._priority_queue.get()
                
                if metadata.status == TaskStatus.CANCELLED:
                    self._priority_queue.task_done()
                    continue
                
                await self._execute_task(metadata, func, is_io=True)
                self._priority_queue.task_done()
                
            except asyncio.CancelledError:
                break
            except Exception as e:
                logger.error("优先级工作协程错误", error=str(e))
    
    async def _execute_task(self, metadata: TaskMetadata, func: Callable, is_io: bool) -> None:
        """执行任务"""
        metadata.status = TaskStatus.RUNNING
        metadata.started_at = time.time()
        self._stats.active_tasks += 1
        
        try:
            # 设置超时
            if metadata.timeout > 0:
                if is_io:
                    result = await asyncio.wait_for(
                        self._run_io_task(func),
                        timeout=metadata.timeout
                    )
                else:
                    result = await asyncio.wait_for(
                        self._run_cpu_task(func),
                        timeout=metadata.timeout
                    )
            else:
                if is_io:
                    result = await self._run_io_task(func)
                else:
                    result = await self._run_cpu_task(func)
            
            # 任务成功完成
            metadata.status = TaskStatus.COMPLETED
            metadata.completed_at = time.time()
            metadata.result = result
            
            self._task_results[metadata.task_id] = result
            self._stats.completed_tasks += 1
            
            execution_time = metadata.completed_at - metadata.started_at
            self._stats.total_execution_time += execution_time
            
            logger.debug(
                "任务完成",
                task_id=metadata.task_id,
                execution_time=execution_time,
                result_type=type(result).__name__
            )
            
        except asyncio.TimeoutError:
            metadata.status = TaskStatus.FAILED
            metadata.error = f"任务超时: {metadata.timeout}秒"
            self._stats.failed_tasks += 1
            logger.warning("任务超时", task_id=metadata.task_id, timeout=metadata.timeout)
            
        except Exception as e:
            metadata.status = TaskStatus.FAILED
            metadata.error = str(e)
            self._stats.failed_tasks += 1
            logger.error("任务执行失败", task_id=metadata.task_id, error=str(e))
            
        finally:
            self._stats.active_tasks -= 1
    
    async def _run_io_task(self, func: Callable) -> Any:
        """运行IO任务"""
        loop = asyncio.get_event_loop()
        
        if asyncio.iscoroutinefunction(func):
            return await func()
        else:
            return await loop.run_in_executor(self._thread_pool, func)
    
    async def _run_cpu_task(self, func: Callable) -> Any:
        """运行CPU任务"""
        loop = asyncio.get_event_loop()
        return await loop.run_in_executor(self._process_pool, func)
    
    async def _stats_worker(self) -> None:
        """统计工作协程"""
        while self._running:
            try:
                await asyncio.sleep(5)  # 每5秒更新一次统计
                
                # 更新队列大小
                self._stats.queue_size = (
                    self._io_queue.qsize() + 
                    self._cpu_queue.qsize() + 
                    self._priority_queue.qsize()
                )
                
                # 更新平均任务时间
                if self._stats.completed_tasks > 0:
                    self._stats.average_task_time = (
                        self._stats.total_execution_time / self._stats.completed_tasks
                    )
                
            except asyncio.CancelledError:
                break
            except Exception as e:
                logger.error("统计工作协程错误", error=str(e))
    
    async def get_stats(self) -> ExecutorStats:
        """获取执行器统计信息"""
        # 更新队列大小
        self._stats.queue_size = (
            self._io_queue.qsize() + 
            self._cpu_queue.qsize() + 
            self._priority_queue.qsize()
        )
        
        return self._stats
    
    async def get_task_info(self, task_id: str) -> Optional[TaskMetadata]:
        """获取任务信息"""
        return self._tasks.get(task_id)
    
    async def list_tasks(self, status: Optional[TaskStatus] = None) -> List[TaskMetadata]:
        """列出任务"""
        tasks = list(self._tasks.values())
        
        if status:
            tasks = [task for task in tasks if task.status == status]
        
        return tasks
    
    def __enter__(self):
        """上下文管理器入口"""
        return self
    
    def __exit__(self, exc_type, exc_val, exc_tb):
        """上下文管理器出口"""
        # 注意：这里不能使用await，所以只是标记
        pass
    
    async def __aenter__(self):
        """异步上下文管理器入口"""
        await self.start()
        return self
    
    async def __aexit__(self, exc_type, exc_val, exc_tb):
        """异步上下文管理器出口"""
        await self.stop()


# 便捷的执行器创建函数
async def create_executor(
    cpu_workers: Optional[int] = None,
    io_workers: int = 16,
    max_concurrent: int = 100
) -> AsyncHighPerformanceExecutor:
    """创建执行器的便捷函数"""
    config = ExecutorConfig(
        cpu_workers=cpu_workers,
        io_workers=io_workers,
        max_concurrent=max_concurrent
    )
    
    return AsyncHighPerformanceExecutor(config)


# 任务装饰器
def io_task(priority: int = 0, timeout: float = 300.0):
    """IO任务装饰器"""
    def decorator(func):
        @wraps(func)
        async def wrapper(executor: AsyncHighPerformanceExecutor, *args, **kwargs):
            task_id = await executor.submit_io_task(
                func, *args, priority=priority, timeout=timeout, **kwargs
            )
            return await executor.get_task_result(task_id)
        return wrapper
    return decorator


def cpu_task(priority: int = 0, timeout: float = 300.0):
    """CPU任务装饰器"""
    def decorator(func):
        @wraps(func)
        async def wrapper(executor: AsyncHighPerformanceExecutor, *args, **kwargs):
            task_id = await executor.submit_cpu_task(
                func, *args, priority=priority, timeout=timeout, **kwargs
            )
            return await executor.get_task_result(task_id)
        return wrapper
    return decorator


def priority_task(timeout: float = 60.0):
    """优先级任务装饰器"""
    def decorator(func):
        @wraps(func)
        async def wrapper(executor: AsyncHighPerformanceExecutor, *args, **kwargs):
            task_id = await executor.submit_priority_task(
                func, *args, timeout=timeout, **kwargs
            )
            return await executor.get_task_result(task_id)
        return wrapper
    return decorator 