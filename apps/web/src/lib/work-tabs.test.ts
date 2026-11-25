import { describe, expect, it } from "vitest"
import {
  closeTabs,
  EMPTY_WORK_TABS,
  isNewRunPath,
  migrateLegacyWorkTabs,
  NEW_RUN_TAB,
  openNewRunTab,
  openTabInBackground,
  originCreatesTab,
  parseWorkTabsState,
  partitionTabs,
  PREVIOUS_WORK_TABS_STORAGE_KEY,
  pruneTabs,
  routePathFromLocation,
  routeTabKey,
  TABLESS_ORIGIN,
  tabHref,
  tabKey,
  upsertFromRoute,
  WORK_TABS_STORAGE_KEY,
  type RouteTab,
  type WorkTab,
  type WorkTabsState,
} from "@/lib/work-tabs"

// EXP-870 / EXP-1293: the work-tab model — route-driven: an issue, a chat,
// an agent's channel or a raw session the URL opened is a tab; the New
// session tab turns into whatever its page navigates to; a `running`-origin
// navigation opens none; and the desktop's
// `partition_tabs` vectors (apps/desktop/crates/ui/src/screens.rs).

const state = (tabs: WorkTab[]) => ({ tabs }) satisfies WorkTabsState
const keys = (s: WorkTabsState) => s.tabs.map(tabKey)
const T = { teamId: `t1`, teamSlug: `acme` }
const OTHER = { teamId: `t2`, teamSlug: `beta` }
const issue = (issueId: string, from: string | null = null, team = T): WorkTab => ({
  kind: `issue`,
  issueId,
  from,
  ...team,
})
const chat = (chatId: string, from: string | null = null, team = T): WorkTab => ({
  kind: `chat`,
  chatId,
  from,
  ...team,
})

describe(`routePathFromLocation`, () => {
  it(`names the three work items (a session path resolves to its chat in WorkTabsSync)`, () => {
    expect(routePathFromLocation(`/t/acme/boards/app/issues/APP-1`, `inbox`)).toEqual({
      kind: `issue`,
      teamSlug: `acme`,
      boardSlug: `app`,
      identifier: `APP-1`,
      from: `inbox`,
    })
    expect(routePathFromLocation(`/t/acme/agents/a1/`, ``)).toEqual({
      kind: `agent`,
      teamSlug: `acme`,
      agentId: `a1`,
      from: null,
    })
    expect(routePathFromLocation(`/t/acme/sessions/s1`, `running`)).toEqual({
      kind: `session`,
      teamSlug: `acme`,
      sessionId: `s1`,
      from: `running`,
    })
  })

  it(`names no tab for a session's Guide redirect`, () => {
    expect(routePathFromLocation(`/t/acme/sessions/s1`, null, `guide`)).toBeNull()
  })

  it(`is null on lists, full pages and broken escapes`, () => {
    for (const path of [
      `/t/acme`,
      `/t/acme/agent`,
      `/t/acme/chats`,
      `/t/acme/inbox`,
      `/t/acme/apps`,
      `/t/acme/app-editor/notes`,
      `/t/acme/new/board`,
      `/t/acme/boards/app/settings/x`,
      `/t/acme/settings/agents/a1`,
      `/t/acme/chats/t1`,
      `/t/acme/sessions/%E0%A4%A`,
      `/about`,
      `/home`,
      `/inbox`,
      `/settings/account`,
      `/devices`,
      `/devices/d1/settings`,
      `/devices/%E0%A4%A`,
    ]) {
      expect(routePathFromLocation(path, null), path).toBeNull()
    }
  })

  it(`#EXP-2124: names the pages a tab holds`, () => {
    expect(routePathFromLocation(`/needs-you`, `x`)).toEqual({ kind: `page`, page: `needs-you`, from: null })
    expect(routePathFromLocation(`/chats/`, null)).toEqual({ kind: `page`, page: `chats`, from: null })
    expect(routePathFromLocation(`/drafts`, null)).toEqual({ kind: `page`, page: `drafts`, from: null })
    expect(routePathFromLocation(`/devices/d%201`, null)).toEqual({
      kind: `device`,
      deviceId: `d 1`,
      from: null,
    })
    expect(routePathFromLocation(`/t/acme/boards/web/`, null)).toEqual({
      kind: `board`,
      teamSlug: `acme`,
      boardSlug: `web`,
      from: null,
    })
    for (const path of [`/t/acme/apps/notes`, `/t/acme/apps/notes/list`]) {
      expect(routePathFromLocation(path, null), path).toEqual({
        kind: `app`,
        teamSlug: `acme`,
        appSlug: `notes`,
        from: null,
      })
    }
  })

  it(`names the entity's team off the route (#EXP-2027)`, () => {
    expect(routePathFromLocation(`/t/beta/sessions/s1`, null)).toMatchObject({ teamSlug: `beta` })
    expect(routePathFromLocation(`/t/%E0%A4%A/agents/a1`, null)).toBeNull()
  })

  it(`knows the New session tab's page`, () => {
    expect(isNewRunPath(`/home`)).toBe(true)
    expect(isNewRunPath(`/home/`)).toBe(true)
    expect(isNewRunPath(`/t/acme/agent`)).toBe(true)
    expect(isNewRunPath(`/t/acme/agent/`)).toBe(true)
    expect(isNewRunPath(`/t/acme/agents/a1`)).toBe(false)
  })
})

