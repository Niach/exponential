// Submit-response contract (EXP-42a): the server's `url` field is ADDITIVE
// and nullable — the client must surface it when present and degrade to null
// against older servers that never send it.
import { afterEach, describe, expect, it, vi } from "vitest"
import type { WidgetRuntimeState } from "./types"
import { submitFeedback } from "./api-client"

const makeState = (): WidgetRuntimeState => ({
  protocol: 1,
  options: { key: `expw_test` },
  identity: {},
  customData: {},
  apiOrigin: `https://app.exponential.test`,
  bundleUrl: `https://app.exponential.test/widget/v1/widget.js`,
  configPromise: Promise.resolve(null),
  config: null,
  disabled: false,
  openRequested: false,
  bundleInjected: true,
  loaderButtonHost: null,
  bundle: null,
})

const submit = (
  state: WidgetRuntimeState,
  overrides: { website?: string } = {}
) =>
  submitFeedback({
    state,
    message: `Broken button`,
    email: null,
    screenshot: null,
    website: overrides.website,
    meta: {
      url: `https://host.example/page`,
      viewportWidth: 800,
      viewportHeight: 600,
      screenWidth: 1600,
      screenHeight: 900,
      devicePixelRatio: 1,
    },
  })


const mockFetchJson = (body: unknown) => {
  const fetchMock = vi.fn(async () => ({
    ok: true,
    status: 200,
    json: async () => body,
  }))
  vi.stubGlobal(`fetch`, fetchMock)
  return fetchMock
}

const mockFetchError = (status: number, body: unknown) => {
  const fetchMock = vi.fn(async () => ({
    ok: false,
    status,
    json: async () => body,
  }))
  vi.stubGlobal(`fetch`, fetchMock)
  return fetchMock
}

afterEach(() => {
  vi.unstubAllGlobals()
})

describe(`submitFeedback response parsing`, () => {
  it(`surfaces identifier and url when the server sends both`, async () => {
    const url = `https://app.exponential.test/t/feedback/projects/exponential/issues/EXP-7`
    mockFetchJson({ ok: true, issueId: `id-1`, identifier: `EXP-7`, url })
    expect(await submit(makeState())).toEqual({
      ok: true,
      identifier: `EXP-7`,
      url,
      emailDelivered: null,
    })
  })

  it(`returns url: null when the server sends an explicit null (the current contract)`, async () => {
    mockFetchJson({ ok: true, issueId: `id-1`, identifier: `EXP-7`, url: null })
    expect(await submit(makeState())).toEqual({
      ok: true,
      identifier: `EXP-7`,
      url: null,
      emailDelivered: null,
    })
  })

  it(`returns url: null against older servers that omit the field`, async () => {
    mockFetchJson({ ok: true, issueId: `id-1`, identifier: `EXP-7` })
    expect(await submit(makeState())).toEqual({
      ok: true,
      identifier: `EXP-7`,
      url: null,
      emailDelivered: null,
    })
  })

  // FEED-5: reporter-attached pictures ride as repeated `images` parts with
  // their original filenames and types.
  it(`appends attached pictures as repeated images parts`, async () => {
    const fetchMock = mockFetchJson({ ok: true, identifier: `EXP-7` })
    await submitFeedback({
      state: makeState(),
      message: `Broken button`,
      email: null,
      screenshot: null,
      images: [
        { blob: new Blob([`a`], { type: `image/png` }), filename: `ref-a.png` },
        {
          blob: new Blob([`b`], { type: `image/jpeg` }),
          filename: `ref-b.jpg`,
        },
      ],
      meta: {
        url: `https://host.example/page`,
        viewportWidth: 800,
        viewportHeight: 600,
        screenWidth: 1600,
        screenHeight: 900,
        devicePixelRatio: 1,
      },
    })
    const request = fetchMock.mock.calls[0] as unknown as [
      string,
      { body: FormData },
    ]
    const images = request[1].body.getAll(`images`) as File[]
    expect(images).toHaveLength(2)
    expect(images[0].name).toBe(`ref-a.png`)
    expect(images[0].type).toBe(`image/png`)
    expect(images[1].name).toBe(`ref-b.jpg`)
    expect(images[1].type).toBe(`image/jpeg`)
  })

  // EXP-435: reporter-picked labels ride as one JSON `labels` field.
  it(`sends selected label ids and omits the field when empty`, async () => {
    const meta = {
      url: `https://host.example/page`,
      viewportWidth: 800,
      viewportHeight: 600,
      screenWidth: 1600,
      screenHeight: 900,
      devicePixelRatio: 1,
    }
    const fetchMock = mockFetchJson({ ok: true, identifier: `EXP-7` })
    await submitFeedback({
      state: makeState(),
      message: `Broken button`,
      email: null,
      screenshot: null,
      labelIds: [`l-1`, `l-2`],
      meta,
    })
    const withLabels = fetchMock.mock.calls[0] as unknown as [
      string,
      { body: FormData },
    ]
    expect(withLabels[1].body.get(`labels`)).toBe(
      JSON.stringify([`l-1`, `l-2`])
    )

    await submitFeedback({
      state: makeState(),
      message: `Broken button`,
      email: null,
      screenshot: null,
      labelIds: [],
      meta,
    })
    const withoutLabels = fetchMock.mock.calls[1] as unknown as [
      string,
      { body: FormData },
    ]
    expect(withoutLabels[1].body.get(`labels`)).toBeNull()
  })

  it(`keeps identifier and url null on an unparseable body`, async () => {
    const fetchMock = vi.fn(async () => ({
      ok: true,
      status: 200,
      json: async () => {
        throw new Error(`bad json`)
      },
    }))
    vi.stubGlobal(`fetch`, fetchMock)
    expect(await submit(makeState())).toEqual({
      ok: true,
      identifier: null,
      url: null,
      emailDelivered: null,
    })
  })
})

