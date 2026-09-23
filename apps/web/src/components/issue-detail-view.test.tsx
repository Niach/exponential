import * as React from "react"
import { act, fireEvent, render, screen } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"
import type { Board, Issue } from "@/db/schema"
import { IssueDetailView } from "@/components/issue-detail-view"

const updateMutate = vi.hoisted(() => vi.fn(async () => ({})))

vi.mock(`@/lib/trpc-client`, () => ({
  trpc: {
    issues: { update: { mutate: updateMutate } },
    notifications: { markReadByIssue: { mutate: vi.fn(async () => ({})) } },
    comments: { create: { mutate: vi.fn(async () => ({})) } },
  },
}))
// Importing the real collections module opens Electric shapes.
vi.mock(`@/lib/collections`, () => ({ issueCollection: {}, teamCollection: {} }))
// EXP-630: the estimate scale rides the team row; no team here.
vi.mock(`@/hooks/use-team-data`, async (importOriginal) => ({
  // eslint-disable-next-line quotes -- esbuild rejects template literals inside typeof import()
  ...(await importOriginal<typeof import("@/hooks/use-team-data")>()),
  useTeamById: () => null,
}))
vi.mock(`@/hooks/use-session`, () => ({
  useSession: () => ({ data: { user: { id: `u1` } } }),
}))
vi.mock(`@/hooks/use-team-statuses`, () => ({
  useTeamStatusesContext: () => ({ resolve: () => null }),
}))
vi.mock(`@/hooks/use-issue-property-handlers`, () => ({
  useIssuePropertyHandlers: () => ({
    issueLabelIds: [],
    duplicatePicker: null,
    handleBoardChange: vi.fn(),
    handleUnmarkDuplicate: vi.fn(),
  }),
}))
vi.mock(`@/hooks/use-remembered-scroll`, () => ({
  useRememberedScroll: () => ({ current: null }),
}))
vi.mock(`@/components/issue-ref-provider`, () => ({ useIssueRefs: () => null }))
// Every sibling of the description is scenery for this test.
vi.mock(`@/components/issue-timeline`, () => ({ IssueTimeline: () => null }))
vi.mock(`@/components/issue-coding-rows`, () => ({
  IssueCodingControl: () => null,
  IssuePrRow: () => null,
}))
vi.mock(`@/components/issue-detail-mobile-bar`, () => ({
  IssueDetailMobileBar: () => null,
}))
vi.mock(`@/components/issue-mobile-header`, () => ({
  IssueMobileHeader: () => null,
  TitleStateDot: () => null,
}))
vi.mock(`@/components/issue-editor/mobile-properties`, () => ({
  IssueEditorMobileProperties: () => null,
}))
vi.mock(`@/components/issue-files-section`, () => ({
  IssueFilesSection: () => null,
}))
vi.mock(`@/components/issue-relations-card`, () => ({
  IssueRelationsSection: () => null,
}))
vi.mock(`@/components/issue-chip`, () => ({ IssueChip: () => null }))
vi.mock(`@/components/sub-issue-composer`, () => ({
  SubIssueComposer: () => null,
}))
vi.mock(`@/components/pin-toggle-button`, () => ({
  PinToggleButton: () => null,
}))
vi.mock(`@/components/widget-submission-card`, () => ({
  WidgetSubmissionCard: () => null,
}))
vi.mock(`@/components/issue-actions-menu`, () => ({
  IssueActionsMenu: () => null,
}))
vi.mock(`@/components/issue-properties-tray`, () => ({
  IssuePropertiesTray: () => null,
}))
vi.mock(`@/components/issue-title-field`, () => ({
  IssueTitleField: () => null,
}))
vi.mock(`@/components/pr-graph-badge`, () => ({ PrGraphBadge: () => null }))
vi.mock(`@/lib/storage/issue-image-upload`, () => ({
  uploadIssueFile: vi.fn(),
  uploadIssueImageFile: vi.fn(),
}))
vi.mock(`@/lib/storage/media-upload`, () => ({
  mediaPlayabilityHint: () => null,
  prepareMediaUpload: vi.fn(),
  uploadIssueMediaFile: vi.fn(),
}))

