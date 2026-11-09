//! #EXP-1629: the Avatar fallback rule (`fixtures/avatar.json`, its
//! `$comment` is the spec), the app's own (packages/design-tokens `avatar`,
//! the IDE's `ui::user_avatar`): a `seed` (a user id) picks
//! `AVATAR_PALETTE[fnv1a32(utf8(seed)) % 8]`, filled at [`AVATAR_FILL_ALPHA`]
//! with the initials in the hue at full alpha; no seed = the
//! `Avatar/fallback` recipe colours. Painters share it from here.

/// design-tokens `avatar` in ITS order (append-only: the hash lands on the index).
pub const AVATAR_PALETTE: [&str; 8] = ["#F87171", "#FB923C", "#FACC15", "#4ADE80", "#2DD4BF", "#60A5FA", "#A78BFA", "#F472B6"];

/// The fill's alpha over the hue.
pub const AVATAR_FILL_ALPHA: f32 = 0.2;

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
