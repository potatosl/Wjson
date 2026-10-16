// SPDX-License-Identifier: MPL-2.0
//! Writes Microsoft "short import" libraries (the format of kernel32.lib and
//! friends): an archive with two linker members, an import descriptor, a null
//! descriptor, a null thunk, and one 20-byte import header per export.
//! See the PE/COFF specification, "Import Library Format".

use crate::coff::*;
use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

/// One archive member: (member name, object bytes, symbols it defines).
pub type ArchiveMember<'a> = (&'a str, &'a [u8], &'a [Vec<u8>]);

pub struct Import {
    pub name: String,
    pub hint: u16,
    pub data: bool,
}

/// "kernel32.dll" -> "kernel32" (used in descriptor symbol names).
fn stem(dll: &str) -> &str {
    match dll.rfind('.') {
        Some(i) => &dll[..i],
        None => dll,
    }
}

fn descriptor(dll: &str) -> (Vec<u8>, Vec<Vec<u8>>) {
    let lib = stem(dll);
    let rw = SCN_CNT_INITIALIZED_DATA | SCN_MEM_READ | SCN_MEM_WRITE;
    let mut name = dll.as_bytes().to_vec();
    name.push(0);
    let desc = format!("__IMPORT_DESCRIPTOR_{lib}").into_bytes();
    let thunk = format!("\x7f{lib}_NULL_THUNK_DATA").into_bytes();
    let obj = Object {
        sections: vec![
            Section {
                name: b".idata$2",
                data: vec![0; 20],
                characteristics: SCN_ALIGN_4BYTES | rw,
                relocs: vec![
                    Reloc { offset: 12, symbol: 2, kind: REL_AMD64_ADDR32NB }, // Name
                    Reloc { offset: 0, symbol: 3, kind: REL_AMD64_ADDR32NB },  // lookup table
                    Reloc { offset: 16, symbol: 4, kind: REL_AMD64_ADDR32NB }, // address table
                ],
            },
            Section { name: b".idata$6", data: name, characteristics: SCN_ALIGN_2BYTES | rw, relocs: vec![] },
        ],
        symbols: vec![
            Symbol::new(&desc, 0, 1, SYM_CLASS_EXTERNAL),
            Symbol::new(b".idata$2", 0, 1, SYM_CLASS_SECTION),
            Symbol::new(b".idata$6", 0, 2, SYM_CLASS_STATIC),
            Symbol::new(b".idata$4", 0, 0, SYM_CLASS_SECTION),
            Symbol::new(b".idata$5", 0, 0, SYM_CLASS_SECTION),
            Symbol::external(b"__NULL_IMPORT_DESCRIPTOR"),
            Symbol::external(&thunk),
        ],
    };
    (obj.to_bytes(), vec![desc])
}

fn null_descriptor() -> (Vec<u8>, Vec<Vec<u8>>) {
    let obj = Object {
        sections: vec![Section {
            name: b".idata$3",
            data: vec![0; 20],
            characteristics: SCN_ALIGN_4BYTES | SCN_CNT_INITIALIZED_DATA | SCN_MEM_READ | SCN_MEM_WRITE,
            relocs: vec![],
        }],
        symbols: vec![Symbol::new(b"__NULL_IMPORT_DESCRIPTOR", 0, 1, SYM_CLASS_EXTERNAL)],
    };
    (obj.to_bytes(), vec![b"__NULL_IMPORT_DESCRIPTOR".to_vec()])
}

fn null_thunk(dll: &str) -> (Vec<u8>, Vec<Vec<u8>>) {
    let thunk = format!("\x7f{}_NULL_THUNK_DATA", stem(dll)).into_bytes();
    let rw8 = SCN_ALIGN_8BYTES | SCN_CNT_INITIALIZED_DATA | SCN_MEM_READ | SCN_MEM_WRITE;
    let obj = Object {
        sections: vec![
            Section { name: b".idata$5", data: vec![0; 8], characteristics: rw8, relocs: vec![] },
            Section { name: b".idata$4", data: vec![0; 8], characteristics: rw8, relocs: vec![] },
        ],
        symbols: vec![Symbol::new(&thunk, 0, 1, SYM_CLASS_EXTERNAL)],
    };
    (obj.to_bytes(), vec![thunk])
}

