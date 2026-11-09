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

// #EXP-1629 (fixtures/avatar.json): the palette itself, for a painter that
// has no `--avatar-N` vars (the eUI natives), in the hash's order.
export const AVATAR_PALETTE = [`#F87171`, `#FB923C`, `#FACC15`, `#4ADE80`, `#2DD4BF`, `#60A5FA`, `#A78BFA`, `#F472B6`] as const

/** The hue's alpha in the fallback's fill. */
export const AVATAR_FILL_ALPHA = 0.2

/** The cross-client initials rule (fixtures/avatar.json `initials`): an
 *  email reduces to its local part, the rest splits on space . _ - +; two or
 *  more parts give their first letters, one part its first two characters;
 *  uppercased; nothing is `?`. */
export function avatarInitials(label: string | null | undefined): string {
  const base = (label ?? ``).trim()
  const at = base.indexOf(`@`)
  const local = at >= 0 ? base.slice(0, at) : base
  const parts = local.split(/[ ._\-+]/).filter((part) => part.trim() !== ``)
  const picked = parts.length >= 2
    ? [...parts[0]!].slice(0, 1).join(``) + [...parts[1]!].slice(0, 1).join(``)
    : [...local].slice(0, 2).join(``)
  return picked === `` ? `?` : picked.toUpperCase()
}
