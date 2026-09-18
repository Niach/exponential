// @vitest-environment node
import { describe, expect, it } from "vitest"
import type { EmojiDataset } from "@exp/emoji"
import {
  findEmojiByShortcode,
  indexEmojiData,
  searchEmoji,
} from "./emoji-search"

// EXP-551 — the dataset's pure half: the index and the ranked search. The
// natives mirror these rules by hand (EmojiCatalog.swift / EmojiCatalog.kt /
// emoji.rs); the app's lib/emoji.test.ts pins the same rules against the REAL
// generated dataset through the re-export.

const FIXTURE: EmojiDataset = {
  version: `test`,
  groups: [`Smileys & emotion`, `People & body`, `Animals & nature`],
  emojis: [
    { u: `😀`, l: `grinning face`, g: 0, s: [`grinning`], t: [`happy`] },
    { u: `😄`, l: `smiling face`, g: 0, s: [`smile`], t: [`smiley`] },
    { u: `😊`, l: `smiling face with smiling eyes`, g: 0, s: [`blush`], t: [] },
    { u: `👍`, l: `thumbs up`, g: 1, s: [`+1`, `thumbsup`], t: [`yes`] },
    { u: `🐛`, l: `bug`, g: 2, s: [`bug`], t: [`insect`] },
    { u: `🎉`, l: `party popper`, g: 0, s: [`tada`], t: [`celebration`] },
  ],
}

const data = indexEmojiData(FIXTURE)

describe(`indexEmojiData`, () => {
  it(`files every record under its group, in dataset order`, () => {
    expect(data.groups.map((group) => group.label)).toEqual(FIXTURE.groups)
    expect(data.groups[0].emojis.map((e) => e.u)).toEqual([`😀`, `😄`, `😊`, `🎉`])
    expect(data.groups[1].emojis.map((e) => e.u)).toEqual([`👍`])
    expect(data.byUnicode.get(`🐛`)?.l).toBe(`bug`)
  })

  it(`indexes GitHub shortcodes`, () => {
    expect(findEmojiByShortcode(data, `tada`)?.u).toBe(`🎉`)
    expect(findEmojiByShortcode(data, `+1`)?.l).toBe(`thumbs up`)
    expect(findEmojiByShortcode(data, `THUMBSUP`)?.l).toBe(`thumbs up`)
    expect(findEmojiByShortcode(data, `definitely-not-an-emoji`)).toBeNull()
  })
})

describe(`searchEmoji`, () => {
  it(`ranks shortcode prefix over label prefix over tags`, () => {
    const results = searchEmoji(data, `smi`, 8)
    // `smile` (a shortcode prefix) before `smiling face …` (a label prefix)
    // before the record that only carries a `smiley` TAG.
    expect(results.map((e) => e.u)).toEqual([`😄`, `😊`])
  })

  it(`puts an exact shortcode first`, () => {
    expect(searchEmoji(data, `+1`, 3)[0].l).toBe(`thumbs up`)
    expect(searchEmoji(data, `tada`, 3)[0].u).toBe(`🎉`)
    expect(searchEmoji(data, `bug`, 3)[0].u).toBe(`🐛`)
  })

  it(`is case-insensitive, tolerates colons and respects the limit`, () => {
    expect(searchEmoji(data, `TADA`, 3)[0].u).toBe(`🎉`)
    expect(searchEmoji(data, `:tada:`, 3)[0].u).toBe(`🎉`)
    expect(searchEmoji(data, `s`, 2)).toHaveLength(2)
    expect(searchEmoji(data, ``, 5)).toEqual([])
    expect(searchEmoji(data, `   `, 5)).toEqual([])
    expect(searchEmoji(data, `zzzzzz`, 5)).toEqual([])
  })
})
