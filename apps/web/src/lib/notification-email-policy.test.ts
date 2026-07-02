import { describe, expect, it } from "vitest"
import {
  buildIssueDeepLinkPath,
  buildUnsubscribeUrl,
  defaultEmailPrefs,
  emailTypeAllowed,
  isResolutionStatus,
  shouldSendImmediateEmail,
  shouldSendReporterResolution,
} from "@/lib/notification-email-policy"

describe(`emailTypeAllowed`, () => {
  it(`defaults to allowed when no prefs row exists (missing row = defaults)`, () => {
    expect(emailTypeAllowed(null, `issue_comment`)).toBe(true)
    expect(emailTypeAllowed(undefined, `pr_merged`)).toBe(true)
  })

  it(`defaults every type to on with a fresh prefs row`, () => {
    const prefs = defaultEmailPrefs()
    for (const type of [
      `issue_assigned`,
      `issue_comment`,
      `issue_status_changed`,
      `issue_mention`,
      `pr_opened`,
      `pr_merged`,
    ] as const) {
      expect(emailTypeAllowed(prefs, type)).toBe(true)
    }
  })

  it(`the master emailEnabled switch blocks all types`, () => {
    const prefs = { ...defaultEmailPrefs(), emailEnabled: false }
    expect(emailTypeAllowed(prefs, `issue_assigned`)).toBe(false)
    expect(emailTypeAllowed(prefs, `pr_merged`)).toBe(false)
  })

  it(`per-type opt-out blocks only that type`, () => {
    const prefs = {
      ...defaultEmailPrefs(),
      typePrefs: { issue_comment: false as const },
    }
    expect(emailTypeAllowed(prefs, `issue_comment`)).toBe(false)
    expect(emailTypeAllowed(prefs, `issue_mention`)).toBe(true)
    expect(emailTypeAllowed(prefs, `pr_opened`)).toBe(true)
  })

  it(`an explicit true in typePrefs stays on`, () => {
    const prefs = {
      ...defaultEmailPrefs(),
      typePrefs: { issue_assigned: true as const },
    }
    expect(emailTypeAllowed(prefs, `issue_assigned`)).toBe(true)
  })
})

describe(`shouldSendImmediateEmail`, () => {
  it(`sends immediately with defaults (missing row or digest off)`, () => {
    expect(shouldSendImmediateEmail(null, `issue_assigned`)).toBe(true)
    expect(shouldSendImmediateEmail(defaultEmailPrefs(), `issue_assigned`)).toBe(
      true
    )
  })

  it(`digest != off skips the immediate send but keeps the type allowed`, () => {
    const prefs = { ...defaultEmailPrefs(), digest: `daily` }
    expect(shouldSendImmediateEmail(prefs, `issue_comment`)).toBe(false)
    // …the rows are left for the future digest cron, not blocked outright:
    expect(emailTypeAllowed(prefs, `issue_comment`)).toBe(true)
  })

  it(`respects the master switch and per-type opt-outs`, () => {
    expect(
      shouldSendImmediateEmail(
        { ...defaultEmailPrefs(), emailEnabled: false },
        `pr_merged`
      )
    ).toBe(false)
    expect(
      shouldSendImmediateEmail(
        { ...defaultEmailPrefs(), typePrefs: { pr_merged: false } },
        `pr_merged`
      )
    ).toBe(false)
  })
})

describe(`reporter resolution guards`, () => {
  it(`only done/cancelled count as resolution statuses`, () => {
    expect(isResolutionStatus(`done`)).toBe(true)
    expect(isResolutionStatus(`cancelled`)).toBe(true)
    expect(isResolutionStatus(`backlog`)).toBe(false)
    expect(isResolutionStatus(`todo`)).toBe(false)
    expect(isResolutionStatus(`in_progress`)).toBe(false)
    expect(isResolutionStatus(`duplicate`)).toBe(false)
  })

  it(`sends on first close`, () => {
    expect(
      shouldSendReporterResolution({
        toStatus: `done`,
        resolvedNotifiedAt: null,
      })
    ).toBe(true)
    expect(
      shouldSendReporterResolution({
        toStatus: `cancelled`,
        resolvedNotifiedAt: undefined,
      })
    ).toBe(true)
  })

  it(`is exactly-once: reopen→re-close does NOT re-email (flag stays set)`, () => {
    expect(
      shouldSendReporterResolution({
        toStatus: `done`,
        resolvedNotifiedAt: new Date(`2026-01-01T00:00:00Z`),
      })
    ).toBe(false)
  })

  it(`never sends on non-closing transitions`, () => {
    expect(
      shouldSendReporterResolution({
        toStatus: `in_progress`,
        resolvedNotifiedAt: null,
      })
    ).toBe(false)
  })
})

describe(`url builders`, () => {
  it(`builds the unsubscribe URL with an encoded token`, () => {
    expect(buildUnsubscribeUrl(`https://app.example.com`, `tok en/1`)).toBe(
      `https://app.example.com/api/email/unsubscribe?token=tok%20en%2F1`
    )
  })

  it(`tolerates a trailing slash on the base URL`, () => {
    expect(buildUnsubscribeUrl(`https://app.example.com/`, `abc`)).toBe(
      `https://app.example.com/api/email/unsubscribe?token=abc`
    )
  })

  it(`builds the issue deep link shared by push and email`, () => {
    expect(
      buildIssueDeepLinkPath({
        workspaceSlug: `metric`,
        projectSlug: `web`,
        identifier: `MET-12`,
      })
    ).toBe(`/w/metric/projects/web/issues/MET-12`)
  })
})
