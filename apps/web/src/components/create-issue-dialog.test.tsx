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

const mockState = vi.hoisted(() => ({
  attachmentFiles: [] as File[],
  // EXP-297: non-image files queued through the rail's second (file) button.
  draftFiles: [] as File[],
  boards: [] as Array<Record<string, unknown>>,
  createMutate: vi.fn(),
  updateMutate: vi.fn(),
}))

const onOpenChange = vi.fn()
const fetchMock = vi.fn()
const createObjectURL = vi.fn()
const revokeObjectURL = vi.fn()
const resizeObserver = vi.fn()

vi.mock(`@/lib/trpc-client`, () => ({
  trpc: {
    issues: {
      create: {
        mutate: mockState.createMutate,
      },
      update: {
        mutate: mockState.updateMutate,
      },
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
      onDismissAttempt,
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
        setMarkdown: (markdown: string) => void
      }>
      footer?: ReactNode
      formProps?: ComponentPropsWithoutRef<`form`>
      imageUpload?: {
        onFiles: (files: File[]) => void | Promise<void>
        onOtherFiles?: (files: File[]) => void | Promise<void>
      }
      onDescriptionChange: (markdown: string) => void
      onDismissAttempt?: () => boolean
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
        <button type="button" onClick={() => handleOpenChange(false)}>
          Close dialog
        </button>
        {/* Mirrors the real shell's Escape / backdrop path: the caller may
            claim the dismissal (REV2-60 discard confirm) by returning true. */}
        <button
          type="button"
          onClick={() => {
            if (onDismissAttempt?.() !== true) {
              handleOpenChange(false)
            }
          }}
        >
          Dismiss dialog
        </button>
      </div>
    )
  }),
}))

