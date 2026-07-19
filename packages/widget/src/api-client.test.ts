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

const submit = (state: WidgetRuntimeState) =>
  submitFeedback({
    state,
    title: `Broken button`,
    description: ``,
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

const submitSupport = (state: WidgetRuntimeState) =>
  submitSupportRequest({
    state,
    message: `Login is broken`,
    email: `user@example.com`,
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
    })
  })

  it(`returns url: null when the server sends an explicit null (the current contract)`, async () => {
    mockFetchJson({ ok: true, issueId: `id-1`, identifier: `EXP-7`, url: null })
    expect(await submit(makeState())).toEqual({
      ok: true,
      identifier: `EXP-7`,
      url: null,
    })
  })

  it(`returns url: null against older servers that omit the field`, async () => {
    mockFetchJson({ ok: true, issueId: `id-1`, identifier: `EXP-7` })
    expect(await submit(makeState())).toEqual({
      ok: true,
      identifier: `EXP-7`,
      url: null,
    })
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
    })
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
