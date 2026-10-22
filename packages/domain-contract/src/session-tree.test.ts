import { describe, expect, test } from "bun:test"

import card from "../fixtures/session-card.json" with { type: "json" }
import thread from "../fixtures/session-thread.json" with { type: "json" }
import list from "../fixtures/threads-list.json" with { type: "json" }
import {
  buildSessionThread,
  composerReplies,
  composerYieldsToCard,
  guideMenuEntries,
  guidePullRequest,
  listAgents,
  partitionThreadStrip,
  partitionThreads,
  questionOption,
  questionOptions,
  railThreads,
  sessionCard,
  threadInStack,
  threadRowMarkState,
  threadRowWord,
  threadsUnread,
  treeState,
  type ThreadStripLayout,
} from "./session-tree"

describe(`session-tree fixtures (EXP-1291)`, () => {
  for (const entry of card.cases) {
    test(`session card: ${entry.name}`, () => {
      const { session, title, now } = entry.input as never
      expect(sessionCard(session, title, now)).toEqual(entry.expected as never)
    })
  }
  test(`session card: a blocked session names its kind (copy.blockedKinds)`, () => {
    const base = card.cases.find((entry) => entry.expected.state === `blocked`)!.input as never as {
      session: Record<string, unknown>
      title: string
      now: string
    }
    const word = (blocked: Record<string, unknown>) =>
      sessionCard({ ...base.session, blocked } as never, base.title, base.now).kindWord
    for (const [kind, text] of Object.entries(card.copy.blockedKinds)) {
      expect(word({ kind })).toBe(text)
    }
    expect(word({ kind: `future_kind` })).toBe(card.copy.states.blocked)
    expect(word({ window: `weekly` })).toBe(card.copy.states.blocked)
  })
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
  for (const entry of thread.stripCases) {
    test(`strip (EXP-1334): ${entry.name}`, () => {
      // The REAL layout type: a shape drift in the fixture fails typecheck.
      expect(partitionThreadStrip(entry.input)).toEqual(entry.expected as ThreadStripLayout)
    })
  }
  for (const entry of thread.repliesCases) {
    test(`quick replies (EXP-1211): ${entry.name}`, () => {
      expect(composerReplies(entry.input as never)).toEqual(entry.expected as never)
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
  for (const entry of list.recentCases) {
    test(`recent (NEW-1): ${entry.name}`, () => {
      const groups = partitionThreads(entry.input as never)
      const recent = groups.find((group) => group.key === `recent`)
      expect(recent?.rows.filter((row) => row.depth === 0).map((row) => row.sessionId) ?? []).toEqual(
        entry.expected
      )
    })
  }
  for (const entry of list.treeStateCases) {
    test(`tree state (NEW-1): ${entry.name}`, () => {
      expect(
        treeState(entry.input.tree as never, entry.input.pullRequests as never, entry.input.now)
      ).toBe(entry.expected)
    })
  }
  for (const entry of list.agentCases) {
    test(`agents: ${entry.name}`, () => {
      const { agents, sessions, teamId, me } = entry.input as never
      expect(listAgents(agents, sessions, teamId, me)).toEqual(entry.expected as never)
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
  for (const entry of list.unreadCases) {
    test(`nav dot: ${entry.name}`, () => {
      expect(threadsUnread(entry.groups as never)).toBe(entry.expected)
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
  test(`a label normalises; anything else drops`, () => {
    expect(questionOption(`Yes`)).toEqual({ label: `Yes`, description: null, attachmentId: null })
    expect(questionOption({ label: `Yes`, description: `Ship it` })).toBeNull()
    expect(questionOption(3)).toBeNull()
  })

  // The wire is `options: string[]` + the parallel `optionDescriptions`.
  test(`questionOptions zips the optionDescriptions sidecar by index`, () => {
    expect(
      questionOptions({ options: [`Yes`, `No`, `Later`, `Never`], optionDescriptions: [`Ship it`, null, ``, `Not now`] }),
    ).toEqual([
      { label: `Yes`, description: `Ship it`, attachmentId: null },
      { label: `No`, description: null, attachmentId: null },
      { label: `Later`, description: null, attachmentId: null },
      { label: `Never`, description: `Not now`, attachmentId: null },
    ])
    // No sidecar / a short sidecar / a non-array sidecar = undescribed.
    expect(questionOptions({ options: [`A`, `B`] })).toEqual([
      { label: `A`, description: null, attachmentId: null },
      { label: `B`, description: null, attachmentId: null },
    ])
    expect(questionOptions({ options: [`A`, `B`], optionDescriptions: [`a`] })).toEqual([
      { label: `A`, description: `a`, attachmentId: null },
      { label: `B`, description: null, attachmentId: null },
    ])
    expect(questionOptions({ options: [`A`], optionDescriptions: `a`, attachmentId: null })).toEqual([{ label: `A`, description: null, attachmentId: null }])
    // The sidecar index follows the ORIGINAL position even past a dropped option.
    expect(
      questionOptions({
        options: [`Own`, 3, `Plain`, `Z`],
        optionDescriptions: [`sidecar`, `dropped`, `filled`, null],
      }),
    ).toEqual([
      { label: `Own`, description: `sidecar`, attachmentId: null },
      { label: `Plain`, description: `filled`, attachmentId: null },
      { label: `Z`, description: null, attachmentId: null },
    ])
    expect(questionOptions({})).toEqual([])
    expect(questionOptions({ options: `Yes` })).toEqual([])
  })
})
