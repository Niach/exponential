import {
  forwardRef,
  useEffect,
  useImperativeHandle,
  useRef,
  type ComponentPropsWithoutRef,
  type ReactNode,
  type Ref,
} from "react"
import { fireEvent, render, screen, waitFor } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"
import { CreateIssueDialog } from "@/components/create-issue-dialog"
import type { IssueDraft } from "@/db/schema"

// EXP-878: the create dialog is a DRAFT editor that happens to be able to file
// an issue. What these tests lock:
//   * closing with content KEEPS a draft, silently — no confirm on any path;
//   * the row's id is minted once per dialog session and reused by every
//     write, so a close is one idempotent upsert;
//   * uploads are EAGER: a pasted image creates the draft row, uploads
//     against it and lands the FINAL `/api/attachments/{id}` URL in the
//     description — the create path uploads nothing;
//   * create hands the draft over (`draftId`) and never writes it back.

const mockState = vi.hoisted(() => ({
  attachmentFiles: [] as File[],
  draftFiles: [] as File[],
  boards: [] as Array<Record<string, unknown>>,
  createMutate: vi.fn(),
  updateMutate: vi.fn(),
  draftUpsert: vi.fn(),
  draftDelete: vi.fn(),
  draftListAttachments: vi.fn(),
  attachmentDelete: vi.fn(),
}))

const onOpenChange = vi.fn()
const onCreated = vi.fn()
const fetchMock = vi.fn()
const resizeObserver = vi.fn()

vi.mock(`@/lib/trpc-client`, () => ({
  trpc: {
    issues: {
      create: { mutate: mockState.createMutate },
      update: { mutate: mockState.updateMutate },
    },
    issueDrafts: {
      upsert: { mutate: mockState.draftUpsert },
      delete: { mutate: mockState.draftDelete },
      listAttachments: { query: mockState.draftListAttachments },
    },
    attachments: {
      delete: { mutate: mockState.attachmentDelete },
    },
  },
}))

vi.mock(`@tanstack/react-db`, async (importOriginal) => {
  const actual = (await importOriginal()) as Record<string, unknown>

  return {
    ...actual,
    useLiveQuery: () => ({ data: mockState.boards }),
  }
})

