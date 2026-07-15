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

const mockFetchJson = (body: unknown) => {
  const fetchMock = vi.fn(async () => ({
    ok: true,
    status: 200,
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

  it(`returns url: null when the server sends an explicit null (non-public project)`, async () => {
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
