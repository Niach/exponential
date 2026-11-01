//! dotLottie (`.lottie`): a zip with `manifest.json`, animations, images,
//! themes and state machines. Versions 1 (`animations/`, `images/`) and 2
//! (`a/`, `i/`, `t/`, `s/`) are both read. Images referenced by an animation
//! are inlined as data URLs so the result is a self-contained Lottie.
//!
//! A tiny zip reader (stored and deflate entries, central directory) keeps
//! the WASM small; zip64 and encryption are refused.

use serde_json::{Map, Value};
use std::collections::BTreeMap;

use crate::validate::Issue;

/// Total uncompressed size cap (the source limit × 8: images dominate).
pub const MAX_UNPACKED: usize = 16 * 1024 * 1024;

#[derive(Debug, Default)]
pub struct DotLottie {
    pub manifest: Value,
    /// Animations by id, in manifest order (images inlined).
    pub animations: Vec<(String, Value)>,
    /// Themes by id: slot id → slot value (`{"p": …}`), ready for `Motion::load_with`.
    pub themes: BTreeMap<String, Map<String, Value>>,
    /// State machines by id (raw JSON; EXP-1605 interprets them).
    pub state_machines: BTreeMap<String, Value>,
}

fn issue(code: &str, message: impl Into<String>) -> Vec<Issue> {
    vec![Issue { code: code.into(), path: String::new(), message: message.into() }]
}

fn u16le(b: &[u8], at: usize) -> Option<usize> {
    Some(u16::from_le_bytes(b.get(at..at + 2)?.try_into().ok()?) as usize)
}

fn u32le(b: &[u8], at: usize) -> Option<usize> {
    Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?) as usize)
}

/// Read every file of a zip archive: name → bytes.
pub fn unzip(data: &[u8]) -> Result<BTreeMap<String, Vec<u8>>, Vec<Issue>> {
    // End of central directory: the last `PK\x05\x06` within 64 KB of the end.
    let eocd = (0..data.len().saturating_sub(21))
        .rev()
        .take(65_557)
        .find(|&i| data[i..].starts_with(b"PK\x05\x06"))
        .ok_or_else(|| issue("zip", "not a zip archive"))?;
    let count = u16le(data, eocd + 10).ok_or_else(|| issue("zip", "truncated"))?;
    let mut at = u32le(data, eocd + 16).ok_or_else(|| issue("zip", "truncated"))?;
    let mut files = BTreeMap::new();
    let mut total = 0usize;
    for _ in 0..count {
        if !data.get(at..).is_some_and(|d| d.starts_with(b"PK\x01\x02")) {
            return Err(issue("zip", "bad central directory"));
        }
        let flags = u16le(data, at + 8).unwrap_or(0);
        let method = u16le(data, at + 10).unwrap_or(0);
        let csize = u32le(data, at + 20).unwrap_or(0);
        let usize_ = u32le(data, at + 24).unwrap_or(0);
        let nlen = u16le(data, at + 28).unwrap_or(0);
        let xlen = u16le(data, at + 30).unwrap_or(0);
        let clen = u16le(data, at + 32).unwrap_or(0);
        let local = u32le(data, at + 42).unwrap_or(0);
        let name = String::from_utf8_lossy(data.get(at + 46..at + 46 + nlen).unwrap_or_default()).into_owned();
        at += 46 + nlen + xlen + clen;
        if flags & 1 != 0 {
            return Err(issue("zip", "encrypted entries are not supported"));
        }
        if csize == 0xFFFF_FFFF || usize_ == 0xFFFF_FFFF {
            return Err(issue("zip", "zip64 is not supported"));
        }
        if name.ends_with('/') {
            continue;
        }
        total += usize_;
        if total > MAX_UNPACKED {
            return Err(issue("limit-bytes", format!("unpacked size over {MAX_UNPACKED} bytes")));
        }
        let lnlen = u16le(data, local + 26).ok_or_else(|| issue("zip", "truncated"))?;
        let lxlen = u16le(data, local + 28).ok_or_else(|| issue("zip", "truncated"))?;
        let start = local + 30 + lnlen + lxlen;
        let raw = data.get(start..start + csize).ok_or_else(|| issue("zip", "truncated entry"))?;
        let bytes = match method {
            0 => raw.to_vec(),
            8 => miniz_oxide::inflate::decompress_to_vec_with_limit(raw, usize_.max(1))
                .map_err(|_| issue("zip", format!("cannot inflate `{name}`")))?,
            m => return Err(issue("zip", format!("compression method {m} is not supported"))),
        };
        files.insert(name, bytes);
    }
    Ok(files)
}

