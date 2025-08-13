## Rust x LeetCode 实战学习指南（稳定版 + 社区最佳实践）

本指南面向准备在 LeetCode 以 Rust 解题的同学，系统覆盖：语法要点、题型模板、常见陷阱、性能优化、代码风格与高质量题解范式。示例尽量仅用 `std`，兼容在线评测环境。

---

### 1. 环境与基本约定

- 仅用稳定版 Rust + 标准库（在线评测通常不允许第三方 crate）。
- 结构体 + 关联函数提交范式：

```rust
pub struct Solution;

impl Solution {
    // 在此添加题目要求的函数签名
}
```

- 常用导入：

```rust
use std::{cmp::{min, max}, collections::{HashMap, HashSet, VecDeque, BinaryHeap}};
```

- 字符串多为 ASCII，处理性能优先使用 `as_bytes()` + 下标访问，避免逐 `char` 开销与 Unicode 复杂性。

---

### 2. Rust 语法与所有权快速回顾（解题向）

- **所有权/借用**：优先用切片 `&[T]`、`&str`，必要时 `to_owned()`/`to_vec()` 获取所有权；尽量避免不必要的 `clone()`。
- **可变性**：`let mut`；容器修改需可变借用；迭代器链末尾 `collect::<Vec<_>>()`。
- **模式匹配**：`match` + `if let`；`Option`/`Result` 常配 `?`（本地调试更便捷，在线题解多为确定性 API，可少用 `Result`）。
- **迭代器**：`iter()` 借用、`into_iter()` 取所有权、`iter_mut()` 可变；热路径可用显式 for 循环减少开销。

---

### 3. 提交模板与本地调试

```rust
pub struct Solution;

impl Solution {
    pub fn two_sum(nums: Vec<i32>, target: i32) -> Vec<i32> {
        use std::collections::HashMap;
        let mut pos = HashMap::new();
        for (i, &x) in nums.iter().enumerate() {
            if let Some(&j) = pos.get(&(target - x)) {
                return vec![j as i32, i as i32];
            }
            pos.insert(x, i);
        }
        vec![]
    }
}

// 本地测试时可添加
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn t_two_sum() {
        assert_eq!(Solution::two_sum(vec![2,7,11,15], 9), vec![0,1]);
    }
}
```

---

### 4. 数组与字符串（高频）

- 数组：索引安全，注意 `usize` 下标；切片 `&nums[l..=r]`。
- 字符串：多题默认 ASCII；建议 `let s = bytes.as_bytes();` 再用下标访问。

示例：反转字符串（344. Reverse String）

```rust
impl Solution {
    pub fn reverse_string(s: &mut Vec<char>) { // 题目给的是 Vec<char>
        let mut i = 0;
        let mut j = s.len().saturating_sub(1);
        while i < j {
            s.swap(i, j);
            i += 1;
            j -= 1;
        }
    }
}
```

示例：无重复字符的最长子串（3. Longest Substring Without Repeating Characters）

```rust
impl Solution {
    pub fn length_of_longest_substring(s: String) -> i32 {
        let b = s.as_bytes();
        let mut last = [usize::MAX; 256];
        let (mut l, mut ans) = (0usize, 0usize);
        for (r, &ch) in b.iter().enumerate() {
            let idx = ch as usize;
            if last[idx] != usize::MAX && last[idx] >= l { l = last[idx] + 1; }
            last[idx] = r;
            ans = ans.max(r - l + 1);
        }
        ans as i32
    }
}
```

---

### 5. 哈希表/计数

模板：

```rust
let mut cnt = [0i32; 128]; // ASCII 计数更快
for &ch in s.as_bytes() { cnt[ch as usize] += 1; }
```

示例：字母异位词分组（49. Group Anagrams）

```rust
impl Solution {
    pub fn group_anagrams(strs: Vec<String>) -> Vec<Vec<String>> {
        use std::collections::HashMap;
        let mut map: HashMap<[u8; 26], Vec<String>> = HashMap::new();
        for s in strs { 
            let mut key = [0u8; 26];
            for &b in s.as_bytes() { key[(b - b'a') as usize] += 1; }
            map.entry(key).or_default().push(s);
        }
        map.into_values().collect()
    }
}
```

---

### 6. 双指针

适用于已排序数组/字符串两端收缩。

示例：有序数组的两数之和（167. Two Sum II）

