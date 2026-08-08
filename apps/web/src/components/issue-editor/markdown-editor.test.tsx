import { describe, expect, it } from "vitest"
import { render, waitFor } from "@testing-library/react"
import { MarkdownEditor } from "@/components/issue-editor/markdown-editor"

// EXP-440: the agent-session feed renders every bubble through this editor, so
// the chat knobs (compact appearance, linkify, hard breaks, aria label) are a
// contract — and they are read ONCE at editor creation, which is exactly the
// kind of wiring a type check cannot catch.

const mountedContent = async (container: HTMLElement) => {
  await waitFor(() => {
    expect(container.querySelector(`.tiptap-content`)).toBeTruthy()
  })
}

describe(`MarkdownEditor chat appearance`, () => {
  it(`renders images, headings, markdown links and bare URLs`, async () => {
    const { container } = render(
      <MarkdownEditor
        markdown={`## Heading\n\n![shot](/api/attachments/abc)\n\nSee https://example.dev/x and [docs](https://example.dev/d).`}
        editable={false}
        onChange={() => {}}
        appearance="chat"
        linkify
        ariaLabel="Agent message"
      />
    )
    await mountedContent(container)
    await waitFor(() => {
      expect(container.querySelector(`img`)).toBeTruthy()
    })
    expect(container.querySelector(`.tiptap-chat`)).toBeTruthy()
    expect(container.querySelector(`[aria-label="Agent message"]`)).toBeTruthy()
    expect(container.querySelector(`h2`)).toBeTruthy()
    expect(
      container.querySelectorAll(`a[href="https://example.dev/x"]`).length
    ).toBe(1)
    expect(
      container.querySelectorAll(`a[href="https://example.dev/d"]`).length
    ).toBe(1)
  })

  it(`turns a single newline into a hard break with hardBreaks`, async () => {
    const { container } = render(
      <MarkdownEditor
        markdown={`line one\nline two`}
        editable={false}
        onChange={() => {}}
        appearance="chat"
        hardBreaks
      />
    )
    await mountedContent(container)
    await waitFor(() => {
      expect(container.querySelector(`br`)).toBeTruthy()
    })
  })

  it(`leaves the document defaults untouched`, async () => {
    const { container } = render(
      <MarkdownEditor
        markdown={`bare https://example.dev/x here\nsecond line`}
        editable={false}
        onChange={() => {}}
      />
    )
    await mountedContent(container)
    expect(container.querySelector(`.tiptap-chat`)).toBeFalsy()
    expect(
      container.querySelector(`[aria-label="Issue description"]`)
    ).toBeTruthy()
    expect(container.querySelector(`a`)).toBeFalsy()
    expect(container.querySelector(`br`)).toBeFalsy()
  })
})
