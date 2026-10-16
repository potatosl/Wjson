// SPDX-License-Identifier: MPL-2.0
//! Reads the export table of a PE image (a Windows DLL).

use alloc::string::String;
use alloc::vec::Vec;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Export {
    pub name: String,
    /// Index into the export name table: the loader's lookup hint.
    pub hint: u16,
    /// Exported data (lives in a non-executable section) rather than code.
    pub data: bool,
}

#[derive(Debug)]
pub struct Exports {
    /// The DLL's own name as recorded in its export directory.
    pub dll: String,
    pub exports: Vec<Export>,
}

struct Section {
    va: u32,
    size: u32,
    raw: u32,
    raw_size: u32,
    flags: u32,
}

fn u16_at(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(at..at + 2)?.try_into().ok()?))
}
fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

const SCN_MEM_EXECUTE: u32 = 0x2000_0000;

pub fn read_exports(image: &[u8]) -> Result<Exports, &'static str> {
    if image.get(..2) != Some(b"MZ") {
        return Err("not a PE image (no MZ header)");
    }
    let pe = u32_at(image, 0x3C).ok_or("truncated DOS header")? as usize;
    if image.get(pe..pe + 4) != Some(b"PE\0\0") {
        return Err("not a PE image (no PE signature)");
    }
    let coff = pe + 4;
    let nsections = u16_at(image, coff + 2).ok_or("truncated COFF header")? as usize;
    let opt_size = u16_at(image, coff + 16).ok_or("truncated COFF header")? as usize;
    let opt = coff + 20;
    let magic = u16_at(image, opt).ok_or("truncated optional header")?;
    let dirs = match magic {
        0x20B => opt + 112, // PE32+
        0x10B => opt + 96,  // PE32
        _ => return Err("unknown optional header magic"),
    };
    let export_rva = u32_at(image, dirs).ok_or("truncated data directories")?;
    let export_size = u32_at(image, dirs + 4).ok_or("truncated data directories")?;

    let mut sections = Vec::with_capacity(nsections);
    let table = opt + opt_size;
    for i in 0..nsections {
        let s = table + 40 * i;
        sections.push(Section {
            size: u32_at(image, s + 8).ok_or("truncated section table")?,
            va: u32_at(image, s + 12).ok_or("truncated section table")?,
            raw_size: u32_at(image, s + 16).ok_or("truncated section table")?,
            raw: u32_at(image, s + 20).ok_or("truncated section table")?,
            flags: u32_at(image, s + 36).ok_or("truncated section table")?,
        });
    }
    // Every offset below comes from the file, so all arithmetic is widened or
    // checked: a malformed image must produce an error, never a wrapped value.
    let section_of = |rva: u32| {
        sections.iter().find(|s| {
            let end = s.va as u64 + s.size.max(s.raw_size) as u64;
            rva >= s.va && (rva as u64) < end
        })
    };
    let offset = |rva: u32| -> Option<usize> {
        let s = section_of(rva)?;
        let off = rva - s.va;
        (off < s.raw_size).then(|| s.raw as usize + off as usize)
    };
    let cstr = |rva: u32| -> Option<String> {
        let at = offset(rva)?;
        let len = image.get(at..)?.iter().position(|&b| b == 0)?;
        String::from_utf8(image[at..at + len].to_vec()).ok()
    };

    if export_rva == 0 {
        return Ok(Exports { dll: String::new(), exports: Vec::new() });
    }
    let dir = offset(export_rva).ok_or("export directory outside the image")?;
    let dll = cstr(u32_at(image, dir + 12).ok_or("truncated export directory")?).unwrap_or_default();
    let nfuncs = u32_at(image, dir + 20).ok_or("truncated export directory")?;
    let nnames = u32_at(image, dir + 24).ok_or("truncated export directory")? as usize;
    let funcs = offset(u32_at(image, dir + 28).ok_or("truncated export directory")?);
    let names = offset(u32_at(image, dir + 32).ok_or("truncated export directory")?);
    let ords = offset(u32_at(image, dir + 36).ok_or("truncated export directory")?);
    let (Some(funcs), Some(names), Some(ords)) = (funcs, names, ords) else {
        if nnames == 0 {
            return Ok(Exports { dll, exports: Vec::new() });
        }
        return Err("export tables outside the image");
    };

    // The count is untrusted: size the vector by what the name table could
    // actually hold in this image, not by what the header claims.
    let mut exports = Vec::with_capacity(nnames.min(image.len() / 4));
    for i in 0..nnames {
        let name_rva = u32_at(image, names + 4 * i).ok_or("truncated name table")?;
        let index = u16_at(image, ords + 2 * i).ok_or("truncated ordinal table")? as u32;
        if index >= nfuncs {
            continue;
        }
        let func_rva = u32_at(image, funcs + 4 * index as usize).ok_or("truncated function table")?;
        let Some(name) = cstr(name_rva) else { continue };
        if name.is_empty() {
            continue;
        }
        // A forwarder's RVA points at a string inside the export directory;
        // the loader resolves it, so it imports like code.
        let forwarded = func_rva >= export_rva && (func_rva as u64) < export_rva as u64 + export_size as u64;
        let data = !forwarded && section_of(func_rva).is_some_and(|s| s.flags & SCN_MEM_EXECUTE == 0);
        exports.push(Export { name, hint: i.min(u16::MAX as usize) as u16, data });
    }
    Ok(Exports { dll, exports })
}
