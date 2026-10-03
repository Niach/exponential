// EXP-244 form fields: the owner-configured name toggle, the collectEmail
// toggle, and config-defined custom fields whose values merge into the
// submitted customData blob.
import { beforeEach, describe, expect, it, vi } from "vitest"
import { render } from "preact"
import type { WidgetRemoteConfig, WidgetRuntimeState } from "../types"

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

const nameConfig: WidgetRemoteConfig = {
  enabled: true,
  form: {
    buttonLabel: null,
    accentColor: null,
    emailRequired: false,
    collectEmail: true,
    collectName: true,
    nameRequired: false,
  },
}

describe(`EXP-244 form fields`, () => {
  let container: HTMLDivElement

  beforeEach(() => {
    submitFeedback.mockClear()
    if (typeof URL.createObjectURL !== `function`) {
      URL.createObjectURL = () => `blob:test`
      URL.revokeObjectURL = () => undefined
    }
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
    state.bundle?.open()
    await flush()
    return state
  }

  const setInput = async (selector: string, value: string) => {
    const input = container.querySelector<HTMLInputElement | HTMLTextAreaElement>(
      selector
    )
    expect(input).toBeTruthy()
    input!.value = value
    input!.dispatchEvent(new Event(`input`, { bubbles: true }))
    await flush()
  }

  const submitForm = async () => {
    container
      .querySelector(`form`)!
      .dispatchEvent(new Event(`submit`, { bubbles: true, cancelable: true }))
    await flush()
  }

  it(`legacy configs render no name field and keep the email field`, async () => {
    await mount({ enabled: true })
    expect(container.querySelector(`#exp-name`)).toBeNull()
    expect(container.querySelector(`#exp-email`)).toBeTruthy()
  })

  it(`collectName renders the name input and submits the typed name`, async () => {
    await mount(nameConfig)
    expect(container.textContent).toContain(`Name (optional)`)
    await setInput(`#exp-message`, `Broken button`)
    await setInput(`#exp-name`, `dani`)
    await submitForm()
    expect(submitFeedback).toHaveBeenCalledTimes(1)
    expect(submitFeedback.mock.calls[0][0]).toMatchObject({ name: `dani` })
  })

  it(`an identified name hides the field and rides the submission`, async () => {
    await mount(nameConfig, { identity: { name: `Known Reporter` } })
    expect(container.querySelector(`#exp-name`)).toBeNull()
    await setInput(`#exp-message`, `Broken button`)
    await submitForm()
    expect(submitFeedback.mock.calls[0][0]).toMatchObject({
      name: `Known Reporter`,
    })
  })

  it(`nameRequired blocks an empty submit with a form error`, async () => {
    await mount({
      ...nameConfig,
      form: { ...nameConfig.form!, nameRequired: true },
    })
    expect(container.textContent).toContain(`Name`)
    expect(container.textContent).not.toContain(`Name (optional)`)
    await setInput(`#exp-message`, `Broken button`)
    await submitForm()
    expect(submitFeedback).not.toHaveBeenCalled()
    expect(container.textContent).toContain(`Your name is required.`)
  })

  it(`an empty message blocks the submit with a form error`, async () => {
    await mount({ enabled: true })
    await submitForm()
    expect(submitFeedback).not.toHaveBeenCalled()
    expect(container.textContent).toContain(`Tell us what happened.`)
  })

  it(`emailRequired blocks a submit without an email`, async () => {
    await mount({
      enabled: true,
      form: { ...nameConfig.form!, collectName: false, emailRequired: true },
    })
    expect(container.textContent).toContain(`Email`)
    expect(container.textContent).not.toContain(`Email (optional)`)
    await setInput(`#exp-message`, `Broken button`)
    await submitForm()
    expect(submitFeedback).not.toHaveBeenCalled()
    expect(container.textContent).toContain(`Your email is required.`)

    await setInput(`#exp-email`, `reporter@example.com`)
    await submitForm()
    expect(submitFeedback).toHaveBeenCalledTimes(1)
    expect(submitFeedback.mock.calls[0][0]).toMatchObject({
      message: `Broken button`,
      email: `reporter@example.com`,
    })
  })

  it(`collectEmail:false hides the email field`, async () => {
    await mount({
      enabled: true,
      form: { ...nameConfig.form!, collectEmail: false, collectName: false },
    })
    expect(container.querySelector(`#exp-email`)).toBeNull()
    await setInput(`#exp-message`, `Broken button`)
    await submitForm()
    expect(submitFeedback).toHaveBeenCalledTimes(1)
    expect(submitFeedback.mock.calls[0][0]).toMatchObject({ email: null })
  })

  it(`custom fields render and their values merge over host customData`, async () => {
    await mount(
      {
        enabled: true,
        form: {
          ...nameConfig.form!,
          collectName: false,
          customFields: [
            { key: `desk`, label: `Desk number`, required: true },
            { key: `mood`, label: `Mood` },
          ],
        },
      },
      { customData: { plan: `pro`, desk: `host-set` } }
    )
    expect(container.textContent).toContain(`Desk number`)
    expect(container.textContent).toContain(`Mood (optional)`)

    await setInput(`#exp-message`, `Broken button`)
    // Required custom field empty → advisory client gate fires.
    await submitForm()
    expect(submitFeedback).not.toHaveBeenCalled()
    expect(container.textContent).toContain(`Please fill in "Desk number".`)

    await setInput(`#exp-custom-desk`, `42b`)
    await submitForm()
    expect(submitFeedback).toHaveBeenCalledTimes(1)
    // Typed value wins over the host key; untouched optional field leaves
    // the host blob alone.
    expect(submitFeedback.mock.calls[0][0]).toMatchObject({
      customData: { plan: `pro`, desk: `42b` },
    })
  })

  it(`rejects an over-8KB merged customData blob before the network call`, async () => {
    await mount(
      {
        enabled: true,
        form: {
          ...nameConfig.form!,
          collectName: false,
          customFields: [{ key: `notes`, label: `Notes` }],
        },
      },
      { customData: { blob: `x`.repeat(8 * 1024) } }
    )
    await setInput(`#exp-message`, `Broken button`)
    await setInput(`#exp-custom-notes`, `overflow`)
    await submitForm()
    expect(submitFeedback).not.toHaveBeenCalled()
    expect(container.textContent).toContain(
      `Your responses are too long to submit`
    )
  })

  it(`a custom field keyed like a prototype member stays prototype-safe`, async () => {
    // "Constructor" slugifies to the key `constructor`, which the server key
    // pattern accepts — a plain-object lookup resolves it to the INHERITED
    // Object constructor: garbage pre-fill and a `.trim()` throw on submit.
    await mount({
      enabled: true,
      form: {
        ...nameConfig.form!,
        collectName: false,
        customFields: [
          { key: `constructor`, label: `Constructor`, required: true },
        ],
      },
    })
    const input = container.querySelector<HTMLInputElement>(
      `#exp-custom-constructor`
    )!
    expect(input.value).toBe(``)

    await setInput(`#exp-message`, `Broken button`)
    // Empty required value: the advisory gate must fire, not throw.
    await submitForm()
    expect(submitFeedback).not.toHaveBeenCalled()
    expect(container.textContent).toContain(`Please fill in "Constructor".`)

    await setInput(`#exp-custom-constructor`, `MyWidget`)
    await submitForm()
    expect(submitFeedback).toHaveBeenCalledTimes(1)
    expect(submitFeedback.mock.calls[0][0]).toMatchObject({
      customData: { constructor: `MyWidget` },
    })
  })

  it(`drops malformed custom field entries instead of crashing`, async () => {
    await mount({
      enabled: true,
      form: {
        ...nameConfig.form!,
        collectName: false,
        customFields: [
          { key: `ok`, label: `Fine` },
          { key: ``, label: `No key` },
          null,
          { key: `nolabel` },
        ] as never,
      },
    })
    expect(container.querySelector(`#exp-custom-ok`)).toBeTruthy()
    expect(container.querySelectorAll(`[id^="exp-custom-"]`).length).toBe(1)
  })
})
