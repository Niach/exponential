import { describe, expect, it, vi } from "vitest"

vi.mock(`@/db/connection`, () => ({ db: {} }))

import { contract } from "@exp/domain-contract"
import { SESSION_TREE_MAX_DEPTH, SESSION_TREE_MAX_SESSIONS } from "@exp/db-schema/domain"
import * as spec from "@exp/eagent/copy"
import reuse from "@exp/eagent/fixtures/reuse.json"
import watchdog from "@exp/eagent/fixtures/watchdog.json"
import { ASSISTANT_SILENT_EVENTS } from "@/lib/agents/assistant-chat"
import { TRIGGER_COPY } from "@/lib/agents/triggers"
import { agentReuseRefusal } from "./agent-reuse"
import { askHiddenText, askText, BUS_COPY, privateText, REPORT_FALLBACK } from "./bus"
import { BACKGROUND_STUCK_MINUTES, silentMinutes } from "./watchdog"
import {
  PRIVATE_PR_LEAD,
  PRIVATE_REPLY_LEAD,
  PRIVATE_REPORT_LEAD,
} from "@exp/domain-contract/chats"

// #EXP-1508: the eAgent orchestration spec (packages/eagent/fixtures) is
// derived from this server. It fails when the server's thresholds, copy or
// pure rules drift from it; the scenarios themselves replay against the TS
// reference (packages/eagent) and eagent-hub-core (#EXP-1509).

const { rules } = spec

describe(`eagent spec: the server's thresholds`, () => {
  it(`caps`, () => {
    expect(rules.caps).toMatchObject({ treeMaxDepth: SESSION_TREE_MAX_DEPTH, treeMaxSessions: SESSION_TREE_MAX_SESSIONS })
    expect(rules.caps).toMatchObject({
      treeMaxDepth: contract.sessionsSend.treeMaxDepth,
      treeMaxSessions: contract.sessionsSend.treeMaxSessions,
    })
  })
  it(`reuse`, () => {
    expect(rules.reuse).toMatchObject({
      maxEndedHours: contract.sessionsSend.reuseMaxEndedHours,
      maxIdleHours: contract.sessionsSend.reuseMaxIdleHours,
      maxChildren: contract.sessionsSend.reuseMaxChildren,
    })
  })
  it(`watchdog`, () => {
    const { $comment: _, ...minutes } = contract.sessionWatchdog as typeof contract.sessionWatchdog & { $comment?: string }
    expect(rules.watchdog).toMatchObject({
      ...minutes,
      backgroundStuckMinutes: BACKGROUND_STUCK_MINUTES,
      onlineWindowSeconds: contract.device.onlineWindowSeconds,
    })
  })
  it(`triggers, reports, silent events`, () => {
    expect(rules.trigger.catchUpHours).toBe(contract.agentTrigger.catchUpHours)
    expect(rules.report.fallback).toBe(REPORT_FALLBACK)
    expect([...rules.assistantSilentEvents].sort()).toEqual([...ASSISTANT_SILENT_EVENTS].sort())
  })
})

describe(`eagent spec: the server's copy`, () => {
  it(`bus lines`, () => {
    expect(rules.copy.claimed).toBe(BUS_COPY.session_claimed)
    expect(spec.unclaimedText()).toBe(BUS_COPY.session_unclaimed)
    expect(spec.unresumedText()).toBe(BUS_COPY.session_unresumed)
    expect(spec.stuckText(42)).toBe(BUS_COPY.session_stuck(42))
    expect(spec.prOpenedText(7, `https://x.test/7`)).toBe(BUS_COPY.pr_opened(7, `https://x.test/7`))
    for (const blocked of [
      { window: `weekly`, resetsAt: `2026-11-03T19:00:00.000Z` },
      { window: `session`, resetsAt: null },
      { window: null, resetsAt: `2026-11-03T19:00:00.000Z` },
      { window: null, resetsAt: null },
    ]) {
      expect(spec.blockedText(blocked)).toBe(
        BUS_COPY.session_blocked({ kind: `rate_limit`, agent: null, since: ``, ...blocked })
      )
    }
  })
  it(`private leads`, () => {
    expect(rules.copy.privateReplyLead).toBe(PRIVATE_REPLY_LEAD)
    expect(rules.copy.privateReportLead).toBe(PRIVATE_REPORT_LEAD)
    expect(rules.copy.privatePrLead).toBe(PRIVATE_PR_LEAD)
    expect(spec.privateText(`s1`, PRIVATE_REPLY_LEAD)).toBe(privateText({ id: `s1` }, PRIVATE_REPLY_LEAD))
  })
  it(`ask text`, () => {
    const cards = [
      { kind: `question` as const, question: `Which?`, options: [`A`, `B`], multi: true },
      { kind: `question` as const, question: `Ready?`, custom: false },
      { kind: `question` as const, question: `Name?` },
      {
        kind: `plan` as const,
        plan: `x`.repeat(2500),
        questions: [{ id: `db`, prompt: `Database?`, options: [{ label: `Postgres` }, { label: `SQLite` }] }],
      },
      { kind: `plan` as const, plan: `Short plan.` },
    ]
    for (const card of cards) {
      expect(spec.askText(`s1`, card)).toBe(askText({ id: `s1` }, card as never))
      expect(spec.askHiddenText(`s1`, card)).toBe(askHiddenText({ id: `s1` }, card as never))
    }
  })
  it(`trigger lines`, () => {
    expect(spec.scheduleText(`0 * * * *`, null, false)).toBe(TRIGGER_COPY.schedule(`0 * * * *`, null, false))
    expect(spec.scheduleText(`0 9 * * 1-5`, `Europe/Vienna`, true)).toBe(TRIGGER_COPY.schedule(`0 9 * * 1-5`, `Europe/Vienna`, true))
    for (const emittedBy of [`session`, `user`, `server`] as const) {
      const payload = { sessionId: `s1`, nested: { a: [1, 2] } }
      expect(spec.eventText(`session.ended`, emittedBy, payload)).toBe(
        TRIGGER_COPY.event({ name: `session.ended`, emittedBy, payload } as never)
      )
    }
  })
})

describe(`eagent spec: the server's pure rules replay the fixtures`, () => {
  for (const entry of reuse.refusalCases) {
    it(`reuse: ${entry.name}`, () => {
      const at = (ms: number | null) => (ms === null ? null : new Date(ms))
      const root = {
        status: entry.root.status as `idle`,
        needsInput: entry.root.needsInput,
        endedAt: at(entry.root.endedAt),
        lastTurnAt: at(entry.root.lastTurnAt),
        lastActivityAt: at(entry.root.lastActivityAt),
        updatedAt: new Date(entry.root.updatedAt),
      }
      expect(agentReuseRefusal(root, entry.children, new Date(entry.now), entry.awaitsCaller ?? false)).toBe(entry.expected)
    })
  }
  for (const entry of watchdog.silentCases) {
    it(`silent minutes ${entry.alive} → ${entry.expected}`, () => {
      expect(silentMinutes(entry.alive ?? NaN, new Date(entry.now))).toBe(entry.expected)
    })
  }
})