// REV2-10 / SLOP-4: the confirmation email carries the reporter's ONLY
// credential, so the panel must learn whether it actually went out.
describe(`submit emailDelivered`, () => {
  it(`surfaces a failed confirmation email`, async () => {
    mockFetchJson({ ok: true, issueId: null, identifier: null, url: null, emailDelivered: false })
    expect(await submit(makeState())).toEqual({
      ok: true,
      identifier: null,
      url: null,
      emailDelivered: false,
    })
  })

  it(`surfaces a delivered confirmation email`, async () => {
    mockFetchJson({ ok: true, emailDelivered: true })
    expect(await submit(makeState())).toMatchObject({
      ok: true,
      emailDelivered: true,
    })
  })

  it(`degrades to null against a server that omits the field`, async () => {
    mockFetchJson({ ok: true })
    expect(await submit(makeState())).toMatchObject({
      ok: true,
      emailDelivered: null,
    })
  })
})

// SLOP-4: the one form field rides as `message`; a headless host's explicit
// title is forwarded only when given (the server titles off the message's
// first line otherwise). No `mode`, no `description`.
describe(`submit payload shape`, () => {
  const bodyOf = (fetchMock: ReturnType<typeof mockFetchJson>): FormData =>
    ((fetchMock.mock.calls[0] as unknown[])[1] as { body: FormData }).body

  it(`sends message and omits title, description and mode`, async () => {
    const fetchMock = mockFetchJson({ ok: true })
    await submit(makeState())
    const body = bodyOf(fetchMock)
    expect(body.get(`message`)).toBe(`Broken button`)
    expect(body.get(`title`)).toBeNull()
    expect(body.get(`description`)).toBeNull()
    expect(body.get(`mode`)).toBeNull()
  })

  it(`forwards an explicit title`, async () => {
    const fetchMock = mockFetchJson({ ok: true })
    await submitFeedback({
      state: makeState(),
      message: `The form loops back after I press Sign in.`,
      title: `Login loops`,
      email: null,
      screenshot: null,
      meta: {
        url: `https://host.example/page`,
        viewportWidth: 800,
        viewportHeight: 600,
        screenWidth: 1600,
        screenHeight: 900,
        devicePixelRatio: 1,
      },
    })
    expect(bodyOf(fetchMock).get(`title`)).toBe(`Login loops`)
  })
})

