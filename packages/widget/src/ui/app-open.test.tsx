// Panel-open + on-demand screenshot flow: opening shows the plain form (no
// automatic capture); the "Take screenshot" button captures and drops into
// the annotation editor; cancelling out of that fresh capture discards it,
// while cancelling a re-edit (Annotate chip) keeps the attached shot. The
// heavy leaf components (capture engine, canvas editor) are mocked — this
// exercises App's phase machine only.
import { beforeEach, describe, expect, it, vi } from "vitest"
import { render } from "preact"
import type { WidgetRuntimeState } from "../types"

const captureScreenshot = vi.fn<(engine: unknown) => Promise<Blob | null>>()

vi.mock(`../capture/engine`, () => ({
  captureScreenshot: (engine: unknown) => captureScreenshot(engine),
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
// The mocked snapDOM engine ({}) and the REAL display-media engine — App picks
// between them by identity, so these are the exact tokens it passes along.
import { snapdomEngine } from "../capture/snapdom-engine"
import { displayMediaEngine } from "../capture/display-media-engine"

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

  // One button (EXP-488): getDisplayMedia support decides the engine, a
  // failed/cancelled display capture falls back to the DOM raster.
  const stubDisplayCapture = () => {
    Object.defineProperty(navigator, `mediaDevices`, {
      configurable: true,
      value: { getDisplayMedia: () => Promise.reject(new Error(`unused`)) },
    })
    return () => {
      delete (navigator as { mediaDevices?: unknown }).mediaDevices
    }
  }

  it(`captures with snapDOM where display capture is unsupported`, async () => {
    captureScreenshot.mockResolvedValue(new Blob([`x`], { type: `image/png` }))
    await openPanel()
    await clickTakeScreenshot()
    expect(captureScreenshot).toHaveBeenCalledTimes(1)
    expect(captureScreenshot).toHaveBeenCalledWith(snapdomEngine)
  })

  it(`prefers native display capture where the browser has it`, async () => {
    const restore = stubDisplayCapture()
    try {
      captureScreenshot.mockResolvedValue(
        new Blob([`x`], { type: `image/png` })
      )
      await openPanel()
      await clickTakeScreenshot()
      expect(captureScreenshot).toHaveBeenCalledTimes(1)
      expect(captureScreenshot).toHaveBeenCalledWith(displayMediaEngine)
      expect(container.querySelector(`[data-testid="annotator"]`)).toBeTruthy()
    } finally {
      restore()
    }
  })

  it(`falls back to snapDOM when display capture yields nothing`, async () => {
    const restore = stubDisplayCapture()
    try {
      captureScreenshot
        .mockResolvedValueOnce(null)
        .mockResolvedValueOnce(new Blob([`x`], { type: `image/png` }))
      await openPanel()
      await clickTakeScreenshot()
      expect(captureScreenshot).toHaveBeenCalledTimes(2)
      expect(captureScreenshot).toHaveBeenNthCalledWith(1, displayMediaEngine)
      expect(captureScreenshot).toHaveBeenNthCalledWith(2, snapdomEngine)
      expect(container.querySelector(`[data-testid="annotator"]`)).toBeTruthy()
    } finally {
      restore()
    }
  })
})