describe(`upsertFromRoute`, () => {
  it(`appends a work item once and focuses it after`, () => {
    let s = upsertFromRoute(EMPTY_WORK_TABS, { kind: `issue`, issueId: `i1`, from: null, ...T })
    s = upsertFromRoute(s, { kind: `chat`, chatId: `t1`, from: null, ...T })
    s = upsertFromRoute(s, { kind: `agent`, agentId: `a1`, from: null, ...T })
    s = upsertFromRoute(s, { kind: `session`, sessionId: `s1`, from: null, ...T })
    expect(keys(s)).toEqual([`issue:i1`, `chat:t1`, `agent:a1`, `session:s1`])
    expect(upsertFromRoute(s, { kind: `chat`, chatId: `t1`, from: null, ...T })).toBe(s)
  })

  it(`restamps the origin only on an explicit one`, () => {
    const s = state([issue(`i1`, `board:app`)])
    expect(upsertFromRoute(s, { kind: `issue`, issueId: `i1`, from: null, ...T })).toBe(s)
    expect(
      upsertFromRoute(s, { kind: `issue`, issueId: `i1`, from: `drafts`, ...T }).tabs
    ).toEqual([issue(`i1`, `drafts`)])
  })

  it(`opens no tab from the Running section, but focuses an open one`, () => {
    expect(originCreatesTab(TABLESS_ORIGIN)).toBe(false)
    expect(
      upsertFromRoute(EMPTY_WORK_TABS, { kind: `chat`, chatId: `t1`, from: TABLESS_ORIGIN, ...T })
    ).toBe(EMPTY_WORK_TABS)
    const open = state([chat(`t1`)])
    expect(
      upsertFromRoute(open, { kind: `chat`, chatId: `t1`, from: TABLESS_ORIGIN, ...T })
    ).toBe(open)
  })

  it(`#FEED-191: an old Inbox origin opens an ordinary tab`, () => {
    let s = state([issue(`i0`, `board:app`)])
    s = upsertFromRoute(s, { kind: `issue`, issueId: `i1`, from: `inbox`, ...T })
    s = upsertFromRoute(s, { kind: `issue`, issueId: `i2`, from: `inbox:my-issues`, ...T })
    expect(keys(s)).toEqual([`issue:i0`, `issue:i1`, `issue:i2`])
  })

  it(`lands a navigation from the New session page IN its tab`, () => {
    const s = state([issue(`i1`), NEW_RUN_TAB, issue(`i2`)])
    const landed = upsertFromRoute(
      s,
      { kind: `chat`, chatId: `t1`, from: null, ...T },
      { landing: true }
    )
    expect(keys(landed)).toEqual([`issue:i1`, `chat:t1`, `issue:i2`])
    // An item that already has a tab takes the focus; the New tab stays.
    expect(
      upsertFromRoute(s, { kind: `issue`, issueId: `i2`, from: null, ...T }, { landing: true })
    ).toBe(s)
    // A Running row (the tabless origin) lands nothing either.
    expect(
      upsertFromRoute(s, { kind: `chat`, chatId: `t9`, from: `running`, ...T }, { landing: true })
    ).toBe(s)
    // Without a New session tab a landing is an ordinary open.
    expect(
      keys(
        upsertFromRoute(
          state([issue(`i1`)]),
          { kind: `chat`, chatId: `t1`, from: null, ...T },
          { landing: true }
        )
      )
    ).toEqual([`issue:i1`, `chat:t1`])
  })

  it(`#EXP-2027: one list across teams, each tab keeping its team`, () => {
    let s = upsertFromRoute(EMPTY_WORK_TABS, { kind: `issue`, issueId: `i1`, from: null, ...T })
    s = upsertFromRoute(s, { kind: `chat`, chatId: `c2`, from: null, ...OTHER })
    expect(s.tabs).toEqual([issue(`i1`), chat(`c2`, null, OTHER)])
  })

  it(`#EXP-2027: a tab migrated without its slug takes the route's`, () => {
    const s = state([issue(`i1`, null, { teamId: `t1`, teamSlug: `` })])
    expect(upsertFromRoute(s, { kind: `issue`, issueId: `i1`, from: null, ...T }).tabs).toEqual([
      issue(`i1`),
    ])
  })

  it(`keys a route like its tab`, () => {
    expect(routeTabKey({ kind: `agent`, agentId: `a1`, from: `x`, ...T })).toBe(`agent:a1`)
  })
})

