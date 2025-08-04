# Tokio 异步编程学习指南

## 📚 概述

Tokio是Rust生态系统中最重要的异步运行时库，为CANP项目提供了高性能的异步IO处理能力。本文档详细介绍Tokio的核心概念、使用方法和最佳实践。

## 🏗️ 核心概念

### 1. 异步编程基础

#### 什么是异步编程

异步编程允许程序在等待IO操作完成时执行其他任务，而不是阻塞等待。

```rust
// 同步版本 - 阻塞等待
fn sync_read_file() -> String {
    std::fs::read_to_string("file.txt").unwrap() // 阻塞
}

// 异步版本 - 非阻塞
async fn async_read_file() -> String {
    tokio::fs::read_to_string("file.txt").await.unwrap() // 非阻塞
}
```

#### Future 特征

```rust
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

// 自定义 Future
struct MyFuture {
    value: Option<String>,
}

impl Future for MyFuture {
    type Output = String;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if let Some(value) = self.value.take() {
            Poll::Ready(value)
        } else {
            Poll::Pending
        }
    }
}
```

### 2. Tokio 运行时

#### 创建运行时

```rust
// 基本运行时
let rt = tokio::runtime::Runtime::new().unwrap();

// 多线程运行时
let rt = tokio::runtime::Builder::new_multi_thread()
    .worker_threads(4)
    .enable_all()
    .build()
    .unwrap();

// 单线程运行时
let rt = tokio::runtime::Builder::new_current_thread()
    .enable_all()
    .build()
    .unwrap();
```

#### 在CANP中的应用

```rust
// 高性能执行器中的Tokio运行时
pub struct HighPerformanceExecutor {
    runtime: Arc<Runtime>,
    io_task_tx: mpsc::UnboundedSender<(TaskMetadata, BoxedTask)>,
    // ... 其他字段
}

impl HighPerformanceExecutor {
    pub fn new(config: ExecutorConfig) -> Self {
        let runtime = Arc::new(
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(config.io_worker_threads)
                .enable_all()
                .build()
                .unwrap()
        );
        
        // ... 初始化其他字段
        Self { runtime, io_task_tx, /* ... */ }
    }
}
```

## 🔄 异步任务

### 1. spawn 和 spawn_blocking

#### spawn - 异步任务

```rust
use tokio;

#[tokio::main]
async fn main() {
    // 生成异步任务
    let handle = tokio::spawn(async {
        // 异步工作
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
        "异步任务完成"
    });

    // 等待任务完成
    let result = handle.await.unwrap();
    println!("{}", result);
}
```

#### spawn_blocking - 阻塞任务

```rust
use tokio;

#[tokio::main]
async fn main() {
    // 生成阻塞任务（在专用线程池中运行）
    let handle = tokio::task::spawn_blocking(|| {
        // CPU密集型或阻塞操作
        std::thread::sleep(std::time::Duration::from_secs(1));
        "阻塞任务完成"
    });

    let result = handle.await.unwrap();
    println!("{}", result);
}
```

### 2. 在CANP中的应用

```rust
// IO密集型任务处理
impl HighPerformanceExecutor {
    pub fn submit_io_task<F>(&self, priority: Priority, task: F) -> Result<()>
    where
        F: FnOnce() -> Pin<Box<dyn Future<Output = Result<()>> + Send>> + Send + 'static,
    {
        let metadata = TaskMetadata::new(TaskType::IoIntensive, priority);
        let boxed_task = Box::pin(task());
        
        self.io_task_tx.send((metadata, boxed_task))?;
        Ok(())
    }
}

// 使用示例
executor.submit_io_task(Priority::Normal, || async {
    // 文件读取任务
    let content = tokio::fs::read_to_string("data.bin").await?;
    // 处理内容
    Ok(())
})?;
```

## 📡 通道 (Channels)

### 1. 基本通道类型

#### mpsc - 多生产者单消费者

```rust
use tokio::sync::mpsc;

#[tokio::main]
async fn main() {
    let (tx, mut rx) = mpsc::channel(100); // 缓冲区大小

    // 生产者
    let producer = tokio::spawn(async move {
        for i in 0..10 {
            tx.send(i).await.unwrap();
        }
    });

    // 消费者
    let consumer = tokio::spawn(async move {
        while let Some(value) = rx.recv().await {
            println!("收到: {}", value);
        }
    });

    producer.await.unwrap();
    consumer.await.unwrap();
}
```

