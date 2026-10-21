import { fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it, vi, beforeEach } from "vitest"
import type { Issue } from "@/db/schema"

// EXP-907: the browser gesture on the work tabs — a MIDDLE click anywhere on
// a tab closes it, exactly like its ×, and the browser's own middle-click
// behaviour (autoscroll, open-in-new-tab) never fires. EVERY tab takes it.

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
  issueCollection: {},
  sessionCollection: { get: () => ({ cli: `claude` }) },
}))
const threadGroups = vi.hoisted(() => ({ value: [] as unknown[] }))
const agents = vi.hoisted(() => ({ value: [] as unknown[] }))
vi.mock(`@/hooks/use-session-tree`, () => ({
  useThreadsList: () => ({
    groups: threadGroups.value,
    isReady: true,
  }),
  useAgents: () => ({ agents: agents.value, isReady: true }),
  useSessionTitles: () => ({ titles: new Map(), held: new Set(), ready: true }),
}))
vi.mock(`@/lib/trpc-client`, () => ({ trpc: {} }))
vi.mock(`@/hooks/use-session`, () => ({ useSession: () => ({ data: { user: { id: `me` } } }) }))
vi.mock(`@/hooks/use-work-tabs`, () => ({
  useWorkTabs: () => ({ tabs: tabs.value }),
  updateWorkTabs,
  openNewRunPage,
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
      { kind: `issue`, issueId: `i1`, from: null },
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
      { kind: `issue`, issueId: `i1`, from: null },
    ]
    renderStrip()
    const tab = screen.getByTestId(`work-tab-issue:i1`)
    fireEvent.mouseDown(tab, { button: 0 })
    auxClick(tab, 2)
    expect(updateWorkTabs).not.toHaveBeenCalled()
  })

  it(`closes a running thread's tab like every other`, () => {
    threadGroups.value = [
      {
        key: `running`,
        title: `Running`,
        rows: [{ sessionId: `t1`, teamId: `t1`, title: `Reduce cold start`, private: false, busy: true, needsYou: false, activityAt: `` }],
      },
    ]
    tabs.value = [{ kind: `thread`, threadId: `t1`, from: null }]
    renderStrip()
    const tab = screen.getByTestId(`work-tab-thread:t1`)
    expect(screen.getAllByLabelText(`Close tab`).length).toBeGreaterThan(0)
    auxClick(tab, 1)
    expect(updateWorkTabs).toHaveBeenCalledTimes(1)
  })
})

// EXP-1255: the × shows on hover only (every tab, the active one too; a
// keyboard focus shows it), a "+" after the last tab opens the New run tab,
// and the New run chip is the Agent page's tab.
describe(`WorkTabsStrip New run (EXP-1255)`, () => {
  const issueTab = { kind: `issue`, issueId: `i1`, from: null }

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
    expect(plus.getAttribute(`aria-label`)).toBe(`New session`)
    const tab = screen.getByTestId(`work-tab-issue:i1`)
    expect(tab.compareDocumentPosition(plus) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy()
    expect(plus.nextElementSibling).toBeNull()
    fireEvent.click(plus)
    expect(openNewRunPage).toHaveBeenCalledWith(`t1`, `acme`, expect.anything())
  })

  it(`draws the New run chip, active on the Agent page`, () => {
    tabs.value = [issueTab, { kind: `new`, from: null }]
    pathname.value = `/t/acme/agent`
    renderStrip()
    const chip = screen.getByTestId(`work-tab-new`)
    expect(chip.textContent).toContain(`New session`)
    expect(chip.getAttribute(`data-active`)).toBe(`true`)
    expect(screen.getByTestId(`work-tab-issue:i1`).getAttribute(`data-active`)).toBeNull()
  })
})

// EXP-1293: thread chips read the Threads list (title + the agent mark),
// agent chips the agent's name; each is active on its own route.
describe(`WorkTabsStrip session-tree chips (EXP-1293)`, () => {
  beforeEach(() => {
    issues.value = []
    threadGroups.value = [
      {
        key: `needs_you`,
        title: `Needs you`,
        rows: [{ sessionId: `th1`, teamId: `t1`, title: `Reduce cold start`, private: false, busy: false, needsYou: true, activityAt: `` }],
      },
    ]
    agents.value = [
      { agentId: `ag1`, teamId: `t1`, name: `Triage`, icon: `bug`, color: `#a855f7`, busy: false, needsYou: false },
    ]
  })

  it(`titles a thread chip and marks it as needing you`, () => {
    tabs.value = [{ kind: `thread`, threadId: `th1`, from: null }]
    pathname.value = `/t/acme/threads/th1`
    renderStrip()
    const chip = screen.getByTestId(`work-tab-thread:th1`)
    expect(chip.textContent).toContain(`Reduce cold start`)
    expect(chip.getAttribute(`data-active`)).toBe(`true`)
    expect(chip.querySelector(`[data-slot="run-mark"]`)?.getAttribute(`data-state`)).toBe(
      `needs_input`
    )
  })

  it(`names an agent chip, active on its channel`, () => {
    tabs.value = [{ kind: `agent`, agentId: `ag1`, from: null }]
    pathname.value = `/t/acme/agents/ag1`
    renderStrip()
    const chip = screen.getByTestId(`work-tab-agent:ag1`)
    expect(chip.textContent).toContain(`Triage`)
    expect(chip.getAttribute(`data-active`)).toBe(`true`)
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
      { kind: `issue`, issueId: `i9`, from: null },
    ]
    renderStrip()
    const tab = screen.getByTestId(`work-tab-issue:i9`)
    expect(
      tab.querySelector(`[data-testid=work-tab-title-placeholder]`)
    ).not.toBeNull()
    expect(screen.queryByText(`Loading…`)).toBeNull()
  })
})
