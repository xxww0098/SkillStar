//! Byte-preserving JSON walk. Unchanged documents come back as the original
//! bytes. A body that is not JSON is reported so the caller can mask it as text.

pub(crate) enum Walked {
    Same,
    Changed(Vec<u8>),
    NotJson,
}

struct Walker<'a> {
    input: &'a [u8],
    out: Vec<u8>,
    last: usize,
    changed: bool,
}

pub(crate) fn walk(
    input: &[u8],
    on_string: &mut impl FnMut(&str, &str, &str) -> Option<String>,
) -> Walked {
    let mut walker = Walker {
        input,
        out: Vec::new(),
        last: 0,
        changed: false,
    };
    let Some(end) = walker.value(skip_ws(input, 0), "", "", on_string) else {
        return Walked::NotJson;
    };
    if skip_ws(input, end) != input.len() {
        return Walked::NotJson;
    }
    if !walker.changed {
        Walked::Same
    } else {
        walker.out.extend_from_slice(&input[walker.last..]);
        Walked::Changed(walker.out)
    }
}

impl<'a> Walker<'a> {
    fn value<F>(&mut self, i: usize, path: &str, key: &str, on_string: &mut F) -> Option<usize>
    where
        F: FnMut(&str, &str, &str) -> Option<String>,
    {
        if i >= self.input.len() {
            return None;
        }
        match self.input[i] {
            b'{' => self.object(i, path, on_string),
            b'[' => self.array(i, path, key, on_string),
            b'"' => self.string(i, path, key, on_string),
            _ => self.atom(i),
        }
    }

    fn object<F>(&mut self, i: usize, path: &str, on_string: &mut F) -> Option<usize>
    where
        F: FnMut(&str, &str, &str) -> Option<String>,
    {
        let mut i = skip_ws(self.input, i + 1);
        if i < self.input.len() && self.input[i] == b'}' {
            return Some(i + 1);
        }
        loop {
            if i >= self.input.len() || self.input[i] != b'"' {
                return None;
            }
            let key_end = string_end(self.input, i)?;
            let key = decode_string(&self.input[i..key_end])?;
            i = skip_ws(self.input, key_end);
            if i >= self.input.len() || self.input[i] != b':' {
                return None;
            }
            let child = if path.is_empty() {
                key.clone()
            } else {
                format!("{path}.{key}")
            };
            i = self.value(skip_ws(self.input, i + 1), &child, &key, on_string)?;
            i = skip_ws(self.input, i);
            if i < self.input.len() && self.input[i] == b',' {
                i = skip_ws(self.input, i + 1);
                continue;
            }
            if i < self.input.len() && self.input[i] == b'}' {
                return Some(i + 1);
            }
            return None;
        }
    }

    fn array<F>(&mut self, i: usize, path: &str, key: &str, on_string: &mut F) -> Option<usize>
    where
        F: FnMut(&str, &str, &str) -> Option<String>,
    {
        let mut i = skip_ws(self.input, i + 1);
        if i < self.input.len() && self.input[i] == b']' {
            return Some(i + 1);
        }
        let mut index = 0;
        loop {
            let child = if path.is_empty() {
                index.to_string()
            } else {
                format!("{path}.{index}")
            };
            i = self.value(i, &child, key, on_string)?;
            i = skip_ws(self.input, i);
            if i < self.input.len() && self.input[i] == b',' {
                i = skip_ws(self.input, i + 1);
                index += 1;
                continue;
            }
            if i < self.input.len() && self.input[i] == b']' {
                return Some(i + 1);
            }
            return None;
        }
    }

    fn string<F>(&mut self, i: usize, path: &str, key: &str, on_string: &mut F) -> Option<usize>
    where
        F: FnMut(&str, &str, &str) -> Option<String>,
    {
        let end = string_end(self.input, i)?;
        let text = decode_string(&self.input[i..end])?;
        if let Some(replacement) = on_string(path, key, &text) {
            self.out.extend_from_slice(&self.input[self.last..i]);
            self.out.push(b'"');
            self.out
                .extend_from_slice(json_escape(&replacement).as_bytes());
            self.out.push(b'"');
            self.last = end;
            self.changed = true;
        }
        Some(end)
    }

    fn atom(&self, i: usize) -> Option<usize> {
        let mut j = i;
        while j < self.input.len()
            && !matches!(
                self.input[j],
                b',' | b']' | b'}' | b' ' | b'\t' | b'\r' | b'\n'
            )
        {
            j += 1;
        }
        if j == i { None } else { Some(j) }
    }
}

fn skip_ws(input: &[u8], mut i: usize) -> usize {
    while i < input.len() && matches!(input[i], b' ' | b'\t' | b'\n' | b'\r') {
        i += 1;
    }
    i
}

fn string_end(input: &[u8], start: usize) -> Option<usize> {
    let mut j = start + 1;
    while j < input.len() {
        match input[j] {
            b'\\' => {
                j += 2;
                if j > input.len() {
                    return None;
                }
            }
            b'"' => return Some(j + 1),
            _ => j += 1,
        }
    }
    None
}

fn decode_string(quoted: &[u8]) -> Option<String> {
    if quoted.contains(&b'\\') {
        serde_json::from_slice(quoted).ok()
    } else {
        let inner = quoted.get(1..quoted.len().saturating_sub(1))?;
        std::str::from_utf8(inner).ok().map(str::to_string)
    }
}

pub(crate) fn json_escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for c in value.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let code = c as u32;
                out.push_str(&format!("\\u{code:04x}"));
            }
            c => out.push(c),
        }
    }
    out
}
