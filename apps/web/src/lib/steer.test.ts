import { describe, expect, it, vi } from "vitest"
import { verifySteerTicket } from "@exp/steer-ticket"
import {
  buildSteerTicketClaims,
  getSteerRelayConfig,
  mintSteerTicket,
  relayGetDevices,
  relayPostKill,
  relayPostStart,
  steerHttpBase,
  steerTicketUrl,
  steerWsBase,
  viewerPermFor,
  STEER_TICKET_TTL_SECONDS,
  type RelayFetch,
} from "@/lib/steer"

const NOW = 1_750_000_000
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
  it(`control: any user, empty ws scope, steer perm, deviceLabel passthrough`, () => {
    expect(
      buildSteerTicketClaims(
        { kind: `control`, userId: `user-1`, deviceLabel: `My MacBook` },
        NOW
      )
    ).toEqual({
      sub: `user-1`,
      ws: ``,
      role: `control`,
      perm: `steer`,
      deviceLabel: `My MacBook`,
      iat: NOW,
      exp: NOW + STEER_TICKET_TTL_SECONDS,
    })
  })

  it(`control: omits deviceLabel when not provided`, () => {
    const claims = buildSteerTicketClaims(
      { kind: `control`, userId: `user-1` },
      NOW
    )
    expect(claims).not.toHaveProperty(`deviceLabel`)
  })

  it(`publisher: workspace-scoped, session-bound, steer perm`, () => {
    expect(
      buildSteerTicketClaims(
        {
          kind: `publisher`,
          userId: `user-1`,
          workspaceId: `ws-1`,
          sessionId: `session-1`,
        },
        NOW
      )
    ).toEqual({
      sub: `user-1`,
      ws: `ws-1`,
      sessionId: `session-1`,
      role: `publisher`,
      perm: `steer`,
      iat: NOW,
      exp: NOW + STEER_TICKET_TTL_SECONDS,
    })
  })

  it(`viewer: workspace owner may steer, plain member only views`, () => {
    const owner = buildSteerTicketClaims(
      {
        kind: `viewer`,
        userId: `user-1`,
        workspaceId: `ws-1`,
        sessionId: `session-1`,
        role: `owner`,
        name: `Dana`,
      },
      NOW
    )
    expect(owner).toEqual({
      sub: `user-1`,
      ws: `ws-1`,
      sessionId: `session-1`,
      name: `Dana`,
      role: `viewer`,
      perm: `steer`,
      iat: NOW,
      exp: NOW + STEER_TICKET_TTL_SECONDS,
    })

    const member = buildSteerTicketClaims(
      {
        kind: `viewer`,
        userId: `user-2`,
        workspaceId: `ws-1`,
        sessionId: `session-1`,
        role: `member`,
        name: `member@example.com`,
      },
      NOW
    )
    expect(member.perm).toBe(`view`)
    expect(member.name).toBe(`member@example.com`)
  })

  it(`maps roles to perms (owner|member only — there is no admin role)`, () => {
    expect(viewerPermFor(`owner`)).toBe(`steer`)
    expect(viewerPermFor(`member`)).toBe(`view`)
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
      {
        kind: `viewer`,
        userId: `user-1`,
        workspaceId: `ws-1`,
        sessionId: `session-1`,
        role: `owner`,
        name: `Dana`,
      },
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
      ws: `ws-1`,
      sessionId: `session-1`,
      role: `viewer`,
      perm: `steer`,
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
  it(`myDevices passes through the relay's device list with the shared secret`, async () => {
    const devices = [
      { deviceId: `dev-1`, deviceLabel: `My MacBook`, connectedAt: 123 },
    ]
    const fetchImpl = vi
      .fn<RelayFetch>()
      .mockResolvedValue(fakeResponse(200, { devices }))

    await expect(
      relayGetDevices(CONFIG, `user 1`, fetchImpl)
    ).resolves.toEqual({ devices })
    expect(fetchImpl).toHaveBeenCalledWith(
      `https://steer.example.com/devices/user%201`,
      { headers: { "x-relay-secret": `test-secret` } }
    )
  })

  it(`throws on a non-ok devices response`, async () => {
    const fetchImpl = vi
      .fn<RelayFetch>()
      .mockResolvedValue(fakeResponse(401, { error: `Unauthorized` }))
    await expect(relayGetDevices(CONFIG, `user-1`, fetchImpl)).rejects.toThrow(
      `Steer relay /devices failed (401)`
    )
  })

  it(`posts /start with the secret and reports success`, async () => {
    const fetchImpl = vi
      .fn<RelayFetch>()
      .mockResolvedValue(fakeResponse(200, { ok: true }))

    await expect(
      relayPostStart(
        CONFIG,
        { userId: `user-1`, deviceId: `dev-1`, issueId: `issue-1` },
        fetchImpl
      )
    ).resolves.toEqual({ ok: true })
    expect(fetchImpl).toHaveBeenCalledWith(`https://steer.example.com/start`, {
      method: `POST`,
      headers: {
        "content-type": `application/json`,
        "x-relay-secret": `test-secret`,
      },
      body: JSON.stringify({
        userId: `user-1`,
        deviceId: `dev-1`,
        issueId: `issue-1`,
      }),
    })
  })

  it(`surfaces the relay reason on 404 (device offline)`, async () => {
    const fetchImpl = vi
      .fn<RelayFetch>()
      .mockResolvedValue(fakeResponse(404, { error: `device_offline` }))
    await expect(
      relayPostStart(
        CONFIG,
        { userId: `user-1`, deviceId: `dev-1`, issueId: `issue-1` },
        fetchImpl
      )
    ).resolves.toEqual({ ok: false, status: 404, reason: `device_offline` })
  })

  it(`falls back to a generic reason when the relay body is not JSON`, async () => {
    const fetchImpl = vi.fn<RelayFetch>().mockResolvedValue({
      ok: false,
      status: 500,
      json: async () => {
        throw new Error(`not json`)
      },
    })
    await expect(
      relayPostStart(
        CONFIG,
        { userId: `user-1`, deviceId: `dev-1`, issueId: `issue-1` },
        fetchImpl
      )
    ).resolves.toEqual({ ok: false, status: 500, reason: `relay_error` })
  })

  it(`kill is best-effort: reports delivery and never throws`, async () => {
    const okFetch = vi
      .fn<RelayFetch>()
      .mockResolvedValue(fakeResponse(200, { ok: true, delivered: true }))
    await expect(
      relayPostKill(CONFIG, `session-1`, okFetch)
    ).resolves.toEqual({ delivered: true })
    expect(okFetch).toHaveBeenCalledWith(
      `https://steer.example.com/sessions/session-1/kill`,
      { method: `POST`, headers: { "x-relay-secret": `test-secret` } }
    )

    const downFetch = vi
      .fn<RelayFetch>()
      .mockRejectedValue(new Error(`ECONNREFUSED`))
    await expect(
      relayPostKill(CONFIG, `session-1`, downFetch)
    ).resolves.toEqual({ delivered: false })
  })
})
