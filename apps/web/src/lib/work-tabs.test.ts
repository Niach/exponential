import { describe, expect, it } from "vitest"
import {
  closeMergedRunTabs,
  closeTabs,
  EMPTY_WORK_TABS,
  originCreatesTab,
  parseWorkTabsState,
  partitionTabs,
  pruneTabs,
  reconcileLive,
  routePathFromLocation,
  routeTabFace,
  routeTabKey,
  TABLESS_ORIGIN,
  tabHref,
  tabKey,
  upsertFromRoute,
  type LiveRun,
  type WorkTab,
  type WorkTabsState,
} from "@/lib/work-tabs"

// EXP-870 / EXP-923: the work-tab model — the same `partition_tabs` vectors
// the desktop walks (apps/desktop/crates/ui/src/screens.rs), and the four
// rules EXP-923 left the strip with: only work you OPENED gets a tab, a
// `running`-origin navigation opens none (the sidebar row is the affordance),
// every tab closes again, and a run whose PR merged takes its tab with it.

const state = (tabs: WorkTab[]) => ({ tabs }) satisfies WorkTabsState

const keys = (s: WorkTabsState) => s.tabs.map(tabKey)

describe(`routePathFromLocation`, () => {
  it(`names the three work-item routes`, () => {
    expect(routePathFromLocation(`/t/acme/boards/web/issues/MET-1`, `inbox`)).toEqual({
      kind: `issue`,
      boardSlug: `web`,
      identifier: `MET-1`,
      from: `inbox`,
    })
    expect(routePathFromLocation(`/t/acme/sessions/s1`, undefined)).toEqual({
      kind: `run`,
      runId: `s1`,
      from: null,
    })
    expect(routePathFromLocation(`/t/acme/support/t1/`, ``)).toEqual({
      kind: `support`,
      threadId: `t1`,
      from: null,
    })
  })

  it(`is null everywhere else, legacy redirects included`, () => {
    for (const path of [
      `/t/acme`,
      `/t/acme/inbox`,
      `/t/acme/boards/web`,
      `/t/acme/boards/web/issues/MET-1/session`,
      `/t/acme/sessions/s1/issue`,
      `/t/acme/settings/general`,
      `/onboarding`,
    ]) {
      expect(routePathFromLocation(path, null), path).toBeNull()
    }
  })
})

describe(`routeTabKey / routeTabFace`, () => {
  it(`makes an issue and its run the same tab`, () => {
    expect(routeTabKey({ kind: `issue`, issueId: `i1`, from: null })).toBe(`issue:i1`)
    expect(
      routeTabKey({ kind: `run`, runId: `s1`, issueId: `i1`, from: null })
    ).toBe(`issue:i1`)
    expect(
      routeTabKey({ kind: `run`, runId: `s1`, issueId: null, from: null })
    ).toBe(`run:s1`)
    expect(routeTabFace({ kind: `issue`, issueId: `i1`, from: null })).toBe(`issue`)
    expect(
      routeTabFace({ kind: `run`, runId: `s1`, issueId: `i1`, from: null })
    ).toBe(`run`)
    expect(routeTabFace({ kind: `support`, threadId: `t`, from: null })).toBeNull()
  })
})

