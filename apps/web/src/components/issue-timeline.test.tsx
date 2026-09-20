import { fireEvent, render, screen } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"
import type { Comment, Issue, IssueEvent, User } from "@/db/schema"
import { forgetTabMemory, issueMemoryOwner } from "@/lib/work-tab-memory"
import { IssueTimeline } from "@/components/issue-timeline"

// EXP-900/EXP-468: the timeline folds its events at read time and the header
// grows a "Show all" toggle only when the fold actually hid something. The
// fold rule itself is fixture-locked in lib/activity/fold.test.ts; this pins
// the wiring — comments enter as barriers, `created` rows stay hidden either
// way, the toggle flips the raw list back in and the "(N)" count follows.

vi.mock(`@/lib/trpc-client`, () => ({ trpc: {} }))
vi.mock(`@/lib/collections`, () => ({
  attachmentCollection: { name: `attachments` },
  commentCollection: { name: `comments` },
  issueEventCollection: { name: `e` },
  labelCollection: { name: `labels` },
  boardCollection: { name: `boards` },
}))
vi.mock(`@/components/comment-composer`, () => ({
  CommentComposer: () => null,
}))
vi.mock(`@/components/comment-rows/regular`, () => ({
  RegularCommentRow: ({ comment }: { comment: Comment }) => (
    <div data-testid="comment-row">{comment.id}</div>
  ),
}))
vi.mock(`@/components/comment-rows/event`, () => ({
  EventRow: ({ event }: { event: IssueEvent }) => (
    <div data-testid="event-row">{`${event.id}:${JSON.stringify(event.payload)}`}</div>
  ),
}))

// One fake live query per collection: the builder records the `.from()` alias
// and hands back whatever the test staged for it.
const staged = vi.hoisted(() => ({ rows: new Map<string, unknown[]>() }))
vi.mock(`@tanstack/react-db`, async (importOriginal) => {
  const actual = await importOriginal<typeof import("@tanstack/react-db")>()
  return {
    ...actual,
    useLiveQuery: (build: (query: unknown) => unknown) => {
      let alias = ``
      const builder: Record<string, unknown> = {}
      for (const method of [`from`, `where`, `orderBy`]) {
        builder[method] = (arg: unknown) => {
          if (method === `from`) alias = Object.keys(arg as object)[0]
          return builder
        }
      }
      build(builder)
      return { data: staged.rows.get(alias) ?? [] }
    },
  }
})

const T0 = Date.UTC(2026, 8, 19, 10, 0)
const at = (minute: number) => new Date(T0 + minute * 60_000)

function event(
  id: string,
  type: IssueEvent[`type`],
  payload: Record<string, unknown>,
  minute: number,
  actor = `A`
): IssueEvent {
  return {
    id,
    issueId: `i1`,
    teamId: `t1`,
    boardId: `b1`,
    boardDeletedAt: null,
    boardArchivedAt: null,
    actorUserId: actor,
    type,
    payload,
    createdAt: at(minute),
    updatedAt: at(minute),
  } as IssueEvent
}

function comment(id: string, author: string, minute: number): Comment {
  return {
    id,
    issueId: `i1`,
    teamId: `t1`,
    boardId: `b1`,
    authorId: author,
    parentId: null,
    body: `no`,
    kind: `regular`,
    source: `user`,
    createdAt: at(minute),
    updatedAt: at(minute),
  } as unknown as Comment
}

const issue = {
  id: `i1`,
  identifier: `EXP-1`,
  source: `user`,
  creatorId: `A`,
  createdAt: at(-60),
} as unknown as Issue

const users: User[] = [
  { id: `A`, name: `Ada`, email: `ada@example.com` } as User,
  { id: `B`, name: `Bob`, email: `bob@example.com` } as User,
]

function renderTimeline() {
  return render(
    <IssueTimeline issue={issue} currentUserId="A" users={users} hideComposer />
  )
}

const roundTrip = [
  event(`e0`, `created`, { status: `backlog` }, -60),
  event(`e1`, `status_changed`, { fromStatusId: `s1`, toStatusId: `s2` }, 0),
  event(`e2`, `status_changed`, { fromStatusId: `s2`, toStatusId: `s1` }, 2),
]

describe(`IssueTimeline activity fold`, () => {
  beforeEach(() => {
    staged.rows.clear()
    // The toggle is remembered per issue across tab switches (EXP-894).
    forgetTabMemory(issueMemoryOwner(`i1`))
  })

  it(`hides a same-actor round trip and offers Show all`, () => {
    staged.rows.set(`e`, roundTrip)
    renderTimeline()
    expect(screen.queryAllByTestId(`event-row`)).toHaveLength(0)
    expect(screen.getByText(`Activity`)).toBeTruthy()
    const toggle = screen.getByTestId(`activity-show-all`)
    expect(toggle.textContent).toBe(`Show all`)

    fireEvent.click(toggle)
    const rows = screen.getAllByTestId(`event-row`).map((n) => n.textContent)
    expect(rows).toEqual([
      `e1:{"fromStatusId":"s1","toStatusId":"s2"}`,
      `e2:{"fromStatusId":"s2","toStatusId":"s1"}`,
    ])
    expect(screen.getByTestId(`activity-show-all`).textContent).toBe(`Show less`)
    // The count follows what is on screen; `created` never counts.
    expect(screen.getByText(`Activity (2)`)).toBeTruthy()
  })

  it(`a comment by another actor in between keeps both moves`, () => {
    staged.rows.set(`e`, roundTrip)
    staged.rows.set(`comments`, [comment(`c1`, `B`, 1)])
    renderTimeline()
    expect(screen.getAllByTestId(`event-row`)).toHaveLength(2)
    expect(screen.getAllByTestId(`comment-row`)).toHaveLength(1)
    expect(screen.queryByTestId(`activity-show-all`)).toBeNull()
    expect(screen.getByText(`Activity (3)`)).toBeTruthy()
  })

  it(`a chain shows as one net row and the toggle stays available`, () => {
    staged.rows.set(`e`, [
      event(`e1`, `status_changed`, { fromStatusId: `a`, toStatusId: `b` }, 0),
      event(`e2`, `status_changed`, { fromStatusId: `b`, toStatusId: `c` }, 1),
    ])
    renderTimeline()
    expect(screen.getAllByTestId(`event-row`).map((n) => n.textContent)).toEqual(
      [`e2:{"toStatusId":"c","fromStatusId":"a"}`]
    )
    expect(screen.getByText(`Activity (1)`)).toBeTruthy()
    expect(screen.getByTestId(`activity-show-all`).textContent).toBe(`Show all`)
  })

  it(`offers no toggle when nothing folded`, () => {
    staged.rows.set(`e`, [
      event(`e1`, `status_changed`, { fromStatusId: `a`, toStatusId: `b` }, 0),
      event(`e2`, `label_added`, { labelId: `L` }, 1),
    ])
    renderTimeline()
    expect(screen.getAllByTestId(`event-row`)).toHaveLength(2)
    expect(screen.queryByTestId(`activity-show-all`)).toBeNull()
  })
})