```rust
impl Solution {
    pub fn two_sum(numbers: Vec<i32>, target: i32) -> Vec<i32> {
        let (mut i, mut j) = (0usize, numbers.len()-1);
        while i < j {
            let s = numbers[i] + numbers[j];
            if s == target { return vec![i as i32 + 1, j as i32 + 1]; }
            if s < target { i += 1; } else { j -= 1; }
        }
        vec![]
    }
}
```

---

### 7. 滑动窗口

适合子串/子数组问题，保持窗口内计数满足约束。

示例：最小覆盖子串（76. Minimum Window Substring）

```rust
impl Solution {
    pub fn min_window(s: String, t: String) -> String {
        let s = s.as_bytes();
        let mut need = [0i32; 128];
        let mut need_cnt = 0;
        for &b in t.as_bytes() { if need[b as usize] == 0 { need_cnt += 1; } need[b as usize] += 1; }
        let (mut l, mut formed) = (0usize, 0i32);
        let mut have = [0i32; 128];
        let mut best = (usize::MAX, 0usize, 0usize);
        for r in 0..s.len() {
            let idx = s[r] as usize;
            have[idx] += 1;
            if have[idx] == need[idx] && need[idx] > 0 { formed += 1; }
            while formed == need_cnt && l <= r {
                if r - l + 1 < best.0 { best = (r - l + 1, l, r + 1); }
                let li = s[l] as usize;
                have[li] -= 1;
                if have[li] < need[li] && need[li] > 0 { formed -= 1; }
                l += 1;
            }
        }
        if best.0 == usize::MAX { String::new() } else { String::from_utf8(s[best.1..best.2].to_vec()).unwrap() }
    }
}
```

---

### 8. 栈与单调栈/队列

单调栈常用于下一个更大元素、柱状图最大矩形等。

示例：柱状图最大矩形（84. Largest Rectangle in Histogram）

```rust
impl Solution {
    pub fn largest_rectangle_area(mut heights: Vec<i32>) -> i32 {
        heights.push(0);
        let mut st: Vec<usize> = Vec::new();
        let mut ans = 0i32;
        for i in 0..heights.len() {
            while let Some(&j) = st.last() {
                if heights[j] > heights[i] {
                    st.pop();
                    let h = heights[j] as i32;
                    let l = st.last().map_or(0, |&k| k + 1);
                    let w = (i - l) as i32;
                    ans = ans.max(h * w);
                } else { break; }
            }
            st.push(i);
        }
        ans
    }
}
```

---

### 9. 链表（`Option<Box<ListNode>>`）

常用操作：反转、合并、快慢指针。注意移动语义与 `take()` 解构。

反转链表（206. Reverse Linked List）

```rust
// 已给出 ListNode 定义
impl Solution {
    pub fn reverse_list(head: Option<Box<ListNode>>) -> Option<Box<ListNode>> {
        let mut cur = head;
        let mut prev: Option<Box<ListNode>> = None;
        while let Some(mut node) = cur {
            cur = node.next.take();
            node.next = prev;
            prev = Some(node);
        }
        prev
    }
}
```

---

### 10. 二叉树（`Option<Rc<RefCell<TreeNode>>>`）

访问/修改节点值需 `RefCell`；共享子树需 `Rc`。层序遍历用 `VecDeque`。

最大深度（104. Maximum Depth of Binary Tree）

```rust
use std::rc::Rc; use std::cell::RefCell; use std::collections::VecDeque;
impl Solution {
    pub fn max_depth(root: Option<Rc<RefCell<TreeNode>>>) -> i32 {
        if root.is_none() { return 0; }
        let mut q = VecDeque::new(); q.push_back(root.unwrap());
        let mut d = 0;
        while !q.is_empty() {
            for _ in 0..q.len() {
                if let Some(node_rc) = q.pop_front() {
                    let node = node_rc.borrow();
                    if let Some(ref l) = node.left { q.push_back(l.clone()); }
                    if let Some(ref r) = node.right { q.push_back(r.clone()); }
                }
            }
            d += 1;
        }
        d
    }
}
```

---

### 11. 图与搜索（BFS/DFS/拓扑/最短路）

- 邻接表：`Vec<Vec<usize>>`/`Vec<Vec<(usize, i64)>>`（带权）。
- DFS 注意递归深度，必要时改非递归栈。

示例：课程表（207. Course Schedule，拓扑排序）

