// FEED-113 locale in the mounted App (init option, auto, live setLocale) and
// NEW-19 (FEED-117/143): a rejected submit leaves everything the reporter
// entered in the form so they can resend after the wait.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"
import { render } from "preact"
import type { WidgetRemoteConfig, WidgetRuntimeState } from "../types"
import type { SubmitResult } from "../api-client"

const captureScreenshot = vi.fn<() => Promise<Blob | null>>()
vi.mock(`../capture/engine`, () => ({
  captureScreenshot: () => captureScreenshot(),
}))
vi.mock(`../capture/snapdom-engine`, () => ({ snapdomEngine: {} }))

let annotatorProps: {
  onSave(next: unknown[], nextCrop: null): void
} | null = null
vi.mock(`./Annotator`, () => ({
  Annotator: (props: NonNullable<typeof annotatorProps>) => {
    annotatorProps = props
    return null
  },
}))

const submitFeedback = vi.fn<(args: unknown) => Promise<SubmitResult>>()
vi.mock(`../api-client`, () => ({
  submitFeedback: (args: unknown) => submitFeedback(args),
}))

import { App } from "./App"

const fullConfig: WidgetRemoteConfig = {
  enabled: true,
  form: {
    buttonLabel: null,
    accentColor: null,
    emailRequired: false,
    collectEmail: true,
    collectName: true,
    nameRequired: false,
    customFields: [{ key: `company`, label: `Firma` }],
    labels: [{ id: `label-bug`, name: `Bug`, color: `#ff0000` }],
  },
}

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

let container: HTMLDivElement

beforeEach(() => {
  annotatorProps = null
  captureScreenshot.mockReset()
  submitFeedback.mockReset()
  URL.createObjectURL = vi.fn(() => `blob:test-${Math.random()}`)
  URL.revokeObjectURL = vi.fn()
  if (typeof globalThis.requestAnimationFrame !== `function`) {
    globalThis.requestAnimationFrame = (fn: FrameRequestCallback) => {
      setTimeout(() => fn(0), 0)
      return 0
    }
  }
  document.body.innerHTML = ``
  container = document.createElement(`div`)
  document.body.appendChild(container)
})

afterEach(() => {
  document.documentElement.lang = ``
})

const mount = async (
  config: WidgetRemoteConfig | null,
  overrides?: Partial<WidgetRuntimeState>,
  open = true
) => {
  const state = makeState(config, overrides)
  render(<App state={state} />, container)
  await flush()
  if (open) {
    state.bundle?.open()
    await flush()
  }
  return state
}

const setInput = async (selector: string, value: string) => {
  const input = container.querySelector<
    HTMLInputElement | HTMLTextAreaElement
  >(selector)!
  input.value = value
  input.dispatchEvent(new Event(`input`, { bubbles: true }))
  await flush()
}

const submitForm = async () => {
  container
    .querySelector(`form`)!
    .dispatchEvent(new Event(`submit`, { bubbles: true, cancelable: true }))
  await flush()
}

describe(`FEED-113 locale`, () => {
  it(`renders the panel in German from the init option`, async () => {
    await mount(fullConfig, { options: { key: `expw_test`, locale: `de` } })
    const text = container.textContent ?? ``
    expect(text).toContain(`Feedback senden`)
    expect(text).toContain(`Was ist passiert?`)
    expect(text).toContain(`Screenshot aufnehmen`)
    expect(text).toContain(`Aus`)
    expect(text).toContain(`Bild hinzufügen`)
    expect(text).toContain(`Bereitgestellt von`)
    expect(
      container.querySelector(`#exp-message`)!.getAttribute(`placeholder`)
    ).toBe(`Beschreiben Sie, was Sie gesehen haben oder was Sie sich wünschen.`)
    // Owner-configured labels stay as given.
    expect(text).toContain(`Firma (optional)`)
    expect(
      container
        .querySelector(`button[aria-haspopup="dialog"]`)!
        .getAttribute(`aria-label`)
    ).toBe(`Feedback senden`)
  })

  it(`auto follows <html lang>`, async () => {
    document.documentElement.lang = `de`
    await mount(null)
    expect(container.textContent).toContain(`Was ist passiert?`)
  })

  it(`German validation errors`, async () => {
    await mount(null, { options: { key: `expw_test`, locale: `de` } })
    await submitForm()
    expect(container.querySelector(`.exp-error`)!.textContent).toBe(
      `Beschreiben Sie bitte, was passiert ist.`
    )
  })

  it(`setLocale (localeOverride + stateChanged) switches live`, async () => {
    const state = await mount(null, {
      options: { key: `expw_test`, locale: `en` },
    })
    expect(container.textContent).toContain(`What happened?`)
    state.localeOverride = `de`
    state.bundle?.stateChanged()
    await flush()
    expect(container.textContent).toContain(`Was ist passiert?`)
    expect(container.textContent).not.toContain(`What happened?`)
  })

  it(`a host label still wins over the localized launcher default`, async () => {
    await mount(
      null,
      { options: { key: `expw_test`, locale: `de`, label: `Hilfe` } },
      false
    )
    expect(container.querySelector(`.exp-fab-label`)?.textContent).toBe(
      `Hilfe`
    )
  })

  it(`renders no launcher when the config marked the widget misconfigured`, async () => {
    await mount(
      null,
      {
        disabled: true,
        configFailure: { code: `origin_not_allowed`, status: 403 },
      },
      false
    )
    expect(container.querySelector(`button[aria-haspopup="dialog"]`)).toBeNull()
  })
})

