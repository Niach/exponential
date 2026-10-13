import { describe, expect, it } from "vitest"
import {
  closeTabs,
  EMPTY_WORK_TABS,
  isNewRunPath,
  isPreviewOrigin,
  NEW_RUN_TAB,
  openNewRunTab,
  originCreatesTab,
  parseWorkTabsState,
  partitionTabs,
  previewSlotKey,
  pruneTabs,
  routePathFromLocation,
  routeTabKey,
  TABLESS_ORIGIN,
  tabHref,
  tabKey,
  upsertFromRoute,
  type WorkTab,
  type WorkTabsState,
} from "@/lib/work-tabs"

// EXP-870 / EXP-1293: the work-tab model — route-driven: an issue, a thread,
// an agent's channel or a raw session the URL opened is a tab; the New
// session tab turns into whatever its page navigates to; a `running`-origin
// navigation opens none; the Inbox shares one preview slot; and the desktop's
// `partition_tabs` vectors (apps/desktop/crates/ui/src/screens.rs).

const state = (tabs: WorkTab[]) => ({ tabs }) satisfies WorkTabsState
const keys = (s: WorkTabsState) => s.tabs.map(tabKey)
const issue = (issueId: string, from: string | null = null): WorkTab => ({
  kind: `issue`,
  issueId,
  from,
})
const thread = (threadId: string, from: string | null = null): WorkTab => ({
  kind: `thread`,
  threadId,
  from,
})

describe(`routePathFromLocation`, () => {
  it(`names the four work items`, () => {
    expect(routePathFromLocation(`/t/acme/boards/app/issues/APP-1`, `inbox`)).toEqual({
      kind: `issue`,
      boardSlug: `app`,
      identifier: `APP-1`,
      from: `inbox`,
    })
    expect(routePathFromLocation(`/t/acme/threads/t1`, null)).toEqual({
      kind: `thread`,
      threadId: `t1`,
      from: null,
    })
    expect(routePathFromLocation(`/t/acme/agents/a1/`, ``)).toEqual({
      kind: `agent`,
      agentId: `a1`,
      from: null,
    })
    expect(routePathFromLocation(`/t/acme/sessions/s1`, `running`)).toEqual({
      kind: `session`,
      sessionId: `s1`,
      from: `running`,
    })
  })

  it(`names no tab for a session's Guide redirect`, () => {
    expect(routePathFromLocation(`/t/acme/sessions/s1`, null, `guide`)).toBeNull()
    expect(routePathFromLocation(`/t/acme/threads/t1`, null, `guide`)).toEqual({
      kind: `thread`,
      threadId: `t1`,
      from: null,
    })
  })

  it(`is null on lists, full pages and broken escapes`, () => {
    for (const path of [
      `/t/acme`,
      `/t/acme/agent`,
      `/t/acme/threads`,
      `/t/acme/inbox`,
      `/t/acme/boards/app`,
      `/t/acme/settings/agents/a1`,
      `/t/acme/threads/%E0%A4%A`,
      `/about`,
    ]) {
      expect(routePathFromLocation(path, null), path).toBeNull()
    }
  })

  it(`knows the New session tab's page`, () => {
    expect(isNewRunPath(`/t/acme/agent`)).toBe(true)
    expect(isNewRunPath(`/t/acme/agent/`)).toBe(true)
    expect(isNewRunPath(`/t/acme/agents/a1`)).toBe(false)
  })
})

describe(`upsertFromRoute`, () => {
  it(`appends a work item once and focuses it after`, () => {
    let s = upsertFromRoute(EMPTY_WORK_TABS, { kind: `issue`, issueId: `i1`, from: null })
    s = upsertFromRoute(s, { kind: `thread`, threadId: `t1`, from: null })
    s = upsertFromRoute(s, { kind: `agent`, agentId: `a1`, from: null })
    s = upsertFromRoute(s, { kind: `session`, sessionId: `s1`, from: null })
    expect(keys(s)).toEqual([`issue:i1`, `thread:t1`, `agent:a1`, `session:s1`])
    expect(upsertFromRoute(s, { kind: `thread`, threadId: `t1`, from: null })).toBe(s)
  })

  it(`restamps the origin only on an explicit one`, () => {
    const s = state([issue(`i1`, `board:app`)])
    expect(upsertFromRoute(s, { kind: `issue`, issueId: `i1`, from: null })).toBe(s)
    expect(
      upsertFromRoute(s, { kind: `issue`, issueId: `i1`, from: `drafts` }).tabs
    ).toEqual([issue(`i1`, `drafts`)])
  })

  it(`opens no tab from the Running section, but focuses an open one`, () => {
    expect(originCreatesTab(TABLESS_ORIGIN)).toBe(false)
    expect(
      upsertFromRoute(EMPTY_WORK_TABS, { kind: `thread`, threadId: `t1`, from: TABLESS_ORIGIN })
    ).toBe(EMPTY_WORK_TABS)
    const open = state([thread(`t1`)])
    expect(
      upsertFromRoute(open, { kind: `thread`, threadId: `t1`, from: TABLESS_ORIGIN })
    ).toBe(open)
  })

  it(`steps the Inbox through ONE preview slot`, () => {
    expect(isPreviewOrigin(`inbox`)).toBe(true)
    expect(previewSlotKey(`inbox:my-issues`)).toBe(`inbox`)
    expect(isPreviewOrigin(`board:app`)).toBe(false)
    let s = state([issue(`i0`, `board:app`)])
    s = upsertFromRoute(s, { kind: `issue`, issueId: `i1`, from: `inbox` })
    s = upsertFromRoute(s, { kind: `issue`, issueId: `i2`, from: `inbox:my-issues` })
    expect(s.tabs).toEqual([issue(`i0`, `board:app`), issue(`i2`, `inbox:my-issues`)])
    // An item that already has a tab only focuses it, keeping its origin.
    expect(upsertFromRoute(s, { kind: `issue`, issueId: `i0`, from: `inbox` })).toBe(s)
  })

  it(`lands a navigation from the New session page IN its tab`, () => {
    const s = state([issue(`i1`), NEW_RUN_TAB, issue(`i2`)])
    const landed = upsertFromRoute(
      s,
      { kind: `thread`, threadId: `t1`, from: null },
      { landing: true }
    )
    expect(keys(landed)).toEqual([`issue:i1`, `thread:t1`, `issue:i2`])
    // An item that already has a tab takes the focus; the New tab stays.
    expect(
      upsertFromRoute(s, { kind: `issue`, issueId: `i2`, from: null }, { landing: true })
    ).toBe(s)
    // A Running row (the tabless origin) lands nothing either.
    expect(
      upsertFromRoute(s, { kind: `thread`, threadId: `t9`, from: `running` }, { landing: true })
    ).toBe(s)
    // Without a New session tab a landing is an ordinary open.
    expect(
      keys(
        upsertFromRoute(
          state([issue(`i1`)]),
          { kind: `thread`, threadId: `t1`, from: null },
          { landing: true }
        )
      )
    ).toEqual([`issue:i1`, `thread:t1`])
  })

  it(`keys a route like its tab`, () => {
    expect(routeTabKey({ kind: `agent`, agentId: `a1`, from: `x` })).toBe(`agent:a1`)
  })
})

