//! MIME types: capabilities.json `v1.mimeByExtension` (the bridge's table),
//! the photo extensions, the byte sniff, and accept lists → picker filters.

use std::sync::LazyLock;

use eengine::contract::CAPABILITIES;

use crate::FileFilter;

/// The photo library's file types (photos.list/get/pick).
pub(crate) const PHOTO_EXTENSIONS: [(&str, &str); 6] =
    [("gif", "image/gif"), ("heic", "image/heic"), ("jpeg", "image/jpeg"), ("jpg", "image/jpeg"), ("png", "image/png"), ("webp", "image/webp")];

/// `(extension, mime)`: the fixture's table, then the photo types it lacks.
static TABLE: LazyLock<Vec<(String, String)>> = LazyLock::new(|| {
    let mut table: Vec<(String, String)> = CAPABILITIES["v1"]["mimeByExtension"]
        .as_object()
        .map(|map| map.iter().filter_map(|(ext, mime)| Some((ext.clone(), mime.as_str()?.to_string()))).collect())
        .unwrap_or_default();
    for (ext, mime) in PHOTO_EXTENSIONS {
        if !table.iter().any(|(e, _)| e == ext) {
            table.push((ext.to_string(), mime.to_string()));
        }
    }
    table
});

fn extension(name: &str) -> Option<String> {
    let (_, ext) = name.rsplit_once('.')?;
    (!ext.is_empty()).then(|| ext.to_ascii_lowercase())
}

/// The table's type for the name's last `.ext` (case-insensitive).
pub(crate) fn by_extension(name: &str) -> Option<String> {
    let ext = extension(name)?;
    TABLE.iter().find(|(e, _)| *e == ext).map(|(_, m)| m.clone())
}

/// The photo type of a name, None for anything that is not a library photo.
pub(crate) fn photo_type(name: &str) -> Option<&'static str> {
    let ext = extension(name)?;
    PHOTO_EXTENSIONS.iter().find(|(e, _)| *e == ext).map(|(_, m)| *m)
}

/// The bridge's sniff: `%PDF-`, FF D8 FF, the PNG signature, `GIF8`.
pub(crate) fn sniff(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"%PDF-") {
        Some("application/pdf")
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("image/jpeg")
    } else if bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
        Some("image/png")
    } else if bytes.starts_with(b"GIF8") {
        Some("image/gif")
    } else {
        None
    }
}

/// An opened file's type: by name, else by bytes, else application/octet-stream.
pub(crate) fn of_file(name: &str, bytes: &[u8]) -> String {
    by_extension(name).or_else(|| sniff(bytes).map(str::to_string)).unwrap_or_else(|| "application/octet-stream".into())
}

/// files.open's `accept` (`image/*`, `text/csv`, `.csv`) as a picker filter.
pub(crate) fn filter(accept: &[String]) -> FileFilter {
    let mut filter = FileFilter::default();
    let mut unmapped = false;
    for entry in accept.iter().map(|a| a.trim()).filter(|a| !a.is_empty()) {
        if let Some(ext) = entry.strip_prefix('.') {
            push(&mut filter.extensions, ext.to_ascii_lowercase());
            continue;
        }
        let mime = entry.to_ascii_lowercase();
        let found: Vec<String> = match mime.strip_suffix("/*") {
            Some(family) => TABLE.iter().filter(|(_, m)| m.split('/').next() == Some(family)).map(|(e, _)| e.clone()).collect(),
            None => TABLE.iter().filter(|(_, m)| *m == mime).map(|(e, _)| e.clone()).collect(),
        };
        unmapped |= found.is_empty() && mime != "*/*";
        if mime == "*/*" {
            return FileFilter::default();
        }
        for ext in found {
            push(&mut filter.extensions, ext);
        }
        push(&mut filter.mime_types, mime);
    }
    if unmapped {
        // An extension platform cannot express this type: offer every file.
        filter.extensions.clear();
    }
    filter.extensions.sort();
    filter
}

fn push(list: &mut Vec<String>, value: String) {
    if !list.contains(&value) {
        list.push(value);
    }
}

/// photos.pick's filter: the library types.
pub(crate) fn photo_filter() -> FileFilter {
    let mut extensions: Vec<String> = PHOTO_EXTENSIONS.iter().map(|(e, _)| e.to_string()).collect();
    extensions.sort();
    let mut mime_types: Vec<String> = PHOTO_EXTENSIONS.iter().map(|(_, m)| m.to_string()).collect();
    mime_types.dedup();
    mime_types.sort();
    mime_types.dedup();
    FileFilter { mime_types, extensions }
}
