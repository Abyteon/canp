#!/bin/bash
# Pixi激活脚本 - Unix/Linux/macOS

echo "🚀 激活CANP Python开发环境..."

# 设置环境变量
export PYTHONPATH="${PWD}/src:${PYTHONPATH}"
export CANP_PROJECT_ROOT="${PWD}"

# 创建必要的目录
mkdir -p output
mkdir -p test_data
mkdir -p .cache

echo "✅ 环境已激活"
echo "📁 项目根目录: ${CANP_PROJECT_ROOT}"
echo "🐍 Python路径: ${PYTHONPATH}"
echo ""
echo "可用命令:"
echo "  pixi run test      - 运行测试"
echo "  pixi run example   - 运行示例"
echo "  pixi run format    - 格式化代码"
echo "  pixi run check     - 完整检查" 