```rust
impl Solution {
    pub fn can_finish(num_courses: i32, prerequisites: Vec<Vec<i32>>) -> bool {
        let n = num_courses as usize;
        let mut g = vec![Vec::new(); n];
        let mut indeg = vec![0i32; n];
        for p in prerequisites { let a = p[0] as usize; let b = p[1] as usize; g[b].push(a); indeg[a] += 1; }
        let mut q = std::collections::VecDeque::new();
        for i in 0..n { if indeg[i] == 0 { q.push_back(i); } }
        let mut visited = 0;
        while let Some(u) = q.pop_front() {
            visited += 1;
            for &v in &g[u] { indeg[v] -= 1; if indeg[v] == 0 { q.push_back(v); } }
        }
        visited == n
    }
}
```

---

### 12. 二分搜索与“答案空间二分”

模板：在单调性上二分。

示例：吃香蕉（875. Koko Eating Bananas）

```rust
impl Solution {
    pub fn min_eating_speed(piles: Vec<i32>, h: i32) -> i32 {
        let (mut l, mut r) = (1i64, *piles.iter().max().unwrap() as i64);
        let mut ans = r;
        while l <= r {
            let m = (l + r) / 2;
            let mut need = 0i64;
            for &p in &piles { need += ((p as i64) + m - 1) / m; }
            if need <= h as i64 { ans = m; r = m - 1; } else { l = m + 1; }
        }
        ans as i32
    }
}
```

---

### 13. 动态规划（1D/2D/滚动/状态压缩）

- 一维滚动：如爬楼梯（70）、打家劫舍（198）。
- 二维 DP：如最长公共子序列（1143）。

示例：LCS（1143. Longest Common Subsequence）

```rust
impl Solution {
    pub fn longest_common_subsequence(text1: String, text2: String) -> i32 {
        let a = text1.as_bytes(); let b = text2.as_bytes();
        let (n, m) = (a.len(), b.len());
        let mut dp = vec![vec![0i32; m + 1]; n + 1];
        for i in 1..=n {
            for j in 1..=m {
                dp[i][j] = if a[i-1] == b[j-1] { dp[i-1][j-1] + 1 } else { dp[i-1][j].max(dp[i][j-1]) };
            }
        }
        dp[n][m]
    }
}
```

---

### 14. 前缀和/差分/位运算

- 前缀和：区间和/数量统计。
- 差分：区间增量（如 370. Range Addition）。
- 位运算：位掩码 DP/按位贪心（如 421. Maximum XOR）。

---

### 15. 堆与优先队列（`BinaryHeap`）

最大堆默认；最小堆用 `Reverse(T)`。

示例：前 K 个高频元素（347. Top K Frequent Elements）

```rust
impl Solution {
    pub fn top_k_frequent(nums: Vec<i32>, k: i32) -> Vec<i32> {
        use std::collections::{HashMap, BinaryHeap};
        use std::cmp::Reverse;
        let mut cnt = HashMap::new();
        for x in nums { *cnt.entry(x).or_insert(0i32) += 1; }
        let mut heap: BinaryHeap<(Reverse<i32>, i32)> = BinaryHeap::new();
        for (x, c) in cnt { heap.push((Reverse(c), x)); if heap.len() > k as usize { heap.pop(); } }
        heap.into_iter().map(|(_, x)| x).collect()
    }
}
```

---

### 16. 贪心

示例：跳跃游戏 II（45. Jump Game II）

```rust
impl Solution {
    pub fn jump(nums: Vec<i32>) -> i32 {
        let (mut end, mut far, mut step) = (0usize, 0usize, 0);
        for i in 0..nums.len()-1 {
            far = far.max(i + nums[i] as usize);
            if i == end { step += 1; end = far; }
        }
        step
    }
}
```

---

### 17. 常见陷阱与解法稳健性

- 不必要的 `clone()`：优先借用与切片。
- `String` vs `&str`：提交函数签名由题面决定，内部尽量 `as_bytes()`。
- 递归深度：树/图深递归可能栈溢出，改迭代或 BFS。
- 索引类型：数组下标用 `usize`，与 `i32`/`i64` 互转要小心。
- 排序稳定性：默认 `sort_unstable()` 更快；比较器用 `sort_unstable_by_key`/`sort_unstable_by`。

---

### 18. 性能优化清单（std-only）

