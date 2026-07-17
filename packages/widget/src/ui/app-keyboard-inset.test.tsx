// Mobile keyboard handling (EXP-171): on-screen keyboards shrink only the
// visual viewport, so the fixed-position panel must be lifted above the
// obscured strip. While the panel is mounted, App mirrors the visual
// viewport into --exp-vv-height/--exp-vv-inset on the root element; the
// panel CSS positions against them. Pinch-zoom (scale ≠ 1) must NOT be
// treated as a keyboard, and closing the panel clears the vars.
import { beforeEach, describe, expect, it } from "vitest"
import { render } from "preact"
import type { WidgetRuntimeState } from "../types"
import { App } from "./App"

class FakeVisualViewport extends EventTarget {
  height = 800
  offsetTop = 0
  scale = 1
}

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

const flush = async () => {
  for (let i = 0; i < 6; i += 1) {
    await new Promise((resolve) => setTimeout(resolve, 0))
  }
}

describe(`visual-viewport keyboard inset`, () => {
  let container: HTMLDivElement
  let state: WidgetRuntimeState
  let viewport: FakeVisualViewport

  beforeEach(async () => {
    viewport = new FakeVisualViewport()
    Object.defineProperty(window, `visualViewport`, {
      configurable: true,
      value: viewport,
    })
    Object.defineProperty(window, `innerHeight`, {
      configurable: true,
      value: 800,
    })
    document.body.innerHTML = ``
    container = document.createElement(`div`)
    document.body.appendChild(container)
    state = makeState()
    render(<App state={state} />, container)
    await flush()
  })

  const root = () => container.querySelector<HTMLElement>(`.exp-root`)!

  it(`tracks the keyboard inset while the panel is open`, async () => {
    state.bundle?.open()
    await flush()
    // No keyboard yet: the vars reflect the full viewport.
    expect(root().style.getPropertyValue(`--exp-vv-height`)).toBe(`800px`)
    expect(root().style.getPropertyValue(`--exp-vv-inset`)).toBe(`0px`)

    // Keyboard opens: the visual viewport shrinks by 360px.
    viewport.height = 440
    viewport.dispatchEvent(new Event(`resize`))
    expect(root().style.getPropertyValue(`--exp-vv-height`)).toBe(`440px`)
    expect(root().style.getPropertyValue(`--exp-vv-inset`)).toBe(`360px`)
  })

  it(`accounts for the visual viewport's own offset`, async () => {
    state.bundle?.open()
    await flush()
    // The browser scrolled the visual viewport down within the layout
    // viewport (keyboard + scroll-into-view): only the strip BELOW the
    // visual viewport is obscured at the bottom.
    viewport.height = 440
    viewport.offsetTop = 100
    viewport.dispatchEvent(new Event(`scroll`))
    expect(root().style.getPropertyValue(`--exp-vv-inset`)).toBe(`260px`)
  })

  it(`ignores pinch-zoom viewports`, async () => {
    state.bundle?.open()
    await flush()
    viewport.height = 400
    viewport.scale = 2
    viewport.dispatchEvent(new Event(`resize`))
    expect(root().style.getPropertyValue(`--exp-vv-height`)).toBe(``)
    expect(root().style.getPropertyValue(`--exp-vv-inset`)).toBe(``)
  })

  it(`clears the vars when the panel closes`, async () => {
    state.bundle?.open()
    await flush()
    viewport.height = 440
    viewport.dispatchEvent(new Event(`resize`))
    expect(root().style.getPropertyValue(`--exp-vv-inset`)).toBe(`360px`)
    state.bundle?.close()
    await flush()
    expect(root().style.getPropertyValue(`--exp-vv-height`)).toBe(``)
    expect(root().style.getPropertyValue(`--exp-vv-inset`)).toBe(``)
  })
})
