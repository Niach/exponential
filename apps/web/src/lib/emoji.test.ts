// EXP-551 — the web half of the shared emoji dataset semantics: search
// ranking, recents and the `:shortcode` token rule. The natives mirror these
// rules by hand (EmojiCatalog.swift / EmojiCatalog.kt / emoji.rs); this file
// pins them against the REAL generated dataset so a regenerated
// emoji.generated.json cannot silently change behaviour.

import { beforeAll, beforeEach, describe, expect, it } from "vitest"
import dataset from "@/lib/emoji.generated.json"
import {
  findEmojiByShortcode,
  indexEmojiData,
  matchEmojiToken,
  MAX_RECENT_EMOJI,
  pushRecentEmoji,
  readRecentEmoji,
  searchEmoji,
} from "@/lib/emoji"

const data = indexEmojiData(dataset)

function memoryStorage(): Storage {
  const map = new Map<string, string>()
  return {
    get length() {
      return map.size
    },
    clear: () => map.clear(),
    getItem: (key: string) => map.get(key) ?? null,
    key: (index: number) => [...map.keys()][index] ?? null,
    removeItem: (key: string) => void map.delete(key),
    setItem: (key: string, value: string) => void map.set(key, value),
  }
}

describe(`emoji dataset`, () => {
  it(`is the generated shape: 9 groups, every record in a group`, () => {
    expect(dataset.groups).toHaveLength(9)
    expect(dataset.groups[0]).toBe(`Smileys & emotion`)
    expect(dataset.groups[8]).toBe(`Flags`)
    expect(dataset.emojis.length).toBeGreaterThan(1800)
    for (const group of data.groups) {
      expect(group.emojis.length).toBeGreaterThan(0)
    }
    // No skin-tone components / regional indicators leaked through.
    expect(dataset.emojis.some((e) => e.l === `light skin tone`)).toBe(false)
    expect(dataset.emojis.some((e) => e.l.startsWith(`regional indicator`))).toBe(
      false
    )
  })

  it(`indexes GitHub shortcodes`, () => {
    expect(findEmojiByShortcode(data, `tada`)?.u).toBe(`🎉`)
    // emojibase emits the emoji-style variation sequence (U+1F44D U+FE0F)
    // for characters that have a text-style variant, so compare by label.
    expect(findEmojiByShortcode(data, `+1`)?.l).toBe(`thumbs up`)
    expect(findEmojiByShortcode(data, `THUMBSUP`)?.l).toBe(`thumbs up`)
    expect(findEmojiByShortcode(data, `definitely-not-an-emoji`)).toBeNull()
  })
})

describe(`searchEmoji`, () => {
  it(`ranks shortcode prefix over label prefix over tags`, () => {
    const results = searchEmoji(data, `smi`, 8)
    // `smile` / `smiley` / `smiling_*` shortcodes come first…
    expect(results[0].s.some((s) => s.startsWith(`smi`))).toBe(true)
    // …and every result actually matches somewhere.
    for (const e of results) {
      const q = `smi`
      const hit =
        e.s.some((s) => s.includes(q)) ||
        e.l.includes(q) ||
        e.t.some((t) => t.includes(q))
      expect(hit).toBe(true)
    }
  })

  it(`puts an exact shortcode first`, () => {
    expect(searchEmoji(data, `+1`, 3)[0].l).toBe(`thumbs up`)
    expect(searchEmoji(data, `tada`, 3)[0].u).toBe(`🎉`)
    expect(searchEmoji(data, `heart`, 3)[0].s).toContain(`heart`)
  })

  it(`is case-insensitive, tolerates colons and respects the limit`, () => {
    expect(searchEmoji(data, `TADA`, 3)[0].u).toBe(`🎉`)
    expect(searchEmoji(data, `:tada:`, 3)[0].u).toBe(`🎉`)
    expect(searchEmoji(data, `a`, 5)).toHaveLength(5)
    expect(searchEmoji(data, ``, 5)).toEqual([])
    expect(searchEmoji(data, `   `, 5)).toEqual([])
    expect(searchEmoji(data, `zzzzzz`, 5)).toEqual([])
  })
})

// EXP-600: skin tones are gone — pickers only ever offer and insert the base
// yellow record; the dataset's `k` variants stay deliberately unread.

describe(`per-device prefs`, () => {
  beforeAll(() => {
    Object.defineProperty(window, `localStorage`, {
      value: memoryStorage(),
      configurable: true,
    })
  })
  beforeEach(() => {
    window.localStorage.clear()
  })

  it(`keeps recents most-recent-first, deduped and capped`, () => {
    expect(readRecentEmoji()).toEqual([])
    pushRecentEmoji(`🎉`)
    pushRecentEmoji(`👍`)
    pushRecentEmoji(`🎉`)
    expect(readRecentEmoji()).toEqual([`🎉`, `👍`])
    for (let i = 0; i < MAX_RECENT_EMOJI + 5; i++) {
      pushRecentEmoji(String.fromCodePoint(0x1f600 + i))
    }
    expect(readRecentEmoji()).toHaveLength(MAX_RECENT_EMOJI)
    expect(readRecentEmoji()[0]).toBe(
      String.fromCodePoint(0x1f600 + MAX_RECENT_EMOJI + 4)
    )
  })

  it(`tolerates corrupt storage`, () => {
    window.localStorage.setItem(`exp.emojiRecent`, `{not json`)
    expect(readRecentEmoji()).toEqual([])
    window.localStorage.setItem(`exp.emojiRecent`, `[1, "🎉", null]`)
    expect(readRecentEmoji()).toEqual([`🎉`])
  })
})

describe(`matchEmojiToken`, () => {
  it(`matches a :shortcode of two or more chars after start or whitespace`, () => {
    expect(matchEmojiToken(`:sm`)).toEqual({
      query: `sm`,
      closed: false,
      length: 3,
    })
    expect(matchEmojiToken(`Ship it :tad`)).toMatchObject({
      query: `tad`,
      closed: false,
      length: 4,
    })
    expect(matchEmojiToken(`Ship it :+1`)).toMatchObject({ query: `+1` })
    expect(matchEmojiToken(`line\n:thumbs_up`)).toMatchObject({
      query: `thumbs_up`,
    })
  })

  it(`reports the closing colon`, () => {
    expect(matchEmojiToken(`Ship it :tada:`)).toEqual({
      query: `tada`,
      closed: true,
      length: 6,
    })
  })

  it(`never triggers on times, labels, smileys, urls or one char`, () => {
    expect(matchEmojiToken(`meet at 12:30`)).toBeNull()
    expect(matchEmojiToken(`note: fix`)).toBeNull()
    expect(matchEmojiToken(`note:fix`)).toBeNull()
    expect(matchEmojiToken(`well :)`)).toBeNull()
    expect(matchEmojiToken(`see http://x`)).toBeNull()
    expect(matchEmojiToken(`:s`)).toBeNull()
    expect(matchEmojiToken(`:`)).toBeNull()
    expect(matchEmojiToken(`a:bc`)).toBeNull()
  })
})
