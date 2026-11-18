import { describe, expect, test } from "bun:test"
import fixture from "../fixtures/search-list.json" with { type: "json" }
import type { ChatHit } from "./chat-store"
import { globalChatPick, globalChatRows, globalChatStart, globalSections } from "./search-list"

describe(`#EDE-99 fixtures/search-list.json: the Chats section`, () => {
  for (const c of fixture.sectionCases) test(`sections: ${c.name}`, () => expect(globalSections(c.input)).toEqual(c.expected as ReturnType<typeof globalSections>))
  for (const c of fixture.rowCases)
    test(`rows: ${c.name}`, () =>
      expect(globalChatRows({ ...c.input, chats: c.input.chats as { rootId: string; count: number; hit: ChatHit }[] })).toEqual(c.expected))
  for (const c of fixture.pickCases)
    test(`pick: ${c.name}`, () => expect(globalChatPick({ query: c.input.query, hit: c.input.hit as ChatHit })).toEqual(c.expected))
  for (const c of fixture.startCases) test(`start: ${c.name}`, () => expect(globalChatStart(c.stops, c.target)).toBe(c.expected))
})
