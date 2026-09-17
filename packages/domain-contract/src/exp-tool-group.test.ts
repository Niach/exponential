// EXP-948: the fixture is the contract. Every client replays
// `fixtures/feed/exp-tool-groups.json` through ITS OWN feed-row projection
// (web `groupFeedRows`, desktop `group_feed_rows`, iOS `AgentFeedRow.rows`,
// Android `groupFeedRows`), with THESE test names. This file replays it
// through a REFERENCE projection that knows only the item kinds the fixture
// uses, so the grouping rule is spelled out once, here.
//
// A case: `feed` (items with `id`, `kind`, and for tools `name`, `toolKind`,
// `detail`, `settled`, `failed`, `workflowId`, `subagentId`), an optional
// `start` (the render window's first index, EXP-783), an optional `lane`
// (project THAT subagent's items instead of the main lane), and `expected` =
// one string per row:
//   `narration@id` · `user@id` · `tool@id` (a lone tool, one of ours included,
//   or a workflow call) · `run@id[ids]` (a generic tool run) ·
//   `expRun@id[ids]: <caption>` (a run of the SAME Exponential tool) ·
//   `subagent@id(lane)`.

import { describe, expect, test } from "bun:test"

import cases from "../fixtures/feed/exp-tool-groups.json" with { type: "json" }
import {
  expToolGroupCaption,
  expToolRowName,
  expToolRunEnd,
  isExpToolCall,
  type ExpToolFeedItem,
} from "./exp-tool-group"

interface Case {
  name: string
  feed: (ExpToolFeedItem & { toolKind?: string; detail?: string; text?: string })[]
  start?: number
  lane?: string
  expected: string[]
}

const fixture = cases as unknown as Case[]

/** The reference projection — the fixture's own grouping rule. */
function project(item: Case): string[] {
  const lane = item.lane
  const feed =
    lane === undefined
      ? item.feed
      : item.feed.filter((row) => row.subagentId === lane)
  const rows: string[] = []
  const seenLanes = new Set<string>()
  for (let i = item.start ?? 0; i < feed.length; i++) {
    const row = feed[i]
    if (lane === undefined && row.subagentId !== undefined) {
      if (!seenLanes.has(row.subagentId)) {
        seenLanes.add(row.subagentId)
        rows.push(`subagent@${row.id}(${row.subagentId})`)
      }
      continue
    }
    if (isExpToolCall(row)) {
      const end = expToolRunEnd(feed, i)
      const members = feed.slice(i, end + 1)
      rows.push(
        end === i
          ? `tool@${row.id}`
          : `expRun@${row.id}[${members.map((m) => m.id).join(`,`)}]: ${expToolGroupCaption(members)}`
      )
      i = end
      continue
    }
    if (row.kind === `tool` && row.workflowId === undefined) {
      let end = i
      while (
        end + 1 < feed.length &&
        feed[end + 1].kind === `tool` &&
        feed[end + 1].workflowId === undefined &&
        feed[end + 1].subagentId === lane &&
        // EXP-948: one of OURS breaks a generic run and never joins it.
        !isExpToolCall(feed[end + 1])
      ) {
        end++
      }
      rows.push(
        end === i
          ? `tool@${row.id}`
          : `run@${row.id}[${feed
              .slice(i, end + 1)
              .map((m) => m.id)
              .join(`,`)}]`
      )
      i = end
      continue
    }
    if (row.kind === `tool`) {
      rows.push(`tool@${row.id}`)
      continue
    }
    rows.push(`${row.kind === `user_message` ? `user` : row.kind}@${row.id}`)
  }
  return rows
}

describe(`Exponential tool groups (EXP-948)`, () => {
  test(`every fixture case projects byte-exact`, () => {
    for (const item of fixture) {
      expect({ name: item.name, rows: project(item) }).toEqual({
        name: item.name,
        rows: item.expected,
      })
    }
  })

  test(`the fixture covers a split, a run break, a failure, a lane and a window`, () => {
    const names = fixture.map((item) => item.name).join(`\n`)
    expect(names).toContain(`splits`)
    expect(names).toContain(`single`)
    expect(names).toContain(`running`)
    expect(names).toContain(`failed`)
    expect(names).toContain(`subagent`)
    expect(names).toContain(`window`)
  })

  test(`the rule reads only kind, name and workflowId`, () => {
    expect(isExpToolCall({ kind: `tool`, name: `exponential_issues_get` })).toBe(true)
    expect(
      isExpToolCall({
        kind: `tool`,
        name: `mcp__exponential__exponential_issues_get`,
      })
    ).toBe(true)
    expect(isExpToolCall({ kind: `tool`, name: `Bash` })).toBe(false)
    expect(isExpToolCall({ kind: `tool`, name: `issues_get` })).toBe(false)
    expect(isExpToolCall({ kind: `tool`, name: `mcp__linear__issues_get` })).toBe(false)
    expect(isExpToolCall({ kind: `tool` })).toBe(false)
    expect(
      isExpToolCall({
        kind: `tool`,
        name: `exponential_issues_get`,
        workflowId: `w`,
      })
    ).toBe(false)
    expect(isExpToolCall({ kind: `narration`, name: `exponential_issues_get` })).toBe(
      false
    )
    expect(expToolRowName(`  exponential.exponential_issues_list  `)).toBe(`issues_list`)
    expect(expToolRowName(`exponential_issues_invented`)).toBeNull()
  })

  test(`the copy is the contract's`, () => {
    const read = (n: number, settled: boolean) =>
      expToolGroupCaption(
        Array.from({ length: n }, () => ({ name: `exponential_issues_get`, settled }))
      )
    expect(read(3, true)).toBe(`Read 3 issues`)
    expect(read(3, false)).toBe(`Reading 3 issues`)
    expect(read(21, true)).toBe(`Read 21 issues`)
    expect(
      expToolGroupCaption([
        { name: `exponential_issues_get`, settled: true },
        { name: `exponential_issues_get`, settled: true, failed: true },
      ])
    ).toBe(`Read 2 issues · 1 failed`)
    expect(
      expToolGroupCaption([
        { name: `exponential_issues_list`, settled: true },
        { name: `exponential_issues_list`, settled: true },
      ])
    ).toBe(`Listed issues 2 times`)
    expect(expToolGroupCaption([{ name: `Bash`, settled: true }])).toBe(``)
  })
})
