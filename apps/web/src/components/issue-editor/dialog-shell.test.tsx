import { forwardRef, useImperativeHandle, type ReactNode } from "react"
import { fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import { IssueEditorDialogShell } from "@/components/issue-editor/dialog-shell"
import { defaultStatusOptions } from "@/lib/team-statuses"

// No TeamStatusesProvider in this render tree, so the shell resolves against
// the CONSTRUCTED default set (EXP-314's unsynced fallback).
const BACKLOG_STATUS = defaultStatusOptions().find(
  (option) => option.builtinKey === `backlog`
)!

const editorFocus = vi.fn()

// The dismiss guards live on DialogContent's Radix props, which the mock below
// would otherwise swallow — capture them so the whitelist can be exercised.
const captured = vi.hoisted(() => ({
  dialogContent: null as Record<string, unknown> | null,
  isMobile: false,
}))

// @exp/ui is a barrel, so the six pieces this test stubs go into ONE partial
// mock — everything else the shell renders (Button, Pill, …) stays real.
vi.mock(`@exp/ui`, async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  // EXP-687: the phone arm is a different header (back arrow + Create pill), so
  // the viewport is a knob here rather than a jsdom accident.
  useIsMobile: () => captured.isMobile,
  Sheet: ({ children }: { children: ReactNode }) => <div>{children}</div>,
  SheetContent: ({ children }: { children: ReactNode }) => <div>{children}</div>,
  SheetTitle: ({ children }: { children: ReactNode }) => <h2>{children}</h2>,
  Dialog: ({ children }: { children: ReactNode }) => <div>{children}</div>,
  DialogContent: ({
    children,
    ...rest
  }: { children: ReactNode } & Record<string, unknown>) => {
    captured.dialogContent = rest
    return <div>{children}</div>
  },
  DialogTitle: ({ children }: { children: ReactNode }) => <h2>{children}</h2>,
  // EXP-941: the due-date control is the shared `DatePicker` now — one stub
  // for the popover + calendar the chips used to wire themselves, reporting
  // the WIRE value the picker reports.
  DatePicker: ({
    onChange,
    renderTrigger,
  }: {
    onChange: (value: string | null) => void
    renderTrigger?: (state: {
      label: string
      value: string | null
    }) => ReactNode
  }) => (
    <div>
      {renderTrigger?.({ label: `Due date`, value: null })}
      <button type="button" onClick={() => onChange(`2026-03-06`)}>
        Pick date
      </button>
    </div>
  ),
  // EXP-1021: the status and priority chips are the TYPED pickers now — the
  // same surface every other picker opens, stubbed down to its trigger and one
  // "pick the second row" button.
  StatusPicker: ({
    onChange,
    statuses,
    trigger,
  }: {
    onChange: (statusId: string) => void
    statuses: Array<{ id: string; name: string }>
    trigger: ReactNode
  }) => (
    <div>
      {trigger}
      <button
        type="button"
        onClick={() => onChange(statuses[1]?.id ?? statuses[0]!.id)}
      >
        Select {statuses[0]!.name}
      </button>
    </div>
  ),
  PriorityPicker: ({
    onChange,
    options,
    trigger,
  }: {
    onChange: (priority: string) => void
    options: Array<{ label: string; value: string }>
    trigger: ReactNode
  }) => (
    <div>
      {trigger}
      <button
        type="button"
        onClick={() => onChange(options[1]?.value ?? options[0]!.value)}
      >
        Select {options[0]!.label}
      </button>
    </div>
  ),
}))

vi.mock(`@/components/issue-editor/mobile-properties`, () => ({
  IssueEditorMobileProperties: () => <div>Mobile properties</div>,
}))

