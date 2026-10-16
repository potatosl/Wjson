// SPDX-License-Identifier: MIT
//! In-place editing of a parsed (or freshly created) document.
//!
//! Model:
//! * New values are created *detached* (no parent). Attaching puts a detached
//!   value under exactly one parent; attaching an already attached value is
//!   refused (copy it first), so the document always stays a tree.
//! * Attaching a container under one of its own descendants is refused
//!   (cycle check walks the parent chain).
//! * Replacing or removing a child detaches it; it can be attached again.
//! * Child lists live in shared side tables. A list that must grow and is not
//!   at the end of its table is moved to the end with doubled capacity, so
//!   repeated appends are amortized O(1). The old slots become garbage that
//!   is reclaimed when the document is freed.

use crate::dom::*;
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum EditError {
    /// Bad handle/node id, or the document failed to parse.
    Invalid = -1,
    /// The value already has a parent (or is the root); copy it first.
    Attached = -2,
    /// The value is the target container or one of its ancestors.
    Cycle = -3,
    /// No such key / index.
    NotFound = -4,
    /// Container operation on the wrong kind of value.
    WrongType = -5,
    /// Not a valid JSON number lexeme / not valid UTF-8.
    BadValue = -6,
}

pub type EditResult<T> = Result<T, EditError>;

/// Extra capacity per relocated container (node id -> capacity).
#[derive(Default, Debug)]
pub struct Slack {
    pub cap: HashMap<u32, u32>,
}

/// RFC 8259 number grammar.
pub fn is_json_number(s: &[u8]) -> bool {
    let mut i = 0;
    let digits = |i: &mut usize| {
        let st = *i;
        while *i < s.len() && s[*i].is_ascii_digit() {
            *i += 1;
        }
        *i - st
    };
    if s.get(i) == Some(&b'-') {
        i += 1;
    }
    match s.get(i) {
        Some(b'0') => i += 1,
        Some(b'1'..=b'9') => {
            digits(&mut i);
        }
        _ => return false,
    }
    if s.get(i) == Some(&b'.') {
        i += 1;
        if digits(&mut i) == 0 {
            return false;
        }
    }
    if matches!(s.get(i), Some(b'e' | b'E')) {
        i += 1;
        if matches!(s.get(i), Some(b'+' | b'-')) {
            i += 1;
        }
        if digits(&mut i) == 0 {
            return false;
        }
    }
    i == s.len()
}

fn number_flags(s: &[u8]) -> u8 {
    let mut flag = NUM_INTEGER;
    if s.iter().any(|&b| matches!(b, b'.' | b'e' | b'E')) {
        flag &= !NUM_INTEGER;
    }
    if s.first() == Some(&b'-') {
        flag |= NUM_NEGATIVE;
    }
    flag
}

impl Dom {
    fn check(&self, id: i64) -> EditResult<u32> {
        let id32 = u32::try_from(id).map_err(|_| EditError::Invalid)?;
        if (id32 as usize) < self.nodes.len() {
            Ok(id32)
        } else {
            Err(EditError::Invalid)
        }
    }

    fn check_kind(&self, id: i64, kind: u8) -> EditResult<u32> {
        let id = self.check(id)?;
        if self.nodes[id as usize].kind != kind {
            return Err(EditError::WrongType);
        }
        Ok(id)
    }

    // ---- creating detached values ------------------------------------------

    pub fn new_null(&mut self) -> u32 {
        self.push_node(Node { kind: KIND_NULL, flag: 0, start: 0, len: 0 })
    }

    pub fn new_bool(&mut self, value: bool) -> u32 {
        self.push_node(Node { kind: KIND_BOOL, flag: value as u8, start: 0, len: 0 })
    }

    pub fn new_number(&mut self, lexeme: &[u8]) -> EditResult<u32> {
        if !is_json_number(lexeme) {
            return Err(EditError::BadValue);
        }
        let flag = number_flags(lexeme);
        let (start, len) = self.push_text(lexeme);
        Ok(self.push_node(Node { kind: KIND_NUMBER, flag, start, len }))
    }

    pub fn new_string(&mut self, text: &[u8]) -> EditResult<u32> {
        if std::str::from_utf8(text).is_err() {
            return Err(EditError::BadValue);
        }
        let (start, len) = self.push_text(text);
        Ok(self.push_node(Node { kind: KIND_STRING, flag: 0, start, len }))
    }

    pub fn new_array(&mut self) -> u32 {
        let start = self.elems.len() as u32;
        self.push_node(Node { kind: KIND_ARRAY, flag: 0, start, len: 0 })
    }

    pub fn new_object(&mut self) -> u32 {
        let start = self.members.len() as u32;
        self.push_node(Node { kind: KIND_OBJECT, flag: 0, start, len: 0 })
    }

    // ---- attachment rules ---------------------------------------------------

