// SPDX-License-Identifier: MPL-2.0
//! wjson_core: the parsing half of the wjson Wave library.
//!
//! Exposes a C ABI (see `include/wjson.h`). A parse always yields a document
//! handle — even on failure — so the caller can read the error position from
//! it. Values are addressed by `i32` node ids; `-1` means "no such value".
//! Every accessor tolerates a null handle and out-of-range ids.

// Every `extern "C"` function below shares one safety contract, stated once
// here instead of in 50 copies: a document/reader handle must be null or come
// from the matching constructor and not yet have been freed; a `(pointer, len)`
// pair must describe `len` readable bytes (or `len == 0`); `out_len` must be
// null or writable; handles must not be used from two threads at once when
// either call mutates (reads of a document nobody is editing may run
// concurrently). Null handles, bad ids and negative lengths are tolerated.
#![allow(clippy::missing_safety_doc)]
// Edition 2024 asks for an explicit `unsafe {}` around each unsafe operation
// inside an `unsafe fn`. Here every such operation is dereferencing or slicing
// the caller's pointers, which is exactly what the contract above covers, so a
// block in each of the ~60 entry points would only repeat it.
#![allow(unsafe_op_in_unsafe_fn)]

pub mod dom;
pub mod edit;
pub mod parser;
pub mod stream;

use dom::*;
use edit::{EditError, Slack};
use parser::{ErrorCode, ParseError};
use std::ptr;

pub struct Doc {
    dom: Dom,
    error: Option<ParseError>,
    slack: Slack,
}

impl Doc {
    fn new(dom: Dom, error: Option<ParseError>) -> Self {
        Doc { dom, error, slack: Slack::default() }
    }
}

const NO_NODE: i32 = -1;

#[inline]
unsafe fn doc_ref<'a>(doc: *const Doc) -> Option<&'a Doc> {
    doc.as_ref()
}

#[inline]
unsafe fn bytes_arg<'a>(p: *const u8, len: i64) -> Option<&'a [u8]> {
    if len < 0 {
        return None;
    }
    if len == 0 {
        return Some(&[]);
    }
    if p.is_null() {
        return None;
    }
    Some(std::slice::from_raw_parts(p, len as usize))
}

#[inline]
unsafe fn write_len(out_len: *mut i64, value: i64) {
    if !out_len.is_null() {
        *out_len = value;
    }
}

fn id_out(id: Option<u32>) -> i32 {
    match id {
        Some(v) if v <= i32::MAX as u32 => v as i32,
        _ => NO_NODE,
    }
}

/// Library version, NUL-terminated.
#[unsafe(no_mangle)]
pub extern "C" fn wj_version() -> *const u8 {
    concat!(env!("CARGO_PKG_VERSION"), "\0").as_ptr()
}

/// Parses `len` bytes with default options (strict, depth 512).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wj_parse(src: *const u8, len: i64) -> *mut Doc {
    wj_parse_ex(src, len, 0, 0)
}

/// Parses with explicit options. `max_depth == 0` selects the default.
/// `flags`: 1 = allow trailing commas, 2 = allow comments.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wj_parse_ex(src: *const u8, len: i64, max_depth: i32, flags: u32) -> *mut Doc {
    let doc = match bytes_arg(src, len) {
        None => Doc::new(
            Dom::default(),
            Some(ParseError { code: ErrorCode::InvalidArgument, offset: 0, line: 1, column: 1 }),
        ),
        Some(bytes) => match parser::parse(bytes, max_depth.max(0) as u32, flags) {
            Ok(dom) => Doc::new(dom, None),
            Err(e) => Doc::new(Dom::default(), Some(e)),
        },
    };
    Box::into_raw(Box::new(doc))
}