fn mime(name: &str) -> &'static str {
    let lower = name.to_ascii_lowercase();
    if lower.ends_with(".png") {
        "image/png"
    } else if lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
        "image/jpeg"
    } else if lower.ends_with(".webp") {
        "image/webp"
    } else if lower.ends_with(".svg") {
        "image/svg+xml"
    } else {
        "application/octet-stream"
    }
}

fn base64(bytes: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for c in bytes.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        s.push(T[(n >> 18) as usize & 63] as char);
        s.push(T[(n >> 12) as usize & 63] as char);
        s.push(if c.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
        s.push(if c.len() > 2 { T[n as usize & 63] as char } else { '=' });
    }
    s
}

/// Inline image assets that point into the archive as data URLs.
fn inline_images(anim: &mut Value, files: &BTreeMap<String, Vec<u8>>) {
    let Some(Value::Array(assets)) = anim.get_mut("assets") else { return };
    for a in assets {
        let Some(p) = a.get("p").and_then(Value::as_str).map(str::to_string) else { continue };
        if p.starts_with("data:") || a.get("layers").is_some() {
            continue;
        }
        let file = p.rsplit('/').next().unwrap_or(&p).to_string();
        let hit = ["i/", "images/"].iter().find_map(|dir| files.get(&format!("{dir}{file}")));
        if let Some(bytes) = hit {
            a["u"] = Value::String(String::new());
            a["p"] = Value::String(format!("data:{};base64,{}", mime(&file), base64(bytes)));
            a["e"] = Value::from(1);
        }
    }
}

/// A dotLottie v2 theme → slot overrides. Rules: `{id, type, value | keyframes}`.
fn theme_slots(theme: &Value) -> Map<String, Value> {
    let mut slots = Map::new();
    for rule in theme.get("rules").and_then(Value::as_array).into_iter().flatten() {
        let Some(id) = rule.get("id").and_then(Value::as_str) else { continue };
        let prop = if let Some(kfs) = rule.get("keyframes").and_then(Value::as_array) {
            let k: Vec<Value> = kfs
                .iter()
                .map(|kf| {
                    let mut o = Map::new();
                    o.insert("t".into(), kf.get("frame").cloned().unwrap_or(Value::from(0)));
                    o.insert("s".into(), match kf.get("value") {
                        Some(Value::Number(n)) => Value::Array(vec![Value::Number(n.clone())]),
                        Some(v) => v.clone(),
                        None => Value::Null,
                    });
                    if let Some(i) = kf.get("inTangent") {
                        o.insert("i".into(), i.clone());
                    }
                    if let Some(t) = kf.get("outTangent") {
                        o.insert("o".into(), t.clone());
                    }
                    if kf.get("hold").and_then(Value::as_bool) == Some(true) {
                        o.insert("h".into(), Value::from(1));
                    }
                    Value::Object(o)
                })
                .collect();
            serde_json::json!({"a": 1, "k": k})
        } else if let Some(v) = rule.get("value") {
            serde_json::json!({"a": 0, "k": v})
        } else {
            continue;
        };
        slots.insert(id.to_string(), serde_json::json!({ "p": prop }));
    }
    slots
}

/// Read a `.lottie` archive.
pub fn read(data: &[u8]) -> Result<DotLottie, Vec<Issue>> {
    let files = unzip(data)?;
    let manifest: Value = files
        .get("manifest.json")
        .and_then(|b| serde_json::from_slice(b).ok())
        .ok_or_else(|| issue("dotlottie", "manifest.json is missing or not JSON"))?;
    let mut out = DotLottie { manifest: manifest.clone(), ..Default::default() };
    let ids: Vec<String> = manifest
        .get("animations")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|a| a.get("id").and_then(Value::as_str).map(str::to_string))
        .collect();
    for id in ids {
        let bytes = ["a/", "animations/"].iter().find_map(|dir| files.get(&format!("{dir}{id}.json")));
        let Some(bytes) = bytes else {
            return Err(issue("dotlottie", format!("animation `{id}` is missing")));
        };
        let mut anim: Value = serde_json::from_slice(bytes).map_err(|e| issue("parse", format!("animation `{id}`: {e}")))?;
        inline_images(&mut anim, &files);
        out.animations.push((id, anim));
    }
    if out.animations.is_empty() {
        return Err(issue("dotlottie", "the manifest lists no animations"));
    }
    for (name, bytes) in &files {
        let stem = |prefix: &str| name.strip_prefix(prefix).and_then(|n| n.strip_suffix(".json")).map(str::to_string);
        if let Some(id) = stem("t/").or_else(|| stem("themes/")) {
            if let Ok(t) = serde_json::from_slice::<Value>(bytes) {
                out.themes.insert(id, theme_slots(&t));
            }
        }
        if let Some(id) = stem("s/").or_else(|| stem("states/")) {
            if let Ok(s) = serde_json::from_slice::<Value>(bytes) {
                out.state_machines.insert(id, s);
            }
        }
    }
    Ok(out)
}

