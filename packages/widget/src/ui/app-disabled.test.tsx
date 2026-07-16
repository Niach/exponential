// A board whose remote config resolves `enabled: false` must fully stand down:
// the FAB disappears, an already-open panel closes, and open() no-ops. The
// loader writes state.disabled from its own configPromise.then registered at
// init() — before this bundle could attach its continuation — so the App reads
// state.disabled inside the config-resolve effect. These tests bypass the
// loader, so the state factory MUST register that loader-mimicking `.then`
// before render, otherwise state.disabled would never flip and the tests would
// pass vacuously. The fail-open guard (config === null on fetch failure keeps
// the widget usable) is exercised too.
import { beforeEach, describe, expect, it, vi } from "vitest"
import { render } from "preact"
import type { WidgetRemoteConfig, WidgetRuntimeState } from "../types"

vi.mock(`../capture/engine`, () => ({
  captureScreenshot: vi.fn(async () => null),
}))
vi.mock(`../capture/snapdom-engine`, () => ({ snapdomEngine: {} }))
vi.mock(`./Annotator`, () => ({ Annotator: () => null }))

import { App } from "./App"

const makeDeferredState = () => {
  let resolveConfig!: (config: WidgetRemoteConfig | null) => void
  const configPromise = new Promise<WidgetRemoteConfig | null>((resolve) => {
    resolveConfig = resolve
  })
  const state: WidgetRuntimeState = {
    protocol: 1,
    options: { key: `expw_test` },
    identity: {},
    customData: {},
    apiOrigin: `https://app.exponential.test`,
    bundleUrl: `https://app.exponential.test/widget/v1/widget.js`,
    configPromise,
    config: null,
    disabled: false,
    openRequested: false,
    bundleInjected: true,
    loaderButtonHost: null,
    bundle: null,
  }
  // Mirror loader.ts: this continuation is registered before the App can
  // attach its own, so it always writes state.config/state.disabled first.
  void configPromise.then((config) => {
    state.config = config
    if (config && !config.enabled) state.disabled = true
  })
  return { state, resolveConfig }
}

const flush = async () => {
  for (let i = 0; i < 6; i += 1) {
    await new Promise((resolve) => setTimeout(resolve, 0))
  }
}

describe(`widget disabled state`, () => {
  let container: HTMLDivElement

  beforeEach(() => {
    if (typeof URL.createObjectURL !== `function`) {
      URL.createObjectURL = () => `blob:test`
      URL.revokeObjectURL = () => undefined
    }
    document.body.innerHTML = ``
    container = document.createElement(`div`)
    document.body.appendChild(container)
  })

  it(`closes an open panel when the config resolves disabled`, async () => {
    const { state, resolveConfig } = makeDeferredState()
    render(<App state={state} />, container)
    await flush()

    state.bundle?.open()
    await flush()
    expect(container.querySelector(`.exp-panel`)).toBeTruthy()
    expect(container.querySelector(`.exp-fab`)).toBeTruthy()

    resolveConfig({ enabled: false })
    await flush()
    expect(container.querySelector(`.exp-panel`)).toBeNull()
    expect(container.querySelector(`.exp-fab`)).toBeNull()
  })

  it(`renders nothing when disabled at mount with a pending open request`, async () => {
    const { state } = makeDeferredState()
    state.disabled = true
    state.config = { enabled: false }
    state.openRequested = true
    render(<App state={state} />, container)
    await flush()

    expect(container.querySelector(`.exp-panel`)).toBeNull()
    expect(container.querySelector(`.exp-fab`)).toBeNull()
  })

  it(`open() no-ops after the config resolves disabled`, async () => {
    const { state, resolveConfig } = makeDeferredState()
    render(<App state={state} />, container)
    await flush()

    resolveConfig({ enabled: false })
    await flush()
    expect(container.querySelector(`.exp-fab`)).toBeNull()

    state.bundle?.open()
    await flush()
    expect(container.querySelector(`.exp-panel`)).toBeNull()
  })

  it(`stays usable when the config fetch fails (fail-open)`, async () => {
    const { state, resolveConfig } = makeDeferredState()
    render(<App state={state} />, container)
    await flush()

    // A null config is the fetch-failure signal: the widget must NOT disable.
    resolveConfig(null)
    await flush()
    expect(container.querySelector(`.exp-fab`)).toBeTruthy()

    state.bundle?.open()
    await flush()
    expect(container.querySelector(`.exp-panel`)).toBeTruthy()
  })
})
