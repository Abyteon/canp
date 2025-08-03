use crate::memory_pool::MmapBlock;
use crate::dbc_parser::{DbcParser, DbcParseResult};
use anyhow::{Result, anyhow};
use std::sync::Arc;
use tracing::{info, warn};

/// 分层解析器配置
#[derive(Debug, Clone)]
pub struct LayerParserConfig {
    /// 是否启用解压缩
    pub enable_decompression: bool,
    /// 解压缩缓冲区大小
    pub decompress_buffer_size: usize,
    /// 批处理大小
    pub batch_size: usize,
    /// 是否启用性能监控
    pub enable_monitoring: bool,
}

impl Default for LayerParserConfig {
    fn default() -> Self {
        Self {
            enable_decompression: true,
            decompress_buffer_size: 1024 * 1024, // 1MB
            batch_size: 100,
            enable_monitoring: true,
        }
    }
}

/// 层类型枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LayerType {
    /// 第0层：文件头部 + 压缩数据
    Layer0,
    /// 第1层：头部 + 未压缩数据
    Layer1,
    /// 第2层：头部 + 未压缩数据
    Layer2,
    /// 最后一层：单帧数据
    FinalLayer,
}

/// 解析结果
#[derive(Debug, Clone)]
pub struct ParseResult {
    /// 解析的层类型
    pub layer_type: LayerType,
    /// 解析的数据块
    pub data_blocks: Vec<DataBlock>,
    /// 解析统计信息
    pub stats: ParseStats,
}

/// 数据块
#[derive(Debug, Clone)]
pub struct DataBlock {
    /// 数据指针和长度
    pub ptr_and_len: (usize, usize),
    /// 数据块类型
    pub block_type: BlockType,
    /// 元数据
    pub metadata: Option<BlockMetadata>,
}

/// 数据块类型
#[derive(Debug, Clone, PartialEq)]
pub enum BlockType {
    /// 头部数据
    Header,
    /// 压缩数据
    CompressedData,
    /// 未压缩数据
    UncompressedData,
    /// 单帧数据
    FrameData,
}

/// 数据块元数据
#[derive(Debug, Clone)]
pub struct BlockMetadata {
    /// 原始偏移量
    pub original_offset: usize,
    /// 数据长度
    pub data_length: usize,
    /// 时间戳
    pub timestamp: Option<u64>,
}

/// 解析统计信息
#[derive(Debug, Clone, Default)]
pub struct ParseStats {
    /// 解析的数据块数量
    pub blocks_parsed: usize,
    /// 解析的字节数
    pub bytes_parsed: usize,
    /// 解析时间（微秒）
    pub parse_time_us: u64,
    /// 错误数量
    pub error_count: usize,
}

/// 分层数据解析器
pub struct LayerParser {
    config: LayerParserConfig,
    dbc_parser: Option<Arc<DbcParser>>,
}

impl LayerParser {
    /// 创建新的分层解析器
    pub fn new(config: LayerParserConfig) -> Result<Self> {
        let dbc_parser = if config.enable_monitoring {
            Some(Arc::new(DbcParser::new(crate::dbc_parser::DbcParserConfig::default())))
        } else {
            None
        };

        Ok(Self {
            config,
            dbc_parser,
        })
    }

    /// 设置DBC解析器
    pub fn with_dbc_parser(mut self, dbc_parser: Arc<DbcParser>) -> Self {
        self.dbc_parser = Some(dbc_parser);
        self
    }

    /// 解析第0层数据
    /// 
    /// ## 参数
    /// 
    /// - `mmap_block`: 文件映射块
    /// 
    /// ## 返回值
    /// 
    /// 返回解析结果，包含头部信息和压缩数据的指针和长度
    pub fn parse_layer_0(&self, mmap_block: &MmapBlock) -> Result<ParseResult> {
        let start_time = std::time::Instant::now();
        let mut stats = ParseStats::default();

        // 检查数据长度
        let data = mmap_block.as_slice();
        if data.len() < 35 {
            return Err(anyhow!("数据长度不足，需要至少35字节，实际: {}", data.len()));
        }

        // 解析35字节头部
        let header_data = &data[0..35];
        let header_block = DataBlock {
            ptr_and_len: (header_data.as_ptr() as usize, header_data.len()),
            block_type: BlockType::Header,
            metadata: Some(BlockMetadata {
                original_offset: 0,
                data_length: header_data.len(),
                timestamp: None,
            }),
        };

        // 解析压缩数据长度（大端序，后4字节）
        let compressed_length_bytes = &data[31..35];
        let compressed_length = u32::from_be_bytes([
            compressed_length_bytes[0],
            compressed_length_bytes[1],
            compressed_length_bytes[2],
            compressed_length_bytes[3],
        ]) as usize;

        // 检查压缩数据长度
        if 35 + compressed_length > data.len() {
            return Err(anyhow!(
                "压缩数据长度超出文件范围: 需要{}字节，实际可用{}字节",
                35 + compressed_length,
                data.len()
            ));
        }

        // 获取压缩数据
        let compressed_data = &data[35..35 + compressed_length];
        let compressed_block = DataBlock {
            ptr_and_len: (compressed_data.as_ptr() as usize, compressed_data.len()),
            block_type: BlockType::CompressedData,
            metadata: Some(BlockMetadata {
                original_offset: 35,
                data_length: compressed_data.len(),
                timestamp: None,
            }),
        };

        stats.blocks_parsed = 2;
        stats.bytes_parsed = data.len();
        stats.parse_time_us = start_time.elapsed().as_micros() as u64;

        info!("第0层解析完成: 头部{}字节, 压缩数据{}字节", 
              header_data.len(), compressed_data.len());

        Ok(ParseResult {
            layer_type: LayerType::Layer0,
            data_blocks: vec![header_block, compressed_block],
            stats,
        })
    }

