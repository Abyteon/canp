#!/usr/bin/env python3
"""
生成测试文件：头部35字节 + 数据段，重复拼接至5MB左右
头部格式：31字节任意数据 + 4字节大端序长度
"""

import struct
import random
import os

def generate_test_file(filename, target_size_mb=5):
    target_size = target_size_mb * 1024 * 1024  # 转换为字节
    
    with open(filename, 'wb') as f:
        current_size = 0
        block_count = 0
        
        while current_size < target_size:
            # 计算剩余空间
            remaining = target_size - current_size
            
            # 数据段大小：随机在1KB到50KB之间，但不超过剩余空间
            min_data_size = min(1024, remaining - 35)
            max_data_size = min(50 * 1024, remaining - 35)
            
            if min_data_size <= 0:
                break
                
            data_size = random.randint(min_data_size, max_data_size)
            
            # 生成35字节头部
            header = bytearray(35)
            
            # 前18字节：序列号（仿造任务要求）
            serial_num = f"SERIAL-{block_count:011d}".encode('ascii')[:18]
            header[:len(serial_num)] = serial_num
            
            # 18-30字节：填充数据
            for i in range(18, 31):
                header[i] = random.randint(0, 255)
                
            # 后4字节：数据段长度（大端序）
            header[31:35] = struct.pack('>I', data_size)
            
            # 生成数据段
            data_block = bytearray(data_size)
            
            # 填充一些有意义的测试数据
            pattern = f"BLOCK-{block_count:06d}-DATA:".encode('ascii')
            pattern_len = len(pattern)
            
            for i in range(data_size):
                if i < pattern_len:
                    data_block[i] = pattern[i]
                elif i % 100 == 0:
                    # 每100字节插入一个标记
                    marker = f"[{i:08d}]".encode('ascii')
                    end_pos = min(i + len(marker), data_size)
                    data_block[i:end_pos] = marker[:end_pos-i]
                else:
                    # 其他位置填充可预测的数据
                    data_block[i] = (block_count * 17 + i) % 256
            
            # 写入文件
            f.write(header)
            f.write(data_block)
            
            current_size += 35 + data_size
            block_count += 1
            
            # 每100个块显示进度
            if block_count % 100 == 0:
                progress = (current_size / target_size) * 100
                print(f"进度: {progress:.1f}% ({current_size/1024/1024:.2f}MB), 块数: {block_count}")
    
    actual_size = os.path.getsize(filename)
    print(f"\n文件生成完成:")
    print(f"文件名: {filename}")
    print(f"目标大小: {target_size_mb}MB")
    print(f"实际大小: {actual_size/1024/1024:.2f}MB ({actual_size} 字节)")
    print(f"总块数: {block_count}")
    print(f"平均块大小: {(actual_size-block_count*35)/block_count:.0f} 字节 (数据部分)")

def verify_file_format(filename, check_blocks=10):
    """验证文件格式是否正确"""
    print(f"\n验证文件格式 (检查前{check_blocks}个块):")
    
    with open(filename, 'rb') as f:
        for i in range(check_blocks):
            # 读取35字节头部
            header = f.read(35)
            if len(header) != 35:
                print(f"块 {i}: 头部读取失败，文件可能已结束")
                break
                
            # 解析序列号
            serial = header[:18].rstrip(b'\x00').decode('ascii', errors='ignore')
            
            # 解析数据长度
            data_length = struct.unpack('>I', header[31:35])[0]
            
            # 读取数据段开头
            data_start = f.read(min(50, data_length))
            
            # 跳过剩余数据
            if data_length > 50:
                f.seek(data_length - 50, 1)
            
            print(f"块 {i}: 序列号='{serial}', 数据长度={data_length}, 数据开头={data_start[:20]}")

if __name__ == "__main__":
    filename = "test_5mb_blocks.bin"
    
    print("🚀 开始生成5MB测试文件...")
    generate_test_file(filename)
    
    print("\n🔍 验证文件格式...")
    verify_file_format(filename)
    
    print("\n✅ 测试文件生成完成！")
