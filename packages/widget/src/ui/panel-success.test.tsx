// Success card (EXP-42a / SLOP-4): "Thanks, your report is in." plus the
// "Filed as EXP-n" line (linked only when the server sent a url — current
// servers always send null; older self-hosted ones may link) and an honest
// email sentence driven by `emailDelivered`.
import { beforeEach, describe, expect, it } from "vitest"
import { render } from "preact"
import { Panel } from "./Panel"

const noop = () => undefined

const renderSuccess = (args: {
  identifier: string | null
  url: string | null
  emailDelivered?: boolean | null
}) => {
  const container = document.createElement(`div`)
  document.body.appendChild(container)
  render(
    <Panel
      phase="success"
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
    />,
    container
  )
  return container
}

describe(`success card`, () => {
  beforeEach(() => {
    document.body.innerHTML = ``
  })

  it(`always leads with the thanks line`, () => {
    const container = renderSuccess({ identifier: null, url: null })
    expect(container.textContent).toContain(`Thanks, your report is in.`)
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

  it(`shows no sub line without an identifier or an email verdict`, () => {
    const container = renderSuccess({ identifier: null, url: null })
    expect(container.querySelector(`a.exp-success-link`)).toBeNull()
    expect(container.querySelector(`.exp-success-sub`)).toBeNull()
    expect(container.textContent).not.toContain(`Filed as`)
  })
})

// REV2-10 / SLOP-4: the emailed link is the reporter's ONLY way back into
// the conversation. The card promises the email only when the server reports
// it went out, says so honestly when it failed, and says nothing about email
// when the reporter left none — and the link is never shown inline.
describe(`success card email honesty`, () => {
  beforeEach(() => {
    document.body.innerHTML = ``
  })

  const sent = `We emailed you a link to follow the conversation.`
  const notSent = `We could not send the follow-up email; your report still reached the team.`

  it(`promises the email when delivery succeeded`, () => {
    const container = renderSuccess({
      identifier: null,
      url: null,
      emailDelivered: true,
    })
    expect(container.textContent).toContain(sent)
    expect(container.textContent).not.toContain(notSent)
  })

  it(`says nothing about email when none was given (or reported)`, () => {
    const container = renderSuccess({
      identifier: `EXP-7`,
      url: null,
      emailDelivered: null,
    })
    expect(container.textContent).toContain(`Filed as EXP-7.`)
    expect(container.textContent).not.toContain(`email`)
  })

  it(`shows the identifier AND the email promise together`, () => {
    const container = renderSuccess({
      identifier: `EXP-7`,
      url: null,
      emailDelivered: true,
    })
    expect(container.textContent).toContain(`Filed as EXP-7.`)
    expect(container.textContent).toContain(sent)
  })

  it(`says so honestly — and shows no link — when the email failed`, () => {
    const container = renderSuccess({
      identifier: `EXP-7`,
      url: null,
      emailDelivered: false,
    })
    expect(container.textContent).toContain(notSent)
    expect(container.textContent).toContain(`Filed as EXP-7.`)
    expect(container.textContent).not.toContain(sent)
    expect(container.querySelector(`a.exp-success-link`)).toBeNull()
    expect(container.textContent).not.toContain(`/support/`)
  })
})
