//! #EXP-1629: the Avatar fallback rule (`fixtures/avatar.json`, its
//! `$comment` is the spec), the app's own (packages/design-tokens `avatar`,
//! the IDE's `ui::user_avatar`): a `seed` (a user id) picks
//! `AVATAR_PALETTE[fnv1a32(utf8(seed)) % 8]`, filled at [`AVATAR_FILL_ALPHA`]
//! with the initials in the hue at full alpha; no seed = the
//! `Avatar/fallback` recipe colours. #EXP-1754: the hue KEY is the seed (the
//! id), else the trimmed name (which may be an email), see
//! [`avatar_hue_key`]. Painters share it from here.

/// design-tokens `avatar` in ITS order (append-only: the hash lands on the index).
pub const AVATAR_PALETTE: [&str; 8] = ["#F87171", "#FB923C", "#FACC15", "#4ADE80", "#2DD4BF", "#60A5FA", "#A78BFA", "#F472B6"];

/// The fill's alpha over the hue.
pub const AVATAR_FILL_ALPHA: f32 = 0.2;

/// What the hue is keyed on (`fixtures/avatar.json` `keys`, #EXP-1754): the
/// id, else the trimmed name, else the trimmed email; blank counts as
/// absent. `None` = no key: the `Avatar/fallback` recipe colours.
pub fn avatar_hue_key<'a>(id: &'a str, name: &'a str, email: &'a str) -> Option<&'a str> {
    if !id.trim().is_empty() {
        return Some(id);
    }
    [name, email].into_iter().map(str::trim).find(|label| !label.is_empty())
}

/// FNV-1a/32 over the seed's UTF-8 bytes, modulo the palette.
pub fn avatar_hue_index(seed: &str) -> usize {
    let mut hash: u32 = 0x811C_9DC5;
    for byte in seed.as_bytes() {
        hash ^= *byte as u32;
        hash = hash.wrapping_mul(0x0100_0193);
    }
    hash as usize % AVATAR_PALETTE.len()
}

/// The letters of a name: an email reduces to its local part, the rest splits
/// on space . _ - +; two or more parts give their first letters, one part its
/// first two characters; uppercased; nothing = `?`.
pub fn avatar_initials(name: &str) -> String {
    let base = name.trim();
    let local = base.split_once('@').map_or(base, |(local, _)| local);
    let parts: Vec<&str> = local.split([' ', '.', '_', '-', '+']).filter(|p| !p.trim().is_empty()).collect();
    let picked: String = match parts.as_slice() {
        [first, second, ..] => first.chars().take(1).chain(second.chars().take(1)).collect(),
        _ => local.chars().take(2).collect(),
    };
    if picked.is_empty() {
        return "?".to_string();
    }
    picked.to_uppercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    /// `fixtures/avatar.json` `keys` (#EXP-1754): id, else name, else email, else grey.
    #[test]
    fn avatar_keys_replay_the_fixture() {
        let f: Value = serde_json::from_str(include_str!("../../../../../packages/exponential-ui/fixtures/avatar.json")).expect("avatar.json");
        let keys = f["keys"].as_array().unwrap();
        assert!(!keys.is_empty());
        for case in keys {
            let field = |k: &str| case[k].as_str().unwrap_or("");
            let key = avatar_hue_key(field("id"), field("name"), field("email"));
            assert_eq!(key, case["key"].as_str(), "{}", case["case"]);
            assert_eq!(key.map(avatar_hue_index), case["hue"].as_u64().map(|h| h as usize), "{}", case["case"]);
        }
    }
}