describe(`NEW-19 a rejected submit keeps the form`, () => {
  it(`keeps message, identity, labels, fields, screenshot and images after a 429`, async () => {
    const shot = new Blob([`png`], { type: `image/png` })
    captureScreenshot.mockResolvedValue(shot)
    await mount(fullConfig)

    // Screenshot: take, then confirm in the (mocked) annotator.
    ;[...container.querySelectorAll<HTMLButtonElement>(`.exp-chip`)]
      .find((chip) => chip.textContent === `Take screenshot`)!
      .click()
    await flush()
    annotatorProps!.onSave([], null)
    await flush()
    expect(container.querySelector(`.exp-shot img`)).not.toBeNull()

    // Attached picture.
    const fileInput =
      container.querySelector<HTMLInputElement>(`input[type="file"]`)!
    const picture = new File([new Uint8Array(8)], `ref.png`, {
      type: `image/png`,
    })
    Object.defineProperty(fileInput, `files`, {
      value: [picture],
      configurable: true,
    })
    fileInput.dispatchEvent(new Event(`change`, { bubbles: true }))
    await flush()

    await setInput(`#exp-message`, `The export button does nothing`)
    await setInput(`#exp-name`, `Dani`)
    await setInput(`#exp-email`, `dani@example.com`)
    await setInput(`#exp-custom-company`, `matflow`)
    container.querySelector<HTMLButtonElement>(`.exp-label-chip`)!.click()
    await flush()

    submitFeedback.mockResolvedValueOnce({
      ok: false,
      message: `Too many reports right now. Try again in 8 minutes.`,
      status: 429,
      code: `rate_limited`,
    })
    await submitForm()

    expect(container.querySelector(`.exp-error`)!.textContent).toBe(
      `Too many reports right now. Try again in 8 minutes.`
    )
    const value = (selector: string) =>
      container.querySelector<HTMLInputElement>(selector)!.value
    expect(value(`#exp-message`)).toBe(`The export button does nothing`)
    expect(value(`#exp-name`)).toBe(`Dani`)
    expect(value(`#exp-email`)).toBe(`dani@example.com`)
    expect(value(`#exp-custom-company`)).toBe(`matflow`)
    expect(
      container
        .querySelector(`.exp-label-chip`)!
        .getAttribute(`aria-pressed`)
    ).toBe(`true`)
    expect(container.querySelector(`.exp-shot img`)).not.toBeNull()
    expect(container.querySelectorAll(`.exp-thumb`)).toHaveLength(1)

    // Resend after the wait: the same payload goes out again.
    submitFeedback.mockResolvedValueOnce({
      ok: true,
      identifier: `EXP-9`,
      url: null,
    })
    await submitForm()
    expect(submitFeedback).toHaveBeenCalledTimes(2)
    const [first, second] = submitFeedback.mock.calls.map(
      (call) =>
        call[0] as {
          message: string
          email: string
          name: string
          customData: Record<string, unknown>
          labelIds: string[]
          screenshot: Blob | null
          images: { blob: Blob }[]
        }
    )
    for (const sent of [first, second]) {
      expect(sent).toMatchObject({
        message: `The export button does nothing`,
        email: `dani@example.com`,
        name: `Dani`,
        customData: { company: `matflow` },
        labelIds: [`label-bug`],
      })
      expect(sent.screenshot).toBe(shot)
      expect(sent.images.map((image) => image.blob)).toEqual([picture])
    }
  })
})
