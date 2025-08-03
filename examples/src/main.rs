use canp::dbc_parser::{DbcParser, DbcParserConfig};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== DBC解析器测试 ===\n");

    // 创建解析器配置
    let config = DbcParserConfig {
        verbose: true,
        validate: true,
        max_file_size: 10 * 1024 * 1024,
    };
    let parser = DbcParser::new(config);

    // 测试1: 解析官方DBC文件
    println!("1. 测试解析官方DBC文件 (official_sample.dbc)");
    match parser.parse_file("../examples/official_sample.dbc") {
        Ok(result) => {
            println!("   ✓ 解析成功!");
            println!("   - 消息数量: {}", result.stats.message_count);
            println!("   - 信号数量: {}", result.stats.signal_count);
            println!("   - 节点数量: {}", result.stats.node_count);
            println!("   - 解析时间: {}ms", result.stats.parse_time_ms);

            // 获取消息信息
            let messages_info = parser.get_messages_info(&result.dbc);
            println!("   - 消息详情:");
            for (msg_id, msg_info) in messages_info {
                println!("     * ID {}: {} ({} 字节, {} 信号)", 
                    msg_id, msg_info.name, msg_info.message_size, msg_info.signals.len());
                
                for signal in &msg_info.signals {
                    println!("       - {}: {}-{} 位, 因子={}, 偏移={}, 单位='{}'", 
                        signal.name, signal.start_bit, signal.start_bit + signal.signal_size - 1,
                        signal.factor, signal.offset, signal.unit);
                }
            }
        }
        Err(e) => {
            println!("   ✗ 解析失败: {}", e);
        }
    }

    println!();

    // 测试2: 解析简单DBC文件
    println!("2. 测试解析简单DBC文件 (sample.dbc)");
    match parser.parse_file("../examples/sample.dbc") {
        Ok(result) => {
            println!("   ✓ 解析成功!");
            println!("   - 消息数量: {}", result.stats.message_count);
            println!("   - 信号数量: {}", result.stats.signal_count);
            println!("   - 节点数量: {}", result.stats.node_count);
            println!("   - 解析时间: {}ms", result.stats.parse_time_ms);
        }
        Err(e) => {
            println!("   ✗ 解析失败: {}", e);
        }
    }

    println!();

    // 测试3: 解析复杂DBC文件
    println!("3. 测试解析复杂DBC文件 (complex.dbc)");
    match parser.parse_file("../examples/complex.dbc") {
        Ok(result) => {
            println!("   ✓ 解析成功!");
            println!("   - 消息数量: {}", result.stats.message_count);
            println!("   - 信号数量: {}", result.stats.signal_count);
            println!("   - 节点数量: {}", result.stats.node_count);
            println!("   - 解析时间: {}ms", result.stats.parse_time_ms);
        }
        Err(e) => {
            println!("   ✗ 解析失败: {}", e);
        }
    }

    println!();

    // 测试4: 验证DBC内容
    println!("4. 测试DBC内容验证");
    if let Ok(result) = parser.parse_file("../examples/official_sample.dbc") {
        let (is_valid, errors) = parser.validate_dbc(&result.dbc);
        if is_valid {
            println!("   ✓ DBC内容验证通过");
        } else {
            println!("   ✗ DBC内容验证失败:");
            for error in errors {
                println!("     - {}", error);
            }
        }
    }

    println!();

    // 测试5: 测试无效内容
    println!("5. 测试无效DBC内容");
    let invalid_content = "This is not a valid DBC file content";
    match parser.parse_content(invalid_content) {
        Ok(_) => {
            println!("   ✗ 应该失败但成功了");
        }
        Err(e) => {
            println!("   ✓ 正确识别无效内容: {}", e);
        }
    }

    println!();

    // 测试6: 测试边界情况
    println!("6. 测试边界情况");
    let empty_content = "";
    match parser.parse_content(empty_content) {
        Ok(_) => {
            println!("   ✓ 空内容解析成功");
        }
        Err(e) => {
            println!("   ✗ 空内容解析失败: {}", e);
        }
    }

    println!("\n=== 测试完成 ===");
    Ok(())
} 