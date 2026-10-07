import { act, renderHook } from "@testing-library/react"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"
import type { Board, IssueDraft, User } from "@/db/schema"
import {
  ISSUE_DRAFT_AUTOSAVE_MS,
  ISSUE_DRAFT_DISCARDED_GRACE_MS,
} from "@/lib/issue-draft-page"
import {
  useIssueDraftEditor,
  type IssueDraftEditorOptions,
} from "@/hooks/use-issue-draft-editor"

// EXP-1170: the New issue page's autosave contract — ONE coalesced write of
// the FULL row per burst, chips never create a row, leaving writes (or
// deletes) exactly once, Create flushes first and hands the row over.

const mocks = vi.hoisted(() => ({
  upsert: vi.fn(),
  draftDelete: vi.fn(),
  listAttachments: vi.fn(),
  create: vi.fn(),
  attachmentDelete: vi.fn(),
  awaitTxId: vi.fn(),
  toastError: vi.fn(),
  /** EXP-1231: the issue created from this draft, as the shape reports it. */
  createdElsewhere: { current: undefined as undefined | { id: string } },
}))

vi.mock(`@/lib/trpc-client`, () => ({
  trpc: {
    issues: { create: { mutate: mocks.create } },
    issueDrafts: {
      upsert: { mutate: mocks.upsert },
      delete: { mutate: mocks.draftDelete },
      listAttachments: { query: mocks.listAttachments },
    },
    attachments: { delete: { mutate: mocks.attachmentDelete } },
  },
}))

vi.mock(`@/lib/collections`, async (importOriginal) => ({
  ...((await importOriginal()) as Record<string, unknown>),
  issueCollection: { utils: { awaitTxId: mocks.awaitTxId } },
}))

vi.mock(`@exp/ui`, async (importOriginal) => ({
  ...((await importOriginal()) as Record<string, unknown>),
  toast: { error: mocks.toastError, message: vi.fn() },
}))

vi.mock(`@/hooks/use-issue-drafts`, () => ({
  useIssueFromDraft: () => mocks.createdElsewhere.current,
}))

const DRAFT_ID = `11111111-1111-4111-8111-111111111111`
const TEAM_ID = `22222222-2222-4222-8222-222222222222`
const BOARD_ID = `33333333-3333-4333-8333-333333333333`

const board = { id: BOARD_ID, slug: `web` } as Board
const users = [{ id: `u1` }, { id: `u2` }] as User[]

function options(
  overrides: Partial<IssueDraftEditorOptions> = {}
): IssueDraftEditorOptions {
  return {
    draftId: DRAFT_ID,
    teamId: TEAM_ID,
    initialBoardId: BOARD_ID,
    boards: [board],
    labels: [],
    users,
    ...overrides,
  }
}

function existingDraft(overrides: Partial<IssueDraft> = {}): IssueDraft {
  return {
    id: DRAFT_ID,
    teamId: TEAM_ID,
    boardId: BOARD_ID,
    userId: `u1`,
    title: `Saved`,
    description: ``,
    statusId: null,
    priority: `none`,
    assigneeId: null,
    labelIds: [],
    dueDate: null,
    createdAt: new Date(),
    updatedAt: new Date(),
    ...overrides,
  } as IssueDraft
}

/** Let the write chain's microtasks run. */
async function settle() {
  await act(async () => {
    for (let i = 0; i < 10; i += 1) await Promise.resolve()
  })
}

async function advance(ms: number) {
  await act(async () => {
    vi.advanceTimersByTime(ms)
  })
  await settle()
}

beforeEach(() => {
  vi.useFakeTimers()
  mocks.createdElsewhere.current = undefined
  for (const mock of Object.values(mocks)) {
    if (typeof mock === `function`) mock.mockReset()
  }
  mocks.upsert.mockImplementation(async (input: { id: string }) => ({
    draft: { id: input.id },
    txId: 1,
  }))
  mocks.draftDelete.mockResolvedValue({ txId: 2 })
  mocks.listAttachments.mockResolvedValue([])
  mocks.create.mockResolvedValue({
    issue: { id: `i1`, identifier: `WEB-7` },
    txId: 3,
  })
  mocks.awaitTxId.mockResolvedValue(undefined)
  mocks.attachmentDelete.mockResolvedValue({ txId: 4 })
})

