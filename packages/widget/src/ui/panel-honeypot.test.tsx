// Honeypot (REV2-69): /api/widget/submit has always dropped submissions that
// carry a non-empty `website`, but no form ever rendered the field — so the
// DOM-walking bots the trick targets had nothing to fall for. Both forms must
// render it, keep it invisible/unfocusable for real reporters, and forward
// whatever a bot typed.
import { beforeEach, describe, expect, it } from "vitest"
import { render } from "preact"
import { Panel } from "./Panel"

const noop = () => undefined

type Submitted = Record<string, unknown> | null

const renderForm = (view: `feedback` | `support`) => {
  const container = document.createElement(`div`)
  document.body.appendChild(container)
  const captured: { feedback: Submitted; support: Submitted } = {
    feedback: null,
    support: null,
  }
  render(
    <Panel
      phase="open"
      view={view}
      canGoBack={false}
      onPickMode={noop}
      onBack={noop}
      successFlavor="feedback"
      successIdentifier={null}
      successUrl={null}
      successEmailDelivered={null}
      position="bottom-right"
      screenshot={null}
      flattening={false}
      captureFailed={false}
      identityEmail="reporter@example.com"
      emailRequired={false}
      collectEmail={true}
      identityName={null}
      collectName={false}
      nameRequired={false}
      customFields={[]}
      onClose={noop}
      onCapture={noop}
      onRetake={noop}
      onAnnotate={noop}
      onRemoveScreenshot={noop}
      onSubmit={async (form) => {
        captured.feedback = form
        return null
      }}
      onSubmitSupport={async (form) => {
        captured.support = form
        return null
      }}
    />,
    container
  )
  return { container, captured }
}

const flush = async () => {
  for (let i = 0; i < 6; i += 1) {
    await new Promise((resolve) => setTimeout(resolve, 0))
  }
}

const honeypotOf = (container: HTMLElement) =>
  container.querySelector<HTMLInputElement>(`input[name="website"]`)

describe(`honeypot field`, () => {
  beforeEach(() => {
    document.body.innerHTML = ``
  })

  for (const view of [`feedback`, `support`] as const) {
    it(`renders an off-screen, unfocusable website input on the ${view} form`, () => {
      const { container } = renderForm(view)
      const input = honeypotOf(container)
      expect(input).toBeTruthy()
      expect(input?.tabIndex).toBe(-1)
      expect(input?.getAttribute(`aria-hidden`)).toBe(`true`)
      // Browsers autofill off-screen url/website profile fields and largely
      // ignore autocomplete="off" (REV-107) — an autofilled honeypot would
      // silently swallow a real report.
      expect(input?.getAttribute(`autocomplete`)).toBe(`one-time-code`)
      // Off-screen, not display:none — bots skip undisplayed fields.
      expect(input?.style.position).toBe(`absolute`)
      expect(input?.style.display).not.toBe(`none`)
    })
  }

  it(`forwards the typed honeypot value with a feedback submission`, async () => {
    const { container, captured } = renderForm(`feedback`)
    const input = honeypotOf(container)!
    input.value = `http://spam.example`
    input.dispatchEvent(new Event(`input`, { bubbles: true }))
    const titleInput = container.querySelector<HTMLInputElement>(`#exp-title`)!
    titleInput.value = `Broken button`
    titleInput.dispatchEvent(new Event(`input`, { bubbles: true }))
    await flush()
    container
      .querySelector(`form`)!
      .dispatchEvent(new Event(`submit`, { bubbles: true, cancelable: true }))
    await flush()
    expect(captured.feedback).toMatchObject({
      title: `Broken button`,
      website: `http://spam.example`,
    })
  })

  it(`forwards the typed honeypot value with a support submission`, async () => {
    const { container, captured } = renderForm(`support`)
    const input = honeypotOf(container)!
    input.value = `http://spam.example`
    input.dispatchEvent(new Event(`input`, { bubbles: true }))
    const message = container.querySelector<HTMLTextAreaElement>(`#exp-message`)!
    message.value = `Login is broken`
    message.dispatchEvent(new Event(`input`, { bubbles: true }))
    await flush()
    container
      .querySelector(`form`)!
      .dispatchEvent(new Event(`submit`, { bubbles: true, cancelable: true }))
    await flush()
    expect(captured.support).toMatchObject({
      message: `Login is broken`,
      website: `http://spam.example`,
    })
  })

  it(`submits an empty honeypot for an untouched form`, async () => {
    const { container, captured } = renderForm(`support`)
    const message = container.querySelector<HTMLTextAreaElement>(`#exp-message`)!
    message.value = `Login is broken`
    message.dispatchEvent(new Event(`input`, { bubbles: true }))
    await flush()
    container
      .querySelector(`form`)!
      .dispatchEvent(new Event(`submit`, { bubbles: true, cancelable: true }))
    await flush()
    expect(captured.support).toMatchObject({ website: `` })
  })
})
