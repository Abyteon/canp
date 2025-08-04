# 高级 Rust 模式学习指南

## 📚 概述

本文档介绍CANP项目中使用的高级Rust编程模式，包括智能指针、特征对象、泛型、宏等核心概念，帮助开发者掌握Rust的高级特性。

## 🏗️ 智能指针 (Smart Pointers)

### 1. Arc - 原子引用计数

#### 基本概念

`Arc` (Atomic Reference Counting) 允许多线程安全地共享数据。

```rust
use std::sync::Arc;
use std::sync::RwLock;

// 共享状态
pub struct SharedState {
    data: Arc<RwLock<HashMap<String, String>>>,
}

impl SharedState {
    pub fn new() -> Self {
        Self {
            data: Arc::new(RwLock::new(HashMap::new())),
        }
    }
    
    pub fn get_data(&self) -> Arc<RwLock<HashMap<String, String>>> {
        Arc::clone(&self.data) // 克隆Arc，增加引用计数
    }
}
```

#### 在CANP中的应用

```rust
// 内存池中的共享缓存
pub struct ZeroCopyMemoryPool {
    mmap_cache: Arc<RwLock<lru::LruCache<String, Arc<Mmap>>>>,
    current_memory_usage: Arc<RwLock<usize>>,
}

impl ZeroCopyMemoryPool {
    pub fn get_mmap(&self, path: &str) -> Option<Arc<Mmap>> {
        let cache = self.mmap_cache.read().unwrap();
        cache.peek(path).map(|mmap| Arc::clone(mmap))
    }
}
```

### 2. Box - 堆分配

#### 基本用法

```rust
// 递归数据结构
#[derive(Debug)]
enum List {
    Cons(i32, Box<List>),
    Nil,
}

// 特征对象
trait Processor {
    fn process(&self, data: &[u8]) -> Vec<u8>;
}

struct DataProcessor;
impl Processor for DataProcessor {
    fn process(&self, data: &[u8]) -> Vec<u8> {
        data.to_vec()
    }
}

// 使用Box存储特征对象
let processor: Box<dyn Processor> = Box::new(DataProcessor);
```

### 3. Rc - 引用计数

#### 单线程引用计数

```rust
use std::rc::Rc;

// 共享不可变数据
struct SharedConfig {
    settings: Rc<HashMap<String, String>>,
}

impl SharedConfig {
    pub fn new() -> Self {
        Self {
            settings: Rc::new(HashMap::new()),
        }
    }
    
    pub fn get_settings(&self) -> Rc<HashMap<String, String>> {
        Rc::clone(&self.settings)
    }
}
```

## 🔄 特征对象 (Trait Objects)

### 1. 动态分发

#### 基本概念

特征对象允许在运行时进行方法分发，提供多态性。

```rust
// 定义特征
trait DataHandler {
    fn handle(&self, data: &[u8]) -> Result<Vec<u8>, Box<dyn std::error::Error>>;
    fn name(&self) -> &str;
}

// 实现特征
struct JsonHandler;
impl DataHandler for JsonHandler {
    fn handle(&self, data: &[u8]) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        // JSON处理逻辑
        Ok(data.to_vec())
    }
    
    fn name(&self) -> &str {
        "json"
    }
}

struct BinaryHandler;
impl DataHandler for BinaryHandler {
    fn handle(&self, data: &[u8]) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        // 二进制处理逻辑
        Ok(data.to_vec())
    }
    
    fn name(&self) -> &str {
        "binary"
    }
}
```

#### 在CANP中的应用

```rust
// 处理器工厂
pub struct ProcessorFactory {
    handlers: HashMap<String, Box<dyn DataHandler + Send + Sync>>,
}

impl ProcessorFactory {
    pub fn new() -> Self {
        let mut handlers = HashMap::new();
        handlers.insert("json".to_string(), Box::new(JsonHandler));
        handlers.insert("binary".to_string(), Box::new(BinaryHandler));
        
        Self { handlers }
    }
    
    pub fn get_handler(&self, name: &str) -> Option<&Box<dyn DataHandler + Send + Sync>> {
        self.handlers.get(name)
    }
}
```

### 2. 对象安全

#### 对象安全规则

```rust
// 对象安全的特征
trait SafeTrait {
    fn method(&self) -> String; // 对象安全
}

// 非对象安全的特征
trait UnsafeTrait {
    fn method<T>(&self, value: T) -> T; // 泛型方法，非对象安全
}

// 使用对象安全特征
fn process_safe(handler: &dyn SafeTrait) {
    println!("{}", handler.method());
}
```

## 🧬 泛型 (Generics)

### 1. 泛型函数

#### 基本语法

```rust
// 泛型函数
fn find_max<T: PartialOrd>(items: &[T]) -> Option<&T> {
    items.iter().max()
}

// 使用
let numbers = vec![1, 2, 3, 4, 5];
let max_number = find_max(&numbers);

let strings = vec!["a", "b", "c"];
let max_string = find_max(&strings);
```

