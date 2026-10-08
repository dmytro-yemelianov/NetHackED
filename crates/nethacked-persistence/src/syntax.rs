use crate::{Error, SaveLimits};

fn digest_key(key: &str) -> bool {
    matches!(
        key,
        "artifact"
            | "caller_artifact"
            | "next_digest"
            | "request_digest"
            | "data_digest"
            | "schema_digest"
            | "pipeline_digest"
    )
}
pub(crate) fn hex_key(key: &str) -> bool {
    matches!(
        key,
        "artifact"
            | "guest_bytes"
            | "payload"
            | "next_payload"
            | "reply"
            | "data"
            | "history_encoding"
            | "events"
            | "caller_artifact"
            | "next_digest"
            | "request_digest"
            | "data_digest"
            | "schema_digest"
            | "pipeline_digest"
            | "accounting_payload"
            | "program_payload"
            | "local_payload"
            | "trace"
            | "outcome"
            | "source_payload"
            | "request"
            | "value"
    )
}
/// Framing/count walk with bounded key/name scratch at each nesting level.
pub(crate) fn check(bytes: &[u8], limits: SaveLimits, canonical: bool) -> Result<(), Error> {
    std::str::from_utf8(bytes).map_err(|_| Error::Json)?;
    let mut parser = Parser {
        bytes,
        pos: 0,
        limits,
        canonical,
        edges: 0,
    };
    parser.value(1, "")?;
    if parser.pos != bytes.len() {
        return Err(Error::Noncanonical);
    }
    Ok(())
}
struct Parser<'a> {
    bytes: &'a [u8],
    pos: usize,
    limits: SaveLimits,
    canonical: bool,
    edges: usize,
}
impl Parser<'_> {
    fn byte(&self) -> Result<u8, Error> {
        self.bytes.get(self.pos).copied().ok_or(Error::Json)
    }
    fn eat(&mut self, b: u8) -> Result<(), Error> {
        if self.byte()? != b {
            return Err(Error::Json);
        }
        self.pos += 1;
        Ok(())
    }
    fn string(&mut self, key: &str, is_key: bool) -> Result<String, Error> {
        let start = self.pos;
        self.eat(b'"')?;
        let hex = !is_key && hex_key(key);
        let bound = if hex && digest_key(key) {
            64
        } else if hex {
            self.limits
                .payload_bytes
                .checked_mul(2)
                .ok_or(Error::Limits)?
        } else if is_key {
            SaveLimits::default().name_bytes
        } else {
            self.limits.name_bytes
        };
        let content = self.pos;
        loop {
            let b = self.byte()?;
            if b == b'"' {
                break;
            }
            if b < 32 {
                return Err(Error::Json);
            }
            if self.pos - content >= bound.checked_mul(6).ok_or(Error::Limits)? {
                return Err(if hex { Error::Payload } else { Error::String });
            }
            if b == b'\\' {
                self.pos += 1;
                self.byte()?;
            }
            self.pos += 1;
        }
        self.pos += 1;
        // Raw bound before decoding/reserving: hex never permits escapes, names have
        // at most six source bytes per decoded byte. UTF-8 is already checked.
        let raw = &self.bytes[start..self.pos];
        if hex && (raw.len() - 2 > bound || raw.contains(&b'\\')) {
            return Err(Error::Payload);
        }
        let contents = &raw[1..raw.len() - 1];
        if hex {
            if contents.len() % 2 != 0
                || !contents
                    .iter()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(b))
            {
                return Err(Error::Hex);
            }
            if digest_key(key) && contents.len() != 64 {
                return Err(Error::Hex);
            }
            return Ok(String::new());
        }
        // Compute decoded byte size without allocating, and permit only canonical
        // escapes. In particular Unicode scalars must be literal UTF-8.
        let mut index = 0;
        let mut decoded = 0usize;
        while index < contents.len() {
            if contents[index] == b'\\' {
                index += 1;
                match contents.get(index).copied().ok_or(Error::Json)? {
                    b'"' | b'\\' | b'b' | b'f' | b'n' | b'r' | b't' => {}
                    b'u' => {
                        let digits = contents.get(index + 1..index + 5).ok_or(Error::Json)?;
                        if digits[0..2] != *b"00"
                            || !digits
                                .iter()
                                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(b))
                        {
                            return Err(Error::Noncanonical);
                        }
                        let digit = |b| if b <= b'9' { b - b'0' } else { b - b'a' + 10 };
                        let control = digit(digits[2]) * 16 + digit(digits[3]);
                        if control >= 32 || matches!(control, 8 | 9 | 10 | 12 | 13) {
                            return Err(Error::Noncanonical);
                        }
                        index += 4;
                    }
                    _ => return Err(Error::Noncanonical),
                }
            }
            decoded = decoded.checked_add(1).ok_or(Error::String)?;
            if decoded > bound {
                return Err(Error::String);
            }
            index += 1;
        }
        let s: String = serde_json::from_slice(raw).map_err(|_| Error::Json)?;
        if serde_json::to_vec(&s).map_err(|_| Error::Json)? != raw {
            return Err(Error::Noncanonical);
        }
        Ok(s)
    }
    fn value(&mut self, depth: usize, key: &str) -> Result<(), Error> {
        if depth > self.limits.depth {
            return Err(Error::Depth);
        }
        match self.byte()? {
            b'{' => {
                self.pos += 1;
                if self.byte()? == b'}' {
                    self.pos += 1;
                    return Ok(());
                }
                let mut previous: Option<String> = None;
                let mut count = 0usize;
                let mut has_target = false;
                let mut has_mode = false;
                loop {
                    count = count.checked_add(1).ok_or(Error::Rows)?;
                    if count > 128 {
                        return Err(Error::Rows);
                    }
                    let next = self.string("", true)?;
                    has_target |= next == "target";
                    has_mode |= next == "mode";
                    if self.canonical && previous.as_ref().is_some_and(|p| p >= &next) {
                        return Err(Error::Noncanonical);
                    }
                    self.eat(b':')?;
                    self.value(depth + 1, &next)?;
                    previous = Some(next);
                    match self.byte()? {
                        b'}' => {
                            self.pos += 1;
                            break;
                        }
                        b',' => self.pos += 1,
                        _ => return Err(Error::Json),
                    }
                }
                if has_target && has_mode {
                    self.edges = self.edges.checked_add(1).ok_or(Error::Edges)?;
                    if self.edges > self.limits.aggregate_edges {
                        return Err(Error::Edges);
                    }
                }
            }
            b'[' => {
                self.pos += 1;
                if self.byte()? == b']' {
                    self.pos += 1;
                    return Ok(());
                }
                let mut count = 0usize;
                loop {
                    count = count.checked_add(1).ok_or(Error::Rows)?;
                    let max = if key == "retention" {
                        self.limits.aggregate_edges
                    } else {
                        self.limits.rows_per_table
                    };
                    if count > max {
                        return Err(Error::Rows);
                    }
                    self.value(depth + 1, key)?;
                    match self.byte()? {
                        b']' => {
                            self.pos += 1;
                            break;
                        }
                        b',' => self.pos += 1,
                        _ => return Err(Error::Json),
                    }
                }
            }
            b'"' => {
                self.string(key, false)?;
            }
            b'0'..=b'9' => {
                let start = self.pos;
                while self.bytes.get(self.pos).is_some_and(u8::is_ascii_digit) {
                    self.pos += 1;
                }
                let raw = &self.bytes[start..self.pos];
                if raw.len() > 1 && raw[0] == b'0' {
                    return Err(Error::Noncanonical);
                }
                let text = std::str::from_utf8(raw).map_err(|_| Error::Json)?;
                text.parse::<u64>().map_err(|_| Error::Json)?;
            }
            b't' => self.literal(b"true")?,
            b'f' => self.literal(b"false")?,
            b'n' => self.literal(b"null")?,
            _ => return Err(Error::Json),
        }
        Ok(())
    }
    fn literal(&mut self, bytes: &[u8]) -> Result<(), Error> {
        for b in bytes {
            self.eat(*b)?;
        }
        Ok(())
    }
}
