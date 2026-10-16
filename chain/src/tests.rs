// SPDX-License-Identifier: MPL-2.0
//! Host-side tests (they run with std on the development machine).

extern crate std;

use crate::*;
use std::string::String;
use std::vec::Vec;

fn u16_at(b: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([b[at], b[at + 1]])
}
fn u32_at(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(b[at..at + 4].try_into().unwrap())
}

/// Symbol names of a COFF object, resolving long names.
fn coff_symbols(obj: &[u8]) -> Vec<(String, i16, u8)> {
    assert_eq!(u16_at(obj, 0), 0x8664);
    let symtab = u32_at(obj, 8) as usize;
    let count = u32_at(obj, 12) as usize;
    let strtab = symtab + 18 * count;
    let mut out = Vec::new();
    for i in 0..count {
        let s = &obj[symtab + 18 * i..symtab + 18 * (i + 1)];
        let name = if u32_at(s, 0) == 0 {
            let off = strtab + u32_at(s, 4) as usize;
            let end = obj[off..].iter().position(|&b| b == 0).unwrap();
            String::from_utf8(obj[off..off + end].to_vec()).unwrap()
        } else {
            let end = s[..8].iter().position(|&b| b == 0).unwrap_or(8);
            String::from_utf8(s[..end].to_vec()).unwrap()
        };
        out.push((name, u16_at(s, 12) as i16, s[16]));
    }
    out
}

/// Members of a COFF archive: (name field, data), skipping the linker and
/// longnames members; also returns the second linker member's symbols.
type ParsedArchive = (Vec<Vec<u8>>, Vec<(String, u16)>, Vec<u32>);

fn archive(lib: &[u8]) -> ParsedArchive {
    assert_eq!(&lib[..8], b"!<arch>\n");
    let mut at = 8;
    let mut members = Vec::new();
    let mut seen_linker = 0;
    let mut second = Vec::new();
    let mut offsets = Vec::new();
    while at < lib.len() {
        let name = std::str::from_utf8(&lib[at..at + 16]).unwrap().trim_end().to_string();
        let size: usize = std::str::from_utf8(&lib[at + 48..at + 58]).unwrap().trim().parse().unwrap();
        assert_eq!(&lib[at + 58..at + 60], b"`\n");
        let data = &lib[at + 60..at + 60 + size];
        if name == "/" {
            seen_linker += 1;
            if seen_linker == 2 {
                let m = u32_at(data, 0) as usize;
                offsets = (0..m).map(|i| u32_at(data, 4 + 4 * i)).collect();
                let n = u32_at(data, 4 + 4 * m) as usize;
                let idx_at = 8 + 4 * m;
                let mut names_at = idx_at + 2 * n;
                for k in 0..n {
                    let end = data[names_at..].iter().position(|&b| b == 0).unwrap();
                    let s = String::from_utf8(data[names_at..names_at + end].to_vec()).unwrap();
                    second.push((s, u16_at(data, idx_at + 2 * k)));
                    names_at += end + 1;
                }
            }
        } else if name != "//" {
            members.push(data.to_vec());
        }
        at += 60 + size + (size & 1);
    }
    (members, second, offsets)
}

fn imports() -> Vec<implib::Import> {
    vec![
        implib::Import { name: "printf".into(), hint: 3, data: false },
        implib::Import { name: "exit".into(), hint: 1, data: false },
        implib::Import { name: "_environ".into(), hint: 2, data: true },
    ]
}

#[test]
fn import_library_layout() {
    let lib = implib::import_library("msvcrt.dll", &imports());
    let (members, symbols, offsets) = archive(&lib);
    assert_eq!(members.len(), 6, "descriptor, null descriptor, null thunk, 3 imports");
    assert_eq!(offsets.len(), 6);

    let names: Vec<&str> = symbols.iter().map(|(s, _)| s.as_str()).collect();
    let mut sorted = names.clone();
    sorted.sort();
    assert_eq!(names, sorted, "second linker member is sorted");
    for want in [
        "__IMPORT_DESCRIPTOR_msvcrt",
        "__NULL_IMPORT_DESCRIPTOR",
        "\x7fmsvcrt_NULL_THUNK_DATA",
        "__imp_printf",
        "printf",
        "__imp_exit",
        "exit",
        "__imp__environ",
    ] {
        assert!(names.contains(&want), "{want:?} missing from {names:?}");
    }
    assert!(!names.contains(&"_environ"), "data imports get no thunk symbol");

    // Every index points at the member that defines the symbol.
    for (sym, index) in &symbols {
        let off = offsets[*index as usize - 1] as usize;
        let size: usize = std::str::from_utf8(&lib[off + 48..off + 58]).unwrap().trim().parse().unwrap();
        let data = &lib[off + 60..off + 60 + size];
        if u16_at(data, 0) == 0 && u16_at(data, 2) == 0xFFFF {
            let name_end = data[20..].iter().position(|&b| b == 0).unwrap();
            let name = std::str::from_utf8(&data[20..20 + name_end]).unwrap();
            assert!(sym == name || *sym == std::format!("__imp_{name}"), "{sym} -> import {name}");
            let dll = &data[20 + name_end + 1..data.len() - 1];
            assert_eq!(dll, b"msvcrt.dll");
            let kind = u16_at(data, 18) & 3;
            assert_eq!(kind == 1, name == "_environ");
            assert_eq!(u16_at(data, 18) >> 2 & 7, 1, "import by name");
        } else {
            let defined: Vec<String> =
                coff_symbols(data).into_iter().filter(|(_, sec, class)| *sec > 0 && *class == 2).map(|s| s.0).collect();
            assert!(defined.contains(sym), "{sym} not defined by its member ({defined:?})");
        }
    }

    // The descriptor references the null descriptor and the null thunk.
    let desc = coff_symbols(&members[0]);
    assert!(desc.iter().any(|(n, s, _)| n == "__NULL_IMPORT_DESCRIPTOR" && *s == 0));
    assert!(desc.iter().any(|(n, s, _)| n == "\x7fmsvcrt_NULL_THUNK_DATA" && *s == 0));
}

