import { describe, expect, it, vi } from "vitest"
import type { RelayCompactOutcome } from "@/lib/steer"

// The default deps read the DB; every test here injects its own.
vi.mock(`@/db/connection`, () => ({ db: {} }))

import {
  describeCompactDeliveryFailure,
  requestSessionCompaction,
  sessionsCompactRefusals,
  type SessionsCompactDeps,
  type SessionsCompactRow,
} from "./sessions-compact"

// EXP-936: the server's half of `exponential_sessions_compact` — ownership,
// the agent gate, and the relay hop whose verdict IS the tool result. The
// device-side verdicts (`too_early`, `cooldown`) are the engine's
// (crates/engine `compaction.rs`); here they only have to survive the hop.

const ROW: SessionsCompactRow = {
  id: `sess-1`,
  userId: `owner`,
  hostUserId: null,
  deviceId: `dev-1`,
  status: `running`,
  agent: `claude`,
}

function deps(
  over: Partial<SessionsCompactDeps> & {
    row?: SessionsCompactRow | null
    outcome?: RelayCompactOutcome | null
  } = {}
) {
  const { row = ROW, outcome = { delivered: true, accepted: true }, ...rest } =
    over
  const built: SessionsCompactDeps = {
    loadSession: vi.fn(async () => row),
    relay: vi.fn(async () => outcome),
    log: vi.fn(),
    ...rest,
  }
  return built
}

describe(`requestSessionCompaction (EXP-936)`, () => {
  it(`names the four refusal codes the contract declared`, () => {
    expect([...sessionsCompactRefusals]).toEqual([
      `too_early`,
      `cooldown`,
      `not_own_session`,
      `unsupported_agent`,
    ])
  })

  it(`relays the caller's own run with keep and returns the host's acceptance`, async () => {
    const d = deps()
    await expect(
      requestSessionCompaction(
        { sessionId: `sess-1`, userId: `owner`, reason: `long`, keep: `open threads` },
        d
      )
    ).resolves.toEqual({ accepted: true })
    // VAPP-43: the ask goes down the session's device, under its account.
    expect(d.relay).toHaveBeenCalledWith(
      `sess-1`,
      { userId: `owner`, deviceId: `dev-1` },
      `open threads`
    )
    // The reason is logged on the server; the feed shows it as the tool
    // card's subject.
    expect(d.log).toHaveBeenCalledWith(expect.stringContaining(`long`))
  })

  it(`the host may ask for the run it hosts, a stranger may not`, async () => {
    const shared = deps({ row: { ...ROW, hostUserId: `host` } })
    await expect(
      requestSessionCompaction(
        { sessionId: `sess-1`, userId: `host`, reason: `r` },
        shared
      )
    ).resolves.toEqual({ accepted: true })
    // A hosted run's device is registered under its HOST.
    expect(shared.relay).toHaveBeenCalledWith(
      `sess-1`,
      { userId: `host`, deviceId: `dev-1` },
      undefined
    )

    const stranger = deps()
    await expect(
      requestSessionCompaction(
        { sessionId: `sess-1`, userId: `someone-else`, reason: `r` },
        stranger
      )
    ).resolves.toEqual({ accepted: false, refusedBecause: `not_own_session` })
    expect(stranger.relay).not.toHaveBeenCalled()
  })

  it(`refuses an agent the contract does not name, without asking the host`, async () => {
    for (const agent of [null, `pi`, `external`]) {
      const d = deps({ row: { ...ROW, agent } })
      await expect(
        requestSessionCompaction(
          { sessionId: `sess-1`, userId: `owner`, reason: `r` },
          d
        )
      ).resolves.toEqual({ accepted: false, refusedBecause: `unsupported_agent` })
      expect(d.relay).not.toHaveBeenCalled()
    }
    for (const agent of [`claude`, `codex`]) {
      const d = deps({ row: { ...ROW, agent } })
      await expect(
        requestSessionCompaction(
          { sessionId: `sess-1`, userId: `owner`, reason: `r` },
          d
        )
      ).resolves.toEqual({ accepted: true })
    }
  })

  it(`carries the host's refusal back as the verdict`, async () => {
    for (const code of [`too_early`, `cooldown`] as const) {
      const d = deps({
        outcome: { delivered: true, accepted: false, refusedBecause: code },
      })
      await expect(
        requestSessionCompaction(
          { sessionId: `sess-1`, userId: `owner`, reason: `r` },
          d
        )
      ).resolves.toEqual({ accepted: false, refusedBecause: code })
    }
  })

  it(`an unknown run, an ended run, no relay and a missing host are errors, not refusals`, async () => {
    const ask = { sessionId: `sess-1`, userId: `owner`, reason: `r` }
    await expect(
      requestSessionCompaction(ask, deps({ row: null }))
    ).rejects.toThrow(`Session not found`)
    await expect(
      requestSessionCompaction(ask, deps({ row: { ...ROW, status: `ended` } }))
    ).rejects.toThrow(`has ended`)
    await expect(
      requestSessionCompaction(ask, deps({ outcome: null }))
    ).rejects.toThrow(`STEER_RELAY_URL`)
    for (const reason of [`device_offline`, `no_verdict`, `busy`, `relay_error`] as const) {
      await expect(
        requestSessionCompaction(ask, deps({ outcome: { delivered: false, reason } }))
      ).rejects.toThrow(describeCompactDeliveryFailure(reason))
    }
    // A row naming no device is offline without a relay hop.
    const noDevice = deps({ row: { ...ROW, deviceId: `` } })
    await expect(requestSessionCompaction(ask, noDevice)).rejects.toThrow(
      describeCompactDeliveryFailure(`device_offline`)
    )
    expect(noDevice.relay).not.toHaveBeenCalled()
    expect(describeCompactDeliveryFailure(`device_offline`)).toContain(`device is not connected`)
    // The no-verdict text points at the fix: the host predates the frame.
    expect(describeCompactDeliveryFailure(`no_verdict`)).toContain(`Update`)
  })
})
