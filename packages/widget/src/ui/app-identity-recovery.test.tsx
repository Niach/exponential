// When the server rejects the identity email the widget hid (the reporter
// typed none), the panel must re-reveal its email input so the visitor can
// recover — for BOTH the feedback and support forms. A later identify() with a
// corrected address heals automatically, and a bare 400 with no structured
// code (old-server skew) still triggers the reveal. The capture/annotator
// leaves are mocked; the api-client is mocked so each submit outcome is
// scripted.
import { beforeEach, describe, expect, it, vi } from "vitest"
import { render } from "preact"
import type { WidgetRemoteConfig, WidgetRuntimeState } from "../types"
import type { SubmitResult } from "../api-client"

vi.mock(`../capture/engine`, () => ({
  captureScreenshot: vi.fn(async () => null),
}))
vi.mock(`../capture/snapdom-engine`, () => ({ snapdomEngine: {} }))
vi.mock(`./Annotator`, () => ({ Annotator: () => null }))

const submitFeedback = vi.fn<(args: unknown) => Promise<SubmitResult>>()
const submitSupportRequest = vi.fn<(args: unknown) => Promise<SubmitResult>>()
vi.mock(`../api-client`, () => ({
  submitFeedback: (args: unknown) => submitFeedback(args),
  submitSupportRequest: (args: unknown) => submitSupportRequest(args),
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

// An address that passes the loader's cheap clamp but that zod's stricter
// z.email() refuses (the '#' is illegal in the local part).
const badIdentityEmail = `user#tag@example.com`

describe(`identity email recovery`, () => {
  let container: HTMLDivElement

  beforeEach(() => {
    submitFeedback.mockReset()
    submitSupportRequest.mockReset()
    if (typeof URL.createObjectURL !== `function`) {
      URL.createObjectURL = () => `blob:test`
      URL.revokeObjectURL = () => undefined
    }
    document.body.innerHTML = ``
    container = document.createElement(`div`)
    document.body.appendChild(container)
  })

  const typeInto = (selector: string, value: string) => {
    const el = container.querySelector<HTMLInputElement | HTMLTextAreaElement>(
      selector
    )!
    el.value = value
    el.dispatchEvent(new Event(`input`, { bubbles: true }))
  }

  const submitForm = async () => {
    // Let any pending input-driven state updates apply before the submit
    // handler closure reads them.
    await flush()
    container
      .querySelector(`form`)!
      .dispatchEvent(new Event(`submit`, { bubbles: true, cancelable: true }))
    await flush()
  }

  const mountOpen = async (config: WidgetRemoteConfig | null) => {
    const state = makeState(config)
    state.identity = { email: badIdentityEmail }
    render(<App state={state} />, container)
    await flush()
    state.bundle?.open()
    await flush()
    return state
  }

  it(`recovers on the feedback form: reveal, friendly message, resubmit with the typed email`, async () => {
    await mountOpen(null)
    // Identity email present → the email input starts hidden.
    expect(container.querySelector(`#exp-email`)).toBeNull()

    submitFeedback.mockResolvedValueOnce({
      ok: false,
      message: `Invalid submission fields`,
      status: 400,
      code: `invalid_email`,
    })
    typeInto(`#exp-title`, `Broken button`)
    await submitForm()

    // Friendly, code-mapped copy AND the email input revealed.
    expect(container.textContent).toContain(
      `Please enter a valid email address.`
    )
    expect(container.querySelector(`#exp-email`)).toBeTruthy()

    submitFeedback.mockResolvedValueOnce({
      ok: true,
      identifier: `EXP-1`,
      url: null,
    })
    typeInto(`#exp-email`, `real@example.com`)
    await submitForm()

    expect(container.textContent).toContain(`Thanks for the report!`)
    expect(submitFeedback).toHaveBeenCalledTimes(2)
    expect(
      (submitFeedback.mock.calls[1][0] as { email: string | null }).email
    ).toBe(`real@example.com`)
  })

  it(`recovers on the support form: reveal, resubmit with the typed email`, async () => {
    await mountOpen({ enabled: true, modes: [`support`] })
    // Support-only opens the support form directly; the email input is hidden.
    expect(container.querySelector(`#exp-message`)).toBeTruthy()
    expect(container.querySelector(`#exp-support-email`)).toBeNull()

    submitSupportRequest.mockResolvedValueOnce({
      ok: false,
      message: `Invalid submission fields`,
      status: 400,
      code: `invalid_email`,
    })
    typeInto(`#exp-message`, `Please help me`)
    await submitForm()

    expect(container.textContent).toContain(
      `Please enter a valid email address.`
    )
    expect(container.querySelector(`#exp-support-email`)).toBeTruthy()

    submitSupportRequest.mockResolvedValueOnce({
      ok: true,
      identifier: null,
      url: null,
    })
    typeInto(`#exp-support-email`, `real@example.com`)
    await submitForm()

    expect(container.textContent).toContain(`We got your request!`)
    expect(
      (submitSupportRequest.mock.calls[1][0] as { email: string }).email
    ).toBe(`real@example.com`)
  })

  it(`heals when the host supplies a corrected identify email`, async () => {
    const state = await mountOpen(null)

    submitFeedback.mockResolvedValueOnce({
      ok: false,
      message: `Invalid submission fields`,
      status: 400,
      code: `invalid_email`,
    })
    typeInto(`#exp-title`, `Broken`)
    await submitForm()
    expect(container.querySelector(`#exp-email`)).toBeTruthy()

    // A different address from the host restores the hidden-input behavior.
    state.identity = { email: `fixed@example.com` }
    state.bundle?.stateChanged()
    await flush()
    expect(container.querySelector(`#exp-email`)).toBeNull()
  })

  it(`reveals the input on a bare 400 with no code (old-server skew)`, async () => {
    await mountOpen(null)

    submitFeedback.mockResolvedValueOnce({
      ok: false,
      message: `Invalid submission fields`,
      status: 400,
      code: null,
    })
    typeInto(`#exp-title`, `Broken`)
    await submitForm()

    expect(container.querySelector(`#exp-email`)).toBeTruthy()
    // No code to map, so the server's own message is surfaced verbatim.
    expect(container.textContent).toContain(`Invalid submission fields`)
  })

  it(`keeps the identity email through a non-email 400 (oversized meta)`, async () => {
    await mountOpen(null)

    // A code-less 400 whose message is NOT the email-failure copy must not
    // blame the address: the input stays hidden and the identity email keeps
    // riding along on the retry.
    submitFeedback.mockResolvedValueOnce({
      ok: false,
      message: `Invalid meta`,
      status: 400,
      code: null,
    })
    typeInto(`#exp-title`, `Broken`)
    await submitForm()

    expect(container.querySelector(`#exp-email`)).toBeNull()
    expect(container.textContent).toContain(`Invalid meta`)

    submitFeedback.mockResolvedValueOnce({
      ok: true,
      identifier: `EXP-2`,
      url: null,
    })
    await submitForm()
    expect(
      (submitFeedback.mock.calls[1][0] as { email: string | null }).email
    ).toBe(badIdentityEmail)
  })
})
