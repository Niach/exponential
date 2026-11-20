import { describe, expect, test } from "bun:test"
import fixture from "../fixtures/find-in-chat.json" with { type: "json" }
import {
  findInChatCounter,
  findInChatKeep,
  findInChatKey,
  findInChatLand,
  findInChatMenu,
  findInChatOlder,
  findInChatQuery,
  findInChatStatus,
  findInChatStep,
  findInChatStops,
  type FindSearchPhase,
  type FindStep,
} from "./find-in-chat"

describe(`#EDE-129 fixtures/find-in-chat.json`, () => {
  for (const c of fixture.menuCases) test(`menu: ${c.name}`, () => expect(findInChatMenu(c.kind, c.entries)).toEqual(c.expected))
  for (const c of fixture.queryCases) test(`query: ${c.name}`, () => expect(findInChatQuery(c.raw)).toBe(c.query))
  for (const c of fixture.matchCases) test(`stops: ${c.name}`, () => expect(findInChatStops(c.rows, c.older)).toEqual(c.expected))
  for (const c of fixture.olderCases)
    test(`older: ${c.name}`, () =>
      expect(findInChatOlder({ sessionId: c.sessionId, lowestSeq: c.lowestSeq, store: c.store, device: c.device, synced: c.synced })).toEqual(c.expected))
  for (const c of fixture.counterCases) test(`counter: ${c.name}`, () => expect(findInChatCounter(c)).toBe(c.expected))
  for (const c of fixture.stepCases) test(`step: ${c.name}`, () => expect(findInChatStep(c.current, c.total, c.step as FindStep)).toBe(c.expected))
  for (const c of fixture.resultCases) test(`keep: ${c.name}`, () => expect(findInChatKeep(c.before, c.current, c.after)).toBe(c.expected))
  for (const c of fixture.landCases) test(`land: ${c.name}`, () => expect(findInChatLand(c.match)).toEqual(c.expected as ReturnType<typeof findInChatLand>))
  for (const c of fixture.statusCases)
    test(`status: ${c.name}`, () =>
      expect(findInChatStatus({ store: c.input.store as FindSearchPhase, device: c.input.device as FindSearchPhase, deviceName: c.input.deviceName })).toBe(c.expected))
  for (const c of fixture.keyCases) test(`key: ${c.name}`, () => expect(findInChatKey(c.input)).toBe(c.expected as ReturnType<typeof findInChatKey>))
})