#[test]
fn long_dll_names_use_the_longnames_member() {
    let lib = implib::import_library("bcryptprimitives.dll", &imports());
    assert!(lib.windows(4).any(|w| w == b"//  "), "has a // member");
    let (members, symbols, _) = archive(&lib);
    assert_eq!(members.len(), 6);
    assert!(symbols.iter().any(|(s, _)| s == "__IMPORT_DESCRIPTOR_bcryptprimitives"));
}

#[test]
fn runtime_object_symbols() {
    let obj = runtime::runtime_object();
    let syms = coff_symbols(&obj);
    let defined = |n: &str| syms.iter().any(|(s, sec, class)| s == n && *sec > 0 && *class == 2);
    for name in ["mainCRTStartup", "__chkstk", "_tls_used", "_tls_index", "_fltused", "??_7type_info@@6B@"] {
        assert!(defined(name), "{name}");
    }
    assert!(syms.iter().any(|(s, sec, _)| s == "main" && *sec == 0));
    assert!(syms.iter().any(|(s, sec, _)| s == "__imp_exit" && *sec == 0));
    // Section names, in order.
    let n = u16_at(&obj, 2) as usize;
    let names: Vec<String> = (0..n)
        .map(|i| {
            let h = &obj[20 + 40 * i..20 + 40 * i + 8];
            String::from_utf8(h.iter().take_while(|&&b| b != 0).copied().collect()).unwrap()
        })
        .collect();
    assert_eq!(names, [".text", ".data", ".rdata$T", ".tls", ".tls$ZZZ", ".CRT$XLA", ".CRT$XLZ"]);
}

#[test]
fn quoting_matches_windows_rules() {
    let q = |s: &str| {
        let mut o = String::new();
        os::quote_arg(s, &mut o);
        o
    };
    assert_eq!(q("plain"), "plain");
    assert_eq!(q(""), "\"\"");
    assert_eq!(q("a b"), "\"a b\"");
    assert_eq!(q("C:\\Program Files\\x\\"), "\"C:\\Program Files\\x\\\\\"");
    assert_eq!(q("say \"hi\""), "\"say \\\"hi\\\"\"");
    assert_eq!(q("a\\\\\"b"), "\"a\\\\\\\\\\\"b\"");
    assert_eq!(q("/LIBPATH:C:\\x"), "/LIBPATH:C:\\x");
}

/// Reads real Windows DLLs when WJLINK_TEST_DLLS points at a folder of them.
#[test]
fn real_dll_exports() {
    let Ok(dir) = std::env::var("WJLINK_TEST_DLLS") else {
        std::eprintln!("WJLINK_TEST_DLLS not set; skipping");
        return;
    };
    let read = |name: &str| pe::read_exports(&std::fs::read(std::format!("{dir}/{name}")).unwrap()).unwrap();
    let msvcrt = read("msvcrt.dll");
    assert_eq!(msvcrt.dll.to_ascii_lowercase(), "msvcrt.dll");
    let find = |t: &pe::Exports, n: &str| t.exports.iter().find(|e| e.name == n).cloned();
    for f in ["printf", "fflush", "getenv", "system", "exit", "memcpy", "__CxxFrameHandler3", "_CxxThrowException"] {
        assert_eq!(find(&msvcrt, f).map(|e| e.data), Some(false), "{f}");
    }
    assert_eq!(find(&msvcrt, "_iob").map(|e| e.data), Some(true), "_iob is data");
    let kernel32 = read("kernel32.dll");
    // HeapAlloc is forwarded to ntdll; it must still import like code.
    assert_eq!(find(&kernel32, "HeapAlloc").map(|e| e.data), Some(false));
    assert!(find(&kernel32, "CreateFileW").is_some());
    // kernel32 exports __chkstk, which wjrt.obj defines; generated libraries skip it.
    assert!(find(&kernel32, "__chkstk").is_some());
    let (lib, _) = crate::implib_for(&std::format!("{dir}/kernel32.dll"), "kernel32.dll").unwrap();
    let (members, symbols, _) = archive(&lib);
    assert!(members.len() > 1000);
    assert!(!symbols.iter().any(|(s, _)| s == "__chkstk" || s == "__imp___chkstk"));
    // Hints are indexes into the (sorted) name table.
    for w in kernel32.exports.windows(2) {
        assert!(w[0].name < w[1].name && w[0].hint + 1 == w[1].hint);
    }
}

