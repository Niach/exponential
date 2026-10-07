import { act, fireEvent, render, screen, waitFor } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"
import { TooltipProvider } from "@exp/ui"
import type { Board } from "@/db/schema"
import { IssueDraftPage } from "@/components/issue-draft-page"
import { ISSUE_DRAFT_COPY } from "@/lib/issue-draft-page"

// EXP-1212: the New issue page's two prompts. The draft controller and the
// router are stand-ins: what is under test is which prompt each exit raises
// and where each choice goes.

const mobileState = vi.hoisted(() => ({ mobile: false }))
const navigate = vi.hoisted(() => vi.fn())
type ShouldBlock = (args: {
  current: { pathname: string }
  next: { pathname: string }
}) => boolean
const HELD = { pathname: `/t/acme/inbox`, search: { tab: `my-issues` } }
const blockerState = vi.hoisted(() => ({
  status: `idle` as `idle` | `blocked`,
  // The held navigation: a push by default (the case that leaves the draft
  // entry behind), BACK for a popstate.
  action: `PUSH` as `PUSH` | `BACK`,
  proceed: vi.fn(),
  reset: vi.fn(),
  shouldBlockFn: null as null | ShouldBlock,
  enableBeforeUnload: undefined as unknown,
}))
const editor = vi.hoisted(() => ({
  hasContent: true,
  canCreate: true,
  creating: false,
  create: vi.fn(),
  discard: vi.fn(async () => true),
  leave: vi.fn(async () => true),
  /** EXP-1231: the options the page handed the controller (its callbacks). */
  options: null as null | { onConsumedElsewhere?: (consumed: unknown) => void },
}))
const toastMessage = vi.hoisted(() => vi.fn())

vi.mock(`@exp/ui`, async (importOriginal) => {
  // eslint-disable-next-line quotes -- esbuild rejects template literals inside typeof import()
  const actual = await importOriginal<typeof import("@exp/ui")>()
  return {
    ...actual,
    useIsMobile: () => mobileState.mobile,
    toast: { ...actual.toast, message: toastMessage },
  }
})
vi.mock(`@tanstack/react-router`, () => ({
  useNavigate: () => navigate,
  useBlocker: (opts: {
    shouldBlockFn: ShouldBlock
    enableBeforeUnload?: unknown
  }) => {
    blockerState.shouldBlockFn = opts.shouldBlockFn
    blockerState.enableBeforeUnload = opts.enableBeforeUnload
    return blockerState.status === `blocked`
      ? {
          status: `blocked`,
          action: blockerState.action,
          next: { pathname: `/t/acme/inbox`, search: { tab: `my-issues` } },
          proceed: blockerState.proceed,
          reset: blockerState.reset,
        }
      : { status: `idle` }
  },
}))
vi.mock(`@/hooks/use-issue-draft-editor`, () => ({
  useIssueDraftEditor: (options: typeof editor.options) => {
    editor.options = options
    return {
    title: `Fix it`,
    description: ``,
    boardId: `b1`,
    status: null,
    priority: `none`,
    assigneeId: null,
    labelIds: [],
    dueDate: null,
    files: [],
    uploading: false,
    setTitle: vi.fn(),
    setDescription: vi.fn(),
    onTitleBlur: vi.fn(),
    onDescriptionBlur: vi.fn(),
    ...editor,
    isCreating: () => editor.creating,
    }
  },
}))
vi.mock(`@/hooks/use-detail-chrome`, () => ({
  useMeasuredSize: () => [() => {}, { width: 0, height: 0 }],
  useTitleCollapsed: () => ({
    scrollRef: () => {},
    titleRef: () => {},
    collapsed: false,
  }),
}))
vi.mock(`@/components/issue-editor/markdown-editor`, () => ({
  MarkdownEditor: () => null,
}))
vi.mock(`@/components/issue-files-section`, () => ({
  FilesSectionView: () => null,
}))
vi.mock(`@/components/issue-properties-panel`, () => ({
  IssuePropertiesPanel: () => null,
}))
vi.mock(`@/lib/storage/issue-image-upload`, () => ({
  uploadDraftFile: vi.fn(),
  uploadDraftImageFile: vi.fn(),
}))
vi.mock(`@/lib/storage/media-upload`, () => ({
  mediaPlayabilityHint: () => null,
  prepareMediaUpload: vi.fn(),
  uploadDraftMediaFile: vi.fn(),
}))

const board = { id: `b1`, slug: `web`, prefix: `WEB` } as Board

function renderPage() {
  return render(
    <TooltipProvider>
      <IssueDraftPage
      draftId="d1"
      teamId="t1"
      teamSlug="acme"
      boards={[board]}
      users={[]}
      labels={[]}
      initialBoardId="b1"
      />
    </TooltipProvider>
  )
}

