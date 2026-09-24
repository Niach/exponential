import { act, fireEvent, render, screen } from "@testing-library/react"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"
import { IssueChip, type IssueChipIssue } from "@/components/issue-chip"

// EXP-1024: THE issue chip is hoverable on a pointer device — the app's
// `IssueChip` wraps the box in `IssuePreviewHoverCard` by default, ALSO when
// the body is a caller-supplied link (the workflow node panel's badge, the
// mini-graph's rows). `issue-chip.test.tsx` runs as a phone, where the hover
// host self-disables; this file is the desktop half of that lock.
//
// The preview reads the synced shapes through `useLiveQuery`; here each query
// answers by the collection it reads from, so the card's issue row is the one
// the chip names and the label/user queries stay empty.

vi.mock(`@exp/ui`, async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  useIsMobile: () => false,
}))

const liveRows = vi.hoisted(() => ({ byTable: {} as Record<string, unknown[]> }))
vi.mock(`@tanstack/react-db`, async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  useLiveQuery: (build: (query: unknown) => unknown) => {
    let table: string | null = null
    const query = {
      from(source: Record<string, unknown>) {
        table = Object.keys(source)[0] ?? null
        return { where: () => ({}) }
      },
    }
    // `undefined` is the skipped query.
    if (build(query) === undefined) return { data: [] }
    return { data: table ? (liveRows.byTable[table] ?? []) : [] }
  },
}))
vi.mock(`@/lib/collections`, () => ({
  issueCollection: {},
  issueLabelCollection: {},
  labelCollection: {},
  userCollection: {},
}))

const issue: IssueChipIssue = {
  id: `i1`,
  identifier: `APP-12`,
  title: `Android issue badges have no status`,
  status: `in_progress`,
  statusId: null,
}

function previewCard(): Element | null {
  return document.body.querySelector(`[data-slot="hover-card-content"]`)
}

describe(`IssueChip hover preview`, () => {
  beforeEach(() => {
    vi.useFakeTimers()
    liveRows.byTable = {
      issues: [
        {
          id: `i1`,
          identifier: `APP-12`,
          title: `Android issue badges have no status`,
          status: `in_progress`,
          statusId: null,
          priority: `high`,
          assigneeId: null,
        },
      ],
    }
  })
  afterEach(() => {
    vi.useRealTimers()
  })

  it(`opens the issue preview when a LINKED chip is hovered`, () => {
    render(
      <IssueChip
        issue={issue}
        testId="chip"
        link={(props) => <a href="/t/acme/boards/app/issues/APP-12" {...props} />}
      />
    )
    const chip = screen.getByTestId(`chip`)
    // The link body is still there under the hover host.
    expect(chip.querySelector(`a`)?.getAttribute(`aria-label`)).toBe(
      `Open APP-12`
    )
    expect(previewCard()).toBeNull()

    fireEvent.pointerEnter(chip)
    // The product's 400 ms open delay (`@exp/ui` hover-card.tsx).
    act(() => {
      vi.advanceTimersByTime(500)
    })
    const card = previewCard()
    expect(card).not.toBeNull()
    expect(card!.textContent).toContain(`APP-12`)
    expect(card!.textContent).toContain(`Android issue badges have no status`)
    // The priority pill: the card is the real preview, not a tooltip.
    expect(card!.textContent).toContain(`High`)
  })

  it(`stays a bare chip when the preview is opted out`, () => {
    render(<IssueChip issue={issue} testId="chip" preview={false} />)
    fireEvent.pointerEnter(screen.getByTestId(`chip`))
    act(() => {
      vi.advanceTimersByTime(500)
    })
    expect(previewCard()).toBeNull()
  })
})