    /// 解析第1层数据
    /// 
    /// ## 参数
    /// 
    /// - `data_ptr`: 数据指针
    /// - `data_len`: 数据长度
    /// 
    /// ## 返回值
    /// 
    /// 返回解析结果，包含多个数据块的指针和长度
    pub fn parse_layer_1(&self, data_ptr: usize, data_len: usize) -> Result<ParseResult> {
        let start_time = std::time::Instant::now();
        let mut stats = ParseStats::default();
        let mut data_blocks = Vec::new();
        let mut offset = 0;

        // 将指针转换为切片进行解析
        let data = unsafe { std::slice::from_raw_parts(data_ptr as *const u8, data_len) };

        while offset + 20 <= data.len() {
            // 解析20字节头部
            let header_data = &data[offset..offset + 20];
            let header_block = DataBlock {
                ptr_and_len: (header_data.as_ptr() as usize, header_data.len()),
                block_type: BlockType::Header,
                metadata: Some(BlockMetadata {
                    original_offset: offset,
                    data_length: header_data.len(),
                    timestamp: None,
                }),
            };
            data_blocks.push(header_block);

            // 解析数据长度（后4字节）
            let length_bytes = &data[offset + 16..offset + 20];
            let block_length = u32::from_be_bytes([
                length_bytes[0],
                length_bytes[1],
                length_bytes[2],
                length_bytes[3],
            ]) as usize;

            offset += 20;

            // 检查数据长度
            if offset + block_length > data.len() {
                warn!("数据块长度超出范围，跳过剩余数据");
                break;
            }

            // 获取数据块
            let block_data = &data[offset..offset + block_length];
            let data_block = DataBlock {
                ptr_and_len: (block_data.as_ptr() as usize, block_data.len()),
                block_type: BlockType::UncompressedData,
                metadata: Some(BlockMetadata {
                    original_offset: offset,
                    data_length: block_data.len(),
                    timestamp: None,
                }),
            };
            data_blocks.push(data_block);

            offset += block_length;
            stats.blocks_parsed += 2;
        }

        stats.bytes_parsed = data.len();
        stats.parse_time_us = start_time.elapsed().as_micros() as u64;

        info!("第1层解析完成: {}个数据块", stats.blocks_parsed / 2);

        Ok(ParseResult {
            layer_type: LayerType::Layer1,
            data_blocks,
            stats,
        })
    }

    /// 解析第2层数据（与第1层格式相同）
    pub fn parse_layer_2(&self, data_ptr: usize, data_len: usize) -> Result<ParseResult> {
        let mut result = self.parse_layer_1(data_ptr, data_len)?;
        result.layer_type = LayerType::Layer2;
        Ok(result)
    }

    /// 解析最后一层数据（单帧数据）
    /// 
    /// ## 参数
    /// 
    /// - `data_ptr`: 数据指针
    /// - `data_len`: 数据长度
    /// 
    /// ## 返回值
    /// 
    /// 返回解析结果，包含单帧数据
    pub fn parse_final_layer(&self, data_ptr: usize, data_len: usize) -> Result<ParseResult> {
        let start_time = std::time::Instant::now();
        let mut stats = ParseStats::default();

        // 将指针转换为切片
        let data = unsafe { std::slice::from_raw_parts(data_ptr as *const u8, data_len) };

        // 创建单帧数据块
        let frame_block = DataBlock {
            ptr_and_len: (data.as_ptr() as usize, data.len()),
            block_type: BlockType::FrameData,
            metadata: Some(BlockMetadata {
                original_offset: 0,
                data_length: data.len(),
                timestamp: None,
            }),
        };

        stats.blocks_parsed = 1;
        stats.bytes_parsed = data.len();
        stats.parse_time_us = start_time.elapsed().as_micros() as u64;

        info!("最后一层解析完成: 单帧数据{}字节", data.len());

        Ok(ParseResult {
            layer_type: LayerType::FinalLayer,
            data_blocks: vec![frame_block],
            stats,
        })
    }

    /// 批量解析第1层数据
    pub fn parse_layer_1_batch(&self, data_blocks: &[DataBlock]) -> Result<Vec<ParseResult>> {
        let mut results = Vec::new();

        for block in data_blocks {
            if block.block_type == BlockType::UncompressedData {
                let result = self.parse_layer_1(block.ptr_and_len.0, block.ptr_and_len.1)?;
                results.push(result);
            }
        }

        Ok(results)
    }