describe(`#EXP-2124: page tabs`, () => {
  const board: WorkTab = { kind: `board`, boardId: `b1`, from: null, ...T }
  const app: WorkTab = { kind: `app`, appId: `a1`, from: null, ...T }
  const device: WorkTab = { kind: `device`, deviceId: `d1`, from: null }
  const needsYou: WorkTab = { kind: `page`, page: `needs-you`, from: null }

  it(`keys every page kind`, () => {
    expect([board, app, device, needsYou].map(tabKey)).toEqual([
      `board:b1`,
      `app:a1`,
      `device:d1`,
      `page:needs-you`,
    ])
  })

  it(`opens a page once and focuses it after`, () => {
    let s = upsertFromRoute(EMPTY_WORK_TABS, needsYou)
    s = upsertFromRoute(s, board)
    s = upsertFromRoute(s, device)
    s = upsertFromRoute(s, app)
    expect(keys(s)).toEqual([`page:needs-you`, `board:b1`, `device:d1`, `app:a1`])
    expect(upsertFromRoute(s, needsYou)).toBe(s)
    expect(upsertFromRoute(s, device)).toBe(s)
    expect(upsertFromRoute(s, board)).toBe(s)
    // A renamed team re-teams a board tab like any team tab.
    expect(upsertFromRoute(s, { ...board, teamSlug: `acme2` } as RouteTab).tabs[1]).toEqual({
      ...board,
      teamSlug: `acme2`,
    })
  })

  it(`lands a page opened from Home in Home's tab`, () => {
    const s = state([issue(`i1`), NEW_RUN_TAB])
    expect(keys(upsertFromRoute(s, needsYou, { landing: true }))).toEqual([`issue:i1`, `page:needs-you`])
    expect(keys(upsertFromRoute(s, board, { landing: true }))).toEqual([`issue:i1`, `board:b1`])
  })

  it(`opens a tab in the background, never twice`, () => {
    const s = state([issue(`i1`)])
    const opened = openTabInBackground(s, board)
    expect(keys(opened)).toEqual([`issue:i1`, `board:b1`])
    expect(openTabInBackground(opened, { ...board })).toBe(opened)
    expect(openTabInBackground(s, issue(`i1`))).toBe(s)
    // Home's tab is the one New session tab.
    const home = openTabInBackground(s, NEW_RUN_TAB)
    expect(keys(home)).toEqual([`issue:i1`, `new`])
    expect(openTabInBackground(home, NEW_RUN_TAB)).toBe(home)
  })

  it(`routes every page kind`, () => {
    const boards = (id: string) => (id === `b1` ? `web` : null)
    const apps = (id: string) => (id === `a1` ? `notes` : null)
    expect(tabHref(needsYou, () => null)).toEqual({ to: `/needs-you`, params: {}, search: {} })
    expect(tabHref({ kind: `page`, page: `drafts`, from: null }, () => null)?.to).toBe(`/drafts`)
    expect(tabHref(device, () => null)).toEqual({
      to: `/devices/$deviceId`,
      params: { deviceId: `d1` },
      search: {},
    })
    expect(tabHref(board, () => null, () => null, boards, apps)).toEqual({
      to: `/t/$teamSlug/boards/$boardSlug`,
      params: { teamSlug: `acme`, boardSlug: `web` },
      search: {},
    })
    expect(tabHref(app, () => null, () => `renamed`, boards, apps)).toEqual({
      to: `/t/$teamSlug/apps/$appSlug`,
      params: { teamSlug: `renamed`, appSlug: `notes` },
      search: {},
    })
    // An unknown board or app waits.
    expect(tabHref(board, () => null)).toBeNull()
    expect(tabHref({ ...app, appId: `a9` } as WorkTab, () => null, () => null, boards, apps)).toBeNull()
  })

  it(`prunes pages by the resolver, Home and the list pages never`, () => {
    const s = state([NEW_RUN_TAB, needsYou, board, app, device])
    const gone = new Set([`board:b1`, `app:a1`, `device:d1`])
    const resolves = (tab: WorkTab) =>
      tab.kind === `new` || tab.kind === `page` ? true : !gone.has(tabKey(tab))
    expect(keys(pruneTabs(s, resolves))).toEqual([`new`, `page:needs-you`])
  })
})

