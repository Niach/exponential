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

vi.mock(`@/components/issue-editor/dialog-shell`, () => ({
  IssueEditorDialogShell: forwardRef(function MockIssueEditorDialogShell(
    {
      description,
      disabled,
      editorRef,
      footer,
      formProps,
      onDescriptionChange,
      onDismissAttempt,
      onOpenChange: handleOpenChange,
      onTitleChange,
      title,
    }: {
      description: string
      disabled?: boolean
      editorRef?: Ref<{
        focus: () => void
        getMarkdown: () => string
        insertImage: (image: { alt?: string; src: string }) => void
        setMarkdown: (markdown: string) => void
      }>
      footer: ReactNode
      formProps?: ComponentPropsWithoutRef<`form`>
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
          {footer}
        </form>
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
  IssueEditorAttachmentButton: ({
    disabled,
    label = `Add image`,
    onFiles,
    uploading,
  }: {
    disabled?: boolean
    label?: string
    onFiles?: (files: File[]) => void | Promise<void>
    uploading?: boolean
  }) => (
    <button
      type="button"
      aria-label={label}
      disabled={disabled || uploading}
      onClick={() => {
        const queued =
          label === `Attach file`
            ? mockState.draftFiles
            : mockState.attachmentFiles

        if (queued.length > 0 && onFiles) {
          void onFiles(queued)
        }
      }}
    >
      {label}
    </button>
  ),
}))

describe(`CreateIssueDialog`, () => {
  beforeEach(() => {
    mockState.attachmentFiles = []
    mockState.draftFiles = []
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
    expect(screen.getByTestId(`issue-attachment-rail`)).toBeTruthy()
    expect(screen.getByText(`draft.png`)).toBeTruthy()
    expect(screen.getByText(`1 image`)).toBeTruthy()

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

  it(`removes footer chips by occurrence and skips removed draft uploads`, async () => {
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

    fireEvent.click(
      screen.getByRole(`button`, { name: `Remove attachment draft.png` })
    )

    await waitFor(() => {
      expect(
        (screen.getByLabelText(`Issue description`) as HTMLTextAreaElement)
          .value
      ).toBe(`Intro paragraph\n`)
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
      expect(screen.getByText(`draft.png`)).toBeTruthy()
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