/// One import header: the linker synthesizes `__imp_name` (and for code a
/// `name` jump thunk) from it.
fn short_import(dll: &str, imp: &Import) -> (Vec<u8>, Vec<Vec<u8>>) {
    let size = imp.name.len() + 1 + dll.len() + 1;
    let mut out = Vec::with_capacity(20 + size);
    out.extend_from_slice(&0u16.to_le_bytes()); // Sig1
    out.extend_from_slice(&0xFFFFu16.to_le_bytes()); // Sig2
    out.extend_from_slice(&0u16.to_le_bytes()); // Version
    out.extend_from_slice(&MACHINE_AMD64.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes()); // TimeDateStamp
    out.extend_from_slice(&(size as u32).to_le_bytes());
    out.extend_from_slice(&imp.hint.to_le_bytes());
    let kind: u16 = if imp.data { 1 } else { 0 }; // IMPORT_OBJECT_DATA / _CODE
    let name_type: u16 = 1; // IMPORT_OBJECT_NAME: import by the exact name
    out.extend_from_slice(&(kind | (name_type << 2)).to_le_bytes());
    out.extend_from_slice(imp.name.as_bytes());
    out.push(0);
    out.extend_from_slice(dll.as_bytes());
    out.push(0);
    let mut syms = vec![format!("__imp_{}", imp.name).into_bytes()];
    if !imp.data {
        syms.push(imp.name.clone().into_bytes());
    }
    (out, syms)
}

/// Builds a complete import library for `dll` (e.g. "kernel32.dll").
pub fn import_library(dll: &str, imports: &[Import]) -> Vec<u8> {
    let mut members = vec![descriptor(dll), null_descriptor(), null_thunk(dll)];
    for imp in imports {
        members.push(short_import(dll, imp));
    }
    write_archive(dll, &members)
}

/// An archive with no members, for libraries the build must find but whose
/// contents are not needed (vcruntime.lib, ucrt.lib, ...).
pub fn empty_library() -> Vec<u8> {
    b"!<arch>\n".to_vec()
}

fn header(out: &mut Vec<u8>, name: &[u8], size: usize) {
    let mut h = [b' '; 60];
    h[..name.len()].copy_from_slice(name);
    let fields: [(usize, &[u8]); 4] = [(16, b"0"), (28, b"0"), (34, b"0"), (40, b"644")];
    for (at, v) in fields {
        h[at..at + v.len()].copy_from_slice(v);
    }
    let size = format!("{size}");
    h[48..48 + size.len()].copy_from_slice(size.as_bytes());
    h[58] = b'`';
    h[59] = b'\n';
    out.extend_from_slice(&h);
}

fn padded(n: usize) -> usize {
    n + (n & 1)
}

/// Writes a COFF archive whose members are all named after `dll`.
fn write_archive(dll: &str, members: &[(Vec<u8>, Vec<Vec<u8>>)]) -> Vec<u8> {
    let named: Vec<ArchiveMember> =
        members.iter().map(|(data, syms)| (dll, data.as_slice(), syms.as_slice())).collect();
    archive(&named)
}

