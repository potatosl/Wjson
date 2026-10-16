// SPDX-License-Identifier: MPL-2.0
//! Strict RFC 8259 parser with two opt-in relaxations
//! (trailing commas and `//` / `/* */` comments).

use crate::dom::*;

pub const OPT_TRAILING_COMMAS: u32 = 1;
pub const OPT_COMMENTS: u32 = 2;

pub const DEFAULT_MAX_DEPTH: u32 = 512;

/// Hard ceiling for `max_depth`. The DOM parser (and deep copies) recurse once
/// per nesting level, so an unbounded limit would let hostile input overflow
/// the stack. Larger requests are clamped to this value.
pub const MAX_DEPTH_LIMIT: u32 = 2048;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum ErrorCode {
    Ok = 0,
    Empty = 1,
    UnexpectedChar = 2,
    UnexpectedEnd = 3,
    InvalidNumber = 4,
    InvalidString = 5,
    InvalidEscape = 6,
    InvalidUnicode = 7,
    ControlChar = 8,
    InvalidUtf8 = 9,
    TrailingData = 10,
    TooDeep = 11,
    TooLarge = 12,
    InvalidArgument = 13,
}

impl ErrorCode {
    pub fn message(self) -> &'static [u8] {
        match self {
            ErrorCode::Ok => b"ok\0",
            ErrorCode::Empty => b"empty input\0",
            ErrorCode::UnexpectedChar => b"unexpected character\0",
            ErrorCode::UnexpectedEnd => b"unexpected end of input\0",
            ErrorCode::InvalidNumber => b"invalid number\0",
            ErrorCode::InvalidString => b"invalid string\0",
            ErrorCode::InvalidEscape => b"invalid escape sequence\0",
            ErrorCode::InvalidUnicode => b"invalid \\u escape or lone surrogate\0",
            ErrorCode::ControlChar => b"unescaped control character in string\0",
            ErrorCode::InvalidUtf8 => b"invalid UTF-8\0",
            ErrorCode::TrailingData => b"trailing data after value\0",
            ErrorCode::TooDeep => b"nesting too deep\0",
            ErrorCode::TooLarge => b"document too large\0",
            ErrorCode::InvalidArgument => b"invalid argument\0",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ParseError {
    pub code: ErrorCode,
    pub offset: usize,
    pub line: usize,
    pub column: usize,
}

impl ParseError {
    fn at(src: &[u8], code: ErrorCode, offset: usize) -> Self {
        let offset = offset.min(src.len());
        let mut line = 1;
        let mut line_start = 0;
        for (i, &b) in src[..offset].iter().enumerate() {
            if b == b'\n' {
                line += 1;
                line_start = i + 1;
            }
        }
        // Column counts UTF-8 code points, 1-based.
        let column = src[line_start..offset]
            .iter()
            .filter(|&&b| (b & 0xC0) != 0x80)
            .count()
            + 1;
        ParseError { code, offset, line, column }
    }
}

type PResult<T> = Result<T, (ErrorCode, usize)>;

struct Parser<'a> {
    src: &'a [u8],
    pos: usize,
    depth: u32,
    max_depth: u32,
    options: u32,
    dom: Dom,
    scratch: Vec<u8>,
    /// Shared child stacks: a container pushes its children here and moves
    /// them into the DOM when it closes (no allocation per container).
    item_stack: Vec<u32>,
    member_stack: Vec<Member>,
}

pub fn parse(src: &[u8], max_depth: u32, options: u32) -> Result<Dom, ParseError> {
    // Parse-table offsets must stay below EXTRA_BIT (2 GiB); the table is
    // never larger than the input.
    if src.len() >= EXTRA_BIT as usize {
        return Err(ParseError::at(src, ErrorCode::TooLarge, 0));
    }
    // UTF-8 is validated inside strings as they are read (bytes >= 0x80 are
    // illegal elsewhere), so the first error in reading order is reported —
    // the same one the streaming parser reports.
    // Skip a UTF-8 BOM, which some editors emit.
    let start = if src.starts_with(&[0xEF, 0xBB, 0xBF]) { 3 } else { 0 };
    let mut p = Parser {
        src,
        pos: start,
        depth: 0,
        max_depth: if max_depth == 0 { DEFAULT_MAX_DEPTH } else { max_depth.min(MAX_DEPTH_LIMIT) },
        options,
        dom: Dom::default(),
        scratch: Vec::new(),
        item_stack: Vec::new(),
        member_stack: Vec::new(),
    };
    let run = |p: &mut Parser| -> PResult<u32> {
        p.skip_ws()?;
        if p.pos >= p.src.len() {
            return Err((ErrorCode::Empty, p.pos));
        }
        let root = p.value()?;
        p.skip_ws()?;
        if p.pos < p.src.len() {
            return Err((ErrorCode::TrailingData, p.pos));
        }
        Ok(root)
    };
    match run(&mut p) {
        Ok(root) => {
            p.dom.root = root;
            p.dom.parent[root as usize] = PARENT_ROOT;
            Ok(p.dom)
        }
        Err((code, offset)) => Err(ParseError::at(src, code, offset)),
    }
}

impl<'a> Parser<'a> {
    #[inline]
    fn peek(&self) -> Option<u8> {
        self.src.get(self.pos).copied()
    }

