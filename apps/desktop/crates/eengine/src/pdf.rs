//! VAPP-265 (E5): `print.pdf`, the Engine's own tool. ONE A4 layout (headings,
//! text, app images, page breaks) both twins write byte for byte, with the
//! two standard Helvetica fonts (WinAnsi) and the images passed through as
//! they are (JPEG as DCTDecode, a PNG's IDATs as FlateDecode). Locked by
//! `packages/eengine/fixtures/capabilities.json` (`pdf`, `pdfCases`; sha256 + size).

use std::sync::LazyLock;

use serde_json::{json, Value};

use crate::contract::CAPABILITIES;
use crate::tools::BridgeContext;

struct Metrics {
    width: f64,
    height: f64,
    margin: f64,
    /// Helvetica, Helvetica-Bold: per WinAnsi code, 1/1000 em.
    widths: [Vec<u64>; 2],
    /// Code point → WinAnsi code (0x80-0x9F).
    win_ansi: Vec<(u32, u8)>,
}

static METRICS: LazyLock<Metrics> = LazyLock::new(|| {
    let pdf = &CAPABILITIES["pdf"];
    let widths = |font: &str| -> Vec<u64> { pdf["widths"][font].as_array().expect("pdf widths").iter().map(|w| w.as_u64().expect("width")).collect() };
    let win_ansi = pdf["winAnsi"].as_object().expect("pdf winAnsi").iter().map(|(k, v)| (k.parse().expect("code point"), v.as_u64().expect("code") as u8)).collect();
    Metrics {
        width: pdf["page"][0].as_f64().expect("page width"),
        height: pdf["page"][1].as_f64().expect("page height"),
        margin: pdf["margin"].as_f64().expect("margin"),
        widths: [widths("Helvetica"), widths("Helvetica-Bold")],
        win_ansi,
    }
});

/// A number as the PDF prints it: rounded to 2 decimals (half up), shortest form.
pub fn num(x: f64) -> String {
    let n = (x * 100.0).round() as i64;
    let (sign, n) = if n < 0 { ("-", -n) } else { ("", n) };
    let (int, frac) = (n / 100, n % 100);
    match frac {
        0 => format!("{sign}{int}"),
        f if f % 10 == 0 => format!("{sign}{int}.{}", f / 10),
        f => format!("{sign}{int}.{f:02}"),
    }
}

fn is_control(c: char) -> bool {
    matches!(c as u32, 0x00..=0x1F | 0x7F..=0x9F)
}

/// A text as WinAnsi bytes: a tab is a space, other controls drop, what cannot be encoded is `?`.
pub fn win_ansi(text: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len());
    for c in text.chars() {
        let cp = c as u32;
        if c == '\t' {
            out.push(b' ');
        } else if is_control(c) {
            continue;
        } else if (0x20..=0x7E).contains(&cp) || (0xA0..=0xFF).contains(&cp) {
            out.push(cp as u8);
        } else if let Some((_, code)) = METRICS.win_ansi.iter().find(|(p, _)| *p == cp) {
            out.push(*code);
        } else {
            out.push(b'?');
        }
    }
    out
}

/// The line breaking of a text at font `font` (0 = F1, 1 = F2) and size `size`: WinAnsi lines.
pub fn lines(text: &str, font: usize, size: u64) -> Vec<Vec<u8>> {
    let widths = &METRICS.widths[font];
    let content = (METRICS.width - 2.0 * METRICS.margin) as u64;
    let fits = |bytes: &[u8]| bytes.iter().map(|b| widths[*b as usize]).sum::<u64>() * size <= content * 1000;
    let mut out = Vec::new();
    for paragraph in text.split('\n') {
        let encoded = win_ansi(paragraph);
        let mut line: Vec<u8> = Vec::new();
        let mut any = false;
        for word in encoded.split(|b| *b == b' ').filter(|w| !w.is_empty()) {
            let mut word = word.to_vec();
            loop {
                let candidate = if line.is_empty() { word.clone() } else { [line.as_slice(), b" ", word.as_slice()].concat() };
                if fits(&candidate) {
                    line = candidate;
                    break;
                }
                if !line.is_empty() {
                    out.push(std::mem::take(&mut line));
                    any = true;
                    continue;
                }
                // A word wider than the line: its longest fitting prefix (one character at least).
                let mut cut = 1;
                while cut < word.len() && fits(&word[..cut + 1]) {
                    cut += 1;
                }
                out.push(word[..cut].to_vec());
                any = true;
                word = word[cut..].to_vec();
            }
        }
        if !line.is_empty() || !any {
            out.push(line);
        }
    }
    out
}

