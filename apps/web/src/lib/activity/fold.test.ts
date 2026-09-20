import { describe, expect, it } from "vitest"
import fixture from "@exp/domain-contract/fixtures/activity-fold.json"
import {
  FOLD_WINDOW_MS,
  foldActivity,
  foldFieldKey,
  type ActivityBarrier,
  type ActivityEvent,
} from "./fold"

// EXP-900: the fold rule, locked ×4 (desktop `domain::activity_fold`, iOS
// `ActivityFoldTests`, Android `ActivityFoldTest`) against the ONE contract
// fixture — same cases, same test names. The EXP-988 contract table lives on
// as the first twelve cases of that fixture.
interface FixtureEvent {
  id: string
  issueId: string
  actorUserId: string | null
  type: string
  payload: Record<string, unknown> | null
  createdAt: string
}

interface FixtureCase {
  name: string
  events: FixtureEvent[]
  barriers: { issueId: string; actorUserId: string | null; createdAt: string }[]
  expected: Omit<FixtureEvent, `issueId` | `actorUserId`>[]
}

const cases = fixture as unknown as FixtureCase[]

function toEvent(row: FixtureEvent): ActivityEvent {
  return {
    id: row.id,
    issueId: row.issueId,
    actorUserId: row.actorUserId,
    type: row.type as ActivityEvent[`type`],
    payload: row.payload,
    createdAt: new Date(row.createdAt),
  }
}

describe(`foldActivity (contract fixture)`, () => {
  for (const c of cases) {
    it(c.name, () => {
      const barriers: ActivityBarrier[] = c.barriers.map((b) => ({
        issueId: b.issueId,
        actorUserId: b.actorUserId,
        createdAt: new Date(b.createdAt),
      }))
      const folded = foldActivity(c.events.map(toEvent), barriers).map(
        (event) => ({
          id: event.id,
          type: event.type,
          payload: event.payload,
          createdAt: new Date(event.createdAt).toISOString().replace(`.000Z`, `Z`),
        })
      )
      expect(folded).toEqual(c.expected)
    })
  }
})

describe(`foldActivity`, () => {
  const at = (minute: number) => new Date(Date.UTC(2026, 8, 19, 10, minute))
  const status = (
    id: string,
    from: string,
    to: string,
    minute: number,
    actor = `A`
  ): ActivityEvent => ({
    id,
    issueId: `i1`,
    actorUserId: actor,
    type: `status_changed`,
    payload: { fromStatusId: from, toStatusId: to },
    createdAt: at(minute),
  })

  it(`the window is ten minutes, first to last`, () => {
    expect(FOLD_WINDOW_MS).toBe(10 * 60 * 1000)
    // Exactly on the boundary still folds; one millisecond past does not.
    expect(
      foldActivity([status(`e1`, `a`, `b`, 0), status(`e2`, `b`, `a`, 10)])
    ).toEqual([])
    const late = status(`e2`, `b`, `a`, 10)
    late.createdAt = new Date(late.createdAt.getTime() + 1)
    expect(
      foldActivity([status(`e1`, `a`, `b`, 0), late]).map((e) => e.id)
    ).toEqual([`e1`, `e2`])
  })

  it(`never mutates the input rows`, () => {
    const first = status(`e1`, `a`, `b`, 0)
    const second = status(`e2`, `b`, `c`, 1)
    const input = [first, second]
    const folded = foldActivity(input)
    expect(folded).toHaveLength(1)
    expect(folded[0]).not.toBe(second)
    expect(first.payload).toEqual({ fromStatusId: `a`, toStatusId: `b` })
    expect(second.payload).toEqual({ fromStatusId: `b`, toStatusId: `c` })
    expect(input.map((e) => e.id)).toEqual([`e1`, `e2`])
  })

  it(`a barrier at the same instant as a change still separates it`, () => {
    const barrier: ActivityBarrier = {
      issueId: `i1`,
      actorUserId: `B`,
      createdAt: at(1),
    }
    expect(
      foldActivity(
        [status(`e1`, `a`, `b`, 0), status(`e2`, `b`, `a`, 1)],
        [barrier]
      ).map((e) => e.id)
    ).toEqual([`e1`, `e2`])
  })
})

describe(`foldFieldKey`, () => {
  const base = {
    id: `e`,
    issueId: `i1`,
    actorUserId: `A`,
    createdAt: new Date(0),
  }
  it(`maps every foldable type to its field and the rest to null`, () => {
    const key = (type: ActivityEvent[`type`], payload: unknown) =>
      foldFieldKey({
        ...base,
        type,
        payload: payload as ActivityEvent[`payload`],
      })
    expect(key(`status_changed`, {})).toBe(`status`)
    expect(key(`assignee_changed`, {})).toBe(`assignee`)
    expect(key(`priority_changed`, {})).toBe(`priority`)
    expect(key(`board_moved`, {})).toBe(`board`)
    expect(key(`label_added`, { labelId: `L` })).toBe(`label:L`)
    expect(key(`label_removed`, { labelId: `L` })).toBe(`label:L`)
    expect(key(`label_added`, {})).toBeNull()
    expect(
      key(`relation_added`, { type: `blocks`, relatedIssueId: `i9` })
    ).toBe(`relation:blocks:i9`)
    expect(
      key(`relation_removed`, { type: `blocks`, relatedIssueId: `i9` })
    ).toBe(`relation:blocks:i9`)
    expect(key(`relation_added`, { type: `blocks` })).toBeNull()
    expect(key(`created`, {})).toBeNull()
    expect(key(`pr_opened`, {})).toBeNull()
    expect(key(`pr_merged`, {})).toBeNull()
  })
})
