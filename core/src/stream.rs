// SPDX-License-Identifier: MPL-2.0
//! Incremental pull parser.
//!
//! Feed input in chunks of any size, then pull events one at a time. When a
//! token is cut by a chunk boundary, `next()` returns `NeedMore` without
//! consuming anything; feed more (or `finish()`) and call again. Only the
//! unconsumed tail of the input is kept, so memory stays bounded by the
//! largest single token, not by the document size.
//!
//! Grammar rules are the same as the DOM parser (strict RFC 8259 plus the
//! optional trailing-comma and comment relaxations). An extra `MULTI` flag
//! accepts a sequence of top-level values (JSON Lines / concatenated JSON).

use crate::parser::{ErrorCode, DEFAULT_MAX_DEPTH, OPT_COMMENTS, OPT_TRAILING_COMMAS};

pub const OPT_MULTI: u32 = 4;

/// Default cap on the unconsumed bytes the reader will hold while it waits for
/// the rest of one token (a string, number or comment). Without a cap, a
/// peer that never closes a string makes the reader buffer without bound.
pub const DEFAULT_MAX_TOKEN: usize = 256 << 20;

/// Longest number lexeme the streaming reader accepts. Numbers are rescanned
/// from their first byte whenever more input arrives, so the length is capped
/// to keep that cost bounded (the DOM parser has no such limit).
pub const MAX_NUMBER_LEN: usize = 1 << 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum Event {
    Error = -1,
    NeedMore = 0,
    ObjectStart = 1,
    ObjectEnd = 2,
    ArrayStart = 3,
    ArrayEnd = 4,
    Key = 5,
    String = 6,
    Number = 7,
    True = 8,
    False = 9,
    Null = 10,
    End = 11,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    /// A value is required (document start, after ':' or after ',' in an array).
    Value,
    /// After '[' (or after ',' when trailing commas are allowed): value or ']'.
    ValueOrEnd,
    /// After '{' (or after ',' when trailing commas are allowed): key or '}'.
    KeyOrEnd,
    /// After ',' in an object: a key is required.
    Key,
    Colon,
    CommaOrEnd,
    /// A complete top-level value has been read.
    Done,
}

#[derive(Clone, Copy, Debug)]
pub struct StreamError {
    pub code: ErrorCode,
    pub offset: u64,
    pub line: u64,
    pub column: u64,
}

pub struct Reader {
    buf: Vec<u8>,
    pos: usize,
    eof: bool,
    // Position of buf[pos] in the whole input.
    offset: u64,
    line: u64,
    column: u64,
    stack: Vec<u8>,
    state: State,
    max_depth: usize,
    flags: u32,
    text: Vec<u8>,
    error: Option<StreamError>,
    documents: u64,
    started: bool,
    /// Longest incomplete token tolerated (see `DEFAULT_MAX_TOKEN`).
    max_token: usize,
    /// Where the scan of an incomplete string/comment stopped, relative to
    /// `pos`, so the next call resumes there instead of starting over (a
    /// token fed in small chunks would otherwise be rescanned quadratically).
    /// Reset whenever input is consumed.
    resume: usize,
}

/// Lexing outcome for one token.
enum Lex {
    /// Token complete: event and number of bytes consumed.
    Token(Event, usize),
    /// The buffer ends inside the token.
    Incomplete,
    /// Error at a relative offset from `pos`.
    Fail(ErrorCode, usize),
}

impl Reader {
    pub fn new(max_depth: u32, flags: u32) -> Self {
        Reader {
            buf: Vec::new(),
            pos: 0,
            eof: false,
            offset: 0,
            line: 1,
            column: 1,
            stack: Vec::new(),
            state: State::Value,
            max_depth: if max_depth == 0 { DEFAULT_MAX_DEPTH as usize } else { max_depth as usize },
            flags,
            text: Vec::new(),
            error: None,
            documents: 0,
            started: false,
            max_token: DEFAULT_MAX_TOKEN,
            resume: 0,
        }
    }

    /// Limits how much input may be buffered while one token is incomplete;
    /// exceeding it yields `ErrorCode::TooLarge`. 0 restores the default.
    pub fn set_max_token(&mut self, bytes: usize) {
        self.max_token = if bytes == 0 { DEFAULT_MAX_TOKEN } else { bytes };
    }

