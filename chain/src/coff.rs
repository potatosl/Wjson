// SPDX-License-Identifier: MIT
//! Minimal COFF object writer (x86-64): sections with relocations, a symbol
//! table and a string table. Enough for import descriptors and the runtime
//! object; no line numbers, no COMDATs.

use alloc::vec::Vec;

pub const MACHINE_AMD64: u16 = 0x8664;

pub const SCN_CNT_CODE: u32 = 0x0000_0020;
pub const SCN_CNT_INITIALIZED_DATA: u32 = 0x0000_0040;
pub const SCN_ALIGN_2BYTES: u32 = 0x0020_0000;
pub const SCN_ALIGN_4BYTES: u32 = 0x0030_0000;
pub const SCN_ALIGN_8BYTES: u32 = 0x0040_0000;
pub const SCN_ALIGN_16BYTES: u32 = 0x0050_0000;
pub const SCN_MEM_EXECUTE: u32 = 0x2000_0000;
pub const SCN_MEM_READ: u32 = 0x4000_0000;
pub const SCN_MEM_WRITE: u32 = 0x8000_0000;

pub const REL_AMD64_ADDR64: u16 = 0x0001;
pub const REL_AMD64_ADDR32NB: u16 = 0x0003;
pub const REL_AMD64_REL32: u16 = 0x0004;

pub const SYM_CLASS_EXTERNAL: u8 = 2;
pub const SYM_CLASS_STATIC: u8 = 3;
pub const SYM_CLASS_SECTION: u8 = 0x68;
pub const SYM_TYPE_FUNCTION: u16 = 0x20;

pub struct Reloc {
    pub offset: u32,
    pub symbol: u32,
    pub kind: u16,
}

pub struct Section {
    pub name: &'static [u8],
    pub data: Vec<u8>,
    pub characteristics: u32,
    pub relocs: Vec<Reloc>,
}

pub struct Symbol {
    pub name: Vec<u8>,
    pub value: u32,
    /// 1-based section number; 0 = undefined/external.
    pub section: i16,
    pub kind: u16,
    pub class: u8,
}

impl Symbol {
    pub fn new(name: &[u8], value: u32, section: i16, class: u8) -> Symbol {
        Symbol { name: name.to_vec(), value, section, kind: 0, class }
    }
    pub fn function(name: &[u8], value: u32, section: i16) -> Symbol {
        Symbol { name: name.to_vec(), value, section, kind: SYM_TYPE_FUNCTION, class: SYM_CLASS_EXTERNAL }
    }
    pub fn external(name: &[u8]) -> Symbol {
        Symbol::new(name, 0, 0, SYM_CLASS_EXTERNAL)
    }
}

pub struct Object {
    pub sections: Vec<Section>,
    pub symbols: Vec<Symbol>,
}

fn put16(out: &mut Vec<u8>, v: u16) {
    out.extend_from_slice(&v.to_le_bytes());
}
fn put32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}

impl Object {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut strings: Vec<u8> = Vec::new();
        let header_size = 20 + 40 * self.sections.len();

        // Section data and relocations follow the headers back to back.
        let mut offset = header_size;
        let mut layout = Vec::with_capacity(self.sections.len());
        for s in &self.sections {
            let data_at = offset;
            offset += s.data.len();
            let relocs_at = if s.relocs.is_empty() { 0 } else { offset };
            offset += 10 * s.relocs.len();
            layout.push((data_at, relocs_at));
        }
        let symtab_at = offset;

        let mut out = Vec::with_capacity(offset + 18 * self.symbols.len() + 64);
        put16(&mut out, MACHINE_AMD64);
        put16(&mut out, self.sections.len() as u16);
        put32(&mut out, 0); // TimeDateStamp: 0 keeps builds reproducible
        put32(&mut out, symtab_at as u32);
        put32(&mut out, self.symbols.len() as u32);
        put16(&mut out, 0); // SizeOfOptionalHeader
        put16(&mut out, 0); // Characteristics

        for (s, &(data_at, relocs_at)) in self.sections.iter().zip(&layout) {
            let mut name = [0u8; 8];
            assert!(s.name.len() <= 8, "long section names are not needed here");
            name[..s.name.len()].copy_from_slice(s.name);
            out.extend_from_slice(&name);
            put32(&mut out, 0); // VirtualSize
            put32(&mut out, 0); // VirtualAddress
            put32(&mut out, s.data.len() as u32);
            put32(&mut out, if s.data.is_empty() { 0 } else { data_at as u32 });
            put32(&mut out, relocs_at as u32);
            put32(&mut out, 0); // PointerToLinenumbers
            put16(&mut out, s.relocs.len() as u16);
            put16(&mut out, 0);
            put32(&mut out, s.characteristics);
        }
        for s in &self.sections {
            out.extend_from_slice(&s.data);
            for r in &s.relocs {
                put32(&mut out, r.offset);
                put32(&mut out, r.symbol);
                put16(&mut out, r.kind);
            }
        }
        debug_assert_eq!(out.len(), symtab_at);

        for sym in &self.symbols {
            if sym.name.len() <= 8 {
                let mut name = [0u8; 8];
                name[..sym.name.len()].copy_from_slice(&sym.name);
                out.extend_from_slice(&name);
            } else {
                put32(&mut out, 0);
                put32(&mut out, 4 + strings.len() as u32);
                strings.extend_from_slice(&sym.name);
                strings.push(0);
            }
            put32(&mut out, sym.value);
            put16(&mut out, sym.section as u16);
            put16(&mut out, sym.kind);
            out.push(sym.class);
            out.push(0); // no auxiliary records
        }
        put32(&mut out, 4 + strings.len() as u32);
        out.extend_from_slice(&strings);
        out
    }
}
