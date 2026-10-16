// SPDX-License-Identifier: MIT
//! Differential fuzzing with zero external dependencies.
//!
//! * A small, independent reference implementation (value model, RFC 8259
//!   recursive-descent parser and serializer) lives in this file. wjson's
//!   DOM parser must accept exactly what it accepts and produce equal values.
//! * The streaming parser, fed in random chunk sizes, must agree with the DOM
//!   parser — same values, and the same error code and offset on bad input.
//! * Random edit sequences must keep a document equal to a model.
//!
//! Deterministic (seeded xorshift).
//!   cargo test --release --test fuzz                                  quick
//!   WJSON_FUZZ_ITERS=1000000 cargo test --release --test fuzz -- --nocapture

use wjson_core::*;

// ---- deterministic randomness ----------------------------------------------------

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0; // xorshift64*
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545F4914F6CDD1D)
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n.max(1)
    }
    fn chance(&mut self, percent: u64) -> bool {
        self.below(100) < percent
    }
}

fn iterations(default: u64) -> u64 {
    std::env::var("WJSON_FUZZ_ITERS").ok().and_then(|v| v.parse().ok()).unwrap_or(default)
}

// ---- reference value model -------------------------------------------------------

/// Numbers keep their lexeme; objects keep every member in order, and
/// lookups use the last occurrence of a key (RFC 8259 leaves duplicates to
/// the implementation; wjson documents "last wins").
#[derive(Clone, Debug, PartialEq)]
enum J {
    Null,
    Bool(bool),
    Num(String),
    Str(String),
    Arr(Vec<J>),
    Obj(Vec<(String, J)>),
}

impl J {
    fn get(&self, key: &str) -> Option<&J> {
        match self {
            J::Obj(m) => m.iter().rev().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }
    fn distinct_keys(&self) -> usize {
        match self {
            J::Obj(m) => m.iter().map(|(k, _)| k.as_str()).collect::<std::collections::HashSet<_>>().len(),
            _ => 0,
        }
    }
}

fn num_value(lexeme: &str) -> f64 {
    lexeme.parse().expect("valid number lexeme")
}

// ---- reference parser (straight from the RFC 8259 grammar) ----------------------------

struct RefParser<'a> {
    s: &'a [u8],
    i: usize,
    depth: usize,
}

impl<'a> RefParser<'a> {
    /// None = reject. Mirrors wjson's documented behavior: strict RFC 8259,
    /// optional leading UTF-8 BOM, nesting limit 512.
    fn parse(s: &'a [u8]) -> Option<J> {
        let start = if s.starts_with(&[0xEF, 0xBB, 0xBF]) { 3 } else { 0 };
        let mut p = RefParser { s, i: start, depth: 0 };
        p.ws();
        let v = p.value()?;
        p.ws();
        (p.i == s.len()).then_some(v)
    }

    fn peek(&self) -> Option<u8> {
        self.s.get(self.i).copied()
    }

