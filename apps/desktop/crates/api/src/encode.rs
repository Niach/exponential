//! Minimal RFC-3986 percent encode/decode — kept hand-rolled so the encoding
//! byte-matches the proven iOS client (`TrpcClient.swift`): everything except
//! the unreserved set (`ALPHA / DIGIT / "-" / "." / "_" / "~"`) is `%XX`
//! (uppercase hex) over the UTF-8 bytes. `URLComponents`-style leniency (a
//! literal `+`) breaks servers that decode `+` as space — we never emit it.

/// Percent-encode `input` keeping only RFC-3986 unreserved characters.
pub(crate) fn percent_encode(input: &str) -> String {
    let mut out = String::with_capacity(input.len() * 3);
    for byte in input.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(*byte as char);
            }
            _ => {
                out.push('%');
                out.push_str(&format!("{byte:02X}"));
            }
        }
    }
    out
}

/// Percent-decode `input`. Invalid escapes pass through verbatim (tolerant —
/// used on OAuth callback fragments the OS hands us; never on secrets we
/// wrote ourselves). Does NOT treat `+` as space (the server side uses
/// `encodeURIComponent`, which never emits `+` for space).
pub(crate) fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            if let (Some(h), Some(l)) = (
                bytes.get(i + 1).and_then(|b| (*b as char).to_digit(16)),
                bytes.get(i + 2).and_then(|b| (*b as char).to_digit(16)),
            ) {
                out.push((h * 16 + l) as u8);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_json_like_ios() {
        // Mirrors the iOS allowed set: alphanumerics + "-._~" pass through,
        // JSON delimiters and spaces are escaped.
        assert_eq!(
            percent_encode(r#"{"id":"a b+c"}"#),
            "%7B%22id%22%3A%22a%20b%2Bc%22%7D"
        );
    }

    #[test]
    fn passes_unreserved_through() {
        assert_eq!(percent_encode("AZaz09-._~"), "AZaz09-._~");
    }

    #[test]
    fn encodes_utf8_bytes() {
        assert_eq!(percent_encode("ä"), "%C3%A4");
    }

    #[test]
    fn decode_round_trips() {
        let original = r#"{"id":"a b+c"} äöü"#;
        assert_eq!(percent_decode(&percent_encode(original)), original);
    }

    #[test]
    fn decode_tolerates_invalid_escapes() {
        assert_eq!(percent_decode("100%"), "100%");
        assert_eq!(percent_decode("%zz"), "%zz");
        assert_eq!(percent_decode("%4"), "%4");
    }

    #[test]
    fn decode_never_plus_as_space() {
        assert_eq!(percent_decode("a+b"), "a+b");
    }
}