beforeEach(() => {
  mobileState.mobile = false
  navigate.mockReset()
  blockerState.status = `idle`
  blockerState.action = `PUSH`
  blockerState.proceed.mockReset()
  blockerState.reset.mockReset()
  editor.hasContent = true
  editor.canCreate = true
  editor.create.mockReset()
  editor.creating = false
  editor.discard.mockReset()
  editor.discard.mockImplementation(async () => true)
  editor.leave.mockReset()
  editor.leave.mockImplementation(async () => true)
  editor.options = null
  toastMessage.mockReset()
})

// EXP-1231: the draft consumed on another client — the controller's
// verdict, the page's exit. Both are the page's OWN exits: they replace or
// leave with the blocker ignored, nothing is asked.
describe(`IssueDraftPage consumed elsewhere`, () => {
  it(`created elsewhere: becomes that issue's detail, replacing the draft entry`, () => {
    renderPage()
    act(() => {
      editor.options?.onConsumedElsewhere?.({
        kind: `created`,
        issue: { id: `i9`, identifier: `WEB-9`, boardId: `b1` },
      })
    })
    expect(navigate).toHaveBeenCalledWith(
      expect.objectContaining({
        to: `/t/$teamSlug/boards/$boardSlug/issues/$issueIdentifier`,
        params: { teamSlug: `acme`, boardSlug: `web`, issueIdentifier: `WEB-9` },
        replace: true,
        ignoreBlocker: true,
      })
    )
    expect(toastMessage).not.toHaveBeenCalled()
    expect(screen.queryByTestId(`issue-draft-leave-dialog`)).toBeNull()
  })

  it(`discarded elsewhere: says so and goes Back past the blocker`, () => {
    renderPage()
    act(() => {
      editor.options?.onConsumedElsewhere?.({ kind: `discarded` })
    })
    expect(toastMessage).toHaveBeenCalledWith(ISSUE_DRAFT_COPY.discardedElsewhere)
    expect(navigate).toHaveBeenCalledWith(
      expect.objectContaining({
        to: `/t/$teamSlug/boards/$boardSlug`,
        params: { teamSlug: `acme`, boardSlug: `web` },
        ignoreBlocker: true,
      })
    )
    expect(editor.leave).not.toHaveBeenCalled()
    expect(editor.discard).not.toHaveBeenCalled()
  })
})

describe(`IssueDraftPage discard`, () => {
  it(`confirms first when the draft has content`, async () => {
    renderPage()
    fireEvent.click(screen.getByTestId(`issue-draft-discard`))
    const confirm = await screen.findByTestId(`issue-draft-discard-confirm`)
    expect(confirm.textContent).toContain(ISSUE_DRAFT_COPY.discardConfirm.title)
    expect(editor.discard).not.toHaveBeenCalled()

    fireEvent.click(
      screen.getByRole(`button`, { name: ISSUE_DRAFT_COPY.discardConfirm.confirm })
    )
    await waitFor(() => expect(navigate).toHaveBeenCalled())
    expect(editor.discard).toHaveBeenCalledTimes(1)
    // The page's own exit never meets the leave dialog.
    expect(navigate.mock.calls[0][0]).toMatchObject({ ignoreBlocker: true })
  })

  it(`cancel keeps the draft`, async () => {
    renderPage()
    fireEvent.click(screen.getByTestId(`issue-draft-discard`))
    await screen.findByTestId(`issue-draft-discard-confirm`)
    fireEvent.click(screen.getByRole(`button`, { name: `Cancel` }))
    await waitFor(() =>
      expect(screen.queryByTestId(`issue-draft-discard-confirm`)).toBeNull()
    )
    expect(editor.discard).not.toHaveBeenCalled()
    expect(navigate).not.toHaveBeenCalled()
  })

  it(`opens the confirm with focus on Cancel, never on Discard`, async () => {
    renderPage()
    fireEvent.click(screen.getByTestId(`issue-draft-discard`))
    const confirm = await screen.findByTestId(`issue-draft-discard-confirm`)
    expect(confirm.getAttribute(`role`)).toBe(`alertdialog`)
    await waitFor(() =>
      expect(document.activeElement?.textContent).toBe(`Cancel`)
    )
  })

  it(`never leaves while a Create is in flight (the discard refused)`, async () => {
    editor.hasContent = false
    editor.discard.mockImplementation(async () => false)
    renderPage()
    fireEvent.click(screen.getByTestId(`issue-draft-discard`))
    await waitFor(() => expect(editor.discard).toHaveBeenCalledTimes(1))
    await act(async () => {})
    expect(navigate).not.toHaveBeenCalled()
  })

  it(`discards at once without content`, async () => {
    editor.hasContent = false
    renderPage()
    fireEvent.click(screen.getByTestId(`issue-draft-discard`))
    await waitFor(() => expect(navigate).toHaveBeenCalled())
    expect(editor.discard).toHaveBeenCalledTimes(1)
    expect(screen.queryByTestId(`issue-draft-discard-confirm`)).toBeNull()
  })
})

