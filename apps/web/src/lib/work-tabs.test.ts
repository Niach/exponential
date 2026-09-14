import { describe, expect, it } from "vitest"
import {
  closeTabs,
  EMPTY_WORK_TABS,
  groupedTabs,
  orderedTabs,
  parseCollapsedGroups,
  parseWorkTabsState,
  partitionTabs,
  partitionUnits,
  pruneTabs,
  reconcileLive,
  routePathFromLocation,
  routeTabFace,
  routeTabKey,
  tabHref,
  tabKey,
  upsertFromRoute,
  workTabGroupsStorageKey,
  type LiveRun,
  type WorkTab,
  type WorkTabsState,
} from "@/lib/work-tabs"

// EXP-870: the work-tab model — the same rules the desktop's
// `live_tab_plan` / `partition_tabs` tests walk (apps/desktop/crates/ui/src/
// screens.rs).

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
  const run = (
    runId: string,
    issueId: string | null,
    agent: string | null = `claude`
  ): LiveRun => ({ runId, issueId, agent })

  // EXP-870 review: a past run of an issue being READ is never rebound to the
  // issue's live run under the reader.
  it(`keeps the run the URL shows bound`, () => {
    const state = {
      tabs: [
        {
          key: `issue:i1` as const,
          kind: `issue` as const,
          issueId: `i1`,
          face: `run` as const,
          runId: `past`,
          from: null,
          live: false,
        },
      ],
    }
    const viewed = reconcileLive(state, [run(`live`, `i1`)], `past`)
    expect(viewed.tabs[0]).toMatchObject({ runId: `past`, live: false })
    const elsewhere = reconcileLive(state, [run(`live`, `i1`)], null)
    expect(elsewhere.tabs[0]).toMatchObject({ runId: `live`, live: true })
  })

  it(`adds every live run of mine without touching existing tabs`, () => {
    const start = state([
      { kind: `issue`, issueId: `i9`, face: `issue`, runId: null, from: `inbox`, live: false },
    ])
    const s = reconcileLive(start, [run(`s1`, `i1`), run(`s2`, null)])
    expect(s.tabs).toEqual([
      start.tabs[0],
      { kind: `issue`, issueId: `i1`, face: `run`, runId: `s1`, from: null, live: true },
      { kind: `run`, runId: `s2`, from: null, live: true },
    ])
    // Idempotent: nothing changes on a second pass.
    expect(reconcileLive(s, [run(`s1`, `i1`), run(`s2`, null)])).toBe(s)
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

  it(`never auto-adds an ended run`, () => {
    expect(reconcileLive(EMPTY_WORK_TABS, [])).toBe(EMPTY_WORK_TABS)
  })

  // EXP-877: there is no dismissal memory any more — a live tab that somehow
  // left the strip (an older build's stored state) simply comes straight back.
  it(`re-adds a live run whose tab is gone`, () => {
    const closed = state([])
    const back = reconcileLive(closed, [run(`s1`, null)])
    expect(keys(back)).toEqual([`run:s1`])
  })
})

describe(`closeTabs`, () => {
  // EXP-877: a LIVE tab is permanent — closing the strip around it leaves it
  // standing, whether it is a run-only tab or an issue tab bound to a run.
  it(`never removes a live tab`, () => {
    const s = state([
      { kind: `run`, runId: `live`, from: null, live: true },
      { kind: `run`, runId: `ended`, from: null, live: false },
      { kind: `support`, threadId: `t1`, from: null },
      { kind: `issue`, issueId: `i1`, face: `issue`, runId: `s3`, from: null, live: true },
    ])
    const closed = closeTabs(s, [
      `run:live`,
      `run:ended`,
      `support:t1`,
      `issue:i1`,
    ])
    expect(keys(closed)).toEqual([`run:live`, `issue:i1`])
  })

  it(`is a no-op for unknown keys and for live-only closes`, () => {
    const s = state([{ kind: `support`, threadId: `t1`, from: null }])
    expect(closeTabs(s, [`issue:nope`])).toBe(s)
    const live = state([{ kind: `run`, runId: `s1`, from: null, live: true }])
    expect(closeTabs(live, [`run:s1`])).toBe(live)
  })
})

describe(`pruneTabs / orderedTabs`, () => {
  it(`drops unresolved tabs`, () => {
    const s = state([
      { kind: `support`, threadId: `t1`, from: null },
      { kind: `run`, runId: `gone`, from: null, live: false },
    ])
    expect(keys(pruneTabs(s, (tab) => tab.kind === `support`))).toEqual([`support:t1`])
    expect(pruneTabs(s, () => true)).toBe(s)
  })

  it(`groups live tabs first in stable order`, () => {
    const tabs: WorkTab[] = [
      { kind: `support`, threadId: `t1`, from: null },
      { kind: `run`, runId: `a`, from: null, live: true },
      { kind: `issue`, issueId: `i1`, face: `issue`, runId: null, from: null, live: false },
      { kind: `run`, runId: `b`, from: null, live: true },
    ]
    expect(orderedTabs(tabs).map(tabKey)).toEqual([
      `run:a`,
      `run:b`,
      `support:t1`,
      `issue:i1`,
    ])
  })
})

