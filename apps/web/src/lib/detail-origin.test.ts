import { describe, expect, it } from "vitest"
import {
  capturedOrigin,
  deriveOrigin,
  formatOrigin,
  isContextFree,
  originBoardSlug,
  originLabel,
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
const issue: DetailOrigin = {
  kind: `issue`,
  boardSlug: `web`,
  identifier: `MET-12`,
}

describe(`screenFromPath`, () => {
  it(`names every list and detail screen, and calls the rest context-free`, () => {
    expect(screenFromPath(`/t/acme/inbox`)).toEqual({ kind: `inbox` })
    expect(screenFromPath(`/t/acme/agent`)).toEqual({ kind: `agent` })
    expect(screenFromPath(`/t/acme/support`)).toEqual({ kind: `support` })
    expect(screenFromPath(`/t/acme/reviews`)).toEqual({ kind: `reviews` })
    expect(screenFromPath(`/t/acme/sessions/s1`)).toEqual({ kind: `session` })
    expect(screenFromPath(`/t/acme/sessions/s1/issue`)).toEqual({
      kind: `session`,
    })
    expect(screenFromPath(`/t/acme/boards/web`)).toEqual({
      kind: `board`,
      boardSlug: `web`,
    })
    expect(screenFromPath(`/t/acme/boards/web/issues/MET-12`)).toEqual({
      kind: `issue`,
      boardSlug: `web`,
      identifier: `MET-12`,
    })
    // The issue's own session route is still that issue's context.
    expect(
      screenFromPath(`/t/acme/boards/web/issues/MET-12/session`)
    ).toEqual({ kind: `issue`, boardSlug: `web`, identifier: `MET-12` })
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
    // Board → issue → Watch: the ISSUE is the origin (EXP-851: the run lands
    // on the issue's own session route).
    expect(deriveOrigin(issueScreen, issue, { kind: `session` })).toEqual(issue)
    // The Agent page is a list context (the sessions column's own center).
    expect(
      deriveOrigin(screenFromPath(`/t/acme/agent`), { kind: `agent` }, {
        kind: `session`,
      })
    ).toEqual({ kind: `agent` })
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
    // The Agent page hands on the origin the composer was opened with — that
    // is how "start coding from an issue" survives the launcher hop.
    expect(capturedOrigin(screenFromPath(`/t/acme/agent`), issue)).toEqual(
      issue
    )
    // A session hands on the origin it CARRIES…
    expect(
      capturedOrigin(screenFromPath(`/t/acme/sessions/s1`), inbox)
    ).toEqual(inbox)
    // …and falls back to the Agent list.
    expect(capturedOrigin(screenFromPath(`/t/acme/sessions/s1`))).toEqual({
      kind: `agent`,
    })
    // EXP-851: an ISSUE is always its own origin, whatever it was opened
    // with — a run started here belongs to the issue.
    expect(
      capturedOrigin(screenFromPath(`/t/acme/boards/web/issues/MET-12`), inbox)
    ).toEqual(issue)
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
      issue,
      { kind: `support` },
      { kind: `reviews` },
      { kind: `agent` },
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
    expect(originLabel(board, `Web`)).toBe(`Web`)
    expect(originLabel(issue, `Web`)).toBe(`Web`)
    // The board's name has to sync in first — never an empty row.
    expect(originLabel(board)).toBe(`Board`)
  })

  it(`names the board a list nav renders`, () => {
    expect(originBoardSlug(board)).toBe(`web`)
    expect(originBoardSlug(issue)).toBe(`web`)
    expect(originBoardSlug(inbox)).toBeNull()
    expect(originBoardSlug({ kind: `agent` })).toBeNull()
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
      `/t/acme/boards/web/issues/MET-12/session`,
      `/t/acme/sessions/s1`,
      `/t/acme/sessions/s1/issue`,
      `/t/acme/reviews/MET-12`,
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

  it(`keeps the main menu everywhere else`, () => {
    // Every LIST screen is itself: the main menu stays.
    for (const path of [
      `/t/acme`,
      `/t/acme/inbox`,
      `/t/acme/agent`,
      `/t/acme/support`,
      `/t/acme/reviews`,
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
