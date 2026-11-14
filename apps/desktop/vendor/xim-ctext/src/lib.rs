//! Currently only support utf8 mode

#![no_std]

extern crate alloc;

#[cfg(feature = "std")]
extern crate std;

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

#[cfg(feature = "std")]
use std::io::{self, Write};

const UTF8_START: &[u8] = &[0x1B, 0x25, 0x47];
const UTF8_END: &[u8] = &[0x1B, 0x25, 0x40];

/// Wrapper for reduce allocation
#[derive(Clone, Copy)]
#[repr(transparent)]
pub struct CText<'s> {
    utf8: &'s str,
}

impl<'s> fmt::Debug for CText<'s> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.utf8)
    }
}

impl<'s> fmt::Display for CText<'s> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.utf8)
    }
}

impl<'s> CText<'s> {
    pub const fn new(utf8: &'s str) -> Self {
        Self { utf8 }
    }

    pub const fn len(self) -> usize {
        self.utf8.len() + UTF8_START.len() + UTF8_END.len()
    }

    #[cfg(feature = "std")]
    pub fn write(self, mut out: impl Write) -> io::Result<usize> {
        let mut writed = 0;
        writed += out.write(UTF8_START)?;
        writed += out.write(self.utf8.as_bytes())?;
        writed += out.write(UTF8_END)?;
        Ok(writed)
    }
}

/// Encoding utf8 to COMPOUND_TEXT with utf8 escape
pub fn utf8_to_compound_text(text: &str) -> Vec<u8> {
    let mut ret = Vec::with_capacity(text.len() + 6);
    ret.extend_from_slice(UTF8_START);
    ret.extend_from_slice(text.as_bytes());
    ret.extend_from_slice(UTF8_END);
    ret
}

#[derive(Debug, Clone)]
pub enum DecodeError {
    InvalidEncoding,
    UnsupportedEncoding,
    Utf8Error(alloc::string::FromUtf8Error),
}

impl From<alloc::string::FromUtf8Error> for DecodeError {
    fn from(err: alloc::string::FromUtf8Error) -> Self {
        DecodeError::Utf8Error(err)
    }
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidEncoding => write!(f, "Invalid compound text"),
            Self::UnsupportedEncoding => write!(f, "This encoding is not supported yet"),
            Self::Utf8Error(e) => write!(f, "Not a valid utf8 {}", e),
        }
    }
}

// Exponential patch (#ERR-18): a COMPOUND_TEXT decoder that never fails.
//
// Upstream decoded escape-free text as UTF-8 and returned an error for
// anything else but the UTF-8 and JIS X 0208 escapes; zed-xim `expect`s the
// result, so an input method committing "ü" as COMPOUND_TEXT (Latin-1 0xFC,
// the X11 default for GR) panicked the app. COMPOUND_TEXT starts in ASCII (GL)
// + ISO 8859-1 right half (GR); escapes designate other sets. Text without an
// escape that is valid UTF-8 still decodes as UTF-8 (what some IMs send).

#[derive(Clone, Copy, PartialEq)]
enum Set {
    Ascii,
    Latin1,
    Other(&'static encoding_rs::Encoding),
}

/// `ESC - F`: a 96-character set designated to GR.
fn gr96(final_byte: u8) -> Set {
    match final_byte {
        0x41 => Set::Latin1,
        0x42 => Set::Other(encoding_rs::ISO_8859_2),
        0x43 => Set::Other(encoding_rs::ISO_8859_3),
        0x44 => Set::Other(encoding_rs::ISO_8859_4),
        0x46 => Set::Other(encoding_rs::ISO_8859_7),
        0x47 => Set::Other(encoding_rs::ISO_8859_8),
        0x4C => Set::Other(encoding_rs::ISO_8859_5),
        0x4D => Set::Other(encoding_rs::WINDOWS_1254),
        _ => Set::Latin1,
    }
}

/// `ESC $ ( F` / `ESC $ ) F`: a 94^2 set, decoded in its EUC form.
fn multi(final_byte: u8) -> Option<Set> {
    match final_byte {
        0x41 => Some(Set::Other(encoding_rs::GBK)),
        0x42 => Some(Set::Other(encoding_rs::EUC_JP)),
        0x43 => Some(Set::Other(encoding_rs::EUC_KR)),
        _ => None,
    }
}

struct Out {
    text: String,
    pending: Vec<u8>,
    encoding: Option<&'static encoding_rs::Encoding>,
}

impl Out {
    fn flush(&mut self) {
        if let Some(encoding) = self.encoding.take() {
            self.text
                .push_str(&encoding.decode_without_bom_handling(&self.pending).0);
        }
        self.pending.clear();
    }

    fn push_char(&mut self, c: char) {
        self.flush();
        self.text.push(c);
    }

    fn push_encoded(&mut self, encoding: &'static encoding_rs::Encoding, byte: u8) {
        if self.encoding != Some(encoding) {
            self.flush();
            self.encoding = Some(encoding);
        }
        self.pending.push(byte);
    }
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|window| window == needle)
}

