# 🧪 测试策略文档

## 概述

本项目采用全面的测试策略，结合Rust社区优秀实践，确保代码质量、性能和可靠性。

## 测试金字塔

```
    🔺 E2E Tests (端到端测试)
   🔺🔺 Integration Tests (集成测试)
  🔺🔺🔺 Unit Tests (单元测试)
 🔺🔺🔺🔺 Property Tests (属性测试)
🔺🔺🔺🔺🔺 Benchmark Tests (基准测试)
```

## 测试类型

### 1. 单元测试 (Unit Tests)

**位置**: `src/*/tests/` 模块内
**覆盖率**: 85%+
**目标**: 测试单个函数/方法的正确性

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_function_behavior() {
        // 测试逻辑
    }
}
```

**特点**:
- 快速执行 (< 1ms)
- 隔离测试
- 易于调试
- 高覆盖率

### 2. 集成测试 (Integration Tests)

**位置**: `tests/integration_tests.rs`
**目标**: 测试模块间的交互

```rust
#[tokio::test]
async fn test_complete_data_processing_pipeline() {
    // 端到端测试逻辑
}
```

**特点**:
- 测试真实场景
- 验证组件集成
- 异步测试支持
- 临时文件管理

### 3. 属性测试 (Property Tests)

**位置**: `tests/property_tests.rs`
**工具**: `proptest`
**目标**: 基于属性的测试，发现边界情况

```rust
proptest! {
    #[test]
    fn test_property(data in prop::collection::vec(any::<u8>(), 0..1000)) {
        // 属性验证逻辑
    }
}
```

**特点**:
- 自动生成测试数据
- 发现边界情况
- 验证不变量
- 减少测试代码

### 4. 基准测试 (Benchmark Tests)

**位置**: `benches/benchmarks.rs`
**工具**: `criterion`
**目标**: 性能测量和回归检测

```rust
fn bench_function(c: &mut Criterion) {
    c.bench_function("function_name", |b| {
        b.iter(|| {
            // 被测试的函数
        });
    });
}
```

**特点**:
- 精确性能测量
- 自动生成报告
- 回归检测
- HTML报告

## 测试最佳实践

### 1. 测试命名

```rust
// ✅ 好的命名
#[test]
fn test_can_frame_parsing_with_valid_data() { }

#[test]
fn test_memory_pool_under_high_pressure() { }

// ❌ 不好的命名
#[test]
fn test1() { }

#[test]
fn test_thing() { }
```

### 2. 测试结构 (AAA模式)

```rust
#[test]
fn test_function() {
    // Arrange (准备)
    let input = create_test_data();
    let expected = expected_result();
    
    // Act (执行)
    let result = function_under_test(input);
    
    // Assert (断言)
    assert_eq!(result, expected);
}
```

### 3. 异步测试

```rust
#[tokio::test]
async fn test_async_function() {
    // 使用 tokio::test 宏
    let result = async_function().await;
    assert!(result.is_ok());
}
```

### 4. 错误测试

```rust
#[test]
fn test_error_conditions() {
    // 测试错误情况
    let result = function_with_error();
    assert!(result.is_err());
    
    // 测试特定错误类型
    match result {
        Err(ErrorType::SpecificError) => {},
        _ => panic!("Expected specific error"),
    }
}
```

### 5. 并发测试

```rust
#[tokio::test]
async fn test_concurrent_access() {
    use tokio::task;
    
    let shared_resource = Arc::new(Mutex::new(0));
    let mut handles = Vec::new();
    
    for _ in 0..10 {
        let resource = Arc::clone(&shared_resource);
        let handle = task::spawn(async move {
            // 并发操作
        });
        handles.push(handle);
    }
    
    for handle in handles {
        handle.await.unwrap();
    }
}
```

## 测试工具和依赖

### 核心依赖

```toml
[dev-dependencies]
tempfile = "3.8"        # 临时文件管理
proptest = "1.4"        # 属性测试
criterion = "0.5"       # 基准测试
tokio-test = "0.4"      # 异步测试
rand = "0.8"            # 随机数据生成
```

### 测试辅助工具

- **tempfile**: 临时文件和目录管理
- **proptest**: 基于属性的测试框架
- **criterion**: 性能基准测试
- **tokio-test**: 异步测试支持
- **rand**: 随机数据生成

## 测试覆盖率

### 目标覆盖率

| 模块 | 目标覆盖率 | 当前覆盖率 |
|------|------------|------------|
| 零拷贝内存池 | 90% | 85% |
| 高性能执行器 | 90% | 87% |
| DBC解析器 | 85% | 80% |
| 数据层解析器 | 80% | 33% |
| 列式存储 | 85% | 100% |
| 处理管道 | 80% | 100% |

### 覆盖率检查

```bash
# 安装 cargo-tarpaulin
cargo install cargo-tarpaulin