    pub fn feed(&mut self, data: &[u8]) -> bool {
        if self.eof {
            return false;
        }
        // Reclaim consumed space before growing.
        if self.pos > 0 && (self.pos >= 1 << 16 || self.pos * 2 >= self.buf.len()) {
            self.buf.drain(..self.pos);
            self.pos = 0;
        }
        self.buf.extend_from_slice(data);
        true
    }

    pub fn finish(&mut self) {
        self.eof = true;
    }

    /// Current key/string (decoded) or number lexeme, without the NUL.
    pub fn text(&self) -> &[u8] {
        match self.text.split_last() {
            Some((_, body)) => body,
            None => &[],
        }
    }

    /// Same bytes followed by a NUL terminator.
    pub fn text_cstr(&self) -> *const u8 {
        if self.text.is_empty() {
            c"".as_ptr() as *const u8
        } else {
            self.text.as_ptr()
        }
    }

    pub fn depth(&self) -> usize {
        self.stack.len()
    }

    pub fn documents(&self) -> u64 {
        self.documents
    }

    pub fn buffered(&self) -> usize {
        self.buf.len() - self.pos
    }

    pub fn error(&self) -> Option<StreamError> {
        self.error
    }

    fn rest(&self) -> &[u8] {
        &self.buf[self.pos..]
    }

    /// Consumes `n` bytes, keeping offset/line/column in step.
    fn advance(&mut self, n: usize) {
        if n > 0 {
            self.resume = 0;
        }
        for &b in &self.buf[self.pos..self.pos + n] {
            if b == b'\n' {
                self.line += 1;
                self.column = 1;
            } else if (b & 0xC0) != 0x80 {
                self.column += 1;
            }
        }
        self.offset += n as u64;
        self.pos += n;
    }

    fn fail(&mut self, code: ErrorCode, rel: usize) -> Event {
        let rel = rel.min(self.buf.len() - self.pos);
        let (mut line, mut column) = (self.line, self.column);
        for &b in &self.buf[self.pos..self.pos + rel] {
            if b == b'\n' {
                line += 1;
                column = 1;
            } else if (b & 0xC0) != 0x80 {
                column += 1;
            }
        }
        self.error = Some(StreamError { code, offset: self.offset + rel as u64, line, column });
        Event::Error
    }

    /// Skips whitespace and (optionally) comments. `Err(())` means the buffer
    /// ends inside a comment and more input is needed.
    fn skip_ws(&mut self) -> Result<(), Event> {
        loop {
            let rest = self.rest();
            let mut i = 0;
            while i < rest.len() && matches!(rest[i], b' ' | b'\t' | b'\n' | b'\r') {
                i += 1;
            }
            self.advance(i);
            let rest = self.rest();
            if rest.first() != Some(&b'/') || self.flags & OPT_COMMENTS == 0 {
                return Ok(());
            }
            match rest.get(1) {
                None if !self.eof => return Err(Event::NeedMore),
                Some(b'/') => {
                    let from = self.resume.max(2);
                    match rest.get(from..).and_then(|t| t.iter().position(|&b| b == b'\n')) {
                        Some(p) => self.advance(from + p + 1),
                        None if self.eof => {
                            let n = rest.len();
                            self.advance(n);
                        }
                        None => {
                            self.resume = rest.len();
                            return Err(Event::NeedMore);
                        }
                    }
                }
                Some(b'*') => {
                    let from = self.resume.max(2);
                    match rest.get(from..).and_then(|t| t.windows(2).position(|w| w == b"*/")) {
                        Some(p) => self.advance(from + p + 2),
                        None if self.eof => return Err(self.fail(ErrorCode::UnexpectedEnd, 0)),
                        None => {
                            // Keep the last byte: it may be the '*' of "*/".
                            self.resume = rest.len().saturating_sub(1).max(2);
                            return Err(Event::NeedMore);
                        }
                    }
                }
                _ => return Err(self.fail(ErrorCode::UnexpectedChar, 0)),
            }
        }
    }

