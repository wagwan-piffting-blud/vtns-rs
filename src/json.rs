//! Minimal JSON reader for the parity-test captures (recorded from the original
//! engine's output). Used only by parity tests and tooling; the synthesizer
//! itself never parses JSON.

use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    Arr(Vec<Json>),
    Obj(BTreeMap<String, Json>),
}

static NULL: Json = Json::Null;

impl Json {
    pub fn parse(s: &str) -> Result<Json, String> {
        let mut p = Parser { b: s.as_bytes(), i: 0 };
        let v = p.value()?;
        p.ws();
        if p.i != p.b.len() {
            return Err(format!("trailing data at {}", p.i));
        }
        Ok(v)
    }

    pub fn load(path: &std::path::Path) -> Result<Json, String> {
        let s = std::fs::read_to_string(path).map_err(|e| format!("{}: {}", path.display(), e))?;
        Json::parse(&s)
    }

    /// Object member, or `Null` when absent / not an object.
    pub fn get(&self, key: &str) -> &Json {
        match self {
            Json::Obj(m) => m.get(key).unwrap_or(&NULL),
            _ => &NULL,
        }
    }

    pub fn at(&self, i: usize) -> &Json {
        match self {
            Json::Arr(a) => a.get(i).unwrap_or(&NULL),
            _ => &NULL,
        }
    }

    pub fn arr(&self) -> &[Json] {
        match self {
            Json::Arr(a) => a,
            _ => &[],
        }
    }

    pub fn f64(&self) -> Option<f64> {
        match self {
            Json::Num(n) => Some(*n),
            Json::Bool(b) => Some(*b as i32 as f64),
            _ => None,
        }
    }

    pub fn i64(&self) -> Option<i64> {
        self.f64().map(|n| n as i64)
    }

    pub fn str(&self) -> Option<&str> {
        match self {
            Json::Str(s) => Some(s),
            _ => None,
        }
    }

    pub fn is_null(&self) -> bool {
        matches!(self, Json::Null)
    }

    /// Decode a hex string field (e.g. `ctx`, `raw`) into bytes. Non-hex pairs
    /// such as a capture's `pp` pointer blanks decode as 0.
    pub fn hex(&self) -> Vec<u8> {
        let s = self.str().unwrap_or("").as_bytes();
        s.chunks(2)
            .map(|c| {
                let h = |x: u8| (x as char).to_digit(16).unwrap_or(0) as u8;
                if c.len() == 2 { h(c[0]) << 4 | h(c[1]) } else { 0 }
            })
            .collect()
    }
}

struct Parser<'a> {
    b: &'a [u8],
    i: usize,
}

impl<'a> Parser<'a> {
    fn ws(&mut self) {
        while self.i < self.b.len() && matches!(self.b[self.i], b' ' | b'\t' | b'\r' | b'\n') {
            self.i += 1;
        }
    }

    fn eat(&mut self, c: u8) -> Result<(), String> {
        self.ws();
        if self.b.get(self.i) == Some(&c) {
            self.i += 1;
            Ok(())
        } else {
            Err(format!("expected '{}' at {}", c as char, self.i))
        }
    }

    fn value(&mut self) -> Result<Json, String> {
        self.ws();
        match self.b.get(self.i) {
            Some(b'{') => self.object(),
            Some(b'[') => self.array(),
            Some(b'"') => Ok(Json::Str(self.string()?)),
            Some(b't') => self.lit("true", Json::Bool(true)),
            Some(b'f') => self.lit("false", Json::Bool(false)),
            Some(b'n') => self.lit("null", Json::Null),
            Some(_) => self.number(),
            None => Err("unexpected end".into()),
        }
    }

    fn lit(&mut self, word: &str, v: Json) -> Result<Json, String> {
        if self.b[self.i..].starts_with(word.as_bytes()) {
            self.i += word.len();
            Ok(v)
        } else {
            Err(format!("bad literal at {}", self.i))
        }
    }

    fn number(&mut self) -> Result<Json, String> {
        let st = self.i;
        while self.i < self.b.len() && matches!(self.b[self.i], b'-' | b'+' | b'.' | b'e' | b'E' | b'0'..=b'9') {
            self.i += 1;
        }
        let s = std::str::from_utf8(&self.b[st..self.i]).unwrap_or("");
        match s {
            "NaN" => Ok(Json::Num(f64::NAN)),
            _ => s.parse().map(Json::Num).map_err(|_| format!("bad number '{}' at {}", s, st)),
        }
    }

    fn string(&mut self) -> Result<String, String> {
        self.i += 1;
        let mut out = String::new();
        loop {
            let c = *self.b.get(self.i).ok_or("unterminated string")?;
            self.i += 1;
            match c {
                b'"' => return Ok(out),
                b'\\' => {
                    let e = *self.b.get(self.i).ok_or("bad escape")?;
                    self.i += 1;
                    match e {
                        b'n' => out.push('\n'),
                        b't' => out.push('\t'),
                        b'r' => out.push('\r'),
                        b'b' => out.push('\u{8}'),
                        b'f' => out.push('\u{c}'),
                        b'u' => {
                            let hex = std::str::from_utf8(self.b.get(self.i..self.i + 4).ok_or("bad \\u")?)
                                .map_err(|_| "bad \\u")?;
                            let mut cp = u32::from_str_radix(hex, 16).map_err(|_| "bad \\u")?;
                            self.i += 4;
                            if (0xd800..0xdc00).contains(&cp) && self.b[self.i..].starts_with(b"\\u") {
                                let lo = std::str::from_utf8(&self.b[self.i + 2..self.i + 6]).map_err(|_| "bad \\u")?;
                                let lo = u32::from_str_radix(lo, 16).map_err(|_| "bad \\u")?;
                                cp = 0x10000 + ((cp - 0xd800) << 10) + (lo - 0xdc00);
                                self.i += 6;
                            }
                            out.push(char::from_u32(cp).unwrap_or('\u{fffd}'));
                        }
                        other => out.push(other as char),
                    }
                }
                _ => {
                    let st = self.i - 1;
                    let mut end = self.i;
                    while end < self.b.len() && self.b[end] != b'"' && self.b[end] != b'\\' {
                        end += 1;
                    }
                    out.push_str(&String::from_utf8_lossy(&self.b[st..end]));
                    self.i = end;
                }
            }
        }
    }

    fn array(&mut self) -> Result<Json, String> {
        self.i += 1;
        let mut v = Vec::new();
        self.ws();
        if self.b.get(self.i) == Some(&b']') {
            self.i += 1;
            return Ok(Json::Arr(v));
        }
        loop {
            v.push(self.value()?);
            self.ws();
            match self.b.get(self.i) {
                Some(b',') => self.i += 1,
                Some(b']') => {
                    self.i += 1;
                    return Ok(Json::Arr(v));
                }
                _ => return Err(format!("bad array at {}", self.i)),
            }
        }
    }

    fn object(&mut self) -> Result<Json, String> {
        self.i += 1;
        let mut m = BTreeMap::new();
        self.ws();
        if self.b.get(self.i) == Some(&b'}') {
            self.i += 1;
            return Ok(Json::Obj(m));
        }
        loop {
            self.ws();
            let k = self.string()?;
            self.eat(b':')?;
            let v = self.value()?;
            m.insert(k, v);
            self.ws();
            match self.b.get(self.i) {
                Some(b',') => self.i += 1,
                Some(b'}') => {
                    self.i += 1;
                    return Ok(Json::Obj(m));
                }
                _ => return Err(format!("bad object at {}", self.i)),
            }
        }
    }
}