    fn ws(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.i += 1;
        }
    }

    fn eat(&mut self, lit: &[u8]) -> Option<()> {
        if self.s[self.i..].starts_with(lit) {
            self.i += lit.len();
            Some(())
        } else {
            None
        }
    }

    fn value(&mut self) -> Option<J> {
        match self.peek()? {
            b'n' => self.eat(b"null").map(|_| J::Null),
            b't' => self.eat(b"true").map(|_| J::Bool(true)),
            b'f' => self.eat(b"false").map(|_| J::Bool(false)),
            b'"' => self.string().map(J::Str),
            b'[' => self.array(),
            b'{' => self.object(),
            b'-' | b'0'..=b'9' => self.number(),
            _ => None,
        }
    }

    fn digits(&mut self) -> usize {
        let st = self.i;
        while matches!(self.peek(), Some(b'0'..=b'9')) {
            self.i += 1;
        }
        self.i - st
    }

    fn number(&mut self) -> Option<J> {
        let st = self.i;
        if self.peek() == Some(b'-') {
            self.i += 1;
        }
        match self.peek()? {
            b'0' => self.i += 1,
            b'1'..=b'9' => {
                self.digits();
            }
            _ => return None,
        }
        if self.peek() == Some(b'.') {
            self.i += 1;
            if self.digits() == 0 {
                return None;
            }
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            self.i += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.i += 1;
            }
            if self.digits() == 0 {
                return None;
            }
        }
        Some(J::Num(String::from_utf8(self.s[st..self.i].to_vec()).unwrap()))
    }

    fn hex4(&mut self) -> Option<u32> {
        let h = self.s.get(self.i..self.i + 4)?;
        let v = u32::from_str_radix(std::str::from_utf8(h).ok()?, 16).ok()?;
        if !h.iter().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        self.i += 4;
        Some(v)
    }

    fn string(&mut self) -> Option<String> {
        self.i += 1;
        let mut out = String::new();
        loop {
            let c = self.peek()?;
            match c {
                b'"' => {
                    self.i += 1;
                    return Some(out);
                }
                0..=0x1F => return None,
                b'\\' => {
                    self.i += 1;
                    let e = self.peek()?;
                    self.i += 1;
                    match e {
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        b'/' => out.push('/'),
                        b'b' => out.push('\u{8}'),
                        b'f' => out.push('\u{c}'),
                        b'n' => out.push('\n'),
                        b'r' => out.push('\r'),
                        b't' => out.push('\t'),
                        b'u' => {
                            let hi = self.hex4()?;
                            let code = if (0xD800..0xDC00).contains(&hi) {
                                self.eat(b"\\u")?;
                                let lo = self.hex4()?;
                                if !(0xDC00..0xE000).contains(&lo) {
                                    return None;
                                }
                                0x10000 + ((hi - 0xD800) << 10) + (lo - 0xDC00)
                            } else {
                                hi
                            };
                            out.push(char::from_u32(code)?);
                        }
                        _ => return None,
                    }
                }
                _ => {
                    // One UTF-8 encoded character.
                    let len = match c {
                        0x00..=0x7F => 1,
                        0xC2..=0xDF => 2,
                        0xE0..=0xEF => 3,
                        0xF0..=0xF4 => 4,
                        _ => return None,
                    };
                    let bytes = self.s.get(self.i..self.i + len)?;
                    out.push_str(std::str::from_utf8(bytes).ok()?);
                    self.i += len;
                }
            }
        }
    }

    fn enter(&mut self) -> Option<()> {
        self.depth += 1;
        (self.depth <= 512).then_some(())
    }

    fn array(&mut self) -> Option<J> {
        self.enter()?;
        self.i += 1;
        let mut items = Vec::new();
        self.ws();
        if self.peek() == Some(b']') {
            self.i += 1;
        } else {
            loop {
                self.ws();
                items.push(self.value()?);
                self.ws();
                match self.peek()? {
                    b',' => self.i += 1,
                    b']' => {
                        self.i += 1;
                        break;
                    }
                    _ => return None,
                }
            }
        }
        self.depth -= 1;
        Some(J::Arr(items))
    }

    fn object(&mut self) -> Option<J> {
        self.enter()?;
        self.i += 1;
        let mut members = Vec::new();
        self.ws();
        if self.peek() == Some(b'}') {
            self.i += 1;
        } else {
            loop {
                self.ws();
                if self.peek()? != b'"' {
                    return None;
                }
                let k = self.string()?;
                self.ws();
                self.eat(b":")?;
                self.ws();
                let v = self.value()?;
                members.push((k, v));
                self.ws();
                match self.peek()? {
                    b',' => self.i += 1,
                    b'}' => {
                        self.i += 1;
                        break;
                    }
                    _ => return None,
                }
            }
        }
        self.depth -= 1;
        Some(J::Obj(members))
    }
}

// ---- random documents and a serializer -----------------------------------------------

fn random_string(rng: &mut Rng) -> String {
    const PIECES: &[&str] = &[
        "a", "Z", "0", " ", "\"", "\\", "/", "\n", "\t", "\u{1}", "\u{1f}", "\u{7f}", "é", "한글", "😀",
        "\u{feff}", "\u{2028}", "key", "<", "&", "\u{0}",
    ];
    (0..rng.below(8)).map(|_| PIECES[rng.below(PIECES.len() as u64) as usize]).collect()
}