// EXP-877: the live tabs are grouped by AGENT, in the contract's own order.
describe(`groupedTabs`, () => {
  const liveRun = (runId: string): WorkTab => ({
    kind: `run`,
    runId,
    from: null,
    live: true,
  })
  const tabs: WorkTab[] = [
    liveRun(`codex1`),
    { kind: `support`, threadId: `t1`, from: null },
    liveRun(`claude1`),
    { kind: `issue`, issueId: `i1`, face: `issue`, runId: null, from: null, live: false },
    liveRun(`other1`),
    liveRun(`claude2`),
  ]
  const agents: Record<string, string | null> = {
    "run:codex1": `codex`,
    "run:claude1": null,
    "run:other1": `gemini`,
    "run:claude2": `Claude`,
  }
  const agentOf = (tab: WorkTab) => agents[tabKey(tab)] ?? null

  it(`orders claude, codex, then the agents this build does not know`, () => {
    const { groups, rest } = groupedTabs(tabs, agentOf)
    expect(groups.map((group) => [group.agent, group.tabs.map(tabKey)])).toEqual([
      [`claude`, [`run:claude1`, `run:claude2`]],
      [`codex`, [`run:codex1`]],
      [`gemini`, [`run:other1`]],
    ])
    // The rest keeps its stored order, untouched.
    expect(rest.map(tabKey)).toEqual([`support:t1`, `issue:i1`])
    expect(orderedTabs(tabs, agentOf).map(tabKey)).toEqual([
      `run:claude1`,
      `run:claude2`,
      `run:codex1`,
      `run:other1`,
      `support:t1`,
      `issue:i1`,
    ])
  })

  it(`omits empty groups`, () => {
    expect(
      groupedTabs([liveRun(`codex1`)], () => `codex`).groups.map((g) => g.agent)
    ).toEqual([`codex`])
    expect(groupedTabs([], () => null).groups).toEqual([])
    expect(
      groupedTabs(
        [{ kind: `support`, threadId: `t1`, from: null }],
        () => null
      ).groups
    ).toEqual([])
  })
})

describe(`parseCollapsedGroups`, () => {
  it(`reads an agent id list, dropping junk`, () => {
    expect(parseCollapsedGroups(JSON.stringify([`claude`, `codex`]))).toEqual([
      `claude`,
      `codex`,
    ])
    expect(parseCollapsedGroups(JSON.stringify([`claude`, `claude`]))).toEqual([
      `claude`,
    ])
    expect(parseCollapsedGroups(JSON.stringify([1, ``, `codex`]))).toEqual([
      `codex`,
    ])
    for (const raw of [null, undefined, ``, `{nope`, `42`, `{}`]) {
      expect(parseCollapsedGroups(raw)).toEqual([])
    }
  })

  it(`keys per team`, () => {
    expect(workTabGroupsStorageKey(`t1`)).toBe(`exp:work-tab-groups:v1:t1`)
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

// EXP-877: the strip lays out group marks and chevrons beside the chips —
// only the chips are packed, the rest is always drawn.
describe(`partitionUnits`, () => {
  const GAP = 3.5
  const OVERFLOW = 22
  const chip = (width: number) => ({ packed: true, width })
  const fixed = (width: number) => ({ packed: false, width })

  it(`never drops a group mark or a chevron, however tight the row`, () => {
    // [mark][chip][chip][chevron][mark] with room for barely one chip.
    const units = [fixed(20), chip(100), chip(100), fixed(20), fixed(20)]
    const visible = partitionUnits(units, 160, GAP, OVERFLOW, null)
    expect(visible).toContain(0)
    expect(visible).toContain(3)
    expect(visible).toContain(4)
    // At least one chip survives, and the second one is the "+N".
    expect(visible).toEqual([0, 1, 3, 4])
  })

  it(`charges the fixed units to the budget before packing the chips`, () => {
    const units = [fixed(20), chip(100), chip(100)]
    // Room for both chips only once the mark is paid for.
    const both = 20 + GAP + 100 + GAP + 100
    expect(partitionUnits(units, both, GAP, OVERFLOW, null)).toEqual([0, 1, 2])
    expect(partitionUnits(units, both - 1, GAP, OVERFLOW, null)).toEqual([0, 1])
  })

  it(`keeps the ACTIVE chip visible, counted among the chips only`, () => {
    const units = [fixed(20), chip(100), chip(100), chip(100)]
    // Unit index 3 is the active chip: it displaces the last packed one.
    const visible = partitionUnits(
      units,
      20 + GAP + 200 + GAP + OVERFLOW + GAP,
      GAP,
      OVERFLOW,
      3
    )
    expect(visible).toEqual([0, 1, 3])
    // An ACTIVE unit that is not packed (a folded chip, a mark) is simply
    // always drawn and commits no width of its own.
    expect(
      partitionUnits([fixed(20), chip(100)], 400, GAP, OVERFLOW, 0)
    ).toEqual([0, 1])
  })

  it(`is every unit when everything fits, and nothing for no units`, () => {
    const units = [fixed(20), chip(50), fixed(20)]
    expect(partitionUnits(units, 400, GAP, OVERFLOW, null)).toEqual([0, 1, 2])
    expect(partitionUnits([], 400, GAP, OVERFLOW, null)).toEqual([])
    // Fixed units alone never overflow.
    expect(partitionUnits([fixed(80), fixed(80)], 10, GAP, OVERFLOW, null)).toEqual([
      0, 1,
    ])
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
