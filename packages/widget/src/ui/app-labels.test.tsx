// EXP-435 reporter labels: config-served labels render as toggle chips on
// the feedback form and the selected ids ride the submission.
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
  submitSupportRequest: vi.fn(),
}))

import { App } from "./App"

const makeState = (config: WidgetRemoteConfig | null): WidgetRuntimeState => ({
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
})

const flush = async () => {
  for (let i = 0; i < 6; i += 1) {
    await new Promise((resolve) => setTimeout(resolve, 0))
  }
}

const labelConfig: WidgetRemoteConfig = {
  enabled: true,
  form: {
    buttonLabel: null,
    accentColor: null,
    position: `bottom-right`,
    emailRequired: false,
    labels: [
      { id: `l-bug`, name: `Bug`, color: `#ef4444` },
      { id: `l-idea`, name: `Idea`, color: `#22c55e` },
    ],
  },
}

describe(`EXP-435 reporter labels`, () => {
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

  const mount = async (config: WidgetRemoteConfig | null) => {
    const state = makeState(config)
    render(<App state={state} />, container)
    await flush()
    state.bundle?.open()
    await flush()
    return state
  }

  const chips = () =>
    [...container.querySelectorAll<HTMLButtonElement>(`.exp-label-chip`)]

  const chip = (name: string) =>
    chips().find((el) => el.textContent?.includes(name))!

  const submitForm = async () => {
    const title = container.querySelector<HTMLInputElement>(`#exp-title`)!
    title.value = `Broken button`
    title.dispatchEvent(new Event(`input`, { bubbles: true }))
    await flush()
    container
      .querySelector(`form`)!
      .dispatchEvent(new Event(`submit`, { bubbles: true, cancelable: true }))
    await flush()
  }

  it(`renders a chip per served label with its color dot`, async () => {
    await mount(labelConfig)
    expect(chips()).toHaveLength(2)
    expect(container.textContent).toContain(`What is this about?`)
    const dot = chip(`Bug`).querySelector<HTMLElement>(`.exp-label-dot`)!
    expect(dot.style.background).toBeTruthy()
  })

  it(`renders no label field without served labels`, async () => {
    await mount({ enabled: true })
    expect(chips()).toHaveLength(0)
    expect(container.textContent).not.toContain(`What is this about?`)
  })

  it(`toggling selects and deselects via aria-pressed`, async () => {
    await mount(labelConfig)
    const bug = chip(`Bug`)
    expect(bug.getAttribute(`aria-pressed`)).toBe(`false`)
    bug.click()
    await flush()
    expect(chip(`Bug`).getAttribute(`aria-pressed`)).toBe(`true`)
    chip(`Bug`).click()
    await flush()
    expect(chip(`Bug`).getAttribute(`aria-pressed`)).toBe(`false`)
  })

  it(`submits the selected label ids`, async () => {
    await mount(labelConfig)
    chip(`Bug`).click()
    await flush()
    chip(`Idea`).click()
    await flush()
    await submitForm()
    expect(submitFeedback).toHaveBeenCalledTimes(1)
    expect(submitFeedback.mock.calls[0][0]).toMatchObject({
      labelIds: [`l-bug`, `l-idea`],
    })
  })

  it(`submits an empty selection untouched`, async () => {
    await mount(labelConfig)
    await submitForm()
    expect(submitFeedback.mock.calls[0][0]).toMatchObject({ labelIds: [] })
  })

  it(`drops malformed label entries and falls back on junk colors`, async () => {
    await mount({
      enabled: true,
      form: {
        ...labelConfig.form!,
        labels: [
          { id: `ok`, name: `Fine`, color: `not-a-color` },
          { id: ``, name: `No id`, color: `#ffffff` },
          null,
          { id: `nameless`, name: ``, color: `#ffffff` },
        ] as never,
      },
    })
    expect(chips()).toHaveLength(1)
    expect(chips()[0].textContent).toContain(`Fine`)
  })
})