### 2. 泛型结构体

```rust
// 泛型结构体
struct DataContainer<T> {
    data: T,
    metadata: HashMap<String, String>,
}

impl<T> DataContainer<T> {
    fn new(data: T) -> Self {
        Self {
            data,
            metadata: HashMap::new(),
        }
    }
    
    fn get_data(&self) -> &T {
        &self.data
    }
}

// 为特定类型实现方法
impl DataContainer<String> {
    fn len(&self) -> usize {
        self.data.len()
    }
}
```

### 3. 在CANP中的应用

```rust
// 泛型解析器
pub struct Parser<T, U> {
    input_type: PhantomData<T>,
    output_type: PhantomData<U>,
}

impl<T, U> Parser<T, U> {
    pub fn new() -> Self {
        Self {
            input_type: PhantomData,
            output_type: PhantomData,
        }
    }
}

// 为特定类型组合实现解析
impl Parser<Vec<u8>, String> {
    pub fn parse_bytes_to_string(&self, data: &[u8]) -> Result<String, std::io::Error> {
        String::from_utf8(data.to_vec()).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }
}
```

## 🔧 宏 (Macros)

### 1. 声明宏 (Declarative Macros)

#### 基本语法

```rust
// 简单的声明宏
macro_rules! greet {
    ($name:expr) => {
        println!("Hello, {}!", $name);
    };
}

// 使用
greet!("World");

// 带多个模式的宏
macro_rules! create_struct {
    ($name:ident { $($field:ident: $type:ty),* }) => {
        struct $name {
            $($field: $type),*
        }
    };
}

// 使用
create_struct!(Person {
    name: String,
    age: u32,
});
```

### 2. 过程宏 (Procedural Macros)

#### 派生宏

```rust
// 自定义派生宏
#[proc_macro_derive(MyDebug)]
pub fn my_debug_derive(input: TokenStream) -> TokenStream {
    // 宏实现逻辑
    TokenStream::new()
}

// 使用
#[derive(MyDebug)]
struct MyStruct {
    field: String,
}
```

### 3. 在CANP中的应用

```rust
// 错误类型宏
macro_rules! define_error {
    ($name:ident) => {
        #[derive(Debug, thiserror::Error)]
        pub enum $name {
            #[error("IO error: {0}")]
            Io(#[from] std::io::Error),
            
            #[error("Parse error: {0}")]
            Parse(String),
            
            #[error("Invalid data: {0}")]
            InvalidData(String),
        }
    };
}

// 使用
define_error!(ProcessingError);
```

## 🔄 异步模式

### 1. 异步特征

```rust
use std::future::Future;
use std::pin::Pin;

// 异步特征
trait AsyncProcessor {
    type Output;
    type Error;
    
    fn process<'a>(
        &'a self,
        data: &'a [u8],
    ) -> Pin<Box<dyn Future<Output = Result<Self::Output, Self::Error>> + Send + 'a>>;
}

// 实现异步特征
struct AsyncDataProcessor;

impl AsyncProcessor for AsyncDataProcessor {
    type Output = Vec<u8>;
    type Error = std::io::Error;
    
    fn process<'a>(
        &'a self,
        data: &'a [u8],
    ) -> Pin<Box<dyn Future<Output = Result<Self::Output, Self::Error>> + Send + 'a>> {
        Box::pin(async move {
            // 异步处理逻辑
            Ok(data.to_vec())
        })
    }
}
```

### 2. 异步流

```rust
use futures::stream::{self, StreamExt};

// 异步流处理
async fn process_stream<S>(mut stream: S) -> Vec<Vec<u8>>
where
    S: Stream<Item = Vec<u8>> + Unpin,
{
    let mut results = Vec::new();
    
    while let Some(data) = stream.next().await {
        // 处理数据
        results.push(data);
    }
    
    results
}

// 创建异步流
let stream = stream::iter(vec![
    vec![1, 2, 3],
    vec![4, 5, 6],
    vec![7, 8, 9],
]);
```

## 🎯 设计模式

### 1. 建造者模式

```rust
// 建造者模式
pub struct ConfigBuilder {
    max_memory: Option<usize>,
    worker_threads: Option<usize>,
    batch_size: Option<usize>,
}

impl ConfigBuilder {
    pub fn new() -> Self {
        Self {
            max_memory: None,
            worker_threads: None,
            batch_size: None,
        }
    }
    
    pub fn max_memory(mut self, memory: usize) -> Self {
        self.max_memory = Some(memory);
        self
    }
    
    pub fn worker_threads(mut self, threads: usize) -> Self {
        self.worker_threads = Some(threads);
        self
    }
    
    pub fn batch_size(mut self, size: usize) -> Self {
        self.batch_size = Some(size);
        self
    }
    
    pub fn build(self) -> Result<Config, String> {
        Ok(Config {
            max_memory: self.max_memory.unwrap_or(1024 * 1024 * 1024),
            worker_threads: self.worker_threads.unwrap_or(num_cpus::get()),
            batch_size: self.batch_size.unwrap_or(1000),
        })
    }
}

pub struct Config {
    max_memory: usize,
    worker_threads: usize,
    batch_size: usize,
}

// 使用
let config = ConfigBuilder::new()
    .max_memory(2 * 1024 * 1024 * 1024)
    .worker_threads(8)
    .batch_size(5000)
    .build()?;
```

