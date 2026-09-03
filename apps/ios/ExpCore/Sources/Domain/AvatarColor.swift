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

/// The palette slot for a user id; a nil id hashes as the empty string, so an
/// unresolved member still gets a stable colour instead of a special case.
public func avatarHueIndex(_ userId: String?) -> Int {
    var hash = fnvOffsetBasis
    for byte in (userId ?? "").utf8 {
        hash ^= UInt32(byte)
        hash = hash &* fnvPrime
    }
    return Int(hash % UInt32(avatarHueCount))
}