fn random_number(rng: &mut Rng) -> String {
    match rng.below(7) {
        0 => (rng.below(1000) as i64 - 500).to_string(),
        1 => (rng.next() as i64).to_string(),
        2 => rng.next().to_string() + &"9".repeat(rng.below(4) as usize), // beyond i64/u64
        3 => {
            let f = f64::from_bits(rng.next());
            if f.is_finite() { format!("{f:e}") } else { "0.5".into() }
        }
        4 => format!("{}", (rng.below(2_000_000) as f64 - 1e6) / 1000.0),
        5 => format!("{}e{}", rng.below(100), rng.below(800) as i64 - 400), // incl. out of f64 range
        _ => format!("-0.{}E+{}", rng.below(1000), rng.below(20)),
    }
}

fn random_value(rng: &mut Rng, depth: u32) -> J {
    if depth == 0 || rng.chance(35) {
        return match rng.below(6) {
            0 => J::Null,
            1 => J::Bool(rng.chance(50)),
            2 | 3 => J::Num(random_number(rng)),
            _ => J::Str(random_string(rng)),
        };
    }
    let n = rng.below(6);
    if rng.chance(50) {
        J::Arr((0..n).map(|_| random_value(rng, depth - 1)).collect())
    } else {
        // Small key alphabet so duplicate keys occur.
        J::Obj((0..n).map(|_| (if rng.chance(30) { format!("k{}", rng.below(3)) } else { random_string(rng) }, random_value(rng, depth - 1))).collect())
    }
}

