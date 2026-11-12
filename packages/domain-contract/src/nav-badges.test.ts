import { describe, expect, test } from "bun:test"
import fixture from "../fixtures/nav-badges.json" with { type: "json" }
import { appIconBadge, chatsBadge, countBadgeLabel, rowBadge } from "./nav-badges"

describe(`nav-badges.json`, () => {
  for (const c of fixture.labelCases) test(`label: ${c.name}`, () => expect(countBadgeLabel(c.count)).toBe(c.expected))
  for (const c of fixture.chatsCases)
    test(`chats: ${c.name}`, () => expect(chatsBadge(c.input.groups as never, c.input.reads as never)).toEqual(c.expected as never))
  for (const c of fixture.rowCases) test(`row: ${c.name}`, () => expect(rowBadge(c.input)).toEqual(c.expected as never))
  for (const c of fixture.appIconCases) test(`app icon: ${c.name}`, () => expect(appIconBadge(c.teams)).toBe(c.expected))
})
