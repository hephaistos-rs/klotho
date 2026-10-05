//! Phase 3 "Done when": downloading a 1 GiB file through `raw` doesn't grow
//! server memory (NFR-PERF-013).
//!
//! The test binary counts every heap allocation. It has this one test so
//! nothing else allocates at the same time. The git client runs as its own
//! process, so only the server's allocations (and the test's small buffers)
//! are counted.

mod common;

use std::alloc::{GlobalAlloc, Layout, System};
use std::io::Write;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use common::{blocking, git};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};

struct Counting;

static CURRENT: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

fn grew(by: usize) {
    let now = CURRENT.fetch_add(by, Ordering::Relaxed) + by;
    PEAK.fetch_max(now, Ordering::Relaxed);
}

// SAFETY: every call is passed straight to `System`; only counters are added.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            grew(layout.size());
        }
        ptr
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let ptr = unsafe { System.alloc_zeroed(layout) };
        if !ptr.is_null() {
            grew(layout.size());
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) };
        CURRENT.fetch_sub(layout.size(), Ordering::Relaxed);
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let new = unsafe { System.realloc(ptr, layout, new_size) };
        if !new.is_null() {
            CURRENT.fetch_sub(layout.size(), Ordering::Relaxed);
            grew(new_size);
        }
        new
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

const SIZE: u64 = 1 << 30;
/// Text that zlib squeezes well, so the push stays quick.
const LINE: &[u8] = b"Klotho streams this file to the client a chunk at a time.\n";

fn expected_byte(at: u64) -> u8 {
    LINE[(at % LINE.len() as u64) as usize]
}

#[tokio::test(flavor = "multi_thread")]
async fn a_1_gib_raw_download_does_not_grow_server_memory() {
    let running = common::start(Duration::from_secs(5)).await;
    let work = tempfile::tempdir().unwrap();
    let (work_path, url) = (work.path().to_owned(), running.url("/alice/demo.git"));
    blocking(move || {
        let local = work_path.join("local");
        std::fs::create_dir_all(&local).unwrap();
        git(&local, &["init", "-q"]);
        let mut file = std::io::BufWriter::new(std::fs::File::create(local.join("big.txt")).unwrap());
        let mut written = 0;
        while written < SIZE {
            let n = (SIZE - written).min(LINE.len() as u64) as usize;
            file.write_all(&LINE[..n]).unwrap();
            written += n as u64;
        }
        file.flush().unwrap();
        drop(file);
        git(&local, &["add", "big.txt"]);
        git(&local, &["commit", "-q", "-m", "big"]);
        git(&local, &["push", "-q", &url, "HEAD:refs/heads/main"]);
    })
    .await;

    let baseline = CURRENT.load(Ordering::Relaxed);
    PEAK.store(baseline, Ordering::Relaxed);

    let mut conn = tokio::net::TcpStream::connect(running.addr).await.unwrap();
    let request = "GET /api/v1/repos/alice/demo/raw/big.txt HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n";
    conn.write_all(request.as_bytes()).await.unwrap();
    let mut response = BufReader::with_capacity(64 * 1024, conn);
    let mut headers = String::new();
    loop {
        let mut line = String::new();
        response.read_line(&mut line).await.unwrap();
        if line == "\r\n" {
            break;
        }
        headers.push_str(&line.to_ascii_lowercase());
    }
    assert!(headers.starts_with("http/1.1 200"), "{headers}");
    assert!(headers.contains("x-content-type-options: nosniff"), "{headers}");
    assert!(headers.contains("content-type: application/octet-stream"), "{headers}");
    assert!(headers.contains(&format!("content-length: {SIZE}")), "{headers}");

    let (mut at, mut buf) = (0u64, vec![0u8; 64 * 1024]);
    loop {
        let n = response.read(&mut buf).await.unwrap();
        if n == 0 {
            break;
        }
        for (i, &byte) in buf[..n].iter().enumerate().step_by(4093) {
            assert_eq!(byte, expected_byte(at + i as u64), "byte {}", at + i as u64);
        }
        at += n as u64;
    }
    assert_eq!(at, SIZE);

    let growth = PEAK.load(Ordering::Relaxed).saturating_sub(baseline);
    assert!(growth < 32 << 20, "heap grew by {} MiB while serving 1 GiB", growth >> 20);
}