// REV2-69: the honeypot value must actually reach the server — the check has
// existed on /api/widget/submit since day one with no client half.
describe(`honeypot field forwarding`, () => {
  const bodyOf = (fetchMock: ReturnType<typeof mockFetchJson>): FormData =>
    ((fetchMock.mock.calls[0] as unknown[])[1] as { body: FormData }).body

  it(`omits website when the honeypot is untouched`, async () => {
    const fetchMock = mockFetchJson({ ok: true })
    await submit(makeState())
    expect(bodyOf(fetchMock).get(`website`)).toBeNull()
  })

  it(`forwards a filled honeypot`, async () => {
    const feedbackFetch = mockFetchJson({ ok: true })
    await submit(makeState(), { website: `http://spam.example` })
    expect(bodyOf(feedbackFetch).get(`website`)).toBe(`http://spam.example`)
  })
})

describe(`submit error status + code parsing`, () => {
  it(`surfaces the status and structured code on a coded 400`, async () => {
    mockFetchError(400, {
      error: `Invalid submission fields`,
      code: `invalid_email`,
    })
    expect(await submit(makeState())).toEqual({
      ok: false,
      message: `Invalid submission fields`,
      status: 400,
      code: `invalid_email`,
      serverMessage: `Invalid submission fields`,
    })
  })

  it(`returns code null on a 400 without a structured code`, async () => {
    mockFetchError(400, { error: `Invalid meta` })
    expect(await submit(makeState())).toEqual({
      ok: false,
      message: `Invalid meta`,
      status: 400,
      code: null,
      serverMessage: `Invalid meta`,
    })
  })

  it(`surfaces status null on a network error`, async () => {
    vi.stubGlobal(
      `fetch`,
      vi.fn(async () => {
        throw new Error(`offline`)
      })
    )
    expect(await submit(makeState())).toMatchObject({
      ok: false,
      status: null,
      code: null,
    })
  })

  it(`keeps its own text on a network error`, async () => {
    vi.stubGlobal(
      `fetch`,
      vi.fn(async () => {
        throw new Error(`offline`)
      })
    )
    expect(await submit(makeState())).toMatchObject({
      message: `Network error. Please try again.`,
    })
  })
})