describe(`the New session tab, closing and pruning`, () => {
  it(`opens ONE New session tab`, () => {
    const s = openNewRunTab(state([issue(`i1`)]))
    expect(keys(s)).toEqual([`issue:i1`, `new`])
    expect(openNewRunTab(s)).toBe(s)
  })

  it(`closes the named tabs only`, () => {
    const s = state([issue(`i1`), chat(`t1`), NEW_RUN_TAB])
    expect(keys(closeTabs(s, [`chat:t1`, `new`]))).toEqual([`issue:i1`])
    expect(closeTabs(s, [`chat:nope`])).toBe(s)
  })

  it(`prunes what no longer resolves`, () => {
    const s = state([issue(`i1`), chat(`t1`)])
    expect(keys(pruneTabs(s, (tab) => tab.kind !== `chat`))).toEqual([`issue:i1`])
    expect(pruneTabs(s, () => true)).toBe(s)
  })
})

describe(`tabHref`, () => {
  const resolve = (id: string) =>
    id === `i1` ? { boardSlug: `app`, identifier: `APP-1` } : null

  it(`routes every kind, each with its own team`, () => {
    expect(tabHref(NEW_RUN_TAB, resolve)).toEqual({
      to: `/home`,
      params: {},
      search: {},
    })
    expect(tabHref(chat(`c2`, null, OTHER), resolve)).toEqual({
      to: `/t/$teamSlug/sessions/$sessionId`,
      params: { teamSlug: `beta`, sessionId: `c2` },
      search: {},
    })
    expect(tabHref(chat(`t1`, `chats`), resolve)).toEqual({
      to: `/t/$teamSlug/sessions/$sessionId`,
      params: { teamSlug: `acme`, sessionId: `t1` },
      search: { from: `chats` },
    })
    expect(tabHref({ kind: `agent`, agentId: `a1`, from: null, ...T }, resolve)).toEqual({
      to: `/t/$teamSlug/agents/$agentId`,
      params: { teamSlug: `acme`, agentId: `a1` },
      search: {},
    })
    expect(
      tabHref({ kind: `session`, sessionId: `s1`, from: null, ...T }, resolve)
    ).toEqual({
      to: `/t/$teamSlug/sessions/$sessionId`,
      params: { teamSlug: `acme`, sessionId: `s1` },
      search: {},
    })
    expect(tabHref(issue(`i1`), resolve)).toEqual({
      to: `/t/$teamSlug/boards/$boardSlug/issues/$issueIdentifier`,
      params: { teamSlug: `acme`, boardSlug: `app`, issueIdentifier: `APP-1` },
      search: {},
    })
  })

  it(`prefers the team's current slug, and waits without any`, () => {
    const slugs = (teamId: string) => (teamId === `t1` ? `renamed` : null)
    expect(tabHref(chat(`c1`), resolve, slugs)?.params.teamSlug).toBe(`renamed`)
    const migrated = chat(`c2`, null, { teamId: `t2`, teamSlug: `` })
    expect(tabHref(migrated, resolve, slugs)).toBeNull()
  })

  it(`waits on an issue that has not synced`, () => {
    expect(tabHref(issue(`i2`), resolve)).toBeNull()
  })
})

