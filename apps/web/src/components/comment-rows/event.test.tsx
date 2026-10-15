import { render } from "@testing-library/react"
import { describe, expect, it } from "vitest"
import type { IssueEvent, Label, User } from "@/db/schema"
import { EventRow } from "@/components/comment-rows/event"

// EXP-723: an activity feed where only the creation row is dated reads as a
// bug, so every event line ends in its own relative time. The other rule this
// pins is older than that: `created` rows are the automation substrate and the
// header already shows creation, so the timeline suppresses them (EXP-530).

const HOUR = 60 * 60 * 1000

function event(overrides: Partial<IssueEvent>): IssueEvent {
  return {
    id: `e1`,
    issueId: `i1`,
    teamId: `t1`,
    boardId: `b1`,
    boardDeletedAt: null,
    boardArchivedAt: null,
    actorUserId: `u1`,
    type: `label_added`,
    payload: { labelId: `l1` },
    createdAt: new Date(Date.now() - 2 * HOUR),
    updatedAt: new Date(Date.now() - 2 * HOUR),
    ...overrides,
  } as IssueEvent
}

const userMap = new Map<string, User>([
  [`u1`, { id: `u1`, name: `Ada Lovelace`, email: `ada@example.com` } as User],
])
const labelMap = new Map<string, Label>([
  [`l1`, { id: `l1`, name: `bug` } as Label],
])

function line(node: React.ReactElement): string {
  const { container } = render(node)
  return container.textContent ?? ``
}

/** EXP-1324: the row is a Trace line — the time is its description. */
function when(node: React.ReactElement): string | null {
  const { container } = render(node)
  return (
    container.querySelector(`[data-slot="trace-description"]`)?.textContent ??
    null
  )
}

describe(`EventRow`, () => {
  it(`ends the phrase with a relative time`, () => {
    const text = line(
      <EventRow event={event({})} userMap={userMap} labelMap={labelMap} />
    )
    expect(text).toContain(`Ada Lovelace`)
    expect(text).toContain(`added label`)
    expect(text).toContain(`bug`)
    expect(
      when(<EventRow event={event({})} userMap={userMap} labelMap={labelMap} />)
    ).toBe(`2 hours ago`)
  })

  it(`is a Trace line at size sm: icon, sentence title, time description`, () => {
    const { container } = render(
      <EventRow event={event({})} userMap={userMap} labelMap={labelMap} />
    )
    expect(container.querySelector(`[data-slot="trace"]`)?.getAttribute(`data-size`)).toBe(`sm`)
    expect(container.querySelector(`[data-slot="trace-icon"] svg`)).not.toBeNull()
    expect(container.querySelector(`[data-slot="trace-title"]`)?.textContent).toBe(
      `Ada Lovelace added label bug`
    )
  })

  it(`dates every type the same way`, () => {
    for (const type of [
      `assignee_changed`,
      `priority_changed`,
      `pr_opened`,
      `pr_merged`,
    ] as const) {
      const time = when(
        <EventRow
          event={event({ type, payload: {} })}
          userMap={userMap}
          labelMap={labelMap}
        />
      )
      expect(time === `2 hours ago` ? type : `${type} has no time`).toBe(type)
    }
  })

  it(`names a name-less status change through the status resolver (F47)`, () => {
    // Outside a provider the resolver is the constructed builtin set, so the
    // legacy anchor reads as the row's display name, never a lowercase munge.
    const text = line(
      <EventRow
        event={event({
          type: `status_changed`,
          payload: { from: `backlog`, to: `in_progress` },
        })}
        userMap={userMap}
        labelMap={labelMap}
      />
    )
    expect(text).toContain(`changed status from Backlog to In Progress`)
  })

  it(`still renders nothing for created and for unknown types`, () => {
    const created = render(
      <EventRow
        event={event({ type: `created`, payload: {} })}
        userMap={userMap}
        labelMap={labelMap}
      />
    )
    expect(created.container.firstChild).toBeNull()

    const unknown = render(
      <EventRow
        event={event({ type: `not_a_real_type` as IssueEvent[`type`] })}
        userMap={userMap}
        labelMap={labelMap}
      />
    )
    expect(unknown.container.firstChild).toBeNull()
  })
})
