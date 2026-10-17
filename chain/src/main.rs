// SPDX-License-Identifier: MPL-2.0
//! wjlink — wjson's Windows link driver. It replaces the parts of MSVC a
//! Rust + Wave build needs:
//!
//!   wjlink init [DIR] [--lld PATH] [--system-dir DIR] [--arch x64|arm64]
//!       Writes import libraries generated from the system DLLs' export
//!       tables (kernel32.lib, msvcrt.lib, ...), empty stand-ins for MSVC-only
//!       libraries, and the runtime startup object wjrt.obj. DIR defaults to
//!       `lib` next to wjlink.exe.
//!   wjlink implib DLL OUT.lib [--arch A]   one import library
//!   wjlink runtime OUT.obj [--arch A]      only the runtime object
//!   wjlink exports DLL              list a DLL's exports
//!   wjlink lib /OUT:X.lib A.obj...  archive objects into a library
//!   wjlink <link.exe arguments>     link: runs rust-lld with the given
//!                                   arguments plus the generated libraries
//!
//! It is usable as `-C linker=` for rustc/cargo and wavec, since both pass
//! link.exe-style arguments (possibly through an @response file, which
//! rust-lld reads itself).
//!
//! On Windows this program is #![no_std]: it imports only a handful of
//! kernel32/shell32/ntdll functions via raw-dylib and brings its own entry
//! point, so a plain rustup toolchain links it with `-C linker=rust-lld`.

#![cfg_attr(windows, no_std)]
#![cfg_attr(windows, no_main)]
#![cfg_attr(windows, windows_subsystem = "console")]

extern crate alloc;

mod coff;
mod implib;
mod os;
mod pe;
mod runtime;

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

const VERSION: &str = env!("CARGO_PKG_VERSION");

/// DLLs that get import libraries: what Rust's std, Wave's std and the
/// generated runtime import from, plus a few common companions.
const DLLS: &[&str] = &[
    "kernel32", "ntdll", "msvcrt", "ws2_32", "userenv", "dbghelp", "advapi32", "user32", "shell32", "bcrypt",
    "ole32", "oleaut32", "secur32", "iphlpapi", "ucrtbase",
];

/// MSVC-only libraries that toolchains name by default. Their contents come
/// from msvcrt.dll and wjrt.obj instead, so empty archives satisfy them.
const EMPTY: &[&str] = &["vcruntime", "ucrt", "legacy_stdio_definitions", "oldnames", "msvcprt"];

/// Libraries added to every link (unused ones cost nothing).
/// ucrtbase comes last: it only fills in what msvcrt.dll lacks (C99 math
/// such as fma, which Rust's compiler_builtins references).
const ALWAYS: &[&str] =
    &["msvcrt.lib", "kernel32.lib", "ntdll.lib", "ws2_32.lib", "userenv.lib", "advapi32.lib", "ucrtbase.lib"];

fn say(s: &str) {
    os::out(s);
    os::out("\n");
}

fn fail(s: &str) -> i32 {
    os::err("wjlink: ");
    os::err(s);
    os::err("\n");
    1
}

fn usage() -> i32 {
    say(concat!(
        "wjlink ",
        env!("CARGO_PKG_VERSION"),
        " - MSVC-free link driver for wjson\n\n",
        "  wjlink init [DIR] [--lld PATH] [--system-dir DIR] [--arch x64|arm64]\n",
        "  wjlink implib DLL OUT.lib [--arch x64|arm64]\n",
        "  wjlink runtime OUT.obj [--arch x64|arm64]\n",
        "  wjlink exports DLL\n",
        "  wjlink lib /OUT:LIB OBJ...      (archive objects, like lib.exe)\n",
        "  wjlink <link.exe arguments...>\n\n",
        "--arch defaults to the architecture wjlink itself was built for."
    ));
    0
}

/// Removes `--arch NAME` from `args` and returns it (default: the host's).
fn take_arch(args: &mut Vec<String>) -> Result<coff::Arch, String> {
    let mut arch = coff::Arch::host();
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--arch" {
            let Some(name) = args.get(i + 1) else { return Err("--arch needs x64 or arm64".to_string()) };
            arch = coff::Arch::parse(name).ok_or_else(|| format!("unknown architecture {name} (use x64 or arm64)"))?;
            args.drain(i..i + 2);
        } else {
            i += 1;
        }
    }
    Ok(arch)
}

/// Builds the import library for one DLL image.
fn implib_for(dll_path: &str, dll_name: &str, arch: coff::Arch) -> Result<(Vec<u8>, usize), String> {
    let image = os::read(dll_path)?;
    let table = pe::read_exports(&image).map_err(|e| format!("{dll_path}: {e}"))?;
    let imports: Vec<implib::Import> = table
        .exports
        .into_iter()
        .filter(|e| !runtime::DEFINES.contains(&e.name.as_str()))
        .map(|e| implib::Import { name: e.name, hint: e.hint, data: e.data })
        .collect();
    let count = imports.len();
    Ok((implib::import_library(arch, dll_name, &imports), count))
}

