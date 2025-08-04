//! # 测试数据生成器
//! 
//! 为数据处理任务生成符合4层数据结构的测试文件：
//! 1. 35字节头部 + 压缩数据（~10KB）
//! 2. 解压后：20字节头部 + 帧序列数据
//! 3. 16字节长度信息 + 帧序列（多个单帧）
//! 4. 单帧数据（DBC格式）

use anyhow::Result;
use flate2::write::GzEncoder;
use flate2::Compression;
use std::fs::{create_dir_all, File};
use std::io::{Write, BufWriter};
use std::path::{Path, PathBuf};
// use std::collections::HashMap;  // 暂时不需要
use tracing::{info, debug};

/// 测试数据生成器配置
#[derive(Debug, Clone)]
pub struct TestDataConfig {
    /// 生成文件数量
    pub file_count: usize,
    /// 目标文件大小（字节）
    pub target_file_size: usize,
    /// 每个文件的帧数量
    pub frames_per_file: usize,
    /// 输出目录
    pub output_dir: PathBuf,
}

impl Default for TestDataConfig {
    fn default() -> Self {
        Self {
            file_count: 20,  // 生成20个测试文件（实际是8000个）
            target_file_size: 15 * 1024 * 1024,  // 15MB
            frames_per_file: 2000,  // 增加到2000帧，确保压缩后达到~10KB
            output_dir: PathBuf::from("test_data"),
        }
    }
}

/// 真实的CAN帧数据结构
#[derive(Debug, Clone)]
pub struct CanFrame {
    /// CAN ID (标准帧11位或扩展帧29位)
    pub id: u32,
    /// 数据长度 (0-8字节)
    pub dlc: u8,
    /// 数据内容 (最多8字节)
    pub data: Vec<u8>,
    /// 时间戳（微秒）
    pub timestamp: u64,
    /// 帧类型 (标准/扩展)
    pub frame_type: CanFrameType,
    /// 远程帧标志
    pub is_remote: bool,
}

/// CAN帧类型
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CanFrameType {
    Standard,  // 标准帧 (11位ID)
    Extended,  // 扩展帧 (29位ID)
}

impl CanFrame {
    /// 生成真实的车辆CAN帧数据
    pub fn generate_realistic_vehicle_frame(timestamp: u64, frame_type: CanFrameType) -> Self {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        
        let mut hasher = DefaultHasher::new();
        timestamp.hash(&mut hasher);
        let seed = hasher.finish();
        
        // 真实的车辆CAN ID (基于实际车辆系统)
        let standard_ids = [
            0x100,  // 发动机数据
            0x200,  // 变速箱数据
            0x300,  // 制动系统
            0x400,  // 转向系统
            0x500,  // 车身控制
            0x600,  // 仪表盘
            0x700,  // 空调系统
            0x800,  // 安全系统
        ];
        
        let extended_ids = [
            0x18FF1234,  // 发动机详细数据
            0x18FF5678,  // 变速箱详细数据
            0x18FF9ABC,  // 制动系统详细数据
            0x18FFDEF0,  // 转向系统详细数据
        ];
        
        let can_id = match frame_type {
            CanFrameType::Standard => {
                standard_ids[(seed % standard_ids.len() as u64) as usize]
            },
            CanFrameType::Extended => {
                extended_ids[(seed % extended_ids.len() as u64) as usize]
            }
        };
        
        // 根据CAN ID生成相应的数据
        let (dlc, data) = Self::generate_realistic_data(can_id, seed);
        
        Self {
            id: can_id,
            dlc,
            data,
            timestamp,
            frame_type,
            is_remote: false, // 数据帧
        }
    }
    
