import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"

// The gate reads only isCloudInstance + getTeamPlan/getPlanLimits; both
// modules are mocked so the tests run without env or Postgres (billing.ts
// pulls in the db connection at import time). The limit values mirror
// PLAN_LIMITS, which billing.test.ts locks.
const { cloud, teamPlanMock } = vi.hoisted(() => ({
  cloud: { value: true },
  teamPlanMock: vi.fn(),
}))

vi.mock(`@/lib/bootstrap-cloud`, () => ({
  isCloudInstance: () => cloud.value,
}))

vi.mock(`@/lib/billing`, () => ({
  getTeamPlan: teamPlanMock,
  getPlanLimits: (plan: string) => ({
    widgetSubmissionsPerHour: plan === `free` ? 60 : Infinity,
  }),
}))

import {
  resetWidgetSubmitLimitStateForTest,
  takeWidgetSubmitToken,
} from "./submit-limit"

const TEAM_A = `team-a`
const TEAM_B = `team-b`

function freePlan() {
  return { plan: `free`, limits: { widgetSubmissionsPerHour: 60 } }
}
function paidPlan() {
  return { plan: `team`, limits: { widgetSubmissionsPerHour: Infinity } }
}

beforeEach(() => {
  cloud.value = true
  teamPlanMock.mockReset()
  resetWidgetSubmitLimitStateForTest()
})

afterEach(() => {
  vi.useRealTimers()
})

describe(`self-hosted`, () => {
  it(`uses the env per-key bucket and never resolves a plan`, async () => {
    cloud.value = false
    // The per-key limiter is a module singleton without a reset, so this test
    // owns a unique publicKey; defaults are burst 10, refill 60/h.
    const config = { publicKey: `expw_selfhost`, teamId: TEAM_A }
    for (let i = 0; i < 10; i++) {
      await expect(takeWidgetSubmitToken(config, 0)).resolves.toEqual({
        ok: true,
      })
    }
    const eleventh = await takeWidgetSubmitToken(config, 0)
    expect(eleventh.ok).toBe(false)
    expect(teamPlanMock).not.toHaveBeenCalled()
  })

  it(`keys the bucket per widget, not per team`, async () => {
    cloud.value = false
    await takeWidgetSubmitToken(
      { publicKey: `expw_selfhost_drained`, teamId: TEAM_A },
      0
    )
    // A different widget of the same team has its own full bucket.
    await expect(
      takeWidgetSubmitToken(
        { publicKey: `expw_selfhost_fresh`, teamId: TEAM_A },
        0
      )
    ).resolves.toEqual({ ok: true })
  })
})

describe(`cloud, free plan`, () => {
  it(`allows the burst then 429s with a retry hint`, async () => {
    teamPlanMock.mockResolvedValue(freePlan())
    const config = { publicKey: `expw_a`, teamId: TEAM_A }
    for (let i = 0; i < 10; i++) {
      await expect(takeWidgetSubmitToken(config, 0)).resolves.toEqual({
        ok: true,
      })
    }
    const blocked = await takeWidgetSubmitToken(config, 0)
    expect(blocked.ok).toBe(false)
    if (!blocked.ok) expect(blocked.retryAfterSeconds).toBeGreaterThanOrEqual(1)
  })

  it(`refills at the plan rate (60/h = one token a minute)`, async () => {
    teamPlanMock.mockResolvedValue(freePlan())
    const config = { publicKey: `expw_a`, teamId: TEAM_A }
    for (let i = 0; i < 10; i++) await takeWidgetSubmitToken(config, 0)
    await expect(takeWidgetSubmitToken(config, 60_000)).resolves.toEqual({
      ok: true,
    })
  })

  it(`aggregates per TEAM across widgets, isolated between teams`, async () => {
    teamPlanMock.mockResolvedValue(freePlan())
    // Two widgets of team A drain ONE shared bucket…
    for (let i = 0; i < 5; i++) {
      await takeWidgetSubmitToken({ publicKey: `expw_a1`, teamId: TEAM_A }, 0)
      await takeWidgetSubmitToken({ publicKey: `expw_a2`, teamId: TEAM_A }, 0)
    }
    const blocked = await takeWidgetSubmitToken(
      { publicKey: `expw_a1`, teamId: TEAM_A },
      0
    )
    expect(blocked.ok).toBe(false)
    // …while team B is untouched.
    await expect(
      takeWidgetSubmitToken({ publicKey: `expw_b`, teamId: TEAM_B }, 0)
    ).resolves.toEqual({ ok: true })
  })
})

describe(`cloud, paid/comp plan`, () => {
  it(`is unlimited — no bucket at all`, async () => {
    teamPlanMock.mockResolvedValue(paidPlan())
    const config = { publicKey: `expw_paid`, teamId: TEAM_A }
    for (let i = 0; i < 100; i++) {
      await expect(takeWidgetSubmitToken(config, 0)).resolves.toEqual({
        ok: true,
      })
    }
  })
})

describe(`plan cache`, () => {
  it(`resolves the plan once within the TTL`, async () => {
    teamPlanMock.mockResolvedValue(paidPlan())
    const config = { publicKey: `expw_paid`, teamId: TEAM_A }
    for (let i = 0; i < 20; i++) await takeWidgetSubmitToken(config, 0)
    expect(teamPlanMock).toHaveBeenCalledTimes(1)
  })

  it(`re-resolves after the 30s TTL (up/downgrades apply within 30s)`, async () => {
    vi.useFakeTimers()
    teamPlanMock.mockResolvedValue(freePlan())
    const config = { publicKey: `expw_a`, teamId: TEAM_A }
    await takeWidgetSubmitToken(config, 0)
    vi.advanceTimersByTime(31_000)
    teamPlanMock.mockResolvedValue(paidPlan())
    for (let i = 0; i < 20; i++) {
      await expect(takeWidgetSubmitToken(config, 0)).resolves.toEqual({
        ok: true,
      })
    }
    expect(teamPlanMock).toHaveBeenCalledTimes(2)
  })

  it(`a rejected lookup is not cached`, async () => {
    teamPlanMock.mockRejectedValueOnce(new Error(`db down`))
    const config = { publicKey: `expw_a`, teamId: TEAM_A }
    await expect(takeWidgetSubmitToken(config, 0)).rejects.toThrow(`db down`)
    teamPlanMock.mockResolvedValue(paidPlan())
    await expect(takeWidgetSubmitToken(config, 0)).resolves.toEqual({
      ok: true,
    })
  })
})