describe(`the New session tab, closing and pruning`, () => {
  it(`opens ONE New session tab`, () => {
    const s = openNewRunTab(state([issue(`i1`)]))
    expect(keys(s)).toEqual([`issue:i1`, `new`])
    expect(openNewRunTab(s)).toBe(s)
  })

  it(`closes the named tabs only`, () => {
    const s = state([issue(`i1`), thread(`t1`), NEW_RUN_TAB])
    expect(keys(closeTabs(s, [`thread:t1`, `new`]))).toEqual([`issue:i1`])
    expect(closeTabs(s, [`thread:nope`])).toBe(s)
  })

  it(`prunes what no longer resolves`, () => {
    const s = state([issue(`i1`), thread(`t1`)])
    expect(keys(pruneTabs(s, (tab) => tab.kind !== `thread`))).toEqual([`issue:i1`])
    expect(pruneTabs(s, () => true)).toBe(s)
  })
})

describe(`tabHref`, () => {
  const resolve = (id: string) =>
    id === `i1` ? { boardSlug: `app`, identifier: `APP-1` } : null

  it(`routes every kind`, () => {
    expect(tabHref(`acme`, NEW_RUN_TAB, resolve)).toEqual({
      to: `/t/$teamSlug/agent`,
      params: { teamSlug: `acme` },
      search: {},
    })
    expect(tabHref(`acme`, thread(`t1`, `threads`), resolve)).toEqual({
      to: `/t/$teamSlug/threads/$threadId`,
      params: { teamSlug: `acme`, threadId: `t1` },
      search: { from: `threads` },
    })
    expect(tabHref(`acme`, { kind: `agent`, agentId: `a1`, from: null }, resolve)).toEqual({
      to: `/t/$teamSlug/agents/$agentId`,
      params: { teamSlug: `acme`, agentId: `a1` },
      search: {},
    })
    expect(
      tabHref(`acme`, { kind: `session`, sessionId: `s1`, from: null }, resolve)
    ).toEqual({
      to: `/t/$teamSlug/sessions/$sessionId`,
      params: { teamSlug: `acme`, sessionId: `s1` },
      search: {},
    })
    expect(tabHref(`acme`, issue(`i1`), resolve)).toEqual({
      to: `/t/$teamSlug/boards/$boardSlug/issues/$issueIdentifier`,
      params: { teamSlug: `acme`, boardSlug: `app`, issueIdentifier: `APP-1` },
      search: {},
    })
  })

  it(`waits on an issue that has not synced`, () => {
    expect(tabHref(`acme`, issue(`i2`), resolve)).toBeNull()
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
      thread(`t1`),
      { kind: `agent`, agentId: `a1`, from: null },
      { kind: `session`, sessionId: `s1`, from: `threads` },
      NEW_RUN_TAB,
    ])
    expect(parseWorkTabsState(JSON.stringify(s))).toEqual(s)
  })

  it(`drops retired kinds and malformed rows`, () => {
    expect(
      parseWorkTabsState(
        JSON.stringify({
          tabs: [
            { kind: `run`, runId: `s1`, from: null, live: true },
            { kind: `support`, threadId: `t1` },
            { kind: `issue` },
            { kind: `thread`, threadId: `t1`, from: 4 },
            { kind: `thread`, threadId: `t1`, from: `x` },
          ],
        })
      )
    ).toEqual(state([thread(`t1`)]))
  })

  it(`reads corrupt or foreign data as an empty strip`, () => {
    expect(parseWorkTabsState(null)).toEqual(EMPTY_WORK_TABS)
    expect(parseWorkTabsState(`{nope`)).toEqual(EMPTY_WORK_TABS)
    expect(parseWorkTabsState(`42`)).toEqual(EMPTY_WORK_TABS)
  })
})
