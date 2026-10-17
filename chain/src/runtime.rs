// SPDX-License-Identifier: MPL-2.0
//! The C runtime startup object (`wjrt.obj`) that replaces the static parts
//! of MSVC's msvcrt.lib. Everything else a program needs (printf, memcpy,
//! exit, C++ exception support for Rust panics) comes from the system's
//! msvcrt.dll through a generated import library.
//!
//! Contents:
//!   mainCRTStartup      calls main(0, NULL, NULL), then msvcrt's exit(), which
//!                       flushes stdio and runs atexit handlers. Wave's main
//!                       takes no arguments and Rust's std reads the command
//!                       line from GetCommandLineW, so argv is not built.
//!   __chkstk            stack probe for frames over 4 KiB (LLVM emits calls).
//!                       x64: size in rax; ARM64: size in 16-byte units in x15.
//!   _tls_used & co.     the TLS directory the loader needs for `.tls`
//!                       sections (Rust's thread_local! uses them).
//!   _fltused            marker the compiler references when floats are used.
//!   ??_7type_info@@6B@  type_info's vtable address; Rust's panic type
//!                       descriptors point at it but never call through it.

use crate::coff::*;
use alloc::vec;
use alloc::vec::Vec;

/// Machine code, assembled from:
///
/// ```text
/// mainCRTStartup:                  __chkstk:
///   sub  rsp, 40                     push rcx
///   xor  ecx, ecx                    push rax
///   xor  edx, edx                    cmp  rax, 4096
///   xor  r8d, r8d                    lea  rcx, [rsp + 24]
///   call main                        jb   1f
///   mov  ecx, eax                  2: sub  rcx, 4096
///   call [rip + __imp_exit]          test [rcx], rcx
///   int3                             sub  rax, 4096
///                                    cmp  rax, 4096
///                                    ja   2b
///                                  1: sub  rcx, rax
///                                    test [rcx], rcx
///                                    pop  rax
///                                    pop  rcx
///                                    ret
/// ```
const TEXT: [u8; 0x50] = [
    // mainCRTStartup (0x00)
    0x48, 0x83, 0xEC, 0x28, // sub rsp, 0x28
    0x31, 0xC9, // xor ecx, ecx
    0x31, 0xD2, // xor edx, edx
    0x45, 0x31, 0xC0, // xor r8d, r8d
    0xE8, 0x00, 0x00, 0x00, 0x00, // call main            (REL32 at 0x0C)
    0x89, 0xC1, // mov ecx, eax
    0xFF, 0x15, 0x00, 0x00, 0x00, 0x00, // call [__imp_exit] (REL32 at 0x14)
    0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, // int3 + padding to 0x20
    // __chkstk (0x20)
    0x51, // push rcx
    0x50, // push rax
    0x48, 0x3D, 0x00, 0x10, 0x00, 0x00, // cmp rax, 0x1000
    0x48, 0x8D, 0x4C, 0x24, 0x18, // lea rcx, [rsp+0x18]
    0x72, 0x18, // jb 0x47
    0x48, 0x81, 0xE9, 0x00, 0x10, 0x00, 0x00, // sub rcx, 0x1000   (0x2F)
    0x48, 0x85, 0x09, // test [rcx], rcx
    0x48, 0x2D, 0x00, 0x10, 0x00, 0x00, // sub rax, 0x1000
    0x48, 0x3D, 0x00, 0x10, 0x00, 0x00, // cmp rax, 0x1000
    0x77, 0xE8, // ja 0x2F
    0x48, 0x29, 0xC1, // sub rcx, rax            (0x47)
    0x48, 0x85, 0x09, // test [rcx], rcx
    0x58, // pop rax
    0x59, // pop rcx
    0xC3, // ret
];

/// ARM64 machine code. The words were assembled with llvm-mc
/// (`-triple=aarch64-pc-windows-msvc`) from:
///
/// ```text
/// mainCRTStartup:                     __chkstk:   (x15 = bytes / 16)
///   stp  x29, x30, [sp, #-16]!          lsl  x16, x15, #4
///   mov  x29, sp                        mov  x17, sp
///   mov  w0, #0                       1: sub  x17, x17, #4096
///   mov  x1, #0                          subs x16, x16, #4096
///   mov  x2, #0                          ldr  xzr, [x17]
///   bl   main                            b.gt 1b
///   adrp x16, __imp_exit                 ret
///   ldr  x16, [x16, :lo12:__imp_exit]
///   blr  x16
///   brk  #0xf000
/// ```
const TEXT_ARM64: [u32; 19] = [
    // mainCRTStartup (0x00)
    0xA9BF7BFD, // stp x29, x30, [sp, #-16]!
    0x910003FD, // mov x29, sp
    0x52800000, // mov w0, #0
    0xD2800001, // mov x1, #0
    0xD2800002, // mov x2, #0
    0x94000000, // bl main                      (BRANCH26 at 0x14)
    0x90000010, // adrp x16, __imp_exit         (PAGEBASE_REL21 at 0x18)
    0xF9400210, // ldr x16, [x16, #lo12]        (PAGEOFFSET_12L at 0x1C)
    0xD63F0200, // blr x16
    0xD43E0000, // brk #0xf000
    0xD503201F, // nop (padding to 0x30)
    0xD503201F, // nop
    // __chkstk (0x30)
    0xD37CEDF0, // lsl x16, x15, #4
    0x910003F1, // mov x17, sp
    0xD1400631, // sub x17, x17, #4096
    0xF1400610, // subs x16, x16, #4096
    0xF940023F, // ldr xzr, [x17]
    0x54FFFFAC, // b.gt -12 (back to the sub)
    0xD65F03C0, // ret
];

