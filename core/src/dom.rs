// SPDX-License-Identifier: MPL-2.0
//! Flat, index-based document model.
//!
//! Every value is a `Node` addressed by a `u32` id. Containers keep their
//! children in one shared side table each, so a parsed document is four
//! vectors and no per-node allocation. Ids are stable for the life of the
//! document, which makes them easy to hand across the C ABI as plain integers.

pub const KIND_NULL: u8 = 0;
pub const KIND_BOOL: u8 = 1;
pub const KIND_NUMBER: u8 = 2;
pub const KIND_STRING: u8 = 3;
pub const KIND_ARRAY: u8 = 4;
pub const KIND_OBJECT: u8 = 5;

/// Number flag: lexeme has no fraction and no exponent.
pub const NUM_INTEGER: u8 = 1;
/// Number flag: lexeme starts with '-'.
pub const NUM_NEGATIVE: u8 = 2;

#[derive(Clone, Copy, Debug)]
pub struct Node {
    pub kind: u8,
    /// Bool value, or NUM_* flags for numbers.
    pub flag: u8,
    /// String/number: offset into `bytes`. Array: offset into `elems`.
    /// Object: offset into `members`.
    pub start: u32,
    /// String/number: byte length (excluding the NUL). Containers: child count.
    pub len: u32,
}

#[derive(Clone, Copy, Debug)]
pub struct Member {
    pub key_start: u32,
    pub key_len: u32,
    pub value: u32,
}

/// Parent marker for the document root.
pub const PARENT_ROOT: i32 = -2;
/// Parent marker for a node that is not attached anywhere.
pub const PARENT_NONE: i32 = -1;
/// `root` value of a document that has no root yet.
pub const NO_ROOT: u32 = u32::MAX;

/// `start` values with this bit refer to `extra` (edit-created text).
pub const EXTRA_BIT: u32 = 1 << 31;

/// Objects with at least this many members get a lazily built hash index.
pub const INDEX_THRESHOLD: u32 = 16;

/// Key -> member position for one object.
pub type KeyIndex = std::collections::HashMap<Box<[u8]>, u32>;
/// Node id of an object -> its key index.
pub type IndexCache = std::collections::HashMap<u32, KeyIndex>;

#[derive(Debug)]
pub struct Dom {
    pub nodes: Vec<Node>,
    /// Decoded string data and raw number lexemes from parsing, each
    /// NUL-terminated so the Wave side can treat them as `str` without
    /// copying. Written only while parsing, so pointers handed out stay valid
    /// until the document is freed.
    pub bytes: Vec<u8>,
    /// Text created by edits: one stable allocation per string (also
    /// NUL-terminated). Addressed by `start` values with EXTRA_BIT set.
    pub extra: Vec<Box<[u8]>>,
    pub elems: Vec<u32>,
    pub members: Vec<Member>,
    pub root: u32,
    /// Parent node of every node (PARENT_ROOT / PARENT_NONE for the rest).
    pub parent: Vec<i32>,
    /// Lazily built key -> member position maps for large objects. Lookups
    /// take `&self`, so the cache sits behind a mutex: a document that is not
    /// being edited may be read from several threads at once.
    pub index: std::sync::Mutex<IndexCache>,
}

impl Default for Dom {
    fn default() -> Self {
        Dom {
            nodes: Vec::new(),
            bytes: Vec::new(),
            extra: Vec::new(),
            elems: Vec::new(),
            members: Vec::new(),
            root: NO_ROOT,
            parent: Vec::new(),
            index: Default::default(),
        }
    }
}

impl Dom {
    pub fn node(&self, id: i64) -> Option<&Node> {
        if id < 0 {
            return None;
        }
        self.nodes.get(id as usize)
    }

    pub fn slice(&self, start: u32, len: u32) -> &[u8] {
        if start & EXTRA_BIT != 0 {
            &self.extra[(start & !EXTRA_BIT) as usize][..len as usize]
        } else {
            let s = start as usize;
            &self.bytes[s..s + len as usize]
        }
    }

    /// Address of NUL-terminated text (stable until the document is freed).
    pub fn text_ptr(&self, start: u32) -> *const u8 {
        if start & EXTRA_BIT != 0 {
            self.extra[(start & !EXTRA_BIT) as usize].as_ptr()
        } else {
            // SAFETY: start indexes inside `bytes` (or one past a NUL).
            unsafe { self.bytes.as_ptr().add(start as usize) }
        }
    }

    /// Parser-only: append to the contiguous parse table.
    pub fn push_bytes(&mut self, data: &[u8]) -> (u32, u32) {
        let start = self.bytes.len() as u32;
        self.bytes.extend_from_slice(data);
        self.bytes.push(0);
        (start, data.len() as u32)
    }