/// A PDF literal string's body: `(`, `)` and `\` escaped, bytes ≥ 0x80 as 3-digit octal.
fn pdf_string(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len());
    for &b in bytes {
        match b {
            b'(' | b')' | b'\\' => {
                out.push('\\');
                out.push(b as char);
            }
            0x80..=0xFF => out.push_str(&format!("\\{b:03o}")),
            _ => out.push(b as char),
        }
    }
    out
}

/// An image the PDF embeds.
struct Image {
    width: u32,
    height: u32,
    /// The XObject's dictionary entries before `/Length`.
    dict: String,
    data: Vec<u8>,
}

fn be16(b: &[u8], i: usize) -> Option<u32> {
    Some(u32::from(*b.get(i)?) << 8 | u32::from(*b.get(i + 1)?))
}

fn be32(b: &[u8], i: usize) -> Option<u32> {
    Some(be16(b, i)? << 16 | be16(b, i + 2)?)
}

/// A JPEG: its first SOF0-SOF3 gives width, height and 1/3/4 components.
fn jpeg(bytes: &[u8]) -> Option<Image> {
    if !bytes.starts_with(&[0xFF, 0xD8]) {
        return None;
    }
    let mut i = 2;
    loop {
        if *bytes.get(i)? != 0xFF {
            return None;
        }
        while *bytes.get(i)? == 0xFF {
            i += 1;
        }
        let marker = bytes[i];
        i += 1;
        match marker {
            0xD0..=0xD7 | 0x01 => continue,
            0xD9 | 0xDA => return None,
            _ => {}
        }
        let len = be16(bytes, i)? as usize;
        if (0xC0..=0xC3).contains(&marker) {
            let height = be16(bytes, i + 3)?;
            let width = be16(bytes, i + 5)?;
            let space = match *bytes.get(i + 7)? {
                1 => "/DeviceGray",
                3 => "/DeviceRGB",
                4 => "/DeviceCMYK",
                _ => return None,
            };
            if width == 0 || height == 0 {
                return None;
            }
            let dict = format!("/Type /XObject /Subtype /Image /Width {width} /Height {height} /ColorSpace {space} /BitsPerComponent 8 /Filter /DCTDecode");
            return Some(Image { width, height, dict, data: bytes.to_vec() });
        }
        i += len;
    }
}

/// A PNG: 8-bit, colour type 0 or 2, not interlaced; its IDATs concatenated.
fn png(bytes: &[u8]) -> Option<Image> {
    if !bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
        return None;
    }
    let mut i = 8;
    let mut header: Option<(u32, u32, u32)> = None;
    let mut data = Vec::new();
    let mut idat = false;
    while i < bytes.len() {
        let len = be32(bytes, i)? as usize;
        let kind = bytes.get(i + 4..i + 8)?;
        let body = bytes.get(i + 8..(i + 8).checked_add(len)?)?;
        bytes.get(i + 8 + len..i + 12 + len)?;
        match kind {
            b"IHDR" if header.is_none() && len == 13 => {
                let (width, height) = (be32(body, 0)?, be32(body, 4)?);
                let (depth, colour, interlace) = (body[8], body[9], body[12]);
                let colours = match colour {
                    0 => 1,
                    2 => 3,
                    _ => return None,
                };
                if depth != 8 || interlace != 0 || width == 0 || height == 0 {
                    return None;
                }
                header = Some((width, height, colours));
            }
            _ if header.is_none() => return None,
            b"IDAT" => {
                data.extend_from_slice(body);
                idat = true;
            }
            b"IEND" => break,
            _ => {}
        }
        i += 12 + len;
    }
    let (width, height, colours) = header?;
    if !idat {
        return None;
    }
    let space = if colours == 1 { "/DeviceGray" } else { "/DeviceRGB" };
    let dict = format!(
        "/Type /XObject /Subtype /Image /Width {width} /Height {height} /ColorSpace {space} /BitsPerComponent 8 /Filter /FlateDecode /DecodeParms << /Predictor 15 /Colors {colours} /BitsPerComponent 8 /Columns {width} >>"
    );
    Some(Image { width, height, dict, data })
}

