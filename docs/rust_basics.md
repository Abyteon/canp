# Rust 基础知识学习指南

## 📚 概述

本文档介绍CANP项目中使用的Rust核心概念和基础知识，帮助开发者快速掌握Rust编程。

## 🏗️ 核心概念

### 1. 所有权系统 (Ownership)

Rust的所有权系统是其最核心的特性，确保内存安全而无需垃圾回收。

#### 基本规则

```rust
// 1. 每个值都有一个所有者
let s1 = String::from("hello"); // s1 是所有者

// 2. 同一时间只能有一个所有者
let s2 = s1; // s1 的所有权移动到 s2，s1 不再有效
// println!("{}", s1); // 编译错误！

// 3. 当所有者离开作用域时，值被丢弃
{
    let s3 = String::from("world");
    // s3 在这里有效
} // s3 在这里被丢弃
```

#### 在CANP中的应用

```rust
// 内存池中的所有权管理
pub struct MemoryMappedBlock {
    mmap: Arc<Mmap>,  // 使用 Arc 实现共享所有权
    file_path: PathBuf,
}

// 零拷贝缓冲区
pub struct MutableMemoryBuffer {
    buffer: BytesMut,  // 内部管理所有权
}
```

### 2. 借用和引用 (Borrowing & References)

#### 不可变引用

```rust
fn calculate_length(s: &String) -> usize {
    s.len() // 借用，不获取所有权
}

let s1 = String::from("hello");
let len = calculate_length(&s1); // 传递引用
println!("'{}' 的长度是 {}", s1, len); // s1 仍然有效
```

#### 可变引用

```rust
fn append_world(s: &mut String) {
    s.push_str(" world");
}

let mut s1 = String::from("hello");
append_world(&mut s1);
println!("{}", s1); // "hello world"
```

#### 借用规则

```rust
// 1. 在任意给定时间，要么只能有一个可变引用，要么只能有任意数量的不可变引用
let mut s = String::from("hello");

let r1 = &s; // 不可变引用
let r2 = &s; // 不可变引用
// let r3 = &mut s; // 编译错误！不能同时有可变和不可变引用

println!("{} and {}", r1, r2); // r1 和 r2 在这里不再使用

let r3 = &mut s; // 现在可以创建可变引用
r3.push_str(" world");
```

### 3. 生命周期 (Lifetimes)

生命周期确保引用在有效期内保持有效。

#### 基本语法

```rust
// 生命周期注解
fn longest<'a>(x: &'a str, y: &'a str) -> &'a str {
    if x.len() > y.len() {
        x
    } else {
        y
    }
}

// 结构体中的生命周期
struct ImportantExcerpt<'a> {
    part: &'a str,
}

let novel = String::from("Call me Ishmael. Some years ago...");
let first_sentence = novel.split('.').next().unwrap();
let i = ImportantExcerpt {
    part: first_sentence,
};
```

#### 在CANP中的应用

```rust
// 解析器中的生命周期管理
pub struct DataLayerParser<'a> {
    memory_pool: &'a ZeroCopyMemoryPool,
    stats: ParsingStats,
}

// 文件映射的生命周期
pub struct MemoryMappedBlock {
    mmap: Arc<Mmap>,  // Arc 管理生命周期
    file_path: PathBuf,
}
```

## 🔄 并发编程

### 1. 线程 (Threads)

#### 基本线程操作

```rust
use std::thread;
use std::time::Duration;

// 创建新线程
let handle = thread::spawn(|| {
    for i in 1..10 {
        println!("线程中的数字: {}", i);
        thread::sleep(Duration::from_millis(1));
    }
});

// 主线程工作
for i in 1..5 {
    println!("主线程中的数字: {}", i);
    thread::sleep(Duration::from_millis(1));
}

// 等待子线程完成
handle.join().unwrap();
```

#### 线程间数据传递

```rust
use std::sync::mpsc;
use std::thread;

let (tx, rx) = mpsc::channel();

thread::spawn(move || {
    let val = String::from("hi");
    tx.send(val).unwrap();
    // println!("val is {}", val); // 编译错误！val 已经被发送
});

let received = rx.recv().unwrap();
println!("收到: {}", received);
```

### 2. 智能指针

#### Box<T> - 堆分配

```rust
// 递归数据结构
enum List {
    Cons(i32, Box<List>),
    Nil,
}

use List::{Cons, Nil};

let list = Cons(1,
    Box::new(Cons(2,
        Box::new(Cons(3,
            Box::new(Nil))))));
```

#### Rc<T> - 引用计数

```rust
use std::rc::Rc;

let a = Rc::new(Cons(5, Rc::new(Cons(10, Rc::new(Nil)))));
println!("创建 a 后，a 的引用计数 = {}", Rc::strong_count(&a));

let b = Cons(3, Rc::clone(&a));
println!("创建 b 后，a 的引用计数 = {}", Rc::strong_count(&a));

{
    let c = Cons(4, Rc::clone(&a));
    println!("创建 c 后，a 的引用计数 = {}", Rc::strong_count(&a));
}

println!("c 离开作用域后，a 的引用计数 = {}", Rc::strong_count(&a));
```