describe(`upsertFromRoute`, () => {
  it(`appends a new tab and focuses an existing one`, () => {
    let s = upsertFromRoute(EMPTY_WORK_TABS, {
      kind: `issue`,
      issueId: `i1`,
      from: `board:web`,
    })
    s = upsertFromRoute(s, { kind: `support`, threadId: `t1`, from: `support` })
    expect(keys(s)).toEqual([`issue:i1`, `support:t1`])
    // Re-opening the same issue with the same token changes nothing.
    const again = upsertFromRoute(s, {
      kind: `issue`,
      issueId: `i1`,
      from: `board:web`,
    })
    expect(again).toBe(s)
  })

  it(`flips the face and binds the run on the issue's run route`, () => {
    let s = upsertFromRoute(EMPTY_WORK_TABS, {
      kind: `issue`,
      issueId: `i1`,
      from: null,
    })
    s = upsertFromRoute(s, { kind: `run`, runId: `s1`, issueId: `i1`, from: null })
    expect(s.tabs).toEqual([
      { kind: `issue`, issueId: `i1`, face: `run`, runId: `s1`, from: null, live: false },
    ])
    // Back to the Issue face keeps the bound run.
    s = upsertFromRoute(s, { kind: `issue`, issueId: `i1`, from: null })
    expect(s.tabs[0]).toMatchObject({ face: `issue`, runId: `s1` })
  })

  it(`lets an explicit from overwrite the stored one, and keeps it otherwise`, () => {
    let s = upsertFromRoute(EMPTY_WORK_TABS, {
      kind: `issue`,
      issueId: `i1`,
      from: `board:web`,
    })
    s = upsertFromRoute(s, { kind: `issue`, issueId: `i1`, from: `inbox` })
    expect(s.tabs[0]).toMatchObject({ from: `inbox` })
    s = upsertFromRoute(s, { kind: `issue`, issueId: `i1`, from: null })
    expect(s.tabs[0]).toMatchObject({ from: `inbox` })
    // A stored null is filled too.
    let r = upsertFromRoute(EMPTY_WORK_TABS, { kind: `run`, runId: `s9`, issueId: null, from: null })
    r = upsertFromRoute(r, { kind: `run`, runId: `s9`, issueId: null, from: `agent` })
    expect(r.tabs[0]).toMatchObject({ from: `agent` })
  })

  it(`folds a run-only tab into its issue once the issue id syncs`, () => {
    // No issue tab yet: the run tab becomes the issue tab in place.
    let s = state([
      { kind: `support`, threadId: `t1`, from: null },
      { kind: `run`, runId: `s1`, from: `agent`, live: true },
    ])
    s = upsertFromRoute(s, { kind: `run`, runId: `s1`, issueId: `i1`, from: null })
    expect(s.tabs).toEqual([
      { kind: `support`, threadId: `t1`, from: null },
      { kind: `issue`, issueId: `i1`, face: `run`, runId: `s1`, from: `agent`, live: true },
    ])
    // An issue tab already there absorbs it.
    let t = state([
      { kind: `run`, runId: `s1`, from: null, live: true },
      { kind: `issue`, issueId: `i1`, face: `issue`, runId: null, from: null, live: false },
    ])
    t = upsertFromRoute(t, { kind: `run`, runId: `s1`, issueId: `i1`, from: null })
    expect(t.tabs).toEqual([
      { kind: `issue`, issueId: `i1`, face: `run`, runId: `s1`, from: null, live: true },
    ])
  })
})