vi.mock(`@/components/issue-editor/dialog-shell`, () => ({
  IssueEditorDialogShell: forwardRef(function MockIssueEditorDialogShell(
    {
      boardPicker,
      chipRowAction,
      description,
      disabled,
      editorRef,
      footer,
      formProps,
      imageUpload,
      onDescriptionChange,
      onOpenChange: handleOpenChange,
      onTitleChange,
      title,
    }: {
      boardPicker?: ReactNode
      chipRowAction?: ReactNode
      description: string
      disabled?: boolean
      editorRef?: Ref<{
        focus: () => void
        getMarkdown: () => string
        insertImage: (image: { alt?: string; src: string }) => void
        insertMedia: (media: { label: string; src: string }) => void
        setMarkdown: (markdown: string) => void
      }>
      footer?: ReactNode
      formProps?: ComponentPropsWithoutRef<`form`>
      imageUpload?: {
        onFiles: (files: File[]) => void | Promise<void>
        onOtherFiles?: (files: File[]) => void | Promise<void>
      }
      onDescriptionChange: (markdown: string) => void
      onOpenChange: (open: boolean) => void
      onTitleChange: (value: string) => void
      title: string
    },
    _ref
  ) {
    const markdownRef = useRef(description)

    useEffect(() => {
      markdownRef.current = description
    }, [description])

    useImperativeHandle(editorRef, () => ({
      focus: () => undefined,
      getMarkdown: () => markdownRef.current,
      insertImage: ({ alt, src }) => {
        const nextMarkdown = markdownRef.current
          ? `${markdownRef.current}\n![${alt ?? ``}](${src})`
          : `![${alt ?? ``}](${src})`

        markdownRef.current = nextMarkdown
        onDescriptionChange(nextMarkdown)
      },
      insertMedia: ({ label, src }) => {
        const nextMarkdown = markdownRef.current
          ? `${markdownRef.current}\n[${label}](${src})`
          : `[${label}](${src})`

        markdownRef.current = nextMarkdown
        onDescriptionChange(nextMarkdown)
      },
      setMarkdown: (markdown) => {
        markdownRef.current = markdown
        onDescriptionChange(markdown)
      },
    }))

    return (
      <div data-testid="issue-editor-create">
        {boardPicker}
        <form {...formProps}>
          <input
            aria-label="Issue title"
            value={title}
            disabled={disabled}
            onChange={(event) => onTitleChange(event.target.value)}
          />
          <textarea
            aria-label="Issue description"
            value={description}
            disabled={disabled}
            onChange={(event) => {
              markdownRef.current = event.target.value
              onDescriptionChange(event.target.value)
            }}
          />
          {chipRowAction}
          {footer}
        </form>
        {/* EXP-335: the pickers live in the editor's formatting toolbar now —
            mirror them as plain buttons wired to the imageUpload config. */}
        <button
          type="button"
          aria-label="Add image"
          onClick={() => {
            if (mockState.attachmentFiles.length > 0 && imageUpload) {
              void imageUpload.onFiles(mockState.attachmentFiles)
            }
          }}
        >
          Add image
        </button>
        <button
          type="button"
          aria-label="Attach file"
          onClick={() => {
            if (mockState.draftFiles.length > 0 && imageUpload?.onOtherFiles) {
              void imageUpload.onOtherFiles(mockState.draftFiles)
            }
          }}
        >
          Attach file
        </button>
        {/* EXP-878: there is exactly ONE close path now — Escape, backdrop and
            the ✕ all land here, and none of them asks anything. */}
        <button type="button" onClick={() => handleOpenChange(false)}>
          Close dialog
        </button>
      </div>
    )
  }),
}))

const BOARD = {
  id: `board-1`,
  teamId: `team-1`,
  name: `App`,
  prefix: `APP`,
  slug: `app`,
  color: `#6366f1`,
}

function renderDialog(props: Record<string, unknown> = {}) {
  return render(
    <CreateIssueDialog
      open
      onOpenChange={onOpenChange}
      boardColor="#6366f1"
      boardId="board-1"
      boardPrefix="APP"
      users={[]}
      teamId="team-1"
      teamSlug="acme"
      {...props}
    />
  )
}

function uploadedImageResponse() {
  return {
    ok: true,
    json: async () => ({
      id: `attachment-1`,
      url: `/api/attachments/attachment-1`,
      filename: `draft.png`,
      contentType: `image/png`,
      sizeBytes: 5,
    }),
  }
}

