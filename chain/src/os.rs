// SPDX-License-Identifier: MIT
//! The little operating-system surface wjlink needs.
//!
//! Windows: direct Win32 calls declared with `raw-dylib`, so rustc writes the
//! import entries itself and the binary links with nothing but rust-lld.
//! Elsewhere (development and tests): Rust's standard library.

use alloc::string::String;
use alloc::vec::Vec;

pub use imp::*;

/// Quotes one argument for a Windows command line (the rules
/// CommandLineToArgvW and the MSVC runtime use to split it again).
pub fn quote_arg(arg: &str, out: &mut String) {
    let needs = arg.is_empty() || arg.contains([' ', '\t', '"', '\n']);
    if !needs {
        out.push_str(arg);
        return;
    }
    out.push('"');
    let mut backslashes = 0;
    for c in arg.chars() {
        match c {
            '\\' => backslashes += 1,
            '"' => {
                for _ in 0..backslashes * 2 + 1 {
                    out.push('\\');
                }
                backslashes = 0;
                out.push('"');
            }
            _ => {
                for _ in 0..backslashes {
                    out.push('\\');
                }
                backslashes = 0;
                out.push(c);
            }
        }
    }
    for _ in 0..backslashes * 2 {
        out.push('\\');
    }
    out.push('"');
}

pub fn join_path(dir: &str, name: &str) -> String {
    let mut s = String::from(dir);
    if !s.is_empty() && !s.ends_with(['/', '\\']) {
        s.push(if cfg!(windows) { '\\' } else { '/' });
    }
    s.push_str(name);
    s
}

#[allow(dead_code)]
fn parent(path: &str) -> &str {
    match path.rfind(['/', '\\']) {
        Some(i) => &path[..i],
        None => ".",
    }
}

#[cfg(not(windows))]
mod imp {
    use super::*;
    use alloc::format;
    use alloc::string::ToString;
    extern crate std;

    pub fn args() -> Vec<String> {
        std::env::args().skip(1).collect()
    }
    pub fn read(path: &str) -> Result<Vec<u8>, String> {
        std::fs::read(path).map_err(|e| format!("cannot read {path}: {e}"))
    }
    pub fn write(path: &str, data: &[u8]) -> Result<(), String> {
        std::fs::write(path, data).map_err(|e| format!("cannot write {path}: {e}"))
    }
    pub fn mkdir_all(path: &str) -> Result<(), String> {
        std::fs::create_dir_all(path).map_err(|e| format!("cannot create {path}: {e}"))
    }
    pub fn exists(path: &str) -> bool {
        std::path::Path::new(path).exists()
    }
    pub fn exe_dir() -> String {
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.display().to_string()))
            .unwrap_or_else(|| ".".into())
    }
    pub fn env(name: &str) -> Option<String> {
        std::env::var(name).ok().filter(|v| !v.is_empty())
    }
    /// Not meaningful off Windows; callers pass --system-dir.
    pub fn system_dir() -> String {
        String::new()
    }
    pub fn pid() -> u32 {
        std::process::id()
    }
    pub fn run(program: &str, args: &[String]) -> Result<i32, String> {
        let status = std::process::Command::new(program)
            .args(args)
            .status()
            .map_err(|e| format!("cannot run {program}: {e}"))?;
        Ok(status.code().unwrap_or(1))
    }
    pub fn out(s: &str) {
        std::print!("{s}");
    }
    pub fn err(s: &str) {
        std::eprint!("{s}");
    }
    pub fn exit(code: i32) -> ! {
        std::process::exit(code)
    }
}

#[cfg(windows)]
mod imp {
    use super::*;
    use alloc::format;
    use core::ffi::c_void;
    use core::ptr::{null, null_mut};

    type Handle = isize;
    const INVALID_HANDLE: Handle = -1;
    const GENERIC_READ: u32 = 0x8000_0000;
    const GENERIC_WRITE: u32 = 0x4000_0000;
    const FILE_SHARE_READ: u32 = 1;
    const FILE_SHARE_DELETE: u32 = 4;
    const OPEN_EXISTING: u32 = 3;
    const CREATE_ALWAYS: u32 = 2;
    const FILE_ATTRIBUTE_NORMAL: u32 = 0x80;
    const INFINITE: u32 = 0xFFFF_FFFF;
    const INVALID_FILE_ATTRIBUTES: u32 = 0xFFFF_FFFF;

