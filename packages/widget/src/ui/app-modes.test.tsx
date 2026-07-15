// Widget modes (EXP-130): both modes → card home screen; picking "Get help"
// shows the support form and submits mode=support through the api client; a
// single mode skips the home screen entirely (feedback-only configs — and
// null configs from older servers — behave exactly like before, covered by
// app-open.test.tsx).
import { beforeEach, describe, expect, it, vi } from "vitest"
import { render } from "preact"
import type { WidgetRemoteConfig, WidgetRuntimeState } from "../types"

vi.mock(`../capture/engine`, () => ({
  captureScreenshot: vi.fn(async () => null),
}))
vi.mock(`../capture/snapdom-engine`, () => ({ snapdomEngine: {} }))
vi.mock(`./Annotator`, () => ({ Annotator: () => null }))

const submitSupportRequest = vi.fn(
  async (_args: { message: string; email: string }) =>
    ({ ok: true, identifier: null, url: null }) as const
)
vi.mock(`../api-client`, () => ({
  submitFeedback: vi.fn(),
  submitSupportRequest: (args: { message: string; email: string }) =>
    submitSupportRequest(args),
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

const bothModes: WidgetRemoteConfig = {
  enabled: true,
  modes: [`feedback`, `support`],
}

describe(`widget modes`, () => {
  let container: HTMLDivElement

  beforeEach(() => {
    submitSupportRequest.mockClear()
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

  const clickByText = async (selector: string, text: string) => {
    const target = [...container.querySelectorAll<HTMLElement>(selector)].find(
      (el) => el.textContent?.includes(text)
    )
    expect(target).toBeTruthy()
    target!.click()
    await flush()
  }

  it(`both modes open onto the card home`, async () => {
    await mount(bothModes)
    expect(container.textContent).toContain(`Hi there 👋`)
    expect(container.textContent).toContain(`How can we help?`)
    expect(container.textContent).toContain(`Give feedback`)
    expect(container.textContent).toContain(`Get help`)
    // Neither form is rendered yet.
    expect(container.querySelector(`#exp-title`)).toBeNull()
    expect(container.querySelector(`#exp-message`)).toBeNull()
  })

  it(`picking Give feedback shows the feedback form, with a way back`, async () => {
    await mount(bothModes)
    await clickByText(`.exp-mode-card`, `Give feedback`)
    expect(container.querySelector(`#exp-title`)).toBeTruthy()
    const back = container.querySelector<HTMLButtonElement>(`.exp-back`)
    expect(back).toBeTruthy()
    back!.click()
    await flush()
    expect(container.textContent).toContain(`Get help`)
    expect(container.querySelector(`#exp-title`)).toBeNull()
  })

  it(`Get help submits a support request with message + email`, async () => {
    await mount(bothModes)
    await clickByText(`.exp-mode-card`, `Get help`)

    const message =
      container.querySelector<HTMLTextAreaElement>(`#exp-message`)!
    message.value = `My login is broken`
    message.dispatchEvent(new Event(`input`, { bubbles: true }))
    const email =
      container.querySelector<HTMLInputElement>(`#exp-support-email`)!
    email.value = `reporter@example.com`
    email.dispatchEvent(new Event(`input`, { bubbles: true }))
    await flush()

    container
      .querySelector(`form`)!
      .dispatchEvent(new Event(`submit`, { bubbles: true, cancelable: true }))
    await flush()

    expect(submitSupportRequest).toHaveBeenCalledTimes(1)
    expect(submitSupportRequest.mock.calls[0][0]).toMatchObject({
      message: `My login is broken`,
      email: `reporter@example.com`,
    })
    expect(container.textContent).toContain(`We got your request!`)
  })

  it(`support-only configs open the support form directly`, async () => {
    await mount({ enabled: true, modes: [`support`] })
    expect(container.querySelector(`#exp-message`)).toBeTruthy()
    // No home screen and no way "back" to one.
    expect(container.querySelector(`.exp-back`)).toBeNull()
    expect(container.textContent).not.toContain(`Give feedback`)
  })

  it(`refuses to submit support without an email`, async () => {
    await mount({ enabled: true, modes: [`support`] })
    const message =
      container.querySelector<HTMLTextAreaElement>(`#exp-message`)!
    message.value = `Help`
    message.dispatchEvent(new Event(`input`, { bubbles: true }))
    await flush()
    container
      .querySelector(`form`)!
      .dispatchEvent(new Event(`submit`, { bubbles: true, cancelable: true }))
    await flush()
    expect(submitSupportRequest).not.toHaveBeenCalled()
    expect(container.textContent).toContain(
      `Your email is required so we can reply.`
    )
  })

  it(`an identified reporter skips the email field and submits with it`, async () => {
    const state = makeState(bothModes)
    state.identity = { email: `known@example.com` }
    render(<App state={state} />, container)
    await flush()
    state.bundle?.open()
    await flush()
    await clickByText(`.exp-mode-card`, `Get help`)

    expect(container.querySelector(`#exp-support-email`)).toBeNull()
    const message =
      container.querySelector<HTMLTextAreaElement>(`#exp-message`)!
    message.value = `Help me`
    message.dispatchEvent(new Event(`input`, { bubbles: true }))
    await flush()
    container
      .querySelector(`form`)!
      .dispatchEvent(new Event(`submit`, { bubbles: true, cancelable: true }))
    await flush()
    expect(submitSupportRequest.mock.calls[0][0]).toMatchObject({
      email: `known@example.com`,
    })
  })
})
