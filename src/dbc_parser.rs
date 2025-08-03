//! # DBC解析模块 (DBC Parser Module)
//! 
//! 基于can-dbc库实现DBC文件解析功能，提供简洁的接口用于解析CAN网络数据。
//! 
//! ## 设计理念
//! 
//! - **复用库实现**：直接使用can-dbc库的解析功能
//! - **简洁接口**：提供易于使用的API
//! - **零拷贝**：尽可能避免不必要的数据拷贝
//! - **错误处理**：提供清晰的错误信息

use anyhow::{Context, Result};
use can_dbc::{DBC, Error as DbcError};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use tracing::{debug, info};

/// DBC解析器配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DbcParserConfig {
    /// 是否启用详细日志
    pub verbose: bool,
    /// 是否验证DBC格式
    pub validate: bool,
    /// 最大文件大小（字节）
    pub max_file_size: usize,
}

impl Default for DbcParserConfig {
    fn default() -> Self {
        Self {
            verbose: false,
            validate: true,
            max_file_size: 10 * 1024 * 1024, // 10MB
        }
    }
}

/// DBC解析结果
#[derive(Debug, Clone)]
pub struct DbcParseResult {
    /// 解析的DBC内容
    pub dbc: DBC,
    /// 解析统计信息
    pub stats: DbcParseStats,
    /// 解析错误（如果有）
    pub errors: Vec<String>,
}

/// DBC解析统计信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DbcParseStats {
    /// 消息数量
    pub message_count: usize,
    /// 信号数量
    pub signal_count: usize,
    /// 节点数量
    pub node_count: usize,
    /// 解析时间（毫秒）
    pub parse_time_ms: u64,
}

impl Default for DbcParseStats {
    fn default() -> Self {
        Self {
            message_count: 0,
            signal_count: 0,
            node_count: 0,
            parse_time_ms: 0,
        }
    }
}

/// DBC解析器
/// 
/// 提供DBC文件解析功能，基于can-dbc库实现
pub struct DbcParser {
    config: DbcParserConfig,
}

impl DbcParser {
    /// 创建新的DBC解析器
    /// 
    /// ## 参数
    /// 
    /// - `config`：解析器配置
    /// 
    /// ## 返回值
    /// 
    /// 返回配置好的DBC解析器
    pub fn new(config: DbcParserConfig) -> Self {
        Self { config }
    }

    /// 从文件解析DBC
    /// 
    /// ## 参数
    /// 
    /// - `path`：DBC文件路径
    /// 
    /// ## 返回值
    /// 
    /// 成功时返回解析结果，失败时返回错误
    /// 
    /// ## 示例
    /// 
    /// ```rust
    /// use canp::dbc_parser::{DbcParser, DbcParserConfig};
    /// 
    /// fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     let config = DbcParserConfig::default();
    ///     let parser = DbcParser::new(config);
    ///     
    ///     // 解析DBC内容
    ///     let dbc_content = r#"
    /// VERSION ""
    /// 
    /// NS_ :
    ///     NS_DESC_
    ///     CM_
    ///     BA_DEF_
    ///     BA_
    ///     VAL_
    ///     CAT_DEF_
    ///     CAT_
    ///     FILTER
    ///     BA_DEF_DEF_
    ///     EV_DATA_
    ///     ENVVAR_DATA_
    ///     SGTYPE_
    ///     SGTYPE_VAL_
    ///     BA_DEF_SGTYPE_
    ///     BA_SGTYPE_
    ///     SIG_TYPE_REF_
    ///     VAL_TABLE_
    ///     SIG_GROUP_
    ///     SIG_VALTYPE_
    ///     SIGTYPE_VALTYPE_
    ///     BO_TX_BU_
    ///     BA_DEF_REL_
    ///     BA_REL_
    ///     BA_DEF_DEF_REL_
    ///     BU_SG_REL_
    ///     BU_EV_REL_
    ///     BU_BO_REL_
    /// 
    /// BS_:
    /// 
    /// BU_: Vector__XXX
    /// 
    /// BO_ 100 EngineData: 8 Vector__XXX
    ///  SG_ EngineSpeed : 0|16@1+ (0.125,0) [0|8031.875] "rpm" Vector__XXX
    ///  SG_ EngineTemp : 16|8@1+ (1,-40) [-40|215] "degC" Vector__XXX
    /// "#;
    ///     
    ///     let result = parser.parse_content(dbc_content)?;
    ///     println!("解析了 {} 个消息", result.stats.message_count);
    ///     Ok(())
    /// }
    /// ```
    pub fn parse_file<P: AsRef<Path>>(&self, path: P) -> Result<DbcParseResult> {
        let start_time = std::time::Instant::now();
        let path = path.as_ref();
        
        info!("开始解析DBC文件: {:?}", path);
        
        // 读取文件
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("无法读取DBC文件: {:?}", path))?;
        