describe(`CreateIssueDialog`, () => {
  beforeEach(() => {
    mockState.attachmentFiles = []
    mockState.draftFiles = []
    mockState.boards = []
    mockState.createMutate.mockReset()
    mockState.updateMutate.mockReset()
    onOpenChange.mockReset()
    fetchMock.mockReset()
    createObjectURL.mockReset()
    revokeObjectURL.mockReset()
    resizeObserver.mockReset()

    let blobIndex = 0
    createObjectURL.mockImplementation(() => `blob:mock-image-${++blobIndex}`)

    vi.stubGlobal(`fetch`, fetchMock)
    vi.stubGlobal(
      `ResizeObserver`,
      class {
        observe = resizeObserver
        unobserve = vi.fn()
        disconnect = vi.fn()
      }
    )
    vi.stubGlobal(`URL`, {
      ...URL,
      createObjectURL,
      revokeObjectURL,
    })
  })

  it(`strips draft images from create payload, uploads after create, and saves final markdown`, async () => {
    const events: string[] = []

    mockState.attachmentFiles = [
      new File([`image`], `draft.png`, {
        type: `image/png`,
      }),
    ]

    mockState.createMutate.mockImplementation(async (input) => {
      events.push(`create`)
      return {
        issue: {
          id: `issue-1`,
          identifier: `APP-1`,
          ...input,
        },
      }
    })

    fetchMock.mockImplementation(async () => {
      events.push(`fetch`)
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
    })

    mockState.updateMutate.mockImplementation(async (input) => {
      events.push(`update`)
      return { issue: input }
    })

    render(
      <CreateIssueDialog
        open
        onOpenChange={onOpenChange}
        boardColor="#6366f1"
        boardId="board-1"
        boardPrefix="APP"
        users={[]}
        teamId="team-1"
      />
    )

    fireEvent.change(screen.getByLabelText(`Issue title`), {
      target: { value: `Draft issue` },
    })
    fireEvent.change(screen.getByLabelText(`Issue description`), {
      target: { value: `Intro paragraph` },
    })

    fireEvent.click(screen.getByLabelText(`Add image`))

    await waitFor(() => {
      expect(
        (screen.getByLabelText(`Issue description`) as HTMLTextAreaElement)
          .value
      ).toBe(`Intro paragraph\n![draft.png](blob:mock-image-1)`)
    })
    expect(screen.queryByTestId(`issue-attachment-rail`)).toBeNull()

    fireEvent.click(screen.getByRole(`button`, { name: `Create issue` }))

    await waitFor(() => {
      expect(mockState.createMutate).toHaveBeenCalledTimes(1)
      expect(mockState.updateMutate).toHaveBeenCalledTimes(1)
      expect(fetchMock).toHaveBeenCalledTimes(1)
    })

    expect(mockState.createMutate).toHaveBeenCalledWith({
      boardId: `board-1`,
      title: `Draft issue`,
      status: `backlog`,
      priority: `none`,
      assigneeId: undefined,
      description: `Intro paragraph`,
      dueDate: undefined,
      labelIds: undefined,
    })

    expect(mockState.updateMutate).toHaveBeenCalledWith({
      id: `issue-1`,
      description: `Intro paragraph\n![draft.png](/api/attachments/attachment-1)`,
    })

    expect(events).toEqual([`create`, `fetch`, `update`])
    expect(onOpenChange).toHaveBeenCalledWith(false)
    expect(revokeObjectURL).toHaveBeenCalledWith(`blob:mock-image-1`)
  })

  // EXP-586: images exist only inline — deleting one from the description is
  // the only way to drop it, and its upload must be skipped.
  it(`skips uploads for draft images removed inline from the description`, async () => {
    mockState.attachmentFiles = [
      new File([`image`], `draft.png`, {
        type: `image/png`,
      }),
    ]

    mockState.createMutate.mockResolvedValue({
      issue: {
        id: `issue-1`,
        identifier: `APP-1`,
      },
    })

    render(
      <CreateIssueDialog
        open
        onOpenChange={onOpenChange}
        boardColor="#6366f1"
        boardId="board-1"
        boardPrefix="APP"
        users={[]}
        teamId="team-1"
      />
    )

    fireEvent.change(screen.getByLabelText(`Issue title`), {
      target: { value: `Draft issue` },
    })
    fireEvent.change(screen.getByLabelText(`Issue description`), {
      target: { value: `Intro paragraph` },
    })
    fireEvent.click(screen.getByLabelText(`Add image`))

    await waitFor(() => {
      expect(
        (screen.getByLabelText(`Issue description`) as HTMLTextAreaElement)
          .value
      ).toBe(`Intro paragraph\n![draft.png](blob:mock-image-1)`)
    })

    fireEvent.change(screen.getByLabelText(`Issue description`), {
      target: { value: `Intro paragraph\n` },
    })

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
      description: `Intro paragraph`,
      dueDate: undefined,
      labelIds: undefined,
    })

    expect(fetchMock).not.toHaveBeenCalled()
    expect(mockState.updateMutate).not.toHaveBeenCalled()
    expect(revokeObjectURL).toHaveBeenCalledWith(`blob:mock-image-1`)
  })

  // REV2-60: Escape / backdrop must not silently destroy a typed draft.
  it(`confirms before discarding a dirty draft on Escape or backdrop`, async () => {
    mockState.attachmentFiles = [
      new File([`image`], `draft.png`, {
        type: `image/png`,
      }),
    ]

    render(
      <CreateIssueDialog
        open
        onOpenChange={onOpenChange}
        boardColor="#6366f1"
        boardId="board-1"
        boardPrefix="APP"
        users={[]}
        teamId="team-1"
      />
    )

    fireEvent.change(screen.getByLabelText(`Issue title`), {
      target: { value: `Draft issue` },
    })
    fireEvent.click(screen.getByLabelText(`Add image`))

    await waitFor(() => {
      expect(
        (screen.getByLabelText(`Issue description`) as HTMLTextAreaElement)
          .value
      ).toBe(`![draft.png](blob:mock-image-1)`)
    })

    fireEvent.click(screen.getByRole(`button`, { name: `Dismiss dialog` }))

    expect(await screen.findByText(`Discard draft?`)).toBeTruthy()
    expect(onOpenChange).not.toHaveBeenCalled()
    expect(revokeObjectURL).not.toHaveBeenCalled()

    fireEvent.click(screen.getByRole(`button`, { name: `Keep editing` }))

    await waitFor(() => {
      expect(screen.queryByText(`Discard draft?`)).toBeNull()
    })
    expect(
      (screen.getByLabelText(`Issue title`) as HTMLInputElement).value
    ).toBe(`Draft issue`)

    fireEvent.click(screen.getByRole(`button`, { name: `Dismiss dialog` }))
    expect(await screen.findByText(`Discard draft?`)).toBeTruthy()
    fireEvent.click(screen.getByRole(`button`, { name: `Discard draft` }))

    await waitFor(() => {
      expect(onOpenChange).toHaveBeenCalledWith(false)
    })
    expect(revokeObjectURL).toHaveBeenCalledWith(`blob:mock-image-1`)
    expect(
      (screen.getByLabelText(`Issue title`) as HTMLInputElement).value
    ).toBe(``)
  })

  // EXP-449: "Follow the caller's board again on the next open" — a successful
  // create closes the dialog, so the pick must not survive into the next one.
  it(`clears the picked board after a successful create`, async () => {
    mockState.boards = [
      {
        id: `board-1`,
        teamId: `team-1`,
        name: `App`,
        prefix: `APP`,
        color: `#6366f1`,
      },
      {
        id: `board-2`,
        teamId: `team-1`,
        name: `Web`,
        prefix: `WEB`,
        color: `#10b981`,
      },
    ]

    mockState.createMutate.mockResolvedValue({
      issue: {
        id: `issue-1`,
        identifier: `WEB-1`,
      },
    })

    render(
      <CreateIssueDialog
        open
        onOpenChange={onOpenChange}
        boardColor="#6366f1"
        boardId="board-1"
        boardPrefix="APP"
        users={[]}
        teamId="team-1"
      />
    )

    fireEvent.keyDown(screen.getByRole(`button`, { name: /APP/ }), {
      key: `Enter`,
    })
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
    })
    expect(onOpenChange).toHaveBeenCalledWith(false)

    await waitFor(() => {
      expect(screen.getByRole(`button`, { name: /APP/ })).toBeTruthy()
    })
  })

  it(`dismisses immediately when the draft is empty`, () => {
    render(
      <CreateIssueDialog
        open
        onOpenChange={onOpenChange}
        boardColor="#6366f1"
        boardId="board-1"
        boardPrefix="APP"
        users={[]}
        teamId="team-1"
      />
    )

    fireEvent.change(screen.getByLabelText(`Issue description`), {
      target: { value: `   ` },
    })
    fireEvent.click(screen.getByRole(`button`, { name: `Dismiss dialog` }))

    expect(screen.queryByText(`Discard draft?`)).toBeNull()
    expect(onOpenChange).toHaveBeenCalledWith(false)
  })
})
