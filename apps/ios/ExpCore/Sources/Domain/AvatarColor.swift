import Foundation

// EXP-698 r4 — the avatar fallback hue.
//
// A user without a picture gets initials painted on a hue picked from the
// `avatar` group in `packages/design-tokens/tokens.json` (here:
// `DesignTokens.Avatar.hues`, generated in ExpUI). The index is FNV-1a 32-bit
// over the UTF-8 bytes of the user id, modulo the palette size — a
// byte-identical contract across all four clients, each pinned by the same
// fixture (`AvatarColorTests`, web's `avatar-color.test.ts`). Never reorder the
// palette: the index is what the hash lands on.

public let avatarHueCount = 8

private let fnvOffsetBasis: UInt32 = 0x811C_9DC5
private let fnvPrime: UInt32 = 0x0100_0193

/// The palette slot for a hue key (`avatarHueKey`); a nil key hashes as the
/// empty string here, but the avatar itself paints a nil key muted grey
/// (#EXP-1754) and never asks for its slot.
public func avatarHueIndex(_ userId: String?) -> Int {
    var hash = fnvOffsetBasis
    for byte in (userId ?? "").utf8 {
        hash ^= UInt32(byte)
        hash = hash &* fnvPrime
    }
    return Int(hash % UInt32(avatarHueCount))
}

/// #EXP-1754 (packages/exponential-ui/fixtures/avatar.json `keys`): what an
/// avatar's hue is keyed on — the person's id, else their name, else their
/// email (trimmed; blank counts as absent). nil = no key: the muted grey chip,
/// never a hash of "". A widget reporter is keyed on their name or email; an
/// anonymous one (no name, no email) has no key, whatever copy is shown.
public func avatarHueKey(id: String?, name: String? = nil, email: String? = nil) -> String? {
    if let id, !id.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty { return id }
    for label in [name, email] {
        let trimmed = (label ?? "").trimmingCharacters(in: .whitespacesAndNewlines)
        if !trimmed.isEmpty { return trimmed }
    }
    return nil
}
