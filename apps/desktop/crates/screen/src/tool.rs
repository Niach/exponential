//! TOOL: `get_screenshots` (the fixture's `tool` block): the kept frames
//! between `from` and `to` matching `filter`, spread to `tool.maxFrames`,
//! each the stored picture (cut to its focused window and fitted to
//! `tool.maxEdge` when it was kept) sent as a PNG after its
//! `{at} · {app} — {title}` line.

use std::io::Cursor;
use std::sync::Mutex;

use chrono::{DateTime, SecondsFormat, TimeZone, Utc};
use serde_json::{json, Value};

use crate::store::Store;
use crate::{fixture, ring};

/// The `tools/list` array: the one tool.
pub(crate) fn tools() -> Value {
    let tool = fixture::tool();
    json!([{
        "name": tool["name"],
        "description": tool["description"],
        "inputSchema": tool["inputSchema"],
        "annotations": { "readOnlyHint": true },
    }])
}

fn text(text: impl Into<String>) -> Value {
    json!({ "content": [{ "type": "text", "text": text.into() }] })
}

fn refusal(text: impl Into<String>) -> Value {
    json!({ "content": [{ "type": "text", "text": text.into() }], "isError": true })
}

/// RFC 3339 UTC, seconds (`2026-10-28T10:00:00Z`).
pub(crate) fn rfc3339(ms: i64) -> String {
    Utc.timestamp_millis_opt(ms).single().unwrap_or_default().to_rfc3339_opts(SecondsFormat::Secs, true)
}

fn time(arguments: &Value, key: &str) -> Result<Option<i64>, String> {
    match arguments.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(raw)) if raw.trim().is_empty() => Ok(None),
        Some(Value::String(raw)) => DateTime::parse_from_rfc3339(raw.trim())
            .map(|at| Some(at.timestamp_millis()))
            .map_err(|_| format!("`{key}` is not an RFC 3339 time: {raw}")),
        Some(other) => Err(format!("`{key}` is not an RFC 3339 time: {other}")),
    }
}

/// One `tools/call`. `store` = `None` while screen memory is off.
pub(crate) fn call(store: Option<&Mutex<Store>>, name: &str, arguments: &Value, now: i64) -> Value {
    if name != fixture::tool_name() {
        return refusal(format!("Unknown tool: {name}"));
    }
    let Some(store) = store else { return text(fixture::copy("off")) };
    let (to, from) = match (time(arguments, "to"), time(arguments, "from")) {
        (Ok(to), Ok(from)) => {
            let to = to.unwrap_or(now);
            (to, from.unwrap_or(to - fixture::default_span_ms()))
        }
        (Err(reason), _) | (_, Err(reason)) => return refusal(reason),
    };
    let filter = arguments.get("filter").and_then(Value::as_str);
    let rows = match store.lock().unwrap().frames(from, to) {
        Ok(rows) => rows,
        Err(reason) => return refusal(reason),
    };
    let candidates: Vec<ring::Candidate<'_>> =
        rows.iter().map(|row| ring::Candidate { at: row.at, app: &row.app, title: &row.title }).collect();
    let picked = ring::select(&candidates, from, to, filter);
    let mut content = Vec::new();
    for index in picked {
        let row = &rows[index];
        let file = store.lock().unwrap().file(row);
        match std::fs::read(&file).map_err(|err| err.to_string()).and_then(|webp| picture(&webp)) {
            Ok(png) => {
                content.push(json!({ "type": "text", "text": format!("{} · {} — {}", rfc3339(row.at), row.app, row.title) }));
                content.push(json!({ "type": "image", "data": base64(&png), "mimeType": "image/png" }));
            }
            Err(reason) => log::warn!("[screen] frame {} unreadable: {reason}", row.path),
        }
    }
    if content.is_empty() {
        return text(fixture::copy("none").replace("{from}", &rfc3339(from)).replace("{to}", &rfc3339(to)));
    }
    json!({ "content": content })
}

/// The stored picture (already cut to the focused window and fitted to
/// `tool.maxEdge` when it was kept) as a PNG.
pub(crate) fn picture(webp: &[u8]) -> Result<Vec<u8>, String> {
    let decoded = webp::Decoder::new(webp).decode().ok_or("not a WebP image")?;
    let (width, height) = (decoded.width(), decoded.height());
    let image = if decoded.is_alpha() {
        image::DynamicImage::ImageRgba8(image::RgbaImage::from_raw(width, height, decoded.to_vec()).ok_or("short WebP buffer")?)
    } else {
        image::DynamicImage::ImageRgb8(image::RgbImage::from_raw(width, height, decoded.to_vec()).ok_or("short WebP buffer")?)
    };
    let mut png = Cursor::new(Vec::new());
    image.write_to(&mut png, image::ImageFormat::Png).map_err(|err| format!("PNG: {err}"))?;
    Ok(png.into_inner())
}

/// Standard base64 (RFC 4648, padded).
pub(crate) fn base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = chunk.iter().enumerate().fold(0u32, |acc, (i, b)| acc | (u32::from(*b) << (16 - 8 * i)));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(TABLE[((n >> (18 - 6 * i)) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixture::fixture;

    #[test]
    fn the_description_fits_the_per_tool_budget() {
        let tool = fixture()["tool"].clone();
        let max = tool["descriptionMaxChars"].as_u64().unwrap() as usize;
        assert!(tool["description"].as_str().unwrap().chars().count() <= max);
        // The whole listing too (what the agent loads).
        let listed = tools();
        assert_eq!(listed[0]["name"], "get_screenshots");
        assert_eq!(listed[0]["inputSchema"], tool["inputSchema"]);
    }

    #[test]
    fn off_and_none_read_the_fixtures_copy() {
        let off = call(None, "get_screenshots", &json!({}), 0);
        assert_eq!(off["content"][0]["text"], fixture()["copy"]["off"]);
        let dir = crate::store::tests::TempDir::new();
        let store = Mutex::new(Store::open(&dir.0).unwrap());
        let none = call(Some(&store), "get_screenshots", &json!({ "to": "2026-10-28T10:01:00Z" }), 0);
        assert_eq!(none["content"][0]["text"], "No screenshots between 2026-10-28T10:00:00Z and 2026-10-28T10:01:00Z.");
        let bad = call(Some(&store), "get_screenshots", &json!({ "from": "yesterday" }), 0);
        assert_eq!(bad["isError"], true);
    }

    #[test]
    fn base64_matches_the_standard_alphabet_and_padding() {
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }
}