afterEach(() => {
  vi.useRealTimers()
})

describe(`useIssueDraftEditor autosave`, () => {
  it(`writes ONE full row per typing burst, after the debounce`, async () => {
    const { result } = renderHook(() => useIssueDraftEditor(options()))
    act(() => result.current.setTitle(`F`))
    act(() => result.current.setTitle(`Fi`))
    act(() => result.current.setTitle(`Fix login`))
    act(() => result.current.setDescription(`Steps`))
    await advance(ISSUE_DRAFT_AUTOSAVE_MS - 1)
    expect(mocks.upsert).not.toHaveBeenCalled()
    await advance(1)
    expect(mocks.upsert).toHaveBeenCalledTimes(1)
    expect(mocks.upsert).toHaveBeenCalledWith({
      id: DRAFT_ID,
      teamId: TEAM_ID,
      boardId: BOARD_ID,
      title: `Fix login`,
      description: `Steps`,
      statusId: null,
      priority: `none`,
      assigneeId: null,
      labelIds: [],
      dueDate: null,
    })
  })

  it(`never creates a row for a chip alone`, async () => {
    const { result, unmount } = renderHook(() => useIssueDraftEditor(options()))
    act(() => result.current.setPriority(`high`))
    await advance(ISSUE_DRAFT_AUTOSAVE_MS * 2)
    unmount()
    await settle()
    expect(mocks.upsert).not.toHaveBeenCalled()
    expect(mocks.draftDelete).not.toHaveBeenCalled()
  })

  it(`writes a chip at once once there is content`, async () => {
    const { result } = renderHook(() => useIssueDraftEditor(options()))
    act(() => result.current.setTitle(`Fix login`))
    act(() => result.current.setPriority(`high`))
    await settle()
    expect(mocks.upsert).toHaveBeenCalledTimes(1)
    expect(mocks.upsert.mock.calls[0][0]).toMatchObject({
      title: `Fix login`,
      priority: `high`,
    })
    // The debounce it cancelled never writes a second time.
    await advance(ISSUE_DRAFT_AUTOSAVE_MS * 2)
    expect(mocks.upsert).toHaveBeenCalledTimes(1)
  })

  it(`writes on blur and skips an identical write after it`, async () => {
    const { result } = renderHook(() => useIssueDraftEditor(options()))
    act(() => result.current.setTitle(`Fix login`))
    act(() => result.current.onTitleBlur())
    await settle()
    expect(mocks.upsert).toHaveBeenCalledTimes(1)
    act(() => result.current.onDescriptionBlur())
    await advance(ISSUE_DRAFT_AUTOSAVE_MS * 2)
    expect(mocks.upsert).toHaveBeenCalledTimes(1)
  })

  it(`serialises a snapshot that changed mid-request after it`, async () => {
    let release: () => void = () => {}
    mocks.upsert.mockImplementationOnce(
      (input: { id: string }) =>
        new Promise((resolve) => {
          release = () => resolve({ draft: { id: input.id }, txId: 1 })
        })
    )
    const { result } = renderHook(() => useIssueDraftEditor(options()))
    act(() => result.current.setTitle(`One`))
    act(() => result.current.onTitleBlur())
    await settle()
    act(() => result.current.setTitle(`Two`))
    act(() => result.current.onTitleBlur())
    await settle()
    expect(mocks.upsert).toHaveBeenCalledTimes(1)
    release()
    await settle()
    expect(mocks.upsert).toHaveBeenCalledTimes(2)
    expect(mocks.upsert.mock.calls[1][0]).toMatchObject({ title: `Two` })
  })
})