- 算法优先：选择合适数据结构与复杂度。
- 容器预分配：`Vec::with_capacity`、`HashMap::with_capacity`。
- 减少边界检查：紧凑 for 循环替代迭代器链（在极限场景）。
- 输入规模大时避免频繁 `String` 拼接；使用 `push`/`push_str` 与预分配缓冲。
- 对 ASCII 任务使用定长数组计数 `[T; N]` 优于 `HashMap`。

---

### 19. 调试与本地测试

- 在本地添加 `#[cfg(test)]` 单元测试，覆盖边界条件。
- 使用 `eprintln!` 调试（在线评测会忽略）。

---

### 20. 代码风格与可读性（社区实践）

- 命名清晰：`left/right`, `l/r`, `n/m`, `row/col` 等常用习惯统一化。
- 小函数/小闭包：拆分步骤便于复用与测试。
- 注重不可变性：优先 `let`，需要时 `let mut`。
- 复杂表达式分解到局部变量，减少嵌套层级。

---

### 21. 题单路线（建议顺序）

- 入门：1 两数之和、20 有效括号、21 合并链表、104/226 树、70/198 DP 基础
- 进阶：3/76 滑窗、49/438 计数、739 单调栈、347 堆、208 Trie、207/210 拓扑
- 提升：200/695 岛屿、560 前缀和、300 LIS、1143 LCS、322 硬币兑换、45 贪心
- 专题：875/1011/410 二分答案、621 任务调度、124 树上路径、968 摄像头

---

### 22. 常用模板速取

```rust
// 读写频繁的计数数组（ASCII）
let mut cnt = [0i32; 128];

// BFS 模板
let mut q = std::collections::VecDeque::new();
q.push_back(start);
while let Some(u) = q.pop_front() { /* 扩展 */ }

// 单调栈（下一个更大）
let mut st: Vec<usize> = Vec::new();
for i in 0..n { while st.last().map_or(false, |&j| a[j] < a[i]) { st.pop(); } st.push(i); }

// 二分答案
let (mut l, mut r) = (low, high);
while l <= r { let m = (l + r) / 2; if check(m) { r = m - 1; } else { l = m + 1; } }
```

---

### 23. 延伸阅读

- Rust Book（所有权/借用/泛型/集合/并发基础）
- 标准库文档（`Vec`, `HashMap`, `BinaryHeap`, `VecDeque`, 迭代器）
- LeetCode 讨论区的 Rust 题解（关注惯用法和边界处理）

---

如需，我可将本指南拆解为可执行示例项目（含单元测试），并提供按题型的模版函数与基准测试，帮助你在本地高效刷题与验证性能。

---

### 24. 并查集（Disjoint Set Union, DSU）

适用于连通性判定、合并集合、连通块个数。

```rust
pub struct DSU {
    parent: Vec<usize>,
    rank: Vec<u8>,
    set_count: usize,
}

impl DSU {
    pub fn new(n: usize) -> Self {
        let mut parent = Vec::with_capacity(n);
        for i in 0..n { parent.push(i); }
        Self { parent, rank: vec![0; n], set_count: n }
    }
    pub fn find(&mut self, x: usize) -> usize {
        if self.parent[x] != x { self.parent[x] = self.find(self.parent[x]); }
        self.parent[x]
    }
    pub fn union(&mut self, a: usize, b: usize) -> bool {
        let mut x = self.find(a);
        let mut y = self.find(b);
        if x == y { return false; }
        if self.rank[x] < self.rank[y] { std::mem::swap(&mut x, &mut y); }
        self.parent[y] = x;
        if self.rank[x] == self.rank[y] { self.rank[x] += 1; }
        self.set_count -= 1;
        true
    }
    pub fn count(&self) -> usize { self.set_count }
}
```

应用：省份数量（547）、账户合并（721）、岛屿数量（并查集版本）。

---

### 25. Trie（前缀树）

高频于词典类题：前缀查询、替换、单词搜索。