    /// 根据CAN ID生成真实的车辆数据
    fn generate_realistic_data(can_id: u32, seed: u64) -> (u8, Vec<u8>) {
        match can_id {
            // 发动机数据 (0x100) - 8字节
            0x100 => {
                let engine_speed = ((seed % 8000) + 800) as u16; // 800-8800 RPM
                let engine_temp = ((seed % 100) + 80) as u8;     // 80-180°C
                let fuel_level = ((seed % 100) + 1) as u8;       // 1-100%
                let oil_pressure = ((seed % 5) + 1) as u8;       // 1-6 bar
                let throttle_pos = (seed % 100) as u8;           // 0-100%
                let load = (seed % 100) as u8;                   // 0-100%
                let rpm_high = (engine_speed >> 8) as u8;
                let rpm_low = (engine_speed & 0xFF) as u8;
                
                (8, vec![rpm_low, rpm_high, engine_temp, fuel_level, oil_pressure, throttle_pos, load, 0])
            },
            
            // 变速箱数据 (0x200) - 6字节
            0x200 => {
                let gear = ((seed % 8) + 1) as u8;               // 1-8档
                let gear_ratio = ((seed % 100) + 50) as u8;      // 50-150%
                let clutch_status = (seed % 2) as u8;            // 0-1
                let transmission_temp = ((seed % 50) + 80) as u8; // 80-130°C
                let fluid_level = ((seed % 20) + 80) as u8;      // 80-100%
                let shift_status = (seed % 4) as u8;             // 0-3
                
                (6, vec![gear, gear_ratio, clutch_status, transmission_temp, fluid_level, shift_status])
            },
            
            // 制动系统 (0x300) - 7字节
            0x300 => {
                let brake_pressure = ((seed % 200) + 50) as u16; // 50-250 bar
                let brake_temp = ((seed % 100) + 50) as u8;      // 50-150°C
                let abs_status = (seed % 2) as u8;               // 0-1
                let brake_fluid = ((seed % 20) + 80) as u8;      // 80-100%
                let brake_wear = (seed % 100) as u8;             // 0-100%
                let brake_force = ((seed % 100) + 1) as u8;      // 1-100%
                let emergency_brake = (seed % 2) as u8;          // 0-1
                
                (7, vec![(brake_pressure & 0xFF) as u8, (brake_pressure >> 8) as u8, brake_temp, abs_status, brake_fluid, brake_wear, brake_force])
            },
            
            // 转向系统 (0x400) - 5字节
            0x400 => {
                let steering_angle = ((seed % 720) as i32 - 360) as i16; // -360到+360度
                let steering_speed = ((seed % 200) + 1) as u8;    // 1-200 deg/s
                let power_steering = ((seed % 100) + 1) as u8;    // 1-100%
                let steering_torque = ((seed % 50) + 1) as u8;    // 1-50 Nm
                let _steering_status = (seed % 4) as u8;           // 0-3
                
                (5, vec![(steering_angle & 0xFF) as u8, (steering_angle >> 8) as u8, steering_speed, power_steering, steering_torque])
            },
            
            // 车身控制 (0x500) - 8字节
            0x500 => {
                let door_status = (seed % 16) as u8;              // 4个门的开关状态
                let window_position = (seed % 100) as u8;         // 0-100%
                let seat_position = (seed % 100) as u8;           // 0-100%
                let mirror_position = (seed % 100) as u8;         // 0-100%
                let light_status = (seed % 8) as u8;              // 各种灯光状态
                let wiper_status = (seed % 4) as u8;              // 雨刷状态
                let climate_control = (seed % 100) as u8;         // 空调设置
                let security_status = (seed % 4) as u8;           // 安全系统状态
                
                (8, vec![door_status, window_position, seat_position, mirror_position, light_status, wiper_status, climate_control, security_status])
            },
            
            // 仪表盘 (0x600) - 6字节
            0x600 => {
                let vehicle_speed = ((seed % 200) + 1) as u8;     // 1-200 km/h
                let fuel_gauge = (seed % 100) as u8;              // 0-100%
                let temp_gauge = ((seed % 50) + 70) as u8;        // 70-120°C
                let oil_gauge = (seed % 100) as u8;               // 0-100%
                let battery_voltage = ((seed % 5) + 12) as u8;    // 12-17V
                let warning_lights = (seed % 16) as u8;           // 警告灯状态
                
                (6, vec![vehicle_speed, fuel_gauge, temp_gauge, oil_gauge, battery_voltage, warning_lights])
            },
            
            // 空调系统 (0x700) - 4字节
            0x700 => {
                let cabin_temp = ((seed % 30) + 15) as u8;        // 15-45°C
                let set_temp = ((seed % 10) + 20) as u8;          // 20-30°C
                let fan_speed = ((seed % 8) + 1) as u8;           // 1-8档
                let ac_mode = (seed % 4) as u8;                   // 0-3模式
                
                (4, vec![cabin_temp, set_temp, fan_speed, ac_mode])
            },
            
            // 安全系统 (0x800) - 5字节
            0x800 => {
                let airbag_status = (seed % 2) as u8;             // 0-1
                let seatbelt_status = (seed % 2) as u8;           // 0-1
                let crash_sensor = (seed % 2) as u8;              // 0-1
                let stability_control = (seed % 2) as u8;         // 0-1
                let traction_control = (seed % 2) as u8;          // 0-1
                
                (5, vec![airbag_status, seatbelt_status, crash_sensor, stability_control, traction_control])
            },
            
            // 扩展帧数据 (更详细的信息)
            0x18FF1234 | 0x18FF5678 | 0x18FF9ABC | 0x18FFDEF0 => {
                let detailed_data = (0..8).map(|i| {
                    ((seed >> (i * 8)) & 0xFF) as u8
                }).collect();
                (8, detailed_data)
            },
            
            // 默认情况
            _ => {
                let dlc = ((seed >> 8) % 9) as u8;
                let data = (0..dlc).map(|i| {
                    ((seed >> (i * 8)) & 0xFF) as u8
                }).collect();
                (dlc, data)
            }
        }
    }
    
