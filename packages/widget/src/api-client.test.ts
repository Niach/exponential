// Submit-response contract (EXP-42a): the server's `url` field is ADDITIVE
// and nullable — the client must surface it when present and degrade to null
// against older servers that never send it.
import { afterEach, describe, expect, it, vi } from "vitest"
import type { WidgetRuntimeState } from "./types"
import { submitFeedback, submitSupportRequest } from "./api-client"

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
    title: `Broken button`,
    description: ``,
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

const submitSupport = (
  state: WidgetRuntimeState,
  overrides: { website?: string } = {}
) =>
  submitSupportRequest({
    state,
    message: `Login is broken`,
    email: `user@example.com`,
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
      title: `Broken button`,
      description: ``,
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
      title: `Broken button`,
      description: ``,
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
      title: `Broken button`,
      description: ``,
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

// REV2-10: the support confirmation email carries the reporter's ONLY
// credential, so the panel must learn whether it actually went out.
describe(`submitSupportRequest emailDelivered`, () => {
  it(`surfaces a failed confirmation email`, async () => {
    mockFetchJson({ ok: true, issueId: null, identifier: null, url: null, emailDelivered: false })
    expect(await submitSupport(makeState())).toEqual({
      ok: true,
      identifier: null,
      url: null,
      emailDelivered: false,
    })
  })

  it(`surfaces a delivered confirmation email`, async () => {
    mockFetchJson({ ok: true, emailDelivered: true })
    expect(await submitSupport(makeState())).toMatchObject({
      ok: true,
      emailDelivered: true,
    })
  })

  it(`degrades to null against a server that omits the field`, async () => {
    mockFetchJson({ ok: true })
    expect(await submitSupport(makeState())).toMatchObject({
      ok: true,
      emailDelivered: null,
    })
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

  it(`forwards a filled honeypot on both forms`, async () => {
    const feedbackFetch = mockFetchJson({ ok: true })
    await submit(makeState(), { website: `http://spam.example` })
    expect(bodyOf(feedbackFetch).get(`website`)).toBe(`http://spam.example`)

    const supportFetch = mockFetchJson({ ok: true })
    await submitSupport(makeState(), { website: `http://spam.example` })
    expect(bodyOf(supportFetch).get(`website`)).toBe(`http://spam.example`)
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
    })
  })

  it(`returns code null on a 400 without a structured code`, async () => {
    mockFetchError(400, { error: `Invalid meta` })
    expect(await submit(makeState())).toEqual({
      ok: false,
      message: `Invalid meta`,
      status: 400,
      code: null,
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

  it(`submitSupportRequest surfaces the status and code too`, async () => {
    mockFetchError(400, {
      error: `Invalid submission fields`,
      code: `invalid_email`,
    })
    expect(await submitSupport(makeState())).toEqual({
      ok: false,
      message: `Invalid submission fields`,
      status: 400,
      code: `invalid_email`,
    })
  })
})