/// Names wjrt.obj defines. Some system DLLs export the same names
/// (kernel32 and ntdll export `__chkstk`), so import libraries leave them out.
pub const DEFINES: &[&str] = &["mainCRTStartup", "__chkstk", "_tls_used", "_tls_index", "_fltused", "??_7type_info@@6B@"];

pub fn runtime_object(arch: Arch) -> Vec<u8> {
    let code = SCN_CNT_CODE | SCN_ALIGN_16BYTES | SCN_MEM_EXECUTE | SCN_MEM_READ;
    let rw8 = SCN_CNT_INITIALIZED_DATA | SCN_ALIGN_8BYTES | SCN_MEM_READ | SCN_MEM_WRITE;
    let ro8 = SCN_CNT_INITIALIZED_DATA | SCN_ALIGN_8BYTES | SCN_MEM_READ;

    // Symbol indices (relocations refer to them).
    const MAIN: u32 = 2;
    const IMP_EXIT: u32 = 3;
    const TLS_START: u32 = 7;
    const TLS_END: u32 = 8;
    const TLS_INDEX: u32 = 4;
    const XL_A: u32 = 9;

    // IMAGE_TLS_DIRECTORY64: start, end, &index, callbacks, zero fill, flags.
    // AddressOfCallBacks = &__xl_a + 8: the null-terminated callback array
    // that the linker gathers between .CRT$XLA and .CRT$XLZ.
    let mut tls_dir = vec![0u8; 40];
    tls_dir[24..32].copy_from_slice(&8u64.to_le_bytes()); // addend for __xl_a

    let addr64 = arch.rel_addr64();
    let (text, text_relocs, chkstk_at) = match arch {
        Arch::X64 => (
            TEXT.to_vec(),
            vec![
                Reloc { offset: 0x0C, symbol: MAIN, kind: REL_AMD64_REL32 },
                Reloc { offset: 0x14, symbol: IMP_EXIT, kind: REL_AMD64_REL32 },
            ],
            0x20,
        ),
        Arch::Arm64 => (
            TEXT_ARM64.iter().flat_map(|w| w.to_le_bytes()).collect(),
            vec![
                Reloc { offset: 0x14, symbol: MAIN, kind: REL_ARM64_BRANCH26 },
                Reloc { offset: 0x18, symbol: IMP_EXIT, kind: REL_ARM64_PAGEBASE_REL21 },
                Reloc { offset: 0x1C, symbol: IMP_EXIT, kind: REL_ARM64_PAGEOFFSET_12L },
            ],
            0x30,
        ),
    };

    let obj = Object {
        arch,
        sections: vec![
            Section { name: b".text", data: text, characteristics: code, relocs: text_relocs },
            // _tls_index (u32) at 0, _fltused (i32) at 4, type_info vtable at 8.
            Section { name: b".data", data: vec![0; 16], characteristics: rw8, relocs: vec![] },
            Section {
                name: b".rdata$T",
                data: tls_dir,
                characteristics: ro8,
                relocs: vec![
                    Reloc { offset: 0, symbol: TLS_START, kind: addr64 },
                    Reloc { offset: 8, symbol: TLS_END, kind: addr64 },
                    Reloc { offset: 16, symbol: TLS_INDEX, kind: addr64 },
                    Reloc { offset: 24, symbol: XL_A, kind: addr64 },
                ],
            },
            Section { name: b".tls", data: vec![0; 8], characteristics: rw8, relocs: vec![] },
            Section { name: b".tls$ZZZ", data: vec![0; 8], characteristics: rw8, relocs: vec![] },
            Section { name: b".CRT$XLA", data: vec![0; 8], characteristics: ro8, relocs: vec![] },
            Section { name: b".CRT$XLZ", data: vec![0; 8], characteristics: ro8, relocs: vec![] },
        ],
        symbols: vec![
            Symbol::function(b"mainCRTStartup", 0x00, 1), // 0
            Symbol::function(b"__chkstk", chkstk_at, 1),  // 1
            Symbol::external(b"main"),                    // 2
            Symbol::external(b"__imp_exit"),              // 3
            Symbol::new(b"_tls_index", 0, 2, SYM_CLASS_EXTERNAL), // 4
            Symbol::new(b"_fltused", 4, 2, SYM_CLASS_EXTERNAL),   // 5
            Symbol::new(b"_tls_used", 0, 3, SYM_CLASS_EXTERNAL),  // 6
            Symbol::new(b"_tls_start", 0, 4, SYM_CLASS_STATIC),   // 7
            Symbol::new(b"_tls_end", 0, 5, SYM_CLASS_STATIC),     // 8
            Symbol::new(b"__xl_a", 0, 6, SYM_CLASS_STATIC),       // 9
            Symbol::new(b"__xl_z", 0, 7, SYM_CLASS_STATIC),       // 10
            Symbol::new(b"??_7type_info@@6B@", 8, 2, SYM_CLASS_EXTERNAL), // 11
        ],
    };
    obj.to_bytes()
}