fn lib_dir() -> String {
    os::env("WJLINK_LIB").unwrap_or_else(|| os::join_path(&os::exe_dir(), "lib"))
}

fn cmd_init(args: &[String]) -> i32 {
    let mut owned = args.to_vec();
    let arch = match take_arch(&mut owned) {
        Ok(a) => a,
        Err(e) => return fail(&format!("init: {e}")),
    };
    let args = &owned[..];
    let mut dir = None;
    let mut lld = None;
    let mut system = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--lld" if i + 1 < args.len() => {
                lld = Some(args[i + 1].clone());
                i += 1;
            }
            "--system-dir" if i + 1 < args.len() => {
                system = Some(args[i + 1].clone());
                i += 1;
            }
            other if dir.is_none() && !other.starts_with("--") => dir = Some(other.to_string()),
            other => return fail(&format!("init: unexpected argument {other}")),
        }
        i += 1;
    }
    let dir = dir.unwrap_or_else(lib_dir);
    let system = system.unwrap_or_else(os::system_dir);
    if system.is_empty() {
        return fail("init: pass --system-dir (the folder with kernel32.dll etc.)");
    }
    if let Err(e) = os::mkdir_all(&dir) {
        return fail(&e);
    }

    let mut made = 0;
    for name in DLLS {
        let dll = format!("{name}.dll");
        let path = os::join_path(&system, &dll);
        if !os::exists(&path) {
            say(&format!("  {name}.lib: skipped ({dll} not found)"));
            continue;
        }
        match implib_for(&path, &dll, arch) {
            Ok((bytes, count)) => {
                if let Err(e) = os::write(&os::join_path(&dir, &format!("{name}.lib")), &bytes) {
                    return fail(&e);
                }
                say(&format!("  {name}.lib: {count} imports from {dll}"));
                made += 1;
            }
            Err(e) => return fail(&e),
        }
    }
    for name in EMPTY {
        if let Err(e) = os::write(&os::join_path(&dir, &format!("{name}.lib")), &implib::empty_library()) {
            return fail(&e);
        }
    }
    if let Err(e) = os::write(&os::join_path(&dir, "wjrt.obj"), &runtime::runtime_object(arch)) {
        return fail(&e);
    }
    if let Some(lld) = lld
        && let Err(e) = os::write(&os::join_path(&dir, "lld.txt"), lld.as_bytes())
    {
        return fail(&e);
    }
    say(&format!("  wjrt.obj: runtime startup object; {} empty stand-ins", EMPTY.len()));
    say(&format!("wjlink: {made} import libraries ({}) in {dir}", arch.name()));
    if made == 0 {
        return fail("init: no system DLLs found");
    }
    0
}

/// `lib /OUT:X.lib a.obj b.obj ...`: a static library, like lib.exe.
fn cmd_lib(args: &[String]) -> i32 {
    let mut out = None;
    let mut inputs = Vec::new();
    for a in args {
        let lower = a.to_ascii_lowercase();
        if lower.starts_with("/out:") || lower.starts_with("-out:") {
            out = Some(a[5..].to_string());
        } else if lower == "/nologo" || lower == "-nologo" {
        } else if a.starts_with('/') && !a[1..].contains(['/', '\\', '.']) {
            return fail(&format!("lib: unsupported option {a}"));
        } else {
            inputs.push(a.clone());
        }
    }
    let Some(out) = out else { return fail("lib: missing /OUT:") };
    let mut objects = Vec::new();
    for path in &inputs {
        let data = match os::read(path) {
            Ok(d) => d,
            Err(e) => return fail(&e),
        };
        let syms = match implib::object_symbols(&data) {
            Ok(s) => s,
            Err(e) => return fail(&format!("{path}: {e}")),
        };
        let name = path.rsplit(['/', '\\']).next().unwrap_or(path).to_string();
        objects.push((name, data, syms));
    }
    let members: Vec<implib::ArchiveMember> =
        objects.iter().map(|(n, d, s)| (n.as_str(), d.as_slice(), s.as_slice())).collect();
    match os::write(&out, &implib::archive(&members)) {
        Ok(()) => 0,
        Err(e) => fail(&e),
    }
}

