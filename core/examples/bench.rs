// SPDX-License-Identifier: MPL-2.0
//! Throughput benchmark for wjson_core (no external dependencies).
//!   cargo run --release --example bench
//!
//! Generates three synthetic documents in memory and reports the best of
//! several runs for DOM parsing, streaming, key lookup and editing.

use std::time::Instant;
use wjson_core::*;

fn records(n: usize) -> String {
    let mut s = String::from("[");
    for i in 0..n {
        if i > 0 {
            s.push(',');
        }
        s.push_str(&format!(
            r#"{{"id":{i},"name":"user {i}","email":"user{i}@example.com","tags":["wave","rust","c++"],"score":{}.{:02},"active":{},"address":{{"city":"Seoul","zip":"0{}"}}}}"#,
            i % 1000,
            i % 100,
            i % 2 == 0,
            4500 + i % 500
        ));
    }
    s.push(']');
    s
}

fn numbers(n: usize) -> String {
    let mut s = String::from("[");
    let mut x: u64 = 88172645463325252;
    for i in 0..n {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        if i > 0 {
            s.push(',');
        }
        let a = (x % 3_600_000_000) as f64 / 1e7 - 180.0;
        let b = ((x >> 20) % 1_800_000_000) as f64 / 1e7 - 90.0;
        s.push_str(&format!("[{a},{b}]"));
    }
    s.push(']');
    s
}

fn strings(n: usize) -> String {
    let mut s = String::from("[");
    for i in 0..n {
        if i > 0 {
            s.push(',');
        }
        s.push_str(&format!(r#""line {i}\twith \"quotes\" and éscapes 한글 텍스트 😀 \n end""#));
    }
    s.push(']');
    s
}

fn best<F: FnMut()>(runs: usize, mut f: F) -> f64 {
    (0..runs)
        .map(|_| {
            let t = Instant::now();
            f();
            t.elapsed().as_secs_f64()
        })
        .fold(f64::INFINITY, f64::min)
}

fn mbps(bytes: usize, secs: f64) -> f64 {
    bytes as f64 / secs / (1024.0 * 1024.0)
}

fn bench_parse(name: &str, text: &str) {
    let bytes = text.as_bytes();
    let dom = best(5, || unsafe {
        let d = wj_parse(bytes.as_ptr(), bytes.len() as i64);
        assert_eq!(wj_ok(d), 1);
        wj_free(d);
    });
    let mut events = 0u64;
    let stream = best(5, || unsafe {
        let r = wj_reader_new(0, 0);
        let mut fed = 0;
        events = 0;
        loop {
            match wj_reader_next(r) {
                0 => {
                    if fed >= bytes.len() {
                        wj_reader_finish(r);
                    } else {
                        let n = (bytes.len() - fed).min(65536);
                        wj_reader_feed(r, bytes[fed..].as_ptr(), n as i64);
                        fed += n;
                    }
                }
                11 => break,
                -1 => panic!("stream error"),
                _ => events += 1,
            }
        }
        wj_reader_free(r);
    });
    println!(
        "{name:<10} {:>7.1} MB   DOM {:>7.1} MB/s   stream {:>7.1} MB/s  ({} events)",
        bytes.len() as f64 / 1048576.0,
        mbps(bytes.len(), dom),
        mbps(bytes.len(), stream),
        events
    );
}

fn bench_lookup() {
    let n = 100_000;
    let mut text = String::from("{");
    for i in 0..n {
        if i > 0 {
            text.push(',');
        }
        text.push_str(&format!("\"key{i}\":{i}"));
    }
    text.push('}');
    let keys: Vec<String> = (0..1_000_000u64).map(|i| format!("key{}", (i * 7919) % n)).collect();
    unsafe {
        let d = wj_parse(text.as_ptr(), text.len() as i64);
        let root = wj_root(d);
        // First lookup builds the index; time steady-state lookups.
        wj_get(d, root, b"key0".as_ptr(), 4);
        let t = best(3, || {
            for k in &keys {
                assert!(wj_get(d, root, k.as_ptr(), k.len() as i64) >= 0);
            }
        });
        // Baseline: what a linear scan over the members costs.
        let sample = &keys[..2000];
        let lin = best(3, || {
            for k in sample {
                let mut found = false;
                for i in (0..wj_len(d, root)).rev() {
                    let mut len = 0;
                    let p = wj_key(d, root, i, &mut len);
                    if std::slice::from_raw_parts(p, len as usize) == k.as_bytes() {
                        found = true;
                        break;
                    }
                }
                assert!(found);
            }
        });
        println!(
            "lookup     100k-key object: indexed {:>6.0} ns/lookup   linear scan {:>9.0} ns/lookup",
            t / keys.len() as f64 * 1e9,
            lin / sample.len() as f64 * 1e9
        );
        wj_free(d);
    }
}

fn bench_edit() {
    unsafe {
        let n = 1_000_000;
        let t = best(3, || {
            let d = wj_new_doc();
            let a = wj_new_array(d);
            let b = wj_new_array(d);
            wj_set_root(d, a);
            wj_arr_push(d, a, b);
            for i in 0..n {
                let s = if i % 2 == 0 { b"12".as_slice() } else { b"-3.5".as_slice() };
                let v = wj_new_number(d, s.as_ptr(), s.len() as i64);
                // Alternate between two arrays to force list relocation.
                let target = if i % 2 == 0 { a } else { b };
                assert_eq!(wj_arr_push(d, target, v), 0);
            }
            wj_free(d);
        });
        println!("edit       {n} interleaved pushes: {:>6.1} ns/push", t / n as f64 * 1e9);
    }
}

fn main() {
    println!("wjson_core {} benchmark (best of runs)", env!("CARGO_PKG_VERSION"));
    bench_parse("records", &records(200_000));
    bench_parse("numbers", &numbers(500_000));
    bench_parse("strings", &strings(200_000));
    bench_lookup();
    bench_edit();
}
