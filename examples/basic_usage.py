"""
CANP Python 基本使用示例

展示如何使用CANP Python的核心功能进行CAN数据处理。
"""

import asyncio
import tempfile
import os
from pathlib import Path

from canp import (
    CANPConfig,
    AsyncProcessingPipeline,
    HighPerformanceMemoryPool,
    AsyncHighPerformanceExecutor,
    DBCParser,
    ColumnarStorage,
    create_default_config
)


async def basic_memory_pool_example():
    """基本内存池使用示例"""
    print("\n1. 内存池使用示例")
    
    # 创建内存池
    memory_pool = HighPerformanceMemoryPool(
        max_memory_mb=512,
        buffer_sizes=[1024, 2048, 4096],
        mmap_cache_size=100
    )
    
    # 获取NumPy数组
    array1 = await memory_pool.get_array(1024)
    array2 = await memory_pool.get_array(2048)
    
    # 使用数组
    array1.fill(42)
    array2.fill(100)
    
    print(f"数组1: {array1[:5]}...")
    print(f"数组2: {array2[:5]}...")
    
    # 归还数组
    await memory_pool.return_array(array1)
    await memory_pool.return_array(array2)
    
    # 获取统计信息
    stats = await memory_pool.get_stats()
    print(f"内存使用: {stats.total_memory_usage_mb:.2f}MB")
    print(f"缓存命中率: {stats.cache_hit_rate:.2%}")


async def basic_executor_example():
    """基本执行器使用示例"""
    print("\n2. 执行器使用示例")
    
    # 创建执行器
    executor = AsyncHighPerformanceExecutor(
        cpu_workers=4,
        io_workers=8,
        max_concurrent=20
    )
    
    await executor.start()
    
    # 定义一些测试函数
    def cpu_intensive_task(n: int) -> int:
        """CPU密集型任务"""
        result = 0
        for i in range(n):
            result += i * i
        return result
    
    async def io_intensive_task(filename: str) -> str:
        """IO密集型任务"""
        await asyncio.sleep(0.1)  # 模拟IO操作
        return f"处理文件: {filename}"
    
    # 提交任务
    task1_id = await executor.submit_cpu_task(cpu_intensive_task, 1000000)
    task2_id = await executor.submit_io_task(io_intensive_task, "test.txt")
    
    # 获取结果
    result1 = await executor.get_task_result(task1_id)
    result2 = await executor.get_task_result(task2_id)
    
    print(f"CPU任务结果: {result1}")
    print(f"IO任务结果: {result2}")
    
    # 获取统计信息
    stats = await executor.get_stats()
    print(f"完成任务: {stats.completed_tasks}")
    print(f"平均任务时间: {stats.average_task_time:.3f}秒")
    
    await executor.stop()


async def basic_dbc_parser_example():
    """基本DBC解析器使用示例"""
    print("\n3. DBC解析器使用示例")
    
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
 SG_ VehicleSpeed : 16|16@1+ (0.00390625,0) [0|250.99609375] "km/h" Vector__XXX
 SG_ EngineTemperature : 32|8@1+ (1,-40) [-40|215] "degC" Vector__XXX
 SG_ FuelLevel : 40|8@1+ (0.392157,0) [0|100] "%" Vector__XXX

BO_ 200 BrakeData: 8 Vector__XXX
 SG_ BrakePressure : 0|16@1+ (0.01,0) [0|655.35] "bar" Vector__XXX
 SG_ BrakeTemperature : 16|16@1+ (0.1,-273.15) [-273.15|6553.5] "degC" Vector__XXX
"""
    
    with tempfile.NamedTemporaryFile(mode='w', suffix='.dbc', delete=False) as f:
        f.write(dbc_content)
        dbc_file = f.name
    
    try:
        # 创建DBC解析器
        parser = DBCParser(cache_enabled=True)
        
        # 解析DBC文件
        dbc_data = await parser.parse_dbc_file(dbc_file)
        
        print(f"解析到 {len(dbc_data['messages'])} 个消息")
        
        # 显示消息信息
        for msg_id, message in dbc_data['messages'].items():
            print(f"消息 {msg_id}: {message['name']} ({len(message['signals'])} 个信号)")
            
            for signal_name, signal in message['signals'].items():
                print(f"  - {signal_name}: {signal['start_bit']}位, {signal['length']}位")
        
    finally:
        # 清理临时文件
        os.unlink(dbc_file)


async def basic_storage_example():
    """基本存储使用示例"""
    print("\n4. 列式存储使用示例")
    
    # 创建临时输出目录
    with tempfile.TemporaryDirectory() as temp_dir:
        # 创建存储
        storage = ColumnarStorage(
            output_dir=temp_dir,
            compression="snappy",
            partition_strategy="time"
        )
        
        # 模拟CAN数据
        can_data = [
            {
                'timestamp': 1640995200.0,
                'can_id': 100,
                'signal_name': 'EngineSpeed',
                'value': 1500.0,
                'unit': 'rpm'
            },
            {
                'timestamp': 1640995200.1,
                'can_id': 100,
                'signal_name': 'VehicleSpeed',
                'value': 60.0,
                'unit': 'km/h'
            },
            {
                'timestamp': 1640995200.2,
                'can_id': 200,
                'signal_name': 'BrakePressure',
                'value': 2.5,
                'unit': 'bar'
            }
        ]
        
        # 写入数据
        await storage.write_can_data(can_data, partition_key="2022-01-01")
        
        print(f"写入 {len(can_data)} 条记录")
        
        # 查询数据
        result = await storage.query_can_data("SELECT * FROM data")
        
        print("查询结果:")
        for row in result:
            print(f"  - {row}")


async def basic_pipeline_example():
    """基本流水线使用示例"""
    print("\n5. 处理流水线使用示例")
    
    # 创建配置
    config = create_default_config()
    config.input_dir = "test_data"
    config.output_dir = "output"
    
    # 创建处理流水线
    pipeline = AsyncProcessingPipeline(config)
    
    # 处理文件
    result = await pipeline.process_files("test_data")
    
    print(f"处理完成: {result}")


async def main():
    """主函数"""
    print("CANP Python 基本使用示例")
    print("=" * 50)
    
    try:
        # 运行各个示例
        await basic_memory_pool_example()
        await basic_executor_example()
        await basic_dbc_parser_example()
        await basic_storage_example()
        await basic_pipeline_example()
        
        print("\n✅ 所有示例运行完成！")
        
    except Exception as e:
        print(f"\n❌ 示例运行失败: {e}")
        raise


if __name__ == "__main__":
    # 设置日志
    import structlog
    structlog.configure(
        processors=[
            structlog.stdlib.filter_by_level,
            structlog.stdlib.add_logger_name,
            structlog.stdlib.add_log_level,
            structlog.stdlib.PositionalArgumentsFormatter(),
            structlog.processors.TimeStamper(fmt="iso"),
            structlog.processors.StackInfoRenderer(),
            structlog.processors.format_exc_info,
            structlog.processors.UnicodeDecoder(),
            structlog.processors.JSONRenderer()
        ],
        context_class=dict,
        logger_factory=structlog.stdlib.LoggerFactory(),
        wrapper_class=structlog.stdlib.BoundLogger,
        cache_logger_on_first_use=True,
    )
    
    # 运行示例
    asyncio.run(main()) 