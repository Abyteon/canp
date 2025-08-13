use anyhow::Result;
use bytes::{BufMut, Bytes, BytesMut};
use lock_pool::{LockGuard, LockPool};
use memmap2::Mmap;
use std::fs::File;
use std::path::Path;
use std::sync::Arc;
use std::sync::RwLock;

use tracing::{info, warn};

fn main() -> Result<()> {
    println!("Memory Pool Example");
}
