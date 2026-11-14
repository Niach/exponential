import { beforeEach, describe, expect, it, vi } from "vitest"

// FEED-159: the widget reporter's resolution email carries the issue TITLE,
// member-editable text. Its `@<email>` mentions read "@Support" for the
// reporter like every other reporter egress (reporterFacingText), except a
// mention of the reporter's own address.

const h = vi.hoisted(() => ({
  selectQueue: [] as unknown[][],
  sent: [] as Array<{ to: string; issueTitle: string }>,
  claims: 0,
}))

vi.mock(`@/db/connection`, () => {
  const builder = (result: unknown[]) => {
    const b = {
      from: () => b,
      innerJoin: () => b,
      leftJoin: () => b,
      where: () => b,
      orderBy: () => b,
      limit: async () => result,
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      then: (res: any, rej: any) => Promise.resolve(result).then(res, rej),
    }
    return b
  }
  return {
    db: {
      select: vi.fn(() => builder(h.selectQueue.shift() ?? [])),
      update: vi.fn(() => ({
        set: () => {
          h.claims += 1
          return { where: () => ({ returning: async () => [{ id: `sub-1` }] }) }
        },
      })),
      insert: vi.fn(() => ({ values: async () => undefined })),
    },
  }
})

vi.mock(`@/lib/email-enabled`, () => ({ emailEnabled: true }))
vi.mock(`@/lib/email`, () => ({
  deliveryStatus: () => `sent`,
  sendReporterResolutionEmail: vi.fn(async (args: { to: string; issueTitle: string }) => {
    h.sent.push(args)
    return { delivered: true, provider: `test`, messageId: `m`, subject: `s` }
  }),
}))
vi.mock(`@/lib/integrations/fcm`, () => ({ sendToUsers: vi.fn() }))
vi.mock(`@/lib/sessions/host`, () => ({ resolveSessionHost: vi.fn() }))

import { fireAndForgetReporterResolution } from "@/lib/integrations/notifications"

beforeEach(() => {
  h.selectQueue = []
  h.sent = []
  h.claims = 0
})

describe(`fireAndForgetReporterResolution (FEED-159)`, () => {
  it(`hides member mentions in the title from each reporter, keeping their own`, async () => {
    h.selectQueue = [
      [{ id: `sub-1`, resolvedNotifiedAt: null }],
      [{ email: `reporter@example.com` }, { email: `other@example.com` }],
      [{ id: `i-1`, identifier: `EXP-1`, title: `Crash for @alice@acme.dev and @reporter@example.com` }],
    ]
    fireAndForgetReporterResolution({ issueId: `i-1`, toStatus: `done` })
    await vi.waitFor(() => expect(h.sent).toHaveLength(2))
    const byTo = Object.fromEntries(h.sent.map((sent) => [sent.to, sent.issueTitle]))
    expect(byTo[`reporter@example.com`]).toBe(`Crash for @Support and @reporter@example.com`)
    expect(byTo[`other@example.com`]).toBe(`Crash for @Support and @Support`)
  })

  it(`FEED-217: a widget with the close mail off sends nothing and claims nothing`, async () => {
    h.selectQueue = [
      [{ id: `sub-1`, resolvedNotifiedAt: null, notifyReporterOnClose: false }],
      [{ email: `reporter@example.com` }],
      [{ id: `i-1`, identifier: `EXP-1`, title: `Crash` }],
    ]
    fireAndForgetReporterResolution({ issueId: `i-1`, toStatus: `done` })
    // The off path returns after the first select: the queue keeps the rest.
    await vi.waitFor(() => expect(h.selectQueue).toHaveLength(2))
    await new Promise((resolve) => setTimeout(resolve, 20))
    expect(h.sent).toHaveLength(0)
    expect(h.claims).toBe(0)
  })

  it(`FEED-217: a deleted widget (null setting) still mails`, async () => {
    h.selectQueue = [
      [{ id: `sub-1`, resolvedNotifiedAt: null, notifyReporterOnClose: null }],
      [{ email: `reporter@example.com` }],
      [{ id: `i-1`, identifier: `EXP-1`, title: `Crash` }],
    ]
    fireAndForgetReporterResolution({ issueId: `i-1`, toStatus: `done` })
    await vi.waitFor(() => expect(h.sent).toHaveLength(1))
  })
})
