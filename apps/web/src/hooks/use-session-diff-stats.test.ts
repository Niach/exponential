import { describe, expect, it } from "vitest"
import {
  MERGED_DIFF_WINDOW_MS,
  shouldConnectSessionDiff,
} from "@/hooks/use-session-diff-stats"

// EXP-875: dialing the relay is not free — it mints a ticket and, with no live
// room left, asks the device to republish its journal. The issue page wants
// `+N −M`, so it may only dial while that number is still a live thing.

const now = new Date(`2026-09-17T12:00:00.000Z`)
const minutesAgo = (n: number) =>
  new Date(now.getTime() - n * 60_000).toISOString()

function session(
  overrides: Partial<{
    status: `running` | `in_review` | `ended`
    prState: `open` | `merged` | `closed` | null
    endedAt: string | null
    updatedAt: string
  }> = {}
) {
  return {
    status: `ended` as const,
    prState: null,
    endedAt: minutesAgo(60),
    updatedAt: minutesAgo(60),
    ...overrides,
  }
}

describe(`shouldConnectSessionDiff`, () => {
  it(`dials for a live run`, () => {
    for (const status of [`running`, `in_review`] as const) {
      expect(
        shouldConnectSessionDiff({
          session: session({ status }),
          steerEnabled: true,
          now,
        })
      ).toBe(true)
    }
  })

  it(`does not dial for an ended run with nothing open`, () => {
    expect(
      shouldConnectSessionDiff({ session: session(), steerEnabled: true, now })
    ).toBe(false)
  })

  it(`dials while the PR is still open — the run's own or the issue's`, () => {
    expect(
      shouldConnectSessionDiff({
        session: session({ prState: `open` }),
        steerEnabled: true,
        now,
      })
    ).toBe(true)
    expect(
      shouldConnectSessionDiff({
        session: session(),
        issuePrState: `open`,
        steerEnabled: true,
        now,
      })
    ).toBe(true)
  })

  it(`dials just after a merge, and stops once the window passes`, () => {
    // EXP-889: the reader who merged keeps the counts on the issue page.
    expect(
      shouldConnectSessionDiff({
        session: session({ prState: `merged`, endedAt: minutesAgo(1) }),
        steerEnabled: true,
        now,
      })
    ).toBe(true)
    expect(
      shouldConnectSessionDiff({
        session: session({
          prState: `merged`,
          endedAt: new Date(
            now.getTime() - MERGED_DIFF_WINDOW_MS - 1_000
          ).toISOString(),
        }),
        steerEnabled: true,
        now,
      })
    ).toBe(false)
  })

  it(`falls back to updatedAt when the row carries no endedAt`, () => {
    expect(
      shouldConnectSessionDiff({
        session: session({
          prState: `merged`,
          endedAt: null,
          updatedAt: minutesAgo(2),
        }),
        steerEnabled: true,
        now,
      })
    ).toBe(true)
  })

  it(`never dials with steering off, or with no run`, () => {
    expect(
      shouldConnectSessionDiff({
        session: session({ status: `running` }),
        steerEnabled: false,
        now,
      })
    ).toBe(false)
    expect(
      shouldConnectSessionDiff({ session: null, steerEnabled: true, now })
    ).toBe(false)
  })
})