fn find_lld(dir: &str) -> Result<String, String> {
    let mut lld = os::env("WJLINK_LLD");
    if lld.is_none()
        && let Ok(bytes) = os::read(&os::join_path(dir, "lld.txt"))
    {
        let s = String::from_utf8_lossy(&bytes).trim().to_string();
        if !s.is_empty() {
            lld = Some(s);
        }
    }
    let lld = lld.unwrap_or_else(|| "rust-lld".into());
    // A bare name would be looked up by the OS, and on Windows that search
    // starts in the current directory: a rust-lld.exe planted in the project
    // being built would run instead of the real one. Resolve it ourselves,
    // through PATH only.
    if lld.contains(['/', '\\']) {
        return Ok(lld);
    }
    let file = if cfg!(windows) && !lld.to_ascii_lowercase().ends_with(".exe") { format!("{lld}.exe") } else { lld.clone() };
    os::find_in_path(&file).ok_or_else(|| {
        format!("{lld} not found on PATH (the current directory is not searched); set WJLINK_LLD or run `wjlink init --lld <rust-lld path>`")
    })
}

fn cmd_link(args: &[String]) -> i32 {
    let dir = lib_dir();
    let runtime = os::join_path(&dir, "wjrt.obj");
    if !os::exists(&runtime) {
        return fail(&format!("{runtime} is missing; run `wjlink init` first"));
    }
    let lld = match find_lld(&dir) {
        Ok(p) => p,
        Err(e) => return fail(&e),
    };
    // The runtime object goes first so its definitions win over same-named
    // DLL exports that a library earlier on the line could otherwise supply.
    let mut full: Vec<String> = vec!["-flavor".into(), "link".into(), runtime];
    full.extend(args.iter().cloned());
    full.push(format!("/LIBPATH:{dir}"));
    full.extend(ALWAYS.iter().filter(|l| os::exists(&os::join_path(&dir, l))).map(|s| s.to_string()));

    // Stay well below the 32767-character command-line limit.
    let length: usize = full.iter().map(|a| a.len() + 3).sum();
    let mut response_file = None;
    if length > 30_000 {
        let mut rsp = String::new();
        for a in &full[2..] {
            os::quote_arg(a, &mut rsp);
            rsp.push('\n');
        }
        let path = os::join_path(&dir, &format!("link-{}.rsp", os::pid()));
        if let Err(e) = os::write(&path, rsp.as_bytes()) {
            return fail(&e);
        }
        full.truncate(2);
        full.push(format!("@{path}"));
        response_file = Some(path);
    }
    let result = os::run(&lld, &full);
    // The response file only exists for this one link; don't leave one behind
    // per build.
    if let Some(path) = response_file {
        os::remove(&path);
    }
    match result {
        Ok(code) => code,
        Err(e) => fail(&format!("{e}; set WJLINK_LLD or run `wjlink init --lld <rust-lld path>`")),
    }
}

fn real_main(args: Vec<String>) -> i32 {
    let Some(first) = args.first() else { return usage() };
    match first.as_str() {
        "help" | "--help" | "-h" => usage(),
        "--version" | "version" => {
            say(&format!("wjlink {VERSION}"));
            0
        }
        "init" => cmd_init(&args[1..]),
        "implib" if args.len() >= 3 => {
            let mut rest = args.clone();
            let arch = match take_arch(&mut rest) {
                Ok(a) => a,
                Err(e) => return fail(&format!("implib: {e}")),
            };
            if rest.len() != 3 {
                return usage();
            }
            let dll = rest[1].rsplit(['/', '\\']).next().unwrap_or(&rest[1]).to_string();
            match implib_for(&rest[1], &dll, arch) {
                Ok((bytes, count)) => match os::write(&rest[2], &bytes) {
                    Ok(()) => {
                        say(&format!("{}: {count} imports ({})", rest[2], arch.name()));
                        0
                    }
                    Err(e) => fail(&e),
                },
                Err(e) => fail(&e),
            }
        }
        "lib" => cmd_lib(&args[1..]),
        "runtime" if args.len() >= 2 => {
            let mut rest = args.clone();
            let arch = match take_arch(&mut rest) {
                Ok(a) => a,
                Err(e) => return fail(&format!("runtime: {e}")),
            };
            if rest.len() != 2 {
                return usage();
            }
            match os::write(&rest[1], &runtime::runtime_object(arch)) {
                Ok(()) => 0,
                Err(e) => fail(&e),
            }
        }
        "exports" if args.len() == 2 => match os::read(&args[1]).map(|img| pe::read_exports(&img)) {
            Ok(Ok(t)) => {
                say(&format!("{} ({} exports)", t.dll, t.exports.len()));
                for e in t.exports {
                    say(&format!("{}{}", e.name, if e.data { "  [data]" } else { "" }));
                }
                0
            }
            Ok(Err(e)) => fail(e),
            Err(e) => fail(&e),
        },
        _ => cmd_link(&args),
    }
}