#### Arc<T> - 原子引用计数

```rust
use std::sync::Arc;
use std::thread;

let counter = Arc::new(Mutex::new(0));
let mut handles = vec![];

for _ in 0..10 {
    let counter = Arc::clone(&counter);
    let handle = thread::spawn(move || {
        let mut num = counter.lock().unwrap();
        *num += 1;
    });
    handles.push(handle);
}

for handle in handles {
    handle.join().unwrap();
}

println!("结果: {}", *counter.lock().unwrap());
```

#### Mutex<T> - 互斥锁

```rust
use std::sync::Mutex;

let m = Mutex::new(5);

{
    let mut num = m.lock().unwrap();
    *num = 6;
} // 锁在这里自动释放

println!("m = {:?}", m);
```

## 🚀 异步编程

### 1. async/await 基础

```rust
use tokio;

async fn fetch_data() -> String {
    // 模拟异步操作
    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
    "数据获取完成".to_string()
}

async fn process_data() {
    let data = fetch_data().await;
    println!("{}", data);
}

#[tokio::main]
async fn main() {
    process_data().await;
}
```

### 2. Future 特征

```rust
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

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

### 3. 在CANP中的应用

```rust
// 异步文件处理
pub async fn parse_file(&mut self, file_data: &[u8]) -> Result<ParsedFileData> {
    // 异步解析操作
    let file_header = FileHeader::from_bytes(&file_data[..35])?;
    
    // 异步解压缩
    let compressed_data = &file_data[35..35+file_header.compressed_length as usize];
    let decompressed_data = self.decompress_data(compressed_data).await?;
    
    // 异步解析
    let parsed_data = self.parse_frame_sequences(&decompressed_data).await?;
    
    Ok(parsed_data)
}
```

## 📦 错误处理

### 1. Result 类型

```rust
use std::fs::File;
use std::io::{self, Read};

fn read_username_from_file() -> Result<String, io::Error> {
    let mut f = File::open("hello.txt")?;
    let mut s = String::new();
    f.read_to_string(&mut s)?;
    Ok(s)
}

// 使用 ? 操作符
fn read_username_from_file_short() -> Result<String, io::Error> {
    let mut s = String::new();
    File::open("hello.txt")?.read_to_string(&mut s)?;
    Ok(s)
}
```

### 2. Option 类型

```rust
fn find_item(items: &[i32], target: i32) -> Option<usize> {
    for (index, &item) in items.iter().enumerate() {
        if item == target {
            return Some(index);
        }
    }
    None
}

// 使用 match
let items = vec![1, 2, 3, 4, 5];
match find_item(&items, 3) {
    Some(index) => println!("找到 3 在索引 {}", index),
    None => println!("没有找到 3"),
}

// 使用 if let
if let Some(index) = find_item(&items, 3) {
    println!("找到 3 在索引 {}", index);
}
```

### 3. 在CANP中的应用

```rust
// 统一的错误处理
use anyhow::{Result, Context};

pub async fn process_files(&self) -> Result<ProcessingResult> {
    let files = self.scan_input_files()
        .context("扫描输入文件失败")?;
    
    let mut results = Vec::new();
    for file in files {
        let result = self.process_single_file(&file)
            .await
            .context(format!("处理文件 {:?} 失败", file))?;
        results.push(result);
    }
    
    Ok(ProcessingResult::from_results(results))
}
```

## 🔧 特征 (Traits)

### 1. 特征定义和实现

```rust
// 定义特征
trait Summary {
    fn summarize(&self) -> String;
    
    // 默认实现
    fn default_summary(&self) -> String {
        String::from("(阅读更多...)")
    }
}

// 为结构体实现特征
struct NewsArticle {
    headline: String,
    location: String,
    author: String,
    content: String,
}

impl Summary for NewsArticle {
    fn summarize(&self) -> String {
        format!("{}, by {} ({})", self.headline, self.author, self.location)
    }
}
```

### 2. 特征作为参数

```rust
// 特征约束
fn notify(item: &impl Summary) {
    println!("突发新闻! {}", item.summarize());
}

// 特征约束语法
fn notify<T: Summary>(item: &T) {
    println!("突发新闻! {}", item.summarize());
}

// 多个特征约束
fn notify(item: &(impl Summary + Display)) {
    println!("突发新闻! {}", item.summarize());
}
```

### 3. 在CANP中的应用

```rust
// 可序列化特征
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct CanFrame {
    pub id: u32,
    pub dlc: u8,
    pub data: Vec<u8>,
    pub timestamp: u64,
    pub frame_type: CanFrameType,
    pub is_remote: bool,
}