describe(`useIssueDraftEditor leaving`, () => {
  it(`writes nothing for an untouched page`, async () => {
    const { unmount } = renderHook(() => useIssueDraftEditor(options()))
    unmount()
    await settle()
    expect(mocks.upsert).not.toHaveBeenCalled()
    expect(mocks.draftDelete).not.toHaveBeenCalled()
  })

  it(`writes the content once on unmount`, async () => {
    const { result, unmount } = renderHook(() => useIssueDraftEditor(options()))
    act(() => result.current.setTitle(`Fix login`))
    unmount()
    await settle()
    await advance(ISSUE_DRAFT_AUTOSAVE_MS * 2)
    expect(mocks.upsert).toHaveBeenCalledTimes(1)
    expect(mocks.upsert.mock.calls[0][0]).toMatchObject({ title: `Fix login` })
  })

  it(`deletes an existing row that was emptied out`, async () => {
    const { result, unmount } = renderHook(() =>
      useIssueDraftEditor(options({ draft: existingDraft() }))
    )
    expect(result.current.title).toBe(`Saved`)
    // The attachments fetch has landed: the row provably holds no file.
    await settle()
    act(() => result.current.setTitle(``))
    unmount()
    await settle()
    expect(mocks.draftDelete).toHaveBeenCalledTimes(1)
    expect(mocks.draftDelete).toHaveBeenCalledWith({ id: DRAFT_ID })
    expect(mocks.upsert).not.toHaveBeenCalled()
  })

  it(`never deletes a reopened draft before its attachments are known`, async () => {
    mocks.listAttachments.mockReturnValue(new Promise(() => {}))
    const { unmount } = renderHook(() =>
      useIssueDraftEditor(options({ draft: existingDraft({ title: `` }) }))
    )
    unmount()
    await settle()
    expect(mocks.draftDelete).not.toHaveBeenCalled()
  })

  it(`never deletes a reopened draft after a failed attachments fetch`, async () => {
    mocks.listAttachments.mockRejectedValue(new Error(`offline`))
    const { unmount } = renderHook(() =>
      useIssueDraftEditor(options({ draft: existingDraft({ title: `` }) }))
    )
    await settle()
    unmount()
    await settle()
    expect(mocks.draftDelete).not.toHaveBeenCalled()
  })

  it(`keeps a files-only draft on leave once its files have loaded`, async () => {
    mocks.listAttachments.mockResolvedValue([
      {
        id: `a1`,
        filename: `log.txt`,
        contentType: `text/plain`,
        sizeBytes: 3,
        url: `/api/attachments/a1`,
      },
    ])
    const { result, unmount } = renderHook(() =>
      useIssueDraftEditor(options({ draft: existingDraft({ title: `` }) }))
    )
    await settle()
    expect(result.current.files).toHaveLength(1)
    unmount()
    await settle()
    expect(mocks.draftDelete).not.toHaveBeenCalled()
  })

  it(`does not rewrite an untouched reopened draft in a solo team`, async () => {
    const { result, unmount } = renderHook(() =>
      useIssueDraftEditor(
        options({ draft: existingDraft(), users: [{ id: `u1` }] as User[] })
      )
    )
    expect(result.current.assigneeId).toBe(`u1`)
    await settle()
    unmount()
    await settle()
    expect(mocks.upsert).not.toHaveBeenCalled()
  })

  it(`says so when the leave write fails`, async () => {
    mocks.upsert.mockRejectedValue(new Error(`offline`))
    const { result, unmount } = renderHook(() => useIssueDraftEditor(options()))
    act(() => result.current.setTitle(`Fix login`))
    unmount()
    await settle()
    expect(mocks.toastError).toHaveBeenCalledTimes(1)
  })

  it(`keeps a single failed debounced write quiet, but not a second one`, async () => {
    mocks.upsert.mockRejectedValue(new Error(`offline`))
    const { result } = renderHook(() => useIssueDraftEditor(options()))
    act(() => result.current.setTitle(`One`))
    await advance(ISSUE_DRAFT_AUTOSAVE_MS)
    expect(mocks.toastError).not.toHaveBeenCalled()
    act(() => result.current.setTitle(`Two`))
    await advance(ISSUE_DRAFT_AUTOSAVE_MS)
    expect(mocks.toastError).toHaveBeenCalledTimes(1)
  })

  it(`writes nothing when a reopened draft was left untouched`, async () => {
    const { unmount } = renderHook(() =>
      useIssueDraftEditor(options({ draft: existingDraft() }))
    )
    unmount()
    await settle()
    expect(mocks.upsert).not.toHaveBeenCalled()
    expect(mocks.draftDelete).not.toHaveBeenCalled()
  })

  it(`reports the leave write's outcome (EXP-1212 R3)`, async () => {
    const { result } = renderHook(() => useIssueDraftEditor(options()))
    act(() => result.current.setTitle(`Fix login`))
    let ok: boolean | null = null
    await act(async () => {
      ok = await result.current.leave()
    })
    expect(ok).toBe(true)
    mocks.upsert.mockRejectedValueOnce(new Error(`offline`))
    act(() => result.current.setTitle(`Fix login now`))
    await act(async () => {
      ok = await result.current.leave()
    })
    expect(ok).toBe(false)
    // The page's normal save error, once.
    expect(mocks.toastError).toHaveBeenCalledTimes(1)
    expect(mocks.toastError).toHaveBeenCalledWith(`Could not save the draft`)
  })

  it(`writes on pagehide`, async () => {
    const { result } = renderHook(() => useIssueDraftEditor(options()))
    act(() => result.current.setTitle(`Fix login`))
    act(() => {
      window.dispatchEvent(new Event(`pagehide`))
    })
    await settle()
    expect(mocks.upsert).toHaveBeenCalledTimes(1)
  })
})