    #[link(name = "kernel32", kind = "raw-dylib")]
    extern "system" {
        fn GetCommandLineW() -> *const u16;
        fn LocalFree(mem: *mut c_void) -> *mut c_void;
        fn CreateFileW(
            name: *const u16,
            access: u32,
            share: u32,
            security: *mut c_void,
            disposition: u32,
            flags: u32,
            template: Handle,
        ) -> Handle;
        fn ReadFile(file: Handle, buf: *mut u8, len: u32, read: *mut u32, overlapped: *mut c_void) -> i32;
        fn WriteFile(file: Handle, buf: *const u8, len: u32, written: *mut u32, overlapped: *mut c_void) -> i32;
        fn CloseHandle(handle: Handle) -> i32;
        fn GetFileSizeEx(file: Handle, size: *mut i64) -> i32;
        fn CreateDirectoryW(name: *const u16, security: *mut c_void) -> i32;
        fn GetFileAttributesW(name: *const u16) -> u32;
        fn GetStdHandle(which: u32) -> Handle;
        fn GetModuleFileNameW(module: *mut c_void, buf: *mut u16, len: u32) -> u32;
        fn GetEnvironmentVariableW(name: *const u16, buf: *mut u16, len: u32) -> u32;
        fn GetSystemDirectoryW(buf: *mut u16, len: u32) -> u32;
        fn GetCurrentProcessId() -> u32;
        fn GetLastError() -> u32;
        fn CreateProcessW(
            app: *const u16,
            command: *mut u16,
            process_security: *mut c_void,
            thread_security: *mut c_void,
            inherit: i32,
            flags: u32,
            environment: *mut c_void,
            directory: *const u16,
            startup: *mut u64,
            info: *mut u64,
        ) -> i32;
        fn WaitForSingleObject(handle: Handle, ms: u32) -> u32;
        fn GetExitCodeProcess(process: Handle, code: *mut u32) -> i32;
        fn ExitProcess(code: u32) -> !;
    }

    #[link(name = "shell32", kind = "raw-dylib")]
    extern "system" {
        fn CommandLineToArgvW(command: *const u16, count: *mut i32) -> *mut *mut u16;
    }

    fn wide(s: &str) -> Vec<u16> {
        let mut v: Vec<u16> = s.encode_utf16().collect();
        v.push(0);
        v
    }

    fn from_wide(p: *const u16) -> String {
        let mut n = 0;
        unsafe {
            while *p.add(n) != 0 {
                n += 1;
            }
            String::from_utf16_lossy(core::slice::from_raw_parts(p, n))
        }
    }

    pub fn args() -> Vec<String> {
        let mut out = Vec::new();
        unsafe {
            let mut n = 0;
            let argv = CommandLineToArgvW(GetCommandLineW(), &mut n);
            if argv.is_null() {
                return out;
            }
            for i in 1..n as usize {
                out.push(from_wide(*argv.add(i)));
            }
            LocalFree(argv as *mut c_void);
        }
        out
    }

    pub fn read(path: &str) -> Result<Vec<u8>, String> {
        let name = wide(path);
        unsafe {
            let h = CreateFileW(
                name.as_ptr(),
                GENERIC_READ,
                FILE_SHARE_READ | FILE_SHARE_DELETE,
                null_mut(),
                OPEN_EXISTING,
                FILE_ATTRIBUTE_NORMAL,
                0,
            );
            if h == INVALID_HANDLE {
                return Err(format!("cannot open {path} (error {})", GetLastError()));
            }
            let mut size = 0i64;
            GetFileSizeEx(h, &mut size);
            let mut data: Vec<u8> = Vec::with_capacity(size.max(0) as usize);
            let mut done = 0usize;
            while done < size as usize {
                let want = ((size as usize - done).min(1 << 30)) as u32;
                let mut got = 0u32;
                if ReadFile(h, data.as_mut_ptr().add(done), want, &mut got, null_mut()) == 0 || got == 0 {
                    break;
                }
                done += got as usize;
            }
            data.set_len(done);
            CloseHandle(h);
            Ok(data)
        }
    }