    /// 批量解析第2层数据
    pub fn parse_layer_2_batch(&self, data_blocks: &[DataBlock]) -> Result<Vec<ParseResult>> {
        let mut results = Vec::new();

        for block in data_blocks {
            if block.block_type == BlockType::UncompressedData {
                let result = self.parse_layer_2(block.ptr_and_len.0, block.ptr_and_len.1)?;
                results.push(result);
            }
        }

        Ok(results)
    }

    /// 批量解析最后一层数据
    pub fn parse_final_layer_batch(&self, data_blocks: &[DataBlock]) -> Result<Vec<ParseResult>> {
        let mut results = Vec::new();

        for block in data_blocks {
            if block.block_type == BlockType::UncompressedData {
                let result = self.parse_final_layer(block.ptr_and_len.0, block.ptr_and_len.1)?;
                results.push(result);
            }
        }

        Ok(results)
    }

    /// 使用DBC解析器解析单帧数据
    pub fn parse_frame_with_dbc(&self, data_ptr: usize, data_len: usize) -> Result<DbcParseResult> {
        if let Some(dbc_parser) = &self.dbc_parser {
            // 将指针转换为切片
            let _data = unsafe { std::slice::from_raw_parts(data_ptr as *const u8, data_len) };
            
            // 这里需要根据实际的帧格式来解析
            // 暂时返回一个示例结果，使用空的DBC内容
            let empty_dbc_content = r#"
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
    SG_MUL_VAL_

BS_:

BU_: Vector__XXX
"#;
            
            // 直接使用DBC解析器解析空内容
            dbc_parser.parse_content(empty_dbc_content)
        } else {
            Err(anyhow!("DBC解析器未配置"))
        }
    }

    /// 获取解析器配置
    pub fn config(&self) -> &LayerParserConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory_pool::MmapBlock;

    #[test]
    fn test_layer_parser_creation() {
        let config = LayerParserConfig::default();
        let parser = LayerParser::new(config).unwrap();
        assert_eq!(parser.config().batch_size, 100);
    }

    #[test]
    fn test_parse_layer_0() {
        let config = LayerParserConfig::default();
        let parser = LayerParser::new(config).unwrap();

        // 创建测试数据：35字节头部 + 4字节压缩数据长度 + 10字节压缩数据
        let mut test_data = vec![0u8; 49];
        
        // 设置压缩数据长度（大端序）：10字节
        test_data[31] = 0;
        test_data[32] = 0;
        test_data[33] = 0;
        test_data[34] = 10;
        
        // 设置压缩数据
        for i in 35..45 {
            test_data[i] = i as u8;
        }

        // 创建MmapBlock（这里简化处理，使用内存数据）
        use memmap2::Mmap;
        use std::io::Write;
        use tempfile::tempfile;
        
        // 创建临时文件并写入数据
        let mut temp_file = tempfile().unwrap();
        temp_file.write_all(&test_data).unwrap();
        temp_file.flush().unwrap();
        
        // 从文件创建mmap
        let mmap = unsafe { Mmap::map(&temp_file).unwrap() };
        let mmap_block = MmapBlock::new(mmap, Some("test_file".to_string()));
        
        // 解析第0层
        let result = parser.parse_layer_0(&mmap_block);
        assert!(result.is_ok());
        
        let parse_result = result.unwrap();
        assert_eq!(parse_result.layer_type, LayerType::Layer0);
        assert_eq!(parse_result.data_blocks.len(), 2);
        assert_eq!(parse_result.stats.blocks_parsed, 2);
    }

    #[test]
    fn test_parse_layer_1() {
        let config = LayerParserConfig::default();
        let parser = LayerParser::new(config).unwrap();

        // 创建测试数据：20字节头部 + 4字节长度 + 5字节数据
        let mut test_data = vec![0u8; 29];
        
        // 设置数据长度（大端序）：5字节
        test_data[16] = 0;
        test_data[17] = 0;
        test_data[18] = 0;
        test_data[19] = 5;
        
        // 设置数据
        for i in 20..25 {
            test_data[i] = i as u8;
        }

        let result = parser.parse_layer_1(test_data.as_ptr() as usize, test_data.len());
        assert!(result.is_ok());
        
        let parse_result = result.unwrap();
        assert_eq!(parse_result.layer_type, LayerType::Layer1);
        assert_eq!(parse_result.data_blocks.len(), 2);
        assert_eq!(parse_result.stats.blocks_parsed, 2);
    }

    #[test]
    fn test_parse_final_layer() {
        let config = LayerParserConfig::default();
        let parser = LayerParser::new(config).unwrap();

        // 创建测试数据
        let test_data = vec![1u8, 2, 3, 4, 5];

        let result = parser.parse_final_layer(test_data.as_ptr() as usize, test_data.len());
        assert!(result.is_ok());
        
        let parse_result = result.unwrap();
        assert_eq!(parse_result.layer_type, LayerType::FinalLayer);
        assert_eq!(parse_result.data_blocks.len(), 1);
        assert_eq!(parse_result.stats.blocks_parsed, 1);
    }
} 