// NEW-19 (FEED-99/115/117): error responses map to reporter-facing copy in
// the widget's language; misconfiguration also tells the integrator why.
describe(`submit error wording`, () => {
  const mockFetchErrorWithHeaders = (
    status: number,
    body: unknown,
    headers: Record<string, string> = {}
  ) => {
    vi.stubGlobal(
      `fetch`,
      vi.fn(async () => ({
        ok: false,
        status,
        headers: new Headers(headers),
        json: async () => body,
      }))
    )
  }
  const german = (): WidgetRuntimeState => ({
    ...makeState(),
    options: { key: `expw_test`, locale: `de` },
  })

  it(`origin_not_allowed explains the allowlist and warns with the page origin`, async () => {
    const warn = vi.spyOn(console, `warn`).mockImplementation(() => undefined)
    mockFetchErrorWithHeaders(403, {
      error: `Origin not allowed`,
      code: `origin_not_allowed`,
      origin: `https://x.example`,
    })
    expect(await submit(makeState())).toMatchObject({
      ok: false,
      status: 403,
      code: `origin_not_allowed`,
      message: `This site isn't allowed to send feedback. The site owner can add it under the widget's allowed domains.`,
    })
    expect(warn).toHaveBeenCalledTimes(1)
    expect(String(warn.mock.calls[0][0])).toContain(window.location.origin)
    expect(String(warn.mock.calls[0][0])).toContain(`allowed domains`)
    warn.mockRestore()
  })

  it(`matches an older server's code-less 403 by its text`, async () => {
    const warn = vi.spyOn(console, `warn`).mockImplementation(() => undefined)
    mockFetchErrorWithHeaders(403, { error: `Origin not allowed` })
    expect(await submit(makeState())).toMatchObject({
      code: `origin_not_allowed`,
      message: expect.stringContaining(`isn't allowed to send feedback`),
    })
    mockFetchErrorWithHeaders(403, { error: `Widget is disabled` })
    expect(await submit(makeState())).toMatchObject({
      code: `widget_disabled`,
      message: `This widget is currently disabled.`,
    })
    warn.mockRestore()
  })

  it(`unknown_key and widget_disabled get their own copy and a console warning`, async () => {
    const warn = vi.spyOn(console, `warn`).mockImplementation(() => undefined)
    mockFetchErrorWithHeaders(404, {
      error: `Unknown widget key`,
      code: `unknown_key`,
    })
    expect(await submit(makeState())).toMatchObject({
      code: `unknown_key`,
      message: expect.stringContaining(`widget key`),
    })
    mockFetchErrorWithHeaders(403, {
      error: `Widget is disabled`,
      code: `widget_disabled`,
    })
    expect(await submit(makeState())).toMatchObject({
      code: `widget_disabled`,
      message: `This widget is currently disabled.`,
    })
    expect(warn).toHaveBeenCalledTimes(2)
    warn.mockRestore()
  })

  it.each([
    [`30`, `Try again in 30 seconds.`],
    [`1`, `Try again in 1 second.`],
    [`60`, `Try again in 1 minute.`],
    [`61`, `Try again in 2 minutes.`],
    [`450`, `Try again in 8 minutes.`],
    [`3600`, `Try again in 1 hour.`],
    [`5000`, `Try again in 2 hours.`],
  ])(`429 with Retry-After %s reads "%s"`, async (header, tail) => {
    mockFetchErrorWithHeaders(
      429,
      {
        error: `Too many submissions, try again later`,
        code: `rate_limited`,
        retryAfterSeconds: Number(header),
      },
      { "Retry-After": header }
    )
    expect(await submit(makeState())).toMatchObject({
      ok: false,
      status: 429,
      code: `rate_limited`,
      message: `Too many reports right now. ${tail}`,
    })
  })

  it(`429 without Retry-After says try again later`, async () => {
    mockFetchErrorWithHeaders(429, {
      error: `Too many submissions, try again later`,
    })
    const result = await submit(makeState())
    expect(result).toMatchObject({
      status: 429,
      code: null,
      message: `Too many reports right now. Try again later.`,
    })
    expect(JSON.stringify(result)).not.toContain(`in a minute`)
  })

  it(`429 falls back to the body's retryAfterSeconds`, async () => {
    mockFetchErrorWithHeaders(429, {
      error: `Too many submissions, try again later`,
      code: `rate_limited`,
      retryAfterSeconds: 120,
    })
    expect(await submit(makeState())).toMatchObject({
      message: `Too many reports right now. Try again in 2 minutes.`,
    })
  })

  it(`speaks German when the widget does`, async () => {
    mockFetchErrorWithHeaders(
      429,
      { error: `Too many submissions, try again later`, code: `rate_limited` },
      { "Retry-After": `450` }
    )
    expect(await submit(german())).toMatchObject({
      message: `Gerade gehen zu viele Meldungen ein. Versuchen Sie es in 8 Minuten erneut.`,
    })
    const warn = vi.spyOn(console, `warn`).mockImplementation(() => undefined)
    mockFetchErrorWithHeaders(403, {
      error: `Origin not allowed`,
      code: `origin_not_allowed`,
    })
    expect(await submit(german())).toMatchObject({
      message: expect.stringContaining(`Diese Website darf kein Feedback senden`),
    })
    warn.mockRestore()
    // An unmapped server error falls back to the German generic line, but
    // keeps the server's own text for code-less matching.
    mockFetchErrorWithHeaders(400, { error: `Invalid meta` })
    expect(await submit(german())).toMatchObject({
      message: `Etwas ist schiefgelaufen. Bitte versuchen Sie es erneut.`,
      serverMessage: `Invalid meta`,
    })
    vi.stubGlobal(
      `fetch`,
      vi.fn(async () => {
        throw new Error(`offline`)
      })
    )
    expect(await submit(german())).toMatchObject({
      message: `Netzwerkfehler. Bitte versuchen Sie es erneut.`,
    })
  })
})
