import { describe, expect, it } from "vitest"
import {
  capturedOrigin,
  deriveOrigin,
  formatOrigin,
  isContextFree,
  originBoardSlug,
  originLabel,
  originHasListNav,
  originListNavigation,
  panelOffset,
  parseOrigin,
  screenFromPath,
  sidebarOccupant,
  type DetailOrigin,
} from "@/lib/detail-origin"

// EXP-818: the same cases the desktop's `derive_origin` test walks
// (apps/desktop/crates/ui/src/navigation.rs
// `derive_origin_keeps_the_list_context_and_derives_without_one`), on the web's
// routes. EXP-851 widened the vocabulary to every LIST screen and made the
// token the sidebar's only input (`sidebarOccupant`).

const inbox: DetailOrigin = { kind: `inbox` }
const board: DetailOrigin = { kind: `board`, boardSlug: `web` }

describe(`screenFromPath`, () => {
  it(`names every list and detail screen, and calls the rest context-free`, () => {
    expect(screenFromPath(`/t/acme/inbox`)).toEqual({ kind: `inbox` })
    expect(screenFromPath(`/t/acme/agent`)).toEqual({ kind: `agent` })
    // EXP-862/SLOP-2: an action's page lists its runs, the TRIGGERED ones
    // included, so it is a list screen of its own.
    expect(screenFromPath(`/t/acme/actions/a1`)).toEqual({
      kind: `action`,
      actionId: `a1`,
    })
    expect(screenFromPath(`/t/acme/actions`)).toEqual({ kind: `other` })
    expect(screenFromPath(`/t/acme/reviews`)).toEqual({ kind: `reviews` })
    expect(screenFromPath(`/t/acme/sessions/s1`)).toEqual({ kind: `session` })
    expect(screenFromPath(`/t/acme/boards/web`)).toEqual({
      kind: `board`,
      boardSlug: `web`,
    })
    expect(screenFromPath(`/t/acme/boards/web/issues/MET-12`)).toEqual({
      kind: `issue`,
      boardSlug: `web`,
      identifier: `MET-12`,
    })
    // Full pages and anything outside a team are context-free.
    for (const path of [
      `/t/acme/devices`,
      `/t/acme/reviews/MET-3`,
      `/t/acme/settings/general`,
      `/t/acme`,
      `/onboarding`,
    ]) {
      expect(screenFromPath(path), path).toEqual({ kind: `other` })
      expect(isContextFree(screenFromPath(path)), path).toBe(true)
    }
    // A list or a detail is NOT context-free.
    for (const path of [
      `/t/acme/inbox`,
      `/t/acme/agent`,
      `/t/acme/actions/a1`,
      `/t/acme/reviews`,
      `/t/acme/sessions/s1`,
      `/t/acme/boards/web`,
      `/t/acme/boards/web/issues/MET-12`,
    ]) {
      expect(isContextFree(screenFromPath(path)), path).toBe(false)
    }
    expect(isContextFree(null)).toBe(true)
  })
})