/// Write a minimal dotLottie v2 (stored entries): one animation, optional
/// themes and state machines. The inverse of [`read`] for our own files.
pub fn write(id: &str, animation: &Value, themes: &[(String, Value)], state_machines: &[(String, Value)]) -> Vec<u8> {
    let mut entries: Vec<(String, Vec<u8>)> = Vec::new();
    let manifest = serde_json::json!({
        "version": "2",
        "generator": crate::dialect::GENERATOR,
        "animations": [{"id": id}],
        "themes": themes.iter().map(|(t, _)| serde_json::json!({"id": t})).collect::<Vec<_>>(),
        "stateMachines": state_machines.iter().map(|(s, _)| serde_json::json!({"id": s})).collect::<Vec<_>>(),
    });
    entries.push(("manifest.json".into(), serde_json::to_vec(&manifest).unwrap_or_default()));
    entries.push((format!("a/{id}.json"), serde_json::to_vec(animation).unwrap_or_default()));
    for (t, v) in themes {
        entries.push((format!("t/{t}.json"), serde_json::to_vec(v).unwrap_or_default()));
    }
    for (s, v) in state_machines {
        entries.push((format!("s/{s}.json"), serde_json::to_vec(v).unwrap_or_default()));
    }
    let mut out = Vec::new();
    let mut central = Vec::new();
    for (name, data) in &entries {
        let crc = crc32(data);
        let offset = out.len() as u32;
        let header = |sig: &[u8], central: bool| {
            let mut h = Vec::new();
            h.extend_from_slice(sig);
            if central {
                h.extend_from_slice(&20u16.to_le_bytes());
            }
            h.extend_from_slice(&20u16.to_le_bytes()); // version needed
            h.extend_from_slice(&0u16.to_le_bytes()); // flags
            h.extend_from_slice(&0u16.to_le_bytes()); // stored
            h.extend_from_slice(&0u32.to_le_bytes()); // time, date
            h.extend_from_slice(&crc.to_le_bytes());
            h.extend_from_slice(&(data.len() as u32).to_le_bytes());
            h.extend_from_slice(&(data.len() as u32).to_le_bytes());
            h.extend_from_slice(&(name.len() as u16).to_le_bytes());
            h.extend_from_slice(&0u16.to_le_bytes()); // extra
            if central {
                h.extend_from_slice(&0u16.to_le_bytes()); // comment
                h.extend_from_slice(&0u16.to_le_bytes()); // disk
                h.extend_from_slice(&0u16.to_le_bytes()); // internal attrs
                h.extend_from_slice(&0u32.to_le_bytes()); // external attrs
                h.extend_from_slice(&offset.to_le_bytes());
            }
            h.extend_from_slice(name.as_bytes());
            h
        };
        out.extend(header(b"PK\x03\x04", false));
        out.extend_from_slice(data);
        central.extend(header(b"PK\x01\x02", true));
    }
    let cd_offset = out.len() as u32;
    out.extend_from_slice(&central);
    out.extend_from_slice(b"PK\x05\x06");
    out.extend_from_slice(&[0, 0, 0, 0]);
    out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    out.extend_from_slice(&(central.len() as u32).to_le_bytes());
    out.extend_from_slice(&cd_offset.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &b in data {
        crc ^= b as u32;
        for _ in 0..8 {
            crc = if crc & 1 != 0 { (crc >> 1) ^ 0xEDB8_8320 } else { crc >> 1 };
        }
    }
    !crc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_and_reads_back() {
        let anim = serde_json::json!({"v": "5.12.0", "fr": 30, "ip": 0, "op": 30, "w": 10, "h": 10, "layers": []});
        let theme = serde_json::json!({"rules": [{"id": "$color.primary", "type": "Color", "value": [1, 0, 0, 1]}]});
        let zip = write("main", &anim, &[("dark".into(), theme)], &[]);
        let d = read(&zip).unwrap();
        assert_eq!(d.animations[0].0, "main");
        assert_eq!(d.animations[0].1, anim);
        assert_eq!(d.themes["dark"]["$color.primary"]["p"]["k"], serde_json::json!([1, 0, 0, 1]));
    }

    #[test]
    fn crc_matches_the_reference() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    }

    #[test]
    fn base64_pads() {
        assert_eq!(base64(b"ab"), "YWI=");
        assert_eq!(base64(b"abc"), "YWJj");
    }
}