#### broadcast - 广播通道

```rust
use tokio::sync::broadcast;

#[tokio::main]
async fn main() {
    let (tx, _) = broadcast::channel(16);
    let mut rx1 = tx.subscribe();
    let mut rx2 = tx.subscribe();

    // 发送者
    tokio::spawn(async move {
        for i in 0..10 {
            tx.send(i).unwrap();
        }
    });

    // 接收者1
    tokio::spawn(async move {
        while let Ok(value) = rx1.recv().await {
            println!("接收者1: {}", value);
        }
    });

    // 接收者2
    tokio::spawn(async move {
        while let Ok(value) = rx2.recv().await {
            println!("接收者2: {}", value);
        }
    });
}
```

### 2. 在CANP中的应用

```rust
// 任务队列管理
pub struct HighPerformanceExecutor {
    io_task_tx: mpsc::UnboundedSender<(TaskMetadata, BoxedTask)>,
    cpu_task_tx: mpsc::UnboundedSender<(TaskMetadata, BoxedCpuTask)>,
    priority_task_tx: mpsc::UnboundedSender<(TaskMetadata, BoxedTask)>,
}

// 任务处理循环
async fn io_task_worker(
    mut rx: mpsc::UnboundedReceiver<(TaskMetadata, BoxedTask)>,
) {
    while let Some((metadata, task)) = rx.recv().await {
        // 处理IO任务
        if let Err(e) = task.await {
            eprintln!("IO任务执行失败: {}", e);
        }
    }
}
```

## ⏰ 定时器和延迟

### 1. 基本用法

```rust
use tokio::time;

#[tokio::main]
async fn main() {
    // 延迟
    time::sleep(time::Duration::from_secs(1)).await;
    println!("1秒后");

    // 定时器
    let mut interval = time::interval(time::Duration::from_secs(1));
    for _ in 0..5 {
        interval.tick().await;
        println!("定时器触发");
    }

    // 超时
    match time::timeout(time::Duration::from_secs(5), async {
        // 可能耗时的操作
        time::sleep(time::Duration::from_secs(10)).await;
    }).await {
        Ok(_) => println!("操作完成"),
        Err(_) => println!("操作超时"),
    }
}
```

### 2. 在CANP中的应用

```rust
// 缓存过期检查
impl DbcManager {
    pub async fn cleanup_expired_cache(&self) {
        let mut interval = tokio::time::interval(Duration::from_secs(300)); // 5分钟
        
        loop {
            interval.tick().await;
            
            let mut cache = self.dbc_cache.write().await;
            let now = SystemTime::now();
            
            cache.retain(|_, entry| {
                entry.created_at.elapsed().unwrap().as_secs() < self.config.cache_expire_seconds
            });
        }
    }
}

// 任务超时处理
impl HighPerformanceExecutor {
    pub async fn submit_task_with_timeout<F>(
        &self,
        task: F,
        timeout: Duration,
    ) -> Result<()>
    where
        F: Future<Output = Result<()>>,
    {
        match tokio::time::timeout(timeout, task).await {
            Ok(result) => result,
            Err(_) => Err(anyhow!("任务执行超时")),
        }
    }
}
```

## 🔒 同步原语

### 1. Mutex

```rust
use tokio::sync::Mutex;
use std::sync::Arc;

#[tokio::main]
async fn main() {
    let counter = Arc::new(Mutex::new(0));
    let mut handles = vec![];

    for _ in 0..10 {
        let counter = Arc::clone(&counter);
        let handle = tokio::spawn(async move {
            let mut num = counter.lock().await;
            *num += 1;
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.await.unwrap();
    }

    println!("最终计数: {}", *counter.lock().await);
}
```

### 2. RwLock

```rust
use tokio::sync::RwLock;
use std::sync::Arc;

#[tokio::main]
async fn main() {
    let data = Arc::new(RwLock::new(vec![1, 2, 3, 4, 5]));

    // 多个读取者
    let read_handles: Vec<_> = (0..5).map(|i| {
        let data = Arc::clone(&data);
        tokio::spawn(async move {
            let values = data.read().await;
            println!("读取者 {}: {:?}", i, *values);
        })
    }).collect();

    // 一个写入者
    let write_handle = tokio::spawn(async move {
        let mut values = data.write().await;
        values.push(6);
        println!("写入者: 添加了 6");
    });

    // 等待所有任务完成
    for handle in read_handles {
        handle.await.unwrap();
    }
    write_handle.await.unwrap();
}
```

