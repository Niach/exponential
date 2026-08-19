// EXP-551 — the shared emoji dataset every client bundles.
//
// `scripts/generate.ts` projects emojibase-data into ONE compact JSON that is
// committed byte-identically into all four apps (see the header of that
// script for the output paths). The keys are short on purpose: the file ships
// inside two mobile bundles and a desktop binary, and every client parses it
// with a hand-written decoder anyway (Swift `Codable`, Kotlin serialization,
// Rust serde) — this file is the one place the shape is spelled out.

/** One pickable emoji. */
export interface EmojiRecord {
  /** The unicode sequence to insert (fully-qualified, no skin tone). */
  u: string
  /** Human label, lowercase, e.g. `grinning face`. */
  l: string
  /** Index into `EmojiDataset.groups`. */
  g: number
  /** Shortcodes without colons, GitHub's set where it has one (`+1`, `tada`). */
  s: string[]
  /** Search tags. */
  t: string[]
  /**
   * The five uniform skin-tone variants, light → dark (Fitzpatrick 1-2 … 6),
   * present only when the emoji supports every one of them. Multi-person
   * emoji with mixed tones are deliberately not offered.
   */
  k?: string[]
}

export interface EmojiDataset {
  /** The emojibase-data version the file was generated from. */
  version: string
  /** Group labels, indexed by `EmojiRecord.g`. */
  groups: string[]
  /** In display order (emojibase `order`). */
  emojis: EmojiRecord[]
}

/** Number of skin tones a toned emoji carries (`k.length`). */
export const EMOJI_TONES = 5

/**
 * The picker group labels, in dataset order. Duplicated into `groups` of the
 * generated file so no client needs this module — this constant only lets
 * TypeScript code name a group.
 */
export const EMOJI_GROUP_LABELS = [
  `Smileys & emotion`,
  `People & body`,
  `Animals & nature`,
  `Food & drink`,
  `Travel & places`,
  `Activities`,
  `Objects`,
  `Symbols`,
  `Flags`,
] as const