    /// 序列化为字节
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(16);
        
        // CAN ID (4字节, 大端序)
        bytes.extend_from_slice(&self.id.to_be_bytes());
        
        // DLC (1字节)
        bytes.push(self.dlc);
        
        // 保留字节 (3字节)
        bytes.extend_from_slice(&[0u8; 3]);
        
        // 数据 (8字节，不足的用0填充)
        bytes.extend_from_slice(&self.data);
        while bytes.len() < 16 {
            bytes.push(0);
        }
        
        bytes
    }
}

/// 测试数据生成器
pub struct TestDataGenerator {
    config: TestDataConfig,
}

impl TestDataGenerator {
    /// 创建新的生成器
    pub fn new(config: TestDataConfig) -> Self {
        Self { config }
    }
    
    /// 生成所有测试文件
    pub async fn generate_all(&self) -> Result<Vec<PathBuf>> {
        info!("🚀 开始生成测试数据...");
        info!("📁 输出目录: {:?}", self.config.output_dir);
        info!("📊 文件数量: {}", self.config.file_count);
        info!("📏 目标大小: {} MB", self.config.target_file_size / 1024 / 1024);
        
        // 创建输出目录
        create_dir_all(&self.config.output_dir)?;
        
        let mut file_paths = Vec::new();
        
        for i in 0..self.config.file_count {
            let file_path = self.config.output_dir.join(format!("test_data_{:04}.bin", i));
            self.generate_single_file(&file_path, i).await?;
            file_paths.push(file_path);
            
            if i % 10 == 0 {
                info!("✅ 已生成 {}/{} 个文件", i + 1, self.config.file_count);
            }
        }
        
        info!("🎉 测试数据生成完成！");
        Ok(file_paths)
    }
    
    /// 生成单个测试文件
    async fn generate_single_file(&self, file_path: &Path, file_index: usize) -> Result<()> {
        debug!("📝 生成文件: {:?}", file_path);
        
        // 生成原始帧数据
        let raw_frame_data = self.generate_frame_sequences(file_index)?;
        
        // 压缩数据
        let compressed_data = self.compress_data(&raw_frame_data)?;
        
        let file = File::create(file_path)?;
        let mut writer = BufWriter::new(file);
        
        // 第1层：35字节头部（包含真实的压缩数据大小）
        let header = self.generate_file_header(file_index, compressed_data.len());
        writer.write_all(&header)?;
        
        // 写入压缩数据
        writer.write_all(&compressed_data)?;
        
        // 填充到目标大小
        let current_size = 35 + compressed_data.len();
        if current_size < self.config.target_file_size {
            let padding_size = self.config.target_file_size - current_size;
            let padding = vec![0u8; padding_size];
            writer.write_all(&padding)?;
        }
        
        writer.flush()?;
        debug!("✅ 文件生成完成: {:?} ({} bytes)", file_path, self.config.target_file_size);
        
        Ok(())
    }
    