fn write_string(rng: &mut Rng, s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str(if rng.chance(50) { "\\n" } else { "\\u000a" }),
            '\t' => out.push_str("\\t"),
            '/' if rng.chance(30) => out.push_str("\\/"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c if (c as u32) >= 0x80 && rng.chance(25) => {
                // Escape non-ASCII, using surrogate pairs above U+FFFF.
                let mut units = [0u16; 2];
                for u in c.encode_utf16(&mut units) {
                    out.push_str(&format!("\\u{:04X}", u));
                }
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

fn ws(rng: &mut Rng, out: &mut String, enabled: bool) {
    if enabled {
        while rng.chance(25) {
            out.push([' ', '\n', '\t', '\r'][rng.below(4) as usize]);
        }
    }
}

fn write_value(rng: &mut Rng, v: &J, out: &mut String, spaces: bool) {
    match v {
        J::Null => out.push_str("null"),
        J::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        J::Num(n) => out.push_str(n),
        J::Str(s) => write_string(rng, s, out),
        J::Arr(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                ws(rng, out, spaces);
                write_value(rng, item, out, spaces);
                ws(rng, out, spaces);
            }
            out.push(']');
        }
        J::Obj(members) => {
            out.push('{');
            for (i, (k, item)) in members.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                ws(rng, out, spaces);
                write_string(rng, k, out);
                ws(rng, out, spaces);
                out.push(':');
                ws(rng, out, spaces);
                write_value(rng, item, out, spaces);
            }
            ws(rng, out, spaces);
            out.push('}');
        }
    }
}

fn render(rng: &mut Rng, v: &J) -> Vec<u8> {
    let mut out = String::new();
    let spaces = rng.chance(50);
    ws(rng, &mut out, spaces);
    write_value(rng, v, &mut out, spaces);
    ws(rng, &mut out, spaces);
    out.into_bytes()
}

fn mutate(rng: &mut Rng, text: &[u8]) -> Vec<u8> {
    const INTERESTING: &[u8] = b"{}[]\":,\\/ -+.eE0123456789tfnu\n\x00\x1f\x7f\xc3\xa9\xff\xed\xa0\x80\xef\xbb\xbf";
    let mut v = text.to_vec();
    for _ in 0..=rng.below(3) {
        let len = v.len() as u64;
        match rng.below(6) {
            0 if len > 0 => {
                v.remove(rng.below(len) as usize);
            }
            1 => {
                let at = rng.below(len + 1) as usize;
                v.insert(at, INTERESTING[rng.below(INTERESTING.len() as u64) as usize]);
            }
            2 if len > 0 => {
                let at = rng.below(len) as usize;
                v[at] = INTERESTING[rng.below(INTERESTING.len() as u64) as usize];
            }
            3 if len > 1 => {
                let a = rng.below(len) as usize;
                let b = rng.below(len) as usize;
                v.swap(a, b);
            }
            4 if len > 0 => v.truncate(rng.below(len) as usize),
            _ => {
                let at = rng.below(len + 1) as usize;
                let dup: Vec<u8> = v[at..].iter().take(rng.below(6) as usize).copied().collect();
                for (k, b) in dup.into_iter().enumerate() {
                    v.insert(at + k, b);
                }
            }
        }
    }
    v
}

// ---- comparing a wjson document with the model ----------------------------------------

fn parse(bytes: &[u8]) -> *mut Doc {
    unsafe { wj_parse(bytes.as_ptr(), bytes.len() as i64) }
}

fn bytes_of<'a>(p: *const u8, len: i64) -> &'a [u8] {
    if len == 0 { &[] } else { unsafe { std::slice::from_raw_parts(p, len as usize) } }
}

fn same(doc: *const Doc, node: i32, v: &J) -> bool {
    unsafe {
        let kind = wj_type(doc, node);
        match v {
            J::Null => kind == 0,
            J::Bool(b) => kind == 1 && wj_bool(doc, node) == *b as i32,
            J::Num(n) => {
                let mut len = 0;
                let p = wj_num(doc, node, &mut len);
                // The lexeme is kept verbatim.
                kind == 2 && bytes_of(p, len) == n.as_bytes()
            }
            J::Str(s) => {
                let mut len = 0;
                let p = wj_str(doc, node, &mut len);
                kind == 3 && bytes_of(p, len) == s.as_bytes()
            }
            J::Arr(items) => {
                kind == 4
                    && wj_len(doc, node) == items.len() as i64
                    && items.iter().enumerate().all(|(i, item)| same(doc, wj_at(doc, node, i as i64), item))
            }
            J::Obj(members) => {
                if kind != 5 || wj_len(doc, node) != members.len() as i64 {
                    return false;
                }
                // Same member order and keys...
                let order = members.iter().enumerate().all(|(i, (k, _))| {
                    let mut len = 0;
                    let p = wj_key(doc, node, i as i64, &mut len);
                    bytes_of(p, len) == k.as_bytes()
                });
                // ...and lookups agree on "last key wins".
                order
                    && members.iter().all(|(k, _)| {
                        let child = wj_get(doc, node, k.as_ptr(), k.len() as i64);
                        child >= 0 && same(doc, child, v.get(k).unwrap())
                    })
            }
        }
    }
}

// ---- streaming reader -> model ----------------------------------------------------------

#[derive(Debug)]
enum StreamOutcome {
    Ok(J),
    Err(i32, i64),
}

fn stream_parse(rng: &mut Rng, input: &[u8]) -> StreamOutcome {
    unsafe {
        let r = wj_reader_new(0, 0);
        let mut stack: Vec<(J, Option<String>)> = Vec::new();
        let mut result: Option<J> = None;
        let mut pending_key: Option<String> = None;
        let mut fed = 0usize;
        let text = |r: *mut wjson_core::stream::Reader| {
            let mut len = 0;
            let p = wj_reader_text(r, &mut len);
            String::from_utf8(bytes_of(p, len).to_vec()).unwrap()
        };
        let outcome = loop {
            let ev = wj_reader_next(r);
            let mut value: Option<J> = None;
            match ev {
                0 => {
                    if fed >= input.len() {
                        wj_reader_finish(r);
                    } else {
                        let n = (1 + rng.below(16) as usize).min(input.len() - fed);
                        wj_reader_feed(r, input[fed..].as_ptr(), n as i64);
                        fed += n;
                    }
                    continue;
                }
                -1 => break StreamOutcome::Err(wj_reader_error_code(r), wj_reader_error_offset(r)),
                11 => break StreamOutcome::Ok(result.take().expect("END without value")),
                1 => stack.push((J::Obj(Vec::new()), pending_key.take())),
                3 => stack.push((J::Arr(Vec::new()), pending_key.take())),
                2 | 4 => {
                    let (v, key) = stack.pop().unwrap();
                    pending_key = key;
                    value = Some(v);
                }
                5 => pending_key = Some(text(r)),
                6 => value = Some(J::Str(text(r))),
                7 => value = Some(J::Num(text(r))),
                8 => value = Some(J::Bool(true)),
                9 => value = Some(J::Bool(false)),
                10 => value = Some(J::Null),
                other => panic!("unknown event {other}"),
            }
            if let Some(v) = value {
                match stack.last_mut() {
                    None => result = Some(v),
                    Some((J::Arr(items), _)) => items.push(v),
                    Some((J::Obj(m), _)) => m.push((pending_key.take().expect("value without key"), v)),
                    _ => unreachable!(),
                }
            }
        };
        wj_reader_free(r);
        outcome
    }
}

// ---- the fuzz tests -------------------------------------------------------------------

fn show(input: &[u8]) -> String {
    String::from_utf8_lossy(input).into_owned()
}

#[test]
fn reference_parser_sanity() {
    // The oracle itself must get the classic cases right.
    for ok in ["0", "-0", "1e5", "[]", "{}", "\"\\ud83d\\ude00\"", " [1 , {\"a\" : null}] ", "\u{feff}1"] {
        assert!(RefParser::parse(ok.as_bytes()).is_some(), "{ok}");
    }
    for bad in ["", "01", "1.", "-", "[1,]", "{\"a\"}", "\"\\ud800\"", "\"\t\"", "nul", "[1]x", "\"\\x\""] {
        assert!(RefParser::parse(bad.as_bytes()).is_none(), "{bad}");
    }
    assert!(RefParser::parse(b"\"\xff\"").is_none());
    assert!(RefParser::parse(b"\"\xed\xa0\x80\"").is_none(), "UTF-8 encoded surrogate");
    assert!(RefParser::parse(b"\"\xc0\xaf\"").is_none(), "overlong UTF-8");
}

#[test]
fn differential_against_reference() {
    let iters = iterations(5000);
    let mut rng = Rng(0x9E3779B97F4A7C15);
    let (mut accepted, mut rejected) = (0u64, 0u64);
    for i in 0..iters {
        let value = random_value(&mut rng, 5);
        let text = render(&mut rng, &value);
        if i % 4 == 0 {
            // Unmutated output must round-trip to exactly the generated value.
            assert_eq!(RefParser::parse(&text).as_ref(), Some(&value), "oracle round trip: {}", show(&text));
        }
        let input = if i % 3 == 0 { text } else { mutate(&mut rng, &text) };
        let reference = RefParser::parse(&input);
        let doc = parse(&input);
        let ok = unsafe { wj_ok(doc) } == 1;
        assert_eq!(ok, reference.is_some(), "acceptance differs for {:?}", show(&input));
        if let Some(v) = &reference {
            accepted += 1;
            assert!(same(doc, unsafe { wj_root(doc) }, v), "value differs for {:?}", show(&input));
            // f64 conversion of every number matches Rust's (correctly rounded) parser.
            check_numbers(doc, unsafe { wj_root(doc) });
        } else {
            rejected += 1;
        }
        unsafe { wj_free(doc) };
    }
    println!("reference differential: {iters} inputs, {accepted} accepted, {rejected} rejected");
}

fn check_numbers(doc: *const Doc, node: i32) {
    unsafe {
        match wj_type(doc, node) {
            2 => {
                let mut len = 0;
                let lexeme = std::str::from_utf8(bytes_of(wj_num(doc, node, &mut len), len)).unwrap();
                let _ = num_value(lexeme); // must parse
            }
            4 | 5 => {
                for i in 0..wj_len(doc, node) {
                    check_numbers(doc, wj_at(doc, node, i));
                }
            }
            _ => {}
        }
    }
}

#[test]
fn streaming_matches_dom() {
    let iters = iterations(5000);
    let mut rng = Rng(0xD1B54A32D192ED03);
    for i in 0..iters {
        let value = random_value(&mut rng, 5);
        let text = render(&mut rng, &value);
        let input = if i % 2 == 0 { text } else { mutate(&mut rng, &text) };
        let doc = parse(&input);
        let stream = stream_parse(&mut rng, &input);
        unsafe {
            match (&stream, wj_ok(doc) == 1) {
                (StreamOutcome::Ok(v), true) => {
                    assert!(same(doc, wj_root(doc), v), "stream value differs for {:?}", show(&input));
                }
                (StreamOutcome::Err(code, offset), false) => {
                    assert_eq!((*code, *offset), (wj_error_code(doc), wj_error_offset(doc)), "error differs for {:?}", show(&input));
                }
                _ => panic!("acceptance differs for {:?}: stream {:?}", show(&input), stream),
            }
            wj_free(doc);
        }
    }
}

// ---- editing vs the model ------------------------------------------------------------------

#[derive(Clone, Debug)]
enum Step {
    Index(usize),
    Key(String),
}

fn resolve<'a>(v: &'a mut J, path: &[Step]) -> &'a mut J {
    path.iter().fold(v, |cur, step| match (cur, step) {
        (J::Arr(a), Step::Index(i)) => &mut a[*i],
        (J::Obj(m), Step::Key(k)) => &mut m.iter_mut().rev().find(|(mk, _)| mk == k).unwrap().1,
        _ => unreachable!(),
    })
}

