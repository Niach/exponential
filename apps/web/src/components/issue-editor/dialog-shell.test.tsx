import { forwardRef, useImperativeHandle, type ReactNode } from "react"
import { fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import { IssueEditorDialogShell } from "@/components/issue-editor/dialog-shell"

const editorFocus = vi.fn()

vi.mock(`@/components/ui/dialog`, () => ({
  Dialog: ({ children }: { children: ReactNode }) => <div>{children}</div>,
  DialogContent: ({ children }: { children: ReactNode }) => (
    <div>{children}</div>
  ),
  DialogTitle: ({ children }: { children: ReactNode }) => <h2>{children}</h2>,
}))

vi.mock(`@/components/ui/popover`, () => ({
  Popover: ({ children }: { children: ReactNode }) => <div>{children}</div>,
  PopoverContent: ({ children }: { children: ReactNode }) => (
    <div>{children}</div>
  ),
  PopoverTrigger: ({ children }: { children: ReactNode }) => (
    <div>{children}</div>
  ),
}))

vi.mock(`@/components/ui/calendar`, () => ({
  Calendar: ({ onSelect }: { onSelect: (date: Date | undefined) => void }) => (
    <button type="button" onClick={() => onSelect(new Date(`2026-03-06`))}>
      Pick date
    </button>
  ),
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

vi.mock(`@/components/option-dropdown-menu`, () => ({
  OptionDropdownMenu: ({
    onSelect,
    options,
    renderTrigger,
  }: {
    onSelect: (value: string) => void
    options: Array<{ label: string; value: string }>
    renderTrigger: (selected: { label: string; value: string }) => ReactNode
  }) => (
    <div>
      {renderTrigger(options[0])}
      <button
        type="button"
        onClick={() => onSelect(options[1]?.value ?? options[0].value)}
      >
        Select {options[0].label}
      </button>
    </div>
  ),
}))

function baseShellProps() {
  return {
    open: true,
    onOpenChange: vi.fn(),
    projectPrefix: `APP`,
    projectColor: `#6366f1`,
    headerContent: <span>New issue</span>,
    title: `Initial title`,
    onTitleChange: vi.fn(),
    description: `Initial description`,
    onDescriptionChange: vi.fn(),
    status: `backlog` as const,
    onStatusChange: vi.fn(),
    priority: `none` as const,
    onPriorityChange: vi.fn(),
    workspaceId: `workspace-1`,
    selectedLabelIds: [],
    onToggleLabel: vi.fn(),
    users: [],
    assigneeId: null,
    onAssigneeChange: vi.fn(),
    dueDate: undefined,
    onDueDateSelect: vi.fn(),
    dueTime: null,
    endTime: null,
    onDueTimeChange: vi.fn(),
    onEndTimeChange: vi.fn(),
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
        projectPrefix="APP"
        projectColor="#6366f1"
        headerContent={<span>New issue</span>}
        title="Initial title"
        onTitleChange={onTitleChange}
        description="Initial description"
        onDescriptionChange={vi.fn()}
        status="backlog"
        onStatusChange={onStatusChange}
        priority="none"
        onPriorityChange={onPriorityChange}
        workspaceId="workspace-1"
        selectedLabelIds={[]}
        onToggleLabel={onToggleLabel}
        users={[]}
        assigneeId={null}
        onAssigneeChange={onAssigneeChange}
        dueDate={undefined}
        onDueDateSelect={onDueDateSelect}
        dueTime={null}
        endTime={null}
        onDueTimeChange={vi.fn()}
        onEndTimeChange={vi.fn()}
        footer={<div>Footer content</div>}
      />
    )

    expect(screen.getByText(`New issue`)).toBeTruthy()
    expect(screen.getByText(`Footer content`)).toBeTruthy()

    fireEvent.change(screen.getByPlaceholderText(`Issue title`), {
      target: { value: `Updated title` },
    })
    fireEvent.click(screen.getByLabelText(`Close dialog`))
    fireEvent.click(screen.getByText(`Select Backlog`))
    fireEvent.click(screen.getByText(`Select No priority`))
    fireEvent.click(screen.getByText(`Toggle label`))
    fireEvent.click(screen.getByText(`Pick assignee`))
    fireEvent.click(screen.getByText(`Pick date`))

    expect(onTitleChange).toHaveBeenCalledWith(`Updated title`)
    expect(onOpenChange).toHaveBeenCalledWith(false)
    expect(onStatusChange).toHaveBeenCalledWith(`todo`)
    expect(onPriorityChange).toHaveBeenCalledWith(`urgent`)
    expect(onToggleLabel).toHaveBeenCalledWith(`label-1`)
    expect(onAssigneeChange).toHaveBeenCalledWith(`user-2`)
    expect(onDueDateSelect).toHaveBeenCalled()
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

  it(`does not hijack Tab while the dialog is disabled`, () => {
    editorFocus.mockClear()
    render(<IssueEditorDialogShell {...baseShellProps()} disabled />)

    fireEvent.keyDown(screen.getByPlaceholderText(`Issue title`), {
      key: `Tab`,
    })
    expect(editorFocus).not.toHaveBeenCalled()
  })
})