### 2. 策略模式

```rust
// 策略模式
trait CompressionStrategy {
    fn compress(&self, data: &[u8]) -> Result<Vec<u8>, Box<dyn std::error::Error>>;
    fn decompress(&self, data: &[u8]) -> Result<Vec<u8>, Box<dyn std::error::Error>>;
}

struct GzipStrategy;
impl CompressionStrategy for GzipStrategy {
    fn compress(&self, data: &[u8]) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        use flate2::write::GzEncoder;
        use flate2::Compression;
        use std::io::Write;
        
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(data)?;
        Ok(encoder.finish()?)
    }
    
    fn decompress(&self, data: &[u8]) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        use flate2::read::GzDecoder;
        use std::io::Read;
        
        let mut decoder = GzDecoder::new(data);
        let mut result = Vec::new();
        decoder.read_to_end(&mut result)?;
        Ok(result)
    }
}

struct Compressor {
    strategy: Box<dyn CompressionStrategy>,
}

impl Compressor {
    pub fn new(strategy: Box<dyn CompressionStrategy>) -> Self {
        Self { strategy }
    }
    
    pub fn compress(&self, data: &[u8]) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        self.strategy.compress(data)
    }
    
    pub fn decompress(&self, data: &[u8]) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        self.strategy.decompress(data)
    }
}
```

### 3. 观察者模式

```rust
use std::sync::Arc;
use tokio::sync::broadcast;

// 观察者模式
pub struct EventPublisher {
    tx: broadcast::Sender<Event>,
}

#[derive(Debug, Clone)]
pub enum Event {
    DataProcessed { bytes: usize },
    ErrorOccurred { message: String },
    MemoryUsage { usage: usize },
}

impl EventPublisher {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(100);
        Self { tx }
    }
    
    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.tx.subscribe()
    }
    
    pub fn publish(&self, event: Event) -> Result<(), broadcast::error::SendError<Event>> {
        self.tx.send(event)
    }
}

// 使用
let publisher = Arc::new(EventPublisher::new());
let mut subscriber = publisher.subscribe();

// 发布事件
publisher.publish(Event::DataProcessed { bytes: 1024 })?;

// 接收事件
if let Ok(event) = subscriber.recv().await {
    println!("收到事件: {:?}", event);
}
```

## 🔧 最佳实践

### 1. 错误处理

```rust
// 使用 thiserror 定义错误类型
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ProcessingError {
    #[error("IO错误: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("解析错误: {0}")]
    Parse(String),
    
    #[error("内存不足: 需要 {needed}, 可用 {available}")]
    InsufficientMemory { needed: usize, available: usize },
}

// 使用 anyhow 进行错误传播
use anyhow::{Context, Result};

fn process_data(data: &[u8]) -> Result<Vec<u8>> {
    let result = parse_data(data)
        .context("解析数据失败")?;
    
    Ok(result)
}
```

### 2. 性能优化

```rust
// 使用 const 函数
const fn calculate_buffer_size(data_size: usize) -> usize {
    data_size * 2
}

// 使用 #[inline] 内联函数
#[inline]
fn fast_hash(data: &[u8]) -> u64 {
    // 快速哈希实现
    0
}

// 使用 #[cold] 标记冷路径
#[cold]
fn handle_error(error: &str) {
    eprintln!("错误: {}", error);
}
```

### 3. 内存安全

```rust
// 使用 Pin 固定数据
use std::pin::Pin;

struct AsyncProcessor {
    data: Pin<Box<Vec<u8>>>,
}

impl AsyncProcessor {
    pub fn new() -> Self {
        Self {
            data: Box::pin(Vec::new()),
        }
    }
    
    pub fn get_data_mut(self: Pin<&mut Self>) -> Pin<&mut Vec<u8>> {
        unsafe { self.map_unchecked_mut(|s| &mut *s.data) }
    }
}
```

## 📚 总结

高级Rust模式为CANP项目提供了强大的抽象能力和性能优化手段。通过合理使用智能指针、特征对象、泛型和宏，我们可以构建出高性能、类型安全、易于维护的系统。

关键要点：
- 使用 `Arc` 进行多线程安全的数据共享
- 使用特征对象实现运行时多态
- 使用泛型提高代码复用性
- 使用宏减少重复代码
- 遵循Rust的设计模式和最佳实践 