vi.mock(`@/components/issue-editor/markdown-editor`, () => ({
  MarkdownEditor: forwardRef(function MockMarkdownEditor(
    {
      markdown,
      onBlur,
      onChange,
    }: {
      markdown: string
      onBlur?: () => void
      onChange: (markdown: string) => void
    },
    ref
  ) {
    useImperativeHandle(ref, () => ({
      focus: editorFocus,
      setMarkdown: vi.fn(),
      getMarkdown: () => markdown,
      insertImage: vi.fn(),
    }))
    return (
      <textarea
        aria-label="Markdown"
        value={markdown}
        onBlur={onBlur}
        onChange={(event) => onChange(event.target.value)}
      />
    )
  }),
}))

vi.mock(`@/components/issue-properties/assignee-picker`, () => ({
  AssigneePicker: ({
    onSelect,
  }: {
    onSelect: (userId: string | null) => void
  }) => (
    <button type="button" onClick={() => onSelect(`user-2`)}>
      Pick assignee
    </button>
  ),
}))

vi.mock(`@/components/issue-properties/label-picker`, () => ({
  LabelPicker: ({ onToggle }: { onToggle: (labelId: string) => void }) => (
    <button type="button" onClick={() => onToggle(`label-1`)}>
      Toggle label
    </button>
  ),
}))

function baseShellProps() {
  return {
    open: true,
    onOpenChange: vi.fn(),
    boardPrefix: `APP`,
    boardColor: `#6366f1`,
    headerContent: <span>New issue</span>,
    title: `Initial title`,
    onTitleChange: vi.fn(),
    description: `Initial description`,
    onDescriptionChange: vi.fn(),
    status: BACKLOG_STATUS,
    onStatusChange: vi.fn(),
    priority: `none` as const,
    onPriorityChange: vi.fn(),
    teamId: `team-1`,
    selectedLabelIds: [],
    onToggleLabel: vi.fn(),
    users: [],
    assigneeId: null,
    onAssigneeChange: vi.fn(),
    dueDate: null,
    onDueDateSelect: vi.fn(),
    footer: <div>Footer content</div>,
  }
}