        // 检查文件大小
        if content.len() > self.config.max_file_size {
            return Err(anyhow::anyhow!(
                "DBC文件过大: {} bytes (最大: {} bytes)",
                content.len(),
                self.config.max_file_size
            ));
        }
        
        // 解析DBC
        let parse_result = self.parse_content(&content)?;
        
        let parse_time = start_time.elapsed();
        let stats = DbcParseStats {
            message_count: parse_result.dbc.messages().len(),
            signal_count: parse_result.dbc.messages().iter().map(|m| m.signals().len()).sum(),
            node_count: parse_result.dbc.nodes().len(),
            parse_time_ms: parse_time.as_millis() as u64,
        };
        
        info!(
            "DBC解析完成: {} 消息, {} 信号, {} 节点, 耗时 {}ms",
            stats.message_count, stats.signal_count, stats.node_count, stats.parse_time_ms
        );
        
        Ok(DbcParseResult {
            dbc: parse_result.dbc,
            stats,
            errors: parse_result.errors,
        })
    }

    /// 从字符串内容解析DBC
    /// 
    /// ## 参数
    /// 
    /// - `content`：DBC文件内容
    /// 
    /// ## 返回值
    /// 
    /// 成功时返回解析结果，失败时返回错误
    pub fn parse_content(&self, content: &str) -> Result<DbcParseResult> {
        let start_time = std::time::Instant::now();
        let mut errors = Vec::new();
        
        // 使用can-dbc库解析
        let dbc = match DBC::try_from(content) {
            Ok(dbc) => dbc,
            Err(e) => {
                let error_msg = match e {
                    DbcError::Nom(nom_err) => format!("解析错误: {:?}", nom_err),
                    DbcError::Incomplete(_dbc, remaining) => {
                        format!(
                            "部分解析成功，剩余未解析数据长度: {}",
                            remaining.len()
                        )
                    }
                    DbcError::MultipleMultiplexors => "多个多路复用器定义".to_string(),
                };
                
                if self.config.validate {
                    return Err(anyhow::anyhow!("DBC解析失败: {}", error_msg));
                } else {
                    errors.push(error_msg);
                    // 尝试继续解析，即使有错误
                    return Err(anyhow::anyhow!("DBC解析失败: {}", errors.join("; ")));
                }
            }
        };
        
        let parse_time = start_time.elapsed();
        let stats = DbcParseStats {
            message_count: dbc.messages().len(),
            signal_count: dbc.messages().iter().map(|m| m.signals().len()).sum(),
            node_count: dbc.nodes().len(),
            parse_time_ms: parse_time.as_millis() as u64,
        };
        
        if self.config.verbose {
            debug!(
                "DBC内容解析: {} 消息, {} 信号, {} 节点",
                stats.message_count, stats.signal_count, stats.node_count
            );
        }
        
        Ok(DbcParseResult {
            dbc,
            stats,
            errors,
        })
    }

    /// 获取消息信息
    /// 
    /// ## 参数
    /// 
    /// - `dbc`：解析的DBC内容
    /// 
    /// ## 返回值
    /// 
    /// 返回消息ID到消息信息的映射
    pub fn get_messages_info(&self, dbc: &DBC) -> HashMap<u32, MessageInfo> {
        let mut messages = HashMap::new();
        
        for message in dbc.messages() {
            let signals: Vec<SignalInfo> = message
                .signals()
                .iter()
                .map(|signal| SignalInfo {
                    name: signal.name().clone(),
                    start_bit: signal.start_bit,
                    signal_size: signal.signal_size,
                    byte_order: format!("{:?}", signal.byte_order()),
                    value_type: format!("{:?}", signal.value_type()),
                    factor: signal.factor,
                    offset: signal.offset,
                    min_value: signal.min,
                    max_value: signal.max,
                    unit: signal.unit().clone(),
                    receivers: signal.receivers().clone(),
                })
                .collect();
            
            messages.insert(
                message.message_id().raw(),
                MessageInfo {
                    name: message.message_name().clone(),
                    message_id: message.message_id().raw(),
                    message_size: *message.message_size() as u8,
                    transmitter: format!("{:?}", message.transmitter()),
                    signals,
                },
            );
        }
        
        messages
    }

    /// 验证DBC内容
    /// 
    /// ## 参数
    /// 
    /// - `dbc`：要验证的DBC内容
    /// 
    /// ## 返回值
    /// 
    /// 返回验证结果和错误列表
    pub fn validate_dbc(&self, dbc: &DBC) -> (bool, Vec<String>) {
        let mut errors = Vec::new();
        
        // 检查消息
        for message in dbc.messages() {
            // 检查信号是否重叠
            let mut used_bits = Vec::new();
            
            for signal in message.signals() {
                let start = signal.start_bit;
                let end = start + signal.signal_size - 1;
                
                // 检查位重叠
                for &(existing_start, existing_end) in &used_bits {
                    if start <= existing_end && end >= existing_start {
                        errors.push(format!(
                            "消息 {} 中的信号 {} 位重叠: {}-{} 与 {}-{}",
                            message.message_name(),
                            signal.name(),
                            start,
                            end,
                            existing_start,
                            existing_end
                        ));
                    }
                }
                
                used_bits.push((start, end));
            }
        }
        
        (errors.is_empty(), errors)
    }
}