### 3. 在CANP中的应用

```rust
// 线程安全的缓存管理
pub struct DbcManager {
    dbc_cache: Arc<RwLock<HashMap<PathBuf, DbcCacheEntry>>>,
    stats: Arc<RwLock<DbcParsingStats>>,
}

impl DbcManager {
    pub async fn load_dbc_file<P: AsRef<Path>>(
        &self,
        file_path: P,
        priority: Option<i32>,
    ) -> Result<()> {
        let path = file_path.as_ref().to_path_buf();
        
        // 检查缓存
        {
            let cache = self.dbc_cache.read().await;
            if cache.contains_key(&path) {
                return Ok(());
            }
        }
        
        // 加载文件
        let content = tokio::fs::read_to_string(&path).await?;
        let dbc = can_dbc::DBC::from_str(&content)?;
        
        // 更新缓存
        {
            let mut cache = self.dbc_cache.write().await;
            cache.insert(path, DbcCacheEntry {
                dbc: Arc::new(dbc),
                created_at: SystemTime::now(),
                priority: priority.unwrap_or(0),
            });
        }
        
        Ok(())
    }
}
```

## 📁 文件系统操作

### 1. 异步文件操作

```rust
use tokio::fs;

#[tokio::main]
async fn main() -> Result<()> {
    // 读取文件
    let content = fs::read_to_string("input.txt").await?;
    
    // 写入文件
    fs::write("output.txt", "Hello, Tokio!").await?;
    
    // 读取目录
    let mut entries = fs::read_dir(".").await?;
    while let Some(entry) = entries.next_entry().await? {
        println!("文件: {:?}", entry.path());
    }
    
    // 文件元数据
    let metadata = fs::metadata("input.txt").await?;
    println!("文件大小: {}", metadata.len());
    
    Ok(())
}
```

### 2. 在CANP中的应用

```rust
// 异步文件处理
impl DataLayerParser {
    pub async fn parse_file(&mut self, file_path: &Path) -> Result<ParsedFileData> {
        // 异步读取文件
        let file_data = tokio::fs::read(file_path).await?;
        
        // 解析文件头部
        let file_header = FileHeader::from_bytes(&file_data[..35])?;
        
        // 异步解压缩
        let compressed_data = &file_data[35..35+file_header.compressed_length as usize];
        let decompressed_data = self.decompress_data(compressed_data).await?;
        
        // 解析数据
        let parsed_data = self.parse_frame_sequences(&decompressed_data).await?;
        
        Ok(parsed_data)
    }
    
    async fn decompress_data(&self, compressed_data: &[u8]) -> Result<Vec<u8>> {
        // 使用 spawn_blocking 处理CPU密集型解压缩
        tokio::task::spawn_blocking(move || {
            let mut decoder = flate2::read::GzDecoder::new(compressed_data);
            let mut decompressed = Vec::new();
            std::io::copy(&mut decoder, &mut decompressed)?;
            Ok::<Vec<u8>, std::io::Error>(decompressed)
        }).await?
    }
}
```

## 🌐 网络编程

### 1. TCP 服务器

```rust
use tokio::net::{TcpListener, TcpStream};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[tokio::main]
async fn main() -> Result<()> {
    let listener = TcpListener::bind("127.0.0.1:8080").await?;
    println!("服务器监听在 127.0.0.1:8080");

    loop {
        let (mut socket, _) = listener.accept().await?;

        tokio::spawn(async move {
            let mut buf = vec![0; 1024];

            loop {
                let n = match socket.read(&mut buf).await {
                    Ok(n) if n == 0 => return,
                    Ok(n) => n,
                    Err(_) => return,
                };

                if let Err(_) = socket.write_all(&buf[0..n]).await {
                    return;
                }
            }
        });
    }
}
```

### 2. HTTP 客户端

```rust
use tokio::net::TcpStream;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[tokio::main]
async fn main() -> Result<()> {
    let mut stream = TcpStream::connect("127.0.0.1:8080").await?;
    
    let request = "GET / HTTP/1.1\r\nHost: localhost\r\n\r\n";
    stream.write_all(request.as_bytes()).await?;
    
    let mut response = Vec::new();
    stream.read_to_end(&mut response).await?;
    
    println!("响应: {}", String::from_utf8_lossy(&response));
    Ok(())
}
```

