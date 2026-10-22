// EXP-698 r4 — the avatar fallback hue.
//
// A user without a picture gets initials painted on a hue picked from the
// `avatar` group in `packages/design-tokens/tokens.json` (mirrored as the CSS
// vars `--avatar-0`..`--avatar-7`). The index is FNV-1a 32-bit over the UTF-8
// bytes of the user id, modulo the palette size — a byte-identical contract
// across all four clients, each pinned by the same fixture in
// `avatar-color.test.ts`. Never reorder the palette: the index is what the
// hash lands on.

export const AVATAR_HUE_COUNT = 8

const FNV_OFFSET_BASIS = 0x811c9dc5
const FNV_PRIME = 0x01000193

const encoder = new TextEncoder()

export function avatarHueIndex(userId: string | null | undefined): number {
  const bytes = encoder.encode(userId ?? ``)
  let hash = FNV_OFFSET_BASIS
  for (const byte of bytes) {
    hash ^= byte
    hash = Math.imul(hash, FNV_PRIME) >>> 0
  }
  return hash % AVATAR_HUE_COUNT
}