describe(`CreateIssueDialog`, () => {
  beforeEach(() => {
    mockState.attachmentFiles = []
    mockState.draftFiles = []
    mockState.boards = [BOARD]
    mockState.createMutate.mockReset()
    mockState.updateMutate.mockReset()
    mockState.draftUpsert.mockReset()
    mockState.draftDelete.mockReset()
    mockState.draftListAttachments.mockReset()
    mockState.attachmentDelete.mockReset()
    onOpenChange.mockReset()
    onCreated.mockReset()
    fetchMock.mockReset()
    resizeObserver.mockReset()

    mockState.draftUpsert.mockImplementation(async (input) => ({
      draft: { id: input.id },
      txId: 1,
    }))
    mockState.draftDelete.mockResolvedValue({ txId: 2, deleted: true })
    mockState.draftListAttachments.mockResolvedValue([])
    mockState.attachmentDelete.mockResolvedValue({ txId: 3 })

    // cmdk scrolls the active row into view; jsdom has no layout.
    Element.prototype.scrollIntoView ??= function scrollIntoView() {}
    vi.stubGlobal(`fetch`, fetchMock)
    vi.stubGlobal(
      `ResizeObserver`,
      class {
        observe = resizeObserver
        unobserve = vi.fn()
        disconnect = vi.fn()
      }
    )
  })

  // The heart of EXP-878: nothing typed is ever destroyed by a stray Escape.
  it(`keeps a draft on close, with one upsert and no confirm`, async () => {
    renderDialog()

    fireEvent.change(screen.getByLabelText(`Issue title`), {
      target: { value: `Parked for later` },
    })
    fireEvent.click(screen.getByRole(`button`, { name: `Close dialog` }))

    await waitFor(() => {
      expect(mockState.draftUpsert).toHaveBeenCalledTimes(1)
    })
    expect(screen.queryByText(`Discard draft?`)).toBeNull()
    expect(onOpenChange).toHaveBeenCalledWith(false)

    const input = mockState.draftUpsert.mock.calls[0][0]
    expect(input).toMatchObject({
      teamId: `team-1`,
      boardId: `board-1`,
      title: `Parked for later`,
      description: ``,
      priority: `none`,
      labelIds: [],
    })
    expect(typeof input.id).toBe(`string`)
    expect(input.id.length).toBeGreaterThan(0)
    expect(mockState.draftDelete).not.toHaveBeenCalled()
  })

  it(`writes nothing when an untouched dialog closes`, async () => {
    renderDialog()

    fireEvent.change(screen.getByLabelText(`Issue description`), {
      target: { value: `   ` },
    })
    fireEvent.click(screen.getByRole(`button`, { name: `Close dialog` }))

    expect(onOpenChange).toHaveBeenCalledWith(false)
    await waitFor(() => {
      expect(mockState.draftUpsert).not.toHaveBeenCalled()
    })
    expect(mockState.draftDelete).not.toHaveBeenCalled()
  })

  // Emptying an existing draft out is how you throw it away from inside the
  // dialog — the row must go, not linger as a blank row in the list.
  it(`deletes the draft when an existing one is emptied`, async () => {
    const draft = {
      id: `draft-1`,
      userId: `user-1`,
      teamId: `team-1`,
      boardId: `board-1`,
      title: `Was a draft`,
      description: ``,
      statusId: null,
      priority: `none`,
      assigneeId: null,
      labelIds: [],
      dueDate: null,
      createdAt: new Date(),
      updatedAt: new Date(),
    } as unknown as IssueDraft

    renderDialog({ draftId: `draft-1`, draft })

    await waitFor(() => {
      expect(
        (screen.getByLabelText(`Issue title`) as HTMLInputElement).value
      ).toBe(`Was a draft`)
    })

    fireEvent.change(screen.getByLabelText(`Issue title`), {
      target: { value: `` },
    })
    fireEvent.click(screen.getByRole(`button`, { name: `Close dialog` }))

    await waitFor(() => {
      expect(mockState.draftDelete).toHaveBeenCalledWith({ id: `draft-1` })
    })
    expect(mockState.draftUpsert).not.toHaveBeenCalled()
  })

  // Eager uploads: the paste creates the row, posts to the DRAFT route and
  // puts the final attachment URL straight into the description.
  it(`creates the draft row on paste and inserts the uploaded URL`, async () => {
    mockState.attachmentFiles = [
      new File([`image`], `draft.png`, { type: `image/png` }),
    ]
    fetchMock.mockImplementation(async () => uploadedImageResponse())

    renderDialog()

    fireEvent.change(screen.getByLabelText(`Issue description`), {
      target: { value: `Intro paragraph` },
    })
    fireEvent.click(screen.getByLabelText(`Add image`))

    await waitFor(() => {
      expect(
        (screen.getByLabelText(`Issue description`) as HTMLTextAreaElement)
          .value
      ).toBe(`Intro paragraph\n![draft.png](/api/attachments/attachment-1)`)
    })

    expect(mockState.draftUpsert).toHaveBeenCalledTimes(1)
    const draftId = mockState.draftUpsert.mock.calls[0][0].id
    expect(fetchMock).toHaveBeenCalledTimes(1)
    expect(fetchMock.mock.calls[0][0]).toBe(`/api/issue-drafts/${draftId}/files`)
  })

  it(`hands the draft to create, uploads nothing, and reports the board`, async () => {
    mockState.attachmentFiles = [
      new File([`image`], `draft.png`, { type: `image/png` }),
    ]
    fetchMock.mockImplementation(async () => uploadedImageResponse())
    mockState.createMutate.mockImplementation(async (input) => ({
      issue: { id: `issue-1`, identifier: `APP-1`, ...input },
      txId: 7,
    }))

    renderDialog({ onCreated })

    fireEvent.change(screen.getByLabelText(`Issue title`), {
      target: { value: `Draft issue` },
    })
    fireEvent.click(screen.getByLabelText(`Add image`))

    await waitFor(() => {
      expect(fetchMock).toHaveBeenCalledTimes(1)
    })
    const draftId = mockState.draftUpsert.mock.calls[0][0].id

    fireEvent.click(screen.getByRole(`button`, { name: `Create issue` }))

    await waitFor(() => {
      expect(mockState.createMutate).toHaveBeenCalledTimes(1)
    })

    expect(mockState.createMutate).toHaveBeenCalledWith({
      boardId: `board-1`,
      title: `Draft issue`,
      status: `backlog`,
      priority: `none`,
      assigneeId: undefined,
      description: `![draft.png](/api/attachments/attachment-1)`,
      dueDate: undefined,
      labelIds: undefined,
      draftId,
    })
    // The create consumed the draft — nothing writes it back afterwards, and
    // nothing is uploaded after the issue exists any more.
    expect(mockState.draftUpsert).toHaveBeenCalledTimes(1)
    expect(mockState.draftDelete).not.toHaveBeenCalled()
    expect(fetchMock).toHaveBeenCalledTimes(1)
    expect(mockState.updateMutate).not.toHaveBeenCalled()

    expect(onCreated).toHaveBeenCalledWith({
      issue: expect.objectContaining({ identifier: `APP-1` }),
      txId: 7,
      boardSlug: `app`,
    })
  })

  // EXP-449: "Follow the caller's board again on the next open" — a successful
  // create closes the dialog, so the pick must not survive into the next one.
  it(`clears the picked board after a successful create`, async () => {
    mockState.boards = [
      BOARD,
      {
        id: `board-2`,
        teamId: `team-1`,
        name: `Web`,
        prefix: `WEB`,
        slug: `web`,
        color: `#10b981`,
      },
    ]

    mockState.createMutate.mockResolvedValue({
      issue: { id: `issue-1`, identifier: `WEB-1` },
      txId: 9,
    })

    renderDialog({ onCreated })

    // EXP-1021: the board chip is the shared `BoardPicker` — a popover
    // trigger, so it opens on the click a native button makes of an Enter.
    fireEvent.click(screen.getByRole(`button`, { name: /APP/ }))
    fireEvent.click(await screen.findByText(`Web`))

    await waitFor(() => {
      expect(screen.getByRole(`button`, { name: /WEB/ })).toBeTruthy()
    })

    fireEvent.change(screen.getByLabelText(`Issue title`), {
      target: { value: `Filed elsewhere` },
    })
    fireEvent.click(screen.getByRole(`button`, { name: `Create issue` }))

    await waitFor(() => {
      expect(mockState.createMutate).toHaveBeenCalledTimes(1)
    })

    expect(mockState.createMutate).toHaveBeenCalledWith({
      boardId: `board-2`,
      title: `Filed elsewhere`,
      status: `backlog`,
      priority: `none`,
      assigneeId: undefined,
      description: undefined,
      dueDate: undefined,
      labelIds: undefined,
      draftId: undefined,
    })
    expect(onCreated).toHaveBeenCalledWith(
      expect.objectContaining({ boardSlug: `web` })
    )
    expect(onOpenChange).toHaveBeenCalledWith(false)

    await waitFor(() => {
      expect(screen.getByRole(`button`, { name: /APP/ })).toBeTruthy()
    })
  })
})
