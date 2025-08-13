"""
DBC解析器模块

高性能的CAN-DBC文件解析和信号提取。
"""

import re
import asyncio
from typing import Dict, List, Optional, Any
from dataclasses import dataclass
from pathlib import Path
import time
import structlog

logger = structlog.get_logger(__name__)


@dataclass
class Signal:
    """CAN信号定义"""
    name: str
    start_bit: int
    length: int
    byte_order: str  # 'little_endian' or 'big_endian'
    value_type: str  # 'signed' or 'unsigned'
    factor: float
    offset: float
    min_value: float
    max_value: float
    unit: str
    receivers: List[str]


@dataclass
class Message:
    """CAN消息定义"""
    id: int
    name: str
    dlc: int
    signals: Dict[str, Signal]
    senders: List[str]


class DBCParser:
    """DBC文件解析器 - 高性能实现"""
    
    def __init__(self, cache_enabled: bool = True, cache_dir: str = ".cache", max_workers: int = 4):
        self.cache_enabled = cache_enabled
        self.cache_dir = cache_dir
        self.max_workers = max_workers
        
        # 缓存
        self._dbc_cache: Dict[str, Dict] = {}
        self._signal_cache: Dict[str, Dict] = {}
        
        # 统计信息
        self._stats = {
            'parsed_files': 0,
            'parsed_messages': 0,
            'parsed_signals': 0,
            'cache_hits': 0,
            'cache_misses': 0
        }
    
    async def parse_dbc_file(self, file_path: str) -> Dict:
        """解析DBC文件"""
        # 检查缓存
        if file_path in self._dbc_cache:
            self._stats['cache_hits'] += 1
            return self._dbc_cache[file_path]
        
        self._stats['cache_misses'] += 1
        
        # 解析文件
        result = self._parse_dbc_file_sync(file_path)
        
        # 缓存结果
        self._dbc_cache[file_path] = result
        self._stats['parsed_files'] += 1
        
        return result
    
    def _parse_dbc_file_sync(self, file_path: str) -> Dict:
        """同步解析DBC文件"""
        messages = {}
        current_message = None
        
        try:
            with open(file_path, 'r', encoding='utf-8') as f:
                for line in f:
                    line = line.strip()
                    
                    # 解析消息定义
                    if line.startswith('BO_ '):
                        parts = line.split()
                        msg_id = int(parts[1])
                        msg_name = parts[2].rstrip(':')
                        dlc = int(parts[3])
                        
                        current_message = Message(
                            id=msg_id,
                            name=msg_name,
                            dlc=dlc,
                            signals={},
                            senders=[]
                        )
                        messages[msg_id] = current_message
                        self._stats['parsed_messages'] += 1
                    
                    # 解析信号定义
                    elif line.startswith(' SG_ ') and current_message:
                        signal = self._parse_signal_line(line)
                        current_message.signals[signal.name] = signal
                        self._stats['parsed_signals'] += 1
        except Exception as e:
            logger.error("解析DBC文件失败", file_path=file_path, error=str(e))
            raise
        
        return {
            'messages': messages,
            'file_path': file_path,
            'parsed_at': time.time()
        }
    
    def _parse_signal_line(self, line: str) -> Signal:
        """解析信号行"""
        # 示例: SG_ VehicleSpeed : 0|16@1+ (0.01,0) [0|655.35] "km/h" Vector__XXX
        parts = line.split()
        name = parts[1].rstrip(':')
        
        # 解析位定义
        bit_def = parts[2]
        start_bit, length = map(int, bit_def.split('|'))
        
        # 解析字节序和值类型
        byte_order = 'little_endian' if '@1' in bit_def else 'big_endian'
        value_type = 'signed' if '-' in bit_def else 'unsigned'
        
        # 解析因子和偏移
        factor_offset = parts[3].strip('()').split(',')
        factor = float(factor_offset[0])
        offset = float(factor_offset[1])
        
        # 解析范围
        range_def = parts[4].strip('[]').split('|')
        min_value = float(range_def[0])
        max_value = float(range_def[1])
        
        # 解析单位
        unit = parts[5].strip('"')
        
        # 解析接收者
        receivers = parts[6].split(',') if len(parts) > 6 else []
        
        return Signal(
            name=name,
            start_bit=start_bit,
            length=length,
            byte_order=byte_order,
            value_type=value_type,
            factor=factor,
            offset=offset,
            min_value=min_value,
            max_value=max_value,
            unit=unit,
            receivers=receivers
        )
    
    def extract_signal_value(self, data: bytes, signal_name: str, message_id: int) -> float:
        """提取信号值 - 高性能位操作"""
        # 获取信号定义
        if message_id not in self._dbc_cache.get('current_file', {}).get('messages', {}):
            raise ValueError(f"消息ID {message_id} 未找到")
        
        message = self._dbc_cache['current_file']['messages'][message_id]
        if signal_name not in message.signals:
            raise ValueError(f"信号 {signal_name} 在消息 {message_id} 中未找到")
        
        signal = message.signals[signal_name]
        
        # 简化的位提取算法
        if signal.byte_order == 'little_endian':
            # 小端序
            result = 0
            bit_pos = 0
            
            for i, byte in enumerate(data):
                if bit_pos >= signal.length:
                    break
                
                bits_to_take = min(8, signal.length - bit_pos)
                mask = (1 << bits_to_take) - 1
                value = (byte >> (bit_pos % 8)) & mask
                result |= value << bit_pos
                bit_pos += bits_to_take
        
        else:
            # 大端序
            result = 0
            bit_pos = signal.length - 1
            
            for i, byte in enumerate(data):
                if bit_pos < 0:
                    break
                
                for j in range(8):
                    if bit_pos < 0:
                        break
                    if byte & (1 << j):
                        result |= (1 << bit_pos)
                    bit_pos -= 1
        
        # 应用因子和偏移
        raw_value = result
        if signal.value_type == 'signed' and raw_value > (1 << (signal.length - 1)):
            raw_value -= (1 << signal.length)
        
        physical_value = raw_value * signal.factor + signal.offset
        return physical_value
    
    async def get_stats(self) -> Dict:
        """获取解析器统计信息"""
        return self._stats.copy() 