    // Not an Iterator: `NeedMore` is a normal result, not the end.
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Event {
        let event = self.next_event();
        if event == Event::NeedMore && self.buffered() > self.max_token {
            return self.fail(ErrorCode::TooLarge, 0);
        }
        event
    }

    fn next_event(&mut self) -> Event {
        if self.error.is_some() {
            return Event::Error;
        }
        if !self.started {
            // Skip a UTF-8 BOM at the very start (wait until we can tell).
            const BOM: [u8; 3] = [0xEF, 0xBB, 0xBF];
            let rest = self.rest();
            if rest.len() < 3 && !self.eof && BOM.starts_with(rest) {
                return Event::NeedMore;
            }
            if rest.starts_with(&BOM) {
                self.pos += 3;
                self.offset += 3;
            }
            self.started = true;
        }
        loop {
            if let Err(e) = self.skip_ws() {
                return e;
            }
            if self.pos == self.buf.len() {
                if !self.eof {
                    return Event::NeedMore;
                }
                return match self.state {
                    State::Done => Event::End,
                    State::Value if self.stack.is_empty() && self.documents > 0 => Event::End,
                    State::Value if self.stack.is_empty() => self.fail(ErrorCode::Empty, 0),
                    _ => self.fail(ErrorCode::UnexpectedEnd, 0),
                };
            }
            let c = self.buf[self.pos];
            match self.state {
                State::Done => {
                    if self.flags & OPT_MULTI != 0 {
                        self.state = State::Value;
                        continue;
                    }
                    return self.fail(ErrorCode::TrailingData, 0);
                }
                State::Colon => {
                    if c != b':' {
                        return self.fail(ErrorCode::UnexpectedChar, 0);
                    }
                    self.advance(1);
                    self.state = State::Value;
                }
                State::CommaOrEnd => {
                    let top = *self.stack.last().unwrap_or(&b'[');
                    match c {
                        b',' => {
                            self.advance(1);
                            let trailing = self.flags & OPT_TRAILING_COMMAS != 0;
                            self.state = match (top, trailing) {
                                (b'{', true) => State::KeyOrEnd,
                                (b'{', false) => State::Key,
                                (_, true) => State::ValueOrEnd,
                                (_, false) => State::Value,
                            };
                        }
                        b'}' if top == b'{' => return self.close(Event::ObjectEnd),
                        b']' if top == b'[' => return self.close(Event::ArrayEnd),
                        _ => return self.fail(ErrorCode::UnexpectedChar, 0),
                    }
                }
                State::KeyOrEnd | State::Key => {
                    if c == b'}' && self.state == State::KeyOrEnd {
                        return self.close(Event::ObjectEnd);
                    }
                    if c != b'"' {
                        return self.fail(ErrorCode::UnexpectedChar, 0);
                    }
                    return match self.lex_string() {
                        Lex::Token(_, n) => {
                            self.advance(n);
                            self.state = State::Colon;
                            Event::Key
                        }
                        Lex::Incomplete => Event::NeedMore,
                        Lex::Fail(code, rel) => self.fail(code, rel),
                    };
                }
                State::ValueOrEnd if c == b']' => return self.close(Event::ArrayEnd),
                State::ValueOrEnd | State::Value => return self.value(c),
            }
        }
    }

    fn close(&mut self, event: Event) -> Event {
        self.advance(1);
        self.stack.pop();
        self.after_value();
        event
    }

    fn after_value(&mut self) {
        if self.stack.is_empty() {
            self.state = State::Done;
            self.documents += 1;
        } else {
            self.state = State::CommaOrEnd;
        }
    }

    fn value(&mut self, c: u8) -> Event {
        let lex = match c {
            b'{' | b'[' => {
                if self.stack.len() >= self.max_depth {
                    return self.fail(ErrorCode::TooDeep, 0);
                }
                self.advance(1);
                self.stack.push(c);
                if c == b'{' {
                    self.state = State::KeyOrEnd;
                    return Event::ObjectStart;
                }
                self.state = State::ValueOrEnd;
                return Event::ArrayStart;
            }
            b'"' => self.lex_string(),
            b't' => self.lex_literal(b"true", Event::True),
            b'f' => self.lex_literal(b"false", Event::False),
            b'n' => self.lex_literal(b"null", Event::Null),
            b'-' | b'0'..=b'9' => self.lex_number(),
            _ => Lex::Fail(ErrorCode::UnexpectedChar, 0),
        };
        match lex {
            Lex::Token(event, n) => {
                self.advance(n);
                self.after_value();
                event
            }
            Lex::Incomplete => Event::NeedMore,
            Lex::Fail(code, rel) => self.fail(code, rel),
        }
    }

