// Success card (EXP-42a): the "Filed as EXP-n" line links to the public
// issue when the server sent a url, and stays plain text when it did not
// (older servers / non-public projects).
import { beforeEach, describe, expect, it } from "vitest"
import { render } from "preact"
import { Panel } from "./Panel"

const noop = () => undefined

const renderSuccess = (args: {
  identifier: string | null
  url: string | null
}) => {
  const container = document.createElement(`div`)
  document.body.appendChild(container)
  render(
    <Panel
      phase="success"
      successIdentifier={args.identifier}
      successUrl={args.url}
      position="bottom-right"
      screenshot={null}
      flattening={false}
      captureFailed={false}
      identityEmail={null}
      emailRequired={false}
      onClose={noop}
      onCapture={noop}
      onRetake={noop}
      onAnnotate={noop}
      onRemoveScreenshot={noop}
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