describe(`deriveOrigin`, () => {
  it(`keeps the list context, and derives one without it`, () => {
    const issueScreen = screenFromPath(`/t/acme/boards/web/issues/MET-12`)
    const session = screenFromPath(`/t/acme/sessions/s1`)

    // Inbox → issue: the inbox stays.
    expect(
      deriveOrigin(screenFromPath(`/t/acme/inbox`), inbox, {
        kind: `issue`,
        boardSlug: `other`,
      })
    ).toEqual(inbox)
    // Inbox → a running session: the inbox stays.
    expect(deriveOrigin(issueScreen, inbox, { kind: `session` })).toEqual(inbox)
    // Board → issue → Watch: the issue's board list stays beside the run
    // (EXP-870: one run URL, the issue is no origin of its own).
    expect(
      deriveOrigin(issueScreen, capturedOrigin(issueScreen, board), {
        kind: `session`,
      })
    ).toEqual(board)
    // The Agent page is a list context (the sessions column's own center).
    expect(
      deriveOrigin(screenFromPath(`/t/acme/agent`), { kind: `agent` }, {
        kind: `session`,
      })
    ).toEqual({ kind: `agent` })
    // EXP-862: so is an action's page — a triggered run opened from it
    // keeps that list, and Back returns to it.
    expect(
      deriveOrigin(
        screenFromPath(`/t/acme/actions/a1`),
        { kind: `action`, actionId: `a1` },
        { kind: `session` }
      )
    ).toEqual({ kind: `action`, actionId: `a1` })
    // Devices (context-free) → a session: no origin at all, so the sidebar's
    // main menu stays put.
    expect(
      deriveOrigin(screenFromPath(`/t/acme/devices`), inbox, {
        kind: `session`,
      })
    ).toBeNull()
    // Reviews → an issue: the issue's board comes along… except Reviews is a
    // LIST now, so its own origin stays.
    expect(
      deriveOrigin(screenFromPath(`/t/acme/reviews`), { kind: `reviews` }, {
        kind: `issue`,
        boardSlug: `web`,
      })
    ).toEqual({ kind: `reviews` })
    // A deep link at boot (nothing before) → issue: its board…
    expect(
      deriveOrigin(null, inbox, { kind: `issue`, boardSlug: `web` })
    ).toEqual(board)
    // …but an issue whose board is unknown keeps what was up.
    expect(deriveOrigin(null, inbox, { kind: `issue` })).toEqual(inbox)
    // A session opened from another session keeps that session's origin.
    expect(deriveOrigin(session, board, { kind: `session` })).toEqual(board)
  })
})

describe(`capturedOrigin`, () => {
  it(`reads the origin in force on a screen`, () => {
    expect(capturedOrigin(screenFromPath(`/t/acme/inbox`))).toEqual(inbox)
    expect(capturedOrigin(screenFromPath(`/t/acme/boards/web`))).toEqual(board)
    expect(capturedOrigin(screenFromPath(`/t/acme/reviews`))).toEqual({
      kind: `reviews`,
    })
    expect(capturedOrigin(screenFromPath(`/t/acme/agent`))).toEqual({
      kind: `agent`,
    })
    // EXP-862: a run opened from an action's page returns THERE, so the
    // page is its own origin and never hands on what it was opened with.
    expect(capturedOrigin(screenFromPath(`/t/acme/actions/a1`))).toEqual({
      kind: `action`,
      actionId: `a1`,
    })
    expect(
      capturedOrigin(screenFromPath(`/t/acme/actions/a1`), board)
    ).toEqual({ kind: `action`, actionId: `a1` })
    // The Agent page hands on the origin the composer was opened with — that
    // is how "start coding from an issue" survives the launcher hop.
    expect(capturedOrigin(screenFromPath(`/t/acme/agent`), board)).toEqual(
      board
    )
    // A session hands on the origin it CARRIES…
    expect(
      capturedOrigin(screenFromPath(`/t/acme/sessions/s1`), inbox)
    ).toEqual(inbox)
    // …and nothing when it carries none.
    expect(capturedOrigin(screenFromPath(`/t/acme/sessions/s1`))).toBeNull()
    // EXP-870: an issue hands on the list it carries, and none without one
    // (a pinned issue's run keeps the rail, desktop parity).
    expect(
      capturedOrigin(screenFromPath(`/t/acme/boards/web/issues/MET-12`), inbox)
    ).toEqual(inbox)
    expect(
      capturedOrigin(screenFromPath(`/t/acme/boards/web/issues/MET-12`))
    ).toBeNull()
    // A full page carries nothing of its own.
    expect(capturedOrigin(screenFromPath(`/t/acme/devices`))).toBeNull()
    expect(capturedOrigin(screenFromPath(`/t/acme/devices`), board)).toEqual(
      board
    )
  })
})