    /// 生成文件头部（35字节）
    /// 根据任务要求：35字节头部，后四个字节（位置31-34）为压缩数据长度
    fn generate_file_header(&self, file_index: usize, compressed_data_size: usize) -> Vec<u8> {
        let mut header = Vec::with_capacity(35);
        
        // 文件标识 (8字节) - 位置0-7
        header.extend_from_slice(b"CANDATA\0");
        
        // 版本号 (4字节) - 位置8-11
        header.extend_from_slice(&1u32.to_be_bytes());
        
        // 文件索引 (4字节) - 位置12-15
        header.extend_from_slice(&(file_index as u32).to_be_bytes());
        
        // 时间戳 (8字节) - 位置16-23
        let timestamp = 1640995200u64 + file_index as u64 * 3600; // 2022年开始，每小时一个文件
        header.extend_from_slice(&timestamp.to_be_bytes());
        
        // CRC32校验 (4字节) - 位置24-27
        header.extend_from_slice(&[0u8; 4]);
        
        // 保留字节 (3字节) - 位置28-30
        header.extend_from_slice(&[0u8; 3]);
        
        // 【关键】后续压缩数据长度 (4字节) - 位置31-34（任务要求的"后四个字节"）
        header.extend_from_slice(&(compressed_data_size as u32).to_be_bytes());
        
        header
    }
    
    /// 生成真实的车辆CAN帧序列数据（第2-4层结构）
    fn generate_frame_sequences(&self, file_index: usize) -> Result<Vec<u8>> {
        let mut data = Vec::new();
        
        // 第2层：20字节头部
        let layer2_header = self.generate_layer2_header(file_index);
        data.extend_from_slice(&layer2_header);
        
        // 生成多个帧序列，模拟真实的车辆数据流
        let sequences_count = 20; // 20个序列，每个序列代表不同的ECU数据
        let frames_per_sequence = self.config.frames_per_file / sequences_count;
        
        for seq_idx in 0..sequences_count {
            // 第3层：16字节长度信息
            let sequence_data = self.generate_realistic_frame_sequence(file_index, seq_idx, frames_per_sequence)?;
            
            let mut length_info = Vec::with_capacity(16);
            // 序列ID (4字节) - 代表不同的ECU
            length_info.extend_from_slice(&(seq_idx as u32).to_be_bytes());
            // 时间戳 (8字节) - 真实的时间戳
            let seq_timestamp = 1640995200u64 + file_index as u64 * 3600 + seq_idx as u64 * 360;
            length_info.extend_from_slice(&seq_timestamp.to_be_bytes());
            // 后续数据长度 (4字节) - 12到15字节位置
            length_info.extend_from_slice(&(sequence_data.len() as u32).to_be_bytes());
            
            data.extend_from_slice(&length_info);
            data.extend_from_slice(&sequence_data);
        }
        
        Ok(data)
    }
    
    /// 生成真实的车辆帧序列
    fn generate_realistic_frame_sequence(&self, file_index: usize, seq_idx: usize, frame_count: usize) -> Result<Vec<u8>> {
        let mut data = Vec::new();
        let base_timestamp = (file_index as u64 * 1_000_000) + (seq_idx as u64 * 100_000);
        
        // 根据序列ID确定ECU类型，生成相应的CAN帧
        let ecu_type = match seq_idx % 8 {
            0 => "Engine",      // 发动机ECU
            1 => "Transmission", // 变速箱ECU
            2 => "Brake",       // 制动ECU
            3 => "Steering",    // 转向ECU
            4 => "Body",        // 车身ECU
            5 => "Dashboard",   // 仪表盘ECU
            6 => "Climate",     // 空调ECU
            _ => "Safety",      // 安全ECU
        };
        
        // 生成真实的CAN帧
        for frame_idx in 0..frame_count {
            let timestamp = base_timestamp + (frame_idx as u64 * 10_000); // 10ms间隔，符合CAN总线频率
            
            // 根据ECU类型生成相应的CAN帧
            let can_frame = self.generate_ecu_specific_frame(ecu_type, timestamp, frame_idx);
            let frame_data = can_frame.to_bytes();
            data.extend_from_slice(&frame_data);
        }
        
        Ok(data)
    }
    
