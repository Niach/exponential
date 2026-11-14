import { describe, expect, it, vi } from "vitest"

vi.mock(`@/db/connection`, () => ({ db: {} }))

import {
  APP_SCREENSHOT_CAP,
  APP_SCREENSHOT_COMMAND,
  screenshotBundle,
  screenshotDeviceVerdict,
  screenshotOutcome,
  takeAppScreenshot,
  type AppScreenshotDeps,
} from "./screenshot"

const NOW = new Date(`2026-11-13T12:00:00Z`)
const resolved = {
  app: { id: `app-1`, teamId: `team-1`, slug: `groceries` },
  pkg: { id: `groceries`, templates: { home: { components: [] } } } as never,
  templateId: `home`,
  data: { title: `Milk` },
}
const row = { id: `row-1`, name: `mint`, caps: [APP_SCREENSHOT_CAP], lastSeenAt: new Date(NOW.getTime() - 5_000) }

describe(`app screenshot (#FEED-244)`, () => {
  it(`bundles the resolved screen for exponential-shot`, () => {
    expect(screenshotBundle(resolved, `phone`)).toEqual({
      schema: 1,
      package: resolved.pkg,
      templateId: `home`,
      data: { title: `Milk` },
      width: 390,
      height: 844,
      mode: `dark`,
    })
    expect(screenshotBundle(resolved, `desktop`)).toMatchObject({ width: 1280, height: 800 })
  })

  it(`targets only a registered, capable, online device`, () => {
    expect(screenshotDeviceVerdict(row, `dev-1`, `host-1`, NOW)).toEqual({ deviceRowId: `row-1`, deviceId: `dev-1`, hostUserId: `host-1`, name: `mint` })
    expect(() => screenshotDeviceVerdict(null, `dev-1`, `host-1`, NOW)).toThrow(/not registered/)
    expect(() => screenshotDeviceVerdict({ ...row, caps: [`update-check`] }, `dev-1`, `host-1`, NOW)).toThrow(/mint cannot paint app screenshots: update Exponential/)
    expect(() => screenshotDeviceVerdict({ ...row, lastSeenAt: new Date(NOW.getTime() - 10 * 60_000) }, `dev-1`, `host-1`, NOW)).toThrow(/mint is offline/)
  })

  it(`turns the device's answer into the attachment or its refusal`, () => {
    expect(() => screenshotOutcome({ status: `done`, result: `{"width":390}` }, `mint`)).not.toThrow()
    expect(() => screenshotOutcome(null, `mint`)).toThrow(/did not answer within 60s/)
    expect(() => screenshotOutcome({ status: `failed`, result: `this device has no GPU renderer` }, `mint`)).toThrow(
      `mint could not paint the screen: this device has no GPU renderer`
    )
  })

  function deps(over: Partial<AppScreenshotDeps> = {}) {
    const calls: Record<string, unknown[]> = { queue: [], gate: [], finalize: [] }
    const d: AppScreenshotDeps = {
      device: async () => ({ deviceRowId: `row-1`, deviceId: `dev-1`, hostUserId: `host-1`, name: `mint` }),
      gate: (async (args: unknown) => void calls.gate.push(args)) as AppScreenshotDeps[`gate`],
      mint: async () => ({ attachmentId: `att-1`, uploadUrl: `https://x/api/attachment-uploads/t`, expiresAt: ``, curl: `` }),
      queue: async (_db, args) => {
        calls.queue.push(args)
        return { id: `cmd-1` }
      },
      wait: async () => ({ status: `done`, result: `{}` }),
      finalize: async (args) => {
        calls.finalize.push(args)
        return { id: `att-1`, url: `/api/attachments/att-1`, filename: `groceries-home-phone.png`, contentType: `image/png`, sizeBytes: 9, width: 390, height: 844, durationMs: null }
      },
      ...over,
    }
    return { d, calls }
  }
  const input = { userId: `user-1`, access: {} as never, sessionId: `s-1`, origin: `https://x`, resolve: async () => resolved }

  it(`queues the command on the session's device as its host and finalizes as the caller`, async () => {
    const { d, calls } = deps()
    const result = await takeAppScreenshot(input, d)
    expect(result.id).toBe(`att-1`)
    const queued = calls.queue[0] as { kind: string; userId: string; deviceRowId: string; payload: { bundle: string; uploadUrl: string } }
    expect(queued).toMatchObject({ kind: APP_SCREENSHOT_COMMAND, userId: `host-1`, deviceRowId: `row-1` })
    expect(JSON.parse(queued.payload.bundle)).toMatchObject({ templateId: `home`, width: 390 })
    expect(queued.payload.uploadUrl).toBe(`https://x/api/attachment-uploads/t`)
    expect(calls.gate[0]).toMatchObject({ teamId: `team-1`, userId: `user-1` })
    expect(calls.finalize[0]).toMatchObject({ attachmentId: `att-1`, userId: `user-1` })
  })

  it(`resolves nothing when the device refuses, and surfaces a failed paint`, async () => {
    let resolvedCalls = 0
    const refused = deps({ device: async () => Promise.reject(new Error(`mint is offline, so it cannot paint a screenshot.`)) })
    await expect(takeAppScreenshot({ ...input, resolve: async () => (resolvedCalls++, resolved) }, refused.d)).rejects.toThrow(/offline/)
    expect(resolvedCalls).toBe(0)
    const failed = deps({ wait: async () => ({ status: `failed`, result: `the screen did not open` }) })
    await expect(takeAppScreenshot(input, failed.d)).rejects.toThrow(`mint could not paint the screen: the screen did not open`)
    expect(failed.calls.finalize).toHaveLength(0)
  })
})
