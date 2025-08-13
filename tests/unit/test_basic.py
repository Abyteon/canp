"""
基本单元测试

验证项目的基本功能可以正常运行。
"""

import pytest
import asyncio
from pathlib import Path

from canp import (
    CANPConfig,
    HighPerformanceMemoryPool,
    AsyncHighPerformanceExecutor,
    DBCParser,
    ColumnarStorage,
    create_default_config
)


class TestBasicFunctionality:
    """基本功能测试"""
    
    @pytest.mark.asyncio
    async def test_memory_pool_basic(self):
        """测试内存池基本功能"""
        from canp.config import MemoryPoolConfig
        
        config = MemoryPoolConfig(max_memory_mb=100)
        memory_pool = HighPerformanceMemoryPool(config)
        
        # 获取数组
        array = await memory_pool.get_array(1024)
        assert array.size == 1024
        assert array.dtype.name == 'uint8'
        
        # 使用数组
        array.fill(42)
        assert array[0] == 42
        
        # 归还数组
        await memory_pool.return_array(array)
        
        # 获取统计
        stats = await memory_pool.get_stats()
        assert stats.total_memory_usage_mb > 0
    
    @pytest.mark.asyncio
    async def test_executor_basic(self):
        """测试执行器基本功能"""
        from canp.config import ExecutorConfig
        
        config = ExecutorConfig(cpu_workers=2, io_workers=2, max_concurrent=5)
        executor = AsyncHighPerformanceExecutor(config)
        
        await executor.start()
        
        # 提交简单任务
        def simple_task():
            return "hello world"
        
        task_id = await executor.submit_cpu_task(simple_task)
        result = await executor.get_task_result(task_id)
        
        assert result == "hello world"
        
        await executor.stop()
    
    @pytest.mark.asyncio
    async def test_dbc_parser_basic(self):
        """测试DBC解析器基本功能"""
        import tempfile
        import os
        
        # 创建临时DBC文件
        dbc_content = """
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
    BA_SGTYPE_
    SIG_TYPE_REF_
    VAL_TABLE_
    SIG_GROUP_
    SIG_VALTYPE_
    SIGTYPE_VALTYPE_
    BO_TX_BU_
    BA_DEF_REL_
    BA_REL_
    BA_DEF_DEF_REL_
    BU_SG_REL_
    BU_EV_REL_
    BU_BO_REL_

BS_:

BU_: Vector__XXX

BO_ 100 EngineData: 8 Vector__XXX
 SG_ EngineSpeed : 0|16@1+ (0.125,0) [0|8031.875] "rpm" Vector__XXX
"""
        
        with tempfile.NamedTemporaryFile(mode='w', suffix='.dbc', delete=False) as f:
            f.write(dbc_content)
            dbc_file = f.name
        
        try:
            parser = DBCParser()
            dbc_data = await parser.parse_dbc_file(dbc_file)
            
            assert 'messages' in dbc_data
            assert len(dbc_data['messages']) > 0
            
            # 检查消息
            message = dbc_data['messages'][100]
            assert message['name'] == 'EngineData'
            assert len(message['signals']) > 0
            
            # 检查信号
            signal = message['signals']['EngineSpeed']
            assert signal['start_bit'] == 0
            assert signal['length'] == 16
            
        finally:
            os.unlink(dbc_file)
    
    @pytest.mark.asyncio
    async def test_storage_basic(self):
        """测试存储基本功能"""
        import tempfile
        
        with tempfile.TemporaryDirectory() as temp_dir:
            storage = ColumnarStorage(output_dir=temp_dir)
            
            # 测试数据
            can_data = [
                {
                    'timestamp': 1640995200.0,
                    'can_id': 100,
                    'signal_name': 'EngineSpeed',
                    'value': 1500.0,
                    'unit': 'rpm'
                }
            ]
            
            # 写入数据
            await storage.write_can_data(can_data, partition_key="test")
            
            # 检查文件是否创建
            partition_dir = Path(temp_dir) / "test"
            assert partition_dir.exists()
            
            # 查询数据
            result = await storage.query_can_data("SELECT * FROM data")
            assert len(result) > 0
    
    def test_config_basic(self):
        """测试配置基本功能"""
        config = create_default_config()
        
        assert config.memory_pool.max_memory_mb == 2048
        assert config.executor.io_workers == 16
        assert config.storage.output_dir == "output"
    
    @pytest.mark.asyncio
    async def test_integration_basic(self):
        """测试基本集成功能"""
        config = create_default_config()
        
        # 创建内存池
        memory_pool = HighPerformanceMemoryPool(config.memory_pool)
        
        # 创建执行器
        executor = AsyncHighPerformanceExecutor(config.executor)
        await executor.start()
        
        # 创建存储
        storage = ColumnarStorage(
            output_dir="test_output",
            compression="snappy",
            partition_strategy="time"
        )
        
        # 基本操作
        array = await memory_pool.get_array(1024)
        array.fill(123)
        
        def test_task():
            return array.sum()
        
        task_id = await executor.submit_cpu_task(test_task)
        result = await executor.get_task_result(task_id)
        
        assert result == 123 * 1024
        
        # 清理
        await memory_pool.return_array(array)
        await executor.stop()
        
        # 清理测试输出目录
        import shutil
        if Path("test_output").exists():
            shutil.rmtree("test_output")


if __name__ == "__main__":
    pytest.main([__file__, "-v"]) 