    /// `value` must be detached and must not be `container` or an ancestor of it.
    fn check_attach(&self, container: u32, value: u32) -> EditResult<()> {
        if self.parent[value as usize] != PARENT_NONE {
            return Err(EditError::Attached);
        }
        let mut cur = container as i32;
        while cur >= 0 {
            if cur as u32 == value {
                return Err(EditError::Cycle);
            }
            cur = self.parent[cur as usize];
        }
        Ok(())
    }

    fn detach(&mut self, value: u32) {
        self.parent[value as usize] = PARENT_NONE;
    }

    pub fn set_root(&mut self, value: i64) -> EditResult<()> {
        let value = self.check(value)?;
        if value == self.root {
            return Ok(());
        }
        if self.parent[value as usize] != PARENT_NONE {
            return Err(EditError::Attached);
        }
        if self.root != NO_ROOT {
            let old = self.root;
            self.detach(old);
        }
        self.root = value;
        self.parent[value as usize] = PARENT_ROOT;
        Ok(())
    }

    /// Detached nodes (e.g. a removed member) report -1 from parent_of.
    pub fn parent_of(&self, id: i64) -> EditResult<i32> {
        let id = self.check(id)?;
        Ok(self.parent[id as usize])
    }

    // ---- growing child lists ------------------------------------------------

    fn capacity(&self, slack: &Slack, id: u32) -> u32 {
        slack.cap.get(&id).copied().unwrap_or(self.nodes[id as usize].len)
    }

    /// Makes room for one more element in array `arr`, relocating if needed.
    fn reserve_elem(&mut self, slack: &mut Slack, arr: u32) {
        let n = self.nodes[arr as usize];
        let (start, len) = (n.start as usize, n.len as usize);
        if len < self.capacity(slack, arr) as usize {
            return;
        }
        if start + len == self.elems.len() {
            self.elems.push(u32::MAX); // at the tail: grow in place
            slack.cap.insert(arr, (len + 1) as u32);
            return;
        }
        let cap = (len * 2).max(4);
        let new_start = self.elems.len();
        self.elems.extend_from_within(start..start + len);
        self.elems.resize(new_start + cap, u32::MAX);
        self.nodes[arr as usize].start = new_start as u32;
        slack.cap.insert(arr, cap as u32);
    }

    fn reserve_member(&mut self, slack: &mut Slack, obj: u32) {
        let n = self.nodes[obj as usize];
        let (start, len) = (n.start as usize, n.len as usize);
        if len < self.capacity(slack, obj) as usize {
            return;
        }
        let blank = Member { key_start: 0, key_len: 0, value: u32::MAX };
        if start + len == self.members.len() {
            self.members.push(blank);
            slack.cap.insert(obj, (len + 1) as u32);
            return;
        }
        let cap = (len * 2).max(4);
        let new_start = self.members.len();
        self.members.extend_from_within(start..start + len);
        self.members.resize(new_start + cap, blank);
        self.nodes[obj as usize].start = new_start as u32;
        slack.cap.insert(obj, cap as u32);
    }

    // ---- arrays ---------------------------------------------------------------

    /// Inserts `value` at `index` (index == len appends).
    pub fn arr_insert(&mut self, slack: &mut Slack, arr: i64, index: i64, value: i64) -> EditResult<()> {
        let arr = self.check_kind(arr, KIND_ARRAY)?;
        let value = self.check(value)?;
        let len = self.nodes[arr as usize].len as i64;
        if index < 0 || index > len {
            return Err(EditError::NotFound);
        }
        self.check_attach(arr, value)?;
        self.reserve_elem(slack, arr);
        let start = self.nodes[arr as usize].start as usize;
        let (idx, len) = (index as usize, len as usize);
        self.elems.copy_within(start + idx..start + len, start + idx + 1);
        self.elems[start + idx] = value;
        self.nodes[arr as usize].len += 1;
        self.parent[value as usize] = arr as i32;
        Ok(())
    }

    pub fn arr_push(&mut self, slack: &mut Slack, arr: i64, value: i64) -> EditResult<()> {
        let len = self.check_kind(arr, KIND_ARRAY).map(|a| self.nodes[a as usize].len)?;
        self.arr_insert(slack, arr, len as i64, value)
    }

    /// Replaces element `index`; the old element becomes detached.
    pub fn arr_set(&mut self, arr: i64, index: i64, value: i64) -> EditResult<()> {
        let arr = self.check_kind(arr, KIND_ARRAY)?;
        let value = self.check(value)?;
        let n = self.nodes[arr as usize];
        if index < 0 || index >= n.len as i64 {
            return Err(EditError::NotFound);
        }
        let slot = n.start as usize + index as usize;
        let old = self.elems[slot];
        if old == value {
            return Ok(());
        }
        self.check_attach(arr, value)?;
        self.elems[slot] = value;
        self.detach(old);
        self.parent[value as usize] = arr as i32;
        Ok(())
    }