describe(`IssueEditorDialogShell`, () => {
  it(`renders the shared shell and forwards key callbacks`, () => {
    const onOpenChange = vi.fn()
    const onTitleChange = vi.fn()
    const onStatusChange = vi.fn()
    const onPriorityChange = vi.fn()
    const onToggleLabel = vi.fn()
    const onAssigneeChange = vi.fn()
    const onDueDateSelect = vi.fn()

    render(
      <IssueEditorDialogShell
        open
        onOpenChange={onOpenChange}
        boardPrefix="APP"
        boardColor="#6366f1"
        headerContent={<span>New issue</span>}
        title="Initial title"
        onTitleChange={onTitleChange}
        description="Initial description"
        onDescriptionChange={vi.fn()}
        status={BACKLOG_STATUS}
        onStatusChange={onStatusChange}
        priority="none"
        onPriorityChange={onPriorityChange}
        teamId="team-1"
        selectedLabelIds={[]}
        onToggleLabel={onToggleLabel}
        users={[]}
        assigneeId={null}
        onAssigneeChange={onAssigneeChange}
        dueDate={null}
        onDueDateSelect={onDueDateSelect}
        footer={<div>Footer content</div>}
      />
    )

    expect(screen.getByText(`New issue`)).toBeTruthy()
    expect(screen.getByText(`Footer content`)).toBeTruthy()

    fireEvent.change(screen.getByPlaceholderText(`Issue title`), {
      target: { value: `Updated title` },
    })
    fireEvent.click(screen.getByLabelText(`Close dialog`))
    // Priorities are display-ordered (REV2-85) so the mock's options[0] is
    // Urgent, while set-status pickers run settings-order (EXP-426) so theirs
    // is Backlog. Either way its "select" picks options[1] — since EXP-685
    // retired Todo, the status list's second row is In Progress.
    fireEvent.click(screen.getByText(`Select Backlog`))
    fireEvent.click(screen.getByText(`Select Urgent`))
    fireEvent.click(screen.getByText(`Toggle label`))
    fireEvent.click(screen.getByText(`Pick assignee`))
    fireEvent.click(screen.getByText(`Pick date`))

    expect(onTitleChange).toHaveBeenCalledWith(`Updated title`)
    expect(onOpenChange).toHaveBeenCalledWith(false)
    expect(onStatusChange).toHaveBeenCalledWith(
      expect.objectContaining({ builtinKey: `in_progress`, name: `In Progress` })
    )
    expect(onPriorityChange).toHaveBeenCalledWith(`high`)
    expect(onToggleLabel).toHaveBeenCalledWith(`label-1`)
    expect(onAssigneeChange).toHaveBeenCalledWith(`user-2`)
    expect(onDueDateSelect).toHaveBeenCalledWith(`2026-03-06`)
  })

  // EXP-10: Tab in the title jumps focus into the description editor instead
  // of cycling through the formatting-toolbar buttons.
  it(`moves focus from the title into the description editor on Tab`, () => {
    editorFocus.mockClear()
    render(<IssueEditorDialogShell {...baseShellProps()} />)

    const titleInput = screen.getByPlaceholderText(`Issue title`)

    fireEvent.keyDown(titleInput, { key: `Tab` })
    expect(editorFocus).toHaveBeenCalledTimes(1)

    // Shift+Tab keeps its default backward behavior — no editor focus.
    fireEvent.keyDown(titleInput, { key: `Tab`, shiftKey: true })
    expect(editorFocus).toHaveBeenCalledTimes(1)
  })

  // EXP-687: the phone New-issue header is a PAGE header — a back arrow where
  // the ✕ was, the title left-aligned, and a labelled Create pill instead of
  // the ArrowUp circle.
  it(`renders the page-style header on a phone`, () => {
    captured.isMobile = true
    try {
      const onOpenChange = vi.fn()
      render(
        <IssueEditorDialogShell
          {...baseShellProps()}
          onOpenChange={onOpenChange}
          primaryAction={{ type: `submit`, label: `Create` }}
        />
      )

      expect(screen.queryByLabelText(`Close dialog`)).toBeNull()
      expect(screen.queryByLabelText(`Submit`)).toBeNull()
      expect(screen.getByText(`Create`)).toBeTruthy()

      fireEvent.click(screen.getByLabelText(`Back`))
      expect(onOpenChange).toHaveBeenCalledWith(false)
    } finally {
      captured.isMobile = false
    }
  })

  it(`does not hijack Tab while the dialog is disabled`, () => {
    editorFocus.mockClear()
    render(<IssueEditorDialogShell {...baseShellProps()} disabled />)

    fireEvent.keyDown(screen.getByPlaceholderText(`Issue title`), {
      key: `Tab`,
    })
    expect(editorFocus).not.toHaveBeenCalled()
  })

  // EXP-568: the formatting rail portals to document.body, so Radix reads
  // every click on it as an interaction OUTSIDE the dialog — and used to
  // close the dialog under the user's finger.
  it(`keeps the dialog open for interactions inside the formatting rail`, () => {
    render(<IssueEditorDialogShell {...baseShellProps()} />)

    const rail = document.createElement(`div`)
    rail.setAttribute(`data-editor-rail`, ``)
    const button = document.createElement(`button`)
    rail.appendChild(button)
    document.body.appendChild(rail)

    const interact = { target: button, preventDefault: vi.fn() }
    ;(
      captured.dialogContent?.onInteractOutside as (event: unknown) => void
    )(interact)
    expect(interact.preventDefault).toHaveBeenCalled()

    const escape = { target: button, preventDefault: vi.fn() }
    ;(
      captured.dialogContent?.onEscapeKeyDown as (event: unknown) => void
    )(escape)
    expect(escape.preventDefault).toHaveBeenCalled()

    rail.remove()
  })

  it(`still dismisses on a genuine outside interaction`, () => {
    render(<IssueEditorDialogShell {...baseShellProps()} />)

    const outside = document.createElement(`div`)
    document.body.appendChild(outside)

    const interact = { target: outside, preventDefault: vi.fn() }
    ;(
      captured.dialogContent?.onInteractOutside as (event: unknown) => void
    )(interact)
    expect(interact.preventDefault).not.toHaveBeenCalled()

    outside.remove()
  })
})