#[cfg(not(windows))]
fn main() {
    let code = real_main(os::args());
    os::exit(code);
}

// ---- Windows: the bits a C runtime would normally provide -----------------------

#[cfg(windows)]
mod rt {
    use core::alloc::{GlobalAlloc, Layout};
    use core::ffi::{c_char, c_void};

    #[link(name = "kernel32", kind = "raw-dylib")]
    unsafe extern "system" {
        fn GetProcessHeap() -> isize;
        fn HeapAlloc(heap: isize, flags: u32, size: usize) -> *mut u8;
        fn HeapReAlloc(heap: isize, flags: u32, mem: *mut u8, size: usize) -> *mut u8;
        fn HeapFree(heap: isize, flags: u32, mem: *mut u8) -> i32;
    }

    // The compiler lowers copies, fills and string scans to these C
    // functions; ntdll exports them. Declaring them is enough for rustc to
    // put them in the import table it generates.
    #[allow(dead_code)]
    #[link(name = "ntdll", kind = "raw-dylib")]
    unsafe extern "C" {
        fn memcpy(dst: *mut c_void, src: *const c_void, n: usize) -> *mut c_void;
        fn memmove(dst: *mut c_void, src: *const c_void, n: usize) -> *mut c_void;
        fn memset(dst: *mut c_void, c: i32, n: usize) -> *mut c_void;
        fn memcmp(a: *const c_void, b: *const c_void, n: usize) -> i32;
        fn strlen(s: *const c_char) -> usize;
        fn wcslen(s: *const u16) -> usize;
    }

    struct Heap;

    // HeapAlloc returns 16-byte aligned blocks on x64, which covers every
    // type wjlink allocates.
    unsafe impl GlobalAlloc for Heap {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            if layout.align() > 16 {
                return core::ptr::null_mut();
            }
            // SAFETY: the process heap handle is always valid.
            unsafe { HeapAlloc(GetProcessHeap(), 0, layout.size()) }
        }
        unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
            if layout.align() > 16 {
                return core::ptr::null_mut();
            }
            // SAFETY: as above; flag 8 is HEAP_ZERO_MEMORY.
            unsafe { HeapAlloc(GetProcessHeap(), 8, layout.size()) }
        }
        unsafe fn dealloc(&self, ptr: *mut u8, _layout: Layout) {
            // SAFETY: GlobalAlloc's contract: `ptr` came from this allocator.
            unsafe {
                HeapFree(GetProcessHeap(), 0, ptr);
            }
        }
        unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
            if layout.align() > 16 {
                return core::ptr::null_mut();
            }
            // SAFETY: GlobalAlloc's contract: `ptr` came from this allocator.
            unsafe { HeapReAlloc(GetProcessHeap(), 0, ptr, size) }
        }
    }

    #[global_allocator]
    static HEAP: Heap = Heap;

    #[panic_handler]
    fn panic(_: &core::panic::PanicInfo) -> ! {
        crate::os::err("wjlink: internal error\n");
        crate::os::exit(101)
    }

    /// Entry point: rust-lld picks `mainCRTStartup` for console programs.
    #[unsafe(no_mangle)]
    pub extern "C" fn mainCRTStartup() -> ! {
        let code = crate::real_main(crate::os::args());
        crate::os::exit(code)
    }

    // Referenced by precompiled core/alloc objects: the float marker and the
    // C++ exception personality (never called: panics abort).
    #[unsafe(no_mangle)]
    #[used]
    pub static _fltused: i32 = 0;

    #[unsafe(no_mangle)]
    pub extern "C" fn __CxxFrameHandler3() -> i32 {
        1 // ExceptionContinueSearch
    }

    // Stack probe for frames over 4 KiB (same code as in wjrt.obj).
    #[cfg(target_arch = "aarch64")]
    core::arch::global_asm!(
        ".globl __chkstk",
        "__chkstk:",
        "lsl x16, x15, #4",
        "mov x17, sp",
        "2:",
        "sub x17, x17, #4096",
        "subs x16, x16, #4096",
        "ldr xzr, [x17]",
        "b.gt 2b",
        "ret",
    );

    #[cfg(target_arch = "x86_64")]
    core::arch::global_asm!(
        ".globl __chkstk",
        "__chkstk:",
        "push rcx",
        "push rax",
        "cmp rax, 4096",
        "lea rcx, [rsp + 24]",
        "jb 3f",
        "2:",
        "sub rcx, 4096",
        "test qword ptr [rcx], rcx",
        "sub rax, 4096",
        "cmp rax, 4096",
        "ja 2b",
        "3:",
        "sub rcx, rax",
        "test qword ptr [rcx], rcx",
        "pop rax",
        "pop rcx",
        "ret",
    );
}

#[cfg(test)]
mod tests;