## 🎯 最佳实践

### 1. 性能优化

```rust
// 使用 spawn_blocking 处理CPU密集型任务
async fn process_data(data: Vec<u8>) -> Result<ProcessedData> {
    tokio::task::spawn_blocking(move || {
        // CPU密集型处理
        process_data_sync(data)
    }).await?
}

// 批量处理
async fn process_batch(items: Vec<Item>) -> Vec<Result<ProcessedItem>> {
    let mut handles = Vec::new();
    
    for item in items {
        let handle = tokio::spawn(async move {
            process_single_item(item).await
        });
        handles.push(handle);
    }
    
    let mut results = Vec::new();
    for handle in handles {
        results.push(handle.await.unwrap());
    }
    
    results
}
```

### 2. 错误处理

```rust
// 使用 anyhow 进行错误处理
use anyhow::{Result, Context};

async fn process_with_retry<F, T>(mut f: F, max_retries: usize) -> Result<T>
where
    F: FnMut() -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<T>> + Send>>,
{
    let mut last_error = None;
    
    for attempt in 0..max_retries {
        match f().await {
            Ok(result) => return Ok(result),
            Err(e) => {
                last_error = Some(e);
                if attempt < max_retries - 1 {
                    tokio::time::sleep(tokio::time::Duration::from_secs(2u64.pow(attempt as u32))).await;
                }
            }
        }
    }
    
    Err(last_error.unwrap())
}
```

### 3. 资源管理

```rust
// 使用 Arc 共享资源
pub struct SharedResource {
    data: Arc<RwLock<HashMap<String, String>>>,
}

impl SharedResource {
    pub fn new() -> Self {
        Self {
            data: Arc::new(RwLock::new(HashMap::new())),
        }
    }
    
    pub async fn get(&self, key: &str) -> Option<String> {
        let data = self.data.read().await;
        data.get(key).cloned()
    }
    
    pub async fn set(&self, key: String, value: String) {
        let mut data = self.data.write().await;
        data.insert(key, value);
    }
}
```

## 🔧 调试和监控

### 1. 异步任务监控

```rust
use tokio::task::JoinHandle;
use std::time::Instant;

async fn monitored_task() -> Result<()> {
    let start = Instant::now();
    
    // 执行任务
    let result = perform_task().await?;
    
    let duration = start.elapsed();
    println!("任务执行时间: {:?}", duration);
    
    Ok(result)
}

// 任务超时监控
async fn timeout_monitor<F, T>(task: F, timeout: Duration) -> Result<T>
where
    F: Future<Output = Result<T>>,
{
    match tokio::time::timeout(timeout, task).await {
        Ok(result) => result,
        Err(_) => {
            eprintln!("任务执行超时");
            Err(anyhow!("任务超时"))
        }
    }
}
```

### 2. 性能分析

```rust
// 使用 tokio::time 进行性能分析
async fn profile_operation<F, T>(name: &str, operation: F) -> T
where
    F: Future<Output = T>,
{
    let start = Instant::now();
    let result = operation.await;
    let duration = start.elapsed();
    
    println!("{} 执行时间: {:?}", name, duration);
    result
}

// 使用示例
let result = profile_operation("文件解析", async {
    parser.parse_file(&file_data).await
}).await;
```

## 📚 学习资源

### 官方文档
- [Tokio Documentation](https://tokio.rs/)
- [Tokio Tutorial](https://tokio.rs/tokio/tutorial)
- [Async Rust Book](https://rust-lang.github.io/async-book/)

### 社区资源
- [Tokio Examples](https://github.com/tokio-rs/tokio/tree/master/examples)
- [Async Rust Patterns](https://rust-lang.github.io/async-book/patterns/index.html)
- [Tokio Best Practices](https://tokio.rs/tokio/tutorial/best-practices)

### 进阶主题
- [Tokio Internals](https://tokio.rs/blog/2019-10-scheduler)
- [Async Streams](https://docs.rs/tokio-stream)
- [Async Traits](https://blog.rust-lang.org/2022/11/17/async-fn-in-traits.html)

---

这个文档详细介绍了Tokio在CANP项目中的应用。建议结合实际代码进行学习，并在实践中不断优化异步编程技能。 