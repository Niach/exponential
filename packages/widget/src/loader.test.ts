// Loader behavior: queue drain, init guards, button rendering, bundle
// injection. The loader runs as a side effect on import, so each test
// resets modules and rebuilds the DOM.
import { beforeEach, describe, expect, it, vi } from "vitest"

const loaderSrc = `https://app.exponential.test/widget/v1/loader.js`

function installScriptTag(): void {
  const script = document.createElement(`script`)
  script.src = loaderSrc
  document.head.appendChild(script)
}

function installSnippetStub(): void {
  const queue: unknown[] = []
  const api: Record<string, unknown> = { q: queue }
  for (const method of [`init`, `identify`, `setCustomData`, `open`, `close`]) {
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

beforeEach(() => {
  document.head.innerHTML = ``
  document.body.innerHTML = ``
  // The loader mounts its host on <html>, so it outlives clearing body — drop
  // any host from a previous test to keep assertions isolated.
  document
    .querySelectorAll(`[data-exponential-widget]`)
    .forEach((element) => element.remove())
  delete (window as { ExponentialWidget?: unknown }).ExponentialWidget
  delete (window as { __expWidget?: unknown }).__expWidget
  vi.restoreAllMocks()
  vi.stubGlobal(
    `fetch`,
    vi.fn(() => new Promise(() => {})) // config fetch stays pending
  )
  installScriptTag()
})

describe(`loader`, () => {
  it(`drains the snippet queue in order`, async () => {
    installSnippetStub()
    window.ExponentialWidget!.init({ key: `expw_${`a`.repeat(32)}` })
    window.ExponentialWidget!.identify({ email: `jane@acme.com` })
    window.ExponentialWidget!.setCustomData({ plan: `pro` })

    await importLoader()

    const state = window.__expWidget!
    expect(state).toBeDefined()
    expect(state.options.key).toBe(`expw_${`a`.repeat(32)}`)
    expect(state.identity).toEqual({ email: `jane@acme.com` })
    expect(state.customData).toEqual({ plan: `pro` })
  })

  it(`derives the bundle URL and api origin from the loader script src`, async () => {
    installSnippetStub()
    window.ExponentialWidget!.init({ key: `expw_${`a`.repeat(32)}` })
    await importLoader()

    expect(window.__expWidget!.bundleUrl).toBe(
      `https://app.exponential.test/widget/v1/widget.js`
    )
    expect(window.__expWidget!.apiOrigin).toBe(`https://app.exponential.test`)
  })

  it(`honors the host override`, async () => {
    installSnippetStub()
    window.ExponentialWidget!.init({
      key: `expw_${`a`.repeat(32)}`,
      host: `https://other.example/`,
    })
    await importLoader()
    expect(window.__expWidget!.apiOrigin).toBe(`https://other.example`)
  })

  it(`ignores a second init with a warning`, async () => {
    const warn = vi.spyOn(console, `warn`).mockImplementation(() => {})
    installSnippetStub()
    window.ExponentialWidget!.init({ key: `expw_${`a`.repeat(32)}` })
    await importLoader()

    window.ExponentialWidget!.init({ key: `expw_${`b`.repeat(32)}` })
    expect(window.__expWidget!.options.key).toBe(`expw_${`a`.repeat(32)}`)
    expect(warn).toHaveBeenCalledWith(
      expect.stringContaining(`already initialized`)
    )
  })

  it(`stays inert when init lacks a key`, async () => {
    const error = vi.spyOn(console, `error`).mockImplementation(() => {})
    installSnippetStub()
    ;(
      window.ExponentialWidget!.init as (options: unknown) => void
    )({})
    await importLoader()

    expect(window.__expWidget).toBeUndefined()
    expect(error).toHaveBeenCalled()
  })

  it(`renders the floating button into a shadow host`, async () => {
    installSnippetStub()
    window.ExponentialWidget!.init({ key: `expw_${`a`.repeat(32)}` })
    await importLoader()

    const host = document.querySelector<HTMLElement>(
      `[data-exponential-widget]`
    )
    expect(host).not.toBeNull()
    const button = host!.shadowRoot!.querySelector(`button.exp-fab`)
    expect(button).not.toBeNull()
    expect(button!.textContent).toContain(`Feedback`)
  })

  it(`does not render a button when showButton is false`, async () => {
    installSnippetStub()
    window.ExponentialWidget!.init({
      key: `expw_${`a`.repeat(32)}`,
      showButton: false,
    })
    await importLoader()
    expect(document.querySelector(`[data-exponential-widget]`)).toBeNull()
  })

  it(`injects the bundle script once on open`, async () => {
    installSnippetStub()
    window.ExponentialWidget!.init({ key: `expw_${`a`.repeat(32)}` })
    await importLoader()

    window.ExponentialWidget!.open()
    window.ExponentialWidget!.open()

    const scripts = document.querySelectorAll(
      `script[src="https://app.exponential.test/widget/v1/widget.js"]`
    )
    expect(scripts).toHaveLength(1)
    expect(window.__expWidget!.openRequested).toBe(true)
  })

  it(`delegates open/close to registered bundle hooks`, async () => {
    installSnippetStub()
    window.ExponentialWidget!.init({ key: `expw_${`a`.repeat(32)}` })
    await importLoader()

    const open = vi.fn()
    const close = vi.fn()
    window.__expWidget!.bundle = { open, close, stateChanged: vi.fn() }

    window.ExponentialWidget!.open()
    window.ExponentialWidget!.close()
    expect(open).toHaveBeenCalledTimes(1)
    expect(close).toHaveBeenCalledTimes(1)
  })

  it(`notifies the bundle when identity changes after mount`, async () => {
    installSnippetStub()
    window.ExponentialWidget!.init({ key: `expw_${`a`.repeat(32)}` })
    await importLoader()

    const stateChanged = vi.fn()
    window.__expWidget!.bundle = {
      open: vi.fn(),
      close: vi.fn(),
      stateChanged,
    }
    window.ExponentialWidget!.identify({ email: `late@acme.com` })
    expect(window.__expWidget!.identity.email).toBe(`late@acme.com`)
    expect(stateChanged).toHaveBeenCalled()
  })
})

describe(`loader identity + custom data clamp`, () => {
  const initAndImport = async () => {
    installSnippetStub()
    window.ExponentialWidget!.init({ key: `expw_${`a`.repeat(32)}` })
    await importLoader()
  }

  it(`drops an invalid email, keeps the previous valid one, and warns`, async () => {
    await initAndImport()
    window.ExponentialWidget!.identify({ email: `good@acme.com` })
    const warn = vi.spyOn(console, `warn`).mockImplementation(() => {})
    window.ExponentialWidget!.identify({ email: `not-an-email` })
    expect(window.__expWidget!.identity.email).toBe(`good@acme.com`)
    expect(warn).toHaveBeenCalled()
  })

  it(`truncates an overlong name and userId to 255 chars`, async () => {
    await initAndImport()
    vi.spyOn(console, `warn`).mockImplementation(() => {})
    window.ExponentialWidget!.identify({
      name: `x`.repeat(300),
      userId: `y`.repeat(300),
    })
    expect(window.__expWidget!.identity.name).toHaveLength(255)
    expect(window.__expWidget!.identity.userId).toHaveLength(255)
  })

  it(`still clears a field via identify({ email: undefined })`, async () => {
    await initAndImport()
    window.ExponentialWidget!.identify({ email: `good@acme.com` })
    expect(window.__expWidget!.identity.email).toBe(`good@acme.com`)
    window.ExponentialWidget!.identify({ email: undefined })
    expect(window.__expWidget!.identity.email).toBeUndefined()
  })

  it(`ignores a merged customData blob over 8KB wholesale and warns`, async () => {
    await initAndImport()
    window.ExponentialWidget!.setCustomData({ plan: `pro` })
    expect(window.__expWidget!.customData).toEqual({ plan: `pro` })

    const warn = vi.spyOn(console, `warn`).mockImplementation(() => {})
    window.ExponentialWidget!.setCustomData({ blob: `x`.repeat(9000) })
    // Keep-last-valid: the previous data survives untouched.
    expect(window.__expWidget!.customData).toEqual({ plan: `pro` })
    expect(warn).toHaveBeenCalled()
  })

  it(`accepts an under-limit customData merge`, async () => {
    await initAndImport()
    window.ExponentialWidget!.setCustomData({ plan: `pro` })
    window.ExponentialWidget!.setCustomData({ tier: `gold` })
    expect(window.__expWidget!.customData).toEqual({ plan: `pro`, tier: `gold` })
  })

  it(`ignores a circular customData value without throwing`, async () => {
    await initAndImport()
    window.ExponentialWidget!.setCustomData({ plan: `pro` })
    vi.spyOn(console, `warn`).mockImplementation(() => {})
    const circular: Record<string, unknown> = {}
    circular.self = circular
    expect(() =>
      (window.ExponentialWidget!.setCustomData as (data: unknown) => void)(
        circular
      )
    ).not.toThrow()
    expect(window.__expWidget!.customData).toEqual({ plan: `pro` })
  })

  it(`clamps identity calls queued before the loader took over`, async () => {
    installSnippetStub()
    window.ExponentialWidget!.init({ key: `expw_${`a`.repeat(32)}` })
    // A good then a bad email queued through the snippet stub: after drain the
    // bad one must have been dropped, leaving the earlier valid address.
    window.ExponentialWidget!.identify({ email: `good@acme.com` })
    window.ExponentialWidget!.identify({ email: `not-an-email` })
    vi.spyOn(console, `warn`).mockImplementation(() => {})
    await importLoader()
    expect(window.__expWidget!.identity.email).toBe(`good@acme.com`)
  })
})
