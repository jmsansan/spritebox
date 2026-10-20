//! A small JSON writer for the metadata, so that its form is the crate's own and stays
//! the same from version to version.

use std::fmt::Write as _;

/// A JSON string literal.
pub(crate) fn quote(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Writes nested objects and arrays with two-space indentation. Values that are
/// written whole, such as `[0, 0, 8, 8]`, stay on one line.
pub(crate) struct Writer {
    out: String,
    /// For each open object or array: whether it has an item yet.
    open: Vec<bool>,
}

impl Writer {
    pub fn new() -> Self {
        Self {
            out: String::new(),
            open: Vec::new(),
        }
    }

    fn item(&mut self) {
        if let Some(has_item) = self.open.last_mut() {
            if *has_item {
                self.out.push(',');
            }
            *has_item = true;
            self.out.push('\n');
            for _ in 0..self.open.len() {
                self.out.push_str("  ");
            }
        }
    }

    fn key(&mut self, key: Option<&str>) {
        self.item();
        if let Some(key) = key {
            self.out.push_str(&quote(key));
            self.out.push_str(": ");
        }
    }

    /// Opens an object, as a member `key` of the current object or as an array item.
    pub fn object(&mut self, key: Option<&str>) {
        self.key(key);
        self.out.push('{');
        self.open.push(false);
    }

    /// Opens an array.
    pub fn array(&mut self, key: Option<&str>) {
        self.key(key);
        self.out.push('[');
        self.open.push(false);
    }

    /// Closes the innermost object or array.
    pub fn close(&mut self, bracket: char) {
        if self.open.pop() == Some(true) {
            self.out.push('\n');
            for _ in 0..self.open.len() {
                self.out.push_str("  ");
            }
        }
        self.out.push(bracket);
    }

    /// Writes a value already in JSON form.
    pub fn raw(&mut self, key: Option<&str>, json: &str) {
        self.key(key);
        self.out.push_str(json);
    }

    pub fn string(&mut self, key: Option<&str>, value: &str) {
        self.raw(key, &quote(value));
    }

    pub fn finish(mut self) -> String {
        self.out.push('\n');
        self.out
    }
}

/// `[a, b, c]` on one line.
pub(crate) fn list(values: &[u32]) -> String {
    let items: Vec<String> = values.iter().map(|v| v.to_string()).collect();
    format!("[{}]", items.join(", "))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_nested_values() {
        let mut w = Writer::new();
        w.object(None);
        w.raw(Some("version"), "1");
        w.object(Some("empty"));
        w.close('}');
        w.array(Some("items"));
        w.raw(None, &list(&[1, 2]));
        w.string(None, "a\"b\n\u{1}");
        w.close(']');
        w.close('}');
        assert_eq!(
            w.finish(),
            "{\n  \"version\": 1,\n  \"empty\": {},\n  \"items\": [\n    [1, 2],\n    \"a\\\"b\\n\\u0001\"\n  ]\n}\n"
        );
    }
}