/// Writes a COFF archive (`!<arch>`) with the first (big-endian) and second
/// (sorted, little-endian) linker members, a longnames member when a name
/// does not fit in 16 bytes, and the given members with their symbols.
pub fn archive(members: &[ArchiveMember]) -> Vec<u8> {
    // Member names: "name/" when it fits in 16 bytes, else "/<offset>" into "//".
    let mut longnames: Vec<u8> = Vec::new();
    let mut member_names: Vec<Vec<u8>> = Vec::with_capacity(members.len());
    for (name, _, _) in members {
        if name.len() < 16 {
            member_names.push(format!("{name}/").into_bytes());
        } else if let Some(at) = find_longname(&longnames, name.as_bytes()) {
            member_names.push(format!("/{at}").into_bytes());
        } else {
            member_names.push(format!("/{}", longnames.len()).into_bytes());
            longnames.extend_from_slice(name.as_bytes());
            longnames.extend_from_slice(b"/\n");
        }
    }

    let symbols: Vec<(usize, &[u8])> = members
        .iter()
        .enumerate()
        .flat_map(|(i, (_, _, syms))| syms.iter().map(move |s| (i, s.as_slice())))
        .collect();
    let names_len: usize = symbols.iter().map(|(_, s)| s.len() + 1).sum();
    let first_size = 4 + 4 * symbols.len() + names_len;
    let second_size = 4 + 4 * members.len() + 4 + 2 * symbols.len() + names_len;

    let mut offset = 8 + 60 + padded(first_size) + 60 + padded(second_size);
    if !longnames.is_empty() {
        offset += 60 + padded(longnames.len());
    }
    let mut member_offsets = Vec::with_capacity(members.len());
    for (_, data, _) in members {
        member_offsets.push(offset as u32);
        offset += 60 + padded(data.len());
    }

    let mut out = Vec::with_capacity(offset);
    out.extend_from_slice(b"!<arch>\n");

    header(&mut out, b"/", first_size);
    out.extend_from_slice(&(symbols.len() as u32).to_be_bytes());
    for (m, _) in &symbols {
        out.extend_from_slice(&member_offsets[*m].to_be_bytes());
    }
    for (_, s) in &symbols {
        out.extend_from_slice(s);
        out.push(0);
    }
    if first_size & 1 == 1 {
        out.push(b'\n');
    }

    let mut sorted = symbols.clone();
    sorted.sort_by(|a, b| a.1.cmp(b.1));
    header(&mut out, b"/", second_size);
    out.extend_from_slice(&(members.len() as u32).to_le_bytes());
    for off in &member_offsets {
        out.extend_from_slice(&off.to_le_bytes());
    }
    out.extend_from_slice(&(sorted.len() as u32).to_le_bytes());
    for (m, _) in &sorted {
        out.extend_from_slice(&((*m + 1) as u16).to_le_bytes());
    }
    for (_, s) in &sorted {
        out.extend_from_slice(s);
        out.push(0);
    }
    if second_size & 1 == 1 {
        out.push(b'\n');
    }

    if !longnames.is_empty() {
        header(&mut out, b"//", longnames.len());
        out.extend_from_slice(&longnames);
        if longnames.len() & 1 == 1 {
            out.push(b'\n');
        }
    }

    for ((_, data, _), name) in members.iter().zip(&member_names) {
        header(&mut out, name, data.len());
        out.extend_from_slice(data);
        if data.len() & 1 == 1 {
            out.push(b'\n');
        }
    }
    debug_assert_eq!(out.len(), offset);
    out
}

fn find_longname(table: &[u8], name: &[u8]) -> Option<usize> {
    let mut at = 0;
    while at < table.len() {
        let end = table[at..].windows(2).position(|w| w == b"/\n").map_or(table.len(), |p| at + p);
        if &table[at..end] == name {
            return Some(at);
        }
        at = end + 2;
    }
    None
}

/// External symbols an x86-64 COFF object defines (what an archive's
/// symbol table must list so the linker pulls the member in).
pub fn object_symbols(obj: &[u8]) -> Result<Vec<Vec<u8>>, &'static str> {
    let u16_at = |at: usize| obj.get(at..at + 2).map(|b| u16::from_le_bytes([b[0], b[1]]));
    let u32_at = |at: usize| obj.get(at..at + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
    let machine = u16_at(0).ok_or("not a COFF object")?;
    if machine != crate::coff::MACHINE_AMD64 {
        return Err("not an x86-64 COFF object");
    }
    let symtab = u32_at(8).ok_or("truncated COFF header")? as usize;
    let count = u32_at(12).ok_or("truncated COFF header")? as usize;
    let strtab = symtab + 18 * count;
    let mut out = Vec::new();
    let mut i = 0;
    while i < count {
        let at = symtab + 18 * i;
        let rec = obj.get(at..at + 18).ok_or("truncated symbol table")?;
        let section = i16::from_le_bytes([rec[12], rec[13]]);
        let value = u32::from_le_bytes([rec[8], rec[9], rec[10], rec[11]]);
        let class = rec[16];
        let aux = rec[17] as usize;
        // Defined externals, plus common symbols (section 0 with a size).
        if class == crate::coff::SYM_CLASS_EXTERNAL && (section > 0 || (section == 0 && value != 0)) {
            let name = if rec[..4] == [0, 0, 0, 0] {
                let off = strtab + u32::from_le_bytes([rec[4], rec[5], rec[6], rec[7]]) as usize;
                let rest = obj.get(off..).ok_or("bad string table offset")?;
                let end = rest.iter().position(|&b| b == 0).ok_or("unterminated symbol name")?;
                rest[..end].to_vec()
            } else {
                let end = rec[..8].iter().position(|&b| b == 0).unwrap_or(8);
                rec[..end].to_vec()
            };
            out.push(name);
        }
        i += 1 + aux;
    }
    Ok(out)
}
