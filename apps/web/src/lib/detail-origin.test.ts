import { describe, expect, it } from "vitest"
import {
  capturedOrigin,
  deriveOrigin,
  formatOrigin,
  isContextFree,
  parseOrigin,
  screenFromPath,
  type DetailOrigin,
} from "@/lib/detail-origin"

// EXP-818: the same cases the desktop's `derive_origin` test walks
// (apps/desktop/crates/ui/src/navigation.rs
// `derive_origin_keeps_the_list_context_and_derives_without_one`), on the web's
// routes.

const inbox: DetailOrigin = { kind: `inbox` }
const board: DetailOrigin = { kind: `board`, boardSlug: `web` }

describe(`screenFromPath`, () => {
  it(`names every list and detail screen, and calls the rest context-free`, () => {
    expect(screenFromPath(`/t/acme/inbox`)).toEqual({ kind: `inbox` })
    expect(screenFromPath(`/t/acme/agent`)).toEqual({ kind: `agent` })
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
    // Full pages and anything outside a team are context-free.
    for (const path of [
      `/t/acme/devices`,
      `/t/acme/reviews`,
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
    const issue = screenFromPath(`/t/acme/boards/web/issues/MET-12`)
    const session = screenFromPath(`/t/acme/sessions/s1`)

    // Inbox → issue: the inbox stays.
    expect(deriveOrigin(screenFromPath(`/t/acme/inbox`), inbox, {
      kind: `issue`,
      boardSlug: `other`,
    })).toEqual(inbox)
    // Inbox → a running session: the inbox stays.
    expect(deriveOrigin(issue, inbox, { kind: `session` })).toEqual(inbox)
    // Board → issue → Watch: the board stays.
    expect(deriveOrigin(issue, board, { kind: `session` })).toEqual(board)
    // The Agent page is a list context (the sessions column's own center).
    expect(
      deriveOrigin(screenFromPath(`/t/acme/agent`), { kind: `sessions` }, {
        kind: `session`,
      })
    ).toEqual({ kind: `sessions` })
    // Devices (context-free) → a session: the sessions list comes along.
    expect(deriveOrigin(screenFromPath(`/t/acme/devices`), inbox, {
      kind: `session`,
    })).toEqual({ kind: `sessions` })
    // Reviews → an issue: the issue's board comes along.
    expect(
      deriveOrigin(screenFromPath(`/t/acme/reviews`), inbox, {
        kind: `issue`,
        boardSlug: `web`,
      })
    ).toEqual(board)
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
    expect(capturedOrigin(screenFromPath(`/t/acme/agent`))).toEqual({
      kind: `sessions`,
    })
    // A detail hands on the origin it CARRIES…
    expect(
      capturedOrigin(screenFromPath(`/t/acme/sessions/s1`), inbox)
    ).toEqual(inbox)
    expect(
      capturedOrigin(screenFromPath(`/t/acme/boards/web/issues/MET-12`), inbox)
    ).toEqual(inbox)
    // …and falls back to what it IS.
    expect(capturedOrigin(screenFromPath(`/t/acme/sessions/s1`))).toEqual({
      kind: `sessions`,
    })
    expect(
      capturedOrigin(screenFromPath(`/t/acme/boards/web/issues/MET-12`))
    ).toEqual({ kind: `issue`, boardSlug: `web`, identifier: `MET-12` })
    // A full page carries nothing of its own.
    expect(capturedOrigin(screenFromPath(`/t/acme/devices`))).toBeNull()
    expect(capturedOrigin(screenFromPath(`/t/acme/devices`), board)).toEqual(
      board
    )
  })
})

describe(`formatOrigin / parseOrigin`, () => {
  it(`round-trips every origin, and the Agent list is the default`, () => {
    const origins: DetailOrigin[] = [
      inbox,
      board,
      { kind: `issue`, boardSlug: `web`, identifier: `MET-12` },
    ]
    for (const origin of origins) {
      expect(parseOrigin(formatOrigin(origin))).toEqual(origin)
    }
    // The sessions list is the session route's own default: no param.
    expect(formatOrigin({ kind: `sessions` })).toBeUndefined()
    expect(formatOrigin(null)).toBeUndefined()
    // …but an explicit token still parses.
    expect(parseOrigin(`sessions`)).toEqual({ kind: `sessions` })
    // Junk is no origin at all.
    expect(parseOrigin(``)).toBeNull()
    expect(parseOrigin(`board:`)).toBeNull()
    expect(parseOrigin(`nope`)).toBeNull()
    expect(parseOrigin(undefined)).toBeNull()
  })
})
