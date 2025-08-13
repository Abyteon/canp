@echo off
REM Pixi激活脚本 - Windows

echo 🚀 激活CANP Python开发环境...

REM 设置环境变量
set PYTHONPATH=%CD%\src;%PYTHONPATH%
set CANP_PROJECT_ROOT=%CD%

REM 创建必要的目录
if not exist output mkdir output
if not exist test_data mkdir test_data
if not exist .cache mkdir .cache

echo ✅ 环境已激活
echo 📁 项目根目录: %CANP_PROJECT_ROOT%
echo 🐍 Python路径: %PYTHONPATH%
echo.
echo 可用命令:
echo   pixi run test      - 运行测试
echo   pixi run example   - 运行示例
echo   pixi run format    - 格式化代码
echo   pixi run check     - 完整检查 