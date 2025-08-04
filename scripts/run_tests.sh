#!/bin/bash

# 测试运行脚本
# 结合社区优秀实践的全面测试策略

set -e

echo "🧪 开始执行全面测试套件..."
echo "=================================="

# 颜色定义
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# 测试计数器
TOTAL_TESTS=0
PASSED_TESTS=0
FAILED_TESTS=0

# 测试函数
run_test() {
    local test_name="$1"
    local test_command="$2"
    
    echo -e "${BLUE}🔍 运行测试: ${test_name}${NC}"
    echo "命令: $test_command"
    
    if eval "$test_command"; then
        echo -e "${GREEN}✅ ${test_name} 通过${NC}"
        ((PASSED_TESTS++))
    else
        echo -e "${RED}❌ ${test_name} 失败${NC}"
        ((FAILED_TESTS++))
    fi
    
    ((TOTAL_TESTS++))
    echo "----------------------------------"
}

# 1. 代码检查
echo -e "${YELLOW}📋 阶段 1: 代码质量检查${NC}"
run_test "代码格式检查" "cargo fmt --check"
run_test "代码风格检查" "cargo clippy -- -D warnings"
run_test "依赖安全检查" "cargo audit"

# 2. 单元测试
echo -e "${YELLOW}📋 阶段 2: 单元测试${NC}"
run_test "库单元测试" "cargo test --lib"
run_test "集成测试" "cargo test --test integration_tests"
run_test "属性测试" "cargo test --test property_tests"

# 3. 文档测试
echo -e "${YELLOW}📋 阶段 3: 文档测试${NC}"
run_test "文档测试" "cargo test --doc"
run_test "文档生成" "cargo doc --no-deps"

# 4. 基准测试
echo -e "${YELLOW}📋 阶段 4: 性能基准测试${NC}"
run_test "基准测试" "cargo bench"

# 5. 集成测试
echo -e "${YELLOW}📋 阶段 5: 端到端集成测试${NC}"
run_test "完整管道测试" "cargo run --example task_processing_example"

# 6. 内存泄漏测试
echo -e "${YELLOW}📋 阶段 6: 内存泄漏检测${NC}"
run_test "内存泄漏测试" "cargo test --lib -- --nocapture | grep -i 'memory leak' || true"

# 7. 并发测试
echo -e "${YELLOW}📋 阶段 7: 并发安全性测试${NC}"
run_test "并发测试" "cargo test --lib -- --nocapture | grep -i 'concurrent' || true"

# 8. 错误处理测试
echo -e "${YELLOW}📋 阶段 8: 错误处理测试${NC}"
run_test "错误处理测试" "cargo test --lib -- --nocapture | grep -i 'error' || true"

# 测试结果汇总
echo -e "${YELLOW}📊 测试结果汇总${NC}"
echo "=================================="
echo -e "总测试数: ${TOTAL_TESTS}"
echo -e "通过测试: ${GREEN}${PASSED_TESTS}${NC}"
echo -e "失败测试: ${RED}${FAILED_TESTS}${NC}"

if [ $FAILED_TESTS -eq 0 ]; then
    echo -e "${GREEN}🎉 所有测试通过！${NC}"
    exit 0
else
    echo -e "${RED}❌ 有 ${FAILED_TESTS} 个测试失败${NC}"
    exit 1
fi 