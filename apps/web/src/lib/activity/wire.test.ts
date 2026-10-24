import { describe, expect, it } from "vitest"
import { activityWire, type ActivityRow } from "@/lib/activity/wire"

const AT = new Date(`2026-10-12T10:00:00.000Z`)

function row(over: Partial<ActivityRow>): ActivityRow {
  return {
    id: `e1`,
    type: `title_changed`,
    payload: null,
    actorUserId: `u1`,
    viaMcp: false,
    sessionId: null,
    createdAt: AT,
    ...over,
  }
}

describe(`activityWire`, () => {
  it(`maps a rename to a title change by a person`, () => {
    expect(
      activityWire(row({ payload: { from: `Old`, to: `New` } }))
    ).toEqual({
      id: `e1`,
      type: `title_changed`,
      field: `title`,
      from: `Old`,
      to: `New`,
      actorId: `u1`,
      actorKind: `user`,
      sessionId: null,
      at: `2026-10-12T10:00:00.000Z`,
      payload: { from: `Old`, to: `New` },
    })
  })

  it(`reads status rows, falling back to the legacy enum`, () => {
    const precise = activityWire(
      row({
        type: `status_changed`,
        payload: { fromStatusId: `s1`, toStatusId: `s2`, toName: `Done` },
      })
    )
    expect([precise.field, precise.from, precise.to]).toEqual([`status`, `s1`, `s2`])
    const legacy = activityWire(
      row({ type: `status_changed`, payload: { from: `backlog`, to: `done` } })
    )
    expect([legacy.from, legacy.to]).toEqual([`backlog`, `done`])
  })

  it(`maps label and field changes`, () => {
    const added = activityWire(row({ type: `label_added`, payload: { labelId: `l1` } }))
    expect([added.field, added.from, added.to]).toEqual([`label`, null, `l1`])
    const removed = activityWire(row({ type: `label_removed`, payload: { labelId: `l1` } }))
    expect([removed.field, removed.from, removed.to]).toEqual([`label`, `l1`, null])
    const assignee = activityWire(
      row({ type: `assignee_changed`, payload: { from: null, to: `u2` } })
    )
    expect([assignee.field, assignee.from, assignee.to]).toEqual([`assignee`, null, `u2`])
    const priority = activityWire(
      row({ type: `priority_changed`, payload: { from: `low`, to: `high` } })
    )
    expect(priority.field).toBe(`priority`)
    expect(activityWire(row({ type: `created`, payload: {} })).field).toBeNull()
  })

  it(`tells agents, runs and automation apart`, () => {
    const agent = activityWire(row({ viaMcp: true, sessionId: `run1` }))
    expect([agent.actorKind, agent.sessionId]).toEqual([`agent`, `run1`])
    expect(activityWire(row({ actorUserId: null })).actorKind).toBe(`automation`)
    // The PR automation's status flip names the PR author as actor.
    expect(
      activityWire(
        row({
          type: `status_changed`,
          viaMcp: true,
          payload: { toStatusId: `s2`, automation: true },
        })
      ).actorKind
    ).toBe(`automation`)
  })
})

// #EXP-1383: a team agent's session acts as the agent; an agent assignee
// reads as the picker's `agent:<id>`.
describe(`activityWire — agents`, () => {
  it(`names the acting agent and reports it as an agent actor`, () => {
    const wire = activityWire(
      row({ viaMcp: true, sessionId: `s1`, actorAgentId: `g1`, actorAgentName: `Sloppy`, payload: { from: `A`, to: `B` } })
    )
    expect(wire).toMatchObject({ actorId: `u1`, actorKind: `agent`, agent: { id: `g1`, name: `Sloppy` }, sessionId: `s1` })
    expect(`agent` in activityWire(row({}))).toBe(false)
  })

  it(`reads an agent assignee as agent:<id>`, () => {
    expect(
      activityWire(
        row({ type: `assignee_changed`, payload: { from: `u2`, to: null, fromAgentId: null, toAgentId: `g1` } })
      )
    ).toMatchObject({ field: `assignee`, from: `u2`, to: `agent:g1` })
    expect(
      activityWire(row({ type: `assignee_changed`, payload: { from: null, to: `u2`, fromAgentId: `g1`, toAgentId: null } }))
    ).toMatchObject({ from: `agent:g1`, to: `u2` })
  })
})