```rust
#[derive(Default)]
struct TrieNode { child: [Option<Box<TrieNode>>; 26], end: bool }

impl TrieNode {
    fn new() -> Self { Default::default() }
    fn insert(&mut self, s: &str) {
        let mut p = self;
        for &b in s.as_bytes() {
            let i = (b - b'a') as usize;
            if p.child[i].is_none() { p.child[i] = Some(Box::new(TrieNode::new())); }
            p = p.child[i].as_mut().unwrap();
        }
        p.end = true;
    }
    fn search(&self, s: &str) -> bool {
        let mut p = self;
        for &b in s.as_bytes() {
            let i = (b - b'a') as usize;
            match p.child[i].as_deref() { Some(n) => p = n, None => return false }
        }
        p.end
    }
    fn starts_with(&self, s: &str) -> bool {
        let mut p = self;
        for &b in s.as_bytes() {
            let i = (b - b'a') as usize;
            match p.child[i].as_deref() { Some(n) => p = n, None => return false }
        }
        true
    }
}
```

---

### 26. KMP（前缀函数）

字符串匹配，`O(n+m)`。

```rust
fn prefix_function(p: &[u8]) -> Vec<usize> {
    let n = p.len();
    let mut pi = vec![0usize; n];
    for i in 1..n {
        let mut j = pi[i-1];
        while j > 0 && p[i] != p[j] { j = pi[j-1]; }
        if p[i] == p[j] { j += 1; }
        pi[i] = j;
    }
    pi
}

fn kmp_find(s: &[u8], p: &[u8]) -> Option<usize> {
    if p.is_empty() { return Some(0); }
    let pi = prefix_function(p);
    let mut j = 0usize;
    for (i, &c) in s.iter().enumerate() {
        while j > 0 && c != p[j] { j = pi[j-1]; }
        if c == p[j] { j += 1; }
        if j == p.len() { return Some(i + 1 - j); }
    }
    None
}
```

---

### 27. Z-函数与滚动哈希（可选）

Z-函数适合“匹配所有位置”；滚动哈希用于子串比较/去重（注意碰撞风险）。

```rust
fn z_function(s: &[u8]) -> Vec<usize> {
    let n = s.len();
    let (mut l, mut r) = (0usize, 0usize);
    let mut z = vec![0usize; n];
    for i in 1..n {
        if i <= r { z[i] = std::cmp::min(r - i + 1, z[i - l]); }
        while i + z[i] < n && s[z[i]] == s[i + z[i]] { z[i] += 1; }
        if i + z[i] - 1 > r { l = i; r = i + z[i] - 1; }
    }
    z
}
```

---

### 28. 树状数组（Fenwick/BIT）

适合前缀和与单点更新。

```rust
pub struct BIT { n: usize, bit: Vec<i64> }
impl BIT {
    pub fn new(n: usize) -> Self { Self { n, bit: vec![0; n+1] } }
    pub fn add(&mut self, mut i: usize, delta: i64) { while i <= self.n { self.bit[i] += delta; i += i & (!i + 1); } }
    pub fn sum(&self, mut i: usize) -> i64 { let mut s = 0; while i > 0 { s += self.bit[i]; i &= i - 1; } s }
    pub fn range_sum(&self, l: usize, r: usize) -> i64 { self.sum(r) - self.sum(l-1) }
}
```

---

### 29. 线段树（含懒标记骨架）

区间查询与更新；多数 LC 题可用 BIT 替代，懒标记适用于区间赋值/增量。

```rust
struct SegTree { n: usize, val: Vec<i64>, lazy: Vec<i64> }
impl SegTree {
    fn new(n: usize) -> Self { let sz = 4*n+5; Self { n, val: vec![0; sz], lazy: vec![0; sz] } }
    fn push(&mut self, idx: usize) {
        if self.lazy[idx] != 0 {
            let d = self.lazy[idx];
            for ch in [idx<<1, idx<<1|1] { self.lazy[ch] += d; self.val[ch] += d; }
            self.lazy[idx] = 0;
        }
    }
    fn update(&mut self, idx: usize, l: usize, r: usize, ql: usize, qr: usize, delta: i64) {
        if ql <= l && r <= qr { self.val[idx] += delta; self.lazy[idx] += delta; return; }
        self.push(idx);
        let m = (l + r) / 2;
        if ql <= m { self.update(idx<<1, l, m, ql, qr, delta); }
        if qr > m { self.update(idx<<1|1, m+1, r, ql, qr, delta); }
        self.val[idx] = self.val[idx<<1].max(self.val[idx<<1|1]);
    }
    fn query(&mut self, idx: usize, l: usize, r: usize, ql: usize, qr: usize) -> i64 {
        if ql <= l && r <= qr { return self.val[idx]; }
        self.push(idx);
        let m = (l + r) / 2;
        let mut ans = i64::MIN;
        if ql <= m { ans = ans.max(self.query(idx<<1, l, m, ql, qr)); }
        if qr > m { ans = ans.max(self.query(idx<<1|1, m+1, r, ql, qr)); }
        ans
    }
}
```

