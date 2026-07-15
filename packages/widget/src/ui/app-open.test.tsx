// Panel-open + on-demand screenshot flow: opening shows the plain form (no
// automatic capture); the "Take screenshot" button captures and drops into
// the annotation editor; cancelling out of that fresh capture discards it,
// while cancelling a re-edit (Annotate chip) keeps the attached shot. The
// heavy leaf components (capture engine, canvas editor) are mocked — this
// exercises App's phase machine only.
import { beforeEach, describe, expect, it, vi } from "vitest"
import { render } from "preact"
import type { WidgetRuntimeState } from "../types"

const captureScreenshot = vi.fn<() => Promise<Blob | null>>()

vi.mock(`../capture/engine`, () => ({
  captureScreenshot: () => captureScreenshot(),
}))
vi.mock(`../capture/snapdom-engine`, () => ({ snapdomEngine: {} }))

let annotatorProps: {
  onCancel(): void
  onSave(next: unknown[], nextCrop: null): void
} | null = null
vi.mock(`./Annotator`, () => ({
  Annotator: (props: NonNullable<typeof annotatorProps>) => {
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
  // Preact effects + the capture promise chain settle across macrotasks (the
  // capture path also waits on a requestAnimationFrame, shimmed to a timeout
  // in jsdom).
  for (let i = 0; i < 6; i += 1) {
    await new Promise((resolve) => setTimeout(resolve, 0))
  }
}

describe(`panel open → on-demand screenshot flow`, () => {
  let container: HTMLDivElement
  let state: WidgetRuntimeState

  beforeEach(async () => {
    annotatorProps = null
    captureScreenshot.mockReset()
    if (typeof URL.createObjectURL !== `function`) {
      URL.createObjectURL = () => `blob:test`
      URL.revokeObjectURL = () => undefined
    }
    if (typeof globalThis.requestAnimationFrame !== `function`) {
      globalThis.requestAnimationFrame = (fn: FrameRequestCallback) => {
        setTimeout(() => fn(0), 0)
        return 0
      }
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

  const clickTakeScreenshot = async () => {
    const button = [
      ...container.querySelectorAll<HTMLButtonElement>(`.exp-chip`),
    ].find((chip) => chip.textContent === `Take screenshot`)
    expect(button).toBeTruthy()
    button!.click()
    await flush()
  }

  it(`opens onto the plain form without capturing`, async () => {
    await openPanel()
    expect(captureScreenshot).not.toHaveBeenCalled()
    expect(container.querySelector(`[data-testid="annotator"]`)).toBeNull()
    expect(container.querySelector(`.exp-panel`)).toBeTruthy()
    expect(container.textContent).toContain(`Take screenshot`)
  })

  it(`Take screenshot captures and opens the annotator`, async () => {
    captureScreenshot.mockResolvedValue(new Blob([`x`], { type: `image/png` }))
    await openPanel()
    await clickTakeScreenshot()
    expect(captureScreenshot).toHaveBeenCalledTimes(1)
    expect(container.querySelector(`[data-testid="annotator"]`)).toBeTruthy()
    // The form stays mounted underneath so typed fields survive.
    const panel = container.querySelector<HTMLElement>(`.exp-panel`)
    expect(panel).toBeTruthy()
    expect(panel?.style.display).toBe(`none`)
  })

  it(`cancelling a fresh capture discards the screenshot`, async () => {
    captureScreenshot.mockResolvedValue(new Blob([`x`], { type: `image/png` }))
    await openPanel()
    await clickTakeScreenshot()
    annotatorProps?.onCancel()
    await flush()
    expect(container.querySelector(`[data-testid="annotator"]`)).toBeNull()
    // Nothing attached: the empty-state button is back, no <img> preview.
    expect(container.querySelector(`.exp-shot img`)).toBeNull()
    expect(container.textContent).toContain(`Take screenshot`)
  })

  it(`cancelling a re-edit keeps the attached screenshot`, async () => {
    captureScreenshot.mockResolvedValue(new Blob([`x`], { type: `image/png` }))
    await openPanel()
    await clickTakeScreenshot()
    // Save attaches the shot; the Annotate chip re-enters the editor.
    annotatorProps?.onSave([], null)
    await flush()
    expect(container.querySelector(`.exp-shot img`)).toBeTruthy()
    const annotateChip = [
      ...container.querySelectorAll<HTMLButtonElement>(`.exp-chip`),
    ].find((chip) => chip.textContent === `Annotate`)
    annotateChip!.click()
    await flush()
    annotatorProps?.onCancel()
    await flush()
    expect(container.querySelector(`.exp-shot img`)).toBeTruthy()
  })

  it(`stays on the form when the capture failed`, async () => {
    captureScreenshot.mockResolvedValue(null)
    await openPanel()
    await clickTakeScreenshot()
    expect(container.querySelector(`[data-testid="annotator"]`)).toBeNull()
    expect(container.querySelector(`.exp-panel`)).toBeTruthy()
    expect(container.textContent).toContain(`Screenshot couldn't be captured.`)
  })
})
