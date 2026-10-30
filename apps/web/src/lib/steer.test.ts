import { describe, expect, it, vi } from "vitest"
import { verifySteerTicket } from "@exp/steer-ticket"
import {
  buildSteerTicketClaims,
  getSteerRelayConfig,
  mintSteerTicket,
  relayPostCompact,
  relayPostKill,
  relayPostNudge,
  relayPostInterrupt,
  steerHttpBase,
  steerTicketUrl,
  steerWsBase,
  STEER_TICKET_TTL_SECONDS,
  type RelayFetch,
} from "@/lib/steer"

const NOW = 1_750_000_000
const DEVICE = { userId: `host-1`, deviceId: `dev-1` }
const DEVICE_BODY = JSON.stringify(DEVICE)
const CONFIG = { url: `https://steer.example.com`, secret: `test-secret` }

function fakeResponse(status: number, body: unknown) {
  return {
    ok: status >= 200 && status < 300,
    status,
    json: async () => body,
  }
}

describe(`getSteerRelayConfig`, () => {
  it(`is enabled only when BOTH url and secret are set`, () => {
    expect(
      getSteerRelayConfig({
        STEER_RELAY_URL: `https://steer.example.com`,
        STEER_RELAY_SECRET: `s`,
      })
    ).toEqual({ url: `https://steer.example.com`, secret: `s` })
    expect(
      getSteerRelayConfig({ STEER_RELAY_URL: `https://steer.example.com` })
    ).toBeNull()
    expect(getSteerRelayConfig({ STEER_RELAY_SECRET: `s` })).toBeNull()
    expect(getSteerRelayConfig({})).toBeNull()
  })

  it(`treats empty/whitespace values as unset`, () => {
    expect(
      getSteerRelayConfig({ STEER_RELAY_URL: `  `, STEER_RELAY_SECRET: `s` })
    ).toBeNull()
    expect(
      getSteerRelayConfig({
        STEER_RELAY_URL: `https://steer.example.com`,
        STEER_RELAY_SECRET: ``,
      })
    ).toBeNull()
  })

  // EXP-504: the selfhost compose network address for the web app's own
  // server-to-server calls — optional, blank = unset (derive from the url).
  it(`carries STEER_RELAY_INTERNAL_URL through when set`, () => {
    expect(
      getSteerRelayConfig({
        STEER_RELAY_URL: `wss://issues.example.com/steer`,
        STEER_RELAY_SECRET: `s`,
        STEER_RELAY_INTERNAL_URL: `http://steer-relay:4002`,
      })
    ).toEqual({
      url: `wss://issues.example.com/steer`,
      secret: `s`,
      internalUrl: `http://steer-relay:4002`,
    })
    expect(
      getSteerRelayConfig({
        STEER_RELAY_URL: `wss://issues.example.com/steer`,
        STEER_RELAY_SECRET: `s`,
        STEER_RELAY_INTERNAL_URL: `  `,
      })
    ).toEqual({ url: `wss://issues.example.com/steer`, secret: `s` })
  })
})

describe(`relay URL derivation`, () => {
  it(`translates http(s) to ws(s) for the socket base`, () => {
    expect(steerWsBase(`http://localhost:4002`)).toBe(`ws://localhost:4002`)
    expect(steerWsBase(`https://steer.example.com`)).toBe(
      `wss://steer.example.com`
    )
  })

  it(`passes ws(s) through and strips trailing slashes`, () => {
    expect(steerWsBase(`wss://steer.example.com/`)).toBe(
      `wss://steer.example.com`
    )
    expect(steerWsBase(`ws://relay.lan:4002`)).toBe(`ws://relay.lan:4002`)
    expect(steerWsBase(`https://steer.example.com///`)).toBe(
      `wss://steer.example.com`
    )
  })

  it(`translates ws(s) to http(s) for the admin HTTP base`, () => {
    expect(steerHttpBase(`wss://steer.example.com`)).toBe(
      `https://steer.example.com`
    )
    expect(steerHttpBase(`ws://relay.lan:4002/`)).toBe(`http://relay.lan:4002`)
    expect(steerHttpBase(`https://steer.example.com`)).toBe(
      `https://steer.example.com`
    )
    expect(steerHttpBase(`http://localhost:4002`)).toBe(
      `http://localhost:4002`
    )
  })

  it(`builds the full dial URL with the ticket in the query string`, () => {
    expect(steerTicketUrl(`https://steer.example.com`, `abc.def`)).toBe(
      `wss://steer.example.com/ws?ticket=abc.def`
    )
    // Tickets are base64url + '.', which never needs escaping — but anything
    // unexpected must still be query-safe.
    expect(steerTicketUrl(`http://localhost:4002/`, `a+b`)).toBe(
      `ws://localhost:4002/ws?ticket=a%2Bb`
    )
  })
})

