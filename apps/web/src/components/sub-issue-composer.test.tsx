import {
  forwardRef,
  useImperativeHandle,
  type Ref,
} from "react"
import { fireEvent, render, screen, waitFor } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"
import { SubIssueComposer } from "@/components/sub-issue-composer"
import type { Issue, User } from "@/db/schema"

// EXP-760 — the inline "Add sub-issues" composer. What matters here is the
// SHAPE of the one write it makes (`parentId` + the parent's board, so the
// relation lands in the create transaction) and its "keep filing" behaviour:
// after a create the form stays open with its chips intact and only the text
// clears, because sub-issues are filed in runs.

const mockState = vi.hoisted(() => ({
  createMutate: vi.fn(),
  awaitTxId: vi.fn(),
}))

vi.mock(`@/lib/trpc-client`, () => ({
  trpc: { issues: { create: { mutate: mockState.createMutate } } },
}))

vi.mock(`@/lib/collections`, () => ({
  issueCollection: { utils: { awaitTxId: mockState.awaitTxId } },
}))

// The real editor is a TipTap instance; here it only has to report what was
// typed and honour setMarkdown('') on a successful create.
vi.mock(`@/components/issue-editor/markdown-editor`, () => ({
  MarkdownEditor: forwardRef(function MockMarkdownEditor(
    {
      markdown,
      onChange,
      placeholder,
    }: {
      markdown: string
      onChange: (value: string) => void
      placeholder?: string
    },
    ref: Ref<{ setMarkdown: (value: string) => void }>
  ) {
    useImperativeHandle(ref, () => ({
      setMarkdown: (value: string) => onChange(value),
    }))
    return (
      <textarea
        aria-label="description"
        placeholder={placeholder}
        value={markdown}
        onChange={(event) => onChange(event.target.value)}
      />
    )
  }),
}))

// The chip row's pickers ride team collections we don't mount; the composer's
// contract with them is just "they change the state the create call reads".
vi.mock(`@/components/issue-editor/chips`, () => ({
  IssueEditorChips: ({
    hideAssignee,
    hideDueDateChip,
    onPriorityChange,
  }: {
    hideAssignee?: boolean
    hideDueDateChip?: boolean
    onPriorityChange: (priority: string) => void
  }) => (
    <div>
      <span>{hideAssignee ? `assignee-hidden` : `assignee-shown`}</span>
      <span>{hideDueDateChip ? `due-hidden` : `due-shown`}</span>
      <button type="button" onClick={() => onPriorityChange(`urgent`)}>
        Set urgent
      </button>
    </div>
  ),
}))

const parent = {
  id: `parent-1`,
  boardId: `board-1`,
  teamId: `team-1`,
  identifier: `APP-1`,
  title: `Parent`,
  status: `backlog`,
  statusId: null,
} as unknown as Issue

const alice = { id: `user-1`, name: `Alice` } as unknown as User
const bob = { id: `user-2`, name: `Bob` } as unknown as User

function open(users: User[] = [alice, bob]) {
  render(<SubIssueComposer parent={parent} teamId="team-1" users={users} />)
  fireEvent.click(screen.getByText(`Add sub-issues`))
}

describe(`SubIssueComposer`, () => {
  beforeEach(() => {
    mockState.createMutate.mockReset()
    mockState.createMutate.mockResolvedValue({
      issue: { id: `issue-9` },
      txId: `tx-9`,
    })
    mockState.awaitTxId.mockReset()
    mockState.awaitTxId.mockResolvedValue(undefined)
  })

  // Closed by default: an always-open second editor under every description
  // would be louder than the description itself.
  it(`starts as a single affordance and opens the form on click`, () => {
    render(
      <SubIssueComposer parent={parent} teamId="team-1" users={[alice, bob]} />
    )
    expect(screen.queryByPlaceholderText(`Sub-issue title`)).toBeNull()

    fireEvent.click(screen.getByText(`Add sub-issues`))
    expect(screen.getByPlaceholderText(`Sub-issue title`)).not.toBeNull()
  })

  it(`creates the child on the parent's board with parentId`, async () => {
    open()
    fireEvent.change(screen.getByPlaceholderText(`Sub-issue title`), {
      target: { value: `  Ship it  ` },
    })
    fireEvent.change(screen.getByLabelText(`description`), {
      target: { value: `Some detail` },
    })
    fireEvent.click(screen.getByText(`Create`))

    await waitFor(() => {
      expect(mockState.createMutate).toHaveBeenCalledTimes(1)
    })
    expect(mockState.createMutate).toHaveBeenCalledWith(
      expect.objectContaining({
        boardId: `board-1`,
        parentId: `parent-1`,
        title: `Ship it`,
        description: `Some detail`,
      })
    )
    // The awaited txId is what makes the new row (and its parent relation)
    // present before the form clears.
    await waitFor(() => {
      expect(mockState.awaitTxId).toHaveBeenCalledWith(`tx-9`)
    })
  })

  it(`submits on Enter and stays open with the text cleared`, async () => {
    open()
    const title = screen.getByPlaceholderText(`Sub-issue title`)
    fireEvent.change(title, { target: { value: `First` } })
    fireEvent.keyDown(title, { key: `Enter` })

    await waitFor(() => {
      expect(mockState.createMutate).toHaveBeenCalledTimes(1)
    })
    // Still open, ready for the next one — and the title is empty again.
    await waitFor(() => {
      expect(
        (screen.getByPlaceholderText(`Sub-issue title`) as HTMLInputElement)
          .value
      ).toBe(``)
    })
  })

  it(`carries the chip picks into the create call`, async () => {
    open()
    fireEvent.click(screen.getByText(`Set urgent`))
    fireEvent.change(screen.getByPlaceholderText(`Sub-issue title`), {
      target: { value: `Urgent one` },
    })
    fireEvent.click(screen.getByText(`Create`))

    await waitFor(() => {
      expect(mockState.createMutate).toHaveBeenCalledWith(
        expect.objectContaining({ priority: `urgent` })
      )
    })
  })

  it(`never fires on an empty title, and Escape closes the form`, () => {
    open()
    fireEvent.click(screen.getByText(`Create`))
    expect(mockState.createMutate).not.toHaveBeenCalled()

    fireEvent.keyDown(screen.getByPlaceholderText(`Sub-issue title`), {
      key: `Escape`,
    })
    expect(screen.queryByPlaceholderText(`Sub-issue title`)).toBeNull()
    expect(screen.getByText(`Add sub-issues`)).not.toBeNull()
  })

  // Mirrors the create dialog: a solo team has nobody to choose between, and
  // the server defaults the assignee anyway. The due-date chip is always out.
  it(`hides the assignee chip in a solo team only`, () => {
    open([alice])
    expect(screen.getByText(`assignee-hidden`)).not.toBeNull()
    expect(screen.getByText(`due-hidden`)).not.toBeNull()
  })

  it(`shows the assignee chip in a multi-member team`, () => {
    open()
    expect(screen.getByText(`assignee-shown`)).not.toBeNull()
  })
})
