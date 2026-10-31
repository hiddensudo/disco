//! Bounded JSON-RPC framing and explicit UTF-16 positions, independent of UI.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    io::{BufRead, Read, Write},
    path::{Path, PathBuf},
};
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Position {
    pub line: u32,
    pub character: u32,
}
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Range {
    pub start: Position,
    pub end: Position,
}
pub fn position(text: &str, index: usize) -> Position {
    let mut p = Position::default();
    for c in text.chars().take(index) {
        if c == '\n' {
            p.line += 1;
            p.character = 0;
        } else {
            p.character += c.len_utf16() as u32;
        }
    }
    p
}
pub fn byte_offset(text: &str, p: Position) -> Option<usize> {
    let mut current = Position::default();
    for (byte, c) in text.char_indices() {
        if current == p {
            return Some(byte);
        }
        if c == '\n' {
            if current.line == p.line {
                return None;
            }
            current.line += 1;
            current.character = 0;
        } else {
            current.character += c.len_utf16() as u32;
        }
    }
    (current == p).then_some(text.len())
}
pub fn uri(path: &Path) -> Result<String, String> {
    url::Url::from_file_path(path)
        .map(String::from)
        .map_err(|_| "Invalid file URI".into())
}
pub fn path(uri: &str) -> Option<PathBuf> {
    url::Url::parse(uri).ok()?.to_file_path().ok()
}
pub fn write(w: &mut impl Write, value: &Value) -> std::io::Result<()> {
    let data = serde_json::to_vec(value)?;
    write!(w, "Content-Length: {}\r\n\r\n", data.len())?;
    w.write_all(&data)?;
    w.flush()
}
pub fn read(r: &mut impl BufRead) -> std::io::Result<Option<Value>> {
    let mut length = None;
    let mut bytes = 0;
    loop {
        let mut line = String::new();
        let n = r.take(16385).read_line(&mut line)?;
        if n == 0 {
            return if bytes == 0 {
                Ok(None)
            } else {
                Err(std::io::Error::other("Incomplete LSP header"))
            };
        }
        bytes += n;
        if bytes > 16384 {
            return Err(std::io::Error::other("LSP header too large"));
        }
        if line == "\r\n" || line == "\n" {
            break;
        }
        if let Some((name, value)) = line.split_once(':') {
            if name.eq_ignore_ascii_case("Content-Length") {
                let value = value
                    .trim()
                    .parse::<usize>()
                    .map_err(std::io::Error::other)?;
                if value > 8 * 1024 * 1024 || length.replace(value).is_some() {
                    return Err(std::io::Error::other("Invalid LSP content length"));
                }
            }
        }
    }
    let length = length.ok_or_else(|| std::io::Error::other("Missing LSP content length"))?;
    let mut data = vec![0; length];
    r.read_exact(&mut data)?;
    serde_json::from_slice(&data)
        .map(Some)
        .map_err(std::io::Error::other)
}
#[derive(Clone, Debug)]
pub struct Completion {
    pub label: String,
    pub detail: String,
    pub raw: Value,
}
#[derive(Clone, Debug)]
pub struct Diagnostic {
    pub range: Range,
    pub severity: u8,
    pub message: String,
}
pub fn completion_items(value: &Value) -> Vec<Completion> {
    let items = value
        .as_array()
        .or_else(|| value.get("items").and_then(Value::as_array));
    let mut result: Vec<_> = items
        .into_iter()
        .flatten()
        .take(1000)
        .filter(|v| v["insertTextFormat"].as_u64() != Some(2))
        .filter_map(|v| {
            Some(Completion {
                label: v["label"].as_str()?.chars().take(256).collect(),
                detail: v["detail"]
                    .as_str()
                    .unwrap_or("")
                    .chars()
                    .take(512)
                    .collect(),
                raw: v.clone(),
            })
        })
        .collect();
    result.sort_by_key(|item| {
        item.raw["sortText"]
            .as_str()
            .unwrap_or(&item.label)
            .to_owned()
    });
    result
}
pub fn apply_completion(
    text: &str,
    index: usize,
    item: &Completion,
) -> Result<(String, usize), String> {
    let byte = text
        .char_indices()
        .nth(index)
        .map_or(text.len(), |(b, _)| b);
    let mut start = byte;
    for (b, c) in text[..byte].char_indices().rev() {
        if c.is_alphanumeric() || c == '_' {
            start = b;
        } else {
            break;
        }
    }
    let primary = &item.raw["textEdit"];
    let (a, b, new) = if let Some(range) = primary.get("range") {
        let range: Range = serde_json::from_value(range.clone()).map_err(|e| e.to_string())?;
        (
            byte_offset(text, range.start).ok_or("Invalid completion start")?,
            byte_offset(text, range.end).ok_or("Invalid completion end")?,
            primary["newText"]
                .as_str()
                .ok_or("Missing completion text")?
                .to_owned(),
        )
    } else {
        (
            start,
            byte,
            item.raw["insertText"]
                .as_str()
                .unwrap_or(&item.label)
                .to_owned(),
        )
    };
    let mut edits = vec![(a, b, new.clone(), true)];
    if let Some(extra) = item.raw["additionalTextEdits"].as_array() {
        if extra.len() > 64 {
            return Err("Too many completion edits".into());
        }
        for edit in extra {
            let range: Range =
                serde_json::from_value(edit["range"].clone()).map_err(|e| e.to_string())?;
            edits.push((
                byte_offset(text, range.start).ok_or("Invalid edit start")?,
                byte_offset(text, range.end).ok_or("Invalid edit end")?,
                edit["newText"]
                    .as_str()
                    .ok_or("Missing edit text")?
                    .to_owned(),
                false,
            ));
        }
    }
    edits.sort_by_key(|e| (e.0, e.1));
    let mut result = String::new();
    let mut at = 0;
    let mut cursor = 0;
    for (a, b, new, primary) in edits {
        if a < at || b < a || b > text.len() {
            return Err("Overlapping completion edits".into());
        }
        result.push_str(&text[at..a]);
        result.push_str(&new);
        if primary {
            cursor = result.chars().count();
        }
        at = b;
    }
    result.push_str(&text[at..]);
    if result.len() > 128 * 1024 {
        return Err("Completion exceeds the editor limit".into());
    }
    Ok((result, cursor))
}
pub fn hover_text(v: &Value) -> String {
    fn collect(v: &Value, out: &mut Vec<String>) {
        if let Some(text) = v.as_str() {
            out.push(text.into());
        } else if let Some(list) = v.as_array() {
            for v in list {
                collect(v, out);
            }
        } else if let Some(text) = v["value"].as_str() {
            out.push(text.into());
        }
    }
    let mut lines = vec![];
    collect(&v["contents"], &mut lines);
    lines.join("\n\n").chars().take(8192).collect()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn framing_utf16_uris_and_completion_edits_are_bounded() {
        let text = "// 🦀\nlet café = 42;";
        for i in 0..=text.chars().count() {
            let b = byte_offset(text, position(text, i)).unwrap();
            assert_eq!(text[..b].chars().count(), i);
        }
        assert!(
            byte_offset(
                "🦀",
                Position {
                    line: 0,
                    character: 1
                }
            )
            .is_none()
        );
        let path = Path::new("/tmp/a b/λ.rs");
        assert_eq!(super::path(&uri(path).unwrap()).as_deref(), Some(path));
        let value = serde_json::json!({"jsonrpc":"2.0","id":1,"result":"🦀"});
        let mut bytes = vec![];
        write(&mut bytes, &value).unwrap();
        assert_eq!(read(&mut std::io::Cursor::new(bytes)).unwrap(), Some(value));
        assert!(
            read(&mut std::io::Cursor::new(
                b"Content-Length: 999999999\r\n\r\n"
            ))
            .is_err()
        );
        let item = Completion {
            label: "vector".into(),
            detail: String::new(),
            raw: serde_json::json!({"insertText":"vector"}),
        };
        assert_eq!(
            apply_completion("std::vec", 8, &item).unwrap(),
            ("std::vector".into(), 11)
        );
    }
}
