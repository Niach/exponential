// EXP-435 theming: the .exp-root custom properties follow the resolved
// theme — config-served, init-option, and runtime setTheme (themeOverride +
// stateChanged) — including the custom panel/text color overrides.
import { beforeEach, describe, expect, it, vi } from "vitest"
import { render } from "preact"
import type { WidgetRemoteConfig, WidgetRuntimeState } from "../types"
import { darkPalette, lightPalette, mixHex } from "../theme"

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

describe(`EXP-435 theme resolution`, () => {
  let container: HTMLDivElement

  beforeEach(() => {
    document.body.innerHTML = ``
    container = document.createElement(`div`)
    document.body.appendChild(container)
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

  const rootVar = (name: string) =>
    container
      .querySelector<HTMLElement>(`.exp-root`)!
      .style.getPropertyValue(name)

  it(`defaults to the dark palette (pre-theme configs)`, async () => {
    await mount({ enabled: true })
    expect(rootVar(`--exp-card`)).toBe(darkPalette.card)
    expect(rootVar(`--exp-accent`)).toBe(darkPalette.defaultAccent)
  })

  it(`a config-served light theme renders the light palette`, async () => {
    await mount({ enabled: true, form: { theme: `light` } as never })
    expect(rootVar(`--exp-card`)).toBe(lightPalette.card)
    expect(rootVar(`--exp-foreground`)).toBe(lightPalette.foreground)
    // The default accent flips with the palette.
    expect(rootVar(`--exp-accent`)).toBe(lightPalette.defaultAccent)
  })

  it(`the init option beats the config theme`, async () => {
    await mount(
      { enabled: true, form: { theme: `light` } as never },
      { options: { key: `expw_test`, theme: `dark` } }
    )
    expect(rootVar(`--exp-card`)).toBe(darkPalette.card)
  })

  it(`a runtime themeOverride + stateChanged live-switches`, async () => {
    const state = await mount({ enabled: true })
    expect(rootVar(`--exp-card`)).toBe(darkPalette.card)
    state.themeOverride = `light`
    state.bundle!.stateChanged()
    await flush()
    expect(rootVar(`--exp-card`)).toBe(lightPalette.card)
  })

  it(`custom panel/text colors land in the derived vars`, async () => {
    await mount({
      enabled: true,
      form: {
        theme: `dark`,
        backgroundColor: `#101828`,
        textColor: `#e2e8f0`,
      } as never,
    })
    expect(rootVar(`--exp-card`)).toBe(`#101828`)
    expect(rootVar(`--exp-foreground`)).toBe(`#e2e8f0`)
    expect(rootVar(`--exp-secondary`)).toBe(mixHex(`#101828`, `#e2e8f0`, 0.1))
  })

  it(`an explicit accent survives every theme`, async () => {
    await mount({
      enabled: true,
      form: { theme: `light`, accentColor: `#336699` } as never,
    })
    expect(rootVar(`--exp-accent`)).toBe(`#336699`)
  })
})