// The desktop's `partition_tabs` vectors (screens.rs), verbatim.
describe(`partitionTabs`, () => {
  const GAP = 3.5
  const OVERFLOW = 22
  const partition = (widths: number[], available: number, active: number | null) =>
    partitionTabs(widths, available, GAP, OVERFLOW, active)

  it(`keeps every tab on an exact fit`, () => {
    expect(partition([100, 100, 100], 300 + GAP * 2, 0)).toEqual([0, 1, 2])
  })

  it(`keeps the last fitting tab`, () => {
    const available = 200 + GAP + OVERFLOW + GAP
    expect(partition([100, 100, 100], available, 0)).toEqual([0, 1])
    expect(partition([100, 100, 100], available - 1, 0)).toEqual([0])
  })

  it(`lets the active tab displace the last visible chip`, () => {
    expect(partition([100, 100, 100], 200 + GAP + OVERFLOW + GAP, 2)).toEqual([0, 2])
  })

  it(`shrinks the prefix for a wide active tab instead of overflowing`, () => {
    const available = 200 + GAP + OVERFLOW + GAP
    expect(partition([100, 100, 250], available, 2)).toEqual([2])
    expect(partition([100, 100, 250], available + 150, 2)).toEqual([0, 2])
  })

  it(`always keeps one tab`, () => {
    expect(partition([500], 40, 0)).toEqual([0])
    expect(partition([500, 500], 0, null)).toEqual([0])
  })

  it(`partitions no tabs to nothing`, () => {
    expect(partition([], 400, null)).toEqual([])
  })
})

