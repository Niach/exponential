// EXP-569 launcher appearance in the Preact bundle: per-device mode/position,
// the edge tab, panel anchoring classes, and the served icon.
import { beforeEach, describe, expect, it, vi } from "vitest"
import { render } from "preact"
import type { WidgetRemoteConfig, WidgetRuntimeState } from "../types"

vi.mock(`../capture/engine`, () => ({
  captureScreenshot: vi.fn(async () => null),
}))
vi.mock(`../capture/snapdom-engine`, () => ({ snapdomEngine: {} }))
vi.mock(`./Annotator`, () => ({ Annotator: () => null }))
vi.mock(`../api-client`, () => ({
  submitFeedback: vi.fn(),
  submitSupportRequest: vi.fn(),
}))

import { App } from "./App"

const makeState = (
  config: WidgetRemoteConfig | null,
  overrides?: Partial<WidgetRuntimeState>
): WidgetRuntimeState => ({
  protocol: 1,
  options: { key: `expw_test` },
  identity: {},
  customData: {},
  apiOrigin: `https://app.exponential.test`,
  bundleUrl: `https://app.exponential.test/widget/v1/widget.js`,
  configPromise: Promise.resolve(config),
  config,
  disabled: false,
  openRequested: false,
  bundleInjected: true,
  loaderButtonHost: null,
  bundle: null,
  ...overrides,
})

const flush = async () => {
  for (let i = 0; i < 6; i += 1) {
    await new Promise((resolve) => setTimeout(resolve, 0))
  }
}

// happy-dom's matchMedia doesn't evaluate queries; serve the launcher's
// breakpoint from `mobile` and leave prefers-color-scheme unmatched (dark).
const stubViewport = (mobile: boolean) => {
  vi.stubGlobal(
    `matchMedia`,
    vi.fn((query: string) => ({
      matches: query === `(max-width: 767px)` && mobile,
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
    }))
  )
}

describe(`EXP-569 launcher rendering`, () => {
  let container: HTMLDivElement

  beforeEach(() => {
    document.body.innerHTML = ``
    container = document.createElement(`div`)
    document.body.appendChild(container)
    vi.unstubAllGlobals()
  })

  const mount = async (
    config: WidgetRemoteConfig | null,
    overrides?: Partial<WidgetRuntimeState>
  ) => {
    const state = makeState(config, overrides)
    render(<App state={state} />, container)
    await flush()
    return state
  }

  const root = () => container.querySelector<HTMLElement>(`.exp-root`)!
  const fab = () => container.querySelector<HTMLButtonElement>(`.exp-fab`)!

  it(`desktop defaults to a labeled bottom-right fab`, async () => {
    stubViewport(false)
    await mount({ enabled: true })
    expect(fab().className).toBe(`exp-fab`)
    expect(fab().querySelector(`.exp-fab-label`)).not.toBeNull()
    expect(fab().parentElement!.style.cssText).toContain(`right: 20px`)
    expect(root().style.getPropertyValue(`--exp-panel-side`)).toBe(`20px`)
  })

  it(`mobile defaults to a label-less middle-right tab`, async () => {
    stubViewport(true)
    await mount({ enabled: true })
    expect(fab().className).toBe(`exp-fab exp-tab exp-tab-right`)
    expect(fab().querySelector(`.exp-fab-label`)).toBeNull()
    expect(fab().parentElement!.style.cssText).toContain(`top: 50%`)
    // The vertically-centered panel clears the flush 44px tab + 12px gap.
    expect(root().style.getPropertyValue(`--exp-panel-side`)).toBe(`56px`)
  })

  it(`the open panel anchors to the launcher position`, async () => {
    stubViewport(true)
    await mount({ enabled: true }, { openRequested: true })
    const panel = container.querySelector(`.exp-panel`)!
    expect(panel.className).toBe(`exp-panel exp-right exp-vmid`)
  })

  it(`a served top-left fab anchors the panel top-left`, async () => {
    stubViewport(false)
    await mount(
      {
        enabled: true,
        form: {
          launcher: {
            desktop: { mode: `fab`, position: `top-left` },
            mobile: { mode: `tab`, position: `middle-right` },
          },
        } as never,
      },
      { openRequested: true }
    )
    expect(fab().parentElement!.style.cssText).toContain(`left: 20px`)
    expect(fab().style.transformOrigin).toBe(`top left`)
    const panel = container.querySelector(`.exp-panel`)!
    expect(panel.className).toBe(`exp-panel exp-left exp-top`)
  })

  it(`renders the served icon and falls back to the megaphone`, async () => {
    stubViewport(false)
    await mount({
      enabled: true,
      form: {
        launcher: {
          desktop: { mode: `fab`, position: `bottom-right` },
          mobile: { mode: `tab`, position: `middle-right` },
          iconSvg: `<svg viewBox="0 0 24 24"><path d="M3 3"></path></svg>`,
        },
      } as never,
    })
    expect(fab().innerHTML).toContain(`d="M3 3"`)

    document.body.innerHTML = ``
    container = document.createElement(`div`)
    document.body.appendChild(container)
    await mount({ enabled: true })
    // The built-in megaphone (no icon configured).
    expect(fab().innerHTML).toContain(`M11 6a13 13 0 0 0 8.4-2.8`)
  })
})
