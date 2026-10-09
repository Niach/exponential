//! Round 1 (docs/round-1-contract.md §4): the BUILT-IN UI strings — the copy
//! no author writes (search placeholder, month buttons, copy, close, …). The
//! ids and English defaults are `catalog/strings.json` (embedded); a host
//! overrides any id per surface; `$string.<id>` in a prop value resolves
//! through the surface's table at bind time. Mirrors `src/strings.ts`.

use std::sync::LazyLock;

use indexmap::IndexMap;
use serde_json::Value;

use crate::generated::catalog as g;

/// One string table: id → text (the defaults merged under the overrides).
pub type StringTable = IndexMap<String, String>;

#[derive(serde::Deserialize)]
struct StringsFile {
    strings: IndexMap<String, String>,
}

/// The English defaults, in source order.
pub static DEFAULT_STRINGS: LazyLock<StringTable> =
    LazyLock::new(|| serde_json::from_str::<StringsFile>(g::STRINGS_JSON).expect("strings.json").strings);

/// `$string.choose` → `choose`; `None` for any other value
/// (`^\$string\.([a-zA-Z][a-zA-Z0-9]*)$`).
pub fn parse_string_ref(value: &str) -> Option<&str> {
    let id = value.strip_prefix("$string.")?;
    let mut chars = id.chars();
    let first = chars.next()?;
    (first.is_ascii_alphabetic() && chars.all(|c| c.is_ascii_alphanumeric())).then_some(id)
}

/// True for a well-formed reference to a KNOWN id.
pub fn is_string_ref(value: &str) -> bool {
    parse_string_ref(value).is_some_and(|id| DEFAULT_STRINGS.contains_key(id))
}

/// The host's overrides merged over the defaults.
pub fn string_table(overrides: &IndexMap<String, String>) -> StringTable {
    let mut out = DEFAULT_STRINGS.clone();
    for (k, v) in overrides {
        out.insert(k.clone(), v.clone());
    }
    out
}

/// `{name}` placeholders filled from `params`; unknown (or null) ones stay.
pub fn format_string(template: &str, params: &serde_json::Map<String, Value>) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let name_len = after.char_indices().find(|(_, c)| !c.is_ascii_alphanumeric()).map(|(i, _)| i).unwrap_or(after.len());
        let name = &after[..name_len];
        let well_formed = name.chars().next().is_some_and(|c| c.is_ascii_alphabetic()) && after[name_len..].starts_with('}');
        if well_formed {
            match params.get(name).filter(|v| !v.is_null()) {
                Some(v) => out.push_str(&crate::json::to_js_string(v)),
                None => out.push_str(&rest[open..open + 1 + name_len + 1]),
            }
            rest = &after[name_len + 1..];
        } else {
            out.push('{');
            rest = after;
        }
    }
    out.push_str(rest);
    out
}

/// A value with a `$string.<id>` reference replaced by the table's text (an
/// unknown id resolves to the id itself); any other value passes.
pub fn resolve_string<'a>(value: &'a str, table: &'a StringTable) -> &'a str {
    match parse_string_ref(value) {
        Some(id) => table.get(id).map(String::as_str).unwrap_or(id),
        None => value,
    }
}

/// The default text of one id.
pub fn default_string(id: &str) -> Option<&'static str> {
    DEFAULT_STRINGS.get(id).map(String::as_str)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn overrides_formatting_resolution() {
        assert_eq!(DEFAULT_STRINGS.keys().map(String::as_str).collect::<Vec<_>>(), g::STRING_IDS);
        assert_eq!(DEFAULT_STRINGS.values().map(String::as_str).collect::<Vec<_>>(), g::STRING_DEFAULTS);
        let mut over = IndexMap::new();
        over.insert("search".to_string(), "Suchen…".to_string());
        assert_eq!(string_table(&over)["search"], "Suchen…");
        let params = json!({"page": 2, "total": 5}).as_object().unwrap().clone();
        assert_eq!(format_string(&DEFAULT_STRINGS["pageOf"], &params), "Page 2 of 5");
        assert_eq!(format_string("{count} of {total}", json!({"count": 1}).as_object().unwrap()), "1 of {total}");
        assert_eq!(format_string("{ x } {1a}", &params), "{ x } {1a}");
        let mut de = IndexMap::new();
        de.insert("cancel".to_string(), "Abbrechen".to_string());
        assert_eq!(resolve_string("$string.cancel", &string_table(&de)), "Abbrechen");
        assert_eq!(resolve_string("plain", &DEFAULT_STRINGS), "plain");
        assert_eq!(resolve_string("$string.nope", &DEFAULT_STRINGS), "nope");
        assert!(is_string_ref("$string.confirm") && !is_string_ref("$string.nope") && parse_string_ref("$string.1x").is_none());
    }
}
