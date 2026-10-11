//! Synthetic comparison of redundant copying/double parsing and buffer reuse.
//! Excludes sockets, scheduling, health, canonical normalization and routing.
use bytes::Bytes;
use dhan_rs::ws::market_feed::{parse_packets, parse_packets_into};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

struct CountingAllocator;
static TRACK: AtomicBool = AtomicBool::new(false);
static ALLOCATIONS: AtomicU64 = AtomicU64::new(0);
// SAFETY: every operation is forwarded unchanged to System. Counters retain
// no pointers and are enabled only during this single-threaded example's probe.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if TRACK.load(Ordering::Relaxed) {
            ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        }
        // SAFETY: the allocator caller supplies a valid Layout.
        unsafe { System.alloc(layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        if TRACK.load(Ordering::Relaxed) {
            ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        }
        // SAFETY: caller ownership and allocation layout are unchanged.
        unsafe { System.realloc(ptr, layout, size) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: the original pointer and Layout are forwarded to System.
        unsafe { System.dealloc(ptr, layout) }
    }
}
#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

fn main() {
    let iterations: usize = std::env::args()
        .nth(1)
        .and_then(|v| v.parse().ok())
        .unwrap_or(10_000);
    assert!(iterations > 0);
    for packets in [1, 100, 500] {
        let mut packet = [0u8; 162];
        packet[0] = 8;
        packet[1..3].copy_from_slice(&162u16.to_le_bytes());
        packet[3] = 1;
        packet[4..8].copy_from_slice(&101u32.to_le_bytes());
        packet[8..12].copy_from_slice(&101.5f32.to_le_bytes());
        let frame = packet.repeat(packets);
        let mut events = Vec::new();
        for _ in 0..200 {
            parse_packets_into(&frame, &mut events).unwrap();
            black_box(&events);
        }
        let old = || {
            let raw = Bytes::copy_from_slice(black_box(&frame));
            black_box(parse_packets(&frame).unwrap());
            black_box(parse_packets(&raw).unwrap());
        };
        let started = Instant::now();
        for _ in 0..iterations {
            old();
        }
        let double_parse_ns = started.elapsed().as_nanos();
        let started = Instant::now();
        for _ in 0..iterations {
            parse_packets_into(black_box(&frame), &mut events).unwrap();
            black_box(&events);
        }
        let reused_parse_ns = started.elapsed().as_nanos();
        ALLOCATIONS.store(0, Ordering::Relaxed);
        TRACK.store(true, Ordering::Relaxed);
        old();
        TRACK.store(false, Ordering::Relaxed);
        let double_parse_allocations = ALLOCATIONS.load(Ordering::Relaxed);
        ALLOCATIONS.store(0, Ordering::Relaxed);
        TRACK.store(true, Ordering::Relaxed);
        parse_packets_into(black_box(&frame), &mut events).unwrap();
        black_box(&events);
        TRACK.store(false, Ordering::Relaxed);
        let reused_parse_allocations = ALLOCATIONS.load(Ordering::Relaxed);
        println!(
            "{}",
            serde_json::json!({
                "scope": "synthetic binary parsing and raw copy only; current parser in both paths",
                "packets_per_message": packets, "iterations": iterations,
                "double_parse_ns": double_parse_ns, "reused_parse_ns": reused_parse_ns,
                "double_parse_allocations_per_message": double_parse_allocations,
                "reused_parse_allocations_per_message": reused_parse_allocations,
                "ratio": double_parse_ns as f64 / reused_parse_ns as f64,
            })
        );
    }
}