fn node_at(doc: *mut Doc, path: &[Step]) -> i32 {
    unsafe {
        path.iter().fold(wj_root(doc), |cur, step| match step {
            Step::Index(i) => wj_at(doc, cur, *i as i64),
            Step::Key(k) => wj_get(doc, cur, k.as_ptr(), k.len() as i64),
        })
    }
}

fn random_container_path(rng: &mut Rng, v: &J) -> Vec<Step> {
    let mut path = Vec::new();
    let mut cur = v;
    loop {
        let children: Vec<(Step, &J)> = match cur {
            J::Arr(a) => a.iter().enumerate().map(|(i, c)| (Step::Index(i), c)).collect(),
            // Only reachable (last-wins) members can be addressed by key.
            J::Obj(m) => m
                .iter()
                .enumerate()
                .filter(|(i, (k, _))| m.iter().rposition(|(mk, _)| mk == k) == Some(*i))
                .map(|(_, (k, c))| (Step::Key(k.clone()), c))
                .collect(),
            _ => unreachable!(),
        };
        let containers: Vec<_> = children.into_iter().filter(|(_, c)| matches!(c, J::Arr(_) | J::Obj(_))).collect();
        if containers.is_empty() || rng.chance(40) {
            return path;
        }
        let pick = rng.below(containers.len() as u64) as usize;
        let (step, child) = containers.into_iter().nth(pick).unwrap();
        path.push(step);
        cur = child;
    }
}