/// Parses a NUL-terminated string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wj_parse_cstr(src: *const u8, max_depth: i32, flags: u32) -> *mut Doc {
    if src.is_null() {
        return wj_parse_ex(ptr::null(), -1, max_depth, flags);
    }
    let len = std::ffi::CStr::from_ptr(src as *const std::ffi::c_char).to_bytes().len();
    wj_parse_ex(src, len as i64, max_depth, flags)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn wj_free(doc: *mut Doc) {
    if !doc.is_null() {
        drop(Box::from_raw(doc));
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn wj_ok(doc: *const Doc) -> i32 {
    match doc_ref(doc) {
        Some(d) if d.error.is_none() => 1,
        _ => 0,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn wj_error_code(doc: *const Doc) -> i32 {
    match doc_ref(doc) {
        None => ErrorCode::InvalidArgument as i32,
        Some(d) => d.error.map_or(0, |e| e.code as i32),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn wj_error_message(doc: *const Doc) -> *const u8 {
    let code = match doc_ref(doc) {
        None => ErrorCode::InvalidArgument,
        Some(d) => d.error.map_or(ErrorCode::Ok, |e| e.code),
    };
    code.message().as_ptr()
}

/// Byte offset of the error (0 when ok).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wj_error_offset(doc: *const Doc) -> i64 {
    doc_ref(doc).and_then(|d| d.error).map_or(0, |e| e.offset as i64)
}

/// 1-based line of the error (0 when ok).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wj_error_line(doc: *const Doc) -> i64 {
    doc_ref(doc).and_then(|d| d.error).map_or(0, |e| e.line as i64)
}

/// 1-based column (in code points) of the error (0 when ok).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wj_error_column(doc: *const Doc) -> i64 {
    doc_ref(doc).and_then(|d| d.error).map_or(0, |e| e.column as i64)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn wj_root(doc: *const Doc) -> i32 {
    match doc_ref(doc) {
        Some(d) if d.error.is_none() => id_out(Some(d.dom.root)),
        _ => NO_NODE,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn wj_node_count(doc: *const Doc) -> i64 {
    doc_ref(doc).map_or(0, |d| d.dom.nodes.len() as i64)
}

/// 0 null, 1 bool, 2 number, 3 string, 4 array, 5 object, -1 invalid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wj_type(doc: *const Doc, node: i32) -> i32 {
    doc_ref(doc)
        .and_then(|d| d.dom.node(node as i64))
        .map_or(-1, |n| n.kind as i32)
}

/// 1 for true, 0 for false, -1 when not a bool.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wj_bool(doc: *const Doc, node: i32) -> i32 {
    match doc_ref(doc).and_then(|d| d.dom.node(node as i64)) {
        Some(n) if n.kind == KIND_BOOL => n.flag as i32,
        _ => -1,
    }
}

/// Element/member count for containers, byte length for strings, -1 otherwise.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wj_len(doc: *const Doc, node: i32) -> i64 {
    match doc_ref(doc).and_then(|d| d.dom.node(node as i64)) {
        Some(n) if matches!(n.kind, KIND_ARRAY | KIND_OBJECT | KIND_STRING) => n.len as i64,
        _ => -1,
    }
}

/// Array element or object member value at `index`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wj_at(doc: *const Doc, node: i32, index: i64) -> i32 {
    id_out(doc_ref(doc).and_then(|d| d.dom.child(node as i64, index)))
}

/// Object key at `index` (decoded, NUL-terminated), or null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wj_key(doc: *const Doc, node: i32, index: i64, out_len: *mut i64) -> *const u8 {
    match doc_ref(doc).and_then(|d| d.dom.member(node as i64, index).map(|m| (d, *m))) {
        Some((d, m)) => {
            write_len(out_len, m.key_len as i64);
            d.dom.text_ptr(m.key_start)
        }
        None => {
            write_len(out_len, 0);
            ptr::null()
        }
    }
}

/// Object member lookup by key bytes (last duplicate wins).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wj_get(doc: *const Doc, node: i32, key: *const u8, key_len: i64) -> i32 {
    match (doc_ref(doc), bytes_arg(key, key_len)) {
        (Some(d), Some(k)) => id_out(d.dom.get(node as i64, k)),
        _ => NO_NODE,
    }
}

/// RFC 6901 JSON Pointer lookup relative to `node`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wj_pointer(doc: *const Doc, node: i32, pointer: *const u8, len: i64) -> i32 {
    match (doc_ref(doc), bytes_arg(pointer, len)) {
        (Some(d), Some(p)) => id_out(d.dom.pointer(node as i64, p)),
        _ => NO_NODE,
    }
}

/// Decoded string bytes (NUL-terminated; may contain embedded NULs from
/// `\u0000`, so prefer `out_len`). Null when not a string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wj_str(doc: *const Doc, node: i32, out_len: *mut i64) -> *const u8 {
    lexeme(doc, node, KIND_STRING, out_len)
}

/// Raw number lexeme exactly as written in the source (NUL-terminated).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wj_num(doc: *const Doc, node: i32, out_len: *mut i64) -> *const u8 {
    lexeme(doc, node, KIND_NUMBER, out_len)
}

/// 1 when the number has no fraction/exponent, 0 when it does, -1 when not a number.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wj_num_is_int(doc: *const Doc, node: i32) -> i32 {
    match doc_ref(doc).and_then(|d| d.dom.node(node as i64)) {
        Some(n) if n.kind == KIND_NUMBER => (n.flag & NUM_INTEGER != 0) as i32,
        _ => -1,
    }
}

