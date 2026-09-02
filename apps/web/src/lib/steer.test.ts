import { describe, expect, it, vi } from "vitest"
import { verifySteerTicket } from "@exp/steer-ticket"
import {
  buildSteerTicketClaims,
  getSteerRelayConfig,
  mintSteerTicket,
  relayPostInput,
  relayPostKill,
  relayPostNudge,
  relayPostStart,
  steerHttpBase,
  steerTicketUrl,
  steerWsBase,
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

  it(`publisher: team-scoped, session-bound`, () => {
    expect(
      buildSteerTicketClaims(
        {
          kind: `publisher`,
          userId: `user-1`,
          teamId: `ws-1`,
          sessionId: `session-1`,
        },
        NOW
      )
    ).toEqual({
      sub: `user-1`,
      team: `ws-1`,
      sessionId: `session-1`,
      role: `publisher`,
      iat: NOW,
      exp: NOW + STEER_TICKET_TTL_SECONDS,
    })
  })

  it(`viewer: session-bound (EXP-312 — owner-only at mint, no perm claim)`, () => {
    expect(
      buildSteerTicketClaims(
        {
          kind: `viewer`,
          userId: `user-1`,
          teamId: `ws-1`,
          sessionId: `session-1`,
        },
        NOW
      )
    ).toEqual({
      sub: `user-1`,
      team: `ws-1`,
      sessionId: `session-1`,
      role: `viewer`,
      iat: NOW,
      exp: NOW + STEER_TICKET_TTL_SECONDS,
    })
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
        teamId: `ws-1`,
        sessionId: `session-1`,
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
      team: `ws-1`,
      sessionId: `session-1`,
      role: `viewer`,
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

    await relayPostStart(
      config,
      { userId: `user-1`, deviceId: `dev-1`, issueId: `issue-1` },
      fetchImpl
    )
    await relayPostKill(config, `sess-1`, fetchImpl)
    await relayPostNudge(config, `user-1`, `dev-1`, fetchImpl)
    await relayPostInput(config, `sess-1`, `hello`, fetchImpl)
    expect(fetchImpl.mock.calls.map(([url]) => url)).toEqual([
      `http://steer-relay:4002/start`,
      `http://steer-relay:4002/sessions/sess-1/kill`,
      `http://steer-relay:4002/devices/user-1/dev-1/nudge`,
      `http://steer-relay:4002/sessions/sess-1/input`,
    ])

    const minted = mintSteerTicket(config, { kind: `control`, userId: `u` })
    expect(`url` in minted && minted.url).toMatch(
      /^wss:\/\/issues\.example\.com\/steer\/ws\?ticket=/
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
      // REV-34: bounded — a wedged relay must not hang startSession.
      signal: expect.any(AbortSignal),
    })
  })

  it(`passes launch options through to /start; undefineds never hit the wire`, async () => {
    const fetchImpl = vi
      .fn<RelayFetch>()
      .mockResolvedValue(fakeResponse(200, { ok: true }))

    await relayPostStart(
      CONFIG,
      {
        userId: `user-1`,
        deviceId: `dev-1`,
        issueId: `issue-1`,
        model: `opus`,
        effort: ``,
        ultracode: true,
        planMode: undefined,
      },
      fetchImpl
    )
    const body = JSON.parse(
      (fetchImpl.mock.calls[0][1] as { body: string }).body
    ) as Record<string, unknown>
    expect(body).toEqual({
      userId: `user-1`,
      deviceId: `dev-1`,
      issueId: `issue-1`,
      model: `opus`,
      effort: ``,
      ultracode: true,
    })
    expect(`planMode` in body).toBe(false)
  })

  it(`serializes a batch subject verbatim: issueIds/teamId/repo, no issueId/installationId`, async () => {
    const fetchImpl = vi
      .fn<RelayFetch>()
      .mockResolvedValue(fakeResponse(200, { ok: true }))

    await relayPostStart(
      CONFIG,
      {
        userId: `user-1`,
        deviceId: `dev-1`,
        issueIds: [`issue-1`, `issue-2`],
        teamId: `ws-1`,
        repo: {
          repositoryId: `repo-1`,
          fullName: `acme/api`,
          defaultBranch: `main`,
        },
        ultracode: true,
      },
      fetchImpl
    )
    const body = JSON.parse(
      (fetchImpl.mock.calls[0][1] as { body: string }).body
    ) as Record<string, unknown>
    expect(body).toEqual({
      userId: `user-1`,
      deviceId: `dev-1`,
      issueIds: [`issue-1`, `issue-2`],
      teamId: `ws-1`,
      repo: {
        repositoryId: `repo-1`,
        fullName: `acme/api`,
        defaultBranch: `main`,
      },
      ultracode: true,
    })
    // A batch body never carries the single-issue key, and the repo group
    // never carries the server-only installationId.
    expect(`issueId` in body).toBe(false)
    expect(`installationId` in (body.repo as Record<string, unknown>)).toBe(
      false
    )
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

  // REV-34: startSession awaits this inline — an accepting-but-wedged relay
  // must abort into the structured failure shape (which the caller surfaces
  // as a retryable relay error), not hold the mutation open indefinitely.
  it(`start bounds a hung relay with an armed timeout signal`, async () => {
    const hungFetch = vi.fn<RelayFetch>().mockImplementation(
      (_url, init) =>
        new Promise((_resolve, reject) => {
          init?.signal?.addEventListener(`abort`, () =>
            reject(new DOMException(`The operation timed out.`, `TimeoutError`))
          )
        })
    )
    const call = relayPostStart(
      CONFIG,
      { userId: `user-1`, deviceId: `dev-1`, issueId: `issue-1` },
      hungFetch
    )
    const signal = hungFetch.mock.calls[0]?.[1]?.signal
    expect(signal).toBeInstanceOf(AbortSignal)
    expect(signal?.aborted).toBe(false)
    await expect(call).resolves.toEqual({
      ok: false,
      status: 504,
      reason: `relay_timeout`,
    })
  })

  // Non-timeout transport failures keep the pre-REV-34 contract: they throw
  // to the caller (startSession is not best-effort like kill/nudge).
  it(`start still throws on a non-timeout fetch failure`, async () => {
    const downFetch = vi
      .fn<RelayFetch>()
      .mockRejectedValue(new Error(`ECONNREFUSED`))
    await expect(
      relayPostStart(
        CONFIG,
        { userId: `user-1`, deviceId: `dev-1`, issueId: `issue-1` },
        downFetch
      )
    ).rejects.toThrow(`ECONNREFUSED`)
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
      {
        method: `POST`,
        headers: { "x-relay-secret": `test-secret` },
        signal: expect.any(AbortSignal),
      }
    )

    const downFetch = vi
      .fn<RelayFetch>()
      .mockRejectedValue(new Error(`ECONNREFUSED`))
    await expect(
      relayPostKill(CONFIG, `session-1`, downFetch)
    ).resolves.toEqual({ delivered: false })
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
    const call = relayPostKill(CONFIG, `session-1`, hungFetch)
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

  // EXP-700: text injection — same best-effort/never-throws contract as
  // kill; an old relay's 404 must read as not-delivered, never as an error.
  it(`input posts the text, reports delivery and never throws`, async () => {
    const okFetch = vi
      .fn<RelayFetch>()
      .mockResolvedValue(fakeResponse(200, { ok: true, delivered: true }))
    await expect(
      relayPostInput(CONFIG, `session-1`, `hello there`, okFetch)
    ).resolves.toEqual({ delivered: true })
    expect(okFetch).toHaveBeenCalledWith(
      `https://steer.example.com/sessions/session-1/input`,
      {
        method: `POST`,
        headers: {
          "content-type": `application/json`,
          "x-relay-secret": `test-secret`,
        },
        body: JSON.stringify({ text: `hello there` }),
        signal: expect.any(AbortSignal),
      }
    )

    const oldRelayFetch = vi
      .fn<RelayFetch>()
      .mockResolvedValue(fakeResponse(404, { error: `Not found` }))
    await expect(
      relayPostInput(CONFIG, `session-1`, `hello`, oldRelayFetch)
    ).resolves.toEqual({ delivered: false })

    const downFetch = vi
      .fn<RelayFetch>()
      .mockRejectedValue(new Error(`ECONNREFUSED`))
    await expect(
      relayPostInput(CONFIG, `session-1`, `hello`, downFetch)
    ).resolves.toEqual({ delivered: false })
  })

  it(`input bounds a hung relay with an armed timeout signal`, async () => {
    const hungFetch = vi.fn<RelayFetch>().mockImplementation(
      (_url, init) =>
        new Promise((_resolve, reject) => {
          init?.signal?.addEventListener(`abort`, () =>
            reject(new DOMException(`The operation timed out.`, `TimeoutError`))
          )
        })
    )
    const call = relayPostInput(CONFIG, `session-1`, `hello`, hungFetch)
    const signal = hungFetch.mock.calls[0]?.[1]?.signal
    expect(signal).toBeInstanceOf(AbortSignal)
    expect(signal?.aborted).toBe(false)
    await expect(call).resolves.toEqual({ delivered: false })
  })
})
