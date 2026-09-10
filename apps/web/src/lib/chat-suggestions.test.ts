import { describe, expect, it } from "vitest"
import {
  CHAT_SUGGESTION_COUNT,
  CHAT_SUGGESTION_POOL,
  pickChatSuggestions,
} from "./chat-suggestions"

// EXP-820: the pool is mirrored on the desktop byte for byte
// (`chat_screen.rs` `CHAT_SUGGESTIONS`); its test locks the same list.
describe(`chat suggestions`, () => {
  it(`is a pool of 16 distinct prompts, the issue-ref ones ending in #`, () => {
    expect(CHAT_SUGGESTION_POOL).toHaveLength(16)
    expect(new Set(CHAT_SUGGESTION_POOL).size).toBe(16)
    expect(CHAT_SUGGESTION_POOL.slice(0, 3)).toEqual([`Fix #`, `Explain #`, `Review #`])
    for (const suggestion of CHAT_SUGGESTION_POOL) {
      expect(suggestion.trim()).toBe(suggestion)
      // A `#` only ever sits where the autocomplete can open on it.
      if (suggestion.includes(`#`)) expect(suggestion).toMatch(/#( |$)/)
    }
  })

  it(`draws distinct entries from the pool, in pool order`, () => {
    let seed = 7
    const random = () => {
      seed = (seed * 48271) % 2147483647
      return seed / 2147483647
    }
    const picked = pickChatSuggestions(CHAT_SUGGESTION_COUNT, random)
    expect(picked).toHaveLength(4)
    expect(new Set(picked).size).toBe(4)
    for (const suggestion of picked) expect(CHAT_SUGGESTION_POOL).toContain(suggestion)
    const order = picked.map((s) => CHAT_SUGGESTION_POOL.indexOf(s))
    expect([...order].sort((a, b) => a - b)).toEqual(order)
  })

  it(`never asks for more than the pool holds and covers every entry`, () => {
    expect(pickChatSuggestions(99, () => 0)).toEqual([...CHAT_SUGGESTION_POOL])
    // Over many draws with real randomness every prompt shows up.
    const seen = new Set<string>()
    for (let i = 0; i < 400; i++) pickChatSuggestions().forEach((s) => seen.add(s))
    expect(seen.size).toBe(CHAT_SUGGESTION_POOL.length)
  })
})
