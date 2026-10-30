//! TOOL: `get_screenshots` (the fixture's `tool` block): the kept frames
//! between `from` (never before the retention cutoff) and `to` of a
//! display that is a work display now, matching `filter`, spread to
//! `tool.maxFrames`, each the stored WebP (cut to its focused window and
//! fitted to `tool.maxEdge` when it was kept) sent as it is after its
//! `{at} · {app} — {title}` line. SIZE: an image over `tool.maxImageBytes`
//! of base64 is re-encoded smaller; while the images exceed
//! `tool.maxTotalBytes` together the middle one goes (`copy.trimmed`).

use std::sync::Mutex;

use chrono::{DateTime, SecondsFormat, TimeZone, Utc};
use serde_json::{json, Value};

use crate::store::Store;
use crate::{fixture, ring, shrink};

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

/// One `tools/call`. `store` = `None` while screen memory is off; `work` =
/// the work displays now (1-based): a frame of another display stays out.
pub(crate) fn call(store: Option<&Mutex<Store>>, name: &str, arguments: &Value, now: i64, work: &[u32]) -> Value {
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
    // RETENTION holds on read too: the sweep may not have run yet.
    let from = from.max(ring::cutoff(now));
    let filter = arguments.get("filter").and_then(Value::as_str);
    let rows = match store.lock().unwrap().frames(from, to) {
        Ok(rows) => rows.into_iter().filter(|row| work.contains(&row.display)).collect::<Vec<_>>(),
        Err(reason) => return refusal(reason),
    };
    let candidates: Vec<ring::Candidate<'_>> =
        rows.iter().map(|row| ring::Candidate { at: row.at, app: &row.app, title: &row.title }).collect();
    let picked = ring::select(&candidates, from, to, filter);
    let mut shots: Vec<(String, String)> = Vec::new();
    for index in picked {
        let row = &rows[index];
        let file = store.lock().unwrap().file(row);
        match std::fs::read(&file).map_err(|err| err.to_string()).and_then(fitted) {
            Ok(webp) => shots.push((format!("{} · {} — {}", rfc3339(row.at), row.app, row.title), base64(&webp))),
            Err(reason) => log::warn!("[screen] frame {} unreadable: {reason}", row.path),
        }
    }
    if shots.is_empty() {
        return text(fixture::copy("none").replace("{from}", &rfc3339(from)).replace("{to}", &rfc3339(to)));
    }
    let left_out = trim(&mut shots, fixture::max_total_bytes());
    let mut content = Vec::new();
    for (line, data) in shots {
        content.push(json!({ "type": "text", "text": line }));
        content.push(json!({ "type": "image", "data": data, "mimeType": "image/webp" }));
    }
    if left_out > 0 {
        content.push(json!({ "type": "text", "text": fixture::copy("trimmed").replace("{count}", &left_out.to_string()) }));
    }
    json!({ "content": content })
}

/// SIZE: while the images' base64 together exceeds `total`, the middle
/// one goes (the first and the last frame of the range stay longest).
/// Returns how many went.
pub(crate) fn trim(shots: &mut Vec<(String, String)>, total: usize) -> usize {
    let mut left_out = 0;
    while shots.len() > 1 && shots.iter().map(|(_, data)| data.len()).sum::<usize>() > total {
        shots.remove(shots.len() / 2);
        left_out += 1;
    }
    left_out
}

/// Base64 characters for `bytes` bytes.
fn base64_len(bytes: usize) -> usize {
    bytes.div_ceil(3) * 4
}

/// SIZE: the stored WebP as it goes out: as it is when its base64 fits
/// `tool.maxImageBytes`, else re-encoded at a lower quality, then smaller
/// (3/4 per step), until it does.
pub(crate) fn fitted(webp: Vec<u8>) -> Result<Vec<u8>, String> {
    let limit = fixture::max_image_bytes();
    if base64_len(webp.len()) <= limit {
        return Ok(webp);
    }
    let mut picture = decode(&webp)?;
    let mut qualities: &[f32] = &[60., 40.];
    loop {
        for &quality in qualities {
            let smaller = shrink::encode_at(&picture, quality);
            if !smaller.is_empty() && base64_len(smaller.len()) <= limit {
                return Ok(smaller);
            }
        }
        if picture.width <= 64 || picture.height <= 64 {
            return Err("too large to send".to_string());
        }
        let (width, height) = (picture.width * 3 / 4, picture.height * 3 / 4);
        picture = shrink::resize(picture, width, height);
        qualities = &[40.];
    }
}