describe(`ticket claim composition`, () => {
  // EXP-710: control claims are the account, the empty team scope and the
  // window — no `deviceLabel` any more (the relay stopped reading presence
  // metadata in EXP-672, so minting it was dead weight on the wire).
  it(`control: any user, empty ws scope, no device metadata`, () => {
    const claims = buildSteerTicketClaims(
      { kind: `control`, userId: `user-1` },
      NOW
    )
    expect(claims).toEqual({
      sub: `user-1`,
      team: ``,
      role: `control`,
      iat: NOW,
      exp: NOW + STEER_TICKET_TTL_SECONDS,
    })
    expect(claims).not.toHaveProperty(`deviceLabel`)
  })

  // VAPP-43: the relay serves control and the call pair only.
  it(`there is no publisher or viewer seed any more`, () => {
    for (const kind of [`publisher`, `viewer`]) {
      const claims = buildSteerTicketClaims(
        { kind, userId: `user-1` } as unknown as Parameters<typeof buildSteerTicketClaims>[0],
        NOW
      )
      expect(claims?.role).not.toBe(kind)
    }
  })
})

describe(`mintSteerTicket`, () => {
  it(`returns disabled (a result, not an error) when the relay is not configured`, () => {
    expect(
      mintSteerTicket(null, { kind: `control`, userId: `user-1` })
    ).toEqual({ disabled: true })
  })

  it(`signs a ticket the relay can verify and returns the dial URL`, () => {
    const result = mintSteerTicket(
      CONFIG,
      { kind: `control`, userId: `user-1` },
      NOW
    )
    if (`disabled` in result) throw new Error(`expected a ticket`)
    expect(result.url).toBe(
      `wss://steer.example.com/ws?ticket=${encodeURIComponent(result.ticket)}`
    )

    // Round-trip through the relay's verify path (wire truth).
    const verdict = verifySteerTicket(result.ticket, CONFIG.secret, NOW)
    if (!verdict.ok) throw new Error(`expected a valid ticket`)
    expect(verdict.claims).toMatchObject({
      sub: `user-1`,
      team: ``,
      role: `control`,
      exp: NOW + STEER_TICKET_TTL_SECONDS,
    })
  })

  it(`rejects tampered tickets`, () => {
    const result = mintSteerTicket(
      CONFIG,
      { kind: `control`, userId: `user-1` },
      NOW
    )
    if (`disabled` in result) throw new Error(`expected a ticket`)
    const verdict = verifySteerTicket(result.ticket, `wrong-secret`, NOW)
    expect(verdict).toEqual({ ok: false, reason: `bad_signature` })
  })
})