#[derive(Default)]
struct Page {
    ops: Vec<String>,
    /// The image numbers it shows (1-based), in first-use order.
    images: Vec<usize>,
    content: bool,
}

struct Layout {
    pages: Vec<Page>,
    y: f64,
}

impl Layout {
    fn top() -> f64 {
        METRICS.height - METRICS.margin
    }

    fn page(&mut self) -> &mut Page {
        self.pages.last_mut().expect("a page")
    }

    fn has_content(&self) -> bool {
        self.pages.last().is_some_and(|p| p.content)
    }

    fn new_page(&mut self) {
        self.pages.push(Page::default());
        self.y = Self::top();
    }

    fn line(&mut self, text: &[u8], font: usize, size: u64, leading: f64) {
        if self.has_content() && self.y - leading < METRICS.margin {
            self.new_page();
        }
        let y = self.y - size as f64;
        let page = self.page();
        if !text.is_empty() {
            page.ops.push(format!("BT /F{} {size} Tf {} {} Td ({}) Tj ET", font + 1, num(METRICS.margin), num(y), pdf_string(text)));
        }
        page.content = true;
        self.y -= leading;
    }

    fn text(&mut self, text: &str, font: usize, size: u64, leading: f64) {
        for line in lines(text, font, size) {
            self.line(&line, font, size, leading);
        }
    }
}

/// The PDF of `title` and its `blocks` (validated `print.pdf` arguments),
/// images read through `image` (an app file's bytes; `None` = missing):
/// the file's bytes and its page count, or the error reason.
pub fn pdf_document(title: &str, blocks: &[Value], image: &dyn Fn(&str) -> Option<Vec<u8>>) -> Result<(Vec<u8>, usize), String> {
    let m = &*METRICS;
    let content_width = m.width - 2.0 * m.margin;
    let content_height = m.height - 2.0 * m.margin;
    let mut layout = Layout { pages: vec![Page::default()], y: Layout::top() };
    let mut images: Vec<(String, Image)> = Vec::new();
    for block in blocks {
        if let Some(heading) = block.get("heading").and_then(Value::as_str) {
            if layout.has_content() {
                layout.y -= 12.0;
            }
            layout.text(heading, 1, 18, 24.0);
            layout.y -= 4.0;
        } else if let Some(text) = block.get("text").and_then(Value::as_str) {
            layout.text(text, 0, 11, 15.0);
            layout.y -= 6.0;
        } else if let Some(id) = block.get("image").and_then(Value::as_str) {
            let k = match images.iter().position(|(file, _)| file == id) {
                Some(at) => at + 1,
                None => {
                    let bytes = image(id).ok_or_else(|| format!("notFound: {id}"))?;
                    let img = jpeg(&bytes).or_else(|| png(&bytes)).ok_or_else(|| format!("unsupportedImage: {id}"))?;
                    images.push((id.to_string(), img));
                    images.len()
                }
            };
            let img = &images[k - 1].1;
            let (w, h) = (f64::from(img.width), f64::from(img.height));
            let scale = (content_width / w).min(content_height / h).min(1.0);
            let (sw, sh) = (w * scale, h * scale);
            if layout.has_content() && layout.y - sh < m.margin {
                layout.new_page();
            }
            let y = layout.y - sh;
            let page = layout.page();
            page.ops.push(format!("q {} 0 0 {} {} {} cm /Im{k} Do Q", num(sw), num(sh), num(m.margin), num(y)));
            if !page.images.contains(&k) {
                page.images.push(k);
            }
            page.content = true;
            layout.y -= sh;
            if let Some(caption) = block.get("caption").and_then(Value::as_str) {
                layout.y -= 4.0;
                layout.text(caption, 0, 9, 12.0);
            }
            layout.y -= 6.0;
        } else if block.get("pageBreak").is_some() && layout.has_content() {
            layout.new_page();
        }
    }
    let mut pages = layout.pages;
    if pages.len() > 1 && pages.last().is_some_and(|p| !p.content) {
        pages.pop();
    }
    Ok((write_file(title, &pages, &images), pages.len()))
}

