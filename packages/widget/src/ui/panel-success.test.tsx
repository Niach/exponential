// Success card (EXP-42a): the "Filed as EXP-n" line links to the public
// issue when the server sent a url, and stays plain text when it did not
// (current servers always send null; older self-hosted ones may link).
import { beforeEach, describe, expect, it } from "vitest"
import { render } from "preact"
import { Panel } from "./Panel"

const noop = () => undefined

const renderSuccess = (args: {
  identifier: string | null
  url: string | null
  flavor?: `feedback` | `support`
  emailDelivered?: boolean | null
}) => {
  const container = document.createElement(`div`)
  document.body.appendChild(container)
  render(
    <Panel
      phase="success"
      view="feedback"
      canGoBack={false}
      onPickMode={noop}
      onBack={noop}
      successFlavor={args.flavor ?? `feedback`}
      successIdentifier={args.identifier}
      successUrl={args.url}
      successEmailDelivered={args.emailDelivered ?? null}
      position="bottom-right"
      screenshot={null}
      flattening={false}
      captureFailed={false}
      uploads={[]}
      uploadError={null}
      identityEmail={null}
      emailRequired={false}
      collectEmail={true}
      identityName={null}
      collectName={false}
      nameRequired={false}
      customFields={[]}
      labels={[]}
      onClose={noop}
      onCapture={noop}
      captureDelay={0}
      onCycleCaptureDelay={noop}
      onRetake={noop}
      onAnnotate={noop}
      onRemoveScreenshot={noop}
      onAddImages={noop}
      onRemoveUpload={noop}
      onSubmit={async () => null}
      onSubmitSupport={async () => null}
    />,
    container
  )
  return container
}

describe(`success card`, () => {
  beforeEach(() => {
    document.body.innerHTML = ``
  })

  it(`links the identifier to the public issue when a url is present`, () => {
    const url = `https://app.exponential.test/t/feedback/projects/exponential/issues/EXP-7`
    const container = renderSuccess({ identifier: `EXP-7`, url })
    const link = container.querySelector<HTMLAnchorElement>(`a.exp-success-link`)
    expect(link).toBeTruthy()
    expect(link?.getAttribute(`href`)).toBe(url)
    expect(link?.getAttribute(`target`)).toBe(`_blank`)
    expect(link?.getAttribute(`rel`)).toBe(`noopener noreferrer`)
    expect(link?.textContent).toBe(`EXP-7`)
    expect(container.textContent).toContain(`Filed as EXP-7.`)
  })

  it(`renders plain text when the url is null`, () => {
    const container = renderSuccess({ identifier: `EXP-7`, url: null })
    // The powered-by footer's anchor is always present — only the
    // issue-link anchor must be absent.
    expect(container.querySelector(`a.exp-success-link`)).toBeNull()
    expect(container.textContent).toContain(`Filed as EXP-7.`)
  })

  it(`falls back to the generic line without an identifier`, () => {
    const container = renderSuccess({ identifier: null, url: null })
    // The powered-by footer's anchor is always present — only the
    // issue-link anchor must be absent.
    expect(container.querySelector(`a.exp-success-link`)).toBeNull()
    expect(container.textContent).toContain(`Your feedback has been sent.`)
  })
})

// REV2-10: the magic link is the reporter's ONLY way back into the
// conversation. When the confirmation email didn't go out, saying "check your
// email" is a lie — and the link must still never be shown inline.
describe(`support success card email honesty`, () => {
  beforeEach(() => {
    document.body.innerHTML = ``
  })

  it(`promises the email when delivery succeeded`, () => {
    const container = renderSuccess({
      identifier: null,
      url: null,
      flavor: `support`,
      emailDelivered: true,
    })
    expect(container.textContent).toContain(`Check your email`)
  })

  it(`keeps the optimistic copy when the server doesn't report delivery`, () => {
    const container = renderSuccess({
      identifier: null,
      url: null,
      flavor: `support`,
      emailDelivered: null,
    })
    expect(container.textContent).toContain(`Check your email`)
  })

  it(`says so honestly — and shows no link — when the email failed`, () => {
    const container = renderSuccess({
      identifier: null,
      url: null,
      flavor: `support`,
      emailDelivered: false,
    })
    expect(container.textContent).toContain(
      `We couldn't send the confirmation email`
    )
    expect(container.textContent).toContain(`will follow up`)
    expect(container.textContent).not.toContain(`Check your email`)
    expect(container.querySelector(`a.exp-success-link`)).toBeNull()
    expect(container.textContent).not.toContain(`/support/`)
  })
})
