import { fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it, vi, beforeEach } from "vitest"
import type { Issue } from "@/db/schema"

// EXP-907: the browser gesture on the work tabs — a MIDDLE click anywhere on
// a tab closes it, exactly like its ×, and the browser's own middle-click
// behaviour (autoscroll, open-in-new-tab) never fires. EXP-923: EVERY tab
// takes it, a live one included — the "permanent" live tab is gone.

const updateWorkTabs = vi.hoisted(() => vi.fn())
const openNewRunPage = vi.hoisted(() => vi.fn())
const pathname = vi.hoisted(() => ({ value: `/t/acme/inbox` }))
const tabs = vi.hoisted(() => ({
  value: [] as unknown[],
}))
const issues = vi.hoisted(() => ({ value: [] as unknown[] }))

vi.mock(`@tanstack/react-router`, () => ({
  useNavigate: () => vi.fn(),
  useRouterState: () => pathname.value,
}))
vi.mock(`@tanstack/react-db`, async (importOriginal) => {
  const actual = (await importOriginal()) as Record<string, unknown>
  return { ...actual, useLiveQuery: () => ({ data: issues.value }) }
})
vi.mock(`@/lib/collections`, () => ({
  codingSessionCollection: {},
  issueCollection: {},
}))
vi.mock(`@/lib/trpc-client`, () => ({ trpc: {} }))
vi.mock(`@/hooks/use-work-tabs`, () => ({
  useWorkTabs: () => ({ tabs: tabs.value }),
  updateWorkTabs,
  openNewRunPage,
}))
vi.mock(`@/lib/session-row-caption`, () => ({
  LIVE_DOT_TONE_BY_SESSION_TONE: { muted: `muted` },
}))
vi.mock(`@/components/issue-properties/status-dropdown`, () => ({
  IssueStatusIcon: () => null,
}))

import { WorkTabsStrip } from "@/components/team/work-tabs-strip"

const issue = {
  id: `i1`,
  identifier: `EXP-1`,
  title: `Fix the sync loop`,
  boardId: `b1`,
} as unknown as Issue

/** jsdom's fireEvent has no `auxClick` helper — the real event, as a browser
 *  fires it. */
function auxClick(node: Element, button: number) {
  fireEvent(
    node,
    new MouseEvent(`auxclick`, { bubbles: true, cancelable: true, button })
  )
}

function renderStrip() {
  return render(
    <WorkTabsStrip
      teamId="t1"
      teamSlug="acme"
      boards={[{ id: `b1`, slug: `web` }] as never}
    />
  )
}

describe(`WorkTabsStrip middle click (EXP-907)`, () => {
  beforeEach(() => {
    updateWorkTabs.mockClear()
    issues.value = [issue]
    pathname.value = `/t/acme/inbox`
  })

  it(`closes the tab on a middle click, and swallows the browser's own`, () => {
    tabs.value = [
      { kind: `issue`, issueId: `i1`, face: `issue`, runId: null, from: null, live: false },
    ]
    renderStrip()
    const tab = screen.getByTestId(`work-tab-issue:i1`)

    // The mousedown is taken, or Chrome opens its autoscroll cursor first.
    const down = fireEvent.mouseDown(tab, { button: 1 })
    expect(down).toBe(false)

    auxClick(tab, 1)
    expect(updateWorkTabs).toHaveBeenCalledTimes(1)
    expect(updateWorkTabs.mock.calls[0]![0]).toBe(`t1`)
  })

  it(`leaves a left click and a right click alone`, () => {
    tabs.value = [
      { kind: `issue`, issueId: `i1`, face: `issue`, runId: null, from: null, live: false },
    ]
    renderStrip()
    const tab = screen.getByTestId(`work-tab-issue:i1`)
    fireEvent.mouseDown(tab, { button: 0 })
    auxClick(tab, 2)
    expect(updateWorkTabs).not.toHaveBeenCalled()
  })

  // EXP-923: a live run has no chip of its own any more, but a tab that is
  // BOUND to one (you opened its issue) closes like every other.
  it(`closes a tab bound to a live run`, () => {
    tabs.value = [{ kind: `run`, runId: `r1`, from: null, live: true }]
    renderStrip()
    const tab = screen.getByTestId(`work-tab-run:r1`)
    expect(screen.getAllByLabelText(`Close tab`).length).toBeGreaterThan(0)
    auxClick(tab, 1)
    expect(updateWorkTabs).toHaveBeenCalledTimes(1)
  })
})

// EXP-1255: the × shows on hover only (every tab, the active one too; a
// keyboard focus shows it), a "+" after the last tab opens the New run tab,
// and the New run chip is the Agent page's tab.
describe(`WorkTabsStrip New run (EXP-1255)`, () => {
  const issueTab = { kind: `issue`, issueId: `i1`, face: `issue`, runId: null, from: null, live: false }

  beforeEach(() => {
    updateWorkTabs.mockClear()
    openNewRunPage.mockClear()
    issues.value = [issue]
    pathname.value = `/t/acme/inbox`
  })

  it(`hides every × until the tab is hovered or the × is focused`, () => {
    tabs.value = [issueTab]
    pathname.value = `/t/acme/boards/web/issues/EXP-1`
    renderStrip()
    const tab = screen.getByTestId(`work-tab-issue:i1`)
    expect(tab.getAttribute(`data-active`)).toBe(`true`)
    const close = tab.querySelector(`[aria-label="Close tab"]`)!
    expect(close.className).toContain(`opacity-0`)
    expect(close.className).toContain(`group-hover/tab:opacity-100`)
    expect(close.className).toContain(`focus-visible:opacity-100`)
  })

  it(`puts the "+" after the last tab and the "+N", and it opens a New run tab`, () => {
    tabs.value = [issueTab]
    renderStrip()
    const plus = screen.getByTestId(`work-tabs-new-run`)
    expect(plus.getAttribute(`aria-label`)).toBe(`New run`)
    const tab = screen.getByTestId(`work-tab-issue:i1`)
    expect(tab.compareDocumentPosition(plus) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy()
    expect(plus.nextElementSibling).toBeNull()
    fireEvent.click(plus)
    expect(openNewRunPage).toHaveBeenCalledWith(`t1`, `acme`, expect.anything())
  })

  it(`draws the New run chip, active on the Agent page`, () => {
    tabs.value = [issueTab, { kind: `new`, runId: null, from: null, live: false }]
    pathname.value = `/t/acme/agent`
    renderStrip()
    const chip = screen.getByTestId(`work-tab-new`)
    expect(chip.textContent).toContain(`New run`)
    expect(chip.getAttribute(`data-active`)).toBe(`true`)
    expect(screen.getByTestId(`work-tab-issue:i1`).getAttribute(`data-active`)).toBeNull()
  })
})

// EXP-1259 D8: a tab whose subject has not synced yet keeps its chip but
// draws a short placeholder bar in the label slot, never "Loading…".
describe(`WorkTabsStrip unloaded subject (D8)`, () => {
  beforeEach(() => {
    issues.value = []
    pathname.value = `/t/acme/inbox`
  })

  it(`keeps the tab and draws the placeholder bar instead of text`, () => {
    tabs.value = [
      { kind: `issue`, issueId: `i9`, face: `issue`, runId: null, from: null, live: false },
    ]
    renderStrip()
    const tab = screen.getByTestId(`work-tab-issue:i9`)
    expect(
      tab.querySelector(`[data-testid=work-tab-title-placeholder]`)
    ).not.toBeNull()
    expect(screen.queryByText(`Loading…`)).toBeNull()
  })
})