describe(`parseWorkTabsState`, () => {
  it(`round-trips a stored state`, () => {
    const s = state([
      issue(`i1`, `inbox`),
      chat(`t1`),
      { kind: `agent`, agentId: `a1`, from: null, ...T },
      { kind: `session`, sessionId: `s1`, from: `chats`, ...T },
      NEW_RUN_TAB,
      { kind: `page`, page: `chats`, from: null },
      { kind: `device`, deviceId: `d1`, from: null },
      { kind: `board`, boardId: `b1`, from: null, ...T },
      { kind: `app`, appId: `a1`, from: null, ...T },
    ])
    expect(parseWorkTabsState(JSON.stringify(s))).toEqual(s)
  })

  it(`#EXP-2124: drops a board or app tab without its team and an unknown page`, () => {
    expect(
      parseWorkTabsState(
        JSON.stringify({
          tabs: [
            { kind: `board`, boardId: `b1`, from: null },
            { kind: `app`, appId: `a1`, from: null },
            { kind: `page`, page: `inbox`, from: null },
            { kind: `device`, from: null },
            { kind: `page`, page: `drafts`, from: `x` },
          ],
        })
      )
    ).toEqual(state([{ kind: `page`, page: `drafts`, from: null }]))
  })

  it(`drops a v4 tab without its team`, () => {
    expect(
      parseWorkTabsState(JSON.stringify({ tabs: [{ kind: `chat`, chatId: `c1`, from: null }] }))
    ).toEqual(EMPTY_WORK_TABS)
  })

  it(`drops retired kinds and malformed rows`, () => {
    expect(
      parseWorkTabsState(
        JSON.stringify({
          tabs: [
            { kind: `run`, runId: `s1`, from: null, live: true },
            { kind: `support`, chatId: `t1` },
            { kind: `issue` },
            { kind: `chat`, chatId: `t1`, from: 4, ...T },
            { kind: `chat`, chatId: `t1`, from: `x`, ...T },
          ],
        })
      )
    ).toEqual(state([chat(`t1`)]))
  })

  it(`reads corrupt or foreign data as an empty strip`, () => {
    expect(parseWorkTabsState(null)).toEqual(EMPTY_WORK_TABS)
    expect(parseWorkTabsState(`{nope`)).toEqual(EMPTY_WORK_TABS)
    expect(parseWorkTabsState(`42`)).toEqual(EMPTY_WORK_TABS)
  })
})

describe(`migrateLegacyWorkTabs (#EXP-2027)`, () => {
  const v3 = (tabs: unknown[]) => JSON.stringify({ tabs })

  it(`merges the per-team v3 blobs into one list, each tab tagged with its team`, () => {
    const merged = migrateLegacyWorkTabs(null, [
      { teamId: `t1`, raw: v3([{ kind: `issue`, issueId: `i1`, from: null }, { kind: `new`, from: null }]) },
      { teamId: `t2`, raw: v3([{ kind: `chat`, chatId: `c2`, from: `chats` }, { kind: `new`, from: null }]) },
    ])
    expect(merged.tabs).toEqual([
      issue(`i1`, null, { teamId: `t1`, teamSlug: `` }),
      NEW_RUN_TAB,
      chat(`c2`, `chats`, { teamId: `t2`, teamSlug: `` }),
    ])
  })

  it(`keeps the v4 tabs first and drops duplicates and junk`, () => {
    const current = JSON.stringify(state([chat(`c2`, null, OTHER)]))
    const merged = migrateLegacyWorkTabs(current, [
      { teamId: `t2`, raw: v3([{ kind: `chat`, chatId: `c2`, from: null }]) },
      { teamId: `t3`, raw: `{nope` },
      { teamId: ``, raw: v3([{ kind: `chat`, chatId: `c9`, from: null }]) },
    ])
    expect(merged.tabs).toEqual([chat(`c2`, null, OTHER)])
    expect(migrateLegacyWorkTabs(null, [])).toBe(EMPTY_WORK_TABS)
  })

  it(`#EXP-2124: v5 first, then the v4 blob unchanged, then the v3 blobs`, () => {
    expect(WORK_TABS_STORAGE_KEY).toBe(`exp:work-tabs:v5`)
    expect(PREVIOUS_WORK_TABS_STORAGE_KEY).toBe(`exp:work-tabs:v4`)
    const v5 = JSON.stringify(state([{ kind: `page`, page: `chats`, from: null }]))
    const v4 = JSON.stringify(state([chat(`c2`, null, OTHER), NEW_RUN_TAB, issue(`i1`)]))
    const merged = migrateLegacyWorkTabs(
      v5,
      [{ teamId: `t1`, raw: v3([{ kind: `issue`, issueId: `i1`, from: null }, { kind: `issue`, issueId: `i3`, from: null }]) }],
      v4
    )
    expect(merged.tabs).toEqual([
      { kind: `page`, page: `chats`, from: null },
      chat(`c2`, null, OTHER),
      NEW_RUN_TAB,
      issue(`i1`),
      issue(`i3`, null, { teamId: `t1`, teamSlug: `` }),
    ])
  })
})
