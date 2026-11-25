import { createElement } from "react"
import { render } from "@testing-library/react"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"
import { EMPTY_WORK_TABS, NEW_RUN_TAB, tabKey, type WorkTabsState } from "@/lib/work-tabs"
import { isNewTabChord, WorkTabsSync } from "@/components/team/work-tabs-sync"

const location = vi.hoisted(() => ({ pathname: `/inbox`, search: {} as Record<string, unknown> }))
const store = vi.hoisted(() => ({ state: { tabs: [] } as { tabs: unknown[] } }))
const liveRows = vi.hoisted(() => ({ apps: [] as unknown[], devices: [] as unknown[] }))

vi.mock(`@tanstack/react-router`, () => ({
  useNavigate: () => vi.fn(),
  useRouterState: ({ select }: { select: (state: unknown) => unknown }) => select({ location }),
}))
vi.mock(`@tanstack/react-db`, async (importOriginal) => {
  const actual = (await importOriginal()) as Record<string, unknown>
  return {
    ...actual,
    useLiveQuery: (build: (query: unknown) => unknown) => {
      let source: { rows?: () => unknown[] } | undefined
      const chain = { where: () => chain }
      const built = build({
        from: (sources: Record<string, { rows?: () => unknown[] }>) => {
          source = Object.values(sources)[0]
          return chain
        },
      })
      return { data: built === undefined ? undefined : (source?.rows?.() ?? []) }
    },
  }
})
vi.mock(`@/lib/collections`, () => ({
  agentCollection: { isReady: () => false, get: () => undefined },
  appCollection: { isReady: () => false, get: () => undefined, rows: () => liveRows.apps },
  deviceCollection: { isReady: () => false, toArray: [], rows: () => liveRows.devices },
  issueCollection: { isReady: () => false, get: () => undefined, rows: () => [] },
  sessionCollection: { isReady: () => false, get: () => undefined },
  teamCollection: { get: () => undefined },
}))
vi.mock(`@/hooks/use-session-tree`, () => ({ useSessionRow: () => ({ row: null }) }))
vi.mock(`@/hooks/use-agent-row`, () => ({ useAgentRow: () => ({ agent: null }) }))
vi.mock(`@/hooks/use-work-tabs`, () => ({
  useWorkTabs: () => store.state,
  updateWorkTabs: (transition: (state: WorkTabsState) => WorkTabsState) => {
    store.state = transition(store.state as WorkTabsState)
  },
  openNewRunPage: vi.fn(),
}))

// Review #22: Alt/Option+T opens the New session tab, but never eats a
// keystroke meant for a field, an IME composition or an open dialog.

function press(target: EventTarget, init: KeyboardEventInit = {}): KeyboardEvent {
  let seen: KeyboardEvent | null = null
  const listener = (event: Event) => {
    seen = event as KeyboardEvent
  }
  window.addEventListener(`keydown`, listener)
  target.dispatchEvent(
    new KeyboardEvent(`keydown`, { code: `KeyT`, altKey: true, bubbles: true, ...init })
  )
  window.removeEventListener(`keydown`, listener)
  return seen!
}

afterEach(() => {
  document.body.innerHTML = ``
})

describe(`isNewTabChord`, () => {
  it(`fires for Alt+T on the page`, () => {
    expect(isNewTabChord(press(document.body))).toBe(true)
  })

  it(`ignores other chords`, () => {
    expect(isNewTabChord(press(document.body, { altKey: false }))).toBe(false)
    expect(isNewTabChord(press(document.body, { metaKey: true }))).toBe(false)
    expect(isNewTabChord(press(document.body, { shiftKey: true }))).toBe(false)
    expect(isNewTabChord(press(document.body, { code: `KeyR` }))).toBe(false)
  })

  it(`skips an editable target`, () => {
    const input = document.createElement(`input`)
    const textarea = document.createElement(`textarea`)
    const editor = document.createElement(`div`)
    editor.setAttribute(`contenteditable`, `true`)
    const inner = document.createElement(`p`)
    editor.append(inner)
    document.body.append(input, textarea, editor)
    expect(isNewTabChord(press(input))).toBe(false)
    expect(isNewTabChord(press(textarea))).toBe(false)
    expect(isNewTabChord(press(inner))).toBe(false)
  })

  it(`skips an IME composition`, () => {
    expect(isNewTabChord(press(document.body, { isComposing: true }))).toBe(false)
  })

  it(`skips while a dialog is open`, () => {
    const dialog = document.createElement(`div`)
    dialog.setAttribute(`role`, `dialog`)
    document.body.append(dialog)
    expect(isNewTabChord(press(document.body))).toBe(false)
  })
})

// #EXP-2124 (sidebar-menus.json `tabs`): every page that has Open is a tab,
// route-driven like the work items; Home IS the New session tab.
describe(`WorkTabsSync pages (#EXP-2124)`, () => {
  const teams = [{ id: `t1`, slug: `acme` }]
  const boards = [{ id: `b1`, slug: `web`, teamId: `t1` }] as never
  const visit = (pathname: string) => {
    location.pathname = pathname
    return render(createElement(WorkTabsSync, { teams, boards }))
  }
  const keys = () => (store.state as WorkTabsState).tabs.map(tabKey)

  beforeEach(() => {
    store.state = EMPTY_WORK_TABS
    liveRows.apps = []
    liveRows.devices = []
  })

  it(`opens Home's tab on Home`, () => {
    visit(`/home`)
    expect(store.state.tabs).toEqual([NEW_RUN_TAB])
  })

  it(`opens the list pages, a board, an app and a device`, () => {
    liveRows.apps = [{ id: `a1`, slug: `notes`, teamId: `t1`, deletedAt: null }]
    liveRows.devices = [{ id: `row-1`, deviceId: `d1` }]
    visit(`/needs-you`).unmount()
    visit(`/t/acme/boards/web`).unmount()
    visit(`/t/acme/apps/notes/list`).unmount()
    visit(`/devices/row-1`).unmount()
    visit(`/needs-you`).unmount()
    expect(keys()).toEqual([`page:needs-you`, `board:b1`, `app:a1`, `device:d1`])
  })

  it(`names no tab for a board of a team the person is not in, or an unknown app`, () => {
    visit(`/t/other/boards/web`).unmount()
    visit(`/t/acme/apps/gone`).unmount()
    visit(`/devices/nope`).unmount()
    expect(keys()).toEqual([])
  })

  it(`lands a page opened from Home in Home's tab`, () => {
    const view = visit(`/home`)
    location.pathname = `/chats`
    view.rerender(createElement(WorkTabsSync, { teams, boards }))
    expect(keys()).toEqual([`page:chats`])
  })
})