describe(`relay admin HTTP`, () => {
  // EXP-504: with an internal URL configured, every server-to-server call
  // dials the compose-network address (no hairpin through public DNS) while
  // the ticket dial URL — what clients connect to — keeps the public url.
  it(`prefers the internal URL for server-to-server calls only`, async () => {
    const config = {
      url: `wss://issues.example.com/steer`,
      secret: `test-secret`,
      internalUrl: `http://steer-relay:4002`,
    }
    const fetchImpl = vi
      .fn<RelayFetch>()
      .mockResolvedValue(fakeResponse(200, { devices: [], delivered: true }))

    await relayPostInterrupt(config, `sess-1`, DEVICE, fetchImpl)
    await relayPostKill(config, `sess-1`, DEVICE, fetchImpl)
    await relayPostNudge(config, `user-1`, `dev-1`, fetchImpl)
    await relayPostCompact(config, `sess-1`, DEVICE, undefined, fetchImpl)
    expect(fetchImpl.mock.calls.map(([url]) => url)).toEqual([
      `http://steer-relay:4002/sessions/sess-1/interrupt`,
      `http://steer-relay:4002/sessions/sess-1/kill`,
      `http://steer-relay:4002/devices/user-1/dev-1/nudge`,
      `http://steer-relay:4002/sessions/sess-1/compact`,
    ])

    const minted = mintSteerTicket(config, { kind: `control`, userId: `u` })
    expect(`url` in minted && minted.url).toMatch(
      /^wss:\/\/issues\.example\.com\/steer\/ws\?ticket=/
    )
  })

  // VAPP-43: kill and interrupt name the session's DEVICE in the body; the
  // relay sends the frame down that device's control socket.
  it(`kill and interrupt post the device, report delivery and never throw`, async () => {
    for (const [post, route] of [
      [relayPostKill, `kill`],
      [relayPostInterrupt, `interrupt`],
    ] as const) {
      const okFetch = vi
        .fn<RelayFetch>()
        .mockResolvedValue(fakeResponse(200, { ok: true, delivered: true }))
      await expect(post(CONFIG, `session-1`, DEVICE, okFetch)).resolves.toEqual({
        delivered: true,
      })
      expect(okFetch).toHaveBeenCalledWith(
        `https://steer.example.com/sessions/session-1/${route}`,
        {
          method: `POST`,
          headers: {
            "content-type": `application/json`,
            "x-relay-secret": `test-secret`,
          },
          body: DEVICE_BODY,
          signal: expect.any(AbortSignal),
        }
      )

      const offlineFetch = vi
        .fn<RelayFetch>()
        .mockResolvedValue(fakeResponse(200, { ok: true, delivered: false }))
      await expect(post(CONFIG, `session-1`, DEVICE, offlineFetch)).resolves.toEqual({
        delivered: false,
      })
      const downFetch = vi
        .fn<RelayFetch>()
        .mockRejectedValue(new Error(`ECONNREFUSED`))
      await expect(post(CONFIG, `session-1`, DEVICE, downFetch)).resolves.toEqual({
        delivered: false,
      })
    }
  })

  // The PR-merge path awaits this fan-out inside the GitHub webhook handler,
  // which GitHub abandons after ~10s — a wedged relay must abort rather than
  // hold the response open. Assert the armed timeout signal reaches fetch, and
  // that the abort it eventually raises lands in the never-throws catch.
  it(`kill bounds a hung relay with an armed timeout signal`, async () => {
    const hungFetch = vi.fn<RelayFetch>().mockImplementation(
      (_url, init) =>
        new Promise((_resolve, reject) => {
          init?.signal?.addEventListener(`abort`, () =>
            reject(new DOMException(`The operation timed out.`, `TimeoutError`))
          )
        })
    )
    const call = relayPostKill(CONFIG, `session-1`, DEVICE, hungFetch)
    const signal = hungFetch.mock.calls[0]?.[1]?.signal
    expect(signal).toBeInstanceOf(AbortSignal)
    expect(signal?.aborted).toBe(false)
    await expect(call).resolves.toEqual({ delivered: false })
  })

  // EXP-481: the check-in nudge — same best-effort/never-throws contract as
  // kill, aimed at a device instead of a session.
  it(`nudge posts the device path, reports delivery and never throws`, async () => {
    const okFetch = vi
      .fn<RelayFetch>()
      .mockResolvedValue(fakeResponse(200, { ok: true, delivered: true }))
    await expect(
      relayPostNudge(CONFIG, `owner 1`, `dev/1`, okFetch)
    ).resolves.toEqual({ delivered: true })
    expect(okFetch).toHaveBeenCalledWith(
      `https://steer.example.com/devices/owner%201/dev%2F1/nudge`,
      {
        method: `POST`,
        headers: { "x-relay-secret": `test-secret` },
        signal: expect.any(AbortSignal),
      }
    )

    const offlineFetch = vi
      .fn<RelayFetch>()
      .mockResolvedValue(fakeResponse(200, { ok: true, delivered: false }))
    await expect(
      relayPostNudge(CONFIG, `owner`, `dev-1`, offlineFetch)
    ).resolves.toEqual({ delivered: false })

    const downFetch = vi
      .fn<RelayFetch>()
      .mockRejectedValue(new Error(`ECONNREFUSED`))
    await expect(
      relayPostNudge(CONFIG, `owner`, `dev-1`, downFetch)
    ).resolves.toEqual({ delivered: false })
  })

  // EXP-936: the compaction ask — awaited by the relay until the host
  // answers, so the outcome carries a verdict (or a named delivery failure)
  // and never throws.
  it(`compact posts keep, reads the host's verdict and never throws`, async () => {
    const accepted = vi
      .fn<RelayFetch>()
      .mockResolvedValue(
        fakeResponse(200, { ok: true, delivered: true, accepted: true })
      )
    await expect(
      relayPostCompact(CONFIG, `session-1`, DEVICE, `open threads`, accepted)
    ).resolves.toEqual({ delivered: true, accepted: true })
    expect(accepted).toHaveBeenCalledWith(
      `https://steer.example.com/sessions/session-1/compact`,
      {
        method: `POST`,
        headers: {
          "content-type": `application/json`,
          "x-relay-secret": `test-secret`,
        },
        body: JSON.stringify({ ...DEVICE, keep: `open threads` }),
        signal: expect.any(AbortSignal),
      }
    )

    // No keep → the device alone, never `keep: undefined` on the wire.
    const refused = vi
      .fn<RelayFetch>()
      .mockResolvedValue(
        fakeResponse(200, {
          ok: true,
          delivered: true,
          accepted: false,
          refusedBecause: `too_early`,
        })
      )
    await expect(
      relayPostCompact(CONFIG, `session-1`, DEVICE, undefined, refused)
    ).resolves.toEqual({
      delivered: true,
      accepted: false,
      refusedBecause: `too_early`,
    })
    expect(refused.mock.calls[0]?.[1]?.body).toBe(DEVICE_BODY)

    // The relay's delivery failures keep their reason; anything else (an
    // old relay's 404, a dead relay, an unknown code) is `relay_error`.
    for (const reason of [`device_offline`, `no_verdict`, `busy`] as const) {
      const undelivered = vi
        .fn<RelayFetch>()
        .mockResolvedValue(
          fakeResponse(200, { ok: true, delivered: false, reason })
        )
      await expect(
        relayPostCompact(CONFIG, `session-1`, DEVICE, `k`, undelivered)
      ).resolves.toEqual({ delivered: false, reason })
    }
    const oldRelayFetch = vi
      .fn<RelayFetch>()
      .mockResolvedValue(fakeResponse(404, { error: `Not found` }))
    await expect(
      relayPostCompact(CONFIG, `session-1`, DEVICE, `k`, oldRelayFetch)
    ).resolves.toEqual({ delivered: false, reason: `relay_error` })
    const downFetch = vi
      .fn<RelayFetch>()
      .mockRejectedValue(new Error(`ECONNREFUSED`))
    await expect(
      relayPostCompact(CONFIG, `session-1`, DEVICE, `k`, downFetch)
    ).resolves.toEqual({ delivered: false, reason: `relay_error` })
    const strangeCode = vi
      .fn<RelayFetch>()
      .mockResolvedValue(
        fakeResponse(200, {
          ok: true,
          delivered: true,
          accepted: false,
          refusedBecause: `because`,
        })
      )
    await expect(
      relayPostCompact(CONFIG, `session-1`, DEVICE, `k`, strangeCode)
    ).resolves.toEqual({
      delivered: true,
      accepted: false,
      refusedBecause: `cooldown`,
    })
  })

  it(`compact bounds a hung relay with an armed timeout signal`, async () => {
    const hungFetch = vi.fn<RelayFetch>().mockImplementation(
      (_url, init) =>
        new Promise((_resolve, reject) => {
          init?.signal?.addEventListener(`abort`, () =>
            reject(new DOMException(`The operation timed out.`, `TimeoutError`))
          )
        })
    )
    const call = relayPostCompact(CONFIG, `session-1`, DEVICE, `k`, hungFetch)
    const signal = hungFetch.mock.calls[0]?.[1]?.signal
    expect(signal).toBeInstanceOf(AbortSignal)
    // The armed timeout is 6s (above the relay's own 4s verdict wait).
    await expect(call).resolves.toEqual({
      delivered: false,
      reason: `relay_error`,
    })
  }, 10_000)
})