describe(`formatOrigin / parseOrigin`, () => {
  it(`round-trips every origin`, () => {
    const origins: DetailOrigin[] = [
      inbox,
      { kind: `inbox`, tab: `my-issues` },
      { kind: `inbox`, tab: `drafts` },
      board,
      { kind: `reviews` },
      { kind: `agent` },
      { kind: `action`, actionId: `a1` },
      { kind: `drafts` },
    ]
    for (const origin of origins) {
      expect(parseOrigin(formatOrigin(origin)), formatOrigin(origin)).toEqual(
        origin
      )
    }
    // No origin = no param: the sidebar's main menu stays.
    expect(formatOrigin(null)).toBeUndefined()
    // EXP-818's spelling of the Agent list still parses.
    expect(parseOrigin(`sessions`)).toEqual({ kind: `agent` })
    // Junk is no origin at all — the retired `issue:` origin included.
    expect(parseOrigin(`issue:web:MET-12`)).toBeNull()
    expect(parseOrigin(``)).toBeNull()
    expect(parseOrigin(`board:`)).toBeNull()
    expect(parseOrigin(`nope`)).toBeNull()
    expect(parseOrigin(undefined)).toBeNull()
  })
})

describe(`originLabel / originBoardSlug`, () => {
  it(`names the list the back row returns to`, () => {
    expect(originLabel(inbox)).toBe(`Inbox`)
    expect(originLabel({ kind: `inbox`, tab: `my-issues` })).toBe(`Inbox`)
    expect(originLabel({ kind: `reviews` })).toBe(`Reviews`)
    expect(originLabel({ kind: `agent` })).toBe(`Agent`)
    expect(originLabel({ kind: `action`, actionId: `a1` }, `Triage`)).toBe(`Triage`)
    expect(originLabel(board, `Web`)).toBe(`Web`)
    // The board's name has to sync in first — never an empty row.
    expect(originLabel(board)).toBe(`Board`)
  })

  it(`names the board a list nav renders`, () => {
    expect(originBoardSlug(board)).toBe(`web`)
    expect(originBoardSlug(inbox)).toBeNull()
    expect(originBoardSlug({ kind: `agent` })).toBeNull()
    expect(originBoardSlug({ kind: `action`, actionId: `a1` })).toBeNull()
  })
})

// EXP-851: which of the sidebar's panels is up, from the URL alone.
// EXP-1246: a list stays beside a detail ONLY for the Inbox and Agent › Recent.
describe(`sidebarOccupant`, () => {
  it(`gives settings the slot on every settings route`, () => {
    expect(sidebarOccupant(`/t/acme/settings`, null)).toEqual({
      kind: `settings`,
    })
    expect(sidebarOccupant(`/t/acme/settings/members`, `inbox`)).toEqual({
      kind: `settings`,
    })
  })

  it(`keeps the inbox beside a detail opened from it`, () => {
    for (const path of [
      `/t/acme/boards/web/issues/MET-12`,
      `/t/acme/sessions/s1`,
    ]) {
      expect(sidebarOccupant(path, `inbox`), path).toEqual({
        kind: `list`,
        origin: inbox,
      })
      expect(sidebarOccupant(path, `inbox:my-issues`), path).toEqual({
        kind: `list`,
        origin: { kind: `inbox`, tab: `my-issues` },
      })
    }
  })

  it(`keeps Recent beside a run opened from it`, () => {
    expect(sidebarOccupant(`/t/acme/sessions/s1`, `agent:recent`)).toEqual({
      kind: `list`,
      origin: { kind: `agent`, tab: `recent` },
    })
    expect(
      sidebarOccupant(`/t/acme/boards/web/issues/MET-12`, `agent:recent`)
    ).toEqual({ kind: `list`, origin: { kind: `agent`, tab: `recent` } })
  })

  // EXP-1246: the md+ Inbox page IS the list-detail host with nothing picked.
  it(`shows the inbox host on the inbox page itself`, () => {
    expect(sidebarOccupant(`/t/acme/inbox`, null)).toEqual({
      kind: `list`,
      origin: inbox,
    })
    expect(
      sidebarOccupant(`/t/acme/inbox`, null, { tab: `my-issues` })
    ).toEqual({ kind: `list`, origin: { kind: `inbox`, tab: `my-issues` } })
    // The phone-only Drafts tab has no list host.
    expect(sidebarOccupant(`/t/acme/inbox`, null, { tab: `drafts` })).toEqual({
      kind: `list`,
      origin: inbox,
    })
  })

  it(`shows Recent on the Agent page only while its panel is open`, () => {
    expect(sidebarOccupant(`/t/acme/agent`, null)).toEqual({ kind: `main` })
    expect(
      sidebarOccupant(`/t/acme/agent`, null, { recentOpen: true })
    ).toEqual({ kind: `list`, origin: { kind: `agent`, tab: `recent` } })
    // The flag means nothing off the Agent page.
    expect(
      sidebarOccupant(`/t/acme/boards/web`, null, { recentOpen: true })
    ).toEqual({ kind: `main` })
  })

  // EXP-1246: board, reviews and action origins keep Back but no list — the
  // review state of the bug report (a Reviews list beside a plain issue tab)
  // cannot happen any more.
  it(`keeps the main menu for every other origin`, () => {
    for (const from of [
      `board:web`,
      `reviews`,
      `action:a1`,
      `agent`,
      `sessions`,
      `running`,
      `drafts`,
      `inbox:drafts`,
    ]) {
      expect(sidebarOccupant(`/t/acme/sessions/s1`, from), from).toEqual({
        kind: `main`,
      })
      expect(
        sidebarOccupant(`/t/acme/boards/web/issues/MET-12`, from),
        from
      ).toEqual({ kind: `main` })
    }
    // They still parse, so Back and the tabless rule can read them.
    expect(parseOrigin(`running`)).toEqual({ kind: `running` })
    expect(formatOrigin({ kind: `running` })).toBe(`running`)
    expect(originHasListNav({ kind: `running` })).toBe(false)
    expect(originHasListNav({ kind: `agent` })).toBe(false)
    expect(originHasListNav({ kind: `board`, boardSlug: `web` })).toBe(false)
    expect(originHasListNav({ kind: `reviews` })).toBe(false)
    expect(originHasListNav({ kind: `action`, actionId: `a1` })).toBe(false)
    expect(originHasListNav({ kind: `inbox` })).toBe(true)
    expect(originHasListNav({ kind: `inbox`, tab: `my-issues` })).toBe(true)
    expect(originHasListNav({ kind: `agent`, tab: `recent` })).toBe(true)
    // The Running section is not a page: no Back destination of its own.
    expect(originListNavigation(`acme`, { kind: `running` })).toBeNull()
    // EXP-923: `running` never travels on to the next detail.
    expect(
      capturedOrigin({ kind: `session` }, { kind: `running` })
    ).toBeNull()
  })

  it(`round-trips the agent:recent token`, () => {
    expect(parseOrigin(`agent:recent`)).toEqual({ kind: `agent`, tab: `recent` })
    expect(formatOrigin({ kind: `agent`, tab: `recent` })).toBe(`agent:recent`)
    expect(originLabel({ kind: `agent`, tab: `recent` })).toBe(`Agent`)
    expect(
      originListNavigation(`acme`, { kind: `agent`, tab: `recent` })
    ).toEqual({ to: `/t/$teamSlug/agent`, params: { teamSlug: `acme` }, search: {} })
    // A run opened from Recent hands the origin on to its issue face.
    expect(
      capturedOrigin({ kind: `session` }, { kind: `agent`, tab: `recent` })
    ).toEqual({ kind: `agent`, tab: `recent` })
  })

  it(`keeps the main menu everywhere else`, () => {
    for (const path of [
      `/t/acme`,
      `/t/acme/actions/a1`,
      `/t/acme/reviews`,
      `/t/acme/reviews/MET-12`,
      `/t/acme/devices`,
      `/t/acme/boards/web`,
      `/onboarding`,
    ]) {
      expect(sidebarOccupant(path, `inbox`), path).toEqual({ kind: `main` })
    }
    // A detail WITHOUT a token — a pinned row, a sidebar session row, a deep
    // link — keeps the main menu too.
    expect(sidebarOccupant(`/t/acme/sessions/s1`, null)).toEqual({
      kind: `main`,
    })
    expect(sidebarOccupant(`/t/acme/boards/web/issues/MET-12`, `junk`)).toEqual({
      kind: `main`,
    })
  })
})