unsafe fn lexeme(doc: *const Doc, node: i32, kind: u8, out_len: *mut i64) -> *const u8 {
    match doc_ref(doc).and_then(|d| d.dom.node(node as i64).map(|n| (d, *n))) {
        Some((d, n)) if n.kind == kind => {
            write_len(out_len, n.len as i64);
            d.dom.text_ptr(n.start)
        }
        _ => {
            write_len(out_len, 0);
            ptr::null()
        }
    }
}

// ---- editing ----------------------------------------------------------------
//
// Creation functions return the new (detached) node id, or a negative
// EditError code. Mutations return 0 or a negative EditError code.
//   -1 invalid handle/node  -2 value already attached  -3 would create a cycle
//   -4 no such key/index    -5 wrong container kind     -6 bad number/UTF-8

#[inline]
unsafe fn doc_mut<'a>(doc: *mut Doc) -> Option<&'a mut Doc> {
    match doc.as_mut() {
        Some(d) if d.error.is_none() => Some(d),
        _ => None,
    }
}

/// Input bytes for an editing call, copied into an owned buffer: the caller
/// may pass text that lives inside this very document, and the edit takes a
/// mutable borrow of it.
unsafe fn edit_input(_doc: *mut Doc, p: *const u8, len: i64) -> Option<Vec<u8>> {
    bytes_arg(p, len).map(|b| b.to_vec())
}

fn code(r: Result<(), EditError>) -> i32 {
    match r {
        Ok(()) => 0,
        Err(e) => e as i32,
    }
}

fn node_or(r: Result<u32, EditError>) -> i32 {
    match r {
        Ok(id) if id <= i32::MAX as u32 => id as i32,
        Ok(_) => EditError::Invalid as i32,
        Err(e) => e as i32,
    }
}

