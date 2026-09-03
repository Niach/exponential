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

// EXP-568: an editable instance mounts the floating selection rail (a TipTap
// BubbleMenu portaled to <body>). Its plugin registers against the live
// editor, so a wiring mistake here breaks every editable editor at mount.
describe(`MarkdownEditor formatting rail`, () => {
  it(`mounts an editable editor with the selection rail attached`, async () => {
    const { container, unmount } = render(
      <MarkdownEditor markdown={`hello`} onChange={() => {}} />
    )
    await mountedContent(container)
    // The rail's element stays DETACHED until a selection shows it (the
    // BubbleMenu plugin appends it to <body> on show), so the assertion here
    // is simply that registering the plugin against a live editor works and
    // the editable content came up.
    expect(
      container.querySelector(`.tiptap-content[contenteditable="true"]`)
    ).toBeTruthy()
    expect(document.querySelector(`[data-editor-rail]`)).toBeNull()
    unmount()
  })
})

// EXP-726: a GFM table must RENDER as a grid on every read-only surface
// (comment bodies, the agent feed, the description preview) — before this the
// `<table>` was dropped and the cells flattened into paragraphs.
describe(`MarkdownEditor tables`, () => {
  it(`renders a read-only table with header cells and alignment`, async () => {
    const { container } = render(
      <MarkdownEditor
        markdown={`| l | c |\n| :--- | :---: |\n| 1 | 2 |`}
        editable={false}
        onChange={() => {}}
      />
    )
    await mountedContent(container)
    await waitFor(() => {
      expect(container.querySelector(`table`)).toBeTruthy()
    })
    expect(container.querySelectorAll(`table th`).length).toBe(2)
    expect(container.querySelectorAll(`table td`).length).toBe(2)
    expect(
      container.querySelector(`table th[style*="text-align"]`)
    ).toBeTruthy()
    expect(
      container.querySelector(`table td[style*="text-align: center"]`)
    ).toBeTruthy()
    // The scroll container the wide-table rule hangs off.
    expect(container.querySelector(`.tableWrapper`)).toBeTruthy()
  })
})
