//! Independent public-lifecycle profile. No timing assertions or oracle tooling.
//! Counts Rust allocator requests, including reallocations; excludes libc/caller buffers.
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};
use std::time::Instant;

struct Count;
static ALLOCS: AtomicUsize = AtomicUsize::new(0);
static BYTES: AtomicUsize = AtomicUsize::new(0);
static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

fn added(bytes: usize) {
    let live = LIVE.fetch_add(bytes, Relaxed) + bytes;
    PEAK.fetch_max(live, Relaxed);
}
unsafe impl GlobalAlloc for Count {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCS.fetch_add(1, Relaxed);
        BYTES.fetch_add(layout.size(), Relaxed);
        let out = unsafe { System.alloc(layout) };
        if !out.is_null() {
            added(layout.size());
        }
        out
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        LIVE.fetch_sub(layout.size(), Relaxed);
        unsafe { System.dealloc(pointer, layout) }
    }
    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        ALLOCS.fetch_add(1, Relaxed);
        BYTES.fetch_add(size, Relaxed);
        let out = unsafe { System.realloc(pointer, layout, size) };
        if !out.is_null() {
            if size >= layout.size() {
                added(size - layout.size());
            } else {
                LIVE.fetch_sub(layout.size() - size, Relaxed);
            }
        }
        out
    }
}
#[global_allocator]
static ALLOCATOR: Count = Count;

fn lifecycle(mode: &str, data: &[u8]) -> [u128; 3] {
    let start = Instant::now();
    if mode == "RustObserved" {
        let mut parser =
            serde_ucl::parse::Parser::with_flags(serde_ucl::value::ParserFlags::empty());
        parser.set_loader(serde_ucl::parse::FsLoader::new());
        if let Ok(cwd) = std::env::current_dir() {
            parser.set_base_dir(cwd);
        }
        let setup = start.elapsed().as_nanos();
        let start = Instant::now();
        let observation = parser.observe_c_input(serde_ucl::parse::Input::bytes(data));
        assert!(observation.error.is_none());
        let submit = start.elapsed().as_nanos();
        let start = Instant::now();
        drop(observation);
        drop(parser);
        [setup, submit, start.elapsed().as_nanos()]
    } else if mode == "Rust" {
        let mut parser = serde_ucl::parse::Parser::new();
        let setup = start.elapsed().as_nanos();
        let start = Instant::now();
        let value = parser.parse(data).expect("valid profile input");
        let submit = start.elapsed().as_nanos();
        let start = Instant::now();
        drop(value);
        drop(parser);
        [setup, submit, start.elapsed().as_nanos()]
    } else {
        unsafe {
            let parser = ucl::ucl_parser_new(0);
            let setup = start.elapsed().as_nanos();
            let start = Instant::now();
            assert!(ucl::ucl_parser_add_chunk(parser, data.as_ptr(), data.len()));
            let root = ucl::ucl_parser_get_object(parser);
            let submit = start.elapsed().as_nanos();
            let start = Instant::now();
            ucl::ucl_parser_free(parser);
            ucl::ucl_object_unref(root);
            [setup, submit, start.elapsed().as_nanos()]
        }
    }
}

fn main() {
    let input = std::env::args().nth(1).unwrap_or("10000".into());
    let data = if let Ok(records) = input.parse::<usize>() {
        (0..records)
            .map(|i| {
                format!("record{i} {{ id={i}; name=\"record {i}\"; enabled=true; weight=1.25; }}\n")
            })
            .collect::<String>()
    } else {
        std::fs::read_to_string(input).expect("profile input file")
    };
    let rounds = std::env::var("PROFILE_ROUNDS")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(30);
    let selected = std::env::var("PROFILE_MODE").ok();
    if std::env::var_os("PROFILE_READS").is_some() {
        profile_reads(data.as_bytes(), rounds);
        return;
    }
    for mode in ["Rust", "RustObserved", "C"] {
        if selected.as_ref().is_some_and(|selected| selected != mode) {
            continue;
        }
        for _ in 0..3 {
            lifecycle(mode, data.as_bytes());
        }
        let mut elapsed = [0u128; 3];
        let (mut allocations, mut bytes, mut peak) = (0, 0, 0);
        for _ in 0..rounds {
            let base = LIVE.load(Relaxed);
            ALLOCS.store(0, Relaxed);
            BYTES.store(0, Relaxed);
            PEAK.store(base, Relaxed);
            let phases = lifecycle(mode, data.as_bytes());
            for (total, phase) in elapsed.iter_mut().zip(phases) {
                *total += phase;
            }
            allocations += ALLOCS.load(Relaxed);
            bytes += BYTES.load(Relaxed);
            peak += PEAK.load(Relaxed) - base;
            assert_eq!(
                LIVE.load(Relaxed),
                base,
                "lifecycle must release Rust allocations"
            );
        }
        println!(
            "{{\"mode\":\"{mode}\",\"input_bytes\":{},\"warmup\":3,\"rounds\":{rounds},\"setup_ns\":{},\"submit_ns\":{},\"destroy_ns\":{},\"alloc_requests\":{},\"requested_bytes\":{},\"peak_extra_live_bytes\":{},\"remaining_live_bytes\":0}}",
            data.len(),
            elapsed[0] / rounds as u128,
            elapsed[1] / rounds as u128,
            elapsed[2] / rounds as u128,
            allocations / rounds,
            bytes / rounds,
            peak / rounds
        );
    }
}

fn profile_reads(data: &[u8], rounds: usize) {
    unsafe {
        let parser = ucl::ucl_parser_new(0);
        assert!(ucl::ucl_parser_add_chunk(parser, data.as_ptr(), data.len()));
        let root = ucl::ucl_parser_get_object(parser);
        ucl::ucl_parser_free(parser);
        for full in [false, true] {
            let mut elapsed = 0;
            let (mut allocations, mut bytes, mut peak) = (0, 0, 0);
            for round in 0..rounds + 3 {
                let base = LIVE.load(Relaxed);
                ALLOCS.store(0, Relaxed);
                BYTES.store(0, Relaxed);
                PEAK.store(base, Relaxed);
                let start = Instant::now();
                let iterator = ucl::ucl_object_iterate_new(root);
                if full {
                    while !ucl::ucl_object_iterate_safe(iterator, true).is_null() {}
                } else {
                    std::hint::black_box(ucl::ucl_object_iterate_safe(iterator, true));
                }
                ucl::ucl_object_iterate_free(iterator);
                let duration = start.elapsed().as_nanos();
                if round >= 3 {
                    elapsed += duration;
                    allocations += ALLOCS.load(Relaxed);
                    bytes += BYTES.load(Relaxed);
                    peak += PEAK.load(Relaxed) - base;
                }
                assert_eq!(LIVE.load(Relaxed), base);
            }
            let traversal = if full { "all_children" } else { "first_child" };
            println!(
                "{{\"mode\":\"CRead\",\"traversal\":\"{traversal}\",\"input_bytes\":{},\"warmup\":3,\"rounds\":{rounds},\"elapsed_ns\":{},\"alloc_requests\":{},\"requested_bytes\":{},\"peak_extra_live_bytes\":{},\"remaining_live_bytes\":0}}",
                data.len(),
                elapsed / rounds as u128,
                allocations / rounds,
                bytes / rounds,
                peak / rounds
            );
        }
        ucl::ucl_object_unref(root);
    }
}
