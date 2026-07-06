// Panel-open flow: a successful automatic capture drops the reporter
// straight into the annotation editor, exactly once per open; a failed
// capture goes to the plain form. The heavy leaf components (capture engine,
// canvas editor) are mocked — this exercises App's phase machine only.
import { beforeEach, describe, expect, it, vi } from "vitest"
import { render } from "preact"
import type { WidgetRuntimeState } from "../types"

const captureScreenshot = vi.fn<() => Promise<Blob | null>>()

vi.mock(`../capture/engine`, () => ({
  captureScreenshot: () => captureScreenshot(),
}))
vi.mock(`../capture/snapdom-engine`, () => ({ snapdomEngine: {} }))

let annotatorProps: { onCancel(): void } | null = null
vi.mock(`./Annotator`, () => ({
  Annotator: (props: { onCancel(): void }) => {
    annotatorProps = props
    return <div data-testid={`annotator`} />
  },
}))

import { App } from "./App"

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
  // Preact effects + the capture promise chain settle across macrotasks.
  for (let i = 0; i < 4; i += 1) {
    await new Promise((resolve) => setTimeout(resolve, 0))
  }
}

describe(`panel open → annotate flow`, () => {
  let container: HTMLDivElement
  let state: WidgetRuntimeState

  beforeEach(async () => {
    annotatorProps = null
    captureScreenshot.mockReset()
    if (typeof URL.createObjectURL !== `function`) {
      URL.createObjectURL = () => `blob:test`
      URL.revokeObjectURL = () => undefined
    }
    document.body.innerHTML = ``
    container = document.createElement(`div`)
    document.body.appendChild(container)
    state = makeState()
    render(<App state={state} />, container)
    await flush()
  })

  const openPanel = async () => {
    state.bundle?.open()
    await flush()
  }

  it(`auto-opens the annotator when the capture succeeded`, async () => {
    captureScreenshot.mockResolvedValue(new Blob([`x`], { type: `image/png` }))
    await openPanel()
    expect(container.querySelector(`[data-testid="annotator"]`)).toBeTruthy()
    // The form stays mounted underneath so typed fields survive.
    const panel = container.querySelector<HTMLElement>(`.exp-panel`)
    expect(panel).toBeTruthy()
    expect(panel?.style.display).toBe(`none`)
  })

  it(`closing the annotator returns to the form and does not re-open it`, async () => {
    captureScreenshot.mockResolvedValue(new Blob([`x`], { type: `image/png` }))
    await openPanel()
    annotatorProps?.onCancel()
    await flush()
    expect(container.querySelector(`[data-testid="annotator"]`)).toBeNull()
    const panel = container.querySelector<HTMLElement>(`.exp-panel`)
    expect(panel?.style.display).not.toBe(`none`)
  })

  it(`goes straight to the form when the capture failed`, async () => {
    captureScreenshot.mockResolvedValue(null)
    await openPanel()
    expect(container.querySelector(`[data-testid="annotator"]`)).toBeNull()
    expect(container.querySelector(`.exp-panel`)).toBeTruthy()
    expect(container.textContent).toContain(`Screenshot couldn't be captured.`)
  })
})