describe(`reconcileLive`, () => {
  const run = (runId: string, issueId: string | null): LiveRun => ({
    runId,
    issueId,
  })

  // EXP-870 review: a past run of an issue being READ is never rebound to the
  // issue's live run under the reader.
  it(`keeps the run the URL shows bound`, () => {
    const start = state([
      { kind: `issue`, issueId: `i1`, face: `run`, runId: `past`, from: null, live: false },
    ])
    const viewed = reconcileLive(start, [run(`live`, `i1`)], `past`)
    expect(viewed.tabs[0]).toMatchObject({ runId: `past`, live: false })
    const elsewhere = reconcileLive(start, [run(`live`, `i1`)], null)
    expect(elsewhere.tabs[0]).toMatchObject({ runId: `live`, live: true })
  })

  // EXP-923: the live auto-add is GONE — a run of mine never appears as a
  // chip on its own, whatever it links. It is a sidebar row.
  it(`never adds a tab for a live run`, () => {
    expect(
      reconcileLive(EMPTY_WORK_TABS, [run(`s1`, `i1`), run(`s2`, null)])
    ).toBe(EMPTY_WORK_TABS)
    const start = state([
      { kind: `issue`, issueId: `i9`, face: `issue`, runId: null, from: `inbox`, live: false },
    ])
    expect(
      keys(reconcileLive(start, [run(`s1`, `i1`), run(`s2`, null)]))
    ).toEqual([`issue:i9`])
  })

  it(`binds an open issue tab to its issue's live run`, () => {
    const s = reconcileLive(
      state([
        { kind: `issue`, issueId: `i1`, face: `issue`, runId: null, from: `board:web`, live: false },
      ]),
      [run(`s1`, `i1`)]
    )
    expect(s.tabs).toEqual([
      { kind: `issue`, issueId: `i1`, face: `issue`, runId: `s1`, from: `board:web`, live: true },
    ])
    // Idempotent: nothing changes on a second pass.
    expect(reconcileLive(s, [run(`s1`, `i1`)])).toBe(s)
  })

  // A resume starts a NEW run on the same issue: the tab follows it.
  it(`rebinds an ended run's issue tab to the issue's new live run`, () => {
    const s = reconcileLive(
      state([
        { kind: `issue`, issueId: `i1`, face: `run`, runId: `old`, from: null, live: false },
      ]),
      [run(`new`, `i1`)]
    )
    expect(s.tabs[0]).toMatchObject({ runId: `new`, live: true })
  })

  it(`keeps a tab whose run ended, marked not live`, () => {
    const s = reconcileLive(
      state([{ kind: `run`, runId: `s1`, from: null, live: true }]),
      []
    )
    expect(s.tabs).toEqual([{ kind: `run`, runId: `s1`, from: null, live: false }])
  })

  it(`is a no-op with nothing live and nothing open`, () => {
    expect(reconcileLive(EMPTY_WORK_TABS, [])).toBe(EMPTY_WORK_TABS)
  })
})

// EXP-923: the origin the sidebar's Running rows navigate with.
describe(`the tabless origin`, () => {
  it(`names exactly the running origin`, () => {
    expect(TABLESS_ORIGIN).toBe(`running`)
    expect(originCreatesTab(`running`)).toBe(false)
    for (const from of [null, undefined, ``, `agent`, `inbox`, `board:web`]) {
      expect(originCreatesTab(from), String(from)).toBe(true)
    }
  })

  it(`creates no tab for a run, an issue or a thread`, () => {
    expect(
      upsertFromRoute(EMPTY_WORK_TABS, {
        kind: `run`,
        runId: `s1`,
        issueId: null,
        from: `running`,
      })
    ).toBe(EMPTY_WORK_TABS)
    expect(
      upsertFromRoute(EMPTY_WORK_TABS, {
        kind: `run`,
        runId: `s1`,
        issueId: `i1`,
        from: `running`,
      })
    ).toBe(EMPTY_WORK_TABS)
    expect(
      upsertFromRoute(EMPTY_WORK_TABS, {
        kind: `issue`,
        issueId: `i1`,
        from: `running`,
      })
    ).toBe(EMPTY_WORK_TABS)
    expect(
      upsertFromRoute(EMPTY_WORK_TABS, {
        kind: `support`,
        threadId: `t1`,
        from: `running`,
      })
    ).toBe(EMPTY_WORK_TABS)
  })

  it(`reuses an issue tab that is already open, keeping its origin`, () => {
    const s = upsertFromRoute(
      state([
        { kind: `issue`, issueId: `i1`, face: `issue`, runId: null, from: `board:web`, live: false },
      ]),
      { kind: `run`, runId: `s1`, issueId: `i1`, from: `running` }
    )
    expect(s.tabs).toEqual([
      { kind: `issue`, issueId: `i1`, face: `run`, runId: `s1`, from: `board:web`, live: false },
    ])
  })

  it(`toggling that tab's faces still creates nothing new`, () => {
    let s = state([
      { kind: `issue`, issueId: `i1`, face: `run`, runId: `s1`, from: null, live: true },
    ])
    s = upsertFromRoute(s, { kind: `issue`, issueId: `i1`, from: `running` })
    expect(keys(s)).toEqual([`issue:i1`])
    expect(s.tabs[0]).toMatchObject({ face: `issue`, runId: `s1` })
  })
})

