// Submit vs annotation-flatten race: saving crop/annotations kicks off an
// async re-encode, and submitting during that window must send the flattened
// result — never the pristine base screenshot, which may contain content the
// reporter deliberately cropped away. The Send button is also disabled while
// the encode runs.
import { beforeEach, describe, expect, it, vi } from "vitest"
import { render } from "preact"
import type { WidgetRuntimeState } from "../types"
import type { AnnotationShape, NormalizedRect } from "../annotate/shapes"

const captureScreenshot = vi.fn<() => Promise<Blob | null>>()

vi.mock(`../capture/engine`, () => ({
  captureScreenshot: () => captureScreenshot(),
}))
vi.mock(`../capture/snapdom-engine`, () => ({ snapdomEngine: {} }))

const flattenAnnotations = vi.fn<() => Promise<Blob | null>>()
vi.mock(`../annotate/flatten`, () => ({
  flattenAnnotations: () => flattenAnnotations(),
}))

const submitFeedback = vi.fn(
  async (_args: { screenshot: Blob | null }) =>
    ({ ok: true, identifier: `EXP-1`, url: null }) as const
)
vi.mock(`../api-client`, () => ({
  submitFeedback: (args: { screenshot: Blob | null }) => submitFeedback(args),
}))

let annotatorProps: {
  onCancel(): void
  onSave(next: AnnotationShape[], nextCrop: NormalizedRect | null): void
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
  for (let i = 0; i < 4; i += 1) {
    await new Promise((resolve) => setTimeout(resolve, 0))
  }
}

const baseBlob = new Blob([`base`], { type: `image/png` })
const annotatedBlob = new Blob([`annotated`], { type: `image/webp` })
const cropRect: NormalizedRect = { x: 0, y: 0, width: 10, height: 10 }

describe(`submit while a flatten is encoding`, () => {
  let container: HTMLDivElement
  let state: WidgetRuntimeState

  beforeEach(async () => {
    annotatorProps = null
    captureScreenshot.mockReset()
    captureScreenshot.mockResolvedValue(baseBlob)
    flattenAnnotations.mockReset()
    submitFeedback.mockClear()
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
    // Auto-capture drops into the annotator; the form is mounted underneath.
    state.bundle?.open()
    await flush()
    const title = container.querySelector<HTMLInputElement>(`#exp-title`)
    title!.value = `Broken thing`
    title!.dispatchEvent(new Event(`input`, { bubbles: true }))
    await flush()
  })

  it(`awaits the pending flatten and sends its result, not the base`, async () => {
    let resolveFlatten: (blob: Blob | null) => void = () => undefined
    flattenAnnotations.mockReturnValue(
      new Promise<Blob | null>((resolve) => {
        resolveFlatten = resolve
      })
    )

    annotatorProps!.onSave([], cropRect)
    // Same task as onSave: the re-render disabling the button has not
    // happened yet, so this reproduces the click-through race exactly.
    const form = container.querySelector(`form`)!
    form.dispatchEvent(new Event(`submit`, { bubbles: true, cancelable: true }))
    await flush()
    expect(submitFeedback).not.toHaveBeenCalled()

    resolveFlatten(annotatedBlob)
    await flush()
    expect(submitFeedback).toHaveBeenCalledTimes(1)
    expect(submitFeedback.mock.calls[0][0].screenshot).toBe(annotatedBlob)
  })

  it(`disables the Send button until the flatten settles`, async () => {
    let resolveFlatten: (blob: Blob | null) => void = () => undefined
    flattenAnnotations.mockReturnValue(
      new Promise<Blob | null>((resolve) => {
        resolveFlatten = resolve
      })
    )

    annotatorProps!.onSave([], cropRect)
    await flush()
    const button = container.querySelector<HTMLButtonElement>(`.exp-submit`)!
    expect(button.disabled).toBe(true)
    expect(button.textContent).toBe(`Preparing screenshot…`)
    // The blocked window submits nothing.
    button.form?.dispatchEvent(
      new Event(`submit`, { bubbles: true, cancelable: true })
    )
    await new Promise((resolve) => setTimeout(resolve, 0))
    expect(submitFeedback).not.toHaveBeenCalled()

    resolveFlatten(annotatedBlob)
    await flush()
    expect(button.disabled).toBe(false)
    expect(button.textContent).toBe(`Send feedback`)
  })
})
