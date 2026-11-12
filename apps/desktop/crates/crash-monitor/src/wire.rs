//! sentry-types writes native debug images in the legacy `symbolic` shape
//! (`{"type":"symbolic","id":…,"name":…}`). GlitchTip only symbolicates
//! the current one (`elf`/`macho`/`pe` with `debug_id` and `code_file`) and
//! keeps a `symbolic` image as an unknown it never looks at. Both tunnel
//! transports pass their wire envelope through [`native_debug_images`]
//! (native crash reports AND the debug-images integration's panics).

use serde_json::Value;

/// The image type GlitchTip expects for an OS (`contexts.os.name`), else
/// for the OS this binary was built for.
fn image_type(os: Option<&str>) -> &'static str {
    let os = os.unwrap_or(if cfg!(target_os = "macos") {
        "macOS"
    } else if cfg!(windows) {
        "Windows"
    } else {
        "Linux"
    });
    match os {
        "macOS" | "Mac OS X" | "Darwin" | "iOS" => "macho",
        "Windows" | "Windows NT" => "pe",
        _ => "elf",
    }
}

/// Rewrite one event payload; `None` = nothing to change.
fn rewrite_event(payload: &[u8]) -> Option<Vec<u8>> {
    let mut event: Value = serde_json::from_slice(payload).ok()?;
    let os = event.pointer("/contexts/os/name").and_then(Value::as_str).map(str::to_owned);
    let kind = image_type(os.as_deref());
    let images = event.pointer_mut("/debug_meta/images")?.as_array_mut()?;
    let mut changed = false;
    for image in images.iter_mut() {
        let Some(image) = image.as_object_mut() else { continue };
        if image.get("type").and_then(Value::as_str) != Some("symbolic") {
            continue;
        }
        image.insert("type".into(), Value::from(kind));
        if let Some(id) = image.remove("id") {
            image.insert("debug_id".into(), id);
        }
        if let Some(name) = image.remove("name") {
            image.insert("code_file".into(), name);
        }
        changed = true;
    }
    changed.then(|| serde_json::to_vec(&event).ok()).flatten()
}

/// The envelope with every event item's `symbolic` images rewritten (other
/// items and an envelope without one pass through byte for byte).
pub fn native_debug_images(body: Vec<u8>) -> Vec<u8> {
    let needle = b"\"symbolic\"";
    if !body.windows(needle.len()).any(|w| w == needle) {
        return body;
    }
    rewrite(&body).unwrap_or(body)
}

fn line(bytes: &[u8], at: usize) -> (&[u8], usize) {
    match bytes[at..].iter().position(|b| *b == b'\n') {
        Some(end) => (&bytes[at..at + end], at + end + 1),
        None => (&bytes[at..], bytes.len()),
    }
}

fn rewrite(body: &[u8]) -> Option<Vec<u8>> {
    let (header, mut at) = line(body, 0);
    let mut out = Vec::with_capacity(body.len() + 64);
    out.extend_from_slice(header);
    out.push(b'\n');
    while at < body.len() {
        let (item_header, next) = line(body, at);
        if item_header.is_empty() {
            at = next;
            continue;
        }
        let mut item: serde_json::Map<String, Value> = serde_json::from_slice(item_header).ok()?;
        let (payload, after) = match item.get("length").and_then(Value::as_u64) {
            Some(length) => {
                let end = next.checked_add(length as usize).filter(|end| *end <= body.len())?;
                // The newline after a sized payload is optional.
                let after = if body.get(end) == Some(&b'\n') { end + 1 } else { end };
                (&body[next..end], after)
            }
            None => line(body, next),
        };
        let is_event = item.get("type").and_then(Value::as_str) == Some("event");
        match is_event.then(|| rewrite_event(payload)).flatten() {
            Some(new) => {
                item.insert("length".into(), Value::from(new.len()));
                out.extend_from_slice(&serde_json::to_vec(&item).ok()?);
                out.push(b'\n');
                out.extend_from_slice(&new);
            }
            None => {
                out.extend_from_slice(item_header);
                out.push(b'\n');
                out.extend_from_slice(payload);
            }
        }
        out.push(b'\n');
        at = after;
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sentry_types::protocol::v7 as proto;

    fn envelope_with(event: proto::Event<'static>) -> Vec<u8> {
        let mut envelope = proto::Envelope::new();
        envelope.add_item(event);
        let mut body = Vec::new();
        envelope.to_writer(&mut body).unwrap();
        body
    }

    fn event(os: &str) -> proto::Event<'static> {
        let mut contexts = proto::Map::new();
        contexts.insert("os".into(), proto::Context::Os(Box::new(proto::OsContext { name: Some(os.into()), ..Default::default() })));
        proto::Event {
            message: Some("boom".into()),
            contexts,
            debug_meta: std::borrow::Cow::Owned(proto::DebugMeta {
                images: vec![proto::DebugImage::Symbolic(proto::SymbolicDebugImage {
                    name: "/usr/bin/exp-desktop".into(),
                    arch: Some("x86_64".into()),
                    image_addr: proto::Addr(0x1000),
                    image_size: 4096,
                    image_vmaddr: proto::Addr(0),
                    id: "b0b1b2b3-b4b5-b6b7-b8b9-babbbcbdbebf".parse().unwrap(),
                    code_id: None,
                    debug_file: None,
                })],
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    fn event_payload(body: &[u8]) -> Value {
        let text = String::from_utf8(body.to_vec()).unwrap();
        let mut lines = text.lines();
        lines.next();
        while let Some(header) = lines.next() {
            let header: Value = serde_json::from_str(header).unwrap();
            let payload = lines.next().unwrap();
            if header["type"] == "event" {
                assert_eq!(header["length"].as_u64().unwrap() as usize, payload.len());
                return serde_json::from_str(payload).unwrap();
            }
        }
        panic!("no event item in {text}");
    }

    #[test]
    fn symbolic_images_become_the_native_type_of_the_os() {
        for (os, kind) in [("Linux", "elf"), ("macOS", "macho"), ("Windows", "pe")] {
            let body = native_debug_images(envelope_with(event(os)));
            let image = &event_payload(&body)["debug_meta"]["images"][0];
            assert_eq!(image["type"], kind, "{os}");
            assert_eq!(image["debug_id"], "b0b1b2b3-b4b5-b6b7-b8b9-babbbcbdbebf");
            assert_eq!(image["code_file"], "/usr/bin/exp-desktop");
            assert_eq!(image["image_addr"], "0x1000");
            assert!(image.get("id").is_none() && image.get("name").is_none());
        }
    }

    #[test]
    fn other_items_and_plain_envelopes_pass_through() {
        let plain = envelope_with(proto::Event { message: Some("hi".into()), ..Default::default() });
        assert_eq!(native_debug_images(plain.clone()), plain);

        let mut envelope = proto::Envelope::new();
        envelope.add_item(event("Linux"));
        envelope.add_item(proto::Attachment { buffer: b"line one\nline two".to_vec(), filename: "log.txt".into(), ..Default::default() });
        let mut body = Vec::new();
        envelope.to_writer(&mut body).unwrap();
        let out = native_debug_images(body);
        let text = String::from_utf8_lossy(&out);
        assert!(text.contains("line one\nline two"), "{text}");
        assert!(text.contains(r#""type":"elf""#), "{text}");
        // The attachment keeps its declared length (sentry-types itself
        // cannot read the rewritten event back: it knows no `elf` image).
        assert!(text.contains(r#""length":17"#), "{text}");
    }
}