/// A WebP as an RGBA picture.
pub(crate) fn decode(webp: &[u8]) -> Result<shrink::Picture, String> {
    let decoded = webp::Decoder::new(webp).decode().ok_or("not a WebP image")?;
    let (width, height) = (decoded.width(), decoded.height());
    let rgba = if decoded.is_alpha() {
        decoded.to_vec()
    } else {
        decoded.chunks_exact(3).flat_map(|pixel| [pixel[0], pixel[1], pixel[2], 255]).collect()
    };
    if rgba.len() != (width * height * 4) as usize {
        return Err("short WebP buffer".to_string());
    }
    Ok(shrink::Picture { width, height, rgba })
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
    use crate::store::{relative_path, Row};

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
        let off = call(None, "get_screenshots", &json!({}), 0, &[1]);
        assert_eq!(off["content"][0]["text"], fixture()["copy"]["off"]);
        let dir = crate::store::tests::TempDir::new();
        let store = Mutex::new(Store::open(&dir.0).unwrap());
        let now = DateTime::parse_from_rfc3339("2026-10-28T10:01:00Z").unwrap().timestamp_millis();
        let none = call(Some(&store), "get_screenshots", &json!({ "to": "2026-10-28T10:01:00Z" }), now, &[1]);
        assert_eq!(none["content"][0]["text"], "No screenshots between 2026-10-28T10:00:00Z and 2026-10-28T10:01:00Z.");
        let bad = call(Some(&store), "get_screenshots", &json!({ "from": "yesterday" }), now, &[1]);
        assert_eq!(bad["isError"], true);
    }

    fn row(at: i64, display: u32) -> Row {
        Row {
            at,
            display,
            app: "Zed".into(),
            title: format!("frame {at}"),
            hash: "0f0f0f0f0f0f0f0f".into(),
            path: relative_path(at, display),
            width: 64,
            height: 64,
            focus: None,
        }
    }

    fn small_webp() -> Vec<u8> {
        shrink::encode(&shrink::Picture { width: 64, height: 64, rgba: [90, 120, 200, 255].repeat(64 * 64) })
    }

    /// RETENTION on read: a frame older than the cutoff that the sweep has
    /// not reached yet never goes out, whatever `from` asks for; a frame of
    /// a display that is not a work display now stays out too.
    #[test]
    fn old_frames_and_non_work_displays_never_go_out() {
        let dir = crate::store::tests::TempDir::new();
        let store = Mutex::new(Store::open(&dir.0).unwrap());
        let now = 1_800_000_000_000;
        let webp = small_webp();
        for (at, display) in [(now - fixture::retention_ms() - 1, 1), (now - 5_000, 1), (now - 4_000, 2)] {
            store.lock().unwrap().insert(&row(at, display), &webp).unwrap();
        }
        let from = rfc3339(now - 3_600_000);
        let reply = call(Some(&store), "get_screenshots", &json!({ "from": from }), now, &[1]);
        let content = reply["content"].as_array().unwrap();
        let lines: Vec<&str> = content.iter().filter(|item| item["type"] == "text").map(|item| item["text"].as_str().unwrap()).collect();
        assert_eq!(lines, [format!("{} · Zed — frame {}", rfc3339(now - 5_000), now - 5_000)]);
        // The stored WebP itself, not a PNG re-encode.
        let image = &content[1];
        assert_eq!(image["mimeType"], "image/webp");
        assert_eq!(image["data"], base64(&webp));
        // No work display at all (a picked display gone): nothing.
        let none = call(Some(&store), "get_screenshots", &json!({ "from": from }), now, &[]);
        assert!(none["content"][0]["text"].as_str().unwrap().starts_with("No screenshots between "));
    }

    /// SIZE: dense frames (noise at the stored size) go out under
    /// `maxImageBytes` each and `maxTotalBytes` together, the middle ones
    /// dropped and counted.
    #[test]
    fn dense_frames_fit_the_payload_caps() {
        let (width, height) = (1568u32, 882u32);
        let mut seed = 0x1234_5678_u64;
        let mut noise = || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed as u8
        };
        let rgba: Vec<u8> = (0..width * height).flat_map(|_| [noise(), noise(), noise(), 255]).collect();
        let dense = shrink::encode(&shrink::Picture { width, height, rgba });
        assert!(base64_len(dense.len()) > fixture::max_image_bytes(), "the stored frame is over the cap: {} bytes", dense.len());
        let dir = crate::store::tests::TempDir::new();
        let store = Mutex::new(Store::open(&dir.0).unwrap());
        let now = 1_800_000_000_000;
        for second in 0..4 {
            store.lock().unwrap().insert(&row(now - 40_000 + second * 10_000, 1), &dense).unwrap();
        }
        let reply = call(Some(&store), "get_screenshots", &json!({}), now, &[1]);
        let content = reply["content"].as_array().unwrap();
        let images: Vec<&str> = content.iter().filter(|item| item["type"] == "image").map(|item| item["data"].as_str().unwrap()).collect();
        assert!(!images.is_empty());
        for data in &images {
            assert!(data.len() <= fixture::max_image_bytes(), "{} base64 chars", data.len());
        }
        let total: usize = images.iter().map(|data| data.len()).sum();
        assert!(total <= fixture::max_total_bytes(), "{total} base64 chars together");
        let left_out = 4 - images.len();
        if left_out > 0 {
            let last = content.last().unwrap()["text"].as_str().unwrap().to_string();
            assert_eq!(last, format!("{left_out} more screenshots in this range were left out to keep the reply small."));
        }
    }

    #[test]
    fn the_trim_drops_the_middle_first() {
        let shot = |name: &str, size: usize| (name.to_string(), "x".repeat(size));
        let mut shots = vec![shot("a", 10), shot("b", 10), shot("c", 10), shot("d", 10)];
        assert_eq!(trim(&mut shots, 25), 2);
        let names: Vec<&str> = shots.iter().map(|(name, _)| name.as_str()).collect();
        assert_eq!(names, ["a", "d"]);
    }

    #[test]
    fn base64_matches_the_standard_alphabet_and_padding() {
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }
}
