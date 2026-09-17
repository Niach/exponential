// EXP-916: the fixture is the contract. Every client replays
// `fixtures/feed/edit-cards.json` through ITS OWN feed-row projection (web
// `groupFeedRows`, desktop `group_feed_rows`, iOS `AgentFeedRow.rows`, Android
// `groupFeedRows`) plus its `editCard`, with THESE test names. This file
// replays it through a REFERENCE projection that knows only the item kinds
// the fixture uses, so the grouping rule is spelled out once, here.
//
// A case: `feed` (items with `id`, `kind`, and for tools `toolKind`,
// `detail`, `settled`, `failed`, `diff`, `workflowId`, `subagentId`), an
// optional `start` (the render window's first index, EXP-783), an optional
// `lane` (project THAT subagent's items instead of the main lane), an optional
// `live` (the id `liveToolRowId` would name), and `expected` = one string per
// row:
//   `narration@id` · `user@id` · `tool@id` (a lone non-edit tool, or a
//   workflow call) · `run@id[ids]` (a tool run) · `subagent@id(lane)` ·
//   `card@id[ids]: <renderEditCard>`.

import { describe, expect, test } from "bun:test"

import cases from "../fixtures/feed/edit-cards.json" with { type: "json" }
import {
  editCard,
  editCardMoreLabel,
  editCardTitle,
  editRunEnd,
  isEditCall,
  renderEditCard,
  EDIT_CARD_PREVIEW,
  type EditCardFeedItem,
} from "./edit-card"

interface Case {
  name: string
  feed: (EditCardFeedItem & { text?: string })[]
  start?: number
  lane?: string
  live?: number
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
  const live = item.live ?? null
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
    if (isEditCall(row)) {
      const end = editRunEnd(feed, i)
      const members = feed.slice(i, end + 1)
      const view = editCard(members, live)
      // EXP-938: a run whose every member is dropped is no card at all.
      if (view.rows.length > 0) {
        rows.push(
          `card@${row.id}[${members.map((m) => m.id).join(`,`)}]: ${renderEditCard(view)}`
        )
      }
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
        !isEditCall(feed[end + 1])
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

describe(`edited-files cards (EXP-916)`, () => {
  test(`every fixture case projects byte-exact`, () => {
    for (const item of fixture) {
      expect({ name: item.name, rows: project(item) }).toEqual({
        name: item.name,
        rows: item.expected,
      })
    }
  })

  test(`the fixture covers a split, a live row, a stub, a lane and a window`, () => {
    const names = fixture.map((item) => item.name).join(`\n`)
    expect(names).toContain(`splits`)
    expect(names).toContain(`live`)
    expect(names).toContain(`failed`)
    expect(names).toContain(`subagent`)
    expect(names).toContain(`window`)
  })

  test(`the rule reads only kind, toolKind and workflowId`, () => {
    expect(isEditCall({ kind: `tool`, toolKind: `edit` })).toBe(true)
    expect(isEditCall({ kind: `tool`, toolKind: `delete` })).toBe(true)
    expect(isEditCall({ kind: `tool`, toolKind: `move` })).toBe(true)
    expect(isEditCall({ kind: `tool`, toolKind: `read` })).toBe(false)
    expect(isEditCall({ kind: `tool` })).toBe(false)
    expect(isEditCall({ kind: `tool`, toolKind: `edit`, workflowId: `w` })).toBe(false)
    expect(isEditCall({ kind: `narration`, toolKind: `edit` })).toBe(false)
  })

  test(`the copy is the contract's`, () => {
    expect(editCardTitle(1)).toBe(`1 file edited`)
    expect(editCardTitle(4)).toBe(`4 files edited`)
    expect(EDIT_CARD_PREVIEW).toBe(5)
    expect(editCardMoreLabel(5)).toBeNull()
    expect(editCardMoreLabel(8)).toBe(`3 more`)
  })

  test(`a live row is only the card's LAST member`, () => {
    const items: EditCardFeedItem[] = [
      { id: 1, kind: `tool`, toolKind: `edit`, detail: `a.ts` },
      { id: 2, kind: `tool`, toolKind: `edit`, detail: `b.ts` },
    ]
    expect(editCard(items, 1).liveIndex).toBeNull()
    expect(editCard(items, 2).liveIndex).toBe(1)
    expect(editCard(items, null).liveIndex).toBeNull()
  })
})