describe(`useIssueDraftEditor ensureDraft`, () => {
  it(`inserts one row for three concurrent uploads`, async () => {
    const { result } = renderHook(() => useIssueDraftEditor(options()))
    let ids: string[] = []
    await act(async () => {
      ids = await Promise.all([
        result.current.ensureDraft(),
        result.current.ensureDraft(),
        result.current.ensureDraft(),
      ])
    })
    expect(ids).toEqual([DRAFT_ID, DRAFT_ID, DRAFT_ID])
    expect(mocks.upsert).toHaveBeenCalledTimes(1)
  })
})

describe(`useIssueDraftEditor create`, () => {
  it(`refuses to create while an upload is in flight`, async () => {
    const { result } = renderHook(() => useIssueDraftEditor(options()))
    act(() => result.current.setTitle(`Fix login`))
    act(() => result.current.beginUpload())
    expect(result.current.canCreate).toBe(false)
    let created: unknown = `unset`
    await act(async () => {
      created = await result.current.create()
    })
    expect(created).toBeNull()
    expect(mocks.create).not.toHaveBeenCalled()
    act(() => result.current.endUpload())
    expect(result.current.canCreate).toBe(true)
  })

  it(`flushes a dirty title BEFORE creating and hands the row over`, async () => {
    const { result } = renderHook(() => useIssueDraftEditor(options()))
    act(() => result.current.setTitle(`Fix login`))
    let created: Awaited<ReturnType<typeof result.current.create>> = null
    await act(async () => {
      created = await result.current.create()
    })
    expect(mocks.upsert).toHaveBeenCalledTimes(1)
    expect(mocks.upsert.mock.invocationCallOrder[0]).toBeLessThan(
      mocks.create.mock.invocationCallOrder[0]
    )
    expect(mocks.create).toHaveBeenCalledWith(
      expect.objectContaining({
        boardId: BOARD_ID,
        title: `Fix login`,
        draftId: DRAFT_ID,
      })
    )
    expect(mocks.awaitTxId).toHaveBeenCalledWith(3)
    expect(created).toEqual({ identifier: `WEB-7`, boardSlug: `web` })
  })

  it(`passes no draftId when no row exists and sends the fallback anchor`, async () => {
    // The flush fails, so the row never came to be.
    mocks.upsert.mockRejectedValueOnce(new Error(`offline`))
    const { result } = renderHook(() => useIssueDraftEditor(options()))
    act(() => result.current.setTitle(`Fix login`))
    await act(async () => {
      await result.current.create()
    })
    const input = mocks.create.mock.calls[0][0]
    expect(input.draftId).toBeUndefined()
    expect(input.status).toBe(`backlog`)
    expect(input.statusId).toBeUndefined()
  })

  it(`stays on the page after a rejected create and still writes on leave`, async () => {
    mocks.create.mockRejectedValueOnce(new Error(`Nope`))
    const { result, unmount } = renderHook(() => useIssueDraftEditor(options()))
    act(() => result.current.setTitle(`Fix login`))
    let created: unknown = `unset`
    await act(async () => {
      created = await result.current.create()
    })
    expect(created).toBeNull()
    expect(result.current.creating).toBe(false)
    expect(result.current.canCreate).toBe(true)
    expect(mocks.toastError).toHaveBeenCalledWith(`Nope`)
    act(() => result.current.setTitle(`Fix login now`))
    unmount()
    await settle()
    expect(mocks.upsert).toHaveBeenCalledTimes(2)
    expect(mocks.upsert.mock.calls[1][0]).toMatchObject({
      title: `Fix login now`,
    })
  })

  it(`never writes the draft back after a successful create`, async () => {
    const { result, unmount } = renderHook(() => useIssueDraftEditor(options()))
    act(() => result.current.setTitle(`Fix login`))
    await act(async () => {
      await result.current.create()
    })
    unmount()
    await settle()
    expect(mocks.upsert).toHaveBeenCalledTimes(1)
    expect(mocks.draftDelete).not.toHaveBeenCalled()
  })

  it(`sends statusId null to the draft for a fallback status`, async () => {
    const { result } = renderHook(() => useIssueDraftEditor(options()))
    act(() => result.current.setTitle(`Fix login`))
    act(() => result.current.onTitleBlur())
    await settle()
    expect(mocks.upsert.mock.calls[0][0].statusId).toBeNull()
  })
})