describe(`closeTabs`, () => {
  // EXP-923: the live exception went with the live tabs — every tab closes.
  it(`closes every tab it is asked to, live or not`, () => {
    const s = state([
      { kind: `run`, runId: `live`, from: null, live: true },
      { kind: `run`, runId: `ended`, from: null, live: false },
      { kind: `support`, threadId: `t1`, from: null },
      { kind: `issue`, issueId: `i1`, face: `issue`, runId: `s3`, from: null, live: true },
    ])
    expect(
      keys(closeTabs(s, [`run:live`, `run:ended`, `support:t1`, `issue:i1`]))
    ).toEqual([])
    expect(keys(closeTabs(s, [`run:live`]))).toEqual([
      `run:ended`,
      `support:t1`,
      `issue:i1`,
    ])
  })

  it(`is a no-op for unknown keys`, () => {
    const s = state([{ kind: `support`, threadId: `t1`, from: null }])
    expect(closeTabs(s, [`issue:nope`])).toBe(s)
    expect(closeTabs(s, [])).toBe(s)
  })
})

// EXP-923: a merged run's tab goes; every other ending leaves it standing.
describe(`closeMergedRunTabs`, () => {
  const tabs = (): WorkTab[] => [
    { kind: `issue`, issueId: `i1`, face: `run`, runId: `s1`, from: null, live: true },
    { kind: `run`, runId: `s2`, from: null, live: true },
    { kind: `support`, threadId: `t1`, from: null },
  ]

  it(`closes the merged run's tab`, () => {
    expect(
      keys(
        closeMergedRunTabs(state(tabs()), [
          { runId: `s1`, issueId: `i1`, endedBy: `merge` },
        ])
      )
    ).toEqual([`run:s2`, `support:t1`])
    expect(
      keys(
        closeMergedRunTabs(state(tabs()), [
          { runId: `s2`, issueId: null, endedBy: `merge` },
        ])
      )
    ).toEqual([`issue:i1`, `support:t1`])
  })

  it(`closes every OTHER tab of the merged issue too`, () => {
    const s = state([
      { kind: `issue`, issueId: `i1`, face: `issue`, runId: null, from: null, live: false },
      { kind: `run`, runId: `s1`, from: null, live: true },
    ])
    expect(
      keys(closeMergedRunTabs(s, [{ runId: `s1`, issueId: `i1`, endedBy: `merge` }]))
    ).toEqual([])
  })

  it(`leaves every other ending alone`, () => {
    const s = state(tabs())
    for (const endedBy of [null, `user`, `agent`, `stale`, `system`]) {
      expect(
        closeMergedRunTabs(s, [{ runId: `s1`, issueId: `i1`, endedBy }]),
        String(endedBy)
      ).toBe(s)
    }
    expect(closeMergedRunTabs(s, [])).toBe(s)
  })
})

describe(`pruneTabs`, () => {
  it(`drops unresolved tabs`, () => {
    const s = state([
      { kind: `support`, threadId: `t1`, from: null },
      { kind: `run`, runId: `gone`, from: null, live: false },
    ])
    expect(keys(pruneTabs(s, (tab) => tab.kind === `support`))).toEqual([
      `support:t1`,
    ])
    expect(pruneTabs(s, () => true)).toBe(s)
  })

  // EXP-923: no grouping and no reordering — the strip IS the stored order.
  it(`keeps the stored order`, () => {
    const tabs: WorkTab[] = [
      { kind: `support`, threadId: `t1`, from: null },
      { kind: `run`, runId: `a`, from: null, live: true },
      { kind: `issue`, issueId: `i1`, face: `issue`, runId: null, from: null, live: false },
    ]
    expect(pruneTabs(state(tabs), () => true).tabs.map(tabKey)).toEqual([
      `support:t1`,
      `run:a`,
      `issue:i1`,
    ])
  })
})