// A stand-in for the TipTap editor with the one contract this view uses: it
// is NOT re-seeded from its `markdown` prop (the real one is not either) —
// the view drives it through `setMarkdown`, which re-enters `onChange` with
// the editor's own serialization.
vi.mock(`@/components/issue-editor/markdown-editor`, () => ({
  MarkdownEditor: React.forwardRef(function FakeEditor(
    {
      markdown,
      onChange,
      onBlur,
    }: {
      markdown: string
      onChange?: (next: string) => void
      onBlur?: () => void
    },
    ref: React.Ref<unknown>
  ) {
    const [value, setValue] = React.useState(markdown)
    const valueRef = React.useRef(markdown)
    const write = (next: string) => {
      valueRef.current = next
      setValue(next)
      onChange?.(next)
    }
    React.useImperativeHandle(ref, () => ({
      getMarkdown: () => valueRef.current,
      setMarkdown: (next: string) => write(next),
      insertImage: vi.fn(),
      appendImage: vi.fn(),
      insertMedia: vi.fn(),
      appendMedia: vi.fn(),
    }))
    return (
      <textarea
        data-testid="description-editor"
        value={value}
        onChange={(event) => write(event.target.value)}
        onBlur={() => onBlur?.()}
      />
    )
  }),
}))

const board = { id: `b1`, slug: `met` } as unknown as Board

const makeIssue = (id: string, description: string | null): Issue =>
  ({
    id,
    identifier: id.toUpperCase(),
    teamId: `t1`,
    boardId: `b1`,
    title: `Do the thing`,
    description,
    duplicateOfId: null,
    source: `user`,
    prUrl: null,
  }) as unknown as Issue

function renderDetail(issue: Issue) {
  const view = render(
    <IssueDetailView
      issue={issue}
      users={[]}
      board={board}
      teamSlug="acme"
      teamId="t1"
    />
  )
  return {
    ...view,
    show: (next: Issue) =>
      view.rerender(
        <IssueDetailView
          issue={next}
          users={[]}
          board={board}
          teamSlug="acme"
          teamId="t1"
        />
      ),
  }
}

const editor = () =>
  screen.getByTestId(`description-editor`) as HTMLTextAreaElement

const type = (text: string) => {
  fireEvent.change(editor(), { target: { value: text } })
}

const blur = async () => {
  await act(async () => {
    fireEvent.blur(editor())
    await Promise.resolve()
  })
}

// EXP-928: the view is reused across issue switches and the synced row only
// catches up with its save's Electric echo — so leaving an issue right after a
// blur save and coming straight back used to show (and then re-save) the text
// the save replaced.
describe(`IssueDetailView description`, () => {
  beforeEach(() => {
    updateMutate.mockClear()
  })

  it(`keeps the saved text when the issue is left and reopened before the echo`, async () => {
    const view = renderDetail(makeIssue(`i1`, `old text`))
    expect(editor().value).toBe(`old text`)

    type(`new text`)
    await blur()
    expect(updateMutate).toHaveBeenCalledWith({
      id: `i1`,
      description: `new text`,
    })

    // Away to another issue…
    view.show(makeIssue(`i2`, `other issue`))
    expect(editor().value).toBe(`other issue`)
    // …and straight back, while the row still reads the pre-save text.
    view.show(makeIssue(`i1`, `old text`))
    expect(editor().value).toBe(`new text`)
  })

  it(`saves the saved text plus the new typing when the echo is still out`, async () => {
    const view = renderDetail(makeIssue(`i3`, `old text`))
    type(`new text`)
    await blur()
    updateMutate.mockClear()

    view.show(makeIssue(`i4`, `other issue`))
    view.show(makeIssue(`i3`, `old text`))

    // Typing on top of whatever the view is showing: the next blur must send
    // the SAVE plus the typing, never the pre-save row plus the typing.
    type(`${editor().value} and more`)
    await blur()
    expect(updateMutate).toHaveBeenCalledWith({
      id: `i3`,
      description: `new text and more`,
    })
  })

  it(`still applies a remote edit made after the save`, async () => {
    const view = renderDetail(makeIssue(`i5`, `old text`))
    type(`new text`)
    await blur()

    // The row moved to something that is neither the pre-save text nor our
    // save: somebody else wrote it, and it wins.
    view.show(makeIssue(`i5`, `remote rewrite`))
    expect(editor().value).toBe(`remote rewrite`)
  })

  it(`lets the row win again when the save failed`, async () => {
    updateMutate.mockRejectedValueOnce(new Error(`nope`))
    const view = renderDetail(makeIssue(`i6`, `old text`))
    type(`new text`)
    await blur()

    view.show(makeIssue(`i7`, `other issue`))
    view.show(makeIssue(`i6`, `old text`))
    expect(editor().value).toBe(`old text`)
  })
})