    fn skip_ws(&mut self) -> PResult<()> {
        loop {
            match self.peek() {
                Some(b' ' | b'\t' | b'\n' | b'\r') => self.pos += 1,
                Some(b'/') if self.options & OPT_COMMENTS != 0 => self.skip_comment()?,
                _ => return Ok(()),
            }
        }
    }

    fn skip_comment(&mut self) -> PResult<()> {
        let start = self.pos;
        match self.src.get(self.pos + 1) {
            Some(b'/') => {
                self.pos += 2;
                while let Some(b) = self.peek() {
                    self.pos += 1;
                    if b == b'\n' {
                        break;
                    }
                }
                Ok(())
            }
            Some(b'*') => {
                self.pos += 2;
                loop {
                    match self.peek() {
                        None => return Err((ErrorCode::UnexpectedEnd, start)),
                        Some(b'*') if self.src.get(self.pos + 1) == Some(&b'/') => {
                            self.pos += 2;
                            return Ok(());
                        }
                        Some(_) => self.pos += 1,
                    }
                }
            }
            _ => Err((ErrorCode::UnexpectedChar, start)),
        }
    }

    fn value(&mut self) -> PResult<u32> {
        match self.peek() {
            None => Err((ErrorCode::UnexpectedEnd, self.pos)),
            Some(b'{') => self.object(),
            Some(b'[') => self.array(),
            Some(b'"') => {
                let (start, len) = self.string()?;
                Ok(self.dom.push_node(Node { kind: KIND_STRING, flag: 0, start, len }))
            }
            Some(b't') => self.literal(b"true", KIND_BOOL, 1),
            Some(b'f') => self.literal(b"false", KIND_BOOL, 0),
            Some(b'n') => self.literal(b"null", KIND_NULL, 0),
            Some(b'-' | b'0'..=b'9') => self.number(),
            Some(_) => Err((ErrorCode::UnexpectedChar, self.pos)),
        }
    }

    fn literal(&mut self, word: &[u8], kind: u8, flag: u8) -> PResult<u32> {
        let end = self.pos + word.len();
        if end > self.src.len() {
            if word.starts_with(&self.src[self.pos..]) {
                return Err((ErrorCode::UnexpectedEnd, self.src.len()));
            }
            return Err((ErrorCode::UnexpectedChar, self.pos));
        }
        if &self.src[self.pos..end] != word {
            return Err((ErrorCode::UnexpectedChar, self.pos));
        }
        self.pos = end;
        Ok(self.dom.push_node(Node { kind, flag, start: 0, len: 0 }))
    }

    fn enter(&mut self) -> PResult<()> {
        self.depth += 1;
        if self.depth > self.max_depth {
            return Err((ErrorCode::TooDeep, self.pos));
        }
        Ok(())
    }

    fn array(&mut self) -> PResult<u32> {
        self.enter()?;
        self.pos += 1; // '['
        let base = self.item_stack.len();
        self.skip_ws()?;
        if self.peek() == Some(b']') {
            self.pos += 1;
        } else {
            loop {
                let v = self.value()?;
                self.item_stack.push(v);
                self.skip_ws()?;
                match self.peek() {
                    Some(b',') => {
                        self.pos += 1;
                        self.skip_ws()?;
                        if self.peek() == Some(b']') && self.options & OPT_TRAILING_COMMAS != 0 {
                            self.pos += 1;
                            break;
                        }
                    }
                    Some(b']') => {
                        self.pos += 1;
                        break;
                    }
                    None => return Err((ErrorCode::UnexpectedEnd, self.pos)),
                    Some(_) => return Err((ErrorCode::UnexpectedChar, self.pos)),
                }
            }
        }
        self.depth -= 1;
        let start = self.dom.elems.len() as u32;
        let len = (self.item_stack.len() - base) as u32;
        self.dom.elems.extend_from_slice(&self.item_stack[base..]);
        let id = self.dom.push_node(Node { kind: KIND_ARRAY, flag: 0, start, len });
        for &child in &self.item_stack[base..] {
            self.dom.parent[child as usize] = id as i32;
        }
        self.item_stack.truncate(base);
        Ok(id)
    }