    fn lex_literal(&self, word: &[u8], event: Event) -> Lex {
        let rest = self.rest();
        if rest.len() < word.len() {
            if word.starts_with(rest) {
                return if self.eof { Lex::Fail(ErrorCode::UnexpectedEnd, rest.len()) } else { Lex::Incomplete };
            }
            return Lex::Fail(ErrorCode::UnexpectedChar, 0);
        }
        if &rest[..word.len()] == word {
            Lex::Token(event, word.len())
        } else {
            Lex::Fail(ErrorCode::UnexpectedChar, 0)
        }
    }

    /// Scans a number exactly like the DOM parser (longest grammatical
    /// prefix, same error codes and offsets). Whenever the decision depends
    /// on bytes not received yet, reports Incomplete.
    fn lex_number(&mut self) -> Lex {
        match self.lex_number_prefix() {
            Lex::Incomplete if self.buffered() > MAX_NUMBER_LEN => Lex::Fail(ErrorCode::TooLarge, 0),
            Lex::Token(_, n) if n > MAX_NUMBER_LEN => Lex::Fail(ErrorCode::TooLarge, 0),
            other => other,
        }
    }

    fn lex_number_prefix(&mut self) -> Lex {
        let rest = &self.buf[self.pos..];
        let eof = self.eof;
        let get = |i: usize| rest.get(i).copied();
        let more = |i: usize| i >= rest.len() && !eof;
        let digit = |i: usize| matches!(rest.get(i), Some(b'0'..=b'9'));
        let mut i = 0;
        if get(i) == Some(b'-') {
            i += 1;
        }
        if more(i) {
            return Lex::Incomplete;
        }
        match get(i) {
            Some(b'0') => {
                i += 1;
                if more(i) {
                    return Lex::Incomplete;
                }
                if digit(i) {
                    return Lex::Fail(ErrorCode::InvalidNumber, i);
                }
            }
            Some(b'1'..=b'9') => {
                while digit(i) {
                    i += 1;
                }
            }
            None => return Lex::Fail(ErrorCode::UnexpectedEnd, i),
            Some(_) => return Lex::Fail(ErrorCode::InvalidNumber, i),
        }
        if more(i) {
            return Lex::Incomplete;
        }
        if get(i) == Some(b'.') {
            i += 1;
            let st = i;
            while digit(i) {
                i += 1;
            }
            if more(i) {
                return Lex::Incomplete;
            }
            if i == st {
                return Lex::Fail(ErrorCode::InvalidNumber, i);
            }
        }
        if matches!(get(i), Some(b'e' | b'E')) {
            i += 1;
            if more(i) {
                return Lex::Incomplete;
            }
            if matches!(get(i), Some(b'+' | b'-')) {
                i += 1;
            }
            let st = i;
            while digit(i) {
                i += 1;
            }
            if more(i) {
                return Lex::Incomplete;
            }
            if i == st {
                return Lex::Fail(ErrorCode::InvalidNumber, i);
            }
        }
        self.text.clear();
        self.text.extend_from_slice(&rest[..i]);
        self.text.push(0); // NUL-terminated for C/Wave callers
        Lex::Token(Event::Number, i)
    }

