import { describe, expect, test } from "bun:test"
import fixture from "../fixtures/nav-badges.json" with { type: "json" }
import { appIconBadge, chatsBadge, countBadgeLabel, needsYouCounts, rowBadge, sumBadges, teamBadge } from "./nav-badges"

describe(`nav-badges.json`, () => {
  for (const c of fixture.labelCases) test(`label: ${c.name}`, () => expect(countBadgeLabel(c.count)).toBe(c.expected))
  for (const c of fixture.chatsCases)
    test(`chats: ${c.name}`, () => expect(chatsBadge(c.input.groups as never, c.input.reads as never)).toEqual(c.expected as never))
  for (const c of fixture.rowCases) test(`row: ${c.name}`, () => expect(rowBadge(c.input)).toEqual(c.expected as never))
  for (const c of fixture.sumCases) test(`sum: ${c.name}`, () => expect(sumBadges(c.input as never)).toEqual(c.expected as never))
  for (const c of fixture.teamCases) test(`team: ${c.name}`, () => expect(teamBadge(c.input as never)).toEqual(c.expected as never))
  for (const c of fixture.needsYouCases)
    test(`needs you: ${c.name}`, () => expect(needsYouCounts(c.input)).toEqual(c.expected as never))
  for (const c of fixture.appIconCases) test(`app icon: ${c.name}`, () => expect(appIconBadge(c.accounts)).toBe(c.expected))
})
