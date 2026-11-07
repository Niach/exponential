//! Round 9 §4: UAX #31 identifiers (`^[\p{XID_Start}_][\p{XID_Continue}]*$`)
//! over code points, and the §14.3 id `mangle`. The tables are GENERATED
//! (`generated/xid.rs`, Unicode 17.0.0).

pub use crate::generated::xid::{is_identifier, is_xid_continue, is_xid_start, UNICODE_VERSION};

/// §14.3 `mangle(s)`: every code point outside XID_Continue becomes `_`,
/// the empty result becomes `_`, and a first code point that is neither
/// XID_Start nor `_` gets a `_` prepended.
pub fn mangle(s: &str) -> String {
    let mut out: String = s.chars().map(|c| if is_xid_continue(c) { c } else { '_' }).collect();
    if out.is_empty() {
        return "_".into();
    }
    let first = out.chars().next().unwrap();
    if first != '_' && !is_xid_start(first) {
        out.insert(0, '_');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifiers_and_mangle() {
        assert!(is_identifier("root"));
        assert!(is_identifier("_x1"));
        assert!(is_identifier("ä_ü"));
        assert!(!is_identifier(""));
        assert!(!is_identifier("user-card"));
        assert!(!is_identifier("1st"));
        assert!(!is_identifier("a.b"));
        assert_eq!(mangle("user-card"), "user_card");
        assert_eq!(mangle("a.b"), "a_b");
        assert_eq!(mangle("1st"), "_1st");
        assert_eq!(mangle("ab c"), "ab_c");
        assert_eq!(mangle(""), "_");
    }
}
