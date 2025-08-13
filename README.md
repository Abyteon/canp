# CANP Python - 高性能CAN总线数据处理流水线

一个基于Python的高性能CAN总线数据处理流水线系统，专为大规模汽车数据分析和处理设计。

## 🚀 快速开始

### 使用Pixi管理环境（推荐）

[Pixi](https://pixi.sh/) 是一个跨平台的多语言包管理工具，提供类似Cargo的体验。

#### 安装Pixi

```bash
# 使用官方安装脚本
curl -fsSL https://pixi.sh/install.sh | bash

# 或使用其他包管理器
# macOS
brew install pixi

# Windows
winget install prefix-dev.pixi
```

#### 使用Pixi管理项目

```bash
# 克隆项目
git clone <repository-url>
cd canp-python

# 初始化pixi环境
pixi install

# 激活环境
pixi shell

# 运行测试
pixi run test

# 运行示例
pixi run example

# 代码格式化
pixi run format

# 完整检查
pixi run check
```

### 传统安装方式

```bash
# 克隆项目
git clone <repository-url>
cd canp-python

# 创建虚拟环境
python -m venv venv
source venv/bin/activate  # Linux/macOS
# 或
venv\Scripts\activate  # Windows

# 安装依赖
pip install -e ".[dev]"
```

### 基本使用

```python
import asyncio
from canp import AsyncProcessingPipeline, create_default_config

async def main():
    # 创建配置
    config = create_default_config()
    
    # 创建处理流水线
    pipeline = AsyncProcessingPipeline(config)
    
    # 处理文件
    result = await pipeline.process_files("test_data")
    
    print(f"处理完成: {result}")

if __name__ == "__main__":
    asyncio.run(main())
```

### 运行示例

```bash
python examples/basic_usage.py
```

### 运行测试

```bash
# 使用pixi
pixi run test

# 或传统方式
pytest tests/ -v
```

## 🛠️ Pixi常用命令

```bash
# 环境管理
pixi install          # 安装依赖
pixi shell            # 激活环境
pixi update           # 更新依赖

# 开发任务
pixi run test         # 运行测试
pixi run test-cov     # 运行测试并生成覆盖率报告
pixi run example      # 运行示例
pixi run format       # 格式化代码
pixi run sort-imports # 整理导入
pixi run type-check   # 类型检查
pixi run lint         # 代码质量检查
pixi run check        # 完整检查（格式化+类型检查+测试）
pixi run benchmark    # 性能基准测试

# 项目维护
pixi run clean        # 清理构建文件
pixi run dev-setup    # 开发环境设置
```

## 📦 核心组件

- **内存池**: 基于NumPy的高性能内存管理
- **执行器**: 异步+多进程的混合并发模型
- **DBC解析器**: 高性能的CAN-DBC文件解析
- **列式存储**: 基于JSON的简化存储实现

## 🏗️ 项目结构

```
canp-python/
├── src/canp/
│   ├── __init__.py
│   ├── config.py         # 配置管理
│   ├── memory.py         # 内存池
│   ├── executor.py       # 执行器
│   ├── dbc.py           # DBC解析器
│   ├── storage.py       # 列式存储
│   └── pipeline.py      # 处理流水线
├── tests/
│   └── unit/
│       └── test_basic.py # 基本测试
├── examples/
│   └── basic_usage.py   # 使用示例
├── scripts/
│   ├── activate.sh      # Unix/Linux/macOS激活脚本
│   └── activate.bat     # Windows激活脚本
├── pixi.toml           # Pixi配置文件
├── pyproject.toml      # Python项目配置
└── README.md           # 项目文档
```

## �� 许可证

MIT License 