    /// 根据ECU类型生成特定的CAN帧
    fn generate_ecu_specific_frame(&self, ecu_type: &str, timestamp: u64, frame_idx: usize) -> CanFrame {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        
        let mut hasher = DefaultHasher::new();
        (ecu_type, timestamp, frame_idx).hash(&mut hasher);
        let seed = hasher.finish();
        
        match ecu_type {
            "Engine" => {
                // 发动机数据：转速、温度、燃油等
                let frame_type = if frame_idx % 3 == 0 { CanFrameType::Extended } else { CanFrameType::Standard };
                CanFrame::generate_realistic_vehicle_frame(timestamp, frame_type)
            },
            "Transmission" => {
                // 变速箱数据：档位、离合器状态等
                let frame_type = if frame_idx % 2 == 0 { CanFrameType::Standard } else { CanFrameType::Extended };
                CanFrame::generate_realistic_vehicle_frame(timestamp, frame_type)
            },
            "Brake" => {
                // 制动系统数据：制动压力、ABS状态等
                CanFrame::generate_realistic_vehicle_frame(timestamp, CanFrameType::Standard)
            },
            "Steering" => {
                // 转向系统数据：转向角度、助力等
                CanFrame::generate_realistic_vehicle_frame(timestamp, CanFrameType::Standard)
            },
            "Body" => {
                // 车身控制数据：车门、车窗、灯光等
                CanFrame::generate_realistic_vehicle_frame(timestamp, CanFrameType::Standard)
            },
            "Dashboard" => {
                // 仪表盘数据：车速、油量、警告等
                CanFrame::generate_realistic_vehicle_frame(timestamp, CanFrameType::Standard)
            },
            "Climate" => {
                // 空调系统数据：温度、风速等
                CanFrame::generate_realistic_vehicle_frame(timestamp, CanFrameType::Standard)
            },
            "Safety" => {
                // 安全系统数据：气囊、安全带等
                CanFrame::generate_realistic_vehicle_frame(timestamp, CanFrameType::Extended)
            },
            _ => {
                // 默认生成标准帧
                CanFrame::generate_realistic_vehicle_frame(timestamp, CanFrameType::Standard)
            }
        }
    }
    
    /// 生成第2层头部（20字节）
    fn generate_layer2_header(&self, file_index: usize) -> Vec<u8> {
        let mut header = Vec::with_capacity(20);
        
        // 数据类型标识 (4字节)
        header.extend_from_slice(b"FRAM");
        
        // 版本号 (4字节)
        header.extend_from_slice(&2u32.to_be_bytes());
        
        // 总帧数 (4字节)
        header.extend_from_slice(&(self.config.frames_per_file as u32).to_be_bytes());
        
        // 文件索引 (4字节)
        header.extend_from_slice(&(file_index as u32).to_be_bytes());
        
        // 后续数据长度 (4字节) - 后四个字节
        let data_length = self.config.frames_per_file * 32; // 预估
        header.extend_from_slice(&(data_length as u32).to_be_bytes());
        
        header
    }
    
    /// 生成单个帧序列（第4层：多个单帧）
    fn generate_single_frame_sequence(&self, file_index: usize, seq_idx: usize, frame_count: usize) -> Result<Vec<u8>> {
        let mut sequence = Vec::new();
        
        let base_timestamp = 1640995200u64 + file_index as u64 * 3600 + seq_idx as u64 * 360;
        
        for frame_idx in 0..frame_count {
            let timestamp = base_timestamp + frame_idx as u64;
            let frame_type = if frame_idx % 2 == 0 { CanFrameType::Standard } else { CanFrameType::Extended };
            let can_frame = CanFrame::generate_realistic_vehicle_frame(timestamp, frame_type);
            
            // 帧头（8字节）
            sequence.extend_from_slice(&timestamp.to_be_bytes()); // 时间戳
            
            // CAN帧数据（16字节）
            sequence.extend_from_slice(&can_frame.to_bytes());
        }
        
        Ok(sequence)
    }
    