// 自定义特征
pub trait DataProcessor {
    fn process(&self, data: &[u8]) -> Result<ProcessedData>;
    fn get_stats(&self) -> ProcessingStats;
}

impl DataProcessor for DataLayerParser {
    fn process(&self, data: &[u8]) -> Result<ProcessedData> {
        // 实现处理逻辑
        todo!()
    }
    
    fn get_stats(&self) -> ProcessingStats {
        self.stats.clone()
    }
}
```

## 📊 集合类型

### 1. Vector

```rust
// 创建和操作
let mut v: Vec<i32> = Vec::new();
v.push(1);
v.push(2);
v.push(3);

// 宏创建
let v = vec![1, 2, 3, 4, 5];

// 访问元素
let third: &i32 = &v[2];
let third: Option<&i32> = v.get(2);

// 迭代
for i in &v {
    println!("{}", i);
}

// 可变迭代
for i in &mut v {
    *i += 50;
}
```

### 2. HashMap

```rust
use std::collections::HashMap;

let mut scores = HashMap::new();
scores.insert(String::from("Blue"), 10);
scores.insert(String::from("Red"), 50);

// 从向量创建
let teams = vec![String::from("Blue"), String::from("Red")];
let initial_scores = vec![10, 50];
let scores: HashMap<_, _> = teams.into_iter().zip(initial_scores.into_iter()).collect();

// 访问值
let team_name = String::from("Blue");
let score = scores.get(&team_name);

// 更新
scores.insert(String::from("Blue"), 25); // 覆盖
scores.entry(String::from("Yellow")).or_insert(50); // 只在不存在时插入
```

### 3. 在CANP中的应用

```rust
// 缓存管理
pub struct DbcManager {
    dbc_cache: Arc<RwLock<HashMap<PathBuf, DbcCacheEntry>>>,
    stats: Arc<RwLock<DbcParsingStats>>,
}

// 统计收集
pub struct ProcessingStats {
    pub files_processed: usize,
    pub frames_parsed: usize,
    pub total_bytes: usize,
}

// 批量处理
pub fn process_batch(&self, frames: Vec<CanFrame>) -> Result<Vec<ParsedMessage>> {
    let mut results = Vec::with_capacity(frames.len());
    for frame in frames {
        if let Some(parsed) = self.parse_can_frame(&frame)? {
            results.push(parsed);
        }
    }
    Ok(results)
}
```

## 🎯 最佳实践

### 1. 性能优化

```rust
// 预分配容量
let mut v = Vec::with_capacity(1000);
for i in 0..1000 {
    v.push(i);
}

// 使用引用避免克隆
fn process_data(data: &[u8]) -> Result<ProcessedData> {
    // 处理逻辑
    todo!()
}

// 使用迭代器
let sum: i32 = (1..=100).sum();
let doubled: Vec<i32> = (1..=10).map(|x| x * 2).collect();
```

### 2. 内存安全

```rust
// 使用智能指针管理内存
use std::sync::Arc;
use std::sync::Mutex;

pub struct SharedState {
    data: Arc<Mutex<Vec<String>>>,
}

// 避免循环引用
use std::rc::{Rc, Weak};
use std::cell::RefCell;

struct Node {
    value: i32,
    parent: RefCell<Weak<Node>>,
    children: RefCell<Vec<Rc<Node>>>,
}
```

### 3. 错误处理

```rust
// 使用 anyhow 进行统一错误处理
use anyhow::{Result, Context};

fn process_file(path: &str) -> Result<()> {
    let content = std::fs::read_to_string(path)
        .context(format!("无法读取文件: {}", path))?;
    
    let parsed = parse_content(&content)
        .context("解析内容失败")?;
    
    Ok(())
}

// 自定义错误类型
#[derive(Debug, thiserror::Error)]
pub enum ProcessingError {
    #[error("文件不存在: {0}")]
    FileNotFound(String),
    #[error("解析失败: {0}")]
    ParseError(String),
    #[error("内存不足")]
    OutOfMemory,
}
```

## 📚 学习资源

### 官方文档
- [Rust Book](https://doc.rust-lang.org/book/)
- [Rust Reference](https://doc.rust-lang.org/reference/)
- [Rust by Example](https://doc.rust-lang.org/rust-by-example/)

### 社区资源
- [Rustlings](https://github.com/rust-lang/rustlings)
- [Rust Playground](https://play.rust-lang.org/)
- [Rust Cookbook](https://rust-lang-nursery.github.io/rust-cookbook/)

### 进阶主题
- [Asynchronous Programming in Rust](https://rust-lang.github.io/async-book/)
- [Rust Performance](https://nnethercote.github.io/perf-book/)
- [Rust Design Patterns](https://rust-unofficial.github.io/patterns/)

---

这个文档涵盖了CANP项目中使用的Rust核心概念。建议按照顺序学习，并在实践中不断巩固这些概念。 