    /// Edits: store text in its own allocation so nothing handed out earlier
    /// ever moves, and so `data` may safely point into this document.
    pub fn push_text(&mut self, data: &[u8]) -> (u32, u32) {
        let mut owned = Vec::with_capacity(data.len() + 1);
        owned.extend_from_slice(data);
        owned.push(0);
        let index = self.extra.len() as u32;
        self.extra.push(owned.into_boxed_slice());
        (index | EXTRA_BIT, data.len() as u32)
    }

    pub fn push_node(&mut self, node: Node) -> u32 {
        let id = self.nodes.len() as u32;
        self.nodes.push(node);
        self.parent.push(PARENT_NONE);
        id
    }

    /// Array element `index` or object member value `index`.
    pub fn child(&self, id: i64, index: i64) -> Option<u32> {
        let n = self.node(id)?;
        if index < 0 || index >= n.len as i64 {
            return None;
        }
        let at = n.start as usize + index as usize;
        match n.kind {
            KIND_ARRAY => self.elems.get(at).copied(),
            KIND_OBJECT => self.members.get(at).map(|m| m.value),
            _ => None,
        }
    }

    pub fn member(&self, id: i64, index: i64) -> Option<&Member> {
        let n = self.node(id)?;
        if n.kind != KIND_OBJECT || index < 0 || index >= n.len as i64 {
            return None;
        }
        self.members.get(n.start as usize + index as usize)
    }

    /// Position (within the object's member list) of the last member named
    /// `key`. Small objects are scanned; large ones use a lazily built hash
    /// index that mutations invalidate.
    pub fn find_member(&self, id: u32, key: &[u8]) -> Option<u32> {
        let n = self.nodes.get(id as usize)?;
        if n.kind != KIND_OBJECT {
            return None;
        }
        let first = n.start as usize;
        let members = &self.members[first..first + n.len as usize];
        if n.len < INDEX_THRESHOLD {
            return members
                .iter()
                .rposition(|m| self.slice(m.key_start, m.key_len) == key)
                .map(|p| p as u32);
        }
        let mut index = self.index.lock().unwrap_or_else(|e| e.into_inner());
        let map = index.entry(id).or_insert_with(|| {
            let mut map = std::collections::HashMap::with_capacity(members.len());
            for (pos, m) in members.iter().enumerate() {
                // Later duplicates overwrite earlier ones: last one wins.
                map.insert(self.slice(m.key_start, m.key_len).into(), pos as u32);
            }
            map
        });
        map.get(key).copied()
    }

    /// Drops the cached key index of one object (after its keys changed).
    pub fn invalidate_index(&self, id: u32) {
        self.index.lock().unwrap_or_else(|e| e.into_inner()).remove(&id);
    }

    /// Object lookup. Duplicate keys resolve to the *last* occurrence,
    /// matching JavaScript's `JSON.parse`.
    pub fn get(&self, id: i64, key: &[u8]) -> Option<u32> {
        let id32 = u32::try_from(id).ok()?;
        let pos = self.find_member(id32, key)?;
        let n = self.nodes[id32 as usize];
        Some(self.members[n.start as usize + pos as usize].value)
    }

    /// RFC 6901 JSON Pointer ("/a/0/b", "~0" = '~', "~1" = '/').
    /// The empty pointer selects `id` itself.
    pub fn pointer(&self, id: i64, pointer: &[u8]) -> Option<u32> {
        let mut cur = u32::try_from(id).ok()?;
        self.node(id)?;
        if pointer.is_empty() {
            return Some(cur);
        }
        if pointer[0] != b'/' {
            return None;
        }
        let mut token: Vec<u8> = Vec::new();
        for raw in pointer[1..].split(|&b| b == b'/') {
            token.clear();
            let mut i = 0;
            while i < raw.len() {
                if raw[i] == b'~' {
                    match raw.get(i + 1) {
                        Some(b'0') => token.push(b'~'),
                        Some(b'1') => token.push(b'/'),
                        _ => return None,
                    }
                    i += 2;
                } else {
                    token.push(raw[i]);
                    i += 1;
                }
            }
            let node = self.nodes[cur as usize];
            cur = match node.kind {
                KIND_OBJECT => self.get(cur as i64, &token)?,
                KIND_ARRAY => {
                    let index = parse_array_index(&token)?;
                    self.child(cur as i64, index)?
                }
                _ => return None,
            };
        }
        Some(cur)
    }
}

/// Array index token per RFC 6901: "0" or a digit string without a leading zero.
fn parse_array_index(token: &[u8]) -> Option<i64> {
    if token.is_empty() || token.len() > 18 {
        return None;
    }
    if token.len() > 1 && token[0] == b'0' {
        return None;
    }
    let mut value: i64 = 0;
    for &b in token {
        if !b.is_ascii_digit() {
            return None;
        }
        value = value * 10 + (b - b'0') as i64;
    }
    Some(value)
}
