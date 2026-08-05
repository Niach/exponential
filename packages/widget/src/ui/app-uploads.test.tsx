// FEED-5 reporter-attached pictures: the feedback form accepts images via
// the hidden file input, drag-and-drop, and paste — validated client-side
// (type / 10 MB / count cap), previewed as removable thumbnails, submitted
// as `images`, and discarded on close.
import { beforeEach, describe, expect, it, vi } from "vitest"
import { render } from "preact"
import type { WidgetRuntimeState } from "../types"
import { maxUploadedImages } from "../uploads"

vi.mock(`../capture/engine`, () => ({
  captureScreenshot: vi.fn(async () => null),
}))
vi.mock(`../capture/snapdom-engine`, () => ({ snapdomEngine: {} }))
vi.mock(`./Annotator`, () => ({ Annotator: () => null }))

const submitFeedback = vi.fn(
  async (_args: Record<string, unknown>) =>
    ({ ok: true, identifier: `EXP-1`, url: null }) as const
)
vi.mock(`../api-client`, () => ({
  submitFeedback: (args: Record<string, unknown>) => submitFeedback(args),
  submitSupportRequest: vi.fn(),
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
  for (let i = 0; i < 6; i += 1) {
    await new Promise((resolve) => setTimeout(resolve, 0))
  }
}

const makeImage = (name: string, type = `image/png`, size = 100): File => {
  const file = new File([new Uint8Array(8)], name, { type })
  // Blob.size is a prototype getter — shadow it so oversized files don't
  // need real 10 MB buffers.
  Object.defineProperty(file, `size`, { value: size })
  return file
}

const dropFiles = (target: Element, files: File[]) => {
  const event = new Event(`drop`, { bubbles: true, cancelable: true })
  Object.assign(event, { dataTransfer: { types: [`Files`], files } })
  target.dispatchEvent(event)
}

describe(`FEED-5 attached pictures`, () => {
  let container: HTMLDivElement

  beforeEach(() => {
    submitFeedback.mockClear()
    URL.createObjectURL = vi.fn(() => `blob:test-${Math.random()}`)
    URL.revokeObjectURL = vi.fn()
    document.body.innerHTML = ``
    container = document.createElement(`div`)
    document.body.appendChild(container)
  })

  const mount = async () => {
    const state = makeState()
    render(<App state={state} />, container)
    await flush()
    state.bundle?.open()
    await flush()
    return state
  }

  const thumbs = () => container.querySelectorAll(`.exp-thumb`)
  const form = () => container.querySelector(`form`)!

  const setTitle = async (value: string) => {
    const input = container.querySelector<HTMLInputElement>(`#exp-title`)!
    input.value = value
    input.dispatchEvent(new Event(`input`, { bubbles: true }))
    await flush()
  }

  it(`attaches picked files as thumbnails and submits them as images`, async () => {
    await mount()
    const input =
      container.querySelector<HTMLInputElement>(`input[type="file"]`)!
    const picture = makeImage(`reference.png`)
    Object.defineProperty(input, `files`, {
      value: [picture],
      configurable: true,
    })
    input.dispatchEvent(new Event(`change`, { bubbles: true }))
    await flush()
    expect(thumbs().length).toBe(1)

    await setTitle(`Broken button`)
    form().dispatchEvent(
      new Event(`submit`, { bubbles: true, cancelable: true })
    )
    await flush()
    expect(submitFeedback).toHaveBeenCalledTimes(1)
    const sent = submitFeedback.mock.calls[0][0] as {
      images: { blob: Blob; filename: string }[]
    }
    expect(sent.images).toHaveLength(1)
    expect(sent.images[0].filename).toBe(`reference.png`)
    expect(sent.images[0].blob).toBe(picture)
  })

  it(`attaches dropped picture files`, async () => {
    await mount()
    dropFiles(form(), [makeImage(`a.png`), makeImage(`b.jpg`, `image/jpeg`)])
    await flush()
    expect(thumbs().length).toBe(2)
  })

  it(`attaches pasted images`, async () => {
    await mount()
    const event = new Event(`paste`, { bubbles: true, cancelable: true })
    Object.assign(event, { clipboardData: { files: [makeImage(`paste.png`)] } })
    form().dispatchEvent(event)
    await flush()
    expect(thumbs().length).toBe(1)
  })

  it(`rejects non-image files with an error`, async () => {
    await mount()
    dropFiles(form(), [makeImage(`notes.pdf`, `application/pdf`)])
    await flush()
    expect(thumbs().length).toBe(0)
    expect(container.textContent).toContain(`Only image files can be attached.`)
  })

  it(`rejects images over 10 MB but keeps valid siblings`, async () => {
    await mount()
    dropFiles(form(), [
      makeImage(`huge.png`, `image/png`, 11 * 1024 * 1024),
      makeImage(`fine.png`),
    ])
    await flush()
    expect(thumbs().length).toBe(1)
    expect(container.textContent).toContain(`Images must be 10 MB or smaller.`)
  })

  it(`caps attachments at ${maxUploadedImages} and hides the add button`, async () => {
    await mount()
    dropFiles(
      form(),
      Array.from({ length: maxUploadedImages + 1 }, (_, index) =>
        makeImage(`pic-${index}.png`)
      )
    )
    await flush()
    expect(thumbs().length).toBe(maxUploadedImages)
    expect(container.textContent).toContain(
      `You can attach up to ${maxUploadedImages} images.`
    )
    expect(container.querySelector(`.exp-add-image`)).toBeNull()
  })

  it(`removes a picture via its remove button`, async () => {
    await mount()
    dropFiles(form(), [makeImage(`a.png`), makeImage(`b.png`)])
    await flush()
    expect(thumbs().length).toBe(2)
    container
      .querySelector<HTMLButtonElement>(
        `.exp-thumb-remove[aria-label="Remove a.png"]`
      )!
      .click()
    await flush()
    expect(thumbs().length).toBe(1)
    expect(URL.revokeObjectURL).toHaveBeenCalledTimes(1)
  })

  it(`discards attached pictures on close`, async () => {
    const state = await mount()
    dropFiles(form(), [makeImage(`a.png`)])
    await flush()
    expect(thumbs().length).toBe(1)
    state.bundle?.close()
    await flush()
    state.bundle?.open()
    await flush()
    expect(thumbs().length).toBe(0)
  })
})