describe(`useIssueDraftEditor discard`, () => {
  it(`deletes an existing row and writes nothing on the way out`, async () => {
    const { result, unmount } = renderHook(() =>
      useIssueDraftEditor(options({ draft: existingDraft() }))
    )
    act(() => result.current.setTitle(`Changed`))
    await act(async () => {
      await result.current.discard()
    })
    unmount()
    await advance(ISSUE_DRAFT_AUTOSAVE_MS * 2)
    expect(mocks.draftDelete).toHaveBeenCalledTimes(1)
    expect(mocks.upsert).not.toHaveBeenCalled()
  })

  it(`never re-inserts the row from an upload started after a discard`, async () => {
    const { result } = renderHook(() => useIssueDraftEditor(options()))
    act(() => result.current.setTitle(`Typed`))
    await act(async () => {
      await result.current.discard()
    })
    await act(async () => {
      await expect(result.current.ensureDraft()).rejects.toThrow()
    })
    expect(mocks.upsert).not.toHaveBeenCalled()
  })

  it(`never runs while a Create is in flight (EXP-1212 R2)`, async () => {
    let release: () => void = () => {}
    mocks.create.mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          release = () =>
            resolve({ issue: { id: `i1`, identifier: `WEB-7` }, txId: 3 })
        })
    )
    const { result } = renderHook(() =>
      useIssueDraftEditor(options({ draft: existingDraft() }))
    )
    await settle()
    expect(result.current.isCreating()).toBe(false)
    let creating: Promise<unknown> = Promise.resolve()
    act(() => {
      creating = result.current.create()
    })
    expect(result.current.isCreating()).toBe(true)
    let discarded: boolean | null = null
    await act(async () => {
      discarded = await result.current.discard()
    })
    expect(discarded).toBe(false)
    expect(mocks.draftDelete).not.toHaveBeenCalled()
    await act(async () => {
      release()
      await creating
    })
    expect(result.current.isCreating()).toBe(false)
    expect(mocks.draftDelete).not.toHaveBeenCalled()
  })

  it(`clears the in-flight flag after a refused Create`, async () => {
    mocks.create.mockRejectedValueOnce(new Error(`Nope`))
    const { result } = renderHook(() => useIssueDraftEditor(options()))
    act(() => result.current.setTitle(`Fix login`))
    await act(async () => {
      await result.current.create()
    })
    expect(result.current.isCreating()).toBe(false)
    let discarded: boolean | null = null
    await act(async () => {
      discarded = await result.current.discard()
    })
    expect(discarded).toBe(true)
  })

  it(`deletes nothing when no row exists`, async () => {
    const { result, unmount } = renderHook(() => useIssueDraftEditor(options()))
    act(() => result.current.setTitle(`Typed`))
    await act(async () => {
      await result.current.discard()
    })
    unmount()
    await advance(ISSUE_DRAFT_AUTOSAVE_MS * 2)
    expect(mocks.draftDelete).not.toHaveBeenCalled()
    expect(mocks.upsert).not.toHaveBeenCalled()
  })
})

