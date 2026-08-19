#!/usr/bin/env bun
// EXP-551 — emits the shared emoji dataset into all four clients.
// Run from the repo root with: `bun run --filter @exp/emoji generate`.
//
// Source of truth: `emojibase-data` (devDependency of this package; MIT — the
// licence is reproduced in LICENSE-emojibase.txt and attributed to every
// client through packages/licenses/curated/supplement.ts). Its per-locale
// `compact.json` plus the GitHub shortcode table are projected into ONE
// compact JSON (shape: src/index.ts) that is committed byte-identically into:
//
//   web      apps/web/src/lib/emoji.generated.json      (lazy import chunk)
//   iOS      apps/ios/Exponential/Resources/emoji.json  (Bundle.main resource)
//   Android  apps/android/app/src/main/assets/emoji.json
//   desktop  apps/desktop/assets/emoji.json             (include_str!)
//
// so the four pickers and `:shortcode` typeaheads search the same names,
// shortcodes and tags. Pickers insert the UNICODE (`u`/`k[n]`), never a
// shortcode: `issues.description` / `comments.body` are plain GFM shared by
// every client and only unicode renders everywhere.
//
// Rules applied here (mirrored by nothing else — this is the one place):
//   - entries without a `group` (regional-indicator letters) and emojibase
//     group 2 (skin-tone components) are dropped; the remaining groups
//     0,1,3..9 are renumbered 0..8 (index into `groups`);
//   - display order = emojibase `order`;
//   - `s` = GitHub shortcodes when GitHub has any, else emojibase's own
//     (36 newest emoji have no GitHub name yet);
//   - `k` = the five UNIFORM skin-tone variants light → dark, only when all
//     five exist (mixed-tone multi-person variants are never offered).

import { readFileSync, writeFileSync } from "node:fs"
import { dirname, join } from "node:path"
import { fileURLToPath } from "node:url"
import type { EmojiDataset, EmojiRecord } from "../src/index"
import { EMOJI_GROUP_LABELS, EMOJI_TONES } from "../src/index"

const __dirname = dirname(fileURLToPath(import.meta.url))
const pkgRoot = join(__dirname, "..")
const repoRoot = join(pkgRoot, "..", "..")
const DATA_DIR = join(repoRoot, "node_modules/emojibase-data")

const OUTPUTS = [
  "apps/web/src/lib/emoji.generated.json",
  "apps/ios/Exponential/Resources/emoji.json",
  "apps/android/app/src/main/assets/emoji.json",
  "apps/desktop/assets/emoji.json",
]

interface CompactEmoji {
  hexcode: string
  label: string
  unicode: string
  group?: number
  order?: number
  tags?: string[]
  skins?: CompactEmoji[]
}
type ShortcodeMap = Record<string, string | string[]>

function readJson<T>(rel: string): T {
  return JSON.parse(readFileSync(join(DATA_DIR, rel), "utf8")) as T
}

const version = (
  readJson<{ version: string }>("package.json") as { version: string }
).version
const compact = readJson<CompactEmoji[]>("en/compact.json")
const github = readJson<ShortcodeMap>("en/shortcodes/github.json")
const emojibase = readJson<ShortcodeMap>("en/shortcodes/emojibase.json")

// emojibase group ids → dataset group index (group 2 = components, dropped).
const GROUP_INDEX: Record<number, number> = {
  0: 0,
  1: 1,
  3: 2,
  4: 3,
  5: 4,
  6: 5,
  7: 6,
  8: 7,
  9: 8,
}
if (Object.keys(GROUP_INDEX).length !== EMOJI_GROUP_LABELS.length) {
  throw new Error("GROUP_INDEX and EMOJI_GROUP_LABELS disagree")
}

// Fitzpatrick modifiers, light → dark, in the order `k` is emitted.
const TONE_MODIFIERS = ["1F3FB", "1F3FC", "1F3FD", "1F3FE", "1F3FF"]
if (TONE_MODIFIERS.length !== EMOJI_TONES) {
  throw new Error("TONE_MODIFIERS and EMOJI_TONES disagree")
}

function shortcodes(hexcode: string): string[] {
  const pick = github[hexcode] ?? emojibase[hexcode]
  if (pick === undefined) return []
  return Array.isArray(pick) ? [...pick] : [pick]
}

/** The five uniform-tone variants (light → dark), or undefined. */
function tones(skins: CompactEmoji[] | undefined): string[] | undefined {
  if (!skins) return undefined
  const byTone = new Map<number, string>()
  for (const skin of skins) {
    const modifiers = skin.hexcode
      .split("-")
      .filter((part) => TONE_MODIFIERS.includes(part))
    if (modifiers.length === 0) continue
    // Mixed-tone variants (two people, two tones) carry differing modifiers.
    if (!modifiers.every((m) => m === modifiers[0])) continue
    const index = TONE_MODIFIERS.indexOf(modifiers[0])
    if (!byTone.has(index)) byTone.set(index, skin.unicode)
  }
  if (byTone.size !== EMOJI_TONES) return undefined
  return TONE_MODIFIERS.map((_, i) => byTone.get(i)!)
}

const emojis: EmojiRecord[] = compact
  .filter((e) => e.group !== undefined && e.group in GROUP_INDEX)
  .sort((a, b) => (a.order ?? 0) - (b.order ?? 0))
  .map((e) => {
    const record: EmojiRecord = {
      u: e.unicode,
      l: e.label,
      g: GROUP_INDEX[e.group!],
      s: shortcodes(e.hexcode),
      t: e.tags ?? [],
    }
    const k = tones(e.skins)
    if (k) record.k = k
    return record
  })

const seen = new Set<string>()
for (const e of emojis) {
  if (seen.has(e.u)) throw new Error(`duplicate emoji ${e.u} (${e.l})`)
  seen.add(e.u)
}

const dataset: EmojiDataset = {
  version,
  groups: [...EMOJI_GROUP_LABELS],
  emojis,
}

// One line per record: diffs stay reviewable when emojibase moves, and every
// client parses it the same regardless of whitespace.
const body =
  `{\n` +
  `"version":${JSON.stringify(dataset.version)},\n` +
  `"groups":${JSON.stringify(dataset.groups)},\n` +
  `"emojis":[\n` +
  emojis.map((e) => JSON.stringify(e)).join(",\n") +
  `\n]}\n`

for (const rel of OUTPUTS) {
  writeFileSync(join(repoRoot, rel), body)
  console.log(`${rel}  (${Math.round(body.length / 1024)} KB)`)
}
console.log(
  `emojibase-data ${version}: ${emojis.length} emoji, ` +
    `${emojis.filter((e) => e.k).length} with skin tones`
)