describe(`tabHref`, () => {
  const resolve = (id: string) =>
    id === `i1` ? { boardSlug: `web`, identifier: `MET-1` } : null

  it(`opens the issue face on the canonical issue URL`, () => {
    expect(
      tabHref(
        `acme`,
        { kind: `issue`, issueId: `i1`, face: `issue`, runId: `s1`, from: `inbox`, live: true },
        resolve
      )
    ).toEqual({
      to: `/t/$teamSlug/boards/$boardSlug/issues/$issueIdentifier`,
      params: { teamSlug: `acme`, boardSlug: `web`, issueIdentifier: `MET-1` },
      search: { from: `inbox` },
    })
  })

  it(`opens the run face on the one run URL`, () => {
    expect(
      tabHref(
        `acme`,
        { kind: `issue`, issueId: `i1`, face: `run`, runId: `s1`, from: null, live: true },
        resolve
      )
    ).toEqual({
      to: `/t/$teamSlug/sessions/$sessionId`,
      params: { teamSlug: `acme`, sessionId: `s1` },
      search: {},
    })
    expect(
      tabHref(`acme`, { kind: `run`, runId: `s2`, from: `agent`, live: false }, resolve)
    ).toEqual({
      to: `/t/$teamSlug/sessions/$sessionId`,
      params: { teamSlug: `acme`, sessionId: `s2` },
      search: { from: `agent` },
    })
    expect(
      tabHref(`acme`, { kind: `support`, threadId: `t1`, from: `support` }, resolve)
    ).toEqual({
      to: `/t/$teamSlug/support/$threadId`,
      params: { teamSlug: `acme`, threadId: `t1` },
      search: { from: `support` },
    })
  })

  it(`waits for an unsynced issue`, () => {
    expect(
      tabHref(
        `acme`,
        { kind: `issue`, issueId: `i2`, face: `issue`, runId: null, from: null, live: false },
        resolve
      )
    ).toBeNull()
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
      { kind: `issue`, issueId: `i1`, face: `run`, runId: `s1`, from: `inbox`, live: true },
      { kind: `run`, runId: `s2`, from: null, live: false },
      { kind: `support`, threadId: `t1`, from: `support` },
    ])
    expect(parseWorkTabsState(JSON.stringify(s))).toEqual(s)
  })

  // EXP-877: a state written before the dismissal memory was deleted still
  // reads — the key is simply dropped.
  it(`tolerates a legacy dismissed key`, () => {
    expect(
      parseWorkTabsState(
        JSON.stringify({
          tabs: [{ kind: `run`, runId: `s1`, from: null, live: true }],
          dismissed: { s1: { attention: true, review: false } },
        })
      )
    ).toEqual({ tabs: [{ kind: `run`, runId: `s1`, from: null, live: true }] })
  })

  it(`reads corrupt or foreign data as an empty strip`, () => {
    expect(parseWorkTabsState(null)).toEqual(EMPTY_WORK_TABS)
    expect(parseWorkTabsState(`{nope`)).toEqual(EMPTY_WORK_TABS)
    expect(parseWorkTabsState(`42`)).toEqual(EMPTY_WORK_TABS)
    expect(
      parseWorkTabsState(
        JSON.stringify({
          tabs: [
            { kind: `issue` },
            { kind: `weird`, id: 1 },
            { kind: `run`, runId: `s1`, live: `yes` },
            { kind: `run`, runId: `s1`, live: true },
            // A run face with no run falls back to the issue face.
            { kind: `issue`, issueId: `i1`, face: `run`, runId: null },
          ],
        })
      )
    ).toEqual({
      tabs: [
        { kind: `run`, runId: `s1`, from: null, live: false },
        { kind: `issue`, issueId: `i1`, face: `issue`, runId: null, from: null, live: false },
      ],
    })
  })
})