// EXP-1231: the draft consumed on ANOTHER client. The hook reads the live
// row (`draft`) and the issue created from it (`useIssueFromDraft`).
describe(`useIssueDraftEditor consumed elsewhere`, () => {
  const createdIssue = { id: `i9`, identifier: `WEB-9`, boardId: BOARD_ID }

  function renderLive(initialDraft: IssueDraft | undefined, onConsumedElsewhere = vi.fn()) {
    const hook = renderHook(
      ({ draft }: { draft: IssueDraft | undefined }) =>
        useIssueDraftEditor(options({ draft, onConsumedElsewhere })),
      { initialProps: { draft: initialDraft } }
    )
    return { ...hook, onConsumedElsewhere }
  }

  it(`lands on the issue created elsewhere and never writes again`, async () => {
    const { result, rerender, unmount, onConsumedElsewhere } = renderLive(existingDraft())
    act(() => result.current.setTitle(`Edited here`))
    // The other client's create: the row goes, the issue lands.
    mocks.createdElsewhere.current = createdIssue
    rerender({ draft: undefined })
    await settle()
    expect(onConsumedElsewhere).toHaveBeenCalledWith({
      kind: `created`,
      issue: createdIssue,
    })
    await advance(ISSUE_DRAFT_AUTOSAVE_MS)
    unmount()
    await settle()
    expect(mocks.upsert).not.toHaveBeenCalled()
    expect(mocks.draftDelete).not.toHaveBeenCalled()
  })

  it(`an issue claiming the draft is proof enough, even with the row still synced`, async () => {
    const { result, rerender, onConsumedElsewhere } = renderLive(existingDraft())
    mocks.createdElsewhere.current = createdIssue
    rerender({ draft: existingDraft() })
    await settle()
    expect(onConsumedElsewhere).toHaveBeenCalledTimes(1)
    expect(result.current.hasContent).toBe(true)
  })

  it(`holds writes while the row is gone, then leaves as discarded after the grace`, async () => {
    const { result, rerender, unmount, onConsumedElsewhere } = renderLive(existingDraft())
    rerender({ draft: undefined })
    act(() => result.current.setTitle(`Typed into a dead page`))
    await advance(ISSUE_DRAFT_AUTOSAVE_MS)
    expect(mocks.upsert).not.toHaveBeenCalled()
    expect(onConsumedElsewhere).not.toHaveBeenCalled()
    await advance(ISSUE_DRAFT_DISCARDED_GRACE_MS)
    expect(onConsumedElsewhere).toHaveBeenCalledWith({ kind: `discarded` })
    unmount()
    await settle()
    expect(mocks.upsert).not.toHaveBeenCalled()
    expect(mocks.draftDelete).not.toHaveBeenCalled()
  })

  it(`resumes (and writes what was typed) when the row returns within the grace`, async () => {
    const { result, rerender, onConsumedElsewhere } = renderLive(existingDraft())
    rerender({ draft: undefined })
    act(() => result.current.setTitle(`Kept`))
    await advance(ISSUE_DRAFT_AUTOSAVE_MS)
    expect(mocks.upsert).not.toHaveBeenCalled()
    rerender({ draft: existingDraft() })
    await advance(ISSUE_DRAFT_AUTOSAVE_MS)
    expect(mocks.upsert).toHaveBeenCalledTimes(1)
    expect(mocks.upsert.mock.calls[0][0].title).toBe(`Kept`)
    await advance(ISSUE_DRAFT_DISCARDED_GRACE_MS)
    expect(onConsumedElsewhere).not.toHaveBeenCalled()
  })

  it(`never calls a row it has not seen synced gone`, async () => {
    const { result, onConsumedElsewhere } = renderLive(undefined)
    act(() => result.current.setTitle(`Fresh`))
    await advance(ISSUE_DRAFT_AUTOSAVE_MS + ISSUE_DRAFT_DISCARDED_GRACE_MS)
    expect(mocks.upsert).toHaveBeenCalledTimes(1)
    expect(onConsumedElsewhere).not.toHaveBeenCalled()
  })

  it(`counts the echo of its own first write as seen`, async () => {
    const { result, rerender, onConsumedElsewhere } = renderLive(undefined)
    act(() => result.current.setTitle(`Fresh`))
    await advance(ISSUE_DRAFT_AUTOSAVE_MS)
    rerender({ draft: existingDraft({ title: `Fresh` }) })
    rerender({ draft: undefined })
    await advance(ISSUE_DRAFT_DISCARDED_GRACE_MS)
    expect(onConsumedElsewhere).toHaveBeenCalledWith({ kind: `discarded` })
  })

  it(`concludes nothing while its own Create is in flight`, async () => {
    let resolveCreate: (value: unknown) => void = () => {}
    mocks.create.mockImplementation(
      () => new Promise((resolve) => (resolveCreate = resolve))
    )
    const { result, rerender, onConsumedElsewhere } = renderLive(existingDraft())
    act(() => result.current.setTitle(`Mine`))
    let created: Promise<unknown> | undefined
    act(() => {
      created = result.current.create()
    })
    await settle()
    // Our own transaction: the row goes and the issue lands, through the
    // shape, before the mutation answers.
    mocks.createdElsewhere.current = createdIssue
    rerender({ draft: undefined })
    await settle()
    expect(onConsumedElsewhere).not.toHaveBeenCalled()
    resolveCreate({ issue: { id: `i9`, identifier: `WEB-9` }, txId: 5 })
    await act(async () => {
      await created
    })
    await advance(ISSUE_DRAFT_DISCARDED_GRACE_MS)
    expect(onConsumedElsewhere).not.toHaveBeenCalled()
  })

  it(`treats the server's CONFLICT on a consumed draft as nothing to save`, async () => {
    const { TRPCClientError } = await import(`@trpc/client`)
    mocks.upsert.mockRejectedValue(
      Object.assign(new TRPCClientError(`consumed`), { data: { code: `CONFLICT` } })
    )
    const { result, unmount } = renderLive(existingDraft())
    act(() => result.current.setTitle(`Late write`))
    let ok: boolean | undefined
    await act(async () => {
      ok = await result.current.leave()
    })
    expect(ok).toBe(true)
    expect(mocks.toastError).not.toHaveBeenCalled()
    unmount()
    await settle()
    expect(mocks.upsert).toHaveBeenCalledTimes(1)
  })

  // The CONFLICT usually arrives BEFORE the shape delivers the issue: the
  // page must stop writing but keep watching, and land on the issue when it
  // comes (the natives keep watching too; sealing here stranded the page).
  it(`still lands on the issue the shape delivers after a CONFLICT`, async () => {
    const { TRPCClientError } = await import(`@trpc/client`)
    mocks.upsert.mockRejectedValue(
      Object.assign(new TRPCClientError(`consumed`), { data: { code: `CONFLICT` } })
    )
    const { result, rerender, onConsumedElsewhere } = renderLive(existingDraft())
    act(() => result.current.setTitle(`Late write`))
    await advance(ISSUE_DRAFT_AUTOSAVE_MS)
    expect(mocks.upsert).toHaveBeenCalledTimes(1)
    expect(onConsumedElsewhere).not.toHaveBeenCalled()
    // Writing has stopped…
    act(() => result.current.setTitle(`Later still`))
    await advance(ISSUE_DRAFT_AUTOSAVE_MS)
    expect(mocks.upsert).toHaveBeenCalledTimes(1)
    // …but the verdict is still taken when the shape lands the issue.
    mocks.createdElsewhere.current = createdIssue
    rerender({ draft: undefined })
    await settle()
    expect(onConsumedElsewhere).toHaveBeenCalledWith({
      kind: `created`,
      issue: createdIssue,
    })
    expect(mocks.toastError).not.toHaveBeenCalled()
  })

  // A verdict the shape delivered DURING the page's own Create is not judged
  // then (the Create owns the exit) — a failed Create must judge it after.
  it(`judges a row gone during a Create once that Create failed`, async () => {
    let rejectCreate: (error: unknown) => void = () => {}
    mocks.create.mockImplementation(
      () => new Promise((_, reject) => (rejectCreate = reject))
    )
    const { result, rerender, onConsumedElsewhere } = renderLive(existingDraft())
    act(() => result.current.setTitle(`Mine`))
    let created: Promise<unknown> | undefined
    act(() => {
      created = result.current.create()
    })
    await settle()
    // Discarded elsewhere while the Create is in flight: nothing yet.
    rerender({ draft: undefined })
    await advance(ISSUE_DRAFT_DISCARDED_GRACE_MS)
    expect(onConsumedElsewhere).not.toHaveBeenCalled()
    rejectCreate(new Error(`Nope`))
    await act(async () => {
      await created
    })
    expect(result.current.isCreating()).toBe(false)
    // The failed Create re-judged: the row is gone, a FRESH grace runs.
    await advance(ISSUE_DRAFT_DISCARDED_GRACE_MS - 1)
    expect(onConsumedElsewhere).not.toHaveBeenCalled()
    await advance(1)
    expect(onConsumedElsewhere).toHaveBeenCalledWith({ kind: `discarded` })
  })

  it(`re-arms the grace when it ran out during a Create that then failed`, async () => {
    let rejectCreate: (error: unknown) => void = () => {}
    mocks.create.mockImplementation(
      () => new Promise((_, reject) => (rejectCreate = reject))
    )
    const { result, rerender, onConsumedElsewhere } = renderLive(existingDraft())
    // Gone BEFORE the Create: the hold is on, the grace is running.
    rerender({ draft: undefined })
    act(() => result.current.setTitle(`Typed under the hold`))
    let created: Promise<unknown> | undefined
    act(() => {
      created = result.current.create()
    })
    await settle()
    // The grace fires mid-Create and bails.
    await advance(ISSUE_DRAFT_DISCARDED_GRACE_MS)
    expect(onConsumedElsewhere).not.toHaveBeenCalled()
    rejectCreate(new Error(`Nope`))
    await act(async () => {
      await created
    })
    // Before: the hold stayed on for ever and every write was dropped
    // silently. Now the row is judged again — still gone, fresh grace.
    await advance(ISSUE_DRAFT_DISCARDED_GRACE_MS)
    expect(onConsumedElsewhere).toHaveBeenCalledWith({ kind: `discarded` })
    expect(mocks.upsert).not.toHaveBeenCalled()
  })

  it(`releases a hold when the row came back during a Create that failed`, async () => {
    let rejectCreate: (error: unknown) => void = () => {}
    mocks.create.mockImplementation(
      () => new Promise((_, reject) => (rejectCreate = reject))
    )
    const { result, rerender, onConsumedElsewhere } = renderLive(existingDraft())
    rerender({ draft: undefined })
    act(() => result.current.setTitle(`Typed under the hold`))
    let created: Promise<unknown> | undefined
    act(() => {
      created = result.current.create()
    })
    await settle()
    expect(mocks.upsert).not.toHaveBeenCalled()
    // A resync brings the row back while the Create is in flight.
    rerender({ draft: existingDraft() })
    await settle()
    rejectCreate(new Error(`Nope`))
    await act(async () => {
      await created
    })
    // Editing resumed: what was typed under the hold is written.
    await advance(ISSUE_DRAFT_AUTOSAVE_MS)
    expect(mocks.upsert).toHaveBeenCalledTimes(1)
    expect(mocks.upsert.mock.calls[0][0].title).toBe(`Typed under the hold`)
    await advance(ISSUE_DRAFT_DISCARDED_GRACE_MS)
    expect(onConsumedElsewhere).not.toHaveBeenCalled()
  })
})