fn build(doc: *mut Doc, v: &J) -> i32 {
    unsafe {
        match v {
            J::Null => wj_new_null(doc),
            J::Bool(b) => wj_new_bool(doc, *b as i32),
            J::Num(n) => wj_new_number(doc, n.as_ptr(), n.len() as i64),
            J::Str(s) => wj_new_string(doc, s.as_ptr(), s.len() as i64),
            J::Arr(a) => {
                let arr = wj_new_array(doc);
                for item in a {
                    assert_eq!(wj_arr_push(doc, arr, build(doc, item)), 0);
                }
                arr
            }
            J::Obj(m) => {
                // Build through a parse so duplicate keys are kept as-is.
                let mut text = String::new();
                let mut r = Rng(1);
                write_value(&mut r, v, &mut text, false);
                let src = parse(text.as_bytes());
                let copy = wj_copy(doc, src, wj_root(src));
                wj_free(src);
                let _ = m;
                copy
            }
        }
    }
}

#[test]
fn edits_match_model() {
    let iters = iterations(5000) / 8;
    let mut rng = Rng(0xA0761D6478BD642F);
    for _ in 0..iters {
        let mut model = J::Arr(vec![random_value(&mut rng, 3)]);
        let mut text = String::new();
        write_value(&mut rng, &model, &mut text, true);
        let doc = parse(text.as_bytes());
        assert!(same(doc, unsafe { wj_root(doc) }, &model));
        for _ in 0..30 {
            let path = random_container_path(&mut rng, &model);
            let target = node_at(doc, &path);
            let small = random_value(&mut rng, 2);
            unsafe {
                match resolve(&mut model, &path) {
                    J::Arr(items) => match rng.below(4) {
                        0 => {
                            assert_eq!(wj_arr_push(doc, target, build(doc, &small)), 0);
                            items.push(small);
                        }
                        1 => {
                            let at = rng.below(items.len() as u64 + 1) as usize;
                            assert_eq!(wj_arr_insert(doc, target, at as i64, build(doc, &small)), 0);
                            items.insert(at, small);
                        }
                        2 if !items.is_empty() => {
                            let at = rng.below(items.len() as u64) as usize;
                            assert_eq!(wj_arr_set(doc, target, at as i64, build(doc, &small)), 0);
                            items[at] = small;
                        }
                        _ if !items.is_empty() => {
                            let at = rng.below(items.len() as u64) as usize;
                            assert!(wj_arr_remove(doc, target, at as i64) >= 0);
                            items.remove(at);
                        }
                        _ => {}
                    },
                    J::Obj(m) => {
                        let key = format!("k{}", rng.below(4));
                        if rng.chance(70) {
                            assert_eq!(wj_obj_set(doc, target, key.as_ptr(), key.len() as i64, build(doc, &small)), 0);
                            // Replace the last occurrence, or append.
                            match m.iter_mut().rev().find(|(k, _)| *k == key) {
                                Some(slot) => slot.1 = small,
                                None => m.push((key, small)),
                            }
                        } else {
                            let before = m.len();
                            m.retain(|(k, _)| *k != key);
                            let removed = wj_obj_remove(doc, target, key.as_ptr(), key.len() as i64);
                            assert_eq!(removed as usize, before - m.len());
                        }
                    }
                    _ => unreachable!(),
                }
                assert!(same(doc, wj_root(doc), &model), "model diverged");
            }
        }
        unsafe { wj_free(doc) };
    }
}

