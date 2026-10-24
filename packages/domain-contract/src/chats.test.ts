import { describe, expect, test } from "bun:test"
import fixture from "../fixtures/chats.json" with { type: "json" }
import {
  agentBubbles,
  agentInstructionsChanged,
  agentMenuEntries,
  chatList,
  chatPreview,
  chatTimeLabel,
  commandDivider,
  plainLine,
  surfaceReply,
  threadMenuEntries,
} from "./chats"

// Thread 58cb32df: the Chats reference replays every case group of
// fixtures/chats.json (the mirrors replay the same file).
describe(`chats.json`, () => {
  for (const c of fixture.previewCases) test(`preview: ${c.name}`, () => expect(chatPreview(c.input as never)).toEqual(c.expected as never))
  for (const c of fixture.timeCases) test(`time: ${c.name}`, () => expect(chatTimeLabel(c.at, c.now, c.utcOffsetMinutes)).toBe(c.expected))
  for (const c of fixture.listCases) test(`list: ${c.name}`, () => expect(chatList(c.input as never)).toEqual(c.expected as never))
  for (const c of fixture.bubbleCases) test(`bubbles: ${c.name}`, () => expect(agentBubbles(c.input as never)).toEqual(c.expected as never))
  for (const c of fixture.menuCases) test(`menu: ${c.name}`, () => expect(threadMenuEntries(c.input)).toEqual(c.expected as never))
  for (const c of fixture.agentMenuCases) test(`agent menu: ${c.name}`, () => expect(agentMenuEntries(c.input)).toEqual(c.expected as never))
  for (const c of fixture.replyCases) test(`reply: ${c.name}`, () => expect(surfaceReply(c.event)).toEqual(c.expected as never))
  for (const c of fixture.dividerCases) test(`divider: ${JSON.stringify(c.body)}`, () => expect(commandDivider(c.body)).toBe(c.expected as never))
  for (const c of fixture.agentHint.cases) test(`agent hint: ${c.name}`, () => expect(agentInstructionsChanged(c.input)).toBe(c.expected))
  for (const c of fixture.plainCases) test(`plain: ${JSON.stringify(c.text)}`, () => expect(plainLine(c.text)).toBe(c.expected))
  test(`every menu entry and tab has copy`, () => {
    for (const e of [`stop`, `stopAndDelete`, `delete`, `compact`, `newConversation`, `settings`]) expect(fixture.copy.menu[e as keyof typeof fixture.copy.menu]).toBeTruthy()
    for (const t of fixture.shell.tabs) expect(fixture.copy.tabs[t as keyof typeof fixture.copy.tabs]).toBeTruthy()
  })
})
