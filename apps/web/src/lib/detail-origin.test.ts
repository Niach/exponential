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
    // EXP-862: the Automations page lists the finished AUTOMATED runs, so it
    // is a list screen of its own.
    expect(screenFromPath(`/t/acme/automations`)).toEqual({
      kind: `automations`,
    })
    expect(screenFromPath(`/t/acme/support`)).toEqual({ kind: `support` })
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
    // Full pages and anything outside a team are context-free — EXP-870's two
    // legacy redirect routes included (they never render a page of their own).
    for (const path of [
      `/t/acme/boards/web/issues/MET-12/session`,
      `/t/acme/sessions/s1/issue`,
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
      `/t/acme/automations`,
      `/t/acme/support`,
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
    // EXP-862: so is the Automations page — a finished automated run opened
    // from it keeps that list, and Back returns to it.
    expect(
      deriveOrigin(
        screenFromPath(`/t/acme/automations`),
        { kind: `automations` },
        { kind: `session` }
      )
    ).toEqual({ kind: `automations` })
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
    expect(capturedOrigin(screenFromPath(`/t/acme/support`))).toEqual({
      kind: `support`,
    })
    expect(capturedOrigin(screenFromPath(`/t/acme/reviews`))).toEqual({
      kind: `reviews`,
    })
    expect(capturedOrigin(screenFromPath(`/t/acme/agent`))).toEqual({
      kind: `agent`,
    })
    // EXP-862: an automated run opened from Automations returns THERE, so
    // the page is its own origin and never hands on what it was opened with.
    expect(capturedOrigin(screenFromPath(`/t/acme/automations`))).toEqual({
      kind: `automations`,
    })
    expect(
      capturedOrigin(screenFromPath(`/t/acme/automations`), board)
    ).toEqual({ kind: `automations` })
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
      board,
      { kind: `support` },
      { kind: `reviews` },
      { kind: `agent` },
      { kind: `automations` },
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
    // EXP-870: the retired issue origin reads as its board's list.
    expect(parseOrigin(`issue:web:MET-12`)).toEqual(board)
    expect(formatOrigin(parseOrigin(`issue:web:MET-12`))).toBe(`board:web`)
    // Junk is no origin at all.
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
    expect(originLabel({ kind: `support` })).toBe(`Support`)
    expect(originLabel({ kind: `reviews` })).toBe(`Reviews`)
    expect(originLabel({ kind: `agent` })).toBe(`Agent`)
    expect(originLabel({ kind: `automations` })).toBe(`Automations`)
    expect(originLabel(board, `Web`)).toBe(`Web`)
    // The board's name has to sync in first — never an empty row.
    expect(originLabel(board)).toBe(`Board`)
  })

  it(`names the board a list nav renders`, () => {
    expect(originBoardSlug(board)).toBe(`web`)
    expect(originBoardSlug(inbox)).toBeNull()
    expect(originBoardSlug({ kind: `agent` })).toBeNull()
    expect(originBoardSlug({ kind: `automations` })).toBeNull()
  })
})