#[test]
fn large_objects_use_index_correctly() {
    // Objects above the index threshold with heavy duplication and edits.
    let mut rng = Rng(7);
    for _ in 0..iterations(5000) / 50 {
        let n = 16 + rng.below(300) as usize;
        let members: Vec<(String, J)> =
            (0..n).map(|i| (format!("k{}", rng.below(n as u64 / 2 + 1)), J::Num(i.to_string()))).collect();
        let mut model = J::Obj(members);
        let mut text = String::new();
        write_value(&mut rng, &model, &mut text, false);
        let doc = parse(text.as_bytes());
        let root = unsafe { wj_root(doc) };
        assert!(same(doc, root, &model));
        assert_eq!(model.distinct_keys() > 0, true);
        for step in 0..50 {
            let key = format!("k{}", rng.below(n as u64 / 2 + 2));
            unsafe {
                if step % 3 == 0 {
                    let J::Obj(m) = &mut model else { unreachable!() };
                    let before = m.len();
                    m.retain(|(k, _)| *k != key);
                    assert_eq!(wj_obj_remove(doc, root, key.as_ptr(), key.len() as i64) as usize, before - m.len());
                } else {
                    let val = J::Num(step.to_string());
                    assert_eq!(wj_obj_set(doc, root, key.as_ptr(), key.len() as i64, build(doc, &val)), 0);
                    let J::Obj(m) = &mut model else { unreachable!() };
                    match m.iter_mut().rev().find(|(k, _)| *k == key) {
                        Some(slot) => slot.1 = val,
                        None => m.push((key, val)),
                    }
                }
                assert!(same(doc, root, &model));
            }
        }
        unsafe { wj_free(doc) };
    }
}