/// An empty, editable document with no root (wj_root returns -1).
#[unsafe(no_mangle)]
pub extern "C" fn wj_new_doc() -> *mut Doc {
    Box::into_raw(Box::new(Doc::new(Dom::default(), None)))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn wj_new_null(doc: *mut Doc) -> i32 {
    doc_mut(doc).map_or(-1, |d| node_or(Ok(d.dom.new_null())))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn wj_new_bool(doc: *mut Doc, value: i32) -> i32 {
    doc_mut(doc).map_or(-1, |d| node_or(Ok(d.dom.new_bool(value != 0))))
}

/// `lexeme` must be a JSON number ("12", "-0.5", "1e9"); -6 otherwise.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wj_new_number(doc: *mut Doc, lexeme: *const u8, len: i64) -> i32 {
    let input = edit_input(doc, lexeme, len);
    match (doc_mut(doc), input) {
        (Some(d), Some(b)) => node_or(d.dom.new_number(&b)),
        _ => -1,
    }
}

/// `text` is the raw (unescaped) UTF-8 content; -6 for invalid UTF-8.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wj_new_string(doc: *mut Doc, text: *const u8, len: i64) -> i32 {
    let input = edit_input(doc, text, len);
    match (doc_mut(doc), input) {
        (Some(d), Some(b)) => node_or(d.dom.new_string(&b)),
        _ => -1,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn wj_new_array(doc: *mut Doc) -> i32 {
    doc_mut(doc).map_or(-1, |d| node_or(Ok(d.dom.new_array())))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn wj_new_object(doc: *mut Doc) -> i32 {
    doc_mut(doc).map_or(-1, |d| node_or(Ok(d.dom.new_object())))
}

/// Makes a detached value the root; the previous root becomes detached.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wj_set_root(doc: *mut Doc, node: i32) -> i32 {
    doc_mut(doc).map_or(-1, |d| code(d.dom.set_root(node as i64)))
}

/// Parent node id; -2 for the root, -1 for a detached value, -3 if invalid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wj_parent(doc: *const Doc, node: i32) -> i32 {
    match doc_ref(doc) {
        Some(d) if d.error.is_none() => d.dom.parent_of(node as i64).unwrap_or(-3),
        _ => -3,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn wj_arr_push(doc: *mut Doc, arr: i32, value: i32) -> i32 {
    doc_mut(doc).map_or(-1, |d| code(d.dom.arr_push(&mut d.slack, arr as i64, value as i64)))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn wj_arr_insert(doc: *mut Doc, arr: i32, index: i64, value: i32) -> i32 {
    doc_mut(doc).map_or(-1, |d| code(d.dom.arr_insert(&mut d.slack, arr as i64, index, value as i64)))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn wj_arr_set(doc: *mut Doc, arr: i32, index: i64, value: i32) -> i32 {
    doc_mut(doc).map_or(-1, |d| code(d.dom.arr_set(arr as i64, index, value as i64)))
}

/// Removes element `index`; returns the now-detached element id (or < 0).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wj_arr_remove(doc: *mut Doc, arr: i32, index: i64) -> i32 {
    doc_mut(doc).map_or(-1, |d| node_or(d.dom.arr_remove(arr as i64, index)))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn wj_obj_set(doc: *mut Doc, obj: i32, key: *const u8, key_len: i64, value: i32) -> i32 {
    let input = edit_input(doc, key, key_len);
    match (doc_mut(doc), input) {
        (Some(d), Some(k)) => code(d.dom.obj_set(&mut d.slack, obj as i64, &k, value as i64)),
        _ => -1,
    }
}

/// Removes every member named `key`; returns the count removed (or < 0).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wj_obj_remove(doc: *mut Doc, obj: i32, key: *const u8, key_len: i64) -> i32 {
    let input = edit_input(doc, key, key_len);
    match (doc_mut(doc), input) {
        (Some(d), Some(k)) => match d.dom.obj_remove(obj as i64, &k) {
            Ok(n) => n as i32,
            Err(e) => e as i32,
        },
        _ => -1,
    }
}

/// Removes every member of an object; returns the count removed (or < 0).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wj_obj_clear(doc: *mut Doc, obj: i32) -> i32 {
    doc_mut(doc).map_or(-1, |d| match d.dom.obj_clear(obj as i64) {
        Ok(n) => n.min(i32::MAX as u32) as i32,
        Err(e) => e as i32,
    })
}

/// Deep-copies `src_node` from `src` (may be the same document) into `doc`
/// as a new detached value.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wj_copy(doc: *mut Doc, src: *const Doc, src_node: i32) -> i32 {
    if std::ptr::eq(doc as *const Doc, src) {
        // Copy through a scratch document to avoid aliasing the source.
        let Some(d) = doc_mut(doc) else { return -1 };
        let mut tmp = Dom::default();
        let mut tmp_slack = Slack::default();
        let root = match tmp.copy_from(&mut tmp_slack, &d.dom, src_node as i64) {
            Ok(r) => r,
            Err(e) => return e as i32,
        };
        return node_or(d.dom.copy_from(&mut d.slack, &tmp, root as i64));
    }
    match (doc_mut(doc), doc_ref(src)) {
        (Some(d), Some(s)) if s.error.is_none() => node_or(d.dom.copy_from(&mut d.slack, &s.dom, src_node as i64)),
        _ => -1,
    }
}

// ---- streaming ------------------------------------------------------------
//
// Events: -1 error, 0 need more input, 1 '{', 2 '}', 3 '[', 4 ']', 5 key,
// 6 string, 7 number, 8 true, 9 false, 10 null, 11 end of input.
// Flags: 1 trailing commas, 2 comments, 4 multiple top-level values.

use stream::Reader;

#[unsafe(no_mangle)]
pub extern "C" fn wj_reader_new(max_depth: i32, flags: u32) -> *mut Reader {
    Box::into_raw(Box::new(Reader::new(max_depth.max(0) as u32, flags)))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn wj_reader_free(r: *mut Reader) {
    if !r.is_null() {
        drop(Box::from_raw(r));
    }
}

/// Caps the input held while one token (string, comment) is incomplete;
/// beyond it the reader reports TooLarge. 0 or less restores the default
/// (256 MiB).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wj_reader_set_max_token(r: *mut Reader, bytes: i64) {
    if let Some(r) = r.as_mut() {
        r.set_max_token(if bytes <= 0 { 0 } else { usize::try_from(bytes).unwrap_or(usize::MAX) });
    }
}

/// Appends input. Returns 0, or -1 after wj_reader_finish / on bad arguments.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wj_reader_feed(r: *mut Reader, data: *const u8, len: i64) -> i32 {
    match (r.as_mut(), bytes_arg(data, len)) {
        (Some(r), Some(d)) => {
            if r.feed(d) {
                0
            } else {
                -1
            }
        }
        _ => -1,
    }
}

/// Marks the end of input.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wj_reader_finish(r: *mut Reader) {
    if let Some(r) = r.as_mut() {
        r.finish();
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn wj_reader_next(r: *mut Reader) -> i32 {
    r.as_mut().map_or(-1, |r| r.next() as i32)
}

/// Key/string (decoded, NUL-terminated) or number lexeme of the last event;
/// valid until the next call to wj_reader_next.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wj_reader_text(r: *const Reader, out_len: *mut i64) -> *const u8 {
    match r.as_ref() {
        Some(r) => {
            write_len(out_len, r.text().len() as i64);
            r.text_cstr()
        }
        None => {
            write_len(out_len, 0);
            c"".as_ptr() as *const u8
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn wj_reader_depth(r: *const Reader) -> i32 {
    r.as_ref().map_or(0, |r| r.depth() as i32)
}

/// Completed top-level values so far.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wj_reader_documents(r: *const Reader) -> i64 {
    r.as_ref().map_or(0, |r| r.documents() as i64)
}

/// Bytes currently held (unconsumed input).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wj_reader_buffered(r: *const Reader) -> i64 {
    r.as_ref().map_or(0, |r| r.buffered() as i64)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn wj_reader_error_code(r: *const Reader) -> i32 {
    r.as_ref().and_then(|r| r.error()).map_or(0, |e| e.code as i32)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn wj_reader_error_message(r: *const Reader) -> *const u8 {
    let code = r.as_ref().and_then(|r| r.error()).map_or(ErrorCode::Ok, |e| e.code);
    code.message().as_ptr()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn wj_reader_error_offset(r: *const Reader) -> i64 {
    r.as_ref().and_then(|r| r.error()).map_or(0, |e| e.offset as i64)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn wj_reader_error_line(r: *const Reader) -> i64 {
    r.as_ref().and_then(|r| r.error()).map_or(0, |e| e.line as i64)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn wj_reader_error_column(r: *const Reader) -> i64 {
    r.as_ref().and_then(|r| r.error()).map_or(0, |e| e.column as i64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CStr;

    fn parse(s: &str) -> *mut Doc {
        unsafe { wj_parse(s.as_ptr(), s.len() as i64) }
    }

    fn string_of(doc: *const Doc, node: i32) -> String {
        let mut len = 0;
        let p = unsafe { wj_str(doc, node, &mut len) };
        assert!(!p.is_null());
        String::from_utf8(unsafe { std::slice::from_raw_parts(p, len as usize) }.to_vec()).unwrap()
    }

    #[test]
    fn parses_nested_document() {
        let d = parse(r#"{"a":[1,2.5,{"b":"x\ny"}],"t":true,"n":null}"#);
        unsafe {
            assert_eq!(wj_ok(d), 1);
            let root = wj_root(d);
            assert_eq!(wj_type(d, root), 5);
            assert_eq!(wj_len(d, root), 3);
            let a = wj_get(d, root, b"a".as_ptr(), 1);
            assert_eq!(wj_len(d, a), 3);
            assert_eq!(wj_num_is_int(d, wj_at(d, a, 0)), 1);
            assert_eq!(wj_num_is_int(d, wj_at(d, a, 1)), 0);
            let p = b"/a/2/b";
            let b = wj_pointer(d, root, p.as_ptr(), p.len() as i64);
            assert_eq!(string_of(d, b), "x\ny");
            assert_eq!(wj_bool(d, wj_get(d, root, b"t".as_ptr(), 1)), 1);
            assert_eq!(wj_type(d, wj_get(d, root, b"n".as_ptr(), 1)), 0);
            assert_eq!(wj_get(d, root, b"zz".as_ptr(), 2), -1);
            wj_free(d);
        }
    }

    #[test]
    fn decodes_unicode_escapes() {
        let d = parse(r#"["\u00e9\ud83d\ude00\u0041"]"#);
        unsafe {
            let s = wj_at(d, wj_root(d), 0);
            assert_eq!(string_of(d, s), "é😀A");
            wj_free(d);
        }
    }

    #[test]
    fn reports_error_positions() {
        let d = parse("{\n  \"a\": [1, 2,]\n}");
        unsafe {
            assert_eq!(wj_ok(d), 0);
            assert_eq!(wj_error_code(d), ErrorCode::UnexpectedChar as i32);
            assert_eq!(wj_error_line(d), 2);
            assert_eq!(wj_error_column(d), 14);
            let msg = CStr::from_ptr(wj_error_message(d) as *const _);
            assert_eq!(msg.to_str().unwrap(), "unexpected character");
            assert_eq!(wj_root(d), -1);
            wj_free(d);
        }
    }

    #[test]
    fn relaxed_options() {
        let s = "{ // c\n \"a\": [1, 2,], /* x */ }";
        unsafe {
            let d = wj_parse_ex(s.as_ptr(), s.len() as i64, 0, 3);
            assert_eq!(wj_ok(d), 1);
            wj_free(d);
            let d = wj_parse(s.as_ptr(), s.len() as i64);
            assert_eq!(wj_ok(d), 0);
            wj_free(d);
        }
    }

    #[test]
    fn rejects_invalid_inputs() {
        let bad = [
            "", " ", "01", "1.", "-", "1e", "[1 2]", "{\"a\" 1}", "\"\\x\"", "\"\\ud800\"",
            "\"a\tb\"", "tru", "nul", "[1]x", "{,}", "[,]", "+1", ".5", "\"abc",
        ];
        for s in bad {
            let d = parse(s);
            unsafe {
                assert_eq!(wj_ok(d), 0, "should reject {s:?}");
                wj_free(d);
            }
        }
        let d = unsafe { wj_parse(b"[\"\xff\"]".as_ptr(), 5) };
        unsafe {
            assert_eq!(wj_error_code(d), ErrorCode::InvalidUtf8 as i32);
            wj_free(d);
        }
    }

    #[test]
    fn depth_limit() {
        let deep = "[".repeat(600) + &"]".repeat(600);
        unsafe {
            let d = parse(&deep);
            assert_eq!(wj_error_code(d), ErrorCode::TooDeep as i32);
            wj_free(d);
            let d = wj_parse_ex(deep.as_ptr(), deep.len() as i64, 1000, 0);
            assert_eq!(wj_ok(d), 1);
            wj_free(d);
        }
    }

    #[test]
    fn duplicate_keys_last_wins_and_null_safety() {
        let d = parse(r#"{"k":1,"k":2}"#);
        unsafe {
            let v = wj_get(d, wj_root(d), b"k".as_ptr(), 1);
            let mut len = 0;
            let p = wj_num(d, v, &mut len);
            assert_eq!(std::slice::from_raw_parts(p, len as usize), b"2");
            wj_free(d);
            assert_eq!(wj_type(ptr::null(), 0), -1);
            assert_eq!(wj_root(ptr::null()), -1);
            assert_eq!(wj_at(ptr::null(), 0, 0), -1);
            wj_free(ptr::null_mut());
        }
    }

    fn num_text(d: *const Doc, node: i32) -> String {
        let mut len = 0;
        let p = unsafe { wj_num(d, node, &mut len) };
        String::from_utf8(unsafe { std::slice::from_raw_parts(p, len as usize) }.to_vec()).unwrap()
    }

    #[test]
    fn edit_build_document_from_scratch() {
        unsafe {
            let d = wj_new_doc();
            assert_eq!(wj_ok(d), 1);
            assert_eq!(wj_root(d), -1);
            let obj = wj_new_object(d);
            assert_eq!(wj_set_root(d, obj), 0);
            let n = wj_new_number(d, b"42".as_ptr(), 2);
            assert_eq!(wj_obj_set(d, obj, b"a".as_ptr(), 1, n), 0);
            let arr = wj_new_array(d);
            assert_eq!(wj_obj_set(d, obj, b"list".as_ptr(), 4, arr), 0);
            for i in 0..100 {
                let s = i.to_string();
                let v = wj_new_number(d, s.as_ptr(), s.len() as i64);
                assert_eq!(wj_arr_push(d, arr, v), 0);
            }
            assert_eq!(wj_len(d, arr), 100);
            assert_eq!(num_text(d, wj_at(d, arr, 99)), "99");
            assert_eq!(wj_new_number(d, b"01".as_ptr(), 2), EditError::BadValue as i32);
            assert_eq!(wj_new_string(d, b"\xff".as_ptr(), 1), EditError::BadValue as i32);
            assert_eq!(wj_root(d), obj);
            wj_free(d);
        }
    }

    #[test]
    fn edit_parsed_document() {
        let d = parse(r#"{"a":1,"b":[1,2,3],"c":{"x":true}}"#);
        unsafe {
            let root = wj_root(d);
            let b = wj_get(d, root, b"b".as_ptr(), 1);
            // replace a
            let s = wj_new_string(d, b"hi".as_ptr(), 2);
            let old_a = wj_get(d, root, b"a".as_ptr(), 1);
            assert_eq!(wj_obj_set(d, root, b"a".as_ptr(), 1, s), 0);
            assert_eq!(string_of(d, wj_get(d, root, b"a".as_ptr(), 1)), "hi");
            assert_eq!(wj_parent(d, old_a), -1, "replaced value is detached");
            // insert / set / remove in array
            let z = wj_new_number(d, b"0".as_ptr(), 1);
            assert_eq!(wj_arr_insert(d, b, 0, z), 0);
            assert_eq!((0..4).map(|i| num_text(d, wj_at(d, b, i))).collect::<Vec<_>>(), ["0", "1", "2", "3"]);
            let removed = wj_arr_remove(d, b, 2);
            assert_eq!(num_text(d, removed), "2");
            assert_eq!(wj_len(d, b), 3);
            assert_eq!(wj_arr_set(d, b, 5, z), EditError::NotFound as i32);
            // re-attach the removed node at the end
            assert_eq!(wj_arr_push(d, b, removed), 0);
            assert_eq!(num_text(d, wj_at(d, b, 3)), "2");
            // remove a key
            assert_eq!(wj_obj_remove(d, root, b"c".as_ptr(), 1), 1);
            assert_eq!(wj_get(d, root, b"c".as_ptr(), 1), -1);
            assert_eq!(wj_obj_remove(d, root, b"c".as_ptr(), 1), 0);
            assert_eq!(wj_len(d, root), 2);
            wj_free(d);
        }
    }

    #[test]
    fn edit_refuses_shared_values_and_cycles() {
        let d = parse(r#"{"a":[1],"b":{}}"#);
        unsafe {
            let root = wj_root(d);
            let a = wj_get(d, root, b"a".as_ptr(), 1);
            let b = wj_get(d, root, b"b".as_ptr(), 1);
            // a is attached: cannot also go under b
            assert_eq!(wj_obj_set(d, b, b"k".as_ptr(), 1, a), EditError::Attached as i32);
            // root into its own child
            assert_eq!(wj_arr_push(d, a, root), EditError::Attached as i32);
            // detached container under its own descendant
            let outer = wj_new_array(d);
            let inner = wj_new_array(d);
            assert_eq!(wj_arr_push(d, outer, inner), 0);
            assert_eq!(wj_arr_push(d, inner, outer), EditError::Cycle as i32);
            assert_eq!(wj_arr_push(d, outer, outer), EditError::Cycle as i32);
            // wrong kinds
            assert_eq!(wj_arr_push(d, b, inner), EditError::WrongType as i32);
            // copy instead of sharing
            let c = wj_copy(d, d, a);
            assert!(c >= 0);
            assert_eq!(wj_obj_set(d, b, b"k".as_ptr(), 1, c), 0);
            assert_eq!(wj_len(d, wj_get(d, b, b"k".as_ptr(), 1)), 1);
            // failed documents cannot be edited
            let bad = parse("[");
            assert_eq!(wj_new_null(bad), -1);
            wj_free(bad);
            wj_free(d);
        }
    }

    #[test]
    fn copy_between_documents_keeps_duplicates_and_lexemes() {
        let src = parse(r#"{"k":1,"k":2.50,"s":"\u00e9"}"#);
        unsafe {
            let dst = wj_new_doc();
            let c = wj_copy(dst, src, wj_root(src));
            assert_eq!(wj_set_root(dst, c), 0);
            wj_free(src);
            let r = wj_root(dst);
            assert_eq!(wj_len(dst, r), 3);
            assert_eq!(num_text(dst, wj_get(dst, r, b"k".as_ptr(), 1)), "2.50");
            assert_eq!(string_of(dst, wj_get(dst, r, b"s".as_ptr(), 1)), "é");
            wj_free(dst);
        }
    }

    #[test]
    fn large_object_index_matches_linear_scan() {
        // 500 keys with duplicates; the indexed lookup must agree with "last wins".
        let mut text = String::from("{");
        for i in 0..500 {
            if i > 0 {
                text.push(',');
            }
            text.push_str(&format!("\"k{}\":{}", i % 300, i));
        }
        text.push('}');
        let d = parse(&text);
        unsafe {
            let root = wj_root(d);
            for k in 0..300 {
                let key = format!("k{k}");
                let v = wj_get(d, root, key.as_ptr(), key.len() as i64);
                let expect = if k < 200 { k + 300 } else { k };
                assert_eq!(num_text(d, v), expect.to_string(), "key {key}");
            }
            // mutations keep the index coherent
            let n = wj_new_number(d, b"-1".as_ptr(), 2);
            assert_eq!(wj_obj_set(d, root, b"new".as_ptr(), 3, n), 0);
            assert_eq!(num_text(d, wj_get(d, root, b"new".as_ptr(), 3)), "-1");
            assert_eq!(wj_obj_remove(d, root, b"k5".as_ptr(), 2), 2);
            assert_eq!(wj_get(d, root, b"k5".as_ptr(), 2), -1);
            assert_eq!(num_text(d, wj_get(d, root, b"k6".as_ptr(), 2)), "306");
            wj_free(d);
        }
    }

    #[test]
    fn edit_with_bytes_from_the_same_document() {
        // Keys/strings read from a document and written back into it must
        // survive the byte table growing (and moving) during the write.
        let d = parse(r#"{"name":"wave","long_key_name_here":1}"#);
        unsafe {
            let root = wj_root(d);
            for round in 0..200 {
                let mut klen = 0;
                let key = wj_key(d, root, 1, &mut klen);
                let mut slen = 0;
                let text = wj_str(d, wj_get(d, root, b"name".as_ptr(), 4), &mut slen);
                let v = wj_new_string(d, text, slen);
                assert!(v >= 0);
                assert_eq!(wj_obj_set(d, root, key, klen, v), 0, "round {round}");
                let mut len = 0;
                let p = wj_key(d, root, 1, &mut len);
                assert_eq!(std::slice::from_raw_parts(p, len as usize), b"long_key_name_here");
                assert_eq!(string_of(d, wj_get(d, root, b"long_key_name_here".as_ptr(), 18)), "wave");
                // Text handed out earlier must stay valid while the document
                // keeps growing.
                let filler = "x".repeat(round * 37 + 1);
                assert!(wj_new_string(d, filler.as_ptr(), filler.len() as i64) >= 0);
                assert_eq!(std::slice::from_raw_parts(key, klen as usize), b"long_key_name_here");
                assert_eq!(std::slice::from_raw_parts(text, slen as usize), b"wave");
            }
            wj_free(d);
        }
    }
    #[test]
    fn clearing_an_object_detaches_members_and_resets_the_index() {
        unsafe {
            let d = wj_new_doc();
            let obj = wj_new_object(d);
            wj_set_root(d, obj);
            let mut values = Vec::new();
            for i in 0..5000 {
                let key = format!("k{i}");
                let v = wj_new_number(d, i.to_string().as_ptr(), i.to_string().len() as i64);
                assert_eq!(wj_obj_set(d, obj, key.as_ptr(), key.len() as i64, v), 0);
                values.push(v);
            }
            // Large object: this builds the lazy key index.
            assert_eq!(wj_get(d, obj, b"k42".as_ptr(), 3), values[42]);
            assert_eq!(wj_obj_clear(d, obj), 5000);
            assert_eq!(wj_len(d, obj), 0);
            assert_eq!(wj_get(d, obj, b"k42".as_ptr(), 3), -1, "stale index entry survived the clear");
            assert_eq!(wj_parent(d, values[42]), -1, "cleared members are detached");
            // The object is reusable, and a detached value can be attached again.
            assert_eq!(wj_obj_set(d, obj, b"k42".as_ptr(), 3, values[42]), 0);
            assert_eq!(wj_get(d, obj, b"k42".as_ptr(), 3), values[42]);
            assert_eq!(wj_len(d, obj), 1);
            assert_eq!(wj_obj_clear(d, obj), 1);
            let arr = wj_new_array(d);
            assert_eq!(wj_obj_clear(d, arr), EditError::WrongType as i32);
            wj_free(d);
        }
    }

    #[test]
    fn max_depth_is_clamped_to_the_hard_limit() {
        // Debug builds use big stack frames; run on a roomy thread.
        std::thread::Builder::new()
            .stack_size(256 << 20)
            .spawn(|| unsafe {
                let limit = parser::MAX_DEPTH_LIMIT as usize;
                let too_deep = "[".repeat(limit + 1) + &"]".repeat(limit + 1);
                let d = wj_parse_ex(too_deep.as_ptr(), too_deep.len() as i64, i32::MAX, 0);
                assert_eq!(wj_ok(d), 0);
                assert_eq!(wj_error_code(d), ErrorCode::TooDeep as i32);
                wj_free(d);

                let fits = "[".repeat(limit) + &"]".repeat(limit);
                let d = wj_parse_ex(fits.as_ptr(), fits.len() as i64, i32::MAX, 0);
                assert_eq!(wj_ok(d), 1);

                // Copying is bounded as well: a document nested deeper than
                // the limit (possible through edits) cannot be deep-copied.
                let e = wj_new_doc();
                let top = wj_new_array(e);
                let mut chain = top;
                for _ in 0..limit + 10 {
                    let inner = wj_new_array(e);
                    assert_eq!(wj_arr_push(e, chain, inner), 0);
                    chain = inner;
                }
                assert_eq!(wj_copy(e, e, top), EditError::TooDeep as i32);
                wj_free(e);
                wj_free(d);
            })
            .unwrap()
            .join()
            .unwrap();
    }

    #[test]
    fn concurrent_lookups_on_a_shared_document() {
        // Large objects build their key index lazily on first lookup; many
        // threads hitting that for the first time must be safe.
        let members: Vec<String> = (0..200).map(|i| format!("\"k{i}\":{i}")).collect();
        let text = format!("{{{}}}", members.join(","));
        let dom = parser::parse(text.as_bytes(), 0, 0).unwrap();
        let root = dom.root as i64;
        std::thread::scope(|scope| {
            for t in 0..8 {
                let dom = &dom;
                scope.spawn(move || {
                    for round in 0..2000 {
                        let i = (round * 7 + t) % 200;
                        let key = format!("k{i}");
                        let id = dom.get(root, key.as_bytes()).expect("key present");
                        assert_eq!(dom.slice(dom.nodes[id as usize].start, dom.nodes[id as usize].len), i.to_string().as_bytes());
                        assert!(dom.get(root, b"missing").is_none());
                    }
                });
            }
        });
    }
}