/// A minimal PE32+ image whose export directory claims `nnames` names.
fn pe_with_exports(nnames: u32, section_va: u32, section_size: u32) -> Vec<u8> {
    let mut img = vec![0u8; 0x800];
    img[..2].copy_from_slice(b"MZ");
    img[0x3C..0x40].copy_from_slice(&0x80u32.to_le_bytes());
    img[0x80..0x84].copy_from_slice(b"PE\0\0");
    let coff = 0x84;
    img[coff + 2..coff + 4].copy_from_slice(&1u16.to_le_bytes()); // one section
    img[coff + 16..coff + 18].copy_from_slice(&240u16.to_le_bytes()); // optional header size
    let opt = coff + 20;
    img[opt..opt + 2].copy_from_slice(&0x20Bu16.to_le_bytes());
    let dirs = opt + 112;
    img[dirs..dirs + 4].copy_from_slice(&section_va.to_le_bytes()); // export RVA
    img[dirs + 4..dirs + 8].copy_from_slice(&0x100u32.to_le_bytes());
    let sec = opt + 240;
    img[sec + 8..sec + 12].copy_from_slice(&section_size.to_le_bytes());
    img[sec + 12..sec + 16].copy_from_slice(&section_va.to_le_bytes());
    img[sec + 16..sec + 20].copy_from_slice(&0x300u32.to_le_bytes()); // raw size
    img[sec + 20..sec + 24].copy_from_slice(&0x400u32.to_le_bytes()); // raw offset
    let dir = 0x400;
    img[dir + 20..dir + 24].copy_from_slice(&1u32.to_le_bytes()); // one function
    img[dir + 24..dir + 28].copy_from_slice(&nnames.to_le_bytes());
    for (field, at) in [(28, 0x40u32), (32, 0x50), (36, 0x60)] {
        img[dir + field..dir + field + 4].copy_from_slice(&(section_va + at).to_le_bytes());
    }
    img
}

#[test]
fn hostile_pe_images_fail_cleanly() {
    // A header that promises billions of names must not size an allocation
    // (that used to abort the process); it is just a truncated table.
    let huge = pe_with_exports(0xFFFF_FFF0, 0x1000, 0x1000);
    assert!(pe::read_exports(&huge).is_err());

    // RVAs that would wrap around u32 are rejected or ignored, not wrapped
    // into a valid-looking offset (this panicked in debug builds).
    let wrapping = pe_with_exports(1, 0xFFFF_FF00, 0xFFFF_FFFF);
    let _ = pe::read_exports(&wrapping);

    // Every truncation of a valid-looking image is an error or an empty
    // result, never a panic.
    let image = pe_with_exports(2, 0x1000, 0x1000);
    for cut in 0..image.len() {
        let _ = pe::read_exports(&image[..cut]);
    }
    // Arbitrary garbage behind the headers, too.
    let mut noisy = image.clone();
    let mut x: u32 = 0x1234_5678;
    for round in 0..200 {
        for b in noisy[0x80..].iter_mut() {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            if x.is_multiple_of(7) {
                *b = (x >> 8) as u8;
            }
        }
        let _ = pe::read_exports(&noisy);
        if round % 50 == 0 {
            noisy = image.clone();
        }
    }
}

#[test]
fn tool_lookup_ignores_the_current_directory() {
    let exists = |p: &str| p == "/opt/llvm/bin/rust-lld" || p == "./rust-lld" || p == "rust-lld" || p == "tools/rust-lld";
    // "." , "" and relative entries never win, even though a file would be found there.
    let path = ".::tools:/usr/bin:/opt/llvm/bin";
    if cfg!(windows) {
        return;
    }
    assert_eq!(os::search_path(path, "rust-lld", &exists).as_deref(), Some("/opt/llvm/bin/rust-lld"));
    assert_eq!(os::search_path(".:tools", "rust-lld", &exists), None);
    assert_eq!(os::search_path("", "rust-lld", &exists), None);
}