fn write_file(title: &str, pages: &[Page], images: &[(String, Image)]) -> Vec<u8> {
    let m = &*METRICS;
    let first_page = 6 + images.len();
    let page_obj = |i: usize| first_page + 2 * i;
    let kids: Vec<String> = (0..pages.len()).map(|i| format!("{} 0 R", page_obj(i))).collect();
    let title_hex: String = title.encode_utf16().map(|u| format!("{u:04X}")).collect();
    let font = |name: &str| format!("<< /Type /Font /Subtype /Type1 /BaseFont /{name} /Encoding /WinAnsiEncoding >>").into_bytes();
    let stream = |dict: &str, data: &[u8]| {
        let sep = if dict.is_empty() { "" } else { " " };
        let mut body = format!("<< {dict}{sep}/Length {} >>\nstream\n", data.len()).into_bytes();
        body.extend_from_slice(data);
        body.extend_from_slice(b"\nendstream");
        body
    };
    let mut objects: Vec<Vec<u8>> = vec![
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        format!("<< /Type /Pages /Kids [{}] /Count {} >>", kids.join(" "), pages.len()).into_bytes(),
        font("Helvetica"),
        font("Helvetica-Bold"),
        format!("<< /Title <FEFF{title_hex}> /Producer (Exponential Engine) >>").into_bytes(),
    ];
    for (_, img) in images {
        objects.push(stream(&img.dict, &img.data));
    }
    for (i, page) in pages.iter().enumerate() {
        let mut numbers = page.images.clone();
        numbers.sort_unstable();
        let xobjects = if numbers.is_empty() {
            String::new()
        } else {
            let refs: Vec<String> = numbers.iter().map(|k| format!("/Im{k} {} 0 R", 5 + k)).collect();
            format!(" /XObject << {} >>", refs.join(" "))
        };
        objects.push(
            format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {} {}] /Resources << /Font << /F1 3 0 R /F2 4 0 R >>{xobjects} >> /Contents {} 0 R >>",
                num(m.width),
                num(m.height),
                page_obj(i) + 1
            )
            .into_bytes(),
        );
        objects.push(stream("", page.ops.join("\n").as_bytes()));
    }
    let mut out: Vec<u8> = b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n".to_vec();
    let mut offsets = Vec::with_capacity(objects.len());
    for (i, body) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
        out.extend_from_slice(body);
        out.extend_from_slice(b"\nendobj\n");
    }
    let xref = out.len();
    let size = objects.len() + 1;
    out.extend_from_slice(format!("xref\n0 {size}\n0000000000 65535 f \n").as_bytes());
    for offset in offsets {
        out.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(format!("trailer\n<< /Size {size} /Root 1 0 R /Info 5 0 R >>\nstartxref\n{xref}\n%%EOF\n").as_bytes());
    out
}

/// The default file name of a title: `/\:*?"<>|` and controls replaced by `-`, + `.pdf`.
pub fn default_pdf_name(title: &str) -> String {
    let base: String = title.chars().map(|c| if is_control(c) || "/\\:*?\"<>|".contains(c) { '-' } else { c }).collect();
    format!("{base}.pdf")
}

/// `print.pdf` (validated arguments): lay out, store as an app file, answer `{file, pages}`.
pub fn print_pdf(cx: &BridgeContext, args: &Value) -> Result<Value, String> {
    let title = args.get("title").and_then(Value::as_str).unwrap_or_default();
    let blocks = args.get("blocks").and_then(Value::as_array).map(Vec::as_slice).unwrap_or_default();
    let read = |id: &str| cx.blobs.get(cx.app, id).ok().flatten();
    let (bytes, pages) = pdf_document(title, blocks, &read)?;
    let name = args.get("name").and_then(Value::as_str).map_or_else(|| default_pdf_name(title), str::to_string);
    let id = cx.blobs.put(cx.app, &name, "application/pdf", &bytes).map_err(|f| f.message)?;
    Ok(json!({"file": {"name": name, "mimeType": "application/pdf", "size": bytes.len(), "fileId": id}, "pages": pages}))
}