/// 消息信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageInfo {
    /// 消息名称
    pub name: String,
    /// 消息ID
    pub message_id: u32,
    /// 消息大小（字节）
    pub message_size: u8,
    /// 发送节点
    pub transmitter: String,
    /// 信号列表
    pub signals: Vec<SignalInfo>,
}

/// 信号信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalInfo {
    /// 信号名称
    pub name: String,
    /// 起始位
    pub start_bit: u64,
    /// 信号大小（位）
    pub signal_size: u64,
    /// 字节序
    pub byte_order: String,
    /// 值类型
    pub value_type: String,
    /// 因子
    pub factor: f64,
    /// 偏移量
    pub offset: f64,
    /// 最小值
    pub min_value: f64,
    /// 最大值
    pub max_value: f64,
    /// 单位
    pub unit: String,
    /// 接收节点列表
    pub receivers: Vec<String>,
}

impl Default for DbcParser {
    fn default() -> Self {
        Self::new(DbcParserConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dbc_parser_creation() {
        let config = DbcParserConfig::default();
        let parser = DbcParser::new(config);
        assert_eq!(parser.config.max_file_size, 10 * 1024 * 1024);
    }

    #[test]
    fn test_parse_invalid_content() {
        let parser = DbcParser::default();
        let result = parser.parse_content("invalid dbc content");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_valid_content() {
        let valid_dbc = r#"
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
 SG_ EngineTemp : 16|8@1+ (1,-40) [-40|215] "degC" Vector__XXX
"#;
        
        let parser = DbcParser::default();
        let result = parser.parse_content(valid_dbc);
        assert!(result.is_ok());
        
        if let Ok(parse_result) = result {
            assert_eq!(parse_result.stats.message_count, 1);
            assert_eq!(parse_result.stats.signal_count, 2);
        }
    }
} 