// Panel-open + on-demand screenshot flow: opening shows the plain form (no
// automatic capture); the "Take screenshot" button captures and drops into
// the annotation editor; cancelling out of that fresh capture discards it,
// while cancelling a re-edit (Annotate chip) keeps the attached shot. The
// heavy leaf components (capture engine, canvas editor) are mocked — this
// exercises App's phase machine only.
import { beforeEach, describe, expect, it, vi } from "vitest"
import { render } from "preact"
import type { WidgetRuntimeState } from "../types"

interface CaptureOptions {
  delayMs?: number
  onCountdown?(secondsLeft: number): void
}
const captureScreenshot =
  vi.fn<(engine: unknown, options: CaptureOptions) => Promise<Blob | null>>()

vi.mock(`../capture/engine`, () => ({
  captureScreenshot: (engine: unknown, options: CaptureOptions) =>
    captureScreenshot(engine, options),
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
    expect(captureScreenshot).toHaveBeenCalledWith(
      snapdomEngine,
      expect.anything()
    )
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
      expect(captureScreenshot).toHaveBeenCalledWith(
        displayMediaEngine,
        expect.anything()
      )
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
      expect(captureScreenshot).toHaveBeenNthCalledWith(
        1,
        displayMediaEngine,
        expect.anything()
      )
      expect(captureScreenshot).toHaveBeenNthCalledWith(
        2,
        snapdomEngine,
        expect.anything()
      )
      expect(container.querySelector(`[data-testid="annotator"]`)).toBeTruthy()
    } finally {
      restore()
    }
  })

  // Delayed capture (FEED-18): the segment attached to Take screenshot
  // cycles the hold; the hold rides into every capture (fallback and Retake
  // included) and the countdown pill shows the seconds the engine reports.
  const delayChip = () =>
    container.querySelector<HTMLButtonElement>(`.exp-chip-delay`)

  const clickDelayChip = async () => {
    delayChip()!.click()
    await flush()
  }

  it(`cycles the capture delay Off → 3s → 5s → Off`, async () => {
    await openPanel()
    expect(delayChip()?.textContent).toBe(`Off`)
    expect(delayChip()?.getAttribute(`aria-pressed`)).toBe(`false`)
    await clickDelayChip()
    expect(delayChip()?.textContent).toBe(`3s`)
    expect(delayChip()?.getAttribute(`aria-pressed`)).toBe(`true`)
    await clickDelayChip()
    expect(delayChip()?.textContent).toBe(`5s`)
    await clickDelayChip()
    expect(delayChip()?.textContent).toBe(`Off`)
    // Cycling never captures by itself.
    expect(captureScreenshot).not.toHaveBeenCalled()
  })

  it(`captures with the chosen delay, on the fallback and on Retake too`, async () => {
    const restore = stubDisplayCapture()
    try {
      captureScreenshot
        .mockResolvedValueOnce(null)
        .mockResolvedValue(new Blob([`x`], { type: `image/png` }))
      await openPanel()
      await clickDelayChip()
      await clickDelayChip()
      await clickTakeScreenshot()
      expect(captureScreenshot).toHaveBeenNthCalledWith(
        1,
        displayMediaEngine,
        expect.objectContaining({ delayMs: 5_000 })
      )
      expect(captureScreenshot).toHaveBeenNthCalledWith(
        2,
        snapdomEngine,
        expect.objectContaining({ delayMs: 5_000 })
      )
      annotatorProps?.onSave([], null)
      await flush()
      const retake = [
        ...container.querySelectorAll<HTMLButtonElement>(`.exp-chip`),
      ].find((chip) => chip.textContent === `Retake`)
      retake!.click()
      await flush()
      expect(captureScreenshot).toHaveBeenCalledTimes(3)
      expect(captureScreenshot).toHaveBeenNthCalledWith(
        3,
        snapdomEngine,
        expect.objectContaining({ delayMs: 5_000 })
      )
    } finally {
      restore()
    }
  })

  it(`captures immediately while the delay is Off`, async () => {
    captureScreenshot.mockResolvedValue(new Blob([`x`], { type: `image/png` }))
    await openPanel()
    await clickTakeScreenshot()
    expect(captureScreenshot).toHaveBeenCalledWith(
      snapdomEngine,
      expect.objectContaining({ delayMs: 0 })
    )
  })

  it(`shows the countdown pill while the hold runs and keeps the form mounted`, async () => {
    let pending: {
      options: CaptureOptions
      resolve(blob: Blob | null): void
    } | null = null
    captureScreenshot.mockImplementation(
      (_engine, options) =>
        new Promise<Blob | null>((resolve) => {
          pending = { options, resolve }
        })
    )
    await openPanel()
    await clickDelayChip()
    await clickTakeScreenshot()
    expect(pending).not.toBeNull()
    // The panel is hidden, not unmounted: the typed fields must survive the
    // multi-second hold. The launcher is gone (it would be in a display
    // frame).
    const panel = container.querySelector<HTMLElement>(`.exp-panel`)
    expect(panel?.style.display).toBe(`none`)
    expect(container.querySelector(`.exp-countdown`)).toBeNull()

    pending!.options.onCountdown?.(3)
    await flush()
    expect(container.querySelector(`.exp-countdown`)?.textContent).toBe(`3`)
    pending!.options.onCountdown?.(1)
    await flush()
    expect(container.querySelector(`.exp-countdown`)?.textContent).toBe(`1`)
    // The 0 tick takes the pill away BEFORE the engine grabs the frame.
    pending!.options.onCountdown?.(0)
    await flush()
    expect(container.querySelector(`.exp-countdown`)).toBeNull()

    pending!.resolve(new Blob([`x`], { type: `image/png` }))
    await flush()
    expect(container.querySelector(`[data-testid="annotator"]`)).toBeTruthy()
    expect(container.querySelector(`.exp-countdown`)).toBeNull()
  })

  // A hold is multi-second: the host can call close() (or the config can
  // resolve the widget disabled) while it runs. The capture that lands
  // afterwards must not reopen the panel, drop the visitor into the
  // annotator, or leave its shot waiting for the next open.
  const pendingCapture = () => {
    let pending: { resolve(blob: Blob | null): void } | null = null
    captureScreenshot.mockImplementation(
      () =>
        new Promise<Blob | null>((resolve) => {
          pending = { resolve }
        })
    )
    return () => pending
  }

  it(`drops a capture that lands after the panel closed`, async () => {
    const held = pendingCapture()
    await openPanel()
    await clickDelayChip()
    await clickTakeScreenshot()
    expect(held()).not.toBeNull()
    state.bundle?.close()
    await flush()

    held()!.resolve(new Blob([`x`], { type: `image/png` }))
    await flush()
    expect(container.querySelector(`[data-testid="annotator"]`)).toBeNull()
    expect(container.querySelector(`.exp-panel`)).toBeNull()

    await openPanel()
    expect(container.querySelector(`.exp-shot img`)).toBeNull()
    expect(container.textContent).toContain(`Take screenshot`)
  })

  it(`drops a Retake that lands after the panel closed`, async () => {
    captureScreenshot.mockResolvedValue(new Blob([`x`], { type: `image/png` }))
    await openPanel()
    await clickTakeScreenshot()
    annotatorProps?.onSave([], null)
    await flush()
    expect(container.querySelector(`.exp-shot img`)).toBeTruthy()

    const held = pendingCapture()
    const retake = [
      ...container.querySelectorAll<HTMLButtonElement>(`.exp-chip`),
    ].find((chip) => chip.textContent === `Retake`)
    retake!.click()
    await flush()
    state.bundle?.close()
    await flush()

    held()!.resolve(new Blob([`y`], { type: `image/png` }))
    await flush()
    expect(container.querySelector(`.exp-panel`)).toBeNull()

    await openPanel()
    expect(container.querySelector(`.exp-shot img`)).toBeNull()
  })

  // The display-media hold runs after the grant, so a frame that fails
  // afterwards (zero-size, or "Stop sharing" mid-hold) has already cost the
  // reporter the countdown — the snapDOM fallback must not charge it twice.
  it(`skips the fallback hold when the granted capture already held`, async () => {
    const restore = stubDisplayCapture()
    try {
      captureScreenshot
        .mockImplementationOnce((_engine, options) => {
          options.onCountdown?.(3)
          options.onCountdown?.(0)
          return Promise.resolve(null)
        })
        .mockResolvedValue(new Blob([`x`], { type: `image/png` }))
      await openPanel()
      await clickDelayChip()
      await clickTakeScreenshot()
      expect(captureScreenshot).toHaveBeenNthCalledWith(
        1,
        displayMediaEngine,
        expect.objectContaining({ delayMs: 3_000 })
      )
      expect(captureScreenshot).toHaveBeenNthCalledWith(
        2,
        snapdomEngine,
        expect.objectContaining({ delayMs: 0 })
      )
      expect(container.querySelector(`[data-testid="annotator"]`)).toBeTruthy()
      expect(container.querySelector(`.exp-countdown`)).toBeNull()
    } finally {
      restore()
    }
  })

  // The other half of that rule: a dismissed picker never reached the hold,
  // so the fallback still runs it — exactly once.
  it(`still holds once when the picker was dismissed`, async () => {
    const restore = stubDisplayCapture()
    try {
      captureScreenshot
        .mockResolvedValueOnce(null)
        .mockResolvedValue(new Blob([`x`], { type: `image/png` }))
      await openPanel()
      await clickDelayChip()
      await clickTakeScreenshot()
      expect(captureScreenshot).toHaveBeenCalledTimes(2)
      expect(captureScreenshot).toHaveBeenNthCalledWith(
        2,
        snapdomEngine,
        expect.objectContaining({ delayMs: 3_000 })
      )
    } finally {
      restore()
    }
  })
})