---

### 30. 回溯模板（组合/排列/子集/N 皇后）

```rust
// 子集
fn subsets(nums: Vec<i32>) -> Vec<Vec<i32>> {
    fn dfs(i: usize, nums: &Vec<i32>, cur: &mut Vec<i32>, ans: &mut Vec<Vec<i32>>) {
        if i == nums.len() { ans.push(cur.clone()); return; }
        dfs(i+1, nums, cur, ans);
        cur.push(nums[i]); dfs(i+1, nums, cur, ans); cur.pop();
    }
    let mut ans = vec![]; let mut cur = vec![]; dfs(0, &nums, &mut cur, &mut ans); ans
}

// 全排列
fn permute(mut nums: Vec<i32>) -> Vec<Vec<i32>> {
    fn dfs(i: usize, nums: &mut Vec<i32>, ans: &mut Vec<Vec<i32>>) {
        if i == nums.len() { ans.push(nums.clone()); return; }
        for j in i..nums.len() { nums.swap(i, j); dfs(i+1, nums, ans); nums.swap(i, j); }
    }
    let mut ans = vec![]; dfs(0, &mut nums, &mut ans); ans
}
```

---

### 31. 最短路：0-1 BFS 与 Dijkstra

```rust
// 0-1 BFS：边权仅 0/1
use std::collections::VecDeque;
fn zero_one_bfs(n: usize, g: &Vec<Vec<(usize, u8)>>, s: usize) -> Vec<i32> {
    let mut dist = vec![i32::MAX; n]; dist[s] = 0;
    let mut dq = VecDeque::new(); dq.push_front(s);
    while let Some(u) = dq.pop_front() {
        for &(v, w) in &g[u] {
            let nd = dist[u] + w as i32;
            if nd < dist[v] {
                dist[v] = nd;
                if w == 0 { dq.push_front(v); } else { dq.push_back(v); }
            }
        }
    }
    dist
}

// Dijkstra：非负权
use std::cmp::Reverse; use std::collections::BinaryHeap;
fn dijkstra(n: usize, g: &Vec<Vec<(usize, i64)>>, s: usize) -> Vec<i64> {
    let mut dist = vec![i64::MAX/4; n]; dist[s] = 0;
    let mut pq = BinaryHeap::new(); pq.push((Reverse(0i64), s));
    while let Some((Reverse(d), u)) = pq.pop() {
        if d != dist[u] { continue; }
        for &(v, w) in &g[u] {
            let nd = d + w; if nd < dist[v] { dist[v] = nd; pq.push((Reverse(nd), v)); }
        }
    }
    dist
}
```

---

### 32. 区间题通用范式

- 合并区间：按起点排序，维护当前合并段。
- 会议室 II：扫描线或最小堆记录结束时间。

```rust
// 合并区间
fn merge(mut a: Vec<Vec<i32>>) -> Vec<Vec<i32>> {
    a.sort_unstable_by_key(|x| (x[0], x[1]));
    let mut ans: Vec<Vec<i32>> = Vec::new();
    for x in a.into_iter() {
        if ans.is_empty() || ans.last().unwrap()[1] < x[0] { ans.push(x); }
        else { let last = ans.last_mut().unwrap(); last[1] = last[1].max(x[1]); }
    }
    ans
}
```

---

### 33. 快速选择（Quickselect）

`O(n)` 期望时间，找第 k 大/小。

```rust
fn quickselect(nums: &mut [i32], k: usize) -> i32 { // 第 k 大（k>=1）
    let n = nums.len(); let target = n - k;
    let (mut l, mut r) = (0usize, n-1);
    loop {
        let mut i = l; let mut j = r;
        let pivot = nums[(l + r) / 2];
        while i <= j {
            while nums[i] < pivot { i += 1; }
            while nums[j] > pivot { if j==0 { break; } j -= 1; }
            if i <= j { nums.swap(i, j); i += 1; if j>0 { j -= 1; } }
        }
        if target <= j { r = j; } else if target >= i { l = i; } else { return nums[target]; }
    }
}
```

---

### 34. 设计题：LRU Cache 骨架（146）