    /// Lexes a string at pos (which holds '"') into `text`, decoded.
    fn lex_string(&mut self) -> Lex {
        let rest = &self.buf[self.pos..];
        // Find the closing quote, skipping escaped characters; continue where
        // an earlier, incomplete attempt stopped.
        let mut i = self.resume.max(1);
        loop {
            match rest.get(i) {
                None => {
                    if self.eof {
                        return Lex::Fail(ErrorCode::UnexpectedEnd, 0);
                    }
                    self.resume = i;
                    return Lex::Incomplete;
                }
                Some(b'"') => break,
                Some(b'\\') => i += 2,
                Some(_) => i += 1,
            }
        }
        let raw = &rest[1..i];
        if let Err(e) = std::str::from_utf8(raw) {
            return Lex::Fail(ErrorCode::InvalidUtf8, 1 + e.valid_up_to());
        }
        let mut out = std::mem::take(&mut self.text);
        out.clear();
        let result = decode_string(raw, &mut out);
        out.push(0); // NUL-terminated for C/Wave callers
        self.text = out;
        match result {
            Ok(()) => Lex::Token(Event::String, i + 1),
            Err((code, at)) => Lex::Fail(code, 1 + at),
        }
    }
}

fn hex4(s: &[u8], at: usize) -> Option<u32> {
    let digits = s.get(at..at + 4)?;
    let mut v = 0u32;
    for &b in digits {
        let d = (b as char).to_digit(16)?;
        v = (v << 4) | d;
    }
    Some(v)
}