    /// 压缩数据
    fn compress_data(&self, data: &[u8]) -> Result<Vec<u8>> {
        use flate2::write::GzEncoder;
        use flate2::Compression;
        use std::io::Write;
        
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(data)?;
        let compressed = encoder.finish()?;
        
        debug!("🗜️ 压缩完成: {} -> {} bytes (压缩比: {:.1}%)", 
            data.len(), compressed.len(), 
            (compressed.len() as f64 / data.len() as f64) * 100.0);
        
        Ok(compressed)
    }
}

/// 数据验证工具
pub struct TestDataValidator;

impl TestDataValidator {
    /// 验证生成的文件结构
    pub fn validate_file(file_path: &Path) -> Result<bool> {
        use std::io::Read;
        
        let mut file = File::open(file_path)?;
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer)?;
        
        if buffer.len() < 35 {
            return Ok(false);
        }
        
        // 验证文件头
        let header = &buffer[0..35];
        if &header[0..8] != b"CANDATA\0" {
            return Ok(false);
        }
        
        // 提取压缩数据长度（位置31-34，任务要求的"后四个字节"）
        let compressed_length = u32::from_be_bytes([
            header[31], header[32], header[33], header[34]
        ]) as usize;
        
        debug!("📊 文件验证: {:?} - 压缩数据长度: {} bytes", file_path, compressed_length);
        
        // 验证数据完整性
        if buffer.len() < 35 + compressed_length {
            return Ok(false);
        }
        
        Ok(true)
    }
    
    /// 统计生成的测试数据
    pub fn analyze_test_data(data_dir: &Path) -> Result<TestDataStats> {
        let mut stats = TestDataStats::default();
        
        for entry in std::fs::read_dir(data_dir)? {
            let entry = entry?;
            let path = entry.path();
            
            if path.is_file() && path.extension().map_or(false, |ext| ext == "bin") {
                if Self::validate_file(&path)? {
                    stats.valid_files += 1;
                    stats.total_size += entry.metadata()?.len();
                } else {
                    stats.invalid_files += 1;
                }
            }
        }
        
        Ok(stats)
    }
}

/// 测试数据统计
#[derive(Debug, Default)]
pub struct TestDataStats {
    pub valid_files: usize,
    pub invalid_files: usize,
    pub total_size: u64,
}

impl TestDataStats {
    pub fn print_summary(&self) {
        info!("📊 测试数据统计:");
        info!("  ✅ 有效文件: {}", self.valid_files);
        info!("  ❌ 无效文件: {}", self.invalid_files);
        info!("  💾 总大小: {:.2} MB", self.total_size as f64 / 1024.0 / 1024.0);
        info!("  📈 平均文件大小: {:.2} MB", 
            if self.valid_files > 0 {
                self.total_size as f64 / self.valid_files as f64 / 1024.0 / 1024.0
            } else {
                0.0
            }
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    
    #[tokio::test]
    async fn test_data_generation() {
        let temp_dir = TempDir::new().unwrap();
        
        let config = TestDataConfig {
            file_count: 2,
            target_file_size: 1024 * 1024, // 1MB测试
            frames_per_file: 100,
            output_dir: temp_dir.path().to_path_buf(),
        };
        
        let generator = TestDataGenerator::new(config);
        let file_paths = generator.generate_all().await.unwrap();
        
        assert_eq!(file_paths.len(), 2);
        
        for path in &file_paths {
            assert!(path.exists());
            assert!(TestDataValidator::validate_file(path).unwrap());
        }
        
        let stats = TestDataValidator::analyze_test_data(temp_dir.path()).unwrap();
        assert_eq!(stats.valid_files, 2);
        assert_eq!(stats.invalid_files, 0);
    }
    
    #[test]
    fn test_can_frame_generation() {
        let frame = CanFrame::generate_realistic_vehicle_frame(12345, CanFrameType::Standard);
        assert!(frame.id > 0);
        assert!(frame.dlc <= 8);
        assert_eq!(frame.data.len(), frame.dlc as usize);
        assert!(!frame.is_remote);
        
        let bytes = frame.to_bytes();
        assert_eq!(bytes.len(), 16);
        
        // 测试扩展帧
        let extended_frame = CanFrame::generate_realistic_vehicle_frame(12345, CanFrameType::Extended);
        assert!(extended_frame.id > 0x7FF); // 扩展帧ID大于11位
        assert_eq!(extended_frame.frame_type, CanFrameType::Extended);
    }
}