// EXP-851: which of the sidebar's three panels is up, from the URL alone.
describe(`sidebarOccupant`, () => {
  it(`gives settings the slot on every settings route`, () => {
    expect(sidebarOccupant(`/t/acme/settings`, null)).toEqual({
      kind: `settings`,
    })
    expect(sidebarOccupant(`/t/acme/settings/members`, `inbox`)).toEqual({
      kind: `settings`,
    })
  })

  it(`shows the list nav on a detail that carries an origin`, () => {
    for (const path of [
      `/t/acme/boards/web/issues/MET-12`,
      `/t/acme/sessions/s1`,
      `/t/acme/support/t1`,
    ]) {
      expect(sidebarOccupant(path, `inbox`), path).toEqual({
        kind: `list`,
        origin: inbox,
      })
    }
    expect(sidebarOccupant(`/t/acme/sessions/s1`, `board:web`)).toEqual({
      kind: `list`,
      origin: board,
    })
  })

  // EXP-916: a review's panel is its file tree — whatever list it came from,
  // and with no token at all (a deep link).
  it(`gives a review detail its file tree`, () => {
    expect(sidebarOccupant(`/t/acme/reviews/MET-12`, `reviews`)).toEqual({
      kind: `review`,
    })
    expect(sidebarOccupant(`/t/acme/reviews/MET-12`, `inbox`)).toEqual({
      kind: `review`,
    })
    expect(sidebarOccupant(`/t/acme/reviews/MET-12`, null)).toEqual({
      kind: `review`,
    })
    // The queue itself is a list screen: the main menu.
    expect(sidebarOccupant(`/t/acme/reviews`, null)).toEqual({ kind: `main` })
  })

  // EXP-945: a RUN's Changes face fills the very same panel — its diff is the
  // same kind of context a review's is. Only `?view=diff` does it; the run
  // face and the results face keep the list they were opened from.
  it(`gives the run's changes face the same file tree`, () => {
    expect(sidebarOccupant(`/t/acme/sessions/s1`, `agent`, `diff`)).toEqual({
      kind: `review`,
    })
    expect(sidebarOccupant(`/t/acme/sessions/s1`, null, `diff`)).toEqual({
      kind: `review`,
    })
    // EXP-923: the AGENT origin has no list panel any more (the Agent page
    // is the composer alone), so its other faces keep the main menu.
    expect(sidebarOccupant(`/t/acme/sessions/s1`, `agent`, `results`)).toEqual({
      kind: `main`,
    })
    expect(sidebarOccupant(`/t/acme/sessions/s1`, `automations`, `results`)).toEqual({
      kind: `list`,
      origin: { kind: `automations` },
    })
    // An ISSUE's `?view=diff` is the phone's own face — there is no sidebar
    // beside it, and the panel stays the list.
    expect(
      sidebarOccupant(`/t/acme/boards/web/issues/MET-12`, `inbox`, `diff`)
    ).toEqual({ kind: `list`, origin: { kind: `inbox` } })
  })

  // EXP-923: two origins name where Back goes but bring NO panel — `agent`
  // (its list is the page's own toggled Recent panel) and `running` (the
  // sidebar's own section, which lives in the main menu).
  it(`keeps the main menu for the panel-less origins`, () => {
    for (const from of [`agent`, `sessions`, `running`]) {
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
    expect(originHasListNav({ kind: `inbox` })).toBe(true)
    // The Running section is not a page: no Back destination of its own.
    expect(originListNavigation(`acme`, { kind: `running` })).toBeNull()
    // EXP-923: `running` never travels on to the next detail.
    expect(
      capturedOrigin({ kind: `session` }, { kind: `running` })
    ).toBeNull()
  })

  it(`keeps the main menu everywhere else`, () => {
    // Every LIST screen is itself: the main menu stays.
    for (const path of [
      `/t/acme`,
      `/t/acme/inbox`,
      `/t/acme/agent`,
      `/t/acme/automations`,
      `/t/acme/support`,
      `/t/acme/reviews`,
      `/t/acme/devices`,
      `/t/acme/boards/web`,
      // EXP-870: the legacy redirects are not details.
      `/t/acme/boards/web/issues/MET-12/session`,
      `/t/acme/sessions/s1/issue`,
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
    for (const kind of [`support`, `reviews`, `agent`, `automations`] as const) {
      expect(originListNavigation(`acme`, { kind })).toEqual({
        to: `/t/$teamSlug/${kind}`,
        params: { teamSlug: `acme` },
        search: {},
      })
    }
  })

  it(`leaves the no-origin fallback to the caller`, () => {
    expect(originListNavigation(`acme`, null)).toBeNull()
    // A legacy issue token goes back to its board.
    expect(
      originListNavigation(`acme`, parseOrigin(`issue:web:MET-12`))?.params
    ).toEqual({ teamSlug: `acme`, boardSlug: `web` })
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