/// Decodes the content between the quotes (already valid UTF-8).
/// Errors carry an offset relative to `raw`.
pub fn decode_string(raw: &[u8], out: &mut Vec<u8>) -> Result<(), (ErrorCode, usize)> {
    let mut i = 0;
    while i < raw.len() {
        // Copy the run up to the next backslash or control character at once.
        let run = raw[i..].iter().position(|&b| b == b'\\' || b < 0x20).unwrap_or(raw.len() - i);
        out.extend_from_slice(&raw[i..i + run]);
        i += run;
        if i == raw.len() {
            break;
        }
        if raw[i] < 0x20 {
            return Err((ErrorCode::ControlChar, i));
        }
        let esc = *raw.get(i + 1).ok_or((ErrorCode::InvalidEscape, i))?;
        let simple = match esc {
            b'"' => Some(b'"'),
            b'\\' => Some(b'\\'),
            b'/' => Some(b'/'),
            b'b' => Some(0x08),
            b'f' => Some(0x0C),
            b'n' => Some(b'\n'),
            b'r' => Some(b'\r'),
            b't' => Some(b'\t'),
            b'u' => None,
            _ => return Err((ErrorCode::InvalidEscape, i)),
        };
        if let Some(c) = simple {
            out.push(c);
            i += 2;
            continue;
        }
        let hi = hex4(raw, i + 2).ok_or((ErrorCode::InvalidUnicode, i))?;
        let mut used = 6;
        let code = if (0xD800..0xDC00).contains(&hi) {
            if raw.get(i + 6) != Some(&b'\\') || raw.get(i + 7) != Some(&b'u') {
                return Err((ErrorCode::InvalidUnicode, i));
            }
            let lo = hex4(raw, i + 8).ok_or((ErrorCode::InvalidUnicode, i))?;
            if !(0xDC00..0xE000).contains(&lo) {
                return Err((ErrorCode::InvalidUnicode, i));
            }
            used = 12;
            0x10000 + ((hi - 0xD800) << 10) + (lo - 0xDC00)
        } else if (0xDC00..0xE000).contains(&hi) {
            return Err((ErrorCode::InvalidUnicode, i));
        } else {
            hi
        };
        let ch = char::from_u32(code).ok_or((ErrorCode::InvalidUnicode, i))?;
        let mut tmp = [0u8; 4];
        out.extend_from_slice(ch.encode_utf8(&mut tmp).as_bytes());
        i += used;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Runs the reader over `input` split into `chunk`-byte pieces and
    /// returns a compact event log ("{ k:a 1 }").
    fn events(input: &[u8], chunk: usize, flags: u32) -> Result<String, (ErrorCode, u64, u64, u64)> {
        let mut r = Reader::new(0, flags);
        let mut log = Vec::new();
        let mut fed = 0;
        loop {
            match r.next() {
                Event::NeedMore => {
                    if fed >= input.len() {
                        r.finish();
                    } else {
                        let end = (fed + chunk).min(input.len());
                        r.feed(&input[fed..end]);
                        fed = end;
                    }
                }
                Event::Error => {
                    let e = r.error().unwrap();
                    return Err((e.code, e.offset, e.line, e.column));
                }
                Event::End => return Ok(log.join(" ")),
                Event::ObjectStart => log.push("{".to_string()),
                Event::ObjectEnd => log.push("}".to_string()),
                Event::ArrayStart => log.push("[".to_string()),
                Event::ArrayEnd => log.push("]".to_string()),
                Event::Key => log.push(format!("k:{}", String::from_utf8_lossy(r.text()))),
                Event::String => log.push(format!("s:{}", String::from_utf8_lossy(r.text()))),
                Event::Number => log.push(format!("n:{}", String::from_utf8_lossy(r.text()))),
                Event::True => log.push("t".into()),
                Event::False => log.push("f".into()),
                Event::Null => log.push("null".into()),
            }
        }
    }

    #[test]
    fn same_events_for_every_chunk_size() {
        let input = br#"{"name":"w\u00e9ve","list":[1,-2.5e3,true,false,null,{"x":[]}],"t":"a\"b"}"#;
        let expect = "{ k:name s:wéve k:list [ n:1 n:-2.5e3 t f null { k:x [ ] } ] k:t s:a\"b }";
        for chunk in 1..=input.len() {
            assert_eq!(events(input, chunk, 0).unwrap(), expect, "chunk {chunk}");
        }
    }

    #[test]
    fn errors_have_positions_independent_of_chunking() {
        let input = b"{\n  \"a\": [1, 2,]\n}";
        let whole = events(input, input.len(), 0).unwrap_err();
        assert_eq!((whole.0, whole.2, whole.3), (ErrorCode::UnexpectedChar, 2, 14));
        for chunk in 1..8 {
            assert_eq!(events(input, chunk, 0).unwrap_err(), whole);
        }
        assert_eq!(events(b"", 1, 0).unwrap_err().0, ErrorCode::Empty);
        assert_eq!(events(b"[1", 1, 0).unwrap_err().0, ErrorCode::UnexpectedEnd);
        assert_eq!(events(b"\"abc", 1, 0).unwrap_err().0, ErrorCode::UnexpectedEnd);
        assert_eq!(events(b"[1] 2", 1, 0).unwrap_err().0, ErrorCode::TrailingData);
        assert_eq!(events(b"[\"\\x\"]", 1, 0).unwrap_err().0, ErrorCode::InvalidEscape);
        assert_eq!(events(b"[01]", 1, 0).unwrap_err().0, ErrorCode::InvalidNumber);
        assert_eq!(events(b"[\"\xff\"]", 1, 0).unwrap_err().0, ErrorCode::InvalidUtf8);
    }

    #[test]
    fn relaxed_flags_across_chunks() {
        let input = b"// head\n{ /* c */ \"a\": [1, 2,], }";
        for chunk in 1..6 {
            assert_eq!(events(input, chunk, 3).unwrap(), "{ k:a [ n:1 n:2 ] }");
        }
        assert!(events(input, 4, 0).is_err());
    }

    #[test]
    fn oversized_tokens_are_refused() {
        // An unterminated string may not grow the buffer past max_token.
        let mut r = Reader::new(0, 0);
        r.set_max_token(1000);
        r.feed(b"\"");
        let mut result = Event::NeedMore;
        for _ in 0..100 {
            r.feed(&[b'a'; 64]);
            result = r.next();
            if result != Event::NeedMore {
                break;
            }
        }
        assert_eq!(result, Event::Error);
        assert_eq!(r.error().unwrap().code, ErrorCode::TooLarge);
        assert!(r.buffered() < 1200);

        // Same for a never-ending comment and a never-ending number.
        for (prefix, flags) in [(&b"/*"[..], OPT_COMMENTS), (&b"//"[..], OPT_COMMENTS)] {
            let mut r = Reader::new(0, flags);
            r.set_max_token(1000);
            r.feed(prefix);
            let mut result = Event::NeedMore;
            for _ in 0..100 {
                r.feed(&[b'x'; 64]);
                result = r.next();
                if result != Event::NeedMore {
                    break;
                }
            }
            assert_eq!(r.error().map(|e| e.code), Some(ErrorCode::TooLarge), "{prefix:?}");
            assert_eq!(result, Event::Error);
        }
        let mut r = Reader::new(0, 0);
        r.feed(b"1");
        let mut result = Event::NeedMore;
        for _ in 0..(MAX_NUMBER_LEN / 1024 + 4) {
            r.feed(&[b'7'; 1024]);
            result = r.next();
            if result != Event::NeedMore {
                break;
            }
        }
        assert_eq!(result, Event::Error);
        assert_eq!(r.error().unwrap().code, ErrorCode::TooLarge);

        // A token that fits is unaffected, however it is chunked.
        let mut r = Reader::new(0, 0);
        r.set_max_token(1000);
        let long = format!("\"{}\"", "b".repeat(900));
        for piece in long.as_bytes().chunks(7) {
            assert_eq!(r.next(), Event::NeedMore);
            r.feed(piece);
        }
        r.finish();
        assert_eq!(r.next(), Event::String);
        assert_eq!(r.text().len(), 900);
    }

    #[test]
    fn long_tokens_in_tiny_chunks_are_not_rescanned() {
        // 4 MB of string and block comment fed 64 bytes at a time. Restarting
        // the scan on every chunk would take minutes; resuming takes a blink.
        let started = std::time::Instant::now();
        let body = "x\\\"y".repeat(1 << 20);
        let doc = format!("/* {body} */ [\"{body}\"]");
        let mut r = Reader::new(0, OPT_COMMENTS);
        r.set_max_token(usize::MAX);
        let mut fed = 0;
        let bytes = doc.as_bytes();
        let mut strings = 0;
        loop {
            match r.next() {
                Event::NeedMore => {
                    if fed >= bytes.len() {
                        r.finish();
                    } else {
                        let end = (fed + 64).min(bytes.len());
                        r.feed(&bytes[fed..end]);
                        fed = end;
                    }
                }
                Event::String => {
                    strings += 1;
                    assert_eq!(r.text().len(), 3 << 20);
                }
                Event::End => break,
                Event::Error => panic!("{:?}", r.error()),
                _ => {}
            }
        }
        assert_eq!(strings, 1);
        assert!(started.elapsed().as_secs() < 20, "took {:?}", started.elapsed());
    }

    #[test]
    fn json_lines_and_bom() {
        let input = b"\xEF\xBB\xBF{\"a\":1}\n{\"a\":2}\n\n[3]\n";
        for chunk in 1..5 {
            assert_eq!(events(input, chunk, OPT_MULTI).unwrap(), "{ k:a n:1 } { k:a n:2 } [ n:3 ]");
        }
        assert_eq!(events(input, 3, 0).unwrap_err().0, ErrorCode::TrailingData);
        assert_eq!(events(b"12 13", 1, OPT_MULTI).unwrap(), "n:12 n:13");
    }

    #[test]
    fn depth_limit_and_bounded_memory() {
        let deep = "[".repeat(10) + &"]".repeat(10);
        let mut r = Reader::new(5, 0);
        r.feed(deep.as_bytes());
        r.finish();
        let mut last = Event::NeedMore;
        for _ in 0..20 {
            last = r.next();
            if last == Event::Error {
                break;
            }
        }
        assert_eq!(last, Event::Error);
        assert_eq!(r.error().unwrap().code, ErrorCode::TooDeep);

        // 1 MB array fed in 4 KB chunks: the reader never holds much of it.
        let mut r = Reader::new(0, 0);
        let item = b"12345678,";
        let mut max_buffered = 0;
        let mut numbers = 0;
        r.feed(b"[");
        let mut sent = 0;
        let total = 1 << 20;
        loop {
            match r.next() {
                Event::NeedMore => {
                    if sent < total {
                        let mut chunk = Vec::new();
                        while chunk.len() < 4096 {
                            chunk.extend_from_slice(item);
                        }
                        sent += chunk.len();
                        r.feed(&chunk);
                    } else if sent >= total {
                        r.feed(b"0]");
                        r.finish();
                        sent = usize::MAX;
                    }
                    max_buffered = max_buffered.max(r.buffered());
                }
                Event::Number => numbers += 1,
                Event::End => break,
                Event::Error => panic!("{:?}", r.error()),
                _ => {}
            }
        }
        assert!(numbers > 100_000);
        assert!(max_buffered < 16 * 1024, "held {max_buffered} bytes");
    }
}
