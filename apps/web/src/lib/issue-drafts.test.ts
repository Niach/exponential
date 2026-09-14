import { describe, expect, it } from "vitest"
import {
  draftTitleLabel,
  hasDraftContent,
  resolveDraftEntries,
  toDialogSeed,
  toUpsertInput,
  type DraftSnapshot,
} from "@/lib/issue-drafts"
import type { Board, IssueDraft } from "@/db/schema"

// EXP-878: the pure half of issue drafts. Everything the dialog and the list
// decide is a function here, so the rules are readable rather than implied by
// a component: what counts as content, what a close writes, what a reopened
// dialog may restore, and which rows a list may show.

const TEAM = `team-1`

function draft(overrides: Partial<IssueDraft> = {}): IssueDraft {
  return {
    id: `draft-1`,
    userId: `user-1`,
    teamId: TEAM,
    boardId: `board-1`,
    title: `A draft`,
    description: ``,
    statusId: null,
    priority: `none`,
    assigneeId: null,
    labelIds: [],
    dueDate: null,
    createdAt: new Date(`2026-01-01T00:00:00Z`),
    updatedAt: new Date(`2026-01-01T00:00:00Z`),
    ...overrides,
  } as unknown as IssueDraft
}

function board(overrides: Partial<Board> = {}): Board {
  return {
    id: `board-1`,
    teamId: TEAM,
    name: `App`,
    slug: `app`,
    prefix: `APP`,
    color: `#6366f1`,
    ...overrides,
  } as unknown as Board
}

function snapshot(overrides: Partial<DraftSnapshot> = {}): DraftSnapshot {
  return {
    id: `draft-1`,
    teamId: TEAM,
    boardId: `board-1`,
    title: ``,
    description: ``,
    statusId: null,
    priority: `none`,
    assigneeId: null,
    labelIds: [],
    dueDate: null,
    attachmentCount: 0,
    ...overrides,
  }
}

describe(`hasDraftContent`, () => {
  it(`is false for an untouched form`, () => {
    expect(
      hasDraftContent({ title: ``, description: ``, attachmentCount: 0 })
    ).toBe(false)
  })

  it(`ignores whitespace-only text`, () => {
    expect(hasDraftContent({ title: `   `, description: `\n  \n` })).toBe(false)
  })

  it(`counts a title, a description or an uploaded attachment`, () => {
    expect(hasDraftContent({ title: `x`, description: `` })).toBe(true)
    expect(hasDraftContent({ title: ``, description: `x` })).toBe(true)
    expect(
      hasDraftContent({ title: ``, description: ``, attachmentCount: 1 })
    ).toBe(true)
  })
})

describe(`toUpsertInput`, () => {
  it(`trims the title and carries the properties verbatim`, () => {
    expect(
      toUpsertInput(
        snapshot({
          title: `  padded  `,
          description: `body`,
          statusId: `status-1`,
          priority: `high`,
          assigneeId: `user-2`,
          labelIds: [`label-1`],
          dueDate: `2026-03-04`,
          attachmentCount: 2,
        })
      )
    ).toEqual({
      id: `draft-1`,
      teamId: TEAM,
      boardId: `board-1`,
      title: `padded`,
      description: `body`,
      statusId: `status-1`,
      priority: `high`,
      assigneeId: `user-2`,
      labelIds: [`label-1`],
      dueDate: `2026-03-04`,
    })
  })
})

describe(`toDialogSeed`, () => {
  const resolvable = {
    boards: [board()],
    labels: [{ id: `label-1` }],
    users: [{ id: `user-2` }],
  }

  it(`restores what still resolves`, () => {
    expect(
      toDialogSeed(
        draft({
          statusId: `status-1`,
          priority: `urgent`,
          assigneeId: `user-2`,
          labelIds: [`label-1`],
          dueDate: `2026-03-04`,
        }),
        resolvable
      )
    ).toEqual({
      id: `draft-1`,
      boardId: `board-1`,
      title: `A draft`,
      description: ``,
      statusId: `status-1`,
      priority: `urgent`,
      assigneeId: `user-2`,
      labelIds: [`label-1`],
      dueDate: `2026-03-04`,
    })
  })

  // `label_ids` carries no foreign key and a member can leave — a stale id
  // would be refused by the create, so it is dropped on the way IN instead.
  it(`drops labels and an assignee that no longer exist`, () => {
    const seed = toDialogSeed(
      draft({ assigneeId: `gone`, labelIds: [`label-1`, `deleted-label`] }),
      resolvable
    )
    expect(seed?.assigneeId).toBeNull()
    expect(seed?.labelIds).toEqual([`label-1`])
  })

  it(`refuses to seed a draft whose board is gone`, () => {
    expect(toDialogSeed(draft({ boardId: `trashed` }), resolvable)).toBeNull()
  })
})

describe(`resolveDraftEntries`, () => {
  it(`keeps this team's rows, hides an unresolvable board, newest first`, () => {
    const entries = resolveDraftEntries(
      [
        draft({
          id: `older`,
          updatedAt: new Date(`2026-01-01T00:00:00Z`),
        }),
        draft({
          id: `newer`,
          updatedAt: new Date(`2026-02-01T00:00:00Z`),
        }),
        draft({ id: `other-team`, teamId: `team-2` }),
        draft({ id: `trashed-board`, boardId: `gone` }),
      ],
      [board()],
      TEAM
    )

    expect(entries.map((entry) => entry.draft.id)).toEqual([`newer`, `older`])
    expect(entries[0].board.slug).toBe(`app`)
  })

  it(`labels an untitled draft`, () => {
    const [entry] = resolveDraftEntries([draft({ title: `  ` })], [board()], TEAM)
    expect(entry.title).toBe(draftTitleLabel)
    expect(entry.untitled).toBe(true)
  })

  it(`resolves nothing without a team`, () => {
    expect(resolveDraftEntries([draft()], [board()], undefined)).toEqual([])
  })
})
