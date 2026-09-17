import { fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it, vi, beforeEach } from "vitest"
import type { Issue } from "@/db/schema"

// EXP-907: the browser gesture on the work tabs — a MIDDLE click anywhere on
// a tab closes it, exactly like its ×, and the browser's own middle-click
// behaviour (autoscroll, open-in-new-tab) never fires. A LIVE tab has no ×
// and takes no middle click either (EXP-877).

const updateWorkTabs = vi.hoisted(() => vi.fn())
const tabs = vi.hoisted(() => ({
  value: [] as unknown[],
}))
const issues = vi.hoisted(() => ({ value: [] as unknown[] }))

vi.mock(`@tanstack/react-router`, () => ({
  useNavigate: () => vi.fn(),
  useRouterState: () => `/t/acme/inbox`,
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
  useCollapsedTabGroups: () => [],
  setTabGroupCollapsed: vi.fn(),
  updateWorkTabs,
}))
vi.mock(`@/components/agent-picker`, () => ({ agentLabel: (a: string) => a }))
vi.mock(`@/components/agent-brand-mark`, () => ({
  AgentBrandMark: () => null,
}))
vi.mock(`@/components/agent-session-row`, () => ({
  LIVE_DOT_TONE_BY_SESSION_TONE: { muted: `muted` },
  RunningIndicator: () => null,
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

  it(`never closes a LIVE tab`, () => {
    tabs.value = [
      { kind: `run`, runId: `r1`, from: null, live: true },
    ]
    renderStrip()
    const tab = screen.getByTestId(`work-tab-run:r1`)
    auxClick(tab, 1)
    expect(updateWorkTabs).not.toHaveBeenCalled()
  })
})