pub fn compound_text_to_utf8(bytes: &[u8]) -> Result<String, DecodeError> {
    if !bytes.contains(&0x1B) {
        if let Ok(text) = core::str::from_utf8(bytes) {
            return Ok(String::from(text));
        }
    }
    let mut out = Out { text: String::new(), pending: Vec::new(), encoding: None };
    let (mut gl, mut gr) = (Set::Ascii, Set::Latin1);
    let mut i = 0;
    while i < bytes.len() {
        let byte = bytes[i];
        i += 1;
        match byte {
            0x1B => {
                let start = i;
                while i < bytes.len() && (0x20..=0x2F).contains(&bytes[i]) {
                    i += 1;
                }
                let intermediates = &bytes[start..i];
                let Some(&final_byte) = bytes.get(i) else { break };
                i += 1;
                match intermediates {
                    // UTF-8 until `ESC % @` (or the end).
                    [0x25] if final_byte == 0x47 => {
                        out.flush();
                        let rest = &bytes[i..];
                        let end = find(rest, UTF8_END).unwrap_or(rest.len());
                        out.text.push_str(&String::from_utf8_lossy(&rest[..end]));
                        i += (end + UTF8_END.len()).min(rest.len());
                    }
                    [0x28] => gl = Set::Ascii,
                    [0x2D] => gr = gr96(final_byte),
                    [0x24, 0x28] => gl = multi(final_byte).unwrap_or(Set::Ascii),
                    [0x24, 0x29] => gr = multi(final_byte).unwrap_or(Set::Latin1),
                    _ => {}
                }
            }
            // CSI (direction) sequences carry no text.
            0x9B => {
                while i < bytes.len() && (0x20..=0x3F).contains(&bytes[i]) {
                    i += 1;
                }
                i += 1;
            }
            0x21..=0x7E => match gl {
                Set::Other(encoding) => out.push_encoded(encoding, byte | 0x80),
                _ => out.push_char(byte as char),
            },
            0xA0..=0xFF => match gr {
                Set::Other(encoding) => out.push_encoded(encoding, byte),
                _ => out.push_char(byte as char),
            },
            0x80..=0x9F => {}
            _ => out.push_char(byte as char),
        }
    }
    out.flush();
    Ok(out.text)
}

#[cfg(test)]
mod tests {
    #[test]
    fn korean() {
        const UTF8: &str = "가나다";
        const COMP: &[u8] = &[
            27, 37, 71, 234, 176, 128, 235, 130, 152, 235, 139, 164, 27, 37, 64,
        ];
        assert_eq!(crate::utf8_to_compound_text(UTF8), COMP);
        assert_eq!(crate::compound_text_to_utf8(COMP).unwrap(), UTF8);
    }

    #[test]
    fn iso_2011_jp() {
        const UTF8: &str = "東京";
        const COMP: &[u8] = &[27, 36, 40, 66, 69, 108, 53, 126];
        assert_eq!(crate::compound_text_to_utf8(COMP).unwrap(), UTF8);
    }

    // Exponential patch (#ERR-18).
    #[test]
    fn unescaped_latin1_is_the_default() {
        assert_eq!(crate::compound_text_to_utf8(&[252]).unwrap(), "ü");
        assert_eq!(crate::compound_text_to_utf8(b"Gr\xfc\xdfe").unwrap(), "Grüße");
    }

    #[test]
    fn unescaped_utf8_still_decodes_as_utf8() {
        assert_eq!(crate::compound_text_to_utf8("ü".as_bytes()).unwrap(), "ü");
        assert_eq!(crate::compound_text_to_utf8(b"").unwrap(), "");
    }

    #[test]
    fn designations_and_mixed_runs() {
        // ESC - A (Latin-1 GR), then UTF-8, then Latin-2.
        let mut bytes = alloc::vec![0x1B, 0x2D, 0x41, b'a', 0xE4];
        bytes.extend_from_slice(&crate::utf8_to_compound_text("→"));
        bytes.extend_from_slice(&[0x1B, 0x2D, 0x42, 0xB9]);
        assert_eq!(crate::compound_text_to_utf8(&bytes).unwrap(), "aä→š");
        // KS C 5601 in GL: 가 = B0A1 in EUC-KR.
        assert_eq!(crate::compound_text_to_utf8(&[0x1B, 0x24, 0x28, 0x43, 0x30, 0x21]).unwrap(), "가");
    }

    #[test]
    fn malformed_input_never_errors() {
        for bytes in [&[0x1B][..], &[0x1B, 0x25, 0x47][..], &[0x1B, 0x25, 0x47, 0xFF][..], &[0x1B, 0x24, 0x28][..], &[0x9B, 0x31][..]] {
            assert!(crate::compound_text_to_utf8(bytes).is_ok(), "{bytes:?}");
        }
    }
}
