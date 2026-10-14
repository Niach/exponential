import { describe, expect, test } from "bun:test"

import card from "../fixtures/session-card.json" with { type: "json" }
import thread from "../fixtures/session-thread.json" with { type: "json" }
import list from "../fixtures/threads-list.json" with { type: "json" }
import {
  buildSessionThread,
  composerYieldsToCard,
  guideMenuEntries,
  guidePullRequest,
  listAgents,
  partitionThreads,
  sessionCard,
  threadRowMarkState,
} from "./session-tree"

describe(`session-tree fixtures (EXP-1291)`, () => {
  for (const entry of card.cases) {
    test(`session card: ${entry.name}`, () => {
      const { session, ordinal, now } = entry.input as never
      expect(sessionCard(session, ordinal, now)).toEqual(entry.expected as never)
    })
  }
  for (const entry of thread.cases) {
    test(`thread: ${entry.name}`, () => {
      expect(buildSessionThread(entry.input as never)).toEqual(entry.expected as never)
    })
  }
  for (const entry of thread.guidePullRequestCases) {
    test(`guide pull request: ${entry.name}`, () => {
      expect(guidePullRequest(entry.pullRequests)?.id ?? null).toBe(entry.expected)
    })
  }
  for (const entry of thread.guideMenuCases) {
    test(`guide menu: ${entry.name}`, () => {
      expect(guideMenuEntries(entry.input)).toEqual(entry.expected)
    })
  }
  for (const entry of thread.composerCases) {
    test(`composer: ${entry.name}`, () => {
      expect(composerYieldsToCard(entry.input)).toBe(entry.expected)
    })
  }
  for (const entry of list.cases) {
    test(`threads list: ${entry.name}`, () => {
      expect(partitionThreads(entry.input as never)).toEqual(entry.expected as never)
    })
  }
  for (const entry of list.agentCases) {
    test(`agents: ${entry.name}`, () => {
      const { sessions, teamId } = entry.input as never
      expect(listAgents(sessions, teamId)).toEqual(entry.expected as never)
    })
  }
  for (const entry of list.markCases) {
    test(`row mark: ${entry.name}`, () => {
      expect(threadRowMarkState(entry.input)).toBe(entry.expected as never)
    })
  }
})
