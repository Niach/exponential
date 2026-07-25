// Headless submit (EXP-244): ExponentialWidget.submit() posts through the
// loader without any panel UI — config-gated, identity/customData merged,
// never throws into the host page.
import { beforeEach, describe, expect, it, vi } from "vitest"
import type { WidgetRemoteConfig } from "./types"

const loaderSrc = `https://app.exponential.test/widget/v1/loader.js`
const testKey = `expw_${`a`.repeat(32)}`

function installScriptTag(): void {
  const script = document.createElement(`script`)
  script.src = loaderSrc
  document.head.appendChild(script)
}

function installSnippetStub(): void {
  const queue: unknown[] = []
  const api: Record<string, unknown> = { q: queue }
  for (const method of [
    `init`,
    `identify`,
    `setCustomData`,
    `open`,
    `close`,
    `submit`,
  ]) {
    api[method] = (...args: unknown[]) => {
      queue.push([method, args])
    }
  }
  ;(window as unknown as { ExponentialWidget: unknown }).ExponentialWidget =
    api
}

async function importLoader(): Promise<void> {
  vi.resetModules()
  await import(`./loader`)
}

// Config GET resolves with the given payload; submit POSTs are captured.
const submitCalls: Array<{ url: string; body: FormData }> = []
let submitResponse: () => Promise<Response> | Response

function stubFetch(config: WidgetRemoteConfig | null): void {
  vi.stubGlobal(
    `fetch`,
    vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = String(input)
      if (url.includes(`/api/widget/config`)) {
        if (config === null) throw new Error(`config fetch failed`)
        return new Response(JSON.stringify(config), { status: 200 })
      }
      submitCalls.push({ url, body: init?.body as FormData })
      return submitResponse()
    })
  )
}

const enabledConfig: WidgetRemoteConfig = { enabled: true }

beforeEach(() => {
  document.head.innerHTML = ``
  document.body.innerHTML = ``
  document
    .querySelectorAll(`[data-exponential-widget]`)
    .forEach((element) => element.remove())
  delete (window as { ExponentialWidget?: unknown }).ExponentialWidget
  delete (window as { __expWidget?: unknown }).__expWidget
  vi.restoreAllMocks()
  submitCalls.length = 0
  submitResponse = () =>
    new Response(JSON.stringify({ ok: true, identifier: `EXP-9`, url: null }), {
      status: 201,
    })
  installScriptTag()
})

describe(`headless submit`, () => {
  const boot = async (config: WidgetRemoteConfig | null) => {
    stubFetch(config)
    installSnippetStub()
    window.ExponentialWidget!.init({ key: testKey, showButton: false })
    await importLoader()
  }

  it(`resolves an error result before init`, async () => {
    stubFetch(enabledConfig)
    installSnippetStub()
    await importLoader()
    const result = await window.ExponentialWidget!.submit({ title: `x` })
    expect(result.ok).toBe(false)
    expect(result.error).toContain(`not initialized`)
  })

  it(`submits feedback with identity + merged customData`, async () => {
    await boot(enabledConfig)
    window.ExponentialWidget!.identify({
      email: `jane@acme.com`,
      name: `Jane`,
    })
    window.ExponentialWidget!.setCustomData({ plan: `pro`, desk: `old` })

    const result = await window.ExponentialWidget!.submit({
      title: `Broken thing`,
      description: `Details`,
      customData: { desk: `42b` },
    })

    expect(result).toEqual({ ok: true, identifier: `EXP-9`, url: null })
    expect(submitCalls.length).toBe(1)
    expect(submitCalls[0].url).toBe(
      `https://app.exponential.test/api/widget/submit`
    )
    const body = submitCalls[0].body
    expect(body.get(`key`)).toBe(testKey)
    expect(body.get(`title`)).toBe(`Broken thing`)
    expect(body.get(`description`)).toBe(`Details`)
    expect(body.get(`email`)).toBe(`jane@acme.com`)
    expect(body.get(`name`)).toBe(`Jane`)
    expect(JSON.parse(body.get(`customData`) as string)).toEqual({
      plan: `pro`,
      desk: `42b`,
    })
    expect(body.get(`meta`)).toBeTruthy()
  })

  it(`payload email/name/screenshot override the identity fallbacks`, async () => {
    await boot(enabledConfig)
    window.ExponentialWidget!.identify({ email: `jane@acme.com` })
    const shot = new Blob([`png-bytes`], { type: `image/png` })
    const result = await window.ExponentialWidget!.submit({
      title: `T`,
      email: `other@acme.com`,
      name: `dani`,
      screenshot: shot,
    })
    expect(result.ok).toBe(true)
    const body = submitCalls[0].body
    expect(body.get(`email`)).toBe(`other@acme.com`)
    expect(body.get(`name`)).toBe(`dani`)
    expect(body.get(`screenshot`)).toBeInstanceOf(File)
  })

  it(`routes mode: support to the support pipeline`, async () => {
    await boot({ enabled: true, modes: [`feedback`, `support`] })
    const result = await window.ExponentialWidget!.submit({
      mode: `support`,
      message: `Help me`,
      email: `reporter@example.com`,
    })
    expect(result.ok).toBe(true)
    const body = submitCalls[0].body
    expect(body.get(`mode`)).toBe(`support`)
    expect(body.get(`message`)).toBe(`Help me`)
    expect(body.get(`email`)).toBe(`reporter@example.com`)
  })

  it(`gates on the served modes`, async () => {
    await boot({ enabled: true, modes: [`feedback`] })
    const result = await window.ExponentialWidget!.submit({
      mode: `support`,
      message: `Help`,
      email: `a@b.co`,
    })
    expect(result).toMatchObject({ ok: false, code: `mode_unavailable` })
    expect(submitCalls.length).toBe(0)
  })

  it(`refuses when the config resolved disabled`, async () => {
    await boot({ enabled: false })
    const result = await window.ExponentialWidget!.submit({ title: `x` })
    expect(result).toMatchObject({ ok: false, code: `widget_disabled` })
    expect(submitCalls.length).toBe(0)
  })

  it(`fails open when the config fetch failed (server re-enforces)`, async () => {
    await boot(null)
    const result = await window.ExponentialWidget!.submit({ title: `x` })
    expect(result.ok).toBe(true)
    expect(submitCalls.length).toBe(1)
  })

  it(`relays server error codes without throwing`, async () => {
    await boot(enabledConfig)
    submitResponse = () =>
      new Response(
        JSON.stringify({ error: `Name is required`, code: `name_required` }),
        { status: 400 }
      )
    const result = await window.ExponentialWidget!.submit({ title: `x` })
    expect(result).toEqual({
      ok: false,
      error: `Name is required`,
      code: `name_required`,
    })
  })

  it(`resolves a network failure as an error result`, async () => {
    await boot(enabledConfig)
    submitResponse = () => Promise.reject(new Error(`offline`))
    const result = await window.ExponentialWidget!.submit({ title: `x` })
    expect(result.ok).toBe(false)
    expect(result.error).toContain(`Network error`)
  })

  it(`drains a submit queued before the loader ran (fire-and-forget)`, async () => {
    stubFetch(enabledConfig)
    installSnippetStub()
    window.ExponentialWidget!.init({ key: testKey, showButton: false })
    window.ExponentialWidget!.submit({ title: `Queued report` })
    await importLoader()
    // The queued call resolves asynchronously after the config fetch.
    for (let i = 0; i < 6; i += 1) {
      await new Promise((resolve) => setTimeout(resolve, 0))
    }
    expect(submitCalls.length).toBe(1)
    expect(submitCalls[0].body.get(`title`)).toBe(`Queued report`)
  })
})