    pub fn write(path: &str, data: &[u8]) -> Result<(), String> {
        let name = wide(path);
        unsafe {
            let h = CreateFileW(name.as_ptr(), GENERIC_WRITE, 0, null_mut(), CREATE_ALWAYS, FILE_ATTRIBUTE_NORMAL, 0);
            if h == INVALID_HANDLE {
                return Err(format!("cannot create {path} (error {})", GetLastError()));
            }
            let mut done = 0usize;
            while done < data.len() {
                let want = ((data.len() - done).min(1 << 30)) as u32;
                let mut put = 0u32;
                if WriteFile(h, data.as_ptr().add(done), want, &mut put, null_mut()) == 0 {
                    CloseHandle(h);
                    return Err(format!("cannot write {path} (error {})", GetLastError()));
                }
                done += put as usize;
            }
            CloseHandle(h);
            Ok(())
        }
    }

    pub fn exists(path: &str) -> bool {
        let name = wide(path);
        unsafe { GetFileAttributesW(name.as_ptr()) != INVALID_FILE_ATTRIBUTES }
    }

    pub fn mkdir_all(path: &str) -> Result<(), String> {
        if path.is_empty() || exists(path) {
            return Ok(());
        }
        let up = parent(path);
        if up != path && up != "." && !up.ends_with(':') {
            mkdir_all(up)?;
        }
        let name = wide(path);
        unsafe {
            if CreateDirectoryW(name.as_ptr(), null_mut()) == 0 && !exists(path) {
                return Err(format!("cannot create {path} (error {})", GetLastError()));
            }
        }
        Ok(())
    }

    pub fn exe_dir() -> String {
        let mut buf = [0u16; 1024];
        let n = unsafe { GetModuleFileNameW(null_mut(), buf.as_mut_ptr(), buf.len() as u32) } as usize;
        let path = String::from_utf16_lossy(&buf[..n]);
        String::from(parent(&path))
    }

    pub fn env(name: &str) -> Option<String> {
        let key = wide(name);
        let mut buf = [0u16; 2048];
        let n = unsafe { GetEnvironmentVariableW(key.as_ptr(), buf.as_mut_ptr(), buf.len() as u32) } as usize;
        (n > 0 && n < buf.len()).then(|| String::from_utf16_lossy(&buf[..n]))
    }

    pub fn system_dir() -> String {
        let mut buf = [0u16; 512];
        let n = unsafe { GetSystemDirectoryW(buf.as_mut_ptr(), buf.len() as u32) } as usize;
        String::from_utf16_lossy(&buf[..n.min(buf.len())])
    }

    pub fn pid() -> u32 {
        unsafe { GetCurrentProcessId() }
    }

    pub fn run(program: &str, args: &[String]) -> Result<i32, String> {
        let mut line = String::new();
        quote_arg(program, &mut line);
        for a in args {
            line.push(' ');
            quote_arg(a, &mut line);
        }
        let mut command = wide(&line);
        // STARTUPINFOW is 104 bytes on x64; only `cb` is set, so the child
        // inherits our console and standard handles.
        let mut startup = [0u64; 13];
        startup[0] = 104;
        let mut info = [0u64; 3];
        unsafe {
            if CreateProcessW(
                null(),
                command.as_mut_ptr(),
                null_mut(),
                null_mut(),
                1,
                0,
                null_mut(),
                null(),
                startup.as_mut_ptr(),
                info.as_mut_ptr(),
            ) == 0
            {
                return Err(format!("cannot run {program} (error {})", GetLastError()));
            }
            let process = info[0] as Handle;
            WaitForSingleObject(process, INFINITE);
            let mut code = 1u32;
            GetExitCodeProcess(process, &mut code);
            CloseHandle(process);
            CloseHandle(info[1] as Handle);
            Ok(code as i32)
        }
    }

    fn put(which: u32, s: &str) {
        unsafe {
            let h = GetStdHandle(which);
            let mut n = 0u32;
            WriteFile(h, s.as_ptr(), s.len() as u32, &mut n, null_mut());
        }
    }
    pub fn out(s: &str) {
        put(-11i32 as u32, s);
    }
    pub fn err(s: &str) {
        put(-12i32 as u32, s);
    }
    pub fn exit(code: i32) -> ! {
        unsafe { ExitProcess(code as u32) }
    }
}