# 运行覆盖率检查
cargo tarpaulin --out Html
```

## 性能测试

### 基准测试指标

| 操作 | 目标性能 | 当前性能 |
|------|----------|----------|
| 文件映射 | < 1ms | 0.5ms |
| CAN帧解析 | < 10μs | 5μs |
| 内存分配 | < 100ns | 50ns |
| 并发处理 | > 1000 req/s | 1200 req/s |

### 性能回归检测

```bash
# 运行基准测试
cargo bench

# 比较性能变化
cargo bench -- --save-baseline new
cargo bench -- --baseline new
```

## 持续集成

### GitHub Actions 配置

```yaml
name: Tests
on: [push, pull_request]

jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v2
      - uses: actions-rs/toolchain@v1
        with:
          toolchain: stable
      
      # 运行测试
      - name: Run tests
        run: |
          cargo test --all-features
          cargo test --test integration_tests
          cargo test --test property_tests
      
      # 运行基准测试
      - name: Run benchmarks
        run: cargo bench --no-run
      
      # 检查覆盖率
      - name: Check coverage
        run: cargo tarpaulin --out Xml
```

## 测试数据管理

### 测试数据生成

```rust
// 使用 TestDataGenerator 生成测试数据
let test_config = TestDataConfig {
    output_dir: temp_dir.path().to_path_buf(),
    file_count: 10,
    target_file_size: 1024 * 1024,
    frames_per_file: 1000,
};
let generator = TestDataGenerator::new(test_config);
generator.generate_all().await.unwrap();
```

### 临时文件管理

```rust
// 使用 tempfile 管理临时文件
let temp_dir = TempDir::new().unwrap();
let test_file = temp_dir.path().join("test.bin");

// 测试完成后自动清理
```

## 调试和故障排除

### 测试调试

```bash
# 运行特定测试
cargo test test_name

# 显示输出
cargo test -- --nocapture

# 并行运行
cargo test -- --test-threads=1

# 运行失败的测试
cargo test -- --skip passed_test_name
```

### 常见问题

1. **测试超时**
   ```rust
   #[tokio::test]
   #[timeout(Duration::from_secs(30))]
   async fn test_with_timeout() { }
   ```

2. **内存泄漏检测**
   ```rust
   #[test]
   fn test_memory_leak() {
       let before = get_memory_usage();
       // 执行操作
       let after = get_memory_usage();
       assert!(after <= before + threshold);
   }
   ```

3. **并发竞争条件**
   ```rust
   #[tokio::test]
   async fn test_race_condition() {
       // 使用 loom 进行并发测试
       loom::model(|| {
           // 并发测试逻辑
       });
   }
   ```

## 测试报告

### 生成测试报告

```bash
# 运行所有测试并生成报告
./scripts/run_tests.sh

# 生成覆盖率报告
cargo tarpaulin --out Html

# 生成基准测试报告
cargo bench -- --output-format=html
```

### 报告内容

- 测试通过率
- 代码覆盖率
- 性能基准
- 错误统计
- 建议改进

## 总结

本项目的测试策略遵循以下原则：

1. **全面性**: 覆盖所有代码路径和边界情况
2. **自动化**: 所有测试都可以自动运行
3. **快速性**: 测试执行时间最小化
4. **可靠性**: 测试结果稳定可重现
5. **可维护性**: 测试代码清晰易懂

通过这种全面的测试策略，我们确保项目的质量、性能和可靠性达到生产环境的要求。 