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
  questionOption,
  questionOptions,
  railThreads,
  sessionCard,
  threadInStack,
  threadRowMarkState,
  threadRowWord,
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
      expect(buildSessionThread(entry.input as never, (entry as { options?: never }).options)).toEqual(entry.expected as never)
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
  for (const entry of list.wordCases) {
    test(`row word: ${entry.name}`, () => {
      expect(threadRowWord(entry.input)).toBe(entry.expected as never)
    })
  }
  for (const entry of list.stackCases) {
    test(`in stack: ${entry.name}`, () => {
      expect(threadInStack(entry.input.pullRequests, entry.input.teamPrs)).toBe(entry.expected)
    })
  }
  for (const entry of list.railCases) {
    test(`rail: ${entry.name}`, () => {
      const words = entry.input.words as Record<string, never>
      const rows = railThreads(entry.input.groups as never, (row) => words[row.sessionId] ?? null)
      expect(rows.map((row) => row.sessionId)).toEqual(entry.expected)
    })
  }
})

describe(`questionOption (EXP-1322)`, () => {
  test(`a label or {label, description} normalises; anything else drops`, () => {
    expect(questionOption(`Yes`)).toEqual({ label: `Yes`, description: null })
    expect(questionOption({ label: `Yes`, description: `Ship it` })).toEqual({ label: `Yes`, description: `Ship it` })
    expect(questionOption({ label: `Yes`, description: `` })).toEqual({ label: `Yes`, description: null })
    expect(questionOption({ label: `Yes` })).toEqual({ label: `Yes`, description: null })
    expect(questionOption({ description: `x` })).toBeNull()
    expect(questionOption(3)).toBeNull()
  })

  // COMPAT (EXP-1322 sidecar): old iOS 0.14.59 / Android 0.14.61 / desktop
  // 0.14.69 only read string options, so the wire is `options: string[]` +
  // the parallel `optionDescriptions`; object options stay readable.
  test(`questionOptions zips the optionDescriptions sidecar by index and keeps object tolerance`, () => {
    expect(
      questionOptions({ options: [`Yes`, `No`, `Later`, `Never`], optionDescriptions: [`Ship it`, null, ``, `Not now`] }),
    ).toEqual([
      { label: `Yes`, description: `Ship it` },
      { label: `No`, description: null },
      { label: `Later`, description: null },
      { label: `Never`, description: `Not now` },
    ])
    // No sidecar / a short sidecar / a non-array sidecar = undescribed.
    expect(questionOptions({ options: [`A`, `B`] })).toEqual([
      { label: `A`, description: null },
      { label: `B`, description: null },
    ])
    expect(questionOptions({ options: [`A`, `B`], optionDescriptions: [`a`] })).toEqual([
      { label: `A`, description: `a` },
      { label: `B`, description: null },
    ])
    expect(questionOptions({ options: [`A`], optionDescriptions: `a` })).toEqual([{ label: `A`, description: null }])
    // Objects posted before the sidecar: their own description wins; the
    // sidecar index follows the ORIGINAL position even past a dropped option.
    expect(
      questionOptions({
        options: [{ label: `Own`, description: `Mine` }, 3, { label: `Plain` }, `Z`],
        optionDescriptions: [`sidecar`, `dropped`, `filled`, null],
      }),
    ).toEqual([
      { label: `Own`, description: `Mine` },
      { label: `Plain`, description: `filled` },
      { label: `Z`, description: null },
    ])
    expect(questionOptions({})).toEqual([])
    expect(questionOptions({ options: `Yes` })).toEqual([])
  })
})