describe(`IssueDraftPage leaving`, () => {
  it(`holds in-app navigation to another path only with content`, () => {
    renderPage()
    const shouldBlock = blockerState.shouldBlockFn!
    expect(blockerState.enableBeforeUnload).toBe(false)
    const away = {
      current: { pathname: `/t/acme/drafts/d1` },
      next: { pathname: `/t/acme/inbox` },
    }
    expect(shouldBlock(away)).toBe(true)
    expect(
      shouldBlock({ current: away.current, next: away.current })
    ).toBe(false)
    editor.hasContent = false
    renderPage()
    expect(blockerState.shouldBlockFn!(away)).toBe(false)
  })

  it(`holds nothing while a Create is in flight`, () => {
    renderPage()
    const away = {
      current: { pathname: `/t/acme/drafts/d1` },
      next: { pathname: `/t/acme/inbox` },
    }
    expect(blockerState.shouldBlockFn!(away)).toBe(true)
    // Read live: the flag flips without a re-render.
    editor.creating = true
    expect(blockerState.shouldBlockFn!(away)).toBe(false)
  })

  it(`opens with focus on Create issue, never on Discard`, async () => {
    blockerState.status = `blocked`
    renderPage()
    await screen.findByTestId(`issue-draft-leave-dialog`)
    await waitFor(() =>
      expect(document.activeElement?.textContent).toBe(
        ISSUE_DRAFT_COPY.leave.create
      )
    )
    expect(document.activeElement?.textContent).not.toBe(
      ISSUE_DRAFT_COPY.leave.discard
    )
  })

  it(`opens with focus on Save draft while Create is disabled`, async () => {
    editor.canCreate = false
    blockerState.status = `blocked`
    renderPage()
    await screen.findByTestId(`issue-draft-leave-dialog`)
    await waitFor(() =>
      expect(document.activeElement?.textContent).toBe(
        ISSUE_DRAFT_COPY.leave.keep
      )
    )
  })

  it(`asks one question over Discard · Save draft · Create issue, no ✕`, async () => {
    blockerState.status = `blocked`
    renderPage()
    const dialog = await screen.findByTestId(`issue-draft-leave-dialog`)
    expect(dialog.querySelector(`[data-slot=dialog-description]`)).toBeNull()
    expect(dialog.querySelector(`[data-slot=dialog-close]`)).toBeNull()
    expect(
      Array.from(dialog.querySelectorAll(`button`)).map((b) => b.textContent)
    ).toEqual([
      ISSUE_DRAFT_COPY.leave.discard,
      ISSUE_DRAFT_COPY.leave.keep,
      ISSUE_DRAFT_COPY.leave.create,
    ])
  })

  it(`Save draft whose save fails stays and drops the held navigation`, async () => {
    editor.leave.mockImplementation(async () => false)
    blockerState.status = `blocked`
    renderPage()
    await screen.findByTestId(`issue-draft-leave-dialog`)
    await act(async () => {
      fireEvent.click(
        screen.getByRole(`button`, { name: ISSUE_DRAFT_COPY.leave.keep })
      )
    })
    await waitFor(() => expect(blockerState.reset).toHaveBeenCalledTimes(1))
    expect(editor.leave).toHaveBeenCalledTimes(1)
    expect(blockerState.proceed).not.toHaveBeenCalled()
  })

  it(`a double click answers the held navigation once`, async () => {
    let release: (ok: boolean) => void = () => {}
    editor.leave.mockImplementation(
      () => new Promise<boolean>((resolve) => (release = resolve))
    )
    blockerState.status = `blocked`
    renderPage()
    await screen.findByTestId(`issue-draft-leave-dialog`)
    const keep = screen.getByRole(`button`, {
      name: ISSUE_DRAFT_COPY.leave.keep,
    })
    fireEvent.click(keep)
    fireEvent.click(keep)
    await act(async () => release(true))
    await waitFor(() => expect(blockerState.proceed).toHaveBeenCalledTimes(1))
    expect(editor.leave).toHaveBeenCalledTimes(1)
  })

  it.each([false, true])(
    `Save draft saves, then continues (phone: %s)`,
    async (phone) => {
      mobileState.mobile = phone
      blockerState.status = `blocked`
      renderPage()
      const dialog = await screen.findByTestId(`issue-draft-leave-dialog`)
      expect(dialog.textContent).toContain(ISSUE_DRAFT_COPY.leave.title)
      fireEvent.click(
        screen.getByRole(`button`, { name: ISSUE_DRAFT_COPY.leave.keep })
      )
      await waitFor(() => expect(blockerState.proceed).toHaveBeenCalled())
      expect(editor.leave).toHaveBeenCalledTimes(1)
      expect(blockerState.reset).not.toHaveBeenCalled()
    }
  )

  // EXP-1212: a consumed draft (Discard, Create) REPLACES its history entry
  // with the held destination instead of replaying the push, so Back never
  // lands on an empty New issue page at the consumed id.
  it(`Discard deletes, then replaces the draft entry with the held destination`, async () => {
    blockerState.status = `blocked`
    renderPage()
    const dialog = await screen.findByTestId(`issue-draft-leave-dialog`)
    fireEvent.click(
      Array.from(dialog.querySelectorAll(`button`)).find(
        (button) => button.textContent === ISSUE_DRAFT_COPY.leave.discard
      )!
    )
    await waitFor(() => expect(navigate).toHaveBeenCalledTimes(1))
    expect(editor.discard).toHaveBeenCalledTimes(1)
    expect(navigate.mock.calls[0][0]).toEqual({
      to: HELD.pathname,
      search: HELD.search,
      replace: true,
      ignoreBlocker: true,
    })
    expect(blockerState.reset).toHaveBeenCalledTimes(1)
    expect(blockerState.proceed).not.toHaveBeenCalled()
    expect(screen.queryByTestId(`issue-draft-discard-confirm`)).toBeNull()
  })

  it(`Create files it and replaces the draft entry with the HELD destination`, async () => {
    editor.create.mockResolvedValue({ identifier: `WEB-1`, boardSlug: `web` })
    blockerState.status = `blocked`
    renderPage()
    const dialog = await screen.findByTestId(`issue-draft-leave-dialog`)
    fireEvent.click(
      Array.from(dialog.querySelectorAll(`button`)).find(
        (button) => button.textContent === ISSUE_DRAFT_COPY.leave.create
      )!
    )
    await waitFor(() => expect(navigate).toHaveBeenCalledTimes(1))
    // Never the new issue: the held destination, in place of this entry.
    expect(navigate.mock.calls[0][0]).toMatchObject({
      to: HELD.pathname,
      replace: true,
      ignoreBlocker: true,
    })
    expect(blockerState.reset).toHaveBeenCalledTimes(1)
    expect(blockerState.proceed).not.toHaveBeenCalled()
  })

  it(`a consumed draft on a held Back goes on as held (nothing to replace)`, async () => {
    blockerState.status = `blocked`
    blockerState.action = `BACK`
    renderPage()
    const dialog = await screen.findByTestId(`issue-draft-leave-dialog`)
    fireEvent.click(
      Array.from(dialog.querySelectorAll(`button`)).find(
        (button) => button.textContent === ISSUE_DRAFT_COPY.leave.discard
      )!
    )
    await waitFor(() => expect(blockerState.proceed).toHaveBeenCalledTimes(1))
    expect(editor.discard).toHaveBeenCalledTimes(1)
    expect(navigate).not.toHaveBeenCalled()
    expect(blockerState.reset).not.toHaveBeenCalled()
  })

  it(`a failed Create stays on the page`, async () => {
    editor.create.mockResolvedValue(null)
    blockerState.status = `blocked`
    renderPage()
    const dialog = await screen.findByTestId(`issue-draft-leave-dialog`)
    await act(async () => {
      fireEvent.click(
        Array.from(dialog.querySelectorAll(`button`)).find(
          (button) => button.textContent === ISSUE_DRAFT_COPY.leave.create
        )!
      )
    })
    await waitFor(() => expect(blockerState.reset).toHaveBeenCalled())
    expect(blockerState.proceed).not.toHaveBeenCalled()
  })

  it(`disables Create without a title`, async () => {
    editor.canCreate = false
    blockerState.status = `blocked`
    renderPage()
    const dialog = await screen.findByTestId(`issue-draft-leave-dialog`)
    const create = Array.from(dialog.querySelectorAll(`button`)).find(
      (button) => button.textContent === ISSUE_DRAFT_COPY.leave.create
    )!
    expect(create.disabled).toBe(true)
  })

  it(`dismissing stays on the page`, async () => {
    blockerState.status = `blocked`
    renderPage()
    await screen.findByTestId(`issue-draft-leave-dialog`)
    fireEvent.keyDown(document.activeElement ?? document.body, {
      key: `Escape`,
    })
    await waitFor(() => expect(blockerState.reset).toHaveBeenCalled())
    expect(blockerState.proceed).not.toHaveBeenCalled()
  })
})