// EXP-870: the one back-to-the-list destination.
describe(`originListNavigation`, () => {
  it(`names every list's own route`, () => {
    expect(originListNavigation(`acme`, board)).toEqual({
      to: `/t/$teamSlug/boards/$boardSlug`,
      params: { teamSlug: `acme`, boardSlug: `web` },
      search: {},
    })
    expect(originListNavigation(`acme`, inbox)).toEqual({
      to: `/t/$teamSlug/inbox`,
      params: { teamSlug: `acme` },
      search: {},
    })
    expect(
      originListNavigation(`acme`, { kind: `inbox`, tab: `my-issues` })
    ).toEqual({
      to: `/t/$teamSlug/inbox`,
      params: { teamSlug: `acme` },
      search: { tab: `my-issues` },
    })
    for (const kind of [`reviews`, `agent`] as const) {
      expect(originListNavigation(`acme`, { kind })).toEqual({
        to: `/t/$teamSlug/${kind}`,
        params: { teamSlug: `acme` },
        search: {},
      })
    }
    expect(
      originListNavigation(`acme`, { kind: `action`, actionId: `a1` })
    ).toEqual({
      to: `/t/$teamSlug/actions/$actionId`,
      params: { teamSlug: `acme`, actionId: `a1` },
      search: { tab: `runs` },
    })
  })

  it(`leaves the no-origin fallback to the caller`, () => {
    expect(originListNavigation(`acme`, null)).toBeNull()
  })

  // EXP-1170: a draft reopened from the phone inbox's Drafts tab returns to
  // that TAB, never to Notifications.
  it(`returns a phone draft to the inbox Drafts tab`, () => {
    expect(parseOrigin(`inbox:drafts`)).toEqual({ kind: `inbox`, tab: `drafts` })
    expect(
      originListNavigation(`acme`, parseOrigin(`inbox:drafts`))
    ).toEqual({
      to: `/t/$teamSlug/inbox`,
      params: { teamSlug: `acme` },
      search: { tab: `drafts` },
    })
  })

  // EXP-1170: a draft reopened from the md+ Drafts list returns there.
  it(`returns a draft to the Drafts list`, () => {
    expect(originListNavigation(`acme`, { kind: `drafts` })).toEqual({
      to: `/t/$teamSlug/drafts`,
      params: { teamSlug: `acme` },
      search: {},
    })
    expect(originLabel({ kind: `drafts` })).toBe(`Drafts`)
    expect(originHasListNav({ kind: `drafts` })).toBe(false)
  })
})

// EXP-1170: the New issue page is the issue detail in DRAFT mode, so it keeps
// the list it was opened from beside it exactly like a detail does.
describe(`sidebarOccupant on the draft page`, () => {
  const draftPath = `/t/acme/drafts/1b4e28ba-2fa1-11d2-883f-0016d3cca427`

  it(`keeps the inbox beside a draft opened from it`, () => {
    expect(sidebarOccupant(draftPath, `inbox`)).toEqual({
      kind: `list`,
      origin: inbox,
    })
    // EXP-1246: a board origin names Back only.
    expect(sidebarOccupant(draftPath, `board:web`)).toEqual({ kind: `main` })
  })

  it(`keeps the main menu without a list origin`, () => {
    expect(sidebarOccupant(draftPath, null)).toEqual({ kind: `main` })
    expect(sidebarOccupant(draftPath, `drafts`)).toEqual({ kind: `main` })
  })

  it(`treats the Drafts list itself as a list screen`, () => {
    expect(sidebarOccupant(`/t/acme/drafts`, `board:web`)).toEqual({
      kind: `main`,
    })
  })
})

// EXP-870: the directional slide — deeper panels wait under the rail's edge,
// shallower ones are pushed out right.
describe(`panelOffset`, () => {
  it(`puts the occupant in the slot`, () => {
    expect(panelOffset(`list`, `list`)).toBe(0)
    expect(panelOffset(`settings`, `settings`)).toBe(0)
  })

  it(`tucks both panels under the rail while the main menu is up`, () => {
    expect(panelOffset(`list`, `main`)).toBe(-1)
    expect(panelOffset(`settings`, `main`)).toBe(-1)
  })

  it(`slides by direction between the list nav and settings`, () => {
    // Forward (list → settings): settings enters from the rail, the list is
    // pushed right…
    expect(panelOffset(`settings`, `list`)).toBe(-1)
    expect(panelOffset(`list`, `settings`)).toBe(1)
  })
})