    /// Removes element `index`; returns the detached element.
    pub fn arr_remove(&mut self, arr: i64, index: i64) -> EditResult<u32> {
        let arr = self.check_kind(arr, KIND_ARRAY)?;
        let n = self.nodes[arr as usize];
        if index < 0 || index >= n.len as i64 {
            return Err(EditError::NotFound);
        }
        let (start, len, idx) = (n.start as usize, n.len as usize, index as usize);
        let old = self.elems[start + idx];
        self.elems.copy_within(start + idx + 1..start + len, start + idx);
        self.nodes[arr as usize].len -= 1;
        self.detach(old);
        Ok(old)
    }

    // ---- objects --------------------------------------------------------------

    /// Sets `key` to `value`: replaces the value of the last member with that
    /// key, or appends a new member. A replaced value becomes detached.
    pub fn obj_set(&mut self, slack: &mut Slack, obj: i64, key: &[u8], value: i64) -> EditResult<()> {
        let obj = self.check_kind(obj, KIND_OBJECT)?;
        let value = self.check(value)?;
        if std::str::from_utf8(key).is_err() {
            return Err(EditError::BadValue);
        }
        if let Some(pos) = self.find_member(obj, key) {
            let slot = self.nodes[obj as usize].start as usize + pos as usize;
            let old = self.members[slot].value;
            if old == value {
                return Ok(());
            }
            self.check_attach(obj, value)?;
            self.members[slot].value = value;
            self.detach(old);
            self.parent[value as usize] = obj as i32;
            return Ok(());
        }
        self.check_attach(obj, value)?;
        let (key_start, key_len) = self.push_text(key);
        self.reserve_member(slack, obj);
        let n = self.nodes[obj as usize];
        self.members[n.start as usize + n.len as usize] = Member { key_start, key_len, value };
        self.nodes[obj as usize].len += 1;
        self.parent[value as usize] = obj as i32;
        // The cached index (if any) stays valid: append the new key to it.
        if let Some(map) = self.index.borrow_mut().get_mut(&obj) {
            map.insert(key.into(), n.len);
        }
        Ok(())
    }

    /// Removes every member named `key`; returns how many were removed.
    pub fn obj_remove(&mut self, obj: i64, key: &[u8]) -> EditResult<u32> {
        let obj = self.check_kind(obj, KIND_OBJECT)?;
        let n = self.nodes[obj as usize];
        let (start, len) = (n.start as usize, n.len as usize);
        let mut kept = 0usize;
        let mut removed = Vec::new();
        for i in 0..len {
            let m = self.members[start + i];
            if self.slice(m.key_start, m.key_len) == key {
                removed.push(m.value);
            } else {
                self.members[start + kept] = m;
                kept += 1;
            }
        }
        if removed.is_empty() {
            return Ok(0);
        }
        self.nodes[obj as usize].len = kept as u32;
        for v in &removed {
            self.detach(*v);
        }
        self.invalidate_index(obj);
        Ok(removed.len() as u32)
    }

    // ---- copying ----------------------------------------------------------------

    /// Deep-copies `src_node` of `src` into this document as a detached value.
    pub fn copy_from(&mut self, slack: &mut Slack, src: &Dom, src_node: i64) -> EditResult<u32> {
        let id = src.check(src_node)?;
        let n = src.nodes[id as usize];
        Ok(match n.kind {
            KIND_NULL => self.new_null(),
            KIND_BOOL => self.new_bool(n.flag != 0),
            KIND_NUMBER => {
                let (start, len) = self.push_text(src.slice(n.start, n.len));
                self.push_node(Node { kind: KIND_NUMBER, flag: n.flag, start, len })
            }
            KIND_STRING => {
                let (start, len) = self.push_text(src.slice(n.start, n.len));
                self.push_node(Node { kind: KIND_STRING, flag: 0, start, len })
            }
            KIND_ARRAY => {
                let arr = self.new_array();
                for i in 0..n.len as i64 {
                    let child = src.child(id as i64, i).ok_or(EditError::Invalid)?;
                    let copy = self.copy_from(slack, src, child as i64)?;
                    self.arr_push(slack, arr as i64, copy as i64)?;
                }
                arr
            }
            _ => {
                let obj = self.new_object();
                for i in 0..n.len as i64 {
                    let m = *src.member(id as i64, i).ok_or(EditError::Invalid)?;
                    let copy = self.copy_from(slack, src, m.value as i64)?;
                    // Append directly so duplicate keys survive the copy.
                    let key = src.slice(m.key_start, m.key_len).to_vec();
                    let (key_start, key_len) = self.push_text(&key);
                    self.reserve_member(slack, obj);
                    let on = self.nodes[obj as usize];
                    self.members[on.start as usize + on.len as usize] =
                        Member { key_start, key_len, value: copy };
                    self.nodes[obj as usize].len += 1;
                    self.parent[copy as usize] = obj as i32;
                }
                obj
            }
        })
    }
}