    fn object(&mut self) -> PResult<u32> {
        self.enter()?;
        self.pos += 1; // '{'
        let base = self.member_stack.len();
        self.skip_ws()?;
        if self.peek() == Some(b'}') {
            self.pos += 1;
        } else {
            loop {
                match self.peek() {
                    Some(b'"') => {}
                    None => return Err((ErrorCode::UnexpectedEnd, self.pos)),
                    Some(_) => return Err((ErrorCode::UnexpectedChar, self.pos)),
                }
                let (key_start, key_len) = self.string()?;
                self.skip_ws()?;
                match self.peek() {
                    Some(b':') => self.pos += 1,
                    None => return Err((ErrorCode::UnexpectedEnd, self.pos)),
                    Some(_) => return Err((ErrorCode::UnexpectedChar, self.pos)),
                }
                self.skip_ws()?;
                let value = self.value()?;
                self.member_stack.push(Member { key_start, key_len, value });
                self.skip_ws()?;
                match self.peek() {
                    Some(b',') => {
                        self.pos += 1;
                        self.skip_ws()?;
                        if self.peek() == Some(b'}') && self.options & OPT_TRAILING_COMMAS != 0 {
                            self.pos += 1;
                            break;
                        }
                    }
                    Some(b'}') => {
                        self.pos += 1;
                        break;
                    }
                    None => return Err((ErrorCode::UnexpectedEnd, self.pos)),
                    Some(_) => return Err((ErrorCode::UnexpectedChar, self.pos)),
                }
            }
        }
        self.depth -= 1;
        let start = self.dom.members.len() as u32;
        let len = (self.member_stack.len() - base) as u32;
        self.dom.members.extend_from_slice(&self.member_stack[base..]);
        let id = self.dom.push_node(Node { kind: KIND_OBJECT, flag: 0, start, len });
        for m in &self.member_stack[base..] {
            self.dom.parent[m.value as usize] = id as i32;
        }
        self.member_stack.truncate(base);
        Ok(id)
    }

    fn number(&mut self) -> PResult<u32> {
        let start = self.pos;
        let src = self.src;
        let mut i = self.pos;
        let mut flag = NUM_INTEGER;
        let digits = |i: &mut usize| -> usize {
            let s = *i;
            while *i < src.len() && src[*i].is_ascii_digit() {
                *i += 1;
            }
            *i - s
        };
        if src.get(i) == Some(&b'-') {
            flag |= NUM_NEGATIVE;
            i += 1;
        }
        match src.get(i) {
            Some(b'0') => {
                i += 1;
                if src.get(i).is_some_and(|b| b.is_ascii_digit()) {
                    return Err((ErrorCode::InvalidNumber, i));
                }
            }
            Some(b'1'..=b'9') => {
                digits(&mut i);
            }
            None => return Err((ErrorCode::UnexpectedEnd, i)),
            Some(_) => return Err((ErrorCode::InvalidNumber, i)),
        }
        if src.get(i) == Some(&b'.') {
            i += 1;
            flag &= !NUM_INTEGER;
            if digits(&mut i) == 0 {
                return Err((ErrorCode::InvalidNumber, i));
            }
        }
        if matches!(src.get(i), Some(b'e' | b'E')) {
            i += 1;
            flag &= !NUM_INTEGER;
            if matches!(src.get(i), Some(b'+' | b'-')) {
                i += 1;
            }
            if digits(&mut i) == 0 {
                return Err((ErrorCode::InvalidNumber, i));
            }
        }
        self.pos = i;
        let (s, len) = self.dom.push_bytes(&src[start..i]);
        Ok(self.dom.push_node(Node { kind: KIND_NUMBER, flag, start: s, len }))
    }

    /// Parses a string at `pos` (which holds '"'), stores the decoded bytes
    /// in the DOM and returns (offset, len). Shares its decoder with the
    /// streaming parser so both report identical errors.
    fn string(&mut self) -> PResult<(u32, u32)> {
        let open = self.pos;
        let src = self.src;
        // Find the closing quote, skipping escaped characters.
        let mut i = open + 1;
        loop {
            match src.get(i) {
                None => return Err((ErrorCode::UnexpectedEnd, open)),
                Some(b'"') => break,
                Some(b'\\') => i += 2,
                Some(_) => i += 1,
            }
        }
        let raw = &src[open + 1..i];
        if let Err(e) = std::str::from_utf8(raw) {
            return Err((ErrorCode::InvalidUtf8, open + 1 + e.valid_up_to()));
        }
        let mut scratch = std::mem::take(&mut self.scratch);
        scratch.clear();
        let decoded = crate::stream::decode_string(raw, &mut scratch).map_err(|(code, at)| (code, open + 1 + at));
        let stored = decoded.map(|_| self.dom.push_bytes(&scratch));
        self.scratch = scratch;
        if stored.is_ok() {
            self.pos = i + 1;
        }
        stored
    }

}