使用哈希表 + 双向链表。以下为安全实现骨架（索引模拟链表）。

```rust
pub struct LRUCache {
    cap: usize,
    map: std::collections::HashMap<i32, usize>,
    key: Vec<i32>, val: Vec<i32>, prev: Vec<usize>, next: Vec<usize>,
    head: usize, tail: usize, free: Vec<usize>,
}

impl LRUCache {
    pub fn new(capacity: i32) -> Self {
        let cap = capacity as usize;
        let mut this = Self {
            cap,
            map: Default::default(),
            key: vec![0; cap+2], val: vec![0; cap+2], prev: vec![0; cap+2], next: vec![0; cap+2],
            head: 0, tail: cap+1, free: (1..=cap).rev().collect(),
        };
        this.next[0] = this.tail; this.prev[this.tail] = 0;
        this
    }
    fn remove(&mut self, i: usize) { let p = self.prev[i]; let n = self.next[i]; self.next[p] = n; self.prev[n] = p; }
    fn push_front(&mut self, i: usize) { let n = self.next[0]; self.next[0] = i; self.prev[i] = 0; self.next[i] = n; self.prev[n] = i; }
    pub fn get(&mut self, key: i32) -> i32 {
        if let Some(&i) = self.map.get(&key) { self.remove(i); self.push_front(i); self.val[i] } else { -1 }
    }
    pub fn put(&mut self, key: i32, value: i32) {
        if let Some(&i) = self.map.get(&key) {
            self.val[i] = value; self.remove(i); self.push_front(i); return;
        }
        if self.free.is_empty() {
            let lru = self.prev[self.tail]; // 移除尾前
            self.remove(lru);
            self.map.remove(&self.key[lru]);
            self.free.push(lru);
        }
        let i = self.free.pop().unwrap();
        self.key[i] = key; self.val[i] = value; self.map.insert(key, i); self.push_front(i);
    }
}
```

---

### 35. 网格并查集与多源 BFS 模板

- 网格坐标映射：`id = r * m + c`。
- 多源 BFS：初始将所有源点入队。

```rust
// 多源 BFS
use std::collections::VecDeque;
fn multi_source_bfs(n: usize, m: usize, sources: &[(usize, usize)]) -> Vec<Vec<i32>> {
    let mut dist = vec![vec![-1; m]; n];
    let mut q = VecDeque::new();
    for &(r, c) in sources { dist[r][c] = 0; q.push_back((r, c)); }
    let dirs = [(1,0),(-1,0),(0,1),(0,-1)];
    while let Some((r, c)) = q.pop_front() {
        for (dr, dc) in dirs { let (nr, nc) = (r as isize+dr, c as isize+dc);
            if nr>=0 && nr<n as isize && nc>=0 && nc<m as isize {
                let (nr, nc) = (nr as usize, nc as usize);
                if dist[nr][nc] == -1 { dist[nr][nc] = dist[r][c] + 1; q.push_back((nr, nc)); }
            }
        }
    }
    dist
}
```

---

### 36. 二维前缀和与差分

```rust
// 二维前缀和：psum[i+1][j+1] = 矩阵[0..=i][0..=j] 之和
fn build_psum(a: &Vec<Vec<i32>>) -> Vec<Vec<i32>> {
    let (n, m) = (a.len(), a[0].len());
    let mut ps = vec![vec![0; m+1]; n+1];
    for i in 0..n { for j in 0..m { ps[i+1][j+1] = ps[i+1][j] + ps[i][j+1] - ps[i][j] + a[i][j]; } }
    ps
}
fn rect_sum(ps: &Vec<Vec<i32>>, r1: usize, c1: usize, r2: usize, c2: usize) -> i32 {
    ps[r2+1][c2+1] - ps[r1][c2+1] - ps[r2+1][c1] + ps[r1][c1]
}
```

---

### 37. 位掩码 DP（子集枚举）

```rust
// 子集枚举：for s in (0..1<<n)
// 子集遍历（子集的子集）：t = s; while t>0 { t=(t-1)&s }
```

---

### 38. 提交小贴士（LeetCode 环境）

- 遵循题面签名与 `struct Solution;` 范式。
- 若需辅助函数/结构，可放在同文件上方或 `impl Solution` 外部。
- 慎用全局静态与 `unsafe`（通常不必要）。
- 关注时限/内存：尽量 `O(1)` 额外空间、原地